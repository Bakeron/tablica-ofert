//! The same offer from the same portal, added by hand once more ("Add offer"): identical, changed
//! (a newer date, other salary, renewed after it expired), almost the same text under a new number
//! of the portal, or a second ad while the first is still open. Plain code, no AI - nothing is
//! merged by itself: the form shows what was found and the person decides.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::clock;
use crate::dedup;
use crate::error::{AppError, AppResult};
use crate::model::*;
use crate::offers;
use crate::sources::alerts;

/// From this much of the same description on (in %), a new number of the portal may be the same
/// offer renewed - the app asks (`zmiany.md`, the author: "95%").
pub const SAME_TEXT: u8 = 95;

/// Words of a description as compared: no numbers (salaries and dates are compared as fields, and
/// counters like "views: 123" change all the time) and no "1 hour ago", "expires in 2 days",
/// "yesterday" - parts that change by themselves.
fn words(text: &str) -> Vec<String> {
    let normalized = dedup::normalize(text);
    let all: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
    let number = |w: &str| w.bytes().all(|b| b.is_ascii_digit());
    let mut out = Vec::with_capacity(all.len());
    let mut i = 0;
    while i < all.len() {
        let w = all[i];
        if number(w) {
            // "2 dni", "1 godz" - the unit goes with its number.
            i += if all.get(i + 1).is_some_and(|u| crate::dates::unit(u).is_some()) { 2 } else { 1 };
            continue;
        }
        let next = all.get(i + 1).copied().unwrap_or_default();
        let changing = crate::dates::PAST.contains(&w)
            || crate::dates::FUTURE.contains(&w)
            || ["wczoraj", "dzisiaj", "dzis", "today", "yesterday", "chwila"].contains(&w)
            // "przed chwilą", "just posted", "just now".
            || (w == "przed" && next == "chwila")
            || (w == "just" && ["posted", "now"].contains(&next))
            || (["posted", "now"].contains(&w) && i > 0 && all[i - 1] == "just");
        if !changing {
            out.push(w.to_string());
        }
        i += 1;
    }
    out
}

/// How much of two descriptions is the same, in % (rounded down): pairs of neighbouring words
/// found in both. Both empty = the same; one empty = nothing in common.
pub fn similarity(a: &str, b: &str) -> u8 {
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return if a.is_empty() && b.is_empty() { 100 } else { 0 };
    }
    if a.len() < 2 || b.len() < 2 {
        return if a == b { 100 } else { 0 };
    }
    let mut pairs: HashMap<(&str, &str), i64> = HashMap::new();
    for p in a.windows(2) {
        *pairs.entry((p[0].as_str(), p[1].as_str())).or_default() += 1;
    }
    let mut common = 0;
    for p in b.windows(2) {
        if let Some(n) = pairs.get_mut(&(p[0].as_str(), p[1].as_str())).filter(|n| **n > 0) {
            *n -= 1;
            common += 1;
        }
    }
    let total = (a.len() - 1 + b.len() - 1) as i64;
    (200 * common / total).clamp(0, 100) as u8
}

