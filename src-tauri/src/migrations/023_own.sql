-- Version 23 (11.11.2026): "own" offers - the person's own recruitments (tab "Own"): added by
-- hand, moved there with "To own", or with a stage past "new" (interested, applied and further).
-- The rest is "Collected". 1 = own; 0 or NULL (a copy from an older device) = collected.
ALTER TABLE offers ADD COLUMN own INTEGER DEFAULT 0;

UPDATE offers SET own = 1
WHERE stage != 'new'
   OR EXISTS (SELECT 1 FROM offer_sources m WHERE m.offer_id = offers.id AND m.source = 'manual');
