-- Version 15: freshness instead of an inbox to empty (zalozenia.md §3). An offer nobody touched
-- leaves for the archive some days after it was published; the others get "probably expired".

ALTER TABLE offers ADD COLUMN published_at TEXT;   -- given by the source (alert: the mail's date); seen again = newer
ALTER TABLE offers ADD COLUMN archived_at TEXT;    -- set = in the archive (not the trash: fetching it again only merges)
ALTER TABLE offers ADD COLUMN kept_at TEXT;        -- brought back from the archive by the user - never leaves by itself

UPDATE offers SET published_at = added_at;

CREATE INDEX offers_fresh ON offers (deleted_at, archived_at, published_at);
