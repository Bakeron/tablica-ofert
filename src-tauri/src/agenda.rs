//! Dates of an offer (interviews, task deadlines), contacts, contact history, reminders and
//! "Add to calendar" (a Google Calendar link or an `.ics` file) - no Google login needed.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Duration, OffsetDateTime};

use crate::clock;
use crate::error::{AppError, AppResult};
use crate::text::{cut, one_line};

const MAX_NOTE: usize = 2_000;
const MAX_FIELD: usize = 200;
/// Offers scored at least this high by AI get a notification.
pub const HIGH_SCORE: i64 = 80;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: i64,
    pub offer_id: i64,
    pub kind: String,
    pub at: String,
    pub minutes: i64,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventInput {
    pub kind: String,
    /// RFC 3339 (the UI sends UTC).
    pub at: String,
    #[serde(default = "default_minutes")]
    pub minutes: i64,
    #[serde(default)]
    pub note: String,
}

fn default_minutes() -> i64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Contact {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub email: String,
    pub phone: String,
    pub linkedin: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: i64,
    pub at: String,
    pub kind: String,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogInput {
    pub at: String,
    pub kind: String,
    #[serde(default)]
    pub note: String,
}

/// Everything the offer card shows in "Dates" and "Contacts".
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agenda {
    pub events: Vec<Event>,
    pub contacts: Vec<Contact>,
    pub log: Vec<LogEntry>,
}

// ---------- checks ----------

fn parse_time(value: &str) -> AppResult<OffsetDateTime> {
    OffsetDateTime::parse(value.trim(), &Rfc3339).map_err(|_| AppError::Invalid("date has a wrong format".into()))
}

fn utc(t: OffsetDateTime) -> String {
    t.to_offset(time::UtcOffset::UTC).replace_nanosecond(0).unwrap_or(t).format(&Rfc3339).unwrap_or_default()
}

fn note(value: &str) -> AppResult<String> {
    let v = value.trim();
    if v.chars().count() > MAX_NOTE || v.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t') {
        return Err(AppError::Invalid("note is too long or has control characters".into()));
    }
    Ok(v.to_string())
}

fn field(value: &str) -> String {
    one_line(value, MAX_FIELD)
}

fn offer_exists(conn: &Connection, offer_id: i64) -> AppResult<()> {
    let found: Option<i64> = conn.query_row("SELECT id FROM offers WHERE id = ?1", [offer_id], |r| r.get(0)).optional()?;
    found.map(|_| ()).ok_or(AppError::NotFound)
}

// ---------- dates ----------

