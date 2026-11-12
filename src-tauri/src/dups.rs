//! Duplicates from another portal (`zmiany.md`, point 15). Plain code compares, for free: the
//! company (without "Sp. z o.o.", case and Polish letters), the title (without "(k/m)" and a second
//! form like "/ Specjalistka"), the city or remote work, the salary and the text.
//!
//! - The same company and title, the same city (or both remote) and salaries that do not
//!   contradict each other: one job - the new ad is joined as a child by itself ("Detach" undoes it).
//! - The same company and title in another city or with clearly other salary: chains look for the
//!   same job in several cities - never joined by itself, the app asks.
//! - "Probably the same" (another title, a hidden company): code picks up to 5 offers, AI judges
//!   them (only on a click or when adding by hand), the person decides. AI alone joins nothing.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::clock;
use crate::dedup;
use crate::error::{AppError, AppResult};
use crate::model::*;
use crate::sources::alerts::link_key;

/// How many "probably the same" offers are shown (and sent to AI).
pub const MAX_CANDIDATES: usize = 5;

/// Salaries differ "clearly" when the ranges are more than this far apart (in %).
const SALARY_GAP: i64 = 15;

/// One ad as compared.
#[derive(Debug, Clone)]
pub struct Ad {
    pub title: String,
    pub company: String,
    pub city: Option<String>,
    pub remote: Remote,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub salary_currency: Option<String>,
    pub salary_period: Option<SalaryPeriod>,
    pub contract: Option<String>,
    pub description: Option<String>,
}

impl Ad {
    pub fn of(input: &OfferInput) -> Self {
        Self {
            title: input.title.clone(),
            company: input.company.clone(),
            city: input.city.clone(),
            remote: input.remote.unwrap_or(Remote::Unknown),
            salary_min: input.salary_min,
            salary_max: input.salary_max,
            salary_currency: input.salary_currency.clone(),
            salary_period: input.salary_period,
            contract: input.contract.clone(),
            description: input.description.clone(),
        }
    }

    #[cfg(test)]
    fn empty() -> Self {
        Self {
            title: String::new(),
            company: String::new(),
            city: None,
            remote: Remote::Unknown,
            salary_min: None,
            salary_max: None,
            salary_currency: None,
            salary_period: None,
            contract: None,
            description: None,
        }
    }

    pub fn load(conn: &Connection, id: i64) -> AppResult<Self> {
        conn.query_row(
            "SELECT title, company, city, remote, salary_min, salary_max, salary_currency, salary_period, contract, description
             FROM offers WHERE id = ?1",
            [id],
            |r| {
                Ok(Self {
                    title: r.get(0)?,
                    company: r.get(1)?,
                    city: r.get(2)?,
                    remote: r.get(3)?,
                    salary_min: r.get(4)?,
                    salary_max: r.get(5)?,
                    salary_currency: r.get(6)?,
                    salary_period: r.get(7)?,
                    contract: r.get(8)?,
                    description: r.get(9)?,
                })
            },
        )
        .optional()?
        .ok_or(AppError::NotFound)
    }
}

/// Two forms of one word: "Specjalista" / "Specjalistka", "Metrolog" / "Metrolożka", "Kelner" /
/// "Kelnerka" - a long common start.
fn forms_of_one_word(a: &str, b: &str) -> bool {
    let short = a.len().min(b.len());
    let common = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    short >= 4 && common >= short.min(5)
}

/// The title as compared: the fingerprint's title (no "(k/m)") without the second form of a word
/// after a slash ("Specjalista / Specjalistka ds. sprzedaży" = "Specjalista ds. sprzedaży").
pub fn title_core(title: &str) -> String {
    let mut parts = title.split('/');
    let mut kept = parts.next().unwrap_or_default().to_string();
    for part in parts {
        let last = dedup::normalize(&kept).rsplit(' ').next().unwrap_or_default().to_string();
        let rest = part.trim_start();
        let (first, after) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let first_norm = dedup::normalize(first);
        if !last.is_empty() && !first_norm.contains(' ') && forms_of_one_word(&last, &first_norm) {
            kept.push(' ');
            kept.push_str(after);
        } else {
            kept.push('/');
            kept.push_str(part);
        }
    }
    dedup::title_key(&kept)
}

