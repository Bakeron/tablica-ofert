//! Local diagnostic log (`zdarzenia.log` next to the database): one line per start with the
//! mode, the version and whether the database key was in Credential Manager and in the copy.
//! Added after the key once vanished from Credential Manager for an unknown reason - the log
//! narrows down when it happens. Never contains keys, offers or anything personal.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub const FILE: &str = "zdarzenia.log";
/// The previous log, kept when the log starts again.
pub const OLD_FILE: &str = "zdarzenia.old.log";
/// Above this size the log starts again (the previous one is kept as `OLD_FILE`).
const MAX_BYTES: u64 = 256 * 1024;

/// Appends one line; failures are ignored - the log must never stop the app.
pub fn log(data_dir: &Path, line: &str) {
    let file = data_dir.join(FILE);
    if fs::metadata(&file).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = fs::rename(&file, data_dir.join(OLD_FILE));
    }
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let stamp = now
        .format(time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory]"))
        .unwrap_or_default();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(f, "{stamp} | {} | {line}", env!("CARGO_PKG_VERSION"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_lines_with_version() {
        let dir = crate::db::tests::temp_dir("diag");
        log(&dir, "start");
        log(&dir, "again");
        let text = fs::read_to_string(dir.join(FILE)).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains(env!("CARGO_PKG_VERSION")) && text.contains("| again"));
    }
}
