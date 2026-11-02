-- Version 7: everything needed for PC - phone sync through Google Drive.
-- Contacts and contact history get stable ids and change times; deletions leave tombstones.

ALTER TABLE contacts ADD COLUMN uid TEXT;
ALTER TABLE contacts ADD COLUMN updated_at TEXT;
UPDATE contacts SET uid = lower(hex(randomblob(16))), updated_at = created_at WHERE uid IS NULL;
CREATE UNIQUE INDEX contacts_uid ON contacts (uid);

ALTER TABLE contact_log ADD COLUMN uid TEXT;
UPDATE contact_log SET uid = lower(hex(randomblob(16))) WHERE uid IS NULL;
CREATE UNIQUE INDEX contact_log_uid ON contact_log (uid);

-- Deleted rows (offers deleted for good, profiles, dates, contacts, history entries):
-- the other device must not bring them back.
CREATE TABLE sync_deleted (
    kind TEXT NOT NULL,                         -- 'offers', 'profiles', 'events', 'contacts', 'log'
    uid  TEXT NOT NULL,
    at   TEXT NOT NULL,
    PRIMARY KEY (kind, uid)
);
