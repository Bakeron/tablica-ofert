-- Version 18: alert e-mails are read anew (8.11.2026). The reading of alerts was fixed on real
-- mails of every portal - offers it used to miss or glue together come in right. The next fetch
-- starts as the first one did: mails from the days offers stay fresh. What is already on the
-- board stays as it is (the same offer is only merged), and what the user deleted does not come back.

UPDATE mail_state SET last_uid = 0;
