-- Version 16: one clean start for the new rules (decision of 6.11.2026). Everything collected by
-- itself goes to the trash (so it never comes back); what the user worked on stays: added by
-- hand, a star, an interest, a stage other than "new", a note, a date, a contact or a contact log.
-- The database is copied to `kopie/` before any migration runs (db.rs).

UPDATE offers
SET deleted_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
    updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE deleted_at IS NULL
  AND stage = 'new'
  AND favorite = 0
  AND interest IS NULL
  AND notes = ''
  AND NOT EXISTS (SELECT 1 FROM offer_sources s WHERE s.offer_id = offers.id AND s.source = 'manual')
  AND NOT EXISTS (SELECT 1 FROM events e WHERE e.offer_id = offers.id)
  AND NOT EXISTS (SELECT 1 FROM contacts c WHERE c.offer_id = offers.id)
  AND NOT EXISTS (SELECT 1 FROM contact_log l WHERE l.offer_id = offers.id);
