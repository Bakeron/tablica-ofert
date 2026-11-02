//! The mailbox only for job alerts (IMAP). Read-only: the inbox is opened with EXAMINE,
//! messages are fetched with BODY.PEEK (they stay unread), nothing is deleted or sent.
//! The password lives in the credential store, the address and server in the settings.

use std::collections::BTreeMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{AppError, AppResult};
use crate::model::OfferInput;

use super::alerts;

pub const SOURCE: &str = "mail";
/// Messages read in one run; the rest waits for the next run (oldest first).
const MAX_MESSAGES: usize = 150;
/// Alerts are small - anything bigger is not an alert.
const MAX_MESSAGE_BYTES: u32 = 3 * 1024 * 1024;
/// First run (or after the server reset its numbering): mails from the last two weeks.
const FIRST_DAYS: i64 = 14;
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct Config {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}

/// Mailbox settings, `None` = address or password not set yet.
pub fn config(settings: &BTreeMap<String, String>) -> AppResult<Option<Config>> {
    let host = settings.get("mail_host").cloned().unwrap_or_default();
    let user = settings.get("mail_user").cloned().unwrap_or_default();
    let port: u16 = settings.get("mail_port").and_then(|p| p.parse().ok()).unwrap_or(993);
    if host.is_empty() || user.is_empty() {
        return Ok(None);
    }
    Ok(crate::secrets::get("mail-password")?.map(|password| Config { host, port, user, password }))
}

/// Where the last run stopped. The server's UIDVALIDITY changes when its numbering restarts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Position {
    pub account: String,
    pub uid_validity: u32,
    pub last_uid: u32,
}

pub fn position(conn: &Connection) -> AppResult<Position> {
    Ok(conn
        .query_row("SELECT account, uid_validity, last_uid FROM mail_state WHERE id = 1", [], |r| {
            Ok(Position { account: r.get(0)?, uid_validity: r.get(1)?, last_uid: r.get(2)? })
        })
        .optional()?
        .unwrap_or_default())
}

pub fn save_position(conn: &Connection, p: &Position) -> AppResult<()> {
    conn.execute(
        "INSERT INTO mail_state (id, account, uid_validity, last_uid) VALUES (1, ?1, ?2, ?3)
         ON CONFLICT (id) DO UPDATE SET account = ?1, uid_validity = ?2, last_uid = ?3",
        params![p.account, p.uid_validity, p.last_uid],
    )?;
    Ok(())
}

pub struct Fetched {
    pub offers: Vec<OfferInput>,
    pub position: Position,
}

fn account_of(c: &Config) -> String {
    format!("{}@{}:{}", c.user, c.host, c.port)
}

type Session = imap::Session<native_tls::TlsStream<TcpStream>>;

fn mail_error(e: imap::error::Error) -> AppError {
    match e {
        imap::error::Error::No(msg) | imap::error::Error::Bad(msg) => AppError::Auth(msg),
        other => AppError::Net(other.to_string()),
    }
}

/// Opens a TLS connection with timeouts and logs in.
fn connect(c: &Config) -> AppResult<Session> {
    let addr = (c.host.as_str(), c.port)
        .to_socket_addrs()
        .map_err(|e| AppError::Net(format!("cannot find the server: {e}")))?
        .next()
        .ok_or_else(|| AppError::Net("cannot find the server".into()))?;
    let tcp = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|e| AppError::Net(e.to_string()))?;
    tcp.set_read_timeout(Some(TIMEOUT)).map_err(|e| AppError::Net(e.to_string()))?;
    tcp.set_write_timeout(Some(TIMEOUT)).map_err(|e| AppError::Net(e.to_string()))?;
    let tls = native_tls::TlsConnector::new().map_err(|e| AppError::Net(e.to_string()))?;
    let stream = tls.connect(&c.host, tcp).map_err(|e| AppError::Net(e.to_string()))?;
    let mut client = imap::Client::new(stream);
    client.read_greeting().map_err(mail_error)?;
    client.login(&c.user, &c.password).map_err(|(e, _)| match e {
        imap::error::Error::No(msg) | imap::error::Error::Bad(msg) => AppError::Auth(msg),
        other => AppError::Net(other.to_string()),
    })
}

/// "Check connection" in the settings: logs in, opens the inbox read-only, counts messages.
pub fn test(c: &Config) -> AppResult<u32> {
    let mut session = connect(c)?;
    let inbox = session.examine("INBOX").map_err(mail_error)?;
    let _ = session.logout();
    Ok(inbox.exists)
}

