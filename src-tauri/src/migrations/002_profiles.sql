-- Version 2: "what I'm looking for" profiles, shared work experience, hard filter results.

-- One profile = one plan of the job search ("Plan A - remote", "Plan B - in my city").
-- `criteria` is JSON checked field by field in Rust (profiles.rs) before it is stored.
CREATE TABLE profiles (
    id          INTEGER PRIMARY KEY,
    uid         TEXT NOT NULL UNIQUE,           -- stable id for future PC - phone sync
    name        TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,     -- order of the tabs above the board
    description TEXT NOT NULL,                  -- written by the user, read by AI later
    criteria    TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- Work experience is shared by all profiles - a single row.
CREATE TABLE candidate (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    experience TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL
);

-- Hard filter result of every offer for every profile.
-- `offers.filter_result` keeps the best result over all profiles (used by the board lists);
-- `offers.filter_reason` from version 1 is no longer used - reasons live here, per profile.
CREATE TABLE offer_filters (
    offer_id   INTEGER NOT NULL REFERENCES offers (id) ON DELETE CASCADE,
    profile_id INTEGER NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    result     TEXT NOT NULL CHECK (result IN ('pass', 'fail', 'unsure')),
    reasons    TEXT NOT NULL DEFAULT '[]',      -- JSON list of { kind, code, value }
    PRIMARY KEY (offer_id, profile_id)
);

CREATE INDEX offer_filters_profile ON offer_filters (profile_id, result);