pub(crate) fn snapshot(conn: &Connection, id: i64) -> AppResult<(SameWas, Option<String>, bool)> {
    let (was, description, open) = conn
        .query_row(
            "SELECT id, title, company, city, remote, salary_min, salary_max, salary_currency, salary_period,
                contract, level, published_at, expires_at, COALESCE(own, 0) = 1, stage, description,
                expired = 0 AND archived_at IS NULL AND expires_at IS NOT NULL AND expires_at > ?2
             FROM offers WHERE id = ?1",
            params![id, clock::now()],
            |r| {
                Ok((
                    SameWas {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        company: r.get(2)?,
                        city: r.get(3)?,
                        remote: r.get(4)?,
                        salary_min: r.get(5)?,
                        salary_max: r.get(6)?,
                        salary_currency: r.get(7)?,
                        salary_period: r.get(8)?,
                        contract: r.get(9)?,
                        level: r.get(10)?,
                        published_at: r.get(11)?,
                        expires_at: r.get(12)?,
                        own: r.get(13)?,
                        stage: r.get(14)?,
                        applied_at: None,
                        sent_via: None,
                    },
                    r.get::<_, Option<String>>(15)?,
                    r.get::<_, bool>(16)?,
                ))
            },
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    let mut was = was;
    if !matches!(was.stage, Stage::New | Stage::Interested) {
        was.applied_at = conn.query_row(
            "SELECT MIN(at) FROM stage_history WHERE offer_id = ?1 AND stage NOT IN ('new', 'interested')",
            [id],
            |r| r.get(0),
        )?;
    }
    let root: i64 = conn.query_row("SELECT COALESCE(parent_id, id) FROM offers WHERE id = ?1", [id], |r| r.get(0))?;
    was.sent_via = crate::tree::sent_via(conn, root, false)?;
    Ok((was, description, open))
}

fn day(date: Option<&str>) -> Option<&str> {
    date.map(|d| d.get(..10).unwrap_or(d))
}

/// Fields the form fills differently than the offer on the board has them. An empty field of the
/// form is no difference (it keeps what is there).
fn differences(was: &SameWas, old_text: Option<&str>, input: &OfferInput, same_text: u8) -> Vec<String> {
    let mut out = Vec::new();
    let mut differ = |field: &str, yes: bool| {
        if yes {
            out.push(field.to_string());
        }
    };
    let words_differ = |old: Option<&str>, new: Option<&str>| {
        new.is_some_and(|n| old.map(dedup::normalize) != Some(dedup::normalize(n)))
    };
    differ("title", dedup::normalize(&was.title) != dedup::normalize(&input.title));
    // A hidden company of the form ("Klient portalu...") says nothing new about a named one.
    differ(
        "company",
        dedup::normalize(&was.company) != dedup::normalize(&input.company) && !crate::dups::company_hidden(&input.company),
    );
    // A city of the list ("Kraków, Gdańsk") is no other city.
    differ("city", input.city.is_some() && offers::join_cities(was.city.as_deref(), input.city.as_deref()) != was.city);
    differ("remote", input.remote.is_some_and(|r| r != Remote::Unknown && r != was.remote));
    let salary_now = (input.salary_min, input.salary_max, input.salary_currency.as_deref(), input.salary_period);
    let salary_was = (was.salary_min, was.salary_max, was.salary_currency.as_deref(), was.salary_period);
    differ("salary", (input.salary_min.is_some() || input.salary_max.is_some()) && salary_now != salary_was);
    differ("contract", words_differ(was.contract.as_deref(), input.contract.as_deref()));
    differ("level", words_differ(was.level.as_deref(), input.level.as_deref()));
    differ("published", input.published_at.is_some() && day(input.published_at.as_deref()) != day(was.published_at.as_deref()));
    differ("expires", input.expires_at.is_some() && day(input.expires_at.as_deref()) != day(was.expires_at.as_deref()));
    differ("description", input.description.is_some() && (old_text.is_none() || same_text < 100));
    out
}

/// Of the differences, the fields the offer on the board does not have yet (point 16: an offer
/// read before only in part, e.g. from an alert) - the form fills them in, nothing to choose.
/// A longer description is the full one; a hidden company ("Klient portalu...") gets its name.
fn filled(was: &SameWas, old_text: Option<&str>, input: &OfferInput, fields: &[String]) -> Vec<String> {
    let old_len = old_text.map_or(0, |t| t.trim().chars().count());
    fields
        .iter()
        .filter(|f| match f.as_str() {
            "company" => crate::dups::company_hidden(&was.company),
            "city" => was.city.as_deref().is_none_or(|c| c.trim().is_empty()),
            "remote" => was.remote == Remote::Unknown,
            "salary" => was.salary_min.is_none() && was.salary_max.is_none(),
            "contract" => was.contract.is_none(),
            "level" => was.level.is_none(),
            "published" => was.published_at.is_none(),
            "expires" => was.expires_at.is_none(),
            "description" => input.description.as_deref().is_some_and(|d| d.trim().chars().count() > old_len),
            _ => false,
        })
        .cloned()
        .collect()
}

/// Every link of an offer (its own and its sources').
fn links_of(conn: &Connection, id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT url FROM offer_sources WHERE offer_id = ?1 AND url IS NOT NULL
         UNION SELECT url FROM offers WHERE id = ?1 AND url IS NOT NULL",
    )?;
    let links = stmt.query_map([id], |r| r.get::<_, String>(0))?.collect::<Result<_, _>>()?;
    Ok(links)
}