/// No name to compare: the placeholder of an alert ("? (ab12)"), "Klient portalu Praca.pl",
/// "Pracodawca", "Poufne", "Confidential".
pub fn company_hidden(company: &str) -> bool {
    let key = dedup::company_key(company);
    let placeholder = company.starts_with("? (") && company.ends_with(')');
    placeholder
        || key.is_empty()
        || ["klient", "pracodawca", "anonim", "poufn", "confidential", "undisclosed", "nie podano"].iter().any(|w| key.starts_with(w))
}

/// Where the two ads are, as far as the fields tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Same,
    Other,
    Unknown,
}

fn cities(city: Option<&str>) -> Vec<String> {
    city.unwrap_or_default()
        .split(',')
        .map(dedup::normalize)
        .filter(|c| !c.is_empty() && !crate::filter::NOT_A_PLACE.contains(&c.as_str()))
        .collect()
}

pub fn place(a: &Ad, b: &Ad) -> Place {
    if a.remote == Remote::Yes && b.remote == Remote::Yes {
        return Place::Same;
    }
    let (x, y) = (cities(a.city.as_deref()), cities(b.city.as_deref()));
    if x.is_empty() || y.is_empty() {
        return Place::Unknown;
    }
    // "Gdańsk" and "Gdańsk pomorskie" are one city.
    let inside = |small: &str, big: &str| format!(" {big} ").contains(&format!(" {small} "));
    if x.iter().any(|c| y.iter().any(|d| inside(c, d) || inside(d, c))) { Place::Same } else { Place::Other }
}

/// The salary range, a missing end taken from the other; `None` = no salary or not comparable.
fn ranges(a: &Ad, b: &Ad) -> Option<((i64, i64), (i64, i64))> {
    let range = |x: &Ad| match (x.salary_min, x.salary_max) {
        (None, None) => None,
        (min, max) => Some((min.or(max)?, max.or(min)?)),
    };
    let differ = |p: Option<&str>, q: Option<&str>| p.zip(q).is_some_and(|(p, q)| !p.eq_ignore_ascii_case(q));
    if differ(a.salary_currency.as_deref(), b.salary_currency.as_deref())
        || differ(a.salary_period.map(|p| p.as_str()), b.salary_period.map(|p| p.as_str()))
    {
        return None;
    }
    Some((range(a)?, range(b)?))
}

/// Clearly other salaries: the ranges are more than `SALARY_GAP`% apart.
pub fn salary_conflict(a: &Ad, b: &Ad) -> bool {
    ranges(a, b).is_some_and(|((a0, a1), (b0, b1))| a1 * (100 + SALARY_GAP) < b0 * 100 || b1 * (100 + SALARY_GAP) < a0 * 100)
}

fn salary_same(a: &Ad, b: &Ad) -> bool {
    ranges(a, b).is_some_and(|(x, y)| x == y)
}

/// Is a new ad of the same company and title the same job as this one?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Same,
    OtherCity,
    OtherSalary,
}

impl Fit {
    pub fn why(self) -> &'static str {
        match self {
            Fit::Same => "",
            Fit::OtherCity => "city",
            Fit::OtherSalary => "salary",
        }
    }
}

pub fn fit(new: &Ad, old: &Ad) -> Fit {
    if place(new, old) == Place::Other {
        Fit::OtherCity
    } else if salary_conflict(new, old) {
        Fit::OtherSalary
    } else {
        Fit::Same
    }
}

/// Top-level offers of one company and title (not in the trash), the first added on top: the
/// fingerprint's own and those saved separately under it (`fingerprint#…`).
pub fn family(conn: &Connection, fingerprint: &str) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM offers
         WHERE (fingerprint = ?1 OR substr(fingerprint, 1, length(?1) + 1) = ?1 || '#')
           AND parent_id IS NULL AND deleted_at IS NULL
         ORDER BY added_at, id",
    )?;
    let ids = stmt.query_map([fingerprint], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(ids)
}

/// The first offer of `family` the new ad fits, or the first one with why it does not.
pub fn first_fit(conn: &Connection, new: &Ad, family: &[i64]) -> AppResult<Result<i64, Option<(i64, Fit)>>> {
    let mut misfit = None;
    for &id in family {
        match fit(new, &Ad::load(conn, id)?) {
            Fit::Same => return Ok(Ok(id)),
            other => {
                misfit.get_or_insert((id, other));
            }
        }
    }
    Ok(Err(misfit))
}

