-- Version 11: an offer in another language than the app gets its title and description
-- translated by AI (in the same call as the score). Kept next to the original, which never changes.

ALTER TABLE offers ADD COLUMN ai_tr_lang TEXT;          -- 'pl' / 'en' - language of the translation
ALTER TABLE offers ADD COLUMN ai_title_tr TEXT;
ALTER TABLE offers ADD COLUMN ai_description_tr TEXT;
