//! Offers: adding (with duplicate merging), board lists, stage, interest, notes, trash.
//! Every change runs in a transaction - it is saved completely or not at all.

use rusqlite::{Connection, OptionalExtension, Row, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::clock;
use crate::dedup;
use crate::profiles;
use crate::error::{AppError, AppResult};
use crate::model::*;

pub(crate) const MAX_SHORT: usize = 300;
pub(crate) const MAX_URL: usize = 2_000;
pub(crate) const MAX_LONG: usize = 100_000;
pub(crate) const MAX_NOTES: usize = 20_000;
const MAX_SALARY: i64 = 10_000_000;

// ---------- validation of data coming from outside ----------

fn text(value: Option<String>, max: usize, field: &str) -> AppResult<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max {
        return Err(AppError::Invalid(format!("{field} is too long (max {max} characters)")));
    }
    // Control characters other than line breaks and tabs are never legitimate here.
    if trimmed.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t') {
        return Err(AppError::Invalid(format!("{field} contains control characters")));
    }
    Ok(Some(trimmed.to_string()))
}

fn required(value: String, max: usize, field: &str) -> AppResult<String> {
    text(Some(value), max, field)?.ok_or_else(|| AppError::Invalid(format!("{field} is required")))
}

/// Only plain web links are stored - never `javascript:`, `file:` or other schemes.
pub fn safe_url(value: Option<String>) -> AppResult<Option<String>> {
    let Some(url) = text(value, MAX_URL, "url")? else { return Ok(None) };
    let lower = url.to_ascii_lowercase();
    let ok_scheme = lower.starts_with("https://") || lower.starts_with("http://");
    if !ok_scheme || url.chars().any(char::is_whitespace) || url.len() <= "https://".len() {
        return Err(AppError::Invalid("the link must start with http:// or https://".into()));
    }
    Ok(Some(url))
}

fn source_name(value: &str) -> AppResult<String> {
    let ok = !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if ok { Ok(value.to_string()) } else { Err(AppError::Invalid("unknown source".into())) }
}

fn currency(value: Option<String>) -> AppResult<Option<String>> {
    let Some(code) = text(value, 3, "currency")? else { return Ok(None) };
    let code = code.to_ascii_uppercase();
    if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(Some(code))
    } else {
        Err(AppError::Invalid("currency must be a 3-letter code, e.g. PLN".into()))
    }
}

fn salary(value: Option<i64>) -> AppResult<Option<i64>> {
    match value {
        Some(v) if !(0..=MAX_SALARY).contains(&v) => Err(AppError::Invalid("salary is out of range".into())),
        other => Ok(other),
    }
}

/// Dates from outside are stored in UTC so they compare correctly as text.
fn utc_time(value: Option<String>) -> AppResult<Option<String>> {
    let Some(value) = text(value, 64, "date")? else { return Ok(None) };
    let parsed = OffsetDateTime::parse(&value, &Rfc3339)
        .map_err(|_| AppError::Invalid("date has a wrong format".into()))?;
    // Checked: a date at the very end of the calendar with an offset falls off it.
    parsed
        .checked_to_offset(time::UtcOffset::UTC)
        .and_then(|t| t.format(&Rfc3339).ok())
        .map(Some)
        .ok_or_else(|| AppError::Invalid("date has a wrong format".into()))
}

/// The date a source published the offer, in any of the forms the sources use: RFC 3339,
/// RFC 2822 (feeds), a date and time without a zone (taken as UTC), a bare date or Unix seconds.
/// A wrong date is dropped (the offer is not), a date in the future becomes `now`.
pub fn published(value: Option<&str>, now: &str) -> Option<String> {
    use time::format_description::well_known::Rfc2822;
    use time::macros::format_description;
    let v = value?.trim();
    if v.is_empty() || v.len() > 64 {
        return None;
    }
    let naive = |s: &str| {
        let s = s.split('.').next().unwrap_or(s).replace(' ', "T");
        let with_time = time::PrimitiveDateTime::parse(&s, format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]")).ok();
        with_time
            .or_else(|| time::Date::parse(&s, format_description!("[year]-[month]-[day]")).ok().map(|d| d.midnight()))
            .map(|t| t.assume_utc())
    };
    let parsed = OffsetDateTime::parse(v, &Rfc3339)
        .or_else(|_| OffsetDateTime::parse(v, &Rfc2822))
        .ok()
        .or_else(|| naive(v))
        .or_else(|| v.parse::<i64>().ok().and_then(|n| OffsetDateTime::from_unix_timestamp(n).ok()))?;
    let date = parsed.checked_to_offset(time::UtcOffset::UTC)?.replace_nanosecond(0).ok()?.format(&Rfc3339).ok()?;
    // Before 2000 is no job offer anyone looks for - a broken date.
    if date.as_str() < "2000" {
        return None;
    }
    Some(if date.as_str() > now { now.to_string() } else { date })
}

/// Validated and trimmed copy of the input.
pub(crate) fn clean(input: OfferInput) -> AppResult<OfferInput> {
    let mut salary_min = salary(input.salary_min)?;
    let mut salary_max = salary(input.salary_max)?;
    if let (Some(a), Some(b)) = (salary_min, salary_max) {
        if a > b {
            std::mem::swap(&mut salary_min, &mut salary_max);
        }
    }
    let has_salary = salary_min.is_some() || salary_max.is_some();
    Ok(OfferInput {
        title: required(input.title, MAX_SHORT, "title")?,
        company: required(input.company, MAX_SHORT, "company")?,
        source: source_name(&input.source)?,
        url: safe_url(input.url)?,
        city: text(input.city, MAX_SHORT, "city")?,
        remote: input.remote,
        salary_min,
        salary_max,
        salary_currency: if has_salary { currency(input.salary_currency)?.or(Some("PLN".into())) } else { None },
        salary_period: if has_salary { input.salary_period.or(Some(SalaryPeriod::Month)) } else { None },
        contract: text(input.contract, MAX_SHORT, "contract")?,
        level: text(input.level, MAX_SHORT, "level")?,
        language: text(input.language, 16, "language")?,
        requirements: text(input.requirements, MAX_LONG, "requirements")?,
        description: text(input.description, MAX_LONG, "description")?,
        expires_at: utc_time(input.expires_at)?,
        published_at: published(input.published_at.as_deref(), &clock::now()),
    })
}

/// An offer row from the sync file (written by the other device) - checked with the same rules
/// as data from outside before it is saved. A row that fails is skipped; the local copy stays.
pub fn check_synced(data: &serde_json::Value) -> AppResult<()> {
    use serde_json::Value;
    fn word(v: &Value, key: &str) -> AppResult<Option<String>> {
        match v.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(AppError::Invalid(format!("{key} has a wrong type"))),
        }
    }
    fn number(v: &Value, key: &str) -> AppResult<Option<i64>> {
        match v.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(n) => n.as_i64().map(Some).ok_or_else(|| AppError::Invalid(format!("{key} has a wrong type"))),
        }
    }
    fn one_of<T>(value: Option<String>, parse: fn(&str) -> Option<T>, key: &str) -> AppResult<()> {
        match value {
            Some(w) if parse(&w).is_none() => Err(AppError::Invalid(format!("{key} has an unknown value"))),
            _ => Ok(()),
        }
    }
    let d = data;
    required(word(d, "title")?.unwrap_or_default(), MAX_SHORT, "title")?;
    required(word(d, "company")?.unwrap_or_default(), MAX_SHORT, "company")?;
    required(word(d, "fingerprint")?.unwrap_or_default(), MAX_SHORT * 2 + 16, "fingerprint")?;
    safe_url(word(d, "url")?)?;
    for key in ["city", "contract", "level"] {
        text(word(d, key)?, MAX_SHORT, key)?;
    }
    text(word(d, "language")?, 16, "language")?;
    for key in ["requirements", "description", "ai_summary", "ai_flags", "ai_remote_quote", "ai_title_tr", "ai_description_tr"] {
        text(word(d, key)?, MAX_LONG, key)?;
    }
    text(word(d, "notes")?, MAX_NOTES, "notes")?;
    for key in ["ai_status", "ai_tr_lang"] {
        text(word(d, key)?, 32, key)?;
    }
    for key in ["parent_uid", "dup_of_uid"] {
        text(word(d, key)?, 64, key)?;
    }
    if !matches!(word(d, "dup_why")?.as_deref(), None | Some("city") | Some("salary")) {
        return Err(AppError::Invalid("dup_why has an unknown value".into()));
    }
    if let Some(source) = word(d, "content_removed")? {
        source_name(&source)?;
    }
    salary(number(d, "salary_min")?)?;
    salary(number(d, "salary_max")?)?;
    currency(word(d, "salary_currency")?)?;
    one_of(word(d, "salary_period")?, SalaryPeriod::parse, "salary_period")?;
    one_of(word(d, "remote")?, Remote::parse, "remote")?;
    one_of(word(d, "interest")?, Interest::parse, "interest")?;
    Stage::parse(&word(d, "stage")?.unwrap_or_default()).ok_or_else(|| AppError::Invalid("stage has an unknown value".into()))?;
    for key in ["favorite", "expired", "expired_by_user", "renew_asked", "own", "ai_precise"] {
        if !matches!(number(d, key)?, None | Some(0) | Some(1)) {
            return Err(AppError::Invalid(format!("{key} must be 0 or 1")));
        }
    }
    for key in ["stage_changed_at", "added_at", "updated_at", "user_changed_at", "expires_at", "deleted_at", "ai_at", "published_at", "archived_at", "kept_at", "renewed_at"] {
        utc_time(word(d, key)?)?;
    }
    let list = |key: &str| d.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    for s in list("sources") {
        source_name(&word(&s, "source")?.unwrap_or_default())?;
        safe_url(word(&s, "url")?)?;
        text(word(&s, "city")?, MAX_SHORT, "city")?;
        utc_time(word(&s, "first_seen")?)?;
    }
    for h in list("history") {
        Stage::parse(&word(&h, "stage")?.unwrap_or_default()).ok_or_else(|| AppError::Invalid("stage has an unknown value".into()))?;
        utc_time(word(&h, "at")?)?;
    }
    for sc in list("ai_scores") {
        if !number(&sc, "score")?.is_some_and(|n| (0..=100).contains(&n)) {
            return Err(AppError::Invalid("score is out of range".into()));
        }
        text(word(&sc, "why")?, MAX_LONG, "why")?;
        text(word(&sc, "quote")?, MAX_LONG, "quote")?;
    }
    Ok(())
}

// ---------- adding ----------

/// Where a new offer goes: by the duplicate rules, or where the person said in "Add offer".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The rules decide (`dups`): the same job joins its first ad, the same company and title in
    /// another city or with other salary is saved separately with a question.
    Auto,
    /// "Separate offer": the person says it is another job - saved on its own, nothing asked.
    Separate,
    /// "Join as a child": under this offer (a top-level one), whatever the rules say.
    Under(i64),
}

pub fn add(conn: &mut Connection, input: OfferInput) -> AppResult<AddOutcome> {
    add_placed(conn, input, Placement::Auto)
}