pub fn add_event(conn: &Connection, offer_id: i64, input: EventInput) -> AppResult<i64> {
    offer_exists(conn, offer_id)?;
    if !["interview", "deadline", "other"].contains(&input.kind.as_str()) {
        return Err(AppError::Invalid("unknown kind of date".into()));
    }
    let at = utc(parse_time(&input.at)?);
    let minutes = input.minutes.clamp(5, 24 * 60);
    conn.execute(
        "INSERT INTO events (uid, offer_id, kind, at, minutes, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![crate::secrets::random_hex(16)?, offer_id, input.kind, at, minutes, note(&input.note)?, clock::now()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Deletes a row and leaves a tombstone, so sync removes it on the other device too.
fn delete_synced(conn: &Connection, table: &str, kind: &str, id: i64) -> AppResult<()> {
    let uid: Option<String> = conn.query_row(&format!("SELECT uid FROM {table} WHERE id = ?1"), [id], |r| r.get(0)).optional()?;
    let uid = uid.ok_or(AppError::NotFound)?;
    conn.execute(&format!("DELETE FROM {table} WHERE id = ?1"), [id])?;
    crate::sync::data::tombstone(conn, kind, &uid, &clock::now())
}

pub fn delete_event(conn: &Connection, id: i64) -> AppResult<()> {
    delete_synced(conn, "events", "events", id)
}

fn event(conn: &Connection, id: i64) -> AppResult<Event> {
    conn.query_row("SELECT id, offer_id, kind, at, minutes, note FROM events WHERE id = ?1", [id], event_from)
        .optional()?
        .ok_or(AppError::NotFound)
}

fn event_from(r: &rusqlite::Row) -> rusqlite::Result<Event> {
    Ok(Event { id: r.get(0)?, offer_id: r.get(1)?, kind: r.get(2)?, at: r.get(3)?, minutes: r.get(4)?, note: r.get(5)? })
}

// ---------- contacts and history ----------

pub fn save_contact(conn: &Connection, offer_id: i64, c: Contact) -> AppResult<i64> {
    offer_exists(conn, offer_id)?;
    let name = field(&c.name);
    if name.is_empty() {
        return Err(AppError::Invalid("name is required".into()));
    }
    let linkedin = field(&c.linkedin);
    if !linkedin.is_empty() && crate::offers::safe_url(Some(linkedin.clone())).is_err() {
        return Err(AppError::Invalid("the link must start with http:// or https://".into()));
    }
    let values = (name, field(&c.role), field(&c.email), field(&c.phone), linkedin);
    if c.id > 0 {
        let n = conn.execute(
            "UPDATE contacts SET name = ?3, role = ?4, email = ?5, phone = ?6, linkedin = ?7, updated_at = ?8
             WHERE id = ?1 AND offer_id = ?2",
            params![c.id, offer_id, values.0, values.1, values.2, values.3, values.4, clock::now()],
        )?;
        return if n == 0 { Err(AppError::NotFound) } else { Ok(c.id) };
    }
    conn.execute(
        "INSERT INTO contacts (uid, offer_id, name, role, email, phone, linkedin, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![crate::secrets::random_hex(16)?, offer_id, values.0, values.1, values.2, values.3, values.4, clock::now()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_contact(conn: &Connection, id: i64) -> AppResult<()> {
    delete_synced(conn, "contacts", "contacts", id)
}

pub fn add_log(conn: &Connection, offer_id: i64, input: LogInput) -> AppResult<i64> {
    offer_exists(conn, offer_id)?;
    if !["call", "mail", "meeting", "message"].contains(&input.kind.as_str()) {
        return Err(AppError::Invalid("unknown kind of contact".into()));
    }
    let at = utc(parse_time(&input.at)?);
    conn.execute(
        "INSERT INTO contact_log (uid, offer_id, at, kind, note) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![crate::secrets::random_hex(16)?, offer_id, at, input.kind, note(&input.note)?],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_log(conn: &Connection, id: i64) -> AppResult<()> {
    delete_synced(conn, "contact_log", "log", id)
}

pub fn agenda(conn: &Connection, offer_id: i64) -> AppResult<Agenda> {
    let mut stmt = conn.prepare("SELECT id, offer_id, kind, at, minutes, note FROM events WHERE offer_id = ?1 ORDER BY at")?;
    let events = stmt.query_map([offer_id], event_from)?.collect::<Result<_, _>>()?;
    let mut stmt = conn.prepare("SELECT id, name, role, email, phone, linkedin FROM contacts WHERE offer_id = ?1 ORDER BY id")?;
    let contacts = stmt
        .query_map([offer_id], |r| {
            Ok(Contact { id: r.get(0)?, name: r.get(1)?, role: r.get(2)?, email: r.get(3)?, phone: r.get(4)?, linkedin: r.get(5)? })
        })?
        .collect::<Result<_, _>>()?;
    let mut stmt = conn.prepare("SELECT id, at, kind, note FROM contact_log WHERE offer_id = ?1 ORDER BY at DESC, id DESC")?;
    let log = stmt
        .query_map([offer_id], |r| Ok(LogEntry { id: r.get(0)?, at: r.get(1)?, kind: r.get(2)?, note: r.get(3)? }))?
        .collect::<Result<_, _>>()?;
    Ok(Agenda { events, contacts, log })
}

// ---------- calendar ----------

fn compact(t: OffsetDateTime) -> String {
    t.to_offset(time::UtcOffset::UTC)
        .format(format_description!("[year][month][day]T[hour][minute][second]Z"))
        .unwrap_or_default()
}

fn offer_title(conn: &Connection, offer_id: i64) -> AppResult<(String, String, Option<String>)> {
    Ok(conn.query_row("SELECT title, company, url FROM offers WHERE id = ?1", [offer_id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })?)
}

fn event_title(kind: &str, title: &str, company: &str, pl: bool) -> String {
    let label = match (kind, pl) {
        ("interview", true) => "Rozmowa",
        ("interview", false) => "Interview",
        ("deadline", true) => "Termin zadania",
        ("deadline", false) => "Task deadline",
        (_, true) => "Termin",
        (_, false) => "Date",
    };
    format!("{label}: {title} - {company}")
}

/// Google Calendar "new event" page with everything filled in - the person clicks "Save".
pub fn google_link(conn: &Connection, event_id: i64, pl: bool) -> AppResult<String> {
    let e = event(conn, event_id)?;
    let (title, company, url) = offer_title(conn, e.offer_id)?;
    let start = parse_time(&e.at)?;
    let end = start + Duration::minutes(e.minutes);
    let mut details = e.note.clone();
    if let Some(u) = &url {
        details = format!("{details}\n\n{u}").trim().to_string();
    }
    let mut link = reqwest::Url::parse("https://calendar.google.com/calendar/render").expect("static address");
    link.query_pairs_mut()
        .append_pair("action", "TEMPLATE")
        .append_pair("text", &event_title(&e.kind, &title, &company, pl))
        .append_pair("dates", &format!("{}/{}", compact(start), compact(end)))
        .append_pair("details", &cut(&details, 1500));
    Ok(link.to_string())
}

/// Text value escaped for iCalendar (RFC 5545): backslash, semicolon, comma, new line.
fn ics_text(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\r', "").replace('\n', "\\n")
}

/// Lines longer than 75 bytes are folded, as RFC 5545 asks.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut len = 0;
    for ch in line.chars() {
        let n = ch.len_utf8();
        if len + n > 75 {
            out.push_str("\r\n ");
            len = 1;
        }
        out.push(ch);
        len += n;
    }
    out
}

/// An `.ics` file (Outlook, phone calendars) with an alarm an hour before.
pub fn ics(conn: &Connection, event_id: i64, pl: bool) -> AppResult<String> {
    let e = event(conn, event_id)?;
    let (title, company, url) = offer_title(conn, e.offer_id)?;
    let start = parse_time(&e.at)?;
    let end = start + Duration::minutes(e.minutes);
    let uid: String = conn.query_row("SELECT uid FROM events WHERE id = ?1", [event_id], |r| r.get(0))?;
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//Tablica Ofert//PL".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{uid}@tablica-ofert"),
        format!("DTSTAMP:{}", compact(OffsetDateTime::now_utc())),
        format!("DTSTART:{}", compact(start)),
        format!("DTEND:{}", compact(end)),
        format!("SUMMARY:{}", ics_text(&event_title(&e.kind, &title, &company, pl))),
    ];
    if !e.note.is_empty() {
        lines.push(format!("DESCRIPTION:{}", ics_text(&e.note)));
    }
    if let Some(u) = url {
        lines.push(format!("URL:{}", ics_text(&u)));
    }
    lines.extend([
        "BEGIN:VALARM".into(),
        "TRIGGER:-PT60M".into(),
        "ACTION:DISPLAY".into(),
        format!("DESCRIPTION:{}", ics_text(&title)),
        "END:VALARM".into(),
        "END:VEVENT".into(),
        "END:VCALENDAR".into(),
    ]);
    Ok(lines.iter().map(|l| fold(l)).collect::<Vec<_>>().join("\r\n") + "\r\n")
}

// ---------- reminders ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Reminder {
    pub kind: &'static str,
    pub title: String,
    pub company: String,
    /// For dates: when (UTC).
    pub at: Option<String>,
    pub event_kind: Option<String>,
    pub score: Option<i64>,
}

fn sent(conn: &Connection, kind: &str, subject: i64, mark: &str) -> AppResult<bool> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO reminders_sent (kind, subject, mark, sent_at) VALUES (?1, ?2, ?3, ?4)",
        params![kind, subject, mark, clock::now()],
    )?;
    // 0 = it was there already.
    Ok(n == 0)
}

