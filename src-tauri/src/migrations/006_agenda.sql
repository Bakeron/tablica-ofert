-- Version 6: dates (interviews, task deadlines), contacts and contact history of an offer,
-- and which reminders were already shown (each one only once).

CREATE TABLE events (
    id         INTEGER PRIMARY KEY,
    uid        TEXT NOT NULL UNIQUE,            -- stable id (sync, calendar file)
    offer_id   INTEGER NOT NULL REFERENCES offers (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('interview', 'deadline', 'other')),
    at         TEXT NOT NULL,                   -- UTC, RFC 3339
    minutes    INTEGER NOT NULL DEFAULT 60,     -- length (calendar)
    note       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE INDEX events_at ON events (at);

CREATE TABLE contacts (
    id         INTEGER PRIMARY KEY,
    offer_id   INTEGER NOT NULL REFERENCES offers (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    role       TEXT NOT NULL DEFAULT '',
    email      TEXT NOT NULL DEFAULT '',
    phone      TEXT NOT NULL DEFAULT '',
    linkedin   TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE contact_log (
    id       INTEGER PRIMARY KEY,
    offer_id INTEGER NOT NULL REFERENCES offers (id) ON DELETE CASCADE,
    at       TEXT NOT NULL,
    kind     TEXT NOT NULL CHECK (kind IN ('call', 'mail', 'meeting', 'message')),
    note     TEXT NOT NULL DEFAULT ''
);

CREATE INDEX contact_log_offer ON contact_log (offer_id, at);

-- `subject` = event id, or offer id for follow-ups and high scores; `mark` tells versions apart
-- (e.g. the stage change time - a new "applied" gets a new follow-up reminder).
CREATE TABLE reminders_sent (
    kind    TEXT NOT NULL,                      -- 'day_before', 'hour_before', 'followup', 'high_score'
    subject INTEGER NOT NULL,
    mark    TEXT NOT NULL DEFAULT '',
    sent_at TEXT NOT NULL,
    PRIMARY KEY (kind, subject, mark)
);
