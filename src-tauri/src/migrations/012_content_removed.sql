-- Version 12: a source whose terms ask to drop closed jobs (Himalayas). An offer the user acted on
-- (stage, note, dates, contacts) stays as the user's own record, but the source's text is removed;
-- this column says which source it was, so the card can tell why the description is gone.

ALTER TABLE offers ADD COLUMN content_removed TEXT;     -- e.g. 'himalayas'; NULL = nothing removed