fn imap_date(days_ago: i64) -> String {
    let day = time::OffsetDateTime::now_utc() - time::Duration::days(days_ago);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{}-{}-{}", day.day(), MONTHS[day.month() as usize - 1], day.year())
}

/// New alert e-mails since the last run. Blocking - call it off the async runtime.
pub fn fetch(c: &Config, last: &Position) -> AppResult<Fetched> {
    let mut session = connect(c)?;
    let inbox = session.examine("INBOX").map_err(mail_error)?;
    let validity = inbox.uid_validity.unwrap_or(0);
    let account = account_of(c);
    let continuing = last.account == account && last.uid_validity == validity && last.last_uid > 0;

    let query = if continuing { format!("UID {}:*", last.last_uid + 1) } else { format!("SINCE {}", imap_date(FIRST_DAYS)) };
    let mut uids: Vec<u32> = session
        .uid_search(&query)
        .map_err(mail_error)?
        .into_iter()
        .filter(|u| !continuing || *u > last.last_uid)
        .collect();
    uids.sort_unstable();
    uids.truncate(MAX_MESSAGES);

    let mut position = Position { account, uid_validity: validity, last_uid: if continuing { last.last_uid } else { 0 } };
    let mut offers = Vec::new();
    for chunk in uids.chunks(25) {
        let set = chunk.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let sizes = session.uid_fetch(&set, "RFC822.SIZE").map_err(mail_error)?;
        let small: Vec<u32> = sizes
            .iter()
            .filter(|f| f.size.is_some_and(|s| s <= MAX_MESSAGE_BYTES))
            .filter_map(|f| f.uid)
            .collect();
        if !small.is_empty() {
            let set = small.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
            // BODY.PEEK: reading does not mark the message as read.
            let bodies = session.uid_fetch(&set, "BODY.PEEK[]").map_err(mail_error)?;
            for f in bodies.iter() {
                if let Some(raw) = f.body() {
                    offers.extend(offers_in_raw(raw));
                }
            }
        }
        position.last_uid = position.last_uid.max(*chunk.last().unwrap_or(&0));
    }
    let _ = session.logout();
    Ok(Fetched { offers, position })
}

/// Offers in one raw e-mail (RFC 822).
pub fn offers_in_raw(raw: &[u8]) -> Vec<OfferInput> {
    let Some(message) = mail_parser::MessageParser::default().parse(raw) else { return Vec::new() };
    let sender = message
        .from()
        .and_then(|a| a.first())
        .and_then(|a| a.address())
        .unwrap_or_default()
        .to_string();
    let html = message.body_html(0).map(|h| h.into_owned());
    let text = message.body_text(0).map(|t| t.into_owned());
    alerts::offers_in_mail(&sender, html.as_deref(), text.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;

    #[test]
    fn raw_mail_with_html_part() {
        let raw = "From: Alerty <alerty@nofluffjobs.com>\r\n\
To: oferty@example.com\r\n\
Subject: =?UTF-8?Q?Nowe_oferty?=\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/alternative; boundary=\"b1\"\r\n\
\r\n\
--b1\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Backend Developer https://nofluffjobs.com/pl/job/backend-developer-firma-z-remote\r\n\
--b1\r\n\
Content-Type: text/html; charset=utf-8\r\n\
Content-Transfer-Encoding: quoted-printable\r\n\
\r\n\
<a href=3D\"https://nofluffjobs.com/pl/job/backend-developer-firma-z-remote\">Backend Developer</a><br>Firma Z<br>Zdalnie<br>18 000 - 24 000 PLN\r\n\
--b1--\r\n";
        let offers = offers_in_raw(raw.as_bytes());
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].company, "Firma Z");
        assert_eq!(offers[0].salary_max, Some(24000));
        assert!(offers_in_raw(b"not a mail at all").is_empty());
    }

    #[test]
    fn position_is_remembered() {
        let conn = test_db();
        assert_eq!(position(&conn).unwrap(), Position::default());
        let p = Position { account: "a@imap.example.com:993".into(), uid_validity: 7, last_uid: 120 };
        save_position(&conn, &p).unwrap();
        assert_eq!(position(&conn).unwrap(), p);
    }

    #[test]
    fn date_for_search() {
        let d = imap_date(0);
        assert_eq!(d.split('-').count(), 3);
        assert!(d.split('-').nth(1).unwrap().len() == 3);
    }
}
