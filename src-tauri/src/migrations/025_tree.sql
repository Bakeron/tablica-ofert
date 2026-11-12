-- Version 25 (11.11.2026): the tree of duplicates. The same job seen again in another ad (another
-- portal, or a second ad of the same portal) is a child of the first one: a row of its own with
-- its link, under the parent on the list. One level only - a child never has children.
-- NULL = a top-level offer. The links merged into one offer before are split into children once,
-- by the app after this step (`tree::convert`).
ALTER TABLE offers ADD COLUMN parent_id INTEGER REFERENCES offers (id) ON DELETE SET NULL;
CREATE INDEX offers_parent ON offers (parent_id);
