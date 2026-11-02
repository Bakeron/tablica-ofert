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

/// Timestamp for backup file names, e.g. `20261102-183005`.
pub fn file_stamp() -> String {
    OffsetDateTime::now_utc()
        .format(format_description!("[year][month][day]-[hour][minute][second]"))
        .unwrap_or_default()
}
