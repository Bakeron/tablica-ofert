-- Version 19 (8.11.2026): two times of change of an offer. `updated_at` stays the time of the data
-- (sources, AI, the app's own jobs), `user_changed_at` is the time of the user's own fields: stage,
-- interest, star, note, trash, archive and "expired". Sync merges the two groups apart, so a fetch
-- on one device never undoes what the user changed on the other.
ALTER TABLE offers ADD COLUMN user_changed_at TEXT;
UPDATE offers SET user_changed_at = updated_at;

-- "Expired" clicked by the user stays until the user takes it off; the automatic one (a past
-- expiry date, a closed Himalayas job) goes only with a newer date. Offers marked so far without
-- a past expiry date were marked by hand.
ALTER TABLE offers ADD COLUMN expired_by_user INTEGER NOT NULL DEFAULT 0;
UPDATE offers SET expired_by_user = 1
WHERE expired = 1
  AND (expires_at IS NULL OR expires_at >= strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
  AND COALESCE(content_removed, '') != 'himalayas';
