use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;

/// Current time in UTC as RFC 3339 (the format stored in the database).
pub fn now() -> String {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .unwrap_or_default()
}

/// Today in the computer's own time zone (YYYY-MM-DD) - for counts the person reads as "today"
/// (Claude's spending): in Poland a day starts at midnight, not at 1:00 or 2:00 like a UTC day.
/// UTC when the zone cannot be read.
pub fn local_day() -> String {
    OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

/// Timestamp for backup file names, e.g. `20261102-183005`.
pub fn file_stamp() -> String {
    OffsetDateTime::now_utc()
        .format(format_description!("[year][month][day]-[hour][minute][second]"))
        .unwrap_or_default()
}
