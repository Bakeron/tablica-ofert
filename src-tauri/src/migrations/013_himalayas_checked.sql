-- Version 13: the day each Himalayas offer was last checked for being closed. A run stopped
-- halfway (no connection) goes on from the next unchecked offer instead of from the start.

ALTER TABLE offer_sources ADD COLUMN checked_on TEXT;   -- YYYY-MM-DD (UTC); NULL = never checked