/// An offer saved under a marked fingerprint (a child, or one saved separately) with this link -
/// its fingerprint does not lead to it, its link does. `(id, in the trash)`.
pub fn marked_by_link(conn: &Connection, url: &str) -> AppResult<Option<(i64, bool)>> {
    let key = link_key(url);
    let mut stmt = conn.prepare(
        "SELECT o.id, o.deleted_at IS NOT NULL, s.url FROM offer_sources s JOIN offers o ON o.id = s.offer_id
         WHERE instr(o.fingerprint, '#') > 0 AND s.url IS NOT NULL
         UNION ALL SELECT id, deleted_at IS NOT NULL, url FROM offers WHERE instr(fingerprint, '#') > 0 AND url IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?, r.get::<_, String>(2)?)))?;
    for row in rows {
        let (id, deleted, link) = row?;
        if link_key(&link) == key {
            return Ok(Some((id, deleted)));
        }
    }
    Ok(None)
}

/// "Probably the same" offer, as the code found it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupCandidate {
    pub was: SameWas,
    pub sources: Vec<String>,
    /// What agrees: "company", "hidden" (a company not given), "title", "titleWords", "city",
    /// "salary", "text".
    pub reasons: Vec<String>,
    /// How much of the description is the same, in %.
    pub similarity: u8,
}

fn words(text: &str) -> std::collections::HashSet<String> {
    title_core(text).split(' ').filter(|w| w.len() > 1).map(str::to_string).collect()
}

fn overlap(a: &str, b: &str) -> f64 {
    let (a, b) = (words(a), words(b));
    let all = a.union(&b).count();
    if all == 0 { 0.0 } else { a.intersection(&b).count() as f64 / all as f64 }
}