const NOTHING: SameOffer =
    SameOffer { kind: SameKind::None, was: None, fields: Vec::new(), filled: Vec::new(), similarity: 0, candidates: Vec::new() };

/// Is the offer of the form already on the board? First the same portal (`same_portal`), then
/// another one (`other_portal`). An offer in the trash is not looked at - adding it says "deleted
/// before" as always.
pub fn check(conn: &Connection, input: OfferInput) -> AppResult<SameOffer> {
    let input = offers::clean(input)?;
    let found = same_portal(conn, &input)?;
    if found.kind != SameKind::None {
        return Ok(found);
    }
    other_portal(conn, &input)
}

/// The same company and title elsewhere: the same place and salaries that do not contradict -
/// nothing to ask (saving joins it as a child by itself, "Detach" undoes it); another city or
/// clearly other salary - the person says. Else "probably the same" offers the code found
/// (another title, a hidden company) - AI judges them in the form, the person decides.
fn other_portal(conn: &Connection, input: &OfferInput) -> AppResult<SameOffer> {
    let ad = crate::dups::Ad::of(input);
    let family = crate::dups::family(conn, &dedup::fingerprint(&input.company, &input.title))?;
    if !family.is_empty() {
        return Ok(match crate::dups::first_fit(conn, &ad, &family)? {
            Err(Some((id, fit))) => {
                let (was, old_text, _) = snapshot(conn, id)?;
                let similarity = match (old_text.as_deref(), input.description.as_deref()) {
                    (Some(old), Some(new)) => similarity(old, new),
                    _ => 0,
                };
                SameOffer { kind: SameKind::Ask, was: Some(was), fields: vec![fit.why().to_string()], similarity, ..NOTHING }
            }
            // Joins as a child by itself - said first only when the CV already went for this job.
            Ok(id) => {
                let (was, _, _) = snapshot(conn, id)?;
                if was.sent_via.is_some() { SameOffer { kind: SameKind::Joined, was: Some(was), ..NOTHING } } else { NOTHING }
            }
            _ => NOTHING,
        });
    }
    let candidates = crate::dups::candidates(conn, &ad, &[])?;
    let Some(first) = candidates.first() else { return Ok(NOTHING) };
    Ok(SameOffer { kind: SameKind::Probable, was: Some(first.was.clone()), similarity: first.similarity, candidates, ..NOTHING })
}

/// "Fill in that one" (point 16): what the form would change in an offer on the board - the empty
/// fields it fills (`filled`) and the ones with another value (the person picks a version).
pub fn compare(conn: &Connection, id: i64, input: OfferInput) -> AppResult<SameOffer> {
    let input = offers::clean(input)?;
    let (was, old_text, _) = snapshot(conn, id)?;
    let same_text = similarity(old_text.as_deref().unwrap_or(""), input.description.as_deref().unwrap_or(""));
    let fields = differences(&was, old_text.as_deref(), &input, same_text);
    let filled = filled(&was, old_text.as_deref(), &input, &fields);
    Ok(SameOffer { kind: SameKind::Complete, was: Some(was), fields, filled, similarity: same_text, candidates: Vec::new() })
}