pub fn add_placed(conn: &mut Connection, input: OfferInput, placement: Placement) -> AppResult<AddOutcome> {
    let input = clean(input)?;
    let fingerprint = dedup::fingerprint(&input.company, &input.title);
    let now = clock::now();
    let tx = conn.transaction()?;

    let removed: bool = tx
        .query_row("SELECT 1 FROM removed_offers WHERE fingerprint = ?1", [&fingerprint], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    if removed {
        return Ok(AddOutcome::Blocked);
    }

    if let Placement::Under(parent) = placement {
        let top: bool = tx
            .query_row("SELECT parent_id IS NULL AND deleted_at IS NULL FROM offers WHERE id = ?1", [parent], |r| r.get(0))
            .optional()?
            .unwrap_or(false);
        if !top {
            return Err(AppError::Invalid("a child goes only under an offer on the board".into()));
        }
        let place = match input.url.as_deref() {
            Some(url) => crate::tree::place(&tx, parent, url)?,
            None => crate::tree::Place::New,
        };
        match place {
            crate::tree::Place::Child(child) => {
                merge_into(&tx, child, &input, &now)?;
            }
            crate::tree::Place::Parent => {
                merge_into(&tx, parent, &input, &now)?;
                add_link(&tx, parent, &input, &now)?;
            }
            crate::tree::Place::New => {
                crate::tree::add_child(&tx, parent, &input, &now)?;
            }
        }
        if input.source == "manual" {
            tx.execute(
                "UPDATE offers SET own = 1, user_changed_at = ?2 WHERE id = ?1 AND COALESCE(own, 0) = 0",
                params![parent, now],
            )?;
        }
        tx.execute("UPDATE offers SET updated_at = ?2 WHERE id = ?1", params![parent, now])?;
        profiles::recheck_offer(&tx, parent)?;
        tx.commit()?;
        return Ok(AddOutcome::Merged { id: parent });
    }

    let existing: Option<(i64, bool)> = tx
        .query_row(
            "SELECT id, deleted_at IS NOT NULL FROM offers WHERE fingerprint = ?1",
            [&fingerprint],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    // A child or an offer saved separately is found by its link, not its fingerprint.
    let marked = match (existing, input.url.as_deref()) {
        (None, Some(url)) | (Some((_, false)), Some(url)) => crate::dups::marked_by_link(&tx, url)?,
        _ => None,
    };
    if let Some((other, deleted)) = marked {
        let in_family = match existing {
            Some((id, _)) => tx.query_row("SELECT id = ?1 OR parent_id IS ?1 FROM offers WHERE id = ?2", params![id, other], |r| r.get(0))?,
            None => false,
        };
        if !in_family {
            if deleted {
                return Ok(AddOutcome::Blocked);
            }
            let filled = merge_into(&tx, other, &input, &now)?;
            if filled {
                tx.execute("UPDATE offers SET updated_at = ?2 WHERE id = ?1", params![other, now])?;
                profiles::recheck_offer(&tx, other)?;
            }
            tx.commit()?;
            return Ok(if filled { AddOutcome::Merged { id: other } } else { AddOutcome::Unchanged { id: other } });
        }
    }

    if existing.is_none() && in_trash_by_link(&tx, &input)? {
        return Ok(AddOutcome::Blocked);
    }
    let outcome = match existing {
        // In the trash: only the user brings it back.
        Some((_, true)) => return Ok(AddOutcome::Blocked),
        // "Separate offer": the person's word - on its own, under a marked fingerprint.
        Some((_, false)) if placement == Placement::Separate => {
            let marked = format!("{fingerprint}#{}", crate::secrets::random_hex(8)?);
            AddOutcome::Added { id: insert_new(&tx, &input, &marked, &now, None)? }
        }
        Some((id, false)) => {
            // Another ad of the same job (a link the family does not know): a child under the
            // first one; the same link of a child: that child.
            let place = match input.url.as_deref() {
                Some(url) => crate::tree::place(&tx, id, url)?,
                None => crate::tree::Place::Parent,
            };
            if let crate::tree::Place::Child(child) = place {
                merge_into(&tx, child, &input, &now)?;
                tx.commit()?;
                return Ok(AddOutcome::Unchanged { id });
            }
            if place == crate::tree::Place::New {
                // The same company and title: one job only in the same place with salaries that
                // do not contradict each other (`dups`) - else a separate offer and a question.
                let family = crate::dups::family(&tx, &fingerprint)?;
                let parent = match crate::dups::first_fit(&tx, &crate::dups::Ad::of(&input), &family)? {
                    Ok(parent) => parent,
                    Err(misfit) => {
                        let marked = format!("{fingerprint}#{}", crate::secrets::random_hex(8)?);
                        let dup = misfit.map(|(other, fit)| (other, fit.why()));
                        let new = insert_new(&tx, &input, &marked, &now, dup)?;
                        add_link(&tx, new, &input, &now)?;
                        profiles::recheck_offer(&tx, new)?;
                        tx.commit()?;
                        return Ok(AddOutcome::Added { id: new });
                    }
                };
                merge_into(&tx, parent, &input, &now)?;
                if input.source == "manual" {
                    tx.execute(
                        "UPDATE offers SET own = 1, user_changed_at = ?2 WHERE id = ?1 AND COALESCE(own, 0) = 0",
                        params![parent, now],
                    )?;
                }
                crate::tree::add_child(&tx, parent, &input, &now)?;
                tx.execute("UPDATE offers SET updated_at = ?2 WHERE id = ?1", params![parent, now])?;
                profiles::recheck_offer(&tx, parent)?;
                tx.commit()?;
                return Ok(AddOutcome::Merged { id: parent });
            }
            let filled = merge_into(&tx, id, &input, &now)?;
            // A collected offer added by hand now: the person chose it - it becomes their own.
            if input.source == "manual" {
                tx.execute(
                    "UPDATE offers SET own = 1, user_changed_at = ?2 WHERE id = ?1 AND COALESCE(own, 0) = 0",
                    params![id, now],
                )?;
            }
            // The source whose text was removed brings the job again (open once more): its link
            // comes back and the offer is checked against that source's terms again.
            let revived = tx.execute(
                "UPDATE offers SET content_removed = NULL, expired = expired * expired_by_user
                 WHERE id = ?1 AND content_removed = ?2",
                params![id, input.source],
            )? > 0;
            let new_source = add_link(&tx, id, &input, &now)?;
            if !filled && !new_source && !revived {
                // Fetched again with nothing new: the row stays exactly as it was (no sync, no recheck).
                tx.commit()?;
                return Ok(AddOutcome::Unchanged { id });
            }
            tx.execute("UPDATE offers SET updated_at = ?2 WHERE id = ?1", params![id, now])?;
            AddOutcome::Merged { id }
        }
        None => AddOutcome::Added { id: insert_new(&tx, &input, &fingerprint, &now, None)? },
    };

    let id = match outcome {
        AddOutcome::Added { id } | AddOutcome::Merged { id } | AddOutcome::Unchanged { id } => id,
        AddOutcome::Blocked => unreachable!(),
    };
    add_link(&tx, id, &input, &now)?;
    // New or merged data - check it against every profile before it shows up anywhere.
    profiles::recheck_offer(&tx, id)?;
    tx.commit()?;
    Ok(outcome)
}

/// A new top-level offer. `dup` - the offer of the same company and title it may be, and why it
/// was not joined ("city", "salary"): the question on the card.
fn insert_new(tx: &Connection, input: &OfferInput, fingerprint: &str, now: &str, dup: Option<(i64, &str)>) -> AppResult<i64> {
    let uid = crate::secrets::random_hex(16)?;
    // Added by hand = chosen by the person: "interested" (the CV goes later, they set
    // "applied" themselves). Collected offers start as new.
    let first_stage = if input.source == "manual" { Stage::Interested } else { Stage::New };
    tx.execute(
        "INSERT INTO offers (uid, fingerprint, title, company, url, city, remote,
            salary_min, salary_max, salary_currency, salary_period, contract, level, language,
            requirements, description, expires_at, stage, stage_changed_at, added_at, updated_at, published_at,
            user_changed_at, own, dup_of, dup_why)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
            ?21, ?18, ?18, ?18, COALESCE(?19, ?18), ?20, ?22, ?23, ?24)",
        params![
            uid, fingerprint, input.title, input.company, input.url, input.city,
            input.remote.unwrap_or(Remote::Unknown), input.salary_min, input.salary_max,
            input.salary_currency, input.salary_period, input.contract, input.level,
            input.language, input.requirements, input.description, input.expires_at, now,
            input.published_at,
            // Added by hand: the person's own act. Collected: no user change yet - the
            // same offer worked on the other device in the meantime keeps that work (sync).
            if input.source == "manual" { now } else { NOT_BY_USER },
            first_stage,
            // Added by hand = the person's own offer ("Own" tab); collected ones are not.
            input.source == "manual",
            dup.map(|d| d.0),
            dup.map(|d| d.1),
        ],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO stage_history (offer_id, stage, at) VALUES (?1, ?2, ?3)",
        params![id, first_stage, now],
    )?;
    Ok(id)
}

/// Remembers where the offer was seen. Another address of the same source is one more link (the
/// same job in another city); the same address again - also with other tracking parameters -
/// is not, and a link of that source without an address gets this one. Returns whether anything
/// was added.
pub(crate) fn add_link(tx: &Connection, id: i64, input: &OfferInput, now: &str) -> AppResult<bool> {
    if let Some(url) = input.url.as_deref() {
        let key = crate::sources::alerts::link_key(url);
        let mut stmt = tx.prepare("SELECT url FROM offer_sources WHERE offer_id = ?1 AND source = ?2 AND url IS NOT NULL")?;
        let known = stmt.query_map(params![id, input.source], |r| r.get::<_, String>(0))?;
        for known in known {
            if crate::sources::alerts::link_key(&known?) == key {
                return Ok(false);
            }
        }
        let filled = tx.execute(
            "UPDATE offer_sources SET url = ?3, city = COALESCE(city, ?4)
             WHERE rowid = (SELECT rowid FROM offer_sources WHERE offer_id = ?1 AND source = ?2 AND url IS NULL LIMIT 1)",
            params![id, input.source, url, input.city],
        )?;
        if filled > 0 {
            return Ok(true);
        }
    }
    Ok(tx.execute(
        "INSERT OR IGNORE INTO offer_sources (offer_id, source, url, first_seen, city) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, input.source, input.url, now, input.city],
    )? > 0)
}

/// Takes the links of one source off an offer (its job closed - the source's terms), keeping one
/// row: the offer was still seen there.
pub fn drop_links(tx: &rusqlite::Transaction, id: i64, source: &str) -> AppResult<()> {
    tx.execute(
        "DELETE FROM offer_sources WHERE offer_id = ?1 AND source = ?2
         AND rowid != (SELECT min(rowid) FROM offer_sources WHERE offer_id = ?1 AND source = ?2)",
        params![id, source],
    )?;
    tx.execute("UPDATE offer_sources SET url = NULL WHERE offer_id = ?1 AND source = ?2", params![id, source])?;
    Ok(())
}

/// The cities of one job seen in several places, as one list ("Kraków, Gdańsk") without repeats.
/// "Remote" or "Poland" is no city to add; a longer form of a city already there ("Gdańsk,
/// pomorskie") adds nothing either.
pub fn join_cities(current: Option<&str>, new: Option<&str>) -> Option<String> {
    let current = current.map(str::trim).filter(|c| !c.is_empty());
    let Some(new) = new.map(str::trim).filter(|c| !c.is_empty()) else { return current.map(str::to_string) };
    let Some(current) = current else { return Some(new.to_string()) };
    let (have, add) = (dedup::normalize(current), dedup::normalize(new));
    let inside = |small: &str, big: &str| format!(" {big} ").contains(&format!(" {small} "));
    if add.is_empty() || crate::filter::NOT_A_PLACE.contains(&add.as_str()) || inside(&add, &have) {
        return Some(current.to_string());
    }
    // "Gdańsk, pomorskie" after "Kraków, Gdańsk": a city of the list, written longer.
    if current.split(',').map(dedup::normalize).any(|c| !c.is_empty() && inside(&c, &add)) {
        return Some(current.to_string());
    }
    let joined = format!("{current}, {new}");
    Some(if joined.chars().count() > MAX_SHORT { current.to_string() } else { joined })
}

/// A portal's offer that is in the trash under another company or title (a mail read better
/// later, a placeholder "? (…)" company): the same link of the same source keeps it deleted.
fn in_trash_by_link(tx: &rusqlite::Transaction, input: &OfferInput) -> AppResult<bool> {
    let Some(key) = input.url.as_deref().and_then(crate::sources::alerts::same_offer_key) else { return Ok(false) };
    let mut stmt = tx.prepare(
        "SELECT s.url FROM offer_sources s JOIN offers o ON o.id = s.offer_id
         WHERE o.deleted_at IS NOT NULL AND s.source = ?1 AND s.url IS NOT NULL",
    )?;
    let urls = stmt.query_map([&input.source], |r| r.get::<_, String>(0))?;
    for url in urls {
        if crate::sources::alerts::same_offer_key(&url?).as_deref() == Some(key.as_str()) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Columns `merge_into` may change - compared before and after to tell whether anything was filled in.
const MERGED_COLUMNS: &str = "SELECT url, city, remote, contract, level, language, requirements, description,
    expires_at, expired, salary_min, salary_max, salary_currency, salary_period, published_at, archived_at
    FROM offers WHERE id = ?1";

fn merged_columns(tx: &Connection, id: i64) -> AppResult<Vec<rusqlite::types::Value>> {
    Ok(tx.query_row(MERGED_COLUMNS, [id], |r| (0..16).map(|i| r.get(i)).collect())?)
}

/// Same offer seen again: fill in what was missing, keep the longer texts. A newer publication
/// date (a source brings it again) starts its age anew and takes it out of the archive.
/// Returns whether anything changed (the caller sets `updated_at` only then).
fn merge_into(tx: &Connection, id: i64, input: &OfferInput, now: &str) -> AppResult<bool> {
    let before = merged_columns(tx, id)?;
    let unglued = unglue_jooble(tx, id, input)?;
    let newer_date = tx.execute(
        "UPDATE offers SET published_at = ?2, archived_at = NULL
         WHERE id = ?1 AND ?2 > COALESCE(published_at, added_at)",
        params![id, input.published_at.as_deref().unwrap_or(now)],
    )? > 0;
    // The same job in another city: the city joins the list (the filter takes any of them).
    let city: Option<String> = tx.query_row("SELECT city FROM offers WHERE id = ?1", [id], |r| r.get(0))?;
    let city = join_cities(city.as_deref(), input.city.as_deref());
    let has_salary: bool = tx.query_row(
        "SELECT salary_min IS NOT NULL OR salary_max IS NOT NULL FROM offers WHERE id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if !has_salary && (input.salary_min.is_some() || input.salary_max.is_some()) {
        tx.execute(
            "UPDATE offers SET salary_min = ?2, salary_max = ?3, salary_currency = ?4, salary_period = ?5
             WHERE id = ?1",
            params![id, input.salary_min, input.salary_max, input.salary_currency, input.salary_period],
        )?;
    }
    tx.execute(
        "UPDATE offers SET
            url = COALESCE(url, ?2),
            city = ?3,
            remote = CASE WHEN remote = 'unknown' AND ?4 IS NOT NULL THEN ?4 ELSE remote END,
            contract = COALESCE(contract, ?5),
            level = COALESCE(level, ?6),
            language = COALESCE(language, ?7),
            requirements = CASE WHEN length(COALESCE(?8, '')) > length(COALESCE(requirements, ''))
                THEN ?8 ELSE requirements END,
            description = CASE WHEN length(COALESCE(?9, '')) > length(COALESCE(description, ''))
                THEN ?9 ELSE description END,
            expires_at = CASE WHEN expires_at IS NULL OR ?10 > expires_at THEN COALESCE(?10, expires_at)
                ELSE expires_at END,
            -- Only a newer date takes off the automatic expiry; the one set by the user stays.
            expired = CASE WHEN expired_by_user = 0
                AND (?11 OR (?10 IS NOT NULL AND (expires_at IS NULL OR ?10 > expires_at))) THEN 0 ELSE expired END
         WHERE id = ?1",
        params![
            id, input.url, city, input.remote, input.contract, input.level, input.language,
            input.requirements, input.description, input.expires_at, newer_date
        ],
    )?;
    let after = merged_columns(tx, id)?;
    // AI read a short alert ("50 or less" when facts are missing); now the full text or the salary
    // came - it reads the offer once more.
    let text_len = |cols: &[rusqlite::types::Value]| match &cols[7] {
        rusqlite::types::Value::Text(t) => t.chars().count(),
        _ => 0,
    };
    let salary_came = matches!(before[10], rusqlite::types::Value::Null) && matches!(before[11], rusqlite::types::Value::Null)
        && !(matches!(after[10], rusqlite::types::Value::Null) && matches!(after[11], rusqlite::types::Value::Null));
    if unglued || salary_came || (text_len(&after) >= 400 && text_len(&after) >= 2 * text_len(&before)) {
        tx.execute("UPDATE offers SET ai_status = NULL, ai_error = NULL WHERE id = ?1 AND ai_status IS NOT NULL", [id])?;
    }
    Ok(after != before)
}

/// A Jooble alert read before 11.11.2026 could glue the next card (`/redir/jdp` link) onto an offer:
/// its text starts with this offer's card and goes on with another company, salary and terms.
/// The same card read right in a later mail is that start alone - the offer takes the new reading
/// (text, requirements, salary, terms) instead of keeping the longer, glued one. Returns whether it did.
fn unglue_jooble(tx: &Connection, id: i64, input: &OfferInput) -> AppResult<bool> {
    let Some(new) = input.description.as_deref().filter(|d| !d.trim().is_empty()) else { return Ok(false) };
    if input.source != crate::sources::jooble::SOURCE {
        return Ok(false);
    }
    let old: Option<String> = tx.query_row("SELECT description FROM offers WHERE id = ?1", [id], |r| r.get(0))?;
    let glued = old.as_deref().and_then(|o| o.strip_prefix(new)).is_some_and(|rest| rest.starts_with('\n') && !rest.trim().is_empty());
    if !glued {
        return Ok(false);
    }
    tx.execute(
        "UPDATE offers SET description = ?2, requirements = ?3, salary_min = ?4, salary_max = ?5,
            salary_currency = ?6, salary_period = ?7, contract = ?8, level = ?9, remote = ?10
         WHERE id = ?1",
        params![
            id, new, input.requirements, input.salary_min, input.salary_max, input.salary_currency,
            input.salary_period, input.contract, input.level, input.remote.unwrap_or(Remote::Unknown)
        ],
    )?;
    Ok(true)
}

/// Offers past their expiry date are marked "expired" (greyed out on the board), from every
/// source. Seen again with a newer date, an offer is open again (`merge_into`) until its date passes.
pub fn mark_expired(conn: &Connection, now: &str) -> AppResult<usize> {
    Ok(conn.execute(
        "UPDATE offers SET expired = 1 WHERE expired = 0 AND expires_at IS NOT NULL AND expires_at < ?1",
        [now],
    )?)
}

// ---------- freshness ----------

/// When the offer counts as published: the source's date (newer when seen again), else when added.
const FRESH_AT: &str = "COALESCE(o.published_at, o.added_at)";

/// Added by hand - kept and always at the top of "Today".
pub(crate) const MANUAL: &str = "EXISTS (SELECT 1 FROM offer_sources m WHERE m.offer_id = o.id AND m.source = 'manual')";

/// No link to the offer anywhere (added by hand from a pasted text) - below the ones with a link.
const NO_LINK: &str = "(o.url IS NULL AND NOT EXISTS (SELECT 1 FROM offer_sources s WHERE s.offer_id = o.id AND s.url IS NOT NULL))";

/// Applied and further: never archived, greyed out or pushed down.
const IN_RECRUITMENT: &str = "(o.stage NOT IN ('new', 'interested'))";

/// The user did something with the offer - it never leaves for the archive by itself.
/// `user_changed_at` of a collected offer nobody worked on yet: older than any real change.
pub const NOT_BY_USER: &str = "1970-01-01T00:00:00Z";

pub(crate) const TOUCHED: &str = "(o.stage != 'new' OR o.favorite = 1 OR o.interest IS NOT NULL OR o.notes != ''
    OR o.kept_at IS NOT NULL
    OR EXISTS (SELECT 1 FROM offer_sources m WHERE m.offer_id = o.id AND m.source = 'manual')
    OR EXISTS (SELECT 1 FROM events e WHERE e.offer_id = o.id)
    OR EXISTS (SELECT 1 FROM contacts c WHERE c.offer_id = o.id)
    OR EXISTS (SELECT 1 FROM contact_log l WHERE l.offer_id = o.id))";

/// Offers waiting for AI (table alias `o`): fresh ones, or older ones the user works on;
/// never the trash, the archive, rejected by the filter or "not for me".
pub fn waiting_for_ai(conn: &Connection) -> AppResult<String> {
    let old = cutoff_now(conn)?;
    Ok(format!(
        "o.ai_status IS NULL AND o.parent_id IS NULL AND o.deleted_at IS NULL AND o.archived_at IS NULL AND o.filter_result != 'fail'
         AND COALESCE(o.interest, '') != 'no' AND ({FRESH_AT} >= '{old}' OR {TOUCHED})"
    ))
}

/// The order AI reads offers in: the ones the user works on, then the newest.
pub const AI_ORDER: &str = "(o.stage != 'new' OR o.favorite = 1 OR o.interest IS NOT NULL) DESC,
    COALESCE(o.published_at, o.added_at) DESC, o.id DESC";

/// Days after publication an offer stays fresh (the "fresh_days" setting).
pub fn fresh_days(conn: &Connection) -> AppResult<i64> {
    Ok(crate::settings::get(conn, "fresh_days")?.parse().unwrap_or(10))
}

/// Offers published before this are old. Always our own RFC 3339 text - safe to put into SQL.
pub fn cutoff(now: &str, days: i64) -> String {
    let now = OffsetDateTime::parse(now, &Rfc3339).unwrap_or_else(|_| OffsetDateTime::now_utc());
    (now - time::Duration::days(days))
        .replace_nanosecond(0)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .unwrap_or_default()
}

fn cutoff_now(conn: &Connection) -> AppResult<String> {
    Ok(cutoff(&clock::now(), fresh_days(conn)?))
}

/// Offers nobody touched leave for the archive some days after they were published.
pub fn archive_old(conn: &Connection, now: &str) -> AppResult<usize> {
    let old = cutoff(now, fresh_days(conn)?);
    Ok(conn.execute(
        &format!(
            "UPDATE offers AS o SET archived_at = ?1, updated_at = ?1
             WHERE o.deleted_at IS NULL AND o.archived_at IS NULL AND o.parent_id IS NULL AND NOT {TOUCHED}
                AND {FRESH_AT} < '{old}'"
        ),
        [now],
    )?)
}

/// What "Expired" on the card did - the interface says so and can undo it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExpiredBy {
    /// Nobody touched it - gone to the archive.
    Archived,
    /// The user worked on it - stays, marked "expired".
    Marked,
}

