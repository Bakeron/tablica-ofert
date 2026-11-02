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

const MAX_SHORT: usize = 300;
const MAX_URL: usize = 2_000;
const MAX_LONG: usize = 100_000;
const MAX_NOTES: usize = 20_000;
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
    parsed
        .to_offset(time::UtcOffset::UTC)
        .format(&Rfc3339)
        .map(Some)
        .map_err(|_| AppError::Invalid("date has a wrong format".into()))
}

/// Validated and trimmed copy of the input.
fn clean(input: OfferInput) -> AppResult<OfferInput> {
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
    })
}

// ---------- adding ----------

pub fn add(conn: &mut Connection, input: OfferInput) -> AppResult<AddOutcome> {
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

    let existing: Option<(i64, bool)> = tx
        .query_row(
            "SELECT id, deleted_at IS NOT NULL FROM offers WHERE fingerprint = ?1",
            [&fingerprint],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let outcome = match existing {
        // In the trash: only the user brings it back.
        Some((_, true)) => return Ok(AddOutcome::Blocked),
        Some((id, false)) => {
            merge_into(&tx, id, &input, &now)?;
            AddOutcome::Merged { id }
        }
        None => {
            let uid = crate::secrets::random_hex(16)?;
            tx.execute(
                "INSERT INTO offers (uid, fingerprint, title, company, url, city, remote,
                    salary_min, salary_max, salary_currency, salary_period, contract, level, language,
                    requirements, description, expires_at, stage, stage_changed_at, added_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                    'new', ?18, ?18, ?18)",
                params![
                    uid, fingerprint, input.title, input.company, input.url, input.city,
                    input.remote.unwrap_or(Remote::Unknown), input.salary_min, input.salary_max,
                    input.salary_currency, input.salary_period, input.contract, input.level,
                    input.language, input.requirements, input.description, input.expires_at, now
                ],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO stage_history (offer_id, stage, at) VALUES (?1, 'new', ?2)",
                params![id, now],
            )?;
            AddOutcome::Added { id }
        }
    };

    let id = match outcome {
        AddOutcome::Added { id } | AddOutcome::Merged { id } => id,
        AddOutcome::Blocked => unreachable!(),
    };
    tx.execute(
        "INSERT OR IGNORE INTO offer_sources (offer_id, source, url, first_seen) VALUES (?1, ?2, ?3, ?4)",
        params![id, input.source, input.url, now],
    )?;
    // New or merged data - check it against every profile before it shows up anywhere.
    profiles::recheck_offer(&tx, id)?;
    tx.commit()?;
    Ok(outcome)
}

/// Same offer seen again: fill in what was missing, keep the longer texts.
fn merge_into(tx: &rusqlite::Transaction, id: i64, input: &OfferInput, now: &str) -> AppResult<()> {
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
            city = COALESCE(city, ?3),
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
            expired = 0,
            updated_at = ?11
         WHERE id = ?1",
        params![
            id, input.url, input.city, input.remote, input.contract, input.level, input.language,
            input.requirements, input.description, input.expires_at, now
        ],
    )?;
    Ok(())
}

// ---------- reading ----------

/// Columns of a board row. `{result}` and `{reasons}` depend on the chosen profile (see `filter_sql`).
const ROW_COLUMNS: &str = "o.id, o.title, o.company, o.city, o.remote, o.salary_min, o.salary_max,
    o.salary_currency, o.salary_period, o.stage, o.stage_changed_at, o.interest, o.favorite,
    o.score, o.score_source,
    (SELECT group_concat(s.source, ',') FROM offer_sources s WHERE s.offer_id = o.id),
    o.expired, {result}, o.added_at, {reasons}";

/// The user already acted on these offers - they stay on the board whatever the filter says.
const KEPT: &str = "(o.stage != 'new' OR o.favorite = 1 OR COALESCE(o.interest, '') IN ('high', 'maybe'))";

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

fn columns(profile: Option<i64>) -> String {
    let (result, reasons) = filter_sql(profile);
    ROW_COLUMNS.replace("{result}", result).replace("{reasons}", reasons)
}

fn row_from(r: &Row) -> rusqlite::Result<OfferRow> {
    let sources: Option<String> = r.get(15)?;
    let mut sources: Vec<String> = sources
        .map(|s| s.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    sources.sort();
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
    })
}