/// The same portal: the same link (identical, read before only in part, changed), or a new number
/// of the portal for the same company and title (a second ad, or almost the same text renewed).
/// Only an offer with a link - the portal is known from it.
fn same_portal(conn: &Connection, input: &OfferInput) -> AppResult<SameOffer> {
    let Some(url) = input.url.clone() else { return Ok(NOTHING) };
    let key = alerts::link_key(&url);

    // The same link (or the same offer number of the portal).
    let same_link: Option<i64> = {
        let mut stmt = conn.prepare(
            "SELECT o.id, s.url FROM offer_sources s JOIN offers o ON o.id = s.offer_id
             WHERE o.deleted_at IS NULL AND s.url IS NOT NULL
             UNION ALL SELECT id, url FROM offers WHERE deleted_at IS NULL AND url IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut found = None;
        for row in rows {
            let (id, link) = row?;
            if alerts::link_key(&link) == key {
                found = Some(id);
                break;
            }
        }
        found
    };
    if let Some(id) = same_link {
        let (was, old_text, _) = snapshot(conn, id)?;
        let same_text = similarity(old_text.as_deref().unwrap_or(""), input.description.as_deref().unwrap_or(""));
        let fields = differences(&was, old_text.as_deref(), input, same_text);
        let filled = filled(&was, old_text.as_deref(), input, &fields);
        // Only empty fields filled in (and the full text) - read before in part, not changed.
        let kind = if fields.is_empty() {
            SameKind::Identical
        } else if fields.iter().all(|f| filled.contains(f)) {
            SameKind::Partial
        } else {
            SameKind::Changed
        };
        return Ok(SameOffer { kind, was: Some(was), fields, filled, similarity: same_text, candidates: Vec::new() });
    }

    // Another link of the same portal for the same company and title.
    let Some(portal) = alerts::portal_of_link(&url) else { return Ok(NOTHING) };
    let fingerprint = dedup::fingerprint(&input.company, &input.title);
    let twin: Option<i64> = conn
        .query_row("SELECT id FROM offers WHERE fingerprint = ?1 AND deleted_at IS NULL", [&fingerprint], |r| r.get(0))
        .optional()?;
    let Some(id) = twin else { return Ok(NOTHING) };
    if !links_of(conn, id)?.iter().any(|l| alerts::portal_of_link(l).as_deref() == Some(portal.as_str())) {
        return Ok(NOTHING);
    }
    let (was, old_text, open) = snapshot(conn, id)?;
    // The first ad is still open (its date ahead) - the portal shows two ads at once.
    if open {
        return Ok(SameOffer { kind: SameKind::Second, was: Some(was), ..NOTHING });
    }
    let same_text = match (old_text.as_deref(), input.description.as_deref()) {
        (Some(old), Some(new)) => similarity(old, new),
        _ => 0,
    };
    if same_text < SAME_TEXT {
        // Another offer of the same company and title - the duplicate rules decide (the tree).
        return Ok(NOTHING);
    }
    let fields = differences(&was, old_text.as_deref(), input, same_text);
    let filled = filled(&was, old_text.as_deref(), input, &fields);
    Ok(SameOffer { kind: SameKind::Similar, was: Some(was), fields, filled, similarity: same_text, candidates: Vec::new() })
}

/// "Update the existing one": the fields of the form go into the offer on the board (an empty one
/// keeps what is there, and so does every field in `keep` - the person chose the old version);
/// the stage, the notes and the history stay, the offer is own. `renew` (the ad came again): it is
/// open again (no "expired", out of the archive) and the history says "ad renewed"; without it
/// (point 16: "Fill in") only the fields change. Returns whether the description changed (AI
/// reads it again).
pub fn update(conn: &mut Connection, id: i64, input: OfferInput, keep: &[String], renew: bool) -> AppResult<bool> {
    let mut input = offers::clean(input)?;
    let now = clock::now();
    let tx = conn.transaction()?;
    type Old = (Stage, Option<String>, bool, Option<String>, String, String);
    let (stage, old_text, closed, published, old_title, old_company): Old = tx
        .query_row(
            "SELECT stage, description, expired = 1 OR archived_at IS NOT NULL, published_at, title, company
             FROM offers WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    for field in keep {
        match field.as_str() {
            "title" => input.title = old_title.clone(),
            "company" => input.company = old_company.clone(),
            "city" => input.city = None,
            "remote" => input.remote = None,
            "salary" => (input.salary_min, input.salary_max, input.salary_currency, input.salary_period) = (None, None, None, None),
            "contract" => input.contract = None,
            "level" => input.level = None,
            "published" => input.published_at = None,
            "expires" => input.expires_at = None,
            "description" => (input.description, input.requirements) = (None, None),
            _ => {}
        }
    }
    // Renewed: a newer publication date, or the ad was over and is open again.
    let newer = input.published_at.as_deref().is_some_and(|p| day(Some(p)) > day(published.as_deref()));
    let renewed = renew && (newer || closed);
    // The ad came again after the CV went - the person decides about a second CV.
    let ask = renewed && !matches!(stage, Stage::New | Stage::Interested);
    let fingerprint = dedup::fingerprint(&input.company, &input.title);
    let taken: bool = tx
        .query_row("SELECT 1 FROM offers WHERE fingerprint = ?1 AND id != ?2", params![fingerprint, id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    let has_salary = input.salary_min.is_some() || input.salary_max.is_some();
    tx.execute(
        "UPDATE offers SET
            title = ?2, company = ?3, fingerprint = CASE WHEN ?4 THEN fingerprint ELSE ?5 END,
            city = COALESCE(?6, city),
            remote = CASE WHEN ?7 IS NOT NULL AND ?7 != 'unknown' THEN ?7 ELSE remote END,
            salary_min = CASE WHEN ?8 THEN ?9 ELSE salary_min END,
            salary_max = CASE WHEN ?8 THEN ?10 ELSE salary_max END,
            salary_currency = CASE WHEN ?8 THEN ?11 ELSE salary_currency END,
            salary_period = CASE WHEN ?8 THEN ?12 ELSE salary_period END,
            contract = COALESCE(?13, contract), level = COALESCE(?14, level),
            requirements = COALESCE(?15, requirements), description = COALESCE(?16, description),
            published_at = CASE WHEN ?17 IS NOT NULL AND ?17 > COALESCE(published_at, '') THEN ?17 ELSE published_at END,
            expires_at = COALESCE(?18, expires_at),
            url = COALESCE(url, ?19),
            expired = CASE WHEN ?22 THEN 0 ELSE expired END,
            expired_by_user = CASE WHEN ?22 THEN 0 ELSE expired_by_user END,
            archived_at = CASE WHEN ?22 THEN NULL ELSE archived_at END,
            content_removed = CASE WHEN ?22 THEN NULL ELSE content_removed END,
            own = 1, user_changed_at = ?20, updated_at = ?20,
            renewed_at = CASE WHEN ?22 THEN ?20 ELSE renewed_at END,
            renew_asked = CASE WHEN ?21 THEN 1 ELSE COALESCE(renew_asked, 0) END
         WHERE id = ?1",
        params![
            id, input.title, input.company, taken, fingerprint, input.city, input.remote,
            has_salary, input.salary_min, input.salary_max, input.salary_currency, input.salary_period,
            input.contract, input.level, input.requirements, input.description, input.published_at,
            input.expires_at, input.url, now, ask, renew
        ],
    )?;
    offers::add_link(&tx, id, &input, &now)?;
    let new_text = input.description.as_deref().is_some_and(|d| Some(d) != old_text.as_deref());
    if new_text {
        tx.execute("UPDATE offers SET ai_status = NULL, ai_error = NULL WHERE id = ?1", [id])?;
    }
    crate::profiles::recheck_offer(&tx, id)?;
    tx.commit()?;
    Ok(new_text)
}

/// The answer to "the ad was renewed after your CV - send it again?": `resent` = the person sent
/// it once more (the history gets "applied" today). Either way the mark RENEWED goes.
pub fn answer_renewed(conn: &mut Connection, id: i64, resent: bool) -> AppResult<()> {
    let now = clock::now();
    let tx = conn.transaction()?;
    let changed = tx.execute(
        "UPDATE offers SET renew_asked = 0, user_changed_at = ?2 WHERE id = ?1 AND COALESCE(renew_asked, 0) = 1",
        params![id, now],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    if resent {
        tx.execute(
            "INSERT INTO stage_history (offer_id, stage, at) VALUES (?1, ?2, ?3)",
            params![id, Stage::Applied, now],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offers::{add, set_stage};

    const TEXT: &str = "Piekarz\n1 dzień temu\nSieć Cukierni Przykładowa szuka piekarza do pracy w Krakowie.\n\
        Wymagania: doświadczenie w piekarni, książeczka sanepidowska, gotowość do pracy w nocy.\n\
        Oferujemy umowę o pracę, premie i szkolenia. Wyświetlenia: 120. Oferta wygasa za 12 dni.";

    fn offer(url: Option<&str>, description: &str) -> OfferInput {
        OfferInput {
            title: "Piekarz".into(),
            company: "Sieć Cukierni Przykładowa".into(),
            source: "manual".into(),
            url: url.map(Into::into),
            city: Some("Kraków".into()),
            remote: None,
            salary_min: Some(5000),
            salary_max: Some(6000),
            salary_currency: Some("PLN".into()),
            salary_period: Some(SalaryPeriod::Month),
            contract: Some("umowa o pracę".into()),
            level: None,
            language: None,
            requirements: None,
            description: Some(description.into()),
            expires_at: None,
            published_at: Some("2026-11-01T08:00:00Z".into()),
        }
    }

    fn id_of(outcome: AddOutcome) -> i64 {
        match outcome {
            AddOutcome::Added { id } | AddOutcome::Merged { id } | AddOutcome::Unchanged { id } => id,
            AddOutcome::Blocked => panic!("blocked"),
        }
    }

    const LINK: &str = "https://www.praca.pl/piekarz_123456.html";

    #[test]
    fn changing_parts_of_the_text_do_not_count() {
        let renewed = TEXT.replace("1 dzień temu", "3 godz. temu").replace("120", "348").replace("za 12 dni", "za 30 dni");
        assert_eq!(similarity(TEXT, &renewed), 100);
        let other = TEXT.replace("w nocy", "w dzień, bez nocek").replace("premie i szkolenia", "kartę sportową");
        let s = similarity(TEXT, &other);
        assert!(s < 100 && s > 50, "{s}");
        assert_eq!(similarity("", ""), 100);
        assert_eq!(similarity(TEXT, ""), 0);
    }

    #[test]
    fn the_same_link_with_the_same_fields_is_identical() {
        let mut conn = crate::db::tests::test_db();
        let id = id_of(add(&mut conn, offer(Some(LINK), TEXT)).unwrap());
        // Tracking parameters and "3 hours ago" instead of "1 day ago" - the same offer.
        let again = offer(Some(&format!("{LINK}?utm_source=x")), &TEXT.replace("1 dzień temu", "3 godz. temu"));
        let found = check(&conn, again).unwrap();
        assert_eq!(found.kind, SameKind::Identical);
        assert_eq!(found.was.unwrap().id, id);
        // A form with fewer fields filled is no difference either.
        let short = OfferInput { salary_min: None, salary_max: None, contract: None, city: None, ..offer(Some(LINK), TEXT) };
        assert_eq!(check(&conn, short).unwrap().kind, SameKind::Identical);
    }

    #[test]
    fn the_same_link_with_other_fields_lists_them_and_updates_on_request() {
        let mut conn = crate::db::tests::test_db();
        let id = id_of(add(&mut conn, offer(Some(LINK), TEXT)).unwrap());
        conn.execute("UPDATE offers SET notes = 'moja notatka', expired = 1, expires_at = '2026-11-05T21:59:59Z' WHERE id = ?1", [id]).unwrap();
        let newer = OfferInput {
            salary_max: Some(6500),
            published_at: Some("2026-11-10T08:00:00Z".into()),
            expires_at: Some("2026-12-10T21:59:59Z".into()),
            ..offer(Some(LINK), TEXT)
        };
        let found = check(&conn, newer.clone()).unwrap();
        assert_eq!(found.kind, SameKind::Changed);
        assert_eq!(found.fields, ["salary", "published", "expires"]);
        assert!(!update(&mut conn, id, newer, &[], true).unwrap(), "the same text - AI does not read it again");
        let (max, expired, notes, renewed, asked): (i64, bool, String, Option<String>, i64) = conn
            .query_row("SELECT salary_max, expired, notes, renewed_at, renew_asked FROM offers WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        assert_eq!((max, expired, notes.as_str(), renewed.is_some(), asked), (6500, false, "moja notatka", true, 0));
        // One offer, nothing new added.
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM offers", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }

    #[test]
    fn renewed_after_the_cv_asks_whether_to_send_it_again() {
        let mut conn = crate::db::tests::test_db();
        let id = id_of(add(&mut conn, offer(Some(LINK), TEXT)).unwrap());
        set_stage(&mut conn, id, Stage::Applied).unwrap();
        let found = check(&conn, OfferInput { published_at: Some("2026-11-10T08:00:00Z".into()), ..offer(Some(LINK), TEXT) }).unwrap();
        assert!(found.was.as_ref().unwrap().applied_at.is_some());
        update(&mut conn, id, OfferInput { published_at: Some("2026-11-10T08:00:00Z".into()), ..offer(Some(LINK), TEXT) }, &[], true).unwrap();
        let row = crate::offers::detail(&conn, id).unwrap();
        assert_eq!(row.row.stage, Stage::Applied, "the stage stays");
        assert!(row.row.renew_asked);
        answer_renewed(&mut conn, id, true).unwrap();
        let row = crate::offers::detail(&conn, id).unwrap();
        assert!(!row.row.renew_asked);
        assert_eq!(row.history.iter().filter(|h| h.stage == Stage::Applied).count(), 2);
        assert!(answer_renewed(&mut conn, id, false).is_err(), "nothing to answer any more");
    }

    #[test]
    fn a_new_number_of_the_same_portal() {
        let mut conn = crate::db::tests::test_db();
        let id = id_of(add(&mut conn, offer(Some(LINK), TEXT)).unwrap());
        let new_link = Some("https://www.praca.pl/piekarz_999999.html");
        // The first ad is still open: a second ad at once.
        conn.execute("UPDATE offers SET expires_at = '2099-01-01T00:00:00Z' WHERE id = ?1", [id]).unwrap();
        assert_eq!(check(&conn, offer(new_link, TEXT)).unwrap().kind, SameKind::Second);
        // The first is over: almost the same text = probably renewed, the person says.
        conn.execute("UPDATE offers SET expired = 1 WHERE id = ?1", [id]).unwrap();
        let found = check(&conn, offer(new_link, &TEXT.replace("1 dzień temu", "przed chwilą"))).unwrap();
        assert_eq!((found.kind, found.similarity), (SameKind::Similar, 100));
        // Another text: another offer of the company - the duplicate rules decide.
        let other = "Piekarz na zmianę dzienną w nowym zakładzie w Wieliczce. Praca od poniedziałku do piątku, \
            bez nocek, umowa zlecenie, możliwość przejścia na etat po trzech miesiącach.";
        assert_eq!(check(&conn, offer(new_link, other)).unwrap().kind, SameKind::None);
        // Another portal, an offer without a link - not this point's question.
        assert_eq!(check(&conn, offer(Some("https://www.olx.pl/oferta/praca/piekarz-CID4-ID1.html"), TEXT)).unwrap().kind, SameKind::None);
        assert_eq!(check(&conn, offer(None, TEXT)).unwrap().kind, SameKind::None);
    }

    #[test]
    fn an_offer_read_in_part_is_filled_in_and_the_person_picks_other_values() {
        let mut conn = crate::db::tests::test_db();
        // From an alert: a hidden company, no salary, a short text.
        let alert = OfferInput {
            company: "Klient portalu Praca.pl".into(),
            source: "pracapl".into(),
            salary_min: None,
            salary_max: None,
            contract: None,
            published_at: None,
            ..offer(Some(LINK), "Piekarz w Krakowie, praca w nocy.")
        };
        let id = id_of(add(&mut conn, alert).unwrap());
        let full = OfferInput { published_at: None, ..offer(Some(LINK), TEXT) };
        let found = check(&conn, full.clone()).unwrap();
        assert_eq!(found.kind, SameKind::Partial, "{:?}", found.fields);
        assert_eq!(found.filled, found.fields);
        assert!(found.fields.contains(&"company".to_string()) && found.fields.contains(&"salary".to_string()));
        // Another value of a filled field: the person picks - here the old city stays.
        let other_city = OfferInput { city: Some("Wieliczka".into()), ..full };
        let compared = compare(&conn, id, other_city.clone()).unwrap();
        assert_eq!(compared.kind, SameKind::Complete);
        assert!(compared.fields.contains(&"city".to_string()) && !compared.filled.contains(&"city".to_string()));
        update(&mut conn, id, other_city, &["city".to_string()], false).unwrap();
        let row = crate::offers::detail(&conn, id).unwrap();
        assert_eq!(row.row.company, "Sieć Cukierni Przykładowa");
        assert_eq!(row.row.city.as_deref(), Some("Kraków"));
        assert_eq!(row.row.salary_max, Some(6000));
        assert_eq!(row.description.as_deref(), Some(TEXT));
        assert!(row.renewed_at.is_none(), "filled in, not renewed");
    }

    #[test]
    fn another_portal_asks_about_another_city_and_offers_probable_ones() {
        let mut conn = crate::db::tests::test_db();
        let first = id_of(add(&mut conn, offer(Some(LINK), TEXT)).unwrap());
        let olx = "https://www.olx.pl/oferta/praca/piekarz-CID4-ID1.html";
        // The same city: nothing to ask - saving joins it by itself.
        assert_eq!(check(&conn, offer(Some(olx), TEXT)).unwrap().kind, SameKind::None);
        let gdansk = OfferInput { city: Some("Gdańsk".into()), ..offer(Some(olx), TEXT) };
        let found = check(&conn, gdansk).unwrap();
        assert_eq!((found.kind, found.fields.as_slice()), (SameKind::Ask, ["city".to_string()].as_slice()));
        assert_eq!(found.was.unwrap().id, first);
        // A hidden company under another title, the same place and salary: probably the same.
        let hidden = OfferInput { title: "Piekarz / Piekarka".into(), company: "Klient portalu Praca.pl".into(), ..offer(Some(olx), TEXT) };
        let found = check(&conn, hidden).unwrap();
        assert_eq!(found.kind, SameKind::Probable);
        assert_eq!(found.candidates[0].was.id, first);
    }

    #[test]
    fn an_offer_in_the_trash_is_not_offered() {
        let mut conn = crate::db::tests::test_db();
        let id = id_of(add(&mut conn, offer(Some(LINK), TEXT)).unwrap());
        crate::offers::trash(&conn, id).unwrap();
        assert_eq!(check(&conn, offer(Some(LINK), TEXT)).unwrap().kind, SameKind::None);
    }
}
