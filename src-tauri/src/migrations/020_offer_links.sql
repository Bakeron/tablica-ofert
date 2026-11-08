-- Version 20 (8.11.2026): one offer may keep several links of the same source - the same job in
-- two cities from one portal is one offer with both links. A link is now told apart by the offer,
-- the source and its address (no address counts as one). `city` - the city that link gave, so the
-- card can say which link is which ("Open the offer - Kraków").
CREATE TABLE offer_sources_new (
    offer_id   INTEGER NOT NULL REFERENCES offers (id) ON DELETE CASCADE,
    source     TEXT NOT NULL,
    url        TEXT,
    first_seen TEXT NOT NULL,
    checked_on TEXT,
    city       TEXT
);
INSERT INTO offer_sources_new (offer_id, source, url, first_seen, checked_on, city)
SELECT s.offer_id, s.source, s.url, s.first_seen, s.checked_on, o.city
FROM offer_sources s JOIN offers o ON o.id = s.offer_id;
DROP TABLE offer_sources;
ALTER TABLE offer_sources_new RENAME TO offer_sources;
CREATE UNIQUE INDEX offer_sources_link ON offer_sources (offer_id, source, COALESCE(url, ''));
