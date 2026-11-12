-- Version 24 (11.11.2026): the same offer added again from its portal, changed - "Update the
-- existing one". `renewed_at` - when it was updated that way (the card's history: "ad renewed").
-- `renew_asked` = 1: the ad came again after the CV went - the mark RENEWED and the question
-- whether to send the CV once more, until the person answers. NULL (a copy from an older device) = 0.
ALTER TABLE offers ADD COLUMN renewed_at TEXT;
ALTER TABLE offers ADD COLUMN renew_asked INTEGER DEFAULT 0;
