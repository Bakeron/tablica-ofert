-- Version 26 (11.11.2026): duplicates from another portal. The same company and title as an offer
-- on the board, but another city or clearly other salary - two recruitments of a chain, or one ad:
-- the app does not join them by itself, it asks. `dup_of` = the offer it may be (the question on
-- the card until the person answers), `dup_why` = "city" or "salary". NULL = nothing to ask.
ALTER TABLE offers ADD COLUMN dup_of INTEGER REFERENCES offers (id) ON DELETE SET NULL;
ALTER TABLE offers ADD COLUMN dup_why TEXT;
