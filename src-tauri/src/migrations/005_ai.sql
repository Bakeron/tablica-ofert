-- Version 5: AI - what AI read from an offer, its match score for every profile,
-- and the previous version of a profile description ("Tidy up" can be undone).

ALTER TABLE offers ADD COLUMN ai_status TEXT CHECK (ai_status IN ('done', 'failed'));  -- NULL = waiting
ALTER TABLE offers ADD COLUMN ai_at TEXT;
ALTER TABLE offers ADD COLUMN ai_summary TEXT;
ALTER TABLE offers ADD COLUMN ai_flags TEXT;            -- JSON list of short warnings ("Uwagi")
ALTER TABLE offers ADD COLUMN ai_remote_quote TEXT;     -- sentence from the offer about remote work
ALTER TABLE offers ADD COLUMN ai_error TEXT;

-- Match score of an offer for one profile: from AI (with a reason and a quote) or from keywords.
CREATE TABLE offer_scores (
    offer_id   INTEGER NOT NULL REFERENCES offers (id) ON DELETE CASCADE,
    profile_id INTEGER NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    score      INTEGER NOT NULL CHECK (score BETWEEN 0 AND 100),
    source     TEXT NOT NULL CHECK (source IN ('ai', 'keywords')),
    why        TEXT,
    quote      TEXT,
    PRIMARY KEY (offer_id, profile_id)
);

ALTER TABLE profiles ADD COLUMN description_prev TEXT;
ALTER TABLE candidate ADD COLUMN experience_prev TEXT;
