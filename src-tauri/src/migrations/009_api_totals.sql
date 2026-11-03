-- Version 9: calls made with a key in its whole life (Jooble gives 500 calls per key, not per
-- day). The daily counter `api_usage` keeps only about a month, so it cannot answer that.

CREATE TABLE api_totals (
    key   TEXT PRIMARY KEY,                     -- e.g. 'jooble'
    calls INTEGER NOT NULL DEFAULT 0
);
