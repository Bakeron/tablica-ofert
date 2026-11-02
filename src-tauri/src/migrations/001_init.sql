-- Version 1: offers, where they were seen, recruitment stage history, trash memory, settings.
-- Times are RFC 3339 strings in UTC.

CREATE TABLE offers (
    id               INTEGER PRIMARY KEY,
    uid              TEXT NOT NULL UNIQUE,            -- stable id for future PC - phone sync
    fingerprint      TEXT NOT NULL UNIQUE,            -- company + title, normalized (duplicate detection)
    title            TEXT NOT NULL,
    company          TEXT NOT NULL,
    url              TEXT,
    city             TEXT,
    remote           TEXT NOT NULL DEFAULT 'unknown' CHECK (remote IN ('yes', 'hybrid', 'no', 'unknown')),
    salary_min       INTEGER,
    salary_max       INTEGER,
    salary_currency  TEXT,
    salary_period    TEXT CHECK (salary_period IN ('month', 'hour')),
    contract         TEXT,
    level            TEXT,
    language         TEXT,
    requirements     TEXT,
    description      TEXT,
    notes            TEXT NOT NULL DEFAULT '',
    interest         TEXT CHECK (interest IN ('high', 'maybe', 'no')),
    favorite         INTEGER NOT NULL DEFAULT 0,
    stage            TEXT NOT NULL DEFAULT 'new',
    stage_changed_at TEXT NOT NULL,
    score            INTEGER CHECK (score BETWEEN 0 AND 100),
    score_source     TEXT CHECK (score_source IN ('ai', 'keywords')),
    filter_result    TEXT NOT NULL DEFAULT 'pass' CHECK (filter_result IN ('pass', 'fail', 'unsure')),
    filter_reason    TEXT,
    expires_at       TEXT,
    expired          INTEGER NOT NULL DEFAULT 0,
    added_at         TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    deleted_at       TEXT                              -- set = offer is in the trash
);

CREATE INDEX offers_list ON offers (deleted_at, filter_result, added_at);

-- Every portal / feed where the same offer appeared.
CREATE TABLE offer_sources (
    offer_id   INTEGER NOT NULL REFERENCES offers (id) ON DELETE CASCADE,
    source     TEXT NOT NULL,
    url        TEXT,
    first_seen TEXT NOT NULL,
    PRIMARY KEY (offer_id, source)
);

CREATE TABLE stage_history (
    id       INTEGER PRIMARY KEY,
    offer_id INTEGER NOT NULL REFERENCES offers (id) ON DELETE CASCADE,
    stage    TEXT NOT NULL,
    at       TEXT NOT NULL
);

CREATE INDEX stage_history_offer ON stage_history (offer_id, at);

-- Offers deleted for good - remembered so the next fetch does not bring them back.
CREATE TABLE removed_offers (
    fingerprint TEXT PRIMARY KEY,
    removed_at  TEXT NOT NULL
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
