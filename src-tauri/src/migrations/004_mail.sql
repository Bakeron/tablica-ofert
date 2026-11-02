-- Version 4: where reading the alert mailbox stopped (one mailbox, one row).
-- `account` = login@server:port - a different mailbox starts again from the last two weeks.

CREATE TABLE mail_state (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    account      TEXT NOT NULL,
    uid_validity INTEGER NOT NULL,
    last_uid     INTEGER NOT NULL
);
