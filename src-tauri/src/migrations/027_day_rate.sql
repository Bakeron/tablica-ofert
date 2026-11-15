-- Version 27 (15.11.2026): B2B day rates ("1200 zł / dzień") get their own period - the CHECK on
-- `salary_period` is widened in `db.rs` (`allow_day_rate`), without rebuilding the table.
-- `english` = "English required?" read from the offer's words (`english.rs`): "required",
-- "nice" or "none"; filled by the filter check, NULL until then.
ALTER TABLE offers ADD COLUMN english TEXT;