fn view_condition(view: View, result: &str) -> String {
    match view {
        View::Board => format!("o.deleted_at IS NULL AND ({result} = 'pass' OR {KEPT})"),
        View::Review => format!("o.deleted_at IS NULL AND {result} = 'unsure' AND NOT {KEPT}"),
        View::Rejected => format!("o.deleted_at IS NULL AND {result} = 'fail' AND NOT {KEPT}"),
        View::Trash => "o.deleted_at IS NOT NULL".to_string(),
    }
}

/// Offers of one tab. `profile = None` - all profiles together (one fitting profile is enough).
pub fn list(conn: &Connection, view: View, profile: Option<i64>) -> AppResult<Vec<OfferRow>> {
    let (result, _) = filter_sql(profile);
    let sql = format!(
        "SELECT {} FROM offers o WHERE {} ORDER BY o.added_at DESC, o.id DESC",
        columns(profile),
        view_condition(view, result)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = match profile {
        Some(p) => stmt.query_map([p], row_from)?.collect::<Result<Vec<_>, _>>()?,
        None => stmt.query_map([], row_from)?.collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// Number of offers in every tab, for the tab labels.
pub fn counts(conn: &Connection, profile: Option<i64>) -> AppResult<ViewCounts> {
    let (result, _) = filter_sql(profile);
    let count = |view| format!("COALESCE(SUM(CASE WHEN {} THEN 1 ELSE 0 END), 0)", view_condition(view, result));
    let sql = format!(
        "SELECT {}, {}, {}, {} FROM offers o",
        count(View::Board),
        count(View::Review),
        count(View::Rejected),
        count(View::Trash)
    );
    let read = |r: &Row| Ok(ViewCounts { board: r.get(0)?, review: r.get(1)?, rejected: r.get(2)?, trash: r.get(3)? });
    Ok(match profile {
        Some(p) => conn.query_row(&sql, [p], read)?,
        None => conn.query_row(&sql, [], read)?,
    })
}

pub fn detail(conn: &Connection, id: i64) -> AppResult<OfferDetail> {
    let sql = format!(
        "SELECT {}, o.url, o.contract, o.level, o.language, o.requirements, o.description,
            o.notes, o.expires_at, o.deleted_at
         FROM offers o WHERE o.id = ?1",
        columns(None)
    );
    let mut detail = conn
        .query_row(&sql, [id], |r| {
            Ok(OfferDetail {
                row: row_from(r)?,
                url: r.get(20)?,
                contract: r.get(21)?,
                level: r.get(22)?,
                language: r.get(23)?,
                requirements: r.get(24)?,
                description: r.get(25)?,
                notes: r.get(26)?,
                expires_at: r.get(27)?,
                deleted_at: r.get(28)?,
                links: Vec::new(),
                history: Vec::new(),
                filters: Vec::new(),
            })
        })
        .optional()?
        .ok_or(AppError::NotFound)?;

    let mut stmt = conn.prepare(
        "SELECT source, url, first_seen FROM offer_sources WHERE offer_id = ?1 ORDER BY first_seen, source",
    )?;
    detail.links = stmt
        .query_map([id], |r| Ok(SourceLink { source: r.get(0)?, url: r.get(1)?, first_seen: r.get(2)? }))?
        .collect::<Result<_, _>>()?;

    let mut stmt = conn.prepare("SELECT stage, at FROM stage_history WHERE offer_id = ?1 ORDER BY at, id")?;
    detail.history = stmt
        .query_map([id], |r| Ok(StageEvent { stage: r.get(0)?, at: r.get(1)? }))?
        .collect::<Result<_, _>>()?;
    detail.filters = profiles::offer_filters(conn, id)?;
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
        tx.execute(
            "UPDATE offers SET stage = ?2, stage_changed_at = ?3, updated_at = ?3 WHERE id = ?1",
            params![id, stage, now],
        )?;
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
        "UPDATE offers SET interest = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, interest, clock::now()],
    )?)
}

pub fn set_favorite(conn: &Connection, id: i64, favorite: bool) -> AppResult<()> {
    changed(conn.execute(
        "UPDATE offers SET favorite = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, favorite, clock::now()],
    )?)
}

pub fn set_notes(conn: &Connection, id: i64, notes: String) -> AppResult<()> {
    let notes = text(Some(notes), MAX_NOTES, "notes")?.unwrap_or_default();
    changed(conn.execute(
        "UPDATE offers SET notes = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, notes, clock::now()],
    )?)
}

pub fn trash(conn: &Connection, id: i64) -> AppResult<()> {
    let now = clock::now();
    changed(conn.execute(
        "UPDATE offers SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
        params![id, now],
    )?)
}

pub fn restore(conn: &Connection, id: i64) -> AppResult<()> {
    changed(conn.execute(
        "UPDATE offers SET deleted_at = NULL, updated_at = ?2 WHERE id = ?1 AND deleted_at IS NOT NULL",
        params![id, clock::now()],
    )?)
}

/// Deletes offers from the trash for good. Their fingerprints are remembered,
/// so alerts and aggregators never bring them back. `None` = empty the whole trash.
pub fn purge(conn: &mut Connection, id: Option<i64>) -> AppResult<usize> {
    let now = clock::now();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO removed_offers (fingerprint, removed_at)
         SELECT fingerprint, ?2 FROM offers WHERE deleted_at IS NOT NULL AND (?1 IS NULL OR id = ?1)",
        params![id, now],
    )?;
    let count = tx.execute(
        "DELETE FROM offers WHERE deleted_at IS NOT NULL AND (?1 IS NULL OR id = ?1)",
        params![id],
    )?;
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
            AddOutcome::Added { id } | AddOutcome::Merged { id } => id,
            AddOutcome::Blocked => panic!("unexpected Blocked"),
        }
    }

    #[test]
    fn adds_and_lists_an_offer_with_history() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Frontend Developer", "Firma Testowa", "manual")).unwrap());
        let rows = list(&conn, View::Board, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].stage, Stage::New);
        assert_eq!(rows[0].sources, vec!["manual"]);
        let d = detail(&conn, id).unwrap();
        assert_eq!(d.history.len(), 1);
        assert_eq!(d.notes, "");
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
    fn trashed_and_purged_offers_do_not_come_back() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("QA Engineer", "Firma B", "adzuna")).unwrap());
        trash(&conn, id).unwrap();
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 0);
        assert_eq!(list(&conn, View::Trash, None).unwrap().len(), 1);
        assert_eq!(add(&mut conn, offer("QA Engineer", "Firma B", "remotive")).unwrap(), AddOutcome::Blocked);

        assert_eq!(purge(&mut conn, Some(id)).unwrap(), 1);
        assert_eq!(list(&conn, View::Trash, None).unwrap().len(), 0);
        assert_eq!(add(&mut conn, offer("QA Engineer", "Firma B", "adzuna")).unwrap(), AddOutcome::Blocked);
    }

    #[test]
    fn purge_only_touches_the_trash() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Dev", "Firma C", "manual")).unwrap());
        assert!(matches!(purge(&mut conn, Some(id)), Err(AppError::NotFound)));
        assert_eq!(purge(&mut conn, None).unwrap(), 0);
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 1);
    }

    #[test]
    fn restore_brings_offer_back_to_the_board() {
        let mut conn = test_db();
        let id = id_of(add(&mut conn, offer("Dev", "Firma D", "manual")).unwrap());
        trash(&conn, id).unwrap();
        restore(&conn, id).unwrap();
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 1);
        assert!(matches!(restore(&conn, id), Err(AppError::NotFound)));
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
        assert_eq!(stages, vec![Stage::New, Stage::Applied, Stage::HrInterview]);
        assert_eq!(d.row.stage, Stage::HrInterview);
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
    fn safe_url_accepts_only_web_links() {
        assert!(safe_url(Some("https://example.com".into())).unwrap().is_some());
        assert!(safe_url(Some("HTTP://example.com/a?b=1".into())).unwrap().is_some());
        assert!(safe_url(Some("file:///C:/x".into())).is_err());
        assert!(safe_url(Some("https://exa mple.com".into())).is_err());
        assert!(safe_url(Some("https://".into())).is_err());
        assert!(safe_url(Some("".into())).unwrap().is_none());
    }
}
