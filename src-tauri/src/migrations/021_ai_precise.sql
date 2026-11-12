-- Version 21 (11.11.2026): the person asked for "Rate more precisely" (Claude) on this offer - every
-- later rating of it ("Rate all again", new data from a source) goes to Claude first, like an offer
-- added by hand. 1 = yes; 0 or NULL (a copy from an older device) = no.
ALTER TABLE offers ADD COLUMN ai_precise INTEGER DEFAULT 0;
