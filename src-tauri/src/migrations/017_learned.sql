-- Version 17: what the "Very" / "No" ratings taught (zalozenia.md §5). A correction shown on top
-- of the score, never written into it. Worked out again from the offers - it never goes to sync.

CREATE TABLE offer_learned (
    offer_id INTEGER PRIMARY KEY REFERENCES offers (id) ON DELETE CASCADE,
    points   INTEGER NOT NULL CHECK (points BETWEEN -15 AND 15),
    words    TEXT NOT NULL    -- JSON list of the words behind the points
);
