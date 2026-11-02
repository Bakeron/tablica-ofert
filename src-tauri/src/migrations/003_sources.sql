-- Version 3: offer sources (aggregators, e-mail alerts) - when each one ran and how it went,
-- and a daily counter of calls to services with limits (Adzuna, Remotive, later AI).

CREATE TABLE source_runs (
    source     TEXT PRIMARY KEY,                -- 'adzuna', 'remotive', 'mail'
    last_try   TEXT,                            -- last start (also when it failed)
    last_ok    TEXT,                            -- last successful run
    last_error TEXT,                            -- short technical message of the last failure
    added      INTEGER NOT NULL DEFAULT 0,      -- result of the last successful run
    merged     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE api_usage (
    key   TEXT NOT NULL,                        -- e.g. 'adzuna', 'ai:gemini'
    day   TEXT NOT NULL,                        -- YYYY-MM-DD in UTC
    calls INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (key, day)
);