/// "Expired" on the card: an offer nobody touched goes to the archive, any other one is marked.
pub fn set_expired(conn: &Connection, id: i64) -> AppResult<ExpiredBy> {
    let now = clock::now();
    let touched: bool = conn
        .query_row(&format!("SELECT {TOUCHED} FROM offers o WHERE o.id = ?1"), [id], |r| r.get(0))
        .optional()?
        .ok_or(AppError::NotFound)?;
    if touched {
        conn.execute(
            "UPDATE offers SET expired = 1, expired_by_user = 1, user_changed_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(ExpiredBy::Marked)
    } else {
        conn.execute("UPDATE offers SET archived_at = ?2, user_changed_at = ?2 WHERE id = ?1", params![id, now])?;
        Ok(ExpiredBy::Archived)
    }
}

/// "Undo" after "Expired": the offer is back as it was.
pub fn unset_expired(conn: &Connection, id: i64) -> AppResult<()> {
    changed(conn.execute(
        "UPDATE offers SET expired = 0, expired_by_user = 0, archived_at = NULL, user_changed_at = ?2 WHERE id = ?1",
        params![id, clock::now()],
    )?)
}

/// "Bring back" in the archive: the user wants it, so it never leaves by itself again.
pub fn unarchive(conn: &Connection, id: i64) -> AppResult<()> {
    let now = clock::now();
    changed(conn.execute(
        "UPDATE offers SET archived_at = NULL, kept_at = ?2, user_changed_at = ?2 WHERE id = ?1 AND archived_at IS NOT NULL",
        params![id, now],
    )?)
}

// ---------- reading ----------

/// Columns of a board row. `{result}` and `{reasons}` depend on the chosen profile (see `filter_sql`),
/// `{old}` is the cutoff of fresh offers.
const ROW_COLUMNS: &str = "o.id, o.title, o.company, o.city, o.remote, o.salary_min, o.salary_max,
    o.salary_currency, o.salary_period, o.stage, o.stage_changed_at, o.interest, o.favorite,
    o.score, o.score_source,
    (SELECT group_concat(v, char(31)) FROM (SELECT DISTINCT CASE WHEN s.source = 'manual'
        THEN 'manual ' || COALESCE(s.url, o.url, '') ELSE s.source END AS v FROM offer_sources s WHERE s.offer_id = o.id)),
    o.expired, {result}, o.added_at, {reasons}, {next}, o.ai_tr_lang, o.ai_title_tr,
    {fresh}, ({fresh} < '{old}' AND NOT {recruit}), {recruit}, {manual}, o.archived_at,
    (SELECT sc.why FROM offer_scores sc WHERE sc.offer_id = o.id AND sc.source = 'ai' ORDER BY sc.score DESC LIMIT 1),
    COALESCE((SELECT l.points FROM offer_learned l WHERE l.offer_id = o.id), 0),
    (SELECT l.words FROM offer_learned l WHERE l.offer_id = o.id), {nolink}, COALESCE(o.own, 0) = 1, {bin},
    COALESCE(o.renew_asked, 0) = 1, o.parent_id,
    (o.dup_of IS NOT NULL AND EXISTS (SELECT 1 FROM offers d WHERE d.id = o.dup_of AND d.deleted_at IS NULL AND d.parent_id IS NULL)),
    {sent}";

/// Number of columns in `ROW_COLUMNS` - the offer card reads its own after them.
const ROW_LEN: usize = 38;

/// When the CV went (the first stage past "interested" in the history; the stage's own date when
/// the history has none); NULL = not in a recruitment.
const SENT_AT: &str = "(CASE WHEN o.stage NOT IN ('new', 'interested') THEN COALESCE((SELECT min(h.at) FROM stage_history h
    WHERE h.offer_id = o.id AND h.stage NOT IN ('new', 'interested')), o.stage_changed_at) END)";

/// A child of the offer is in a recruitment (the CV went through another ad) - one recruitment
/// for the whole family: the parent is not "to send" any more.
pub(crate) const CHILD_SENT: &str = "EXISTS (SELECT 1 FROM offers c WHERE c.parent_id = o.id AND c.deleted_at IS NULL
    AND c.stage NOT IN ('new', 'interested'))";

/// The score as shown: with what the ratings taught (`learn`). NULL stays NULL.
const SHOWN_SCORE: &str = "MIN(100, MAX(0, o.score + COALESCE((SELECT l.points FROM offer_learned l WHERE l.offer_id = o.id), 0)))";

/// The nearest date of an offer from 3 hours ago on (an interview that just started still counts).
const NEXT_EVENT: &str = "(SELECT e.at || '|' || e.kind FROM events e WHERE e.offer_id = o.id
    AND e.at >= strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-3 hours') ORDER BY e.at LIMIT 1)";

/// The user already acted on these offers - they stay on the board whatever the filter says.
const KEPT: &str = "(o.stage != 'new' OR o.favorite = 1 OR COALESCE(o.interest, '') IN ('high', 'maybe'))";

/// The user said "no" - the offer leaves every other tab for "Not for me" (not the trash: it stays
/// in the database, so fetching it again only merges).
const DECLINED: &str = "(COALESCE(o.interest, '') = 'no')";

/// Neither in the trash nor in the archive.
const LIVE: &str = "o.deleted_at IS NULL AND o.archived_at IS NULL";

/// At the top of "Today" whatever their score: added by hand or starred.
const PINNED: &str = "(o.favorite = 1 OR EXISTS (SELECT 1 FROM offer_sources m WHERE m.offer_id = o.id AND m.source = 'manual'))";

/// Filter result and reasons: for one profile (`?1`), or the best over all profiles.
fn filter_sql(profile: Option<i64>) -> (&'static str, &'static str) {
    match profile {
        Some(_) => (
            "COALESCE((SELECT f.result FROM offer_filters f WHERE f.offer_id = o.id AND f.profile_id = ?1), 'pass')",
            "(SELECT f.reasons FROM offer_filters f WHERE f.offer_id = o.id AND f.profile_id = ?1)",
        ),
        None => (
            "o.filter_result",
            "(SELECT f.reasons FROM offer_filters f JOIN profiles p ON p.id = f.profile_id
              WHERE f.offer_id = o.id AND f.result = o.filter_result ORDER BY p.position, p.id LIMIT 1)",
        ),
    }
}

fn columns(profile: Option<i64>, old: &str) -> String {
    let (result, reasons) = filter_sql(profile);
    ROW_COLUMNS
        .replace("{result}", result)
        .replace("{reasons}", reasons)
        .replace("{next}", NEXT_EVENT)
        .replace("{fresh}", FRESH_AT)
        .replace("{old}", old)
        .replace("{recruit}", IN_RECRUITMENT)
        .replace("{manual}", MANUAL)
        .replace("{nolink}", NO_LINK)
        .replace("{bin}", BIN_KIND)
        .replace("{sent}", SENT_AT)
}

/// The name of a source as the lists show it: an ad added by hand with a link is named after the
/// portal of that link (Pracuj, NFJ, an unknown one by its address), "manual" only without a link.
/// The `manual` flag of a row still says it was added by hand.
fn shown_source(source: &str) -> String {
    source.strip_prefix("manual ").map_or_else(|| source.to_string(), portal_label)
}

/// Portal of a link added by hand; "manual" when the link is empty or not a web address.
fn portal_label(url: &str) -> String {
    crate::sources::alerts::portal_of_link(url).unwrap_or_else(|| "manual".into())
}

fn row_from(r: &Row) -> rusqlite::Result<OfferRow> {
    let sources: Option<String> = r.get(15)?;
    let next: Option<(String, String)> = r
        .get::<_, Option<String>>(20)?
        .and_then(|v| v.split_once('|').map(|(a, k)| (a.to_string(), k.to_string())));
    let mut sources: Vec<String> = sources
        .map(|s| s.split('\u{1f}').map(shown_source).collect())
        .unwrap_or_default();
    sources.sort();
    sources.dedup();
    Ok(OfferRow {
        id: r.get(0)?,
        title: r.get(1)?,
        company: r.get(2)?,
        city: r.get(3)?,
        remote: r.get(4)?,
        salary_min: r.get(5)?,
        salary_max: r.get(6)?,
        salary_currency: r.get(7)?,
        salary_period: r.get(8)?,
        stage: r.get(9)?,
        stage_changed_at: r.get(10)?,
        interest: r.get(11)?,
        favorite: r.get(12)?,
        score: r.get(13)?,
        score_source: r.get(14)?,
        sources,
        expired: r.get(16)?,
        filter_result: r.get(17)?,
        added_at: r.get(18)?,
        filter_reasons: profiles::reasons_from(r.get(19)?),
        next_event: next.as_ref().map(|(at, _)| at.clone()),
        next_event_kind: next.map(|(_, kind)| kind),
        translation_lang: r.get(21)?,
        title_translated: r.get(22)?,
        published_at: r.get(23)?,
        stale: r.get(24)?,
        in_recruitment: r.get(25)?,
        manual: r.get(26)?,
        archived_at: r.get(27)?,
        score_why: r.get(28)?,
        learned: r.get(29)?,
        learned_words: r
            .get::<_, Option<String>>(30)?
            .and_then(|w| serde_json::from_str(&w).ok())
            .unwrap_or_default(),
        no_link: r.get(31)?,
        own: r.get(32)?,
        bin: r.get(33)?,
        renew_asked: r.get(34)?,
        parent_id: r.get(35)?,
        dup_ask: r.get(36)?,
        sent_at: r.get(37)?,
        sent_via: None,
        children: Vec::new(),
    })
}

