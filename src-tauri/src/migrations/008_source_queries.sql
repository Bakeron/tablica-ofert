-- Version 8: when each search (one keyword in one place) last went well. A keyword searched
-- for the first time looks two weeks back, one searched a few hours ago only a couple of days.

CREATE TABLE source_queries (
    source  TEXT NOT NULL,                      -- 'adzuna'
    query   TEXT NOT NULL,                      -- keyword|city|radius, e.g. 'wordpress||0'
    last_ok TEXT NOT NULL,
    PRIMARY KEY (source, query)
);
