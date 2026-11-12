-- Version 22 (11.11.2026): tokens used by Claude per day and model - the app counts its own
-- spending from them (a normal API key cannot read the account's balance). Only paid calls are
-- kept (Claude); free providers are counted in `api_usage` as before. Stays on this device (not
-- synchronized), like `api_usage`.
CREATE TABLE ai_spend (
    day         TEXT NOT NULL,                  -- YYYY-MM-DD
    model       TEXT NOT NULL,                  -- as the provider named it in its answer
    calls       INTEGER NOT NULL DEFAULT 0,
    input       INTEGER NOT NULL DEFAULT 0,     -- full-price input tokens
    output      INTEGER NOT NULL DEFAULT 0,
    cache_read  INTEGER NOT NULL DEFAULT 0,     -- read back from the prompt cache (much cheaper)
    cache_write INTEGER NOT NULL DEFAULT 0,     -- written to the prompt cache (a bit dearer)
    PRIMARY KEY (day, model)
);