/// A top-level offer - children are shown under their parent, not by themselves.
const TOP: &str = "o.parent_id IS NULL";

fn view_condition(view: View, result: &str, old: &str) -> String {
    let condition = match view {
        // The person's own recruitments, whatever the filter says.
        View::Own => format!("{LIVE} AND {OWN} AND NOT {DECLINED}"),
        // Fresh offers worth a look today: not rejected by the filter, not in a recruitment yet.
        View::Today => format!(
            "{LIVE} AND NOT {DECLINED} AND ({result} != 'fail' OR {KEPT}) AND NOT {IN_RECRUITMENT}
             AND o.expired = 0 AND {FRESH_AT} >= '{old}'"
        ),
        // "All" in Collected: everything the filter let through or left to check - never fewer than "Best today".
        View::Board => format!("{LIVE} AND NOT {DECLINED} AND ({result} != 'fail' OR {KEPT})"),
        View::Review => format!("{LIVE} AND NOT {DECLINED} AND {result} = 'unsure' AND NOT {KEPT}"),
        View::Rejected => format!("{LIVE} AND NOT {DECLINED} AND {result} = 'fail' AND NOT {KEPT}"),
        View::Declined => format!("{LIVE} AND {DECLINED}"),
        View::Archive => "o.deleted_at IS NULL AND o.archived_at IS NOT NULL".to_string(),
        View::Trash => "o.deleted_at IS NOT NULL".to_string(),
        // "Not for me", expired and deleted in one list.
        View::Bin => format!("{BIN_KIND} IS NOT NULL"),
    };
    // A child deleted by itself is in the bin on its own; any other child goes with its parent.
    match view {
        View::Trash | View::Bin => format!("({condition}) AND ({TOP} OR o.deleted_at IS NOT NULL)"),
        _ => format!("({condition}) AND {TOP}"),
    }
}

/// Why the offer is in the bin (the order of the checks as in the tabs: deleted, then expired, then
/// "not for me"); NULL = not in the bin.
const BIN_KIND: &str = "(CASE WHEN o.deleted_at IS NOT NULL THEN 'deleted' WHEN o.archived_at IS NOT NULL THEN 'expired'
    WHEN COALESCE(o.interest, '') = 'no' THEN 'declined' END)";

/// When the offer went to the bin: deleted, archived, or marked "no" (the person's last change).
const BIN_AT: &str = "COALESCE(CASE WHEN o.deleted_at IS NOT NULL THEN o.deleted_at WHEN o.archived_at IS NOT NULL THEN o.archived_at
    ELSE o.user_changed_at END, '')";

/// Groups of "Own", top down: the CV still to be sent, in a recruitment (also through a child),
/// over (rejected, withdrawn).
fn own_group() -> String {
    format!("CASE WHEN o.stage IN ('new', 'interested') AND NOT {CHILD_SENT} THEN 0 WHEN o.stage IN ('rejected', 'withdrawn') THEN 2 ELSE 1 END")
}

/// The person's own offer (tab "Own", `023_own.sql`); 0 or NULL = collected.
pub(crate) const OWN: &str = "(COALESCE(o.own, 0) = 1)";

/// What a tab shows. "Today", "Offers", "To check" and "Rejected by the filter" are the switch
/// inside "Collected" (best today / all / to check / rejected) - own offers are not there.
fn tab_condition(view: View, result: &str, old: &str) -> String {
    let condition = view_condition(view, result, old);
    match view {
        View::Today | View::Board | View::Review | View::Rejected => format!("{condition} AND NOT {OWN}"),
        View::Own | View::Bin | View::Declined | View::Archive | View::Trash => condition,
    }
}

/// How many offers "Today" shows besides the ones added by hand or starred (the "today_count" setting).
fn today_count(conn: &Connection) -> AppResult<usize> {
    Ok(crate::settings::get(conn, "today_count")?.parse().unwrap_or(15))
}

/// Offers of one tab. `profile = None` - all profiles together (one fitting profile is enough).
/// "Today": added by hand and starred first, then the best scores (AI orders, never hides),
/// the newest first among equal ones - as many as the setting says, the pinned ones always.
pub fn list(conn: &Connection, view: View, profile: Option<i64>) -> AppResult<Vec<OfferRow>> {
    let (result, _) = filter_sql(profile);
    let old = cutoff_now(conn)?;
    let order = match view {
        View::Today => format!("{PINNED} DESC, {NO_LINK}, o.score IS NULL, {SHOWN_SCORE} DESC, {FRESH_AT} DESC, o.id DESC"),
        View::Own => format!("{}, {NO_LINK}, {FRESH_AT} DESC, o.id DESC", own_group()),
        // The last thrown in on top.
        View::Bin => format!("{BIN_AT} DESC, o.id DESC"),
        View::Archive => format!("o.archived_at DESC, {FRESH_AT} DESC, o.id DESC"),
        _ => format!("{FRESH_AT} DESC, o.id DESC"),
    };
    let sql = format!(
        "SELECT {}, {PINNED} FROM offers o WHERE {} ORDER BY {order}",
        columns(profile, &old),
        tab_condition(view, result, &old)
    );
    let mut stmt = conn.prepare(&sql)?;
    let read = |r: &Row| Ok((row_from(r)?, r.get::<_, bool>(ROW_LEN)?));
    let rows: Vec<(OfferRow, bool)> = match profile {
        Some(p) => stmt.query_map([p], read)?.collect::<Result<Vec<_>, _>>()?,
        None => stmt.query_map([], read)?.collect::<Result<Vec<_>, _>>()?,
    };
    let mut rows: Vec<OfferRow> = if view != View::Today {
        rows.into_iter().map(|(row, _)| row).collect()
    } else {
        let room = today_count(conn)?;
        let pinned = rows.iter().filter(|(_, p)| *p).count();
        rows.into_iter()
            .enumerate()
            .filter(|(i, (_, p))| *p || *i < room.max(pinned))
            .map(|(_, (row, _))| row)
            .collect()
    };
    with_children(conn, &mut rows, profile)?;
    Ok(rows)
}

/// The children of every row, the first seen on top (a child deleted by itself is in the bin).
fn with_children(conn: &Connection, rows: &mut [OfferRow], profile: Option<i64>) -> AppResult<()> {
    let parents: std::collections::HashSet<i64> = conn
        .prepare("SELECT DISTINCT parent_id FROM offers WHERE parent_id IS NOT NULL AND deleted_at IS NULL")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    if parents.is_empty() {
        return Ok(());
    }
    let sql = format!(
        "SELECT {} FROM offers o WHERE o.parent_id = ?2 AND o.deleted_at IS NULL ORDER BY o.added_at, o.id",
        columns(profile, &cutoff_now(conn)?)
    );
    let mut stmt = conn.prepare(&sql)?;
    for row in rows.iter_mut().filter(|r| parents.contains(&r.id)) {
        row.children = stmt.query_map(params![profile, row.id], row_from)?.collect::<Result<_, _>>()?;
        if row.children.iter().any(|c| c.sent_at.is_some()) {
            row.sent_via = crate::tree::sent_via(conn, row.id, true)?;
        }
    }
    Ok(())
}