/// Reminders due at `now`; each is returned once and then remembered.
/// Interviews and deadlines: a day before (from 24 h before) and an hour before;
/// follow-up: 7 days after "applied" with no later stage; new offers scored 80+ by AI.
pub fn due(conn: &Connection, now: OffsetDateTime) -> AppResult<Vec<Reminder>> {
    let mut out = Vec::new();
    let now_s = utc(now);
    let day = utc(now + Duration::hours(24));
    let hour = utc(now + Duration::minutes(65));
    let mut stmt = conn.prepare(
        "SELECT e.id, e.kind, e.at, o.title, o.company FROM events e JOIN offers o ON o.id = e.offer_id
         WHERE e.at > ?1 AND e.at <= ?2 AND o.deleted_at IS NULL ORDER BY e.at",
    )?;
    let rows: Vec<(i64, String, String, String, String)> = stmt
        .query_map(params![now_s, day], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<Result<_, _>>()?;
    for (id, kind, at, title, company) in rows {
        let soon = at <= hour;
        let which = if soon { "hour_before" } else { "day_before" };
        // An hour before also covers "a day before" - one notification is enough.
        if soon {
            sent(conn, "day_before", id, &at)?;
        }
        if !sent(conn, which, id, &at)? {
            out.push(Reminder { kind: which, title, company, at: Some(at), event_kind: Some(kind), score: None });
        }
    }
    let week_ago = utc(now - Duration::days(7));
    let mut stmt = conn.prepare(
        "SELECT id, title, company, stage_changed_at FROM offers
         WHERE stage = 'applied' AND stage_changed_at <= ?1 AND deleted_at IS NULL",
    )?;
    let rows: Vec<(i64, String, String, String)> =
        stmt.query_map([week_ago], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
    for (id, title, company, changed) in rows {
        if !sent(conn, "followup", id, &changed)? {
            out.push(Reminder { kind: "followup", title, company, at: None, event_kind: None, score: None });
        }
    }
    let mut stmt = conn.prepare(
        "SELECT id, title, company, score FROM offers
         WHERE score_source = 'ai' AND score >= ?1 AND stage = 'new' AND deleted_at IS NULL AND filter_result != 'fail'",
    )?;
    let rows: Vec<(i64, String, String, i64)> =
        stmt.query_map([HIGH_SCORE], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
    for (id, title, company, score) in rows {
        if !sent(conn, "high_score", id, "")? {
            out.push(Reminder { kind: "high_score", title, company, at: None, event_kind: None, score: Some(score) });
        }
    }
    Ok(out)
}

/// Title and body of a notification. `local_time` turns a UTC time into the person's clock.
pub fn reminder_text(r: &Reminder, pl: bool, local_time: impl Fn(&str) -> String) -> (String, String) {
    let what = format!("{} - {}", r.title, r.company);
    let interview = r.event_kind.as_deref() == Some("interview");
    match (r.kind, pl) {
        ("hour_before", true) => (
            if interview { "Rozmowa za godzinę" } else { "Termin za godzinę" }.into(),
            format!("{what}, {}", r.at.as_deref().map(&local_time).unwrap_or_default()),
        ),
        ("hour_before", false) => (
            if interview { "Interview in an hour" } else { "Date in an hour" }.into(),
            format!("{what}, {}", r.at.as_deref().map(&local_time).unwrap_or_default()),
        ),
        ("day_before", true) => (
            if interview { "Jutro rozmowa" } else { "Jutro termin" }.into(),
            format!("{what}, {}", r.at.as_deref().map(&local_time).unwrap_or_default()),
        ),
        ("day_before", false) => (
            if interview { "Interview tomorrow" } else { "Date tomorrow" }.into(),
            format!("{what}, {}", r.at.as_deref().map(&local_time).unwrap_or_default()),
        ),
        ("followup", true) => ("Brak odpowiedzi od 7 dni".into(), format!("{what} - może czas napisać follow-up?")),
        ("followup", false) => ("No reply for 7 days".into(), format!("{what} - time for a follow-up?")),
        (_, true) => ("Oferta z wysoką oceną".into(), format!("{what} ({})", r.score.unwrap_or_default())),
        (_, false) => ("Highly scored offer".into(), format!("{what} ({})", r.score.unwrap_or_default())),
    }
}

/// Local time of a UTC timestamp, e.g. "5.11 14:00" - the computer's offset.
pub fn local_label(at: &str) -> String {
    let Ok(t) = OffsetDateTime::parse(at, &Rfc3339) else { return at.to_string() };
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    t.to_offset(offset)
        .format(format_description!("[day padding:none].[month] [hour]:[minute]"))
        .unwrap_or_else(|_| at.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::model::{AddOutcome, OfferInput, Stage};
    use time::macros::datetime;

    fn offer(conn: &mut Connection) -> i64 {
        match crate::offers::add(
            conn,
            OfferInput { title: "Frontend Developer".into(), company: "Firma, Testowa; Sp".into(), source: "manual".into(), url: Some("https://example.com/o/1".into()), ..Default::default() },
        )
        .unwrap()
        {
            AddOutcome::Added { id } => id,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn dates_contacts_and_history() {
        let mut conn = test_db();
        let id = offer(&mut conn);
        let e = add_event(&conn, id, EventInput { kind: "interview".into(), at: "2026-11-05T14:00:00+01:00".into(), minutes: 45, note: "Teams".into() }).unwrap();
        assert!(add_event(&conn, id, EventInput { kind: "party".into(), at: "2026-11-05T14:00:00Z".into(), minutes: 60, note: String::new() }).is_err());
        assert!(add_event(&conn, id, EventInput { kind: "other".into(), at: "jutro".into(), minutes: 60, note: String::new() }).is_err());
        save_contact(&conn, id, Contact { name: "Anna Zmyślona".into(), role: "Rekruterka".into(), ..Default::default() }).unwrap();
        assert!(save_contact(&conn, id, Contact { name: "X".into(), linkedin: "javascript:alert(1)".into(), ..Default::default() }).is_err());
        add_log(&conn, id, LogInput { at: "2026-11-02T10:00:00Z".into(), kind: "call".into(), note: "Krótka rozmowa".into() }).unwrap();
        let a = agenda(&conn, id).unwrap();
        assert_eq!(a.events[0].at, "2026-11-05T13:00:00Z");
        assert_eq!(a.events[0].minutes, 45);
        assert_eq!(a.contacts[0].role, "Rekruterka");
        assert_eq!(a.log.len(), 1);
        delete_event(&conn, e).unwrap();
        assert!(agenda(&conn, id).unwrap().events.is_empty());
    }

    #[test]
    fn calendar_link_and_file() {
        let mut conn = test_db();
        let id = offer(&mut conn);
        let e = add_event(&conn, id, EventInput { kind: "interview".into(), at: "2026-11-05T13:00:00Z".into(), minutes: 60, note: "Link do spotkania: w mailu".into() }).unwrap();
        let link = google_link(&conn, e, true).unwrap();
        assert!(link.starts_with("https://calendar.google.com/calendar/render?action=TEMPLATE"));
        assert!(link.contains("dates=20261105T130000Z%2F20261105T140000Z"));
        let file = ics(&conn, e, true).unwrap();
        assert!(file.contains("DTSTART:20261105T130000Z\r\n"));
        assert!(file.contains("SUMMARY:Rozmowa: Frontend Developer - Firma\\, Testowa\\; Sp"));
        assert!(file.lines().all(|l| l.len() <= 76));
        assert!(file.ends_with("END:VCALENDAR\r\n"));
    }

    #[test]
    fn reminders_come_once() {
        let mut conn = test_db();
        let id = offer(&mut conn);
        add_event(&conn, id, EventInput { kind: "interview".into(), at: "2026-11-05T13:00:00Z".into(), minutes: 60, note: String::new() }).unwrap();
        // Two days before: nothing yet.
        assert!(due(&conn, datetime!(2026-11-03 12:00 UTC)).unwrap().is_empty());
        // The evening before: "tomorrow", once.
        let r = due(&conn, datetime!(2026-11-04 18:00 UTC)).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, "day_before");
        assert!(due(&conn, datetime!(2026-11-04 19:00 UTC)).unwrap().is_empty());
        // An hour before: once more.
        let r = due(&conn, datetime!(2026-11-05 12:10 UTC)).unwrap();
        assert_eq!(r[0].kind, "hour_before");
        assert!(due(&conn, datetime!(2026-11-05 12:30 UTC)).unwrap().is_empty());

        crate::offers::set_stage(&mut conn, id, Stage::Applied).unwrap();
        let later = OffsetDateTime::now_utc() + Duration::days(8);
        let r = due(&conn, later).unwrap();
        assert_eq!(r.iter().filter(|x| x.kind == "followup").count(), 1);
        assert!(due(&conn, later).unwrap().is_empty());
        let (title, body) = reminder_text(&r[0], true, |s| s.to_string());
        assert_eq!(title, "Brak odpowiedzi od 7 dni");
        assert!(body.contains("follow-up"));
    }

    #[test]
    fn ics_folding() {
        let long = "SUMMARY:".to_string() + &"ż".repeat(60);
        assert!(fold(&long).split("\r\n").all(|l| l.len() <= 75));
    }
}