/// Up to `MAX_CANDIDATES` offers on the board that may be the same job under another title or a
/// hidden company: the same company with a similar title or text, or a hidden company in the same
/// place with the same salary or text. `skip` - the offer itself and its tree.
pub fn candidates(conn: &Connection, new: &Ad, skip: &[i64]) -> AppResult<Vec<DupCandidate>> {
    let new_company = dedup::company_key(&new.company);
    let new_hidden = company_hidden(&new.company);
    let new_title = title_core(&new.title);
    let mut stmt = conn.prepare("SELECT id FROM offers WHERE deleted_at IS NULL AND parent_id IS NULL ORDER BY id DESC")?;
    let ids: Vec<i64> = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut found: Vec<(i64, i64, Vec<String>, u8)> = Vec::new();
    for id in ids {
        if skip.contains(&id) {
            continue;
        }
        let old = Ad::load(conn, id)?;
        let hidden = new_hidden || company_hidden(&old.company);
        let same_company = !hidden && dedup::company_key(&old.company) == new_company;
        if !same_company && !hidden {
            continue;
        }
        let same_title = title_core(&old.title) == new_title;
        let close_title = !same_title && overlap(&old.title, &new.title) >= 0.5;
        let where_ = place(new, &old);
        let same_pay = salary_same(new, &old);
        let other_pay = salary_conflict(new, &old);
        let similarity = match (new.description.as_deref(), old.description.as_deref()) {
            (Some(a), Some(b)) if !a.trim().is_empty() && !b.trim().is_empty() => crate::same::similarity(a, b),
            _ => 0,
        };
        let gate = if same_company {
            same_title || close_title || similarity >= 50
        } else {
            where_ != Place::Other && (same_pay || similarity >= 50 || same_title)
        };
        if !gate {
            continue;
        }
        let mut score = 0;
        let mut reasons = Vec::new();
        let mut agree = |yes: bool, points: i64, why: &str| {
            if yes {
                score += points;
                reasons.push(why.to_string());
            }
        };
        agree(same_company, 3, "company");
        agree(hidden, 0, "hidden");
        agree(same_title, 3, "title");
        agree(close_title, 2, "titleWords");
        agree(where_ == Place::Same, 1, "city");
        agree(same_pay, 2, "salary");
        agree(similarity >= 50, if similarity >= 85 { 3 } else { 1 }, "text");
        if where_ == Place::Other {
            score -= 2;
        }
        if other_pay {
            score -= 2;
        }
        if score >= 4 {
            found.push((score, id, reasons, similarity));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    found.truncate(MAX_CANDIDATES);
    found
        .into_iter()
        .map(|(_, id, reasons, similarity)| {
            Ok(DupCandidate { was: crate::same::snapshot(conn, id)?.0, sources: sources_of(conn, id)?, reasons, similarity })
        })
        .collect()
}

pub fn sources_of(conn: &Connection, id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT source FROM offer_sources WHERE offer_id = ?1 ORDER BY source")?;
    let sources = stmt.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(sources)
}

/// The offer and its tree - not offered as its own duplicate.
pub fn tree_of(conn: &Connection, id: i64) -> AppResult<Vec<i64>> {
    let top: i64 = conn.query_row("SELECT COALESCE(parent_id, id) FROM offers WHERE id = ?1", [id], |r| r.get(0))?;
    let mut stmt = conn.prepare("SELECT id FROM offers WHERE id = ?1 OR parent_id = ?1")?;
    let ids = stmt.query_map([top], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(ids)
}

/// "Join as a child": two offers on the board become one tree - the one added first is the parent
/// (`zmiany.md`: the parent is the first ad), the other its child; the children of the later one
/// go under the parent too (one level). Returns `(parent, child)`.
pub fn join(conn: &mut Connection, a: i64, b: i64) -> AppResult<(i64, i64)> {
    let tx = conn.transaction()?;
    let top = |id: i64| -> AppResult<(String, i64)> {
        tx.query_row(
            "SELECT added_at, id FROM offers WHERE id = ?1 AND parent_id IS NULL AND deleted_at IS NULL",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::Invalid("only two offers on the board can be joined".into()))
    };
    let (first, second) = (top(a)?, top(b)?);
    if a == b {
        return Err(AppError::Invalid("an offer cannot be joined with itself".into()));
    }
    let (parent, child) = if first <= second { (first.1, second.1) } else { (second.1, first.1) };
    let now = clock::now();
    tx.execute("UPDATE offers SET parent_id = ?1, updated_at = ?2, user_changed_at = ?2 WHERE parent_id = ?3", params![parent, now, child])?;
    tx.execute(
        "UPDATE offers SET parent_id = ?1, dup_of = NULL, dup_why = NULL, updated_at = ?2, user_changed_at = ?2 WHERE id = ?3",
        params![parent, now, child],
    )?;
    tx.execute("UPDATE offers SET dup_of = NULL, dup_why = NULL WHERE id = ?1 AND dup_of = ?2", params![parent, child])?;
    crate::tree::join_own(&tx, parent)?;
    tx.commit()?;
    Ok((parent, child))
}

/// The answer to "the same company and title - the same ad?" on the card: `join` = one tree,
/// otherwise a separate offer and the question goes. Returns the parent when joined.
pub fn answer(conn: &mut Connection, id: i64, join_it: bool) -> AppResult<Option<i64>> {
    let other: Option<i64> = conn
        .query_row("SELECT dup_of FROM offers WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .flatten();
    let Some(other) = other else { return Err(AppError::NotFound) };
    if join_it {
        return Ok(Some(join(conn, id, other)?.0));
    }
    conn.execute(
        "UPDATE offers SET dup_of = NULL, dup_why = NULL, user_changed_at = ?2 WHERE id = ?1",
        params![id, clock::now()],
    )?;
    Ok(None)
}

/// The question on the card: the offer it may be and why it was not joined.
pub fn note(conn: &Connection, id: i64) -> AppResult<Option<DupNote>> {
    let row: Option<(i64, Option<String>)> = conn
        .query_row(
            "SELECT o.dup_of, o.dup_why FROM offers o JOIN offers d ON d.id = o.dup_of
             WHERE o.id = ?1 AND d.deleted_at IS NULL AND d.parent_id IS NULL",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((other, why)) = row else { return Ok(None) };
    Ok(Some(DupNote {
        was: crate::same::snapshot(conn, other)?.0,
        sources: sources_of(conn, other)?,
        why: why.unwrap_or_else(|| "city".into()),
    }))
}

/// "The same company and title as that offer, but…" on the card.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupNote {
    pub was: SameWas,
    pub sources: Vec<String>,
    /// "city" or "salary".
    pub why: String,
}

/// What AI is shown about an ad (data from the internet - the prompt says so).
pub fn ad_text(ad: &Ad) -> String {
    let salary = match (ad.salary_min, ad.salary_max) {
        (None, None) => "not given".to_string(),
        (a, b) => format!(
            "{}-{} {} per {}",
            a.map_or("?".into(), |v| v.to_string()),
            b.map_or("?".into(), |v| v.to_string()),
            ad.salary_currency.as_deref().unwrap_or("PLN"),
            ad.salary_period.map_or("month", |p| p.as_str())
        ),
    };
    let remote = match ad.remote {
        Remote::Yes => "remote",
        Remote::Hybrid => "hybrid",
        Remote::No => "on site",
        Remote::Unknown => "not given",
    };
    let description = crate::text::cut(ad.description.as_deref().unwrap_or("not given"), 1500);
    format!(
        "Title: {}\nCompany: {}\nCity: {}\nWork mode: {remote}\nSalary: {salary}\nContract: {}\nDescription:\n{}",
        ad.title,
        if company_hidden(&ad.company) { "not given" } else { ad.company.as_str() },
        ad.city.as_deref().unwrap_or("not given"),
        ad.contract.as_deref().unwrap_or("not given"),
        description
    )
    .replace(['<', '>'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::offers::{add, add_placed, Placement};

    fn ad(city: Option<&str>, min: Option<i64>, max: Option<i64>) -> Ad {
        Ad {
            title: "Piekarz".into(),
            company: "Sieć Cukierni Przykładowa".into(),
            city: city.map(Into::into),
            salary_min: min,
            salary_max: max,
            salary_currency: Some("PLN".into()),
            salary_period: Some(SalaryPeriod::Month),
            ..Ad::empty()
        }
    }

    fn offer(source: &str, url: &str, city: &str, min: i64) -> OfferInput {
        OfferInput {
            title: "Piekarz".into(),
            company: "Sieć Cukierni Przykładowa Sp. z o.o.".into(),
            source: source.into(),
            url: Some(url.into()),
            city: Some(city.into()),
            salary_min: Some(min),
            salary_max: Some(min + 1000),
            salary_currency: Some("PLN".into()),
            salary_period: Some(SalaryPeriod::Month),
            description: Some("Wypiek pieczywa i ciast, praca w nocy, umowa o pracę.".into()),
            ..Default::default()
        }
    }

    fn id_of(outcome: AddOutcome) -> i64 {
        match outcome {
            AddOutcome::Added { id } | AddOutcome::Merged { id } | AddOutcome::Unchanged { id } => id,
            AddOutcome::Blocked => panic!("blocked"),
        }
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn titles_without_the_second_form() {
        assert_eq!(title_core("Specjalista / Specjalistka ds. sprzedaży"), "specjalista ds sprzedazy");
        assert_eq!(title_core("Metrolog/Metrolożka (k/m)"), "metrolog");
        assert_eq!(title_core("Kelner / Kelnerka"), "kelner");
        // Two jobs joined by a slash stay two words.
        assert_eq!(title_core("Frontend / Fullstack Developer"), "frontend fullstack developer");
        assert!(company_hidden("Klient portalu Praca.pl"));
        assert!(company_hidden("? (ab12)"));
        assert!(!company_hidden("Sieć Cukierni Przykładowa"));
    }

    #[test]
    fn place_and_salary() {
        assert_eq!(fit(&ad(Some("Kraków"), Some(5000), Some(6000)), &ad(Some("Kraków, małopolskie"), Some(5200), None)), Fit::Same);
        assert_eq!(fit(&ad(Some("Kraków"), None, None), &ad(Some("Gdańsk"), None, None)), Fit::OtherCity);
        assert_eq!(fit(&ad(Some("Kraków"), Some(5000), Some(6000)), &ad(Some("Kraków"), Some(9000), Some(11000))), Fit::OtherSalary);
        // Unknown city or salary: nothing contradicts.
        assert_eq!(fit(&ad(None, None, None), &ad(Some("Gdańsk"), Some(9000), None)), Fit::Same);
        // Both remote: the same place, whatever city is written.
        let (mut a, mut b) = (ad(Some("Kraków"), None, None), ad(Some("Gdańsk"), None, None));
        a.remote = Remote::Yes;
        b.remote = Remote::Yes;
        assert_eq!(fit(&a, &b), Fit::Same);
    }

    #[test]
    fn the_same_city_joins_by_itself_another_city_asks() {
        let mut conn = test_db();
        let first = id_of(add(&mut conn, offer("pracuj", "https://www.pracuj.pl/praca/piekarz,oferta,1001", "Kraków", 5000)).unwrap());
        // Another portal, the same city: a child.
        let olx = add(&mut conn, offer("olx", "https://www.olx.pl/oferta/praca/piekarz-CID4-ID1.html", "Kraków", 5000)).unwrap();
        assert_eq!(olx, AddOutcome::Merged { id: first });
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM offers WHERE parent_id IS NOT NULL"), 1);
        // Another city: a separate offer with the question.
        let gdansk = id_of(add(&mut conn, offer("olx", "https://www.olx.pl/oferta/praca/piekarz-CID4-ID2.html", "Gdańsk", 5000)).unwrap());
        assert_ne!(gdansk, first);
        let n = note(&conn, gdansk).unwrap().unwrap();
        assert_eq!((n.was.id, n.why.as_str()), (first, "city"));
        // Fetched again - the same separate offer, no third one.
        let again = add(&mut conn, offer("olx", "https://www.olx.pl/oferta/praca/piekarz-CID4-ID2.html?utm_source=mail", "Gdańsk", 5000)).unwrap();
        assert_eq!(id_of(again), gdansk);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM offers WHERE parent_id IS NULL"), 2);
        // Clearly other salary in the same city: asks too.
        let rich = id_of(add(&mut conn, offer("nfj", "https://nofluffjobs.com/pl/job/piekarz-1", "Kraków", 12000)).unwrap());
        assert_eq!(note(&conn, rich).unwrap().unwrap().why, "salary");
        // "Separate offer": the question goes. "Join": a child of the first added.
        assert_eq!(answer(&mut conn, rich, false).unwrap(), None);
        assert!(note(&conn, rich).unwrap().is_none());
        assert_eq!(answer(&mut conn, gdansk, true).unwrap(), Some(first));
        let parent: Option<i64> = conn.query_row("SELECT parent_id FROM offers WHERE id = ?1", [gdansk], |r| r.get(0)).unwrap();
        assert_eq!(parent, Some(first));
    }

    #[test]
    fn placed_by_the_person() {
        let mut conn = test_db();
        let first = id_of(add(&mut conn, offer("pracuj", "https://www.pracuj.pl/praca/piekarz,oferta,1001", "Kraków", 5000)).unwrap());
        // The same city, but the person says it is another offer.
        let own = add_placed(&mut conn, offer("manual", "https://www.olx.pl/oferta/praca/piekarz-CID4-ID9.html", "Kraków", 5000), Placement::Separate).unwrap();
        let own = id_of(own);
        assert_ne!(own, first);
        assert!(note(&conn, own).unwrap().is_none(), "the person decided - nothing to ask");
        // Another title and a hidden company, put under an offer by the person.
        let hidden = OfferInput {
            title: "Piekarz / Piekarka".into(),
            company: "Klient portalu Praca.pl".into(),
            ..offer("manual", "https://www.praca.pl/piekarz_5.html", "Kraków", 5000)
        };
        assert_eq!(add_placed(&mut conn, hidden, Placement::Under(first)).unwrap(), AddOutcome::Merged { id: first });
        let kids = count(&conn, &format!("SELECT COUNT(*) FROM offers WHERE parent_id = {first}"));
        assert_eq!(kids, 1);
        // Under a child - refused (one level).
        let child: i64 = conn.query_row("SELECT id FROM offers WHERE parent_id = ?1", [first], |r| r.get(0)).unwrap();
        assert!(add_placed(&mut conn, offer("manual", "https://x.example/1", "Kraków", 5000), Placement::Under(child)).is_err());
    }

    #[test]
    fn probably_the_same_under_another_title_or_a_hidden_company() {
        let mut conn = test_db();
        let first = id_of(add(&mut conn, offer("pracuj", "https://www.pracuj.pl/praca/piekarz,oferta,1001", "Kraków", 5000)).unwrap());
        add(&mut conn, OfferInput { title: "Księgowa".into(), company: "Biuro Inne".into(), ..offer("pracuj", "https://www.pracuj.pl/praca/k,oferta,2", "Kraków", 5000) }).unwrap();
        let hidden = Ad {
            title: "Piekarz / Piekarka".into(),
            company: "Klient portalu Praca.pl".into(),
            city: Some("Kraków".into()),
            salary_min: Some(5000),
            salary_max: Some(6000),
            salary_currency: Some("PLN".into()),
            salary_period: Some(SalaryPeriod::Month),
            ..Ad::empty()
        };
        let found = candidates(&conn, &hidden, &[]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].was.id, first);
        assert!(found[0].reasons.contains(&"hidden".to_string()) && found[0].reasons.contains(&"salary".to_string()));
        // A hidden company in another city with other pay - nothing.
        let far = Ad { city: Some("Gdańsk".into()), salary_min: Some(9000), salary_max: Some(9500), ..hidden.clone() };
        assert!(candidates(&conn, &far, &[]).unwrap().is_empty());
        // The same company under a close title.
        let close = Ad { title: "Piekarz nocny".into(), company: "Sieć Cukierni Przykładowa".into(), ..hidden };
        assert_eq!(candidates(&conn, &close, &[]).unwrap()[0].was.id, first);
        assert!(candidates(&conn, &close, &[first]).unwrap().is_empty(), "skipped");
    }
}