/// Offers for the CSV export: one tab as the board shows it for all profiles (`Some`), or every
/// offer, the archive and the trash too (`None`). Newest first.
pub fn export_rows(conn: &Connection, view: Option<View>) -> AppResult<Vec<ExportRow>> {
    let (result, _) = filter_sql(None);
    let old = cutoff_now(conn)?;
    // The same order of checks as the tabs: trash, archive, "not for me", then the filter result.
    let list = format!(
        "CASE WHEN {} THEN 'trash' WHEN {} THEN 'archive' WHEN {} THEN 'declined' WHEN {OWN} THEN 'own'
              WHEN {} THEN 'review' WHEN {} THEN 'board' ELSE 'rejected' END",
        view_condition(View::Trash, result, &old),
        view_condition(View::Archive, result, &old),
        view_condition(View::Declined, result, &old),
        // "To check" first: "All" holds those too.
        view_condition(View::Review, result, &old),
        view_condition(View::Board, result, &old),
    );
    let filter = match view {
        Some(v) => tab_condition(v, result, &old),
        None => "1 = 1".into(),
    };
    let sql = format!(
        "SELECT o.id, o.title, o.company, o.city, o.remote, o.salary_min, o.salary_max, o.salary_currency, o.salary_period,
                o.contract, o.level, o.stage, o.interest, o.favorite, o.score, o.score_source,
                (SELECT group_concat(DISTINCT s.source) FROM offer_sources s WHERE s.offer_id = o.id),
                o.url, o.added_at, o.notes, {list}, {FRESH_AT}
         FROM offers o WHERE {filter} ORDER BY {FRESH_AT} DESC, o.id DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| {
            let sources: Option<String> = r.get(16)?;
            Ok(ExportRow {
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
                stage: r.get(11)?,
                interest: r.get(12)?,
                favorite: r.get(13)?,
                score: r.get(14)?,
                score_source: r.get(15)?,
                sources: sources.map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default(),
                url: r.get(17)?,
                added_at: r.get(18)?,
                notes: r.get(19)?,
                list: r.get(20)?,
                published_at: r.get(21)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Number of offers in every tab, for the tab labels.
pub fn counts(conn: &Connection, profile: Option<i64>) -> AppResult<ViewCounts> {
    let (result, _) = filter_sql(profile);
    let old = cutoff_now(conn)?;
    let count = |cond: String| format!("COALESCE(SUM(CASE WHEN {cond} THEN 1 ELSE 0 END), 0)");
    let to_send = format!("{} AND o.stage IN ('new', 'interested') AND NOT {CHILD_SENT}", view_condition(View::Own, result, &old));
    let sql = format!(
        "SELECT {}, {}, {}, {}, {}, {}, {}, {} FROM offers o",
        count(tab_condition(View::Board, result, &old)),
        count(tab_condition(View::Review, result, &old)),
        count(tab_condition(View::Rejected, result, &old)),
        count(view_condition(View::Declined, result, &old)),
        count(view_condition(View::Trash, result, &old)),
        count(to_send),
        count(view_condition(View::Archive, result, &old)),
        count(view_condition(View::Bin, result, &old)),
    );
    let read = |r: &Row| {
        Ok(ViewCounts {
            to_send: r.get(5)?,
            today: 0,
            board: r.get(0)?,
            review: r.get(1)?,
            rejected: r.get(2)?,
            declined: r.get(3)?,
            archive: r.get(6)?,
            trash: r.get(4)?,
            bin: r.get(7)?,
        })
    };
    let mut counts = match profile {
        Some(p) => conn.query_row(&sql, [p], read)?,
        None => conn.query_row(&sql, [], read)?,
    };
    counts.today = list(conn, View::Today, profile)?.len() as i64;
    Ok(counts)
}

pub fn detail(conn: &Connection, id: i64) -> AppResult<OfferDetail> {
    let sql = format!(
        "SELECT {}, o.url, o.contract, o.level, o.language, o.requirements, o.description,
            o.notes, o.expires_at, o.deleted_at, o.content_removed, o.renewed_at
         FROM offers o WHERE o.id = ?1",
        columns(None, &cutoff_now(conn)?)
    );
    let mut detail = conn
        .query_row(&sql, [id], |r| {
            Ok(OfferDetail {
                row: row_from(r)?,
                url: r.get(ROW_LEN)?,
                contract: r.get(ROW_LEN + 1)?,
                level: r.get(ROW_LEN + 2)?,
                language: r.get(ROW_LEN + 3)?,
                requirements: r.get(ROW_LEN + 4)?,
                description: r.get(ROW_LEN + 5)?,
                notes: r.get(ROW_LEN + 6)?,
                expires_at: r.get(ROW_LEN + 7)?,
                deleted_at: r.get(ROW_LEN + 8)?,
                content_removed: r.get(ROW_LEN + 9)?,
                renewed_at: r.get(ROW_LEN + 10)?,
                links: Vec::new(),
                history: Vec::new(),
                filters: Vec::new(),
                ai: crate::ai::OfferAi::default(),
                agenda: crate::agenda::Agenda::default(),
                dup: None,
            })
        })
        .optional()?
        .ok_or(AppError::NotFound)?;

    let mut stmt = conn.prepare(
        "SELECT source, url, first_seen, city FROM offer_sources WHERE offer_id = ?1 ORDER BY first_seen, source, rowid",
    )?;
    detail.links = stmt
        .query_map([id], |r| {
            let (source, url): (String, Option<String>) = (r.get(0)?, r.get(1)?);
            let source = match (&*source, &url) {
                ("manual", Some(u)) => portal_label(u),
                _ => source,
            };
            Ok(SourceLink { source, url, first_seen: r.get(2)?, city: r.get(3)? })
        })?
        .collect::<Result<_, _>>()?;

    let mut stmt = conn.prepare("SELECT stage, at FROM stage_history WHERE offer_id = ?1 ORDER BY at, id")?;
    detail.history = stmt
        .query_map([id], |r| Ok(StageEvent { stage: r.get(0)?, at: r.get(1)? }))?
        .collect::<Result<_, _>>()?;
    with_children(conn, std::slice::from_mut(&mut detail.row), None)?;
    detail.filters = profiles::offer_filters(conn, id)?;
    detail.ai = crate::ai::offer_ai(conn, id)?;
    detail.agenda = crate::agenda::agenda(conn, id)?;
    detail.dup = crate::dups::note(conn, id)?;
    Ok(detail)
}

// ---------- changes made by the user ----------

fn changed(count: usize) -> AppResult<()> {
    if count == 0 { Err(AppError::NotFound) } else { Ok(()) }
}

pub fn set_stage(conn: &mut Connection, id: i64, stage: Stage) -> AppResult<()> {
    let now = clock::now();
    let tx = conn.transaction()?;
    let current: Stage = tx
        .query_row("SELECT stage FROM offers WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .ok_or(AppError::NotFound)?;
    if current != stage {
        // Any stage past "new" (interested, applied and further) = the person's own recruitment:
        // the offer moves to "Own" (as the migration 023 did). Back to "new" keeps it there.
        tx.execute(
            "UPDATE offers SET stage = ?2, stage_changed_at = ?3, user_changed_at = ?3,
                own = CASE WHEN ?2 != 'new' THEN 1 ELSE own END
             WHERE id = ?1",
            params![id, stage, now],
        )?;
        // A child's recruitment is the family's: the whole family (the row on the list) moves to "Own".
        if stage != Stage::New {
            crate::tree::set_family_own(&tx, id, true)?;
        }
        tx.execute(
            "INSERT INTO stage_history (offer_id, stage, at) VALUES (?1, ?2, ?3)",
            params![id, stage, now],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn set_interest(conn: &Connection, id: i64, interest: Option<Interest>) -> AppResult<()> {
    changed(conn.execute(
        "UPDATE offers SET interest = ?2, user_changed_at = ?3 WHERE id = ?1",
        params![id, interest, clock::now()],
    )?)
}

pub fn set_favorite(conn: &Connection, id: i64, favorite: bool) -> AppResult<()> {
    changed(conn.execute(
        "UPDATE offers SET favorite = ?2, user_changed_at = ?3 WHERE id = ?1",
        params![id, favorite, clock::now()],
    )?)
}

/// "To own": a collected offer becomes the person's own (tab "Own"); `false` = back to collected
/// (the "Undo" of the message).
pub fn set_own(conn: &Connection, id: i64, own: bool) -> AppResult<()> {
    changed(crate::tree::set_family_own(conn, id, own)?)
}

pub fn set_notes(conn: &Connection, id: i64, notes: String) -> AppResult<()> {
    let notes = text(Some(notes), MAX_NOTES, "notes")?.unwrap_or_default();
    changed(conn.execute(
        "UPDATE offers SET notes = ?2, user_changed_at = ?3 WHERE id = ?1",
        params![id, notes, clock::now()],
    )?)
}

/// "Fill in the full description" from the offer page: kept when longer than what is stored;
/// AI reads the offer again.
pub fn update_description(conn: &mut Connection, id: i64, text: String) -> AppResult<bool> {
    let text = crate::text::cut(&crate::text::squeeze(&text), MAX_LONG);
    let text = self::text(Some(text), MAX_LONG, "description")?.unwrap_or_default();
    let tx = conn.transaction()?;
    let n = tx.execute(
        "UPDATE offers SET description = ?2, ai_status = NULL, updated_at = ?3
         WHERE id = ?1 AND length(?2) > length(COALESCE(description, ''))",
        params![id, text, clock::now()],
    )?;
    if n > 0 {
        profiles::recheck_offer(&tx, id)?;
    }
    tx.commit()?;
    Ok(n > 0)
}

pub fn trash(conn: &Connection, id: i64) -> AppResult<()> {
    let now = clock::now();
    changed(conn.execute(
        "UPDATE offers SET deleted_at = ?2, user_changed_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
        params![id, now],
    )?)
}

pub fn restore(conn: &Connection, id: i64) -> AppResult<()> {
    changed(conn.execute(
        "UPDATE offers SET deleted_at = NULL, user_changed_at = ?2 WHERE id = ?1 AND deleted_at IS NOT NULL",
        params![id, clock::now()],
    )?)
}

/// How an offer sat in the bin before "Restore" - "Undo" puts it back exactly so.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinPlace {
    pub deleted_at: Option<String>,
    pub archived_at: Option<String>,
    pub kept_at: Option<String>,
    /// Marked "not for me".
    pub declined: bool,
}

/// "Restore" in the bin, one for every kind: out of the trash, out of the archive (and never
/// archived by itself again, as "Bring back" did) and no longer "not for me". The offer goes back
/// to where it came from - "Own" or "Collected" (its `own` flag is untouched).
pub fn bring_back(conn: &Connection, id: i64) -> AppResult<BinPlace> {
    let now = clock::now();
    let place = conn
        .query_row(
            "SELECT deleted_at, archived_at, kept_at, COALESCE(interest, '') = 'no' FROM offers WHERE id = ?1",
            [id],
            |r| Ok(BinPlace { deleted_at: r.get(0)?, archived_at: r.get(1)?, kept_at: r.get(2)?, declined: r.get(3)? }),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    if place.deleted_at.is_none() && place.archived_at.is_none() && !place.declined {
        return Err(AppError::NotFound);
    }
    conn.execute(
        "UPDATE offers SET deleted_at = NULL, archived_at = NULL,
            kept_at = CASE WHEN archived_at IS NOT NULL THEN ?2 ELSE kept_at END,
            interest = CASE WHEN interest = 'no' THEN NULL ELSE interest END, user_changed_at = ?2
         WHERE id = ?1",
        params![id, now],
    )?;
    Ok(place)
}

/// "Undo" after "Restore": the offer is back in the bin as it was.
pub fn put_back(conn: &Connection, id: i64, place: BinPlace) -> AppResult<()> {
    let deleted_at = utc_time(place.deleted_at)?;
    let archived_at = utc_time(place.archived_at)?;
    let kept_at = utc_time(place.kept_at)?;
    changed(conn.execute(
        "UPDATE offers SET deleted_at = ?2, archived_at = ?3, kept_at = ?4,
            interest = CASE WHEN ?5 THEN 'no' ELSE interest END, user_changed_at = ?6
         WHERE id = ?1",
        params![id, deleted_at, archived_at, kept_at, place.declined, clock::now()],
    )?)
}

/// The keys of offers changed (8.11.2026): every offer gets its fingerprint again. Offers that
/// turn out to be one are merged - the one in use (not in the trash, with the user's work, then
/// the older one) stays and takes the other's sources, dates, contacts and notes; the other one
/// goes like a purged one (a tombstone for sync). Keys of offers deleted for good are made again.
/// Returns how many offers were merged away.
pub fn refresh_fingerprints(conn: &mut Connection) -> AppResult<usize> {
    let now = clock::now();
    let tx = conn.transaction()?;
    let mut groups: std::collections::BTreeMap<String, Vec<(i64, String)>> = std::collections::BTreeMap::new();
    {
        let mut stmt = tx.prepare(&format!(
            "SELECT o.id, o.company, o.title, o.fingerprint FROM offers o
             ORDER BY o.deleted_at IS NOT NULL, NOT {TOUCHED}, o.added_at, o.uid"
        ))?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let (id, company, title, old): (i64, String, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
            groups.entry(dedup::fingerprint(&company, &title)).or_default().push((id, old));
        }
    }
    let mut merged = 0;
    for (fingerprint, offers) in &groups {
        let (keeper, old) = &offers[0];
        for (gone, _) in &offers[1..] {
            let (city, notes, favorite, interest, url): (Option<String>, String, bool, Option<String>, Option<String>) = tx.query_row(
                "SELECT city, notes, favorite, interest, url FROM offers WHERE id = ?1",
                [gone],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            let mine: (Option<String>, String) = tx.query_row("SELECT city, notes FROM offers WHERE id = ?1", [keeper], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let notes = match (mine.1.trim().is_empty(), notes.trim().is_empty()) {
                (_, true) => mine.1.clone(),
                (true, false) => notes,
                (false, false) => crate::text::cut(&format!("{}\n\n{notes}", mine.1), MAX_NOTES),
            };
            tx.execute(
                "UPDATE offers SET city = ?2, notes = ?3, favorite = MAX(favorite, ?4), interest = COALESCE(interest, ?5),
                    url = COALESCE(url, ?6), updated_at = ?7, user_changed_at = ?7
                 WHERE id = ?1",
                params![keeper, join_cities(mine.0.as_deref(), city.as_deref()), notes, favorite, interest, url, now],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO offer_sources (offer_id, source, url, first_seen, checked_on, city)
                 SELECT ?1, source, url, first_seen, checked_on, city FROM offer_sources WHERE offer_id = ?2",
                params![keeper, gone],
            )?;
            for table in ["events", "contacts", "contact_log"] {
                tx.execute(&format!("UPDATE {table} SET offer_id = ?1 WHERE offer_id = ?2"), params![keeper, gone])?;
            }
            tx.execute(
                "INSERT INTO sync_deleted (kind, uid, at) SELECT 'offers', uid, ?2 FROM offers WHERE id = ?1
                 ON CONFLICT (kind, uid) DO UPDATE SET at = excluded.at",
                params![gone, now],
            )?;
            tx.execute("DELETE FROM offers WHERE id = ?1", [gone])?;
            merged += 1;
        }
        if old != fingerprint {
            // Through a placeholder: another offer may still hold this key until its own turn.
            tx.execute("UPDATE offers SET fingerprint = '~' || id, updated_at = ?2 WHERE id = ?1", params![keeper, now])?;
        }
    }
    for (fingerprint, offers) in &groups {
        tx.execute("UPDATE offers SET fingerprint = ?2 WHERE id = ?1 AND fingerprint = '~' || id", params![offers[0].0, fingerprint])?;
    }
    let removed: Vec<(String, String)> = tx
        .prepare("SELECT fingerprint, removed_at FROM removed_offers")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (old, at) in removed {
        let new = dedup::refresh_key(&old);
        if new != old {
            tx.execute("DELETE FROM removed_offers WHERE fingerprint = ?1", [&old])?;
            tx.execute("INSERT OR IGNORE INTO removed_offers (fingerprint, removed_at) VALUES (?1, ?2)", params![new, at])?;
        }
    }
    tx.commit()?;
    Ok(merged)
}

/// Deletes offers from the bin for good - the ones the chosen switch variant shows (`Bin` = all,
/// `Declined`, `Archive` = expired, `Trash` = deleted). Their fingerprints are remembered, so alerts
/// and aggregators never bring them back. `id = None` = empty the whole variant.
pub fn purge(conn: &mut Connection, id: Option<i64>, view: View) -> AppResult<usize> {
    if !matches!(view, View::Bin | View::Declined | View::Archive | View::Trash) {
        return Err(AppError::Invalid("only the bin can be emptied".into()));
    }
    let picked = format!("({}) AND (?1 IS NULL OR o.id = ?1)", view_condition(view, "", ""));
    let now = clock::now();
    let tx = conn.transaction()?;
    tx.execute(
        &format!(
            "INSERT OR IGNORE INTO removed_offers (fingerprint, removed_at)
             SELECT o.fingerprint, ?2 FROM offers o WHERE {picked}"
        ),
        params![id, now],
    )?;
    // Tombstones first: sync deletes these offers on the other device too.
    tx.execute(
        &format!(
            "INSERT INTO sync_deleted (kind, uid, at) SELECT 'offers', o.uid, ?2 FROM offers o WHERE {picked}
             ON CONFLICT (kind, uid) DO UPDATE SET at = excluded.at"
        ),
        params![id, now],
    )?;
    // Children of a purged parent go with it (their keys are the parent's).
    tx.execute(
        &format!(
            "INSERT INTO sync_deleted (kind, uid, at) SELECT 'offers', c.uid, ?2 FROM offers c
             WHERE c.parent_id IN (SELECT o.id FROM offers o WHERE {picked})
             ON CONFLICT (kind, uid) DO UPDATE SET at = excluded.at"
        ),
        params![id, now],
    )?;
    tx.execute(
        &format!("DELETE FROM offers WHERE parent_id IN (SELECT o.id FROM offers o WHERE {picked})"),
        params![id],
    )?;
    let count = tx.execute(&format!("DELETE FROM offers WHERE id IN (SELECT o.id FROM offers o WHERE {picked})"), params![id])?;
    tx.commit()?;
    if id.is_some() && count == 0 {
        return Err(AppError::NotFound);
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;

    fn offer(title: &str, company: &str, source: &str) -> OfferInput {
        OfferInput {
            title: title.into(),
            company: company.into(),
            source: source.into(),
            ..Default::default()
        }
    }

    fn id_of(outcome: AddOutcome) -> i64 {
        match outcome {
            AddOutcome::Added { id } | AddOutcome::Merged { id } | AddOutcome::Unchanged { id } => id,
            AddOutcome::Blocked => panic!("unexpected Blocked"),
        }
    }

    #[test]
    fn outside_data_over_the_limits_is_refused() {
        let mut conn = test_db();
        let with = |change: &dyn Fn(&mut OfferInput)| {
            let mut o = offer("Tester", "Firma Testowa", "adzuna");
            change(&mut o);
            o
        };
        let refused = [
            with(&|o| o.title = "x".repeat(MAX_SHORT + 1)),
            with(&|o| o.company = "x".repeat(MAX_SHORT + 1)),
            with(&|o| o.city = Some("x".repeat(MAX_SHORT + 1))),
            with(&|o| o.description = Some("x".repeat(MAX_LONG + 1))),
            with(&|o| o.requirements = Some("x".repeat(MAX_LONG + 1))),
            with(&|o| o.url = Some(format!("https://example.com/{}", "x".repeat(MAX_URL)))),
            with(&|o| o.url = Some("ftp://example.com/oferta".into())),
            with(&|o| o.title = "Tester\u{0007}".into()),
            with(&|o| o.description = Some("Opis\u{001b}[31m".into())),
            with(&|o| o.salary_max = Some(MAX_SALARY + 1)),
            with(&|o| {
                o.salary_min = Some(5000);
                o.salary_currency = Some("ZŁOTE".into());
            }),
            with(&|o| o.expires_at = Some("jutro".into())),
            with(&|o| o.source = "Adzuna!".into()),
        ];
        for (i, o) in refused.into_iter().enumerate() {
            assert!(matches!(add(&mut conn, o), Err(AppError::Invalid(_))), "case {i} was accepted");
        }
        // At the limit and with line breaks - still fine.
        let ok = with(&|o| {
            o.title = "x".repeat(MAX_SHORT);
            o.description = Some("Pierwsza linia\nDruga\tlinia".into());
        });
        assert!(add(&mut conn, ok).is_ok());
        let count: i64 = conn.query_row("SELECT count(*) FROM offers", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn offers_past_their_date_are_expired() {
        let mut conn = test_db();
        let mut old = offer("Tester", "Firma Testowa", "adzuna");
        old.expires_at = Some("2026-11-01T00:00:00Z".into());
        let old = id_of(add(&mut conn, old).unwrap());
        let mut open = offer("Analityk", "Firma Testowa", "jooble");
        open.expires_at = Some("2026-12-01T00:00:00Z".into());
        let open = id_of(add(&mut conn, open).unwrap());
        let none = id_of(add(&mut conn, offer("Programista", "Firma Testowa", "manual")).unwrap());

        assert_eq!(mark_expired(&conn, "2026-11-04T12:00:00Z").unwrap(), 1);
        assert!(detail(&conn, old).unwrap().row.expired);
        assert!(!detail(&conn, open).unwrap().row.expired);
        assert!(!detail(&conn, none).unwrap().row.expired);
        // Nothing new the next time.
        assert_eq!(mark_expired(&conn, "2026-11-04T13:00:00Z").unwrap(), 0);
    }

    #[test]
    fn only_a_newer_date_takes_off_the_automatic_expiry_and_never_the_users() {
        let mut conn = test_db();
        let mut first = offer("Tester", "Firma Testowa", "adzuna");
        first.published_at = Some("2026-11-01T00:00:00Z".into());
        first.expires_at = Some("2026-11-03T00:00:00Z".into());
        let auto = id_of(add(&mut conn, first.clone()).unwrap());
        mark_expired(&conn, "2026-11-04T12:00:00Z").unwrap();
        // The same offer with the same dates: still expired.
        add(&mut conn, first.clone()).unwrap();
        assert!(detail(&conn, auto).unwrap().row.expired);
        // A newer expiry date: open again.
        let mut later = first.clone();
        later.expires_at = Some("2026-12-01T00:00:00Z".into());
        add(&mut conn, later).unwrap();
        assert!(!detail(&conn, auto).unwrap().row.expired);

        // "Expired" clicked by the user stays, whatever comes later.
        let mut mine = offer("Analityk", "Firma Testowa", "jooble");
        mine.published_at = Some("2026-11-01T00:00:00Z".into());
        let id = id_of(add(&mut conn, mine.clone()).unwrap());
        set_favorite(&conn, id, true).unwrap();
        assert_eq!(set_expired(&conn, id).unwrap(), ExpiredBy::Marked);
        mine.published_at = Some("2026-11-20T00:00:00Z".into());
        mine.expires_at = Some("2026-12-20T00:00:00Z".into());
        add(&mut conn, mine).unwrap();
        assert!(detail(&conn, id).unwrap().row.expired);
        // Until the user takes it off.
        unset_expired(&conn, id).unwrap();
        assert!(!detail(&conn, id).unwrap().row.expired);
    }

    #[test]
    fn one_job_in_another_city_is_a_separate_offer_with_a_question() {
        let mut conn = test_db();
        let mut krakow = offer("Kasjer", "Sklep Przykładowy", "pracuj");
        krakow.city = Some("Kraków".into());
        krakow.url = Some("https://www.pracuj.pl/praca/kasjer-krakow,oferta,1001".into());
        // A fixed date: without one it is "now", and a later second is a newer date (merged).
        krakow.published_at = Some("2026-11-01T08:00:00Z".into());
        let id = id_of(add(&mut conn, krakow.clone()).unwrap());
        // A chain looks for the same job in another city - not joined by itself (point 15).
        let mut gdansk = krakow.clone();
        gdansk.city = Some("Gdańsk".into());
        gdansk.url = Some("https://www.pracuj.pl/praca/kasjer-gdansk,oferta,1002".into());
        let other = id_of(add(&mut conn, gdansk.clone()).unwrap());
        assert_ne!(other, id);
        // Seen again - with a tracking parameter, and the city written longer: nothing new.
        let mut again = gdansk.clone();
        again.url = Some(format!("{}?utm_source=mail", gdansk.url.as_deref().unwrap()));
        again.city = Some("Gdańsk, pomorskie".into());
        assert_eq!(add(&mut conn, again).unwrap(), AddOutcome::Unchanged { id: other });
        let d = detail(&conn, id).unwrap();
        assert_eq!(d.row.city.as_deref(), Some("Kraków"));
        assert!(d.row.children.is_empty());
        let separate = detail(&conn, other).unwrap();
        let links: Vec<(Option<&str>, Option<&str>)> = separate.links.iter().map(|l| (l.url.as_deref(), l.city.as_deref())).collect();
        assert_eq!(links, vec![(gdansk.url.as_deref(), Some("Gdańsk"))]);
        assert!(separate.row.dup_ask);
        assert_eq!(separate.dup.map(|n| (n.was.id, n.why)), Some((id, "city".to_string())));
        // The same city on another link: the second ad of the job - a child.
        let mut second = krakow.clone();
        second.url = Some("https://www.pracuj.pl/praca/kasjer-krakow,oferta,1003".into());
        assert_eq!(add(&mut conn, second).unwrap(), AddOutcome::Merged { id });
        assert_eq!(detail(&conn, id).unwrap().row.children.len(), 1);
        // "Remote" is no city to add.
        let mut remote = krakow.clone();
        remote.city = Some("Remote".into());
        add(&mut conn, remote).unwrap();
        assert_eq!(detail(&conn, id).unwrap().row.city.as_deref(), Some("Kraków"));
    }

    #[test]
    fn the_filter_takes_any_city_of_the_list() {
        use crate::model::{CityRule, Criteria, ProfileInput, Remote};
        let mut conn = test_db();
        let criteria = Criteria {
            cities: vec![CityRule { name: "Gdańsk".into(), radius_km: 0 }],
            work_modes: vec![Remote::No],
            ..Default::default()
        };
        profiles::save(&mut conn, ProfileInput { id: None, name: "Gdańsk".into(), description: "Kasjer".into(), criteria }).unwrap();
        let mut krakow = offer("Kasjer", "Sklep Przykładowy", "pracuj");
        krakow.city = Some("Kraków".into());
        krakow.remote = Some(Remote::No);
        let id = id_of(add(&mut conn, krakow.clone()).unwrap());
        assert_eq!(detail(&conn, id).unwrap().row.filter_result, crate::model::FilterResult::Fail);
        let mut gdansk = krakow;
        gdansk.city = Some("Gdańsk".into());
        add(&mut conn, gdansk).unwrap();
        assert_eq!(detail(&conn, id).unwrap().row.filter_result, crate::model::FilterResult::Pass);
    }

    #[test]
    fn closed_source_keeps_one_row_without_links() {
        let mut conn = test_db();
        let mut a = offer("Kasjer", "Sklep Przykładowy", "himalayas");
        a.url = Some("https://himalayas.app/companies/sklep/jobs/kasjer-1".into());
        let id = id_of(add(&mut conn, a.clone()).unwrap());
        a.url = Some("https://himalayas.app/companies/sklep/jobs/kasjer-2".into());
        add(&mut conn, a).unwrap();
        let tx = conn.transaction().unwrap();
        drop_links(&tx, id, "himalayas").unwrap();
        tx.commit().unwrap();
        let links = detail(&conn, id).unwrap().links;
        assert_eq!(links.len(), 1);
        assert!(links[0].url.is_none());
    }

    #[test]
    fn new_keys_merge_old_twins_and_keep_the_users_work() {
        let mut conn = test_db();
        // Saved with the old keys (as before 8.11.2026): two keys for one job.
        let long = id_of(add(&mut conn, offer("Tester", "Firma Przykładowa Sp. z o.o. Sp. k.", "pracuj")).unwrap());
        // Added a day earlier: the same second would be decided by the random `uid`.
        conn.execute("UPDATE offers SET fingerprint = 'firma przykladowa sp k|tester', added_at = '2026-11-01T00:00:00Z' WHERE id = ?1", [long]).unwrap();
        let short = id_of(add(&mut conn, offer("Tester", "Firma Przykładowa", "jjit")).unwrap());
        set_notes(&conn, short, "Zadzwonić we wtorek".into()).unwrap();
        crate::agenda::add_event(&conn, long, crate::agenda::EventInput { kind: "interview".into(), at: "2026-11-20T10:00:00Z".into(), minutes: 60, note: String::new() }).unwrap();
        let cpp = id_of(add(&mut conn, offer("Programista C++", "Firma Testowa", "pracuj")).unwrap());
        conn.execute("UPDATE offers SET fingerprint = 'firma testowa|programista c' WHERE id = ?1", [cpp]).unwrap();
        conn.execute("INSERT INTO removed_offers (fingerprint, removed_at) VALUES ('firma usunieta sp j|kasjer', '2026-11-01T00:00:00Z')", []).unwrap();

        assert_eq!(refresh_fingerprints(&mut conn).unwrap(), 1);
        // Both had the user's work (a note, a date) - the older one stays with both.
        let d = detail(&conn, long).unwrap();
        assert_eq!(d.notes, "Zadzwonić we wtorek");
        assert_eq!(d.agenda.events.len(), 1);
        let mut sources = d.row.sources.clone();
        sources.sort();
        assert_eq!(sources, vec!["jjit", "pracuj"]);
        assert!(matches!(detail(&conn, short), Err(AppError::NotFound)));
        assert_eq!(detail(&conn, cpp).unwrap().row.id, cpp);
        // The same job fetched again joins the kept offer; a deleted one stays deleted with its new key.
        assert_eq!(add(&mut conn, offer("Tester", "Firma Przykładowa", "adzuna")).unwrap(), AddOutcome::Merged { id: long });
        assert_eq!(add(&mut conn, offer("Programista C++", "Firma Testowa", "adzuna")).unwrap(), AddOutcome::Merged { id: cpp });
        assert_eq!(add(&mut conn, offer("Kasjer", "Firma Usunięta Spółka Jawna", "adzuna")).unwrap(), AddOutcome::Blocked);
        let tombstones: i64 = conn.query_row("SELECT count(*) FROM sync_deleted WHERE kind = 'offers'", [], |r| r.get(0)).unwrap();
        assert_eq!(tombstones, 1);
        // Run again: nothing more to do.
        assert_eq!(refresh_fingerprints(&mut conn).unwrap(), 0);
    }

    #[test]
    fn an_ad_added_by_hand_is_named_after_the_portal_of_its_link() {
        let mut conn = test_db();
        let with_link = |title: &str, url: Option<&str>| OfferInput { url: url.map(str::to_string), ..offer(title, "Firma Testowa", "manual") };
        let pracuj = id_of(add(&mut conn, with_link("Magazynier", Some("https://www.pracuj.pl/praca/magazynier,oferta,1001"))).unwrap());
        let other = id_of(add(&mut conn, with_link("Kierowca", Some("https://kariera.firma-testowa.pl/oferta/7"))).unwrap());
        let bare = id_of(add(&mut conn, with_link("Sprzedawca", None)).unwrap());
        let rows = list(&conn, View::Own, None).unwrap();
        let sources = |id: i64| rows.iter().find(|r| r.id == id).unwrap().sources.clone();
        assert_eq!(sources(pracuj), vec!["pracuj"]);
        assert_eq!(sources(other), vec!["kariera.firma-testowa.pl"]);
        assert_eq!(sources(bare), vec!["manual"]);
        // Still added by hand, and so is the link on the card.
        assert!(rows.iter().all(|r| r.manual));
        let d = detail(&conn, pracuj).unwrap();
        assert_eq!(d.links[0].source, "pracuj");
        assert_eq!(detail(&conn, bare).unwrap().links[0].source, "manual");
    }

    #[test]
    fn adds_and_lists_an_offer_with_history() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Frontend Developer", "Firma Testowa", "manual")).unwrap());
        // Added by hand = own: in "Own", not among the collected ones.
        assert!(list(&conn, View::Board, None).unwrap().is_empty());
        let rows = list(&conn, View::Own, None).unwrap();
        assert_eq!(rows.len(), 1);
        // Added by hand = chosen by the person: interested from the start.
        assert_eq!(rows[0].stage, Stage::Interested);
        assert_eq!(rows[0].sources, vec!["manual"]);
        let d = detail(&conn, id).unwrap();
        assert_eq!(d.history.len(), 1);
        assert_eq!(d.notes, "");
    }

    /// Added by hand = the person's own offer; a collected one is not, until it is added by hand.
    #[test]
    fn an_offer_added_by_hand_is_own() {
        let mut conn = test_db();
        let manual = id_of(add(&mut conn, offer("Frontend Developer", "Firma Testowa", "manual")).unwrap());
        let collected = id_of(add(&mut conn, offer("Tester", "Firma Przykładowa", "adzuna")).unwrap());
        assert!(detail(&conn, manual).unwrap().row.own);
        assert!(!detail(&conn, collected).unwrap().row.own);
        // Fetched again from a source: still collected.
        add(&mut conn, offer("Tester", "Firma Przykładowa", "jooble")).unwrap();
        assert!(!detail(&conn, collected).unwrap().row.own);
        // The same offer added by hand: chosen by the person.
        assert_eq!(add(&mut conn, offer("Tester", "Firma Przykładowa", "manual")).unwrap(), AddOutcome::Merged { id: collected });
        assert!(detail(&conn, collected).unwrap().row.own);
    }

    #[test]
    fn duplicate_from_another_portal_is_merged() {
        let mut conn = test_db();
        let first = add(&mut conn, offer("Frontend Developer (k/m)", "Firma Testowa Sp. z o.o.", "adzuna")).unwrap();
        let mut second = offer("frontend developer", "FIRMA TESTOWA", "remotive");
        second.salary_min = Some(9000);
        second.salary_max = Some(12000);
        second.description = Some("Longer description of the role".into());
        second.remote = Some(Remote::Yes);
        let merged = add(&mut conn, second).unwrap();
        assert_eq!(merged, AddOutcome::Merged { id: id_of(first) });

        let d = detail(&conn, id_of(first)).unwrap();
        assert_eq!(d.row.sources, vec!["adzuna", "remotive"]);
        assert_eq!(d.row.salary_min, Some(9000));
        assert_eq!(d.row.salary_currency.as_deref(), Some("PLN"));
        assert_eq!(d.row.remote, Remote::Yes);
        assert_eq!(d.description.as_deref(), Some("Longer description of the role"));
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 1);
    }

    #[test]
    fn merge_keeps_existing_salary_and_longer_text() {
        let mut conn = test_db();
        let mut a = offer("Tester", "Firma A", "manual");
        a.salary_min = Some(8000);
        a.description = Some("A long and detailed description".into());
        let id = id_of(add(&mut conn, a).unwrap());
        let mut b = offer("Tester", "Firma A", "adzuna");
        b.salary_min = Some(1);
        b.description = Some("Short".into());
        add(&mut conn, b).unwrap();
        let d = detail(&conn, id).unwrap();
        assert_eq!(d.row.salary_min, Some(8000));
        assert_eq!(d.description.as_deref(), Some("A long and detailed description"));
    }

    #[test]
    fn a_glued_jooble_offer_is_fixed_by_the_next_mail() {
        let mut conn = test_db();
        let card = "Zmyślona Sp. z o.o.\nGdańsk\nPraca zdalna";
        let mut glued = offer("Analityk danych", "Zmyślona Sp. z o.o.", "jooble");
        glued.description = Some(format!("{card}\nProgramista Python\n14500 zł\nPrzykładowa Firma\nUmowa o pracę"));
        glued.requirements = Some("Programista Python, Przykładowa Firma".into());
        glued.salary_min = Some(14500);
        glued.contract = Some("UoP".into());
        let id = id_of(add(&mut conn, glued).unwrap());
        conn.execute("UPDATE offers SET ai_status = 'done' WHERE id = ?1", [id]).unwrap();
        let mut right = offer("Analityk danych", "Zmyślona Sp. z o.o.", "jooble");
        right.description = Some(card.into());
        right.remote = Some(Remote::Yes);
        add(&mut conn, right.clone()).unwrap();
        let d = detail(&conn, id).unwrap();
        assert_eq!(d.description.as_deref(), Some(card));
        assert_eq!((d.requirements.as_deref(), d.row.salary_min, d.contract.as_deref()), (None, None, None));
        assert_eq!(d.row.remote, Remote::Yes);
        let ai: Option<String> = conn.query_row("SELECT ai_status FROM offers WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(ai, None, "the AI reads the fixed offer again");
        // Another source, or a text that only shares the start of a line, keeps the longer text.
        let mut other = offer("Analityk danych", "Zmyślona Sp. z o.o.", "adzuna");
        other.description = Some("Zmyślona".into());
        add(&mut conn, other).unwrap();
        right.description = Some("Zmyślona Sp. z o.o.\nGdań".into());
        add(&mut conn, right).unwrap();
        assert_eq!(detail(&conn, id).unwrap().description.as_deref(), Some(card));
    }

    #[test]
    fn a_deleted_portal_offer_read_better_later_stays_deleted() {
        let mut conn = test_db();
        let mut junk = offer("Programista PHP Firma X • Gdynia", "? (abc123)", "jooble");
        junk.url = Some("https://pl.jooble.org/away/42?mid=a&pos=1".into());
        let id = id_of(add(&mut conn, junk).unwrap());
        trash(&conn, id).unwrap();
        let mut better = offer("Programista PHP", "Firma X", "jooble");
        better.url = Some("https://pl.jooble.org/away/42?mid=b&pos=3".into());
        assert_eq!(add(&mut conn, better.clone()).unwrap(), AddOutcome::Blocked);
        // Another offer of the same portal still comes in.
        better.url = Some("https://pl.jooble.org/away/43?mid=b".into());
        assert!(matches!(add(&mut conn, better).unwrap(), AddOutcome::Added { .. }));
    }

    #[test]
    fn a_full_text_after_a_short_alert_is_read_by_ai_again() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Tester", "Firma A", "jooble")).unwrap());
        let status = |c: &Connection| c.query_row("SELECT ai_status FROM offers WHERE id = ?1", [id], |r| r.get::<_, Option<String>>(0)).unwrap();
        conn.execute("UPDATE offers SET ai_status = 'done'", []).unwrap();
        // The same short text again: AI does not read it once more.
        let mut b = offer("Tester", "Firma A", "jooble");
        b.description = Some("Krótko.".into());
        add(&mut conn, b).unwrap();
        assert_eq!(status(&conn).as_deref(), Some("done"));
        // The full description from another source: it waits for AI again.
        let mut c = offer("Tester", "Firma A", "adzuna");
        c.description = Some("Pełny opis oferty. ".repeat(30));
        add(&mut conn, c).unwrap();
        assert_eq!(status(&conn), None);
    }

    #[test]
    fn trashed_and_purged_offers_do_not_come_back() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("QA Engineer", "Firma B", "adzuna")).unwrap());
        trash(&conn, id).unwrap();
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 0);
        assert_eq!(list(&conn, View::Trash, None).unwrap().len(), 1);
        assert_eq!(add(&mut conn, offer("QA Engineer", "Firma B", "remotive")).unwrap(), AddOutcome::Blocked);

        assert_eq!(purge(&mut conn, Some(id), View::Trash).unwrap(), 1);
        assert_eq!(list(&conn, View::Trash, None).unwrap().len(), 0);
        assert_eq!(add(&mut conn, offer("QA Engineer", "Firma B", "adzuna")).unwrap(), AddOutcome::Blocked);
    }

    /// "Empty the bin" takes what the chosen variant shows, nothing else; none of the purged
    /// offers comes back as new.
    #[test]
    fn emptying_the_bin_takes_the_chosen_variant_only() {
        let mut conn = test_db();
        let declined = id_of(add(&mut conn, offer("Nie dla mnie", "Firma A", "adzuna")).unwrap());
        let expired = id_of(add(&mut conn, offer("Wygasła", "Firma B", "adzuna")).unwrap());
        let deleted = id_of(add(&mut conn, offer("Usunięta", "Firma C", "adzuna")).unwrap());
        add(&mut conn, offer("Zostaje", "Firma D", "adzuna")).unwrap();
        set_interest(&conn, declined, Some(Interest::No)).unwrap();
        set_expired(&conn, expired).unwrap();
        trash(&conn, deleted).unwrap();
        // Not the bin - refused.
        assert!(matches!(purge(&mut conn, None, View::Board), Err(AppError::Invalid(_))));
        // One offer of another variant is not touched.
        assert!(matches!(purge(&mut conn, Some(declined), View::Trash), Err(AppError::NotFound)));

        assert_eq!(purge(&mut conn, None, View::Declined).unwrap(), 1);
        let bin: Vec<i64> = list(&conn, View::Bin, None).unwrap().iter().map(|r| r.id).collect();
        assert!(!bin.contains(&declined) && bin.contains(&expired) && bin.contains(&deleted));
        assert_eq!(purge(&mut conn, None, View::Bin).unwrap(), 2);
        assert!(list(&conn, View::Bin, None).unwrap().is_empty());
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 1);
        for (title, company) in [("Nie dla mnie", "Firma A"), ("Wygasła", "Firma B"), ("Usunięta", "Firma C")] {
            assert_eq!(add(&mut conn, offer(title, company, "remotive")).unwrap(), AddOutcome::Blocked);
        }
    }

    #[test]
    fn purge_only_touches_the_trash() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Dev", "Firma C", "manual")).unwrap());
        assert!(matches!(purge(&mut conn, Some(id), View::Trash), Err(AppError::NotFound)));
        assert_eq!(purge(&mut conn, None, View::Trash).unwrap(), 0);
        assert_eq!(list(&conn, View::Own, None).unwrap().len(), 1);
    }

    #[test]
    fn restore_brings_offer_back_to_the_board() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Dev", "Firma D", "manual")).unwrap());
        trash(&conn, id).unwrap();
        restore(&conn, id).unwrap();
        assert_eq!(list(&conn, View::Own, None).unwrap().len(), 1);
        assert!(matches!(restore(&conn, id), Err(AppError::NotFound)));
    }

    #[test]
    fn offers_without_a_link_go_below_the_other_own_ones() {
        let mut conn = test_db();
        let mut with_link = offer("Z linkiem", "Firma A", "manual");
        with_link.url = Some("https://example.com/oferta/1".into());
        let linked = id_of(add(&mut conn, with_link).unwrap());
        let bare = id_of(add(&mut conn, offer("Bez linku", "Firma B", "manual")).unwrap());
        // Added later (newer), still below the one with a link.
        conn.execute("UPDATE offers SET added_at = '2099-01-01T00:00:00Z', published_at = NULL WHERE id = ?1", [bare]).unwrap();
        let rows = list(&conn, View::Own, None).unwrap();
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![linked, bare]);
        assert!(!rows[0].no_link && rows[1].no_link);
    }

    /// "Own": the CV still to be sent on top, then recruitments, rejected and withdrawn at the
    /// bottom; "not for me" leaves it. The number on the tab = the CVs still to be sent.
    #[test]
    fn own_offers_go_from_to_send_to_over() {
        let mut conn = test_db();
        let over = id_of(add(&mut conn, offer("Odrzucona", "Firma A", "manual")).unwrap());
        let sent = id_of(add(&mut conn, offer("Wysłana", "Firma B", "manual")).unwrap());
        let to_send = id_of(add(&mut conn, offer("Do wysłania", "Firma C", "manual")).unwrap());
        let declined = id_of(add(&mut conn, offer("Nie dla mnie", "Firma D", "manual")).unwrap());
        let collected = id_of(add(&mut conn, offer("Zebrana", "Firma E", "adzuna")).unwrap());
        set_stage(&mut conn, sent, Stage::Applied).unwrap();
        set_stage(&mut conn, over, Stage::Rejected).unwrap();
        set_interest(&conn, declined, Some(Interest::No)).unwrap();
        // Newest first within a group: the rejected one was added first, still at the bottom.
        conn.execute("UPDATE offers SET added_at = '2099-01-01T00:00:00Z', published_at = NULL WHERE id = ?1", [over]).unwrap();
        let own: Vec<i64> = list(&conn, View::Own, None).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(own, vec![to_send, sent, over]);
        assert!(!own.contains(&collected) && !own.contains(&declined));
        assert_eq!(counts(&conn, None).unwrap().to_send, 1);
    }

    /// The bin: "not for me", expired and deleted offers in one list, each with its kind, the last
    /// thrown in on top; the rest stays out.
    #[test]
    fn the_bin_holds_declined_expired_and_deleted_offers() {
        let mut conn = test_db();
        let declined = id_of(add(&mut conn, offer("Nie dla mnie", "Firma A", "adzuna")).unwrap());
        let expired = id_of(add(&mut conn, offer("Wygasła", "Firma B", "adzuna")).unwrap());
        let deleted = id_of(add(&mut conn, offer("Usunięta", "Firma C", "manual")).unwrap());
        let kept = id_of(add(&mut conn, offer("Zostaje", "Firma D", "adzuna")).unwrap());
        set_interest(&conn, declined, Some(Interest::No)).unwrap();
        assert_eq!(set_expired(&conn, expired).unwrap(), ExpiredBy::Archived);
        trash(&conn, deleted).unwrap();
        // The moments they went in: deleted last, then "not for me", the expired one first.
        conn.execute("UPDATE offers SET archived_at = '2026-11-01T00:00:00Z' WHERE id = ?1", [expired]).unwrap();
        conn.execute("UPDATE offers SET user_changed_at = '2026-11-05T00:00:00Z' WHERE id = ?1", [declined]).unwrap();
        conn.execute("UPDATE offers SET deleted_at = '2026-11-09T00:00:00Z' WHERE id = ?1", [deleted]).unwrap();

        let bin = list(&conn, View::Bin, None).unwrap();
        let got: Vec<(i64, Option<BinKind>)> = bin.iter().map(|r| (r.id, r.bin)).collect();
        assert_eq!(
            got,
            vec![(deleted, Some(BinKind::Deleted)), (declined, Some(BinKind::Declined)), (expired, Some(BinKind::Expired))]
        );
        assert!(!bin.iter().any(|r| r.id == kept));
        assert_eq!(detail(&conn, kept).unwrap().row.bin, None);
        // Deleted wins over "not for me": one kind per offer.
        trash(&conn, declined).unwrap();
        assert_eq!(detail(&conn, declined).unwrap().row.bin, Some(BinKind::Deleted));
    }

    /// The bin's switch: all / not for me / expired / deleted - each variant lists the offers of
    /// its kind only, and the numbers match the lists.
    #[test]
    fn the_bin_switch_splits_it_by_kind() {
        let mut conn = test_db();
        let declined = id_of(add(&mut conn, offer("Nie dla mnie", "Firma A", "adzuna")).unwrap());
        let expired = id_of(add(&mut conn, offer("Wygasła", "Firma B", "adzuna")).unwrap());
        let deleted = id_of(add(&mut conn, offer("Usunięta", "Firma C", "manual")).unwrap());
        let both = id_of(add(&mut conn, offer("Usunięta i nie dla mnie", "Firma D", "adzuna")).unwrap());
        add(&mut conn, offer("Zostaje", "Firma E", "adzuna")).unwrap();
        set_interest(&conn, declined, Some(Interest::No)).unwrap();
        set_interest(&conn, both, Some(Interest::No)).unwrap();
        set_expired(&conn, expired).unwrap();
        trash(&conn, deleted).unwrap();
        trash(&conn, both).unwrap();

        let ids = |view: View| -> Vec<i64> {
            let mut v: Vec<i64> = list(&conn, view, None).unwrap().iter().map(|r| r.id).collect();
            v.sort();
            v
        };
        let kinds = |view: View| -> Vec<Option<BinKind>> { list(&conn, view, None).unwrap().iter().map(|r| r.bin).collect() };
        assert_eq!(ids(View::Declined), vec![declined]);
        assert_eq!(ids(View::Archive), vec![expired]);
        assert_eq!(ids(View::Trash), vec![deleted, both]);
        assert_eq!(ids(View::Bin), vec![declined, expired, deleted, both]);
        assert!(kinds(View::Declined).iter().all(|k| *k == Some(BinKind::Declined)));
        assert!(kinds(View::Archive).iter().all(|k| *k == Some(BinKind::Expired)));
        assert!(kinds(View::Trash).iter().all(|k| *k == Some(BinKind::Deleted)));
        let c = counts(&conn, None).unwrap();
        assert_eq!((c.bin, c.declined, c.archive, c.trash), (4, 1, 1, 2));
    }

    /// One "Restore" for every kind: the offer leaves the bin for the tab it came from ("Own" or
    /// "Collected"), an expired one is kept from now on; "Undo" puts it back exactly as it was.
    #[test]
    fn restore_brings_any_bin_offer_back_where_it_came_from() {
        let mut conn = test_db();
        let declined = id_of(add(&mut conn, offer("Nie dla mnie", "Firma A", "adzuna")).unwrap());
        let expired = id_of(add(&mut conn, offer("Wygasła", "Firma B", "adzuna")).unwrap());
        let own = id_of(add(&mut conn, offer("Własna usunięta i nie dla mnie", "Firma C", "manual")).unwrap());
        let live = id_of(add(&mut conn, offer("Zostaje", "Firma D", "adzuna")).unwrap());
        set_interest(&conn, declined, Some(Interest::No)).unwrap();
        set_expired(&conn, expired).unwrap();
        set_interest(&conn, own, Some(Interest::No)).unwrap();
        trash(&conn, own).unwrap();
        let before = detail(&conn, own).unwrap();

        let places: Vec<BinPlace> = [declined, expired, own].iter().map(|&id| bring_back(&conn, id).unwrap()).collect();
        assert!(list(&conn, View::Bin, None).unwrap().is_empty());
        let collected: Vec<i64> = list(&conn, View::Board, None).unwrap().iter().map(|r| r.id).collect();
        assert!(collected.contains(&declined) && collected.contains(&expired) && !collected.contains(&own));
        assert_eq!(list(&conn, View::Own, None).unwrap().iter().map(|r| r.id).collect::<Vec<_>>(), vec![own]);
        // Restored from the archive = kept: the old-offer cleanup never takes it again.
        assert!(conn.query_row("SELECT kept_at IS NOT NULL FROM offers WHERE id = ?1", [expired], |r| r.get::<_, bool>(0)).unwrap());
        // Nothing in the bin - nothing to restore.
        assert!(matches!(bring_back(&conn, live), Err(AppError::NotFound)));

        // "Undo": every one back in the bin as it was, with the same kind and dates.
        for (id, place) in [declined, expired, own].into_iter().zip(places) {
            put_back(&conn, id, place).unwrap();
        }
        let kinds: Vec<(i64, Option<BinKind>)> = {
            let mut v: Vec<_> = list(&conn, View::Bin, None).unwrap().iter().map(|r| (r.id, r.bin)).collect();
            v.sort_by_key(|(id, _)| *id);
            v
        };
        assert_eq!(kinds, vec![(declined, Some(BinKind::Declined)), (expired, Some(BinKind::Expired)), (own, Some(BinKind::Deleted))]);
        let after = detail(&conn, own).unwrap();
        assert_eq!((after.deleted_at, after.row.interest), (before.deleted_at, before.row.interest));
        assert!(conn.query_row("SELECT kept_at IS NULL FROM offers WHERE id = ?1", [expired], |r| r.get::<_, bool>(0)).unwrap());
    }

    /// "Applied" (any stage past "new") moves a collected offer to "Own"; back to "new" keeps it there.
    #[test]
    fn a_stage_past_new_makes_the_offer_own() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Tester", "Firma A", "adzuna")).unwrap());
        assert!(!detail(&conn, id).unwrap().row.own);
        set_stage(&mut conn, id, Stage::Applied).unwrap();
        assert!(detail(&conn, id).unwrap().row.own);
        assert_eq!(list(&conn, View::Own, None).unwrap()[0].id, id);
        set_stage(&mut conn, id, Stage::New).unwrap();
        assert!(detail(&conn, id).unwrap().row.own);
        // "Interested" too - the CV is still to be sent, the top of "Own".
        let other = id_of(add(&mut conn, offer("Analityk", "Firma B", "jooble")).unwrap());
        set_stage(&mut conn, other, Stage::Interested).unwrap();
        assert!(detail(&conn, other).unwrap().row.own);
    }

    /// "To own" moves a collected offer from "Collected" to "Own", "Undo" moves it back.
    #[test]
    fn to_own_moves_an_offer_between_the_tabs() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Tester", "Firma A", "adzuna")).unwrap());
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 1);
        set_own(&conn, id, true).unwrap();
        assert!(list(&conn, View::Board, None).unwrap().is_empty());
        assert_eq!(list(&conn, View::Own, None).unwrap()[0].id, id);
        // The person's change: it wins in the sync.
        let changed: String = conn.query_row("SELECT user_changed_at FROM offers WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_ne!(changed, NOT_BY_USER);
        set_own(&conn, id, false).unwrap();
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 1);
        assert!(matches!(set_own(&conn, 999, true), Err(AppError::NotFound)));
    }

    #[test]
    fn stage_change_is_recorded_once() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Dev", "Firma E", "manual")).unwrap());
        set_stage(&mut conn, id, Stage::Applied).unwrap();
        set_stage(&mut conn, id, Stage::Applied).unwrap();
        set_stage(&mut conn, id, Stage::HrInterview).unwrap();
        let d = detail(&conn, id).unwrap();
        let stages: Vec<Stage> = d.history.iter().map(|e| e.stage).collect();
        assert_eq!(stages, vec![Stage::Interested, Stage::Applied, Stage::HrInterview]);
        assert_eq!(d.row.stage, Stage::HrInterview);
    }

    #[test]
    fn no_moves_the_offer_to_its_own_tab_and_back() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("QA Engineer", "Firma B", "adzuna")).unwrap());
        let tabs = |conn: &Connection| {
            let c = counts(conn, None).unwrap();
            (c.board, c.review, c.rejected, c.declined, c.trash)
        };
        assert_eq!(tabs(&conn), (1, 0, 0, 0, 0));

        // "No" wins even over a favorite or a stage - the offer is in "Not for me" only.
        set_interest(&conn, id, Some(Interest::No)).unwrap();
        set_favorite(&conn, id, true).unwrap();
        assert_eq!(tabs(&conn), (0, 0, 0, 1, 0));
        assert_eq!(list(&conn, View::Declined, None).unwrap()[0].id, id);
        // The export knows the tab of every offer and takes one tab as the board does.
        let all = export_rows(&conn, None).unwrap();
        assert!(all.iter().any(|r| r.id == id && r.list == View::Declined));
        assert!(export_rows(&conn, Some(View::Declined)).unwrap().iter().all(|r| r.list == View::Declined));
        assert!(export_rows(&conn, Some(View::Board)).unwrap().iter().all(|r| r.id != id));
        // An own offer is "own" in the list column, whatever the filter says.
        let own = id_of(add(&mut conn, offer("Analityk", "Firma C", "manual")).unwrap());
        assert!(export_rows(&conn, None).unwrap().iter().any(|r| r.id == own && r.list == View::Own));

        // Fetched again: merged into the same offer, which stays where the user put it.
        assert_eq!(add(&mut conn, offer("QA Engineer", "Firma B", "remotive")).unwrap(), AddOutcome::Merged { id });
        assert_eq!(tabs(&conn), (0, 0, 0, 1, 0));

        // Changing the interest brings it back.
        set_interest(&conn, id, None).unwrap();
        assert_eq!(tabs(&conn), (1, 0, 0, 0, 0));
    }

    #[test]
    fn interest_favorite_and_notes() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Dev", "Firma F", "manual")).unwrap());
        set_interest(&conn, id, Some(Interest::High)).unwrap();
        set_favorite(&conn, id, true).unwrap();
        set_notes(&conn, id, "  call on Monday  ".into()).unwrap();
        let d = detail(&conn, id).unwrap();
        assert_eq!(d.row.interest, Some(Interest::High));
        assert!(d.row.favorite);
        assert_eq!(d.notes, "call on Monday");
        assert!(set_notes(&conn, id, "x".repeat(MAX_NOTES + 1)).is_err());
        assert!(matches!(set_interest(&conn, 999, None), Err(AppError::NotFound)));
    }

    #[test]
    fn rejects_unsafe_or_invalid_input() {
        let mut conn = test_db();
        let mut bad_link = offer("Dev", "Firma G", "manual");
        bad_link.url = Some("javascript:alert(1)".into());
        assert!(add(&mut conn, bad_link).is_err());

        assert!(add(&mut conn, offer("   ", "Firma G", "manual")).is_err());
        assert!(add(&mut conn, offer("Dev", "Firma G", "Bad Source!")).is_err());

        let mut negative = offer("Dev", "Firma G", "manual");
        negative.salary_min = Some(-5);
        assert!(add(&mut conn, negative).is_err());

        let mut swapped = offer("Dev", "Firma G", "manual");
        swapped.salary_min = Some(15000);
        swapped.salary_max = Some(10000);
        swapped.url = Some("https://example.com/oferta/1".into());
        swapped.expires_at = Some("2026-12-01T10:00:00+01:00".into());
        let id = id_of(add(&mut conn, swapped).unwrap());
        let d = detail(&conn, id).unwrap();
        assert_eq!((d.row.salary_min, d.row.salary_max), (Some(10000), Some(15000)));
        assert_eq!(d.expires_at.as_deref(), Some("2026-12-01T09:00:00Z"));
    }

    #[test]
    fn publication_dates_in_every_form_the_sources_use() {
        let now = "2026-11-06T12:00:00Z";
        let p = |v: &str| published(Some(v), now);
        assert_eq!(p("2026-11-01T08:00:00Z").as_deref(), Some("2026-11-01T08:00:00Z"));
        assert_eq!(p("2026-11-01T08:00:00+01:00").as_deref(), Some("2026-11-01T07:00:00Z"));
        assert_eq!(p("Mon, 02 Nov 2026 18:18:39 +0000").as_deref(), Some("2026-11-02T18:18:39Z"));
        assert_eq!(p("2026-11-02T10:13:07").as_deref(), Some("2026-11-02T10:13:07Z"));
        assert_eq!(p("2026-11-02T00:00:00.0000000").as_deref(), Some("2026-11-02T00:00:00Z"));
        assert_eq!(p("2026-11-02").as_deref(), Some("2026-11-02T00:00:00Z"));
        assert_eq!(p("1793238550").as_deref(), Some("2026-10-29T01:49:10Z"));
        // From the future - now; broken or empty - no date (the offer is kept).
        assert_eq!(p("2027-01-01T00:00:00Z").as_deref(), Some(now));
        assert_eq!(p("jutro"), None);
        assert_eq!(p("0"), None);
        assert_eq!(published(None, now), None);
        // The very end of the calendar with an offset: dropped, not a crash.
        assert_eq!(p("9999-12-31T23:59:59-23:59"), None);
    }

    /// An offer with its publication date `days` ago.
    fn published_days_ago(conn: &mut Connection, title: &str, source: &str, days: i64) -> i64 {
        let mut o = offer(title, "Firma Testowa", source);
        o.published_at = Some(cutoff(&clock::now(), days));
        id_of(add(conn, o).unwrap())
    }

    #[test]
    fn untouched_old_offers_leave_for_the_archive_the_others_stay_marked() {
        let mut conn = test_db();
        let untouched = published_days_ago(&mut conn, "Tester", "adzuna", 12);
        let fresh = published_days_ago(&mut conn, "Analityk", "adzuna", 3);
        let starred = published_days_ago(&mut conn, "Grafik", "adzuna", 12);
        set_favorite(&conn, starred, true).unwrap();
        let noted = published_days_ago(&mut conn, "Pisarz", "jooble", 12);
        set_notes(&conn, noted, "zadzwonić".into()).unwrap();
        let by_hand = published_days_ago(&mut conn, "Księgowy", "manual", 12);
        let applied = published_days_ago(&mut conn, "Programista", "adzuna", 12);
        set_stage(&mut conn, applied, Stage::Applied).unwrap();

        assert_eq!(archive_old(&conn, &clock::now()).unwrap(), 1);
        let archive: Vec<i64> = list(&conn, View::Archive, None).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(archive, vec![untouched]);
        // Collected and own together (the one added by hand is own).
        let mut board = list(&conn, View::Board, None).unwrap();
        board.extend(list(&conn, View::Own, None).unwrap());
        let stale = |id: i64| board.iter().find(|r| r.id == id).unwrap().stale;
        assert!(!stale(fresh));
        assert!(stale(starred) && stale(noted) && stale(by_hand));
        // In a recruitment: never "probably expired", only its stage.
        assert!(!stale(applied));
        assert!(board.iter().find(|r| r.id == applied).unwrap().in_recruitment);
        assert!(board.iter().find(|r| r.id == by_hand).unwrap().manual);

        // "Bring back": the user wants it - it never leaves by itself again.
        unarchive(&conn, untouched).unwrap();
        assert_eq!(archive_old(&conn, &clock::now()).unwrap(), 0);
        assert!(list(&conn, View::Archive, None).unwrap().is_empty());
        assert!(matches!(unarchive(&conn, untouched), Err(AppError::NotFound)));
    }

    #[test]
    fn expired_by_hand_archives_untouched_and_marks_the_rest() {
        let mut conn = test_db();
        let plain = id_of(add(&mut conn, offer("Tester", "Firma A", "adzuna")).unwrap());
        let starred = id_of(add(&mut conn, offer("Tester", "Firma B", "adzuna")).unwrap());
        set_favorite(&conn, starred, true).unwrap();

        assert_eq!(set_expired(&conn, plain).unwrap(), ExpiredBy::Archived);
        assert_eq!(set_expired(&conn, starred).unwrap(), ExpiredBy::Marked);
        assert_eq!(list(&conn, View::Archive, None).unwrap()[0].id, plain);
        assert!(detail(&conn, starred).unwrap().row.expired);
        // Expired offers are not for today.
        assert!(list(&conn, View::Today, None).unwrap().is_empty());

        unset_expired(&conn, plain).unwrap();
        unset_expired(&conn, starred).unwrap();
        assert_eq!(list(&conn, View::Today, None).unwrap().len(), 2);
        assert!(matches!(set_expired(&conn, 999), Err(AppError::NotFound)));
    }

    /// "Best today" of the collected ones: starred first, then the best scores; the one added by
    /// hand is own (tab "Own"), not here.
    #[test]
    fn today_puts_starred_first_then_the_best_scores() {
        let mut conn = test_db();
        crate::settings::set(&conn, "today_count", "10").unwrap();
        let mut ids = Vec::new();
        for i in 0..14 {
            let id = published_days_ago(&mut conn, &format!("Oferta {i}"), "adzuna", 1);
            conn.execute("UPDATE offers SET score = ?2, score_source = 'ai' WHERE id = ?1", params![id, 30 + i]).unwrap();
            ids.push(id);
        }
        let old = published_days_ago(&mut conn, "Stara", "adzuna", 12);
        conn.execute("UPDATE offers SET score = 99 WHERE id = ?1", [old]).unwrap();
        let by_hand = published_days_ago(&mut conn, "Ręczna", "manual", 1);
        set_favorite(&conn, ids[0], true).unwrap();
        let applied = ids[13];
        set_stage(&mut conn, applied, Stage::Applied).unwrap();

        let today: Vec<i64> = list(&conn, View::Today, None).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(today.len(), 10);
        // Starred first, then the rest by score; old, applied and own ones never.
        assert_eq!(&today[..2], &[ids[0], ids[12]]);
        assert!(!today.contains(&old) && !today.contains(&applied) && !today.contains(&by_hand));
        assert_eq!(counts(&conn, None).unwrap().today, 10);
    }

    #[test]
    fn ai_reads_fresh_offers_first_and_skips_old_ones_nobody_works_on() {
        let mut conn = test_db();
        let old = published_days_ago(&mut conn, "Stara", "adzuna", 15);
        let old_starred = published_days_ago(&mut conn, "Stara z gwiazdką", "adzuna", 15);
        set_favorite(&conn, old_starred, true).unwrap();
        let older = published_days_ago(&mut conn, "Starsza świeża", "adzuna", 5);
        let newest = published_days_ago(&mut conn, "Najnowsza", "adzuna", 1);
        let sql = format!("SELECT o.id FROM offers o WHERE {} ORDER BY {AI_ORDER}", waiting_for_ai(&conn).unwrap());
        let ids: Vec<i64> = conn.prepare(&sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(ids, vec![old_starred, newest, older]);
        assert!(!ids.contains(&old));
    }

    #[test]
    fn safe_url_accepts_only_web_links() {
        assert!(safe_url(Some("https://example.com".into())).unwrap().is_some());
        assert!(safe_url(Some("HTTP://example.com/a?b=1".into())).unwrap().is_some());
        assert!(safe_url(Some("file:///C:/x".into())).is_err());
        assert!(safe_url(Some("https://exa mple.com".into())).is_err());
        assert!(safe_url(Some("https://".into())).is_err());
        assert!(safe_url(Some("".into())).unwrap().is_none());
    }
}
