//! PC - phone sync through the user's own Google Drive (zalozenia.md §2), optional.
//! "Sync now": download the file, decrypt, merge row by row with this device, write the
//! result here, encrypt and upload it. Uploading never wipes the other device's changes.

pub mod crypto;
pub mod data;
pub mod drive;
pub mod merge;

use serde::Serialize;

use crate::commands::{AppState, with_db};
use crate::error::{AppError, AppResult};
use crate::{clock, net, secrets, settings};

pub const KEY_SECRET: &str = "sync-key";
/// More offers deleted at once than this - the person confirms first.
pub const ASK_ABOVE: usize = 3;
/// Which device wrote the sync file - tells this device's own file apart (`merge::check_versions`).
const DEVICE: &str = if cfg!(target_os = "android") { "phone" } else { "pc" };

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// A Google project (client) is available - built in or typed in the settings.
    pub client: bool,
    pub connected: bool,
    pub has_key: bool,
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SyncOutcome {
    Done { changed: usize, removed: usize },
    /// Several offers would be deleted - ask, then call again with `confirm = true`.
    Confirm { removed: usize },
}

pub fn client(conn: &rusqlite::Connection) -> AppResult<Option<drive::OAuthClient>> {
    let id = settings::get(conn, "google_client_id")?;
    if !id.is_empty() {
        if let Some(secret) = secrets::get("google-client-secret")? {
            return Ok(Some(drive::OAuthClient { id, secret }));
        }
    }
    Ok(drive::built_in_client())
}

fn key() -> AppResult<Option<[u8; 32]>> {
    secrets::get(KEY_SECRET)?.map(|k| crypto::key_from_hex(&k)).transpose()
}

/// The sync key, created at the first use.
pub fn key_or_new() -> AppResult<[u8; 32]> {
    if let Some(k) = key()? {
        return Ok(k);
    }
    let k = crypto::new_key()?;
    secrets::set(KEY_SECRET, &crypto::key_to_hex(&k))?;
    Ok(k)
}

pub fn status(conn: &rusqlite::Connection) -> AppResult<SyncStatus> {
    let (last_sync, last_error) = conn
        .query_row("SELECT last_ok, last_error FROM source_runs WHERE source = 'drive'", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or((None, None));
    Ok(SyncStatus {
        client: client(conn)?.is_some(),
        connected: secrets::get(drive::REFRESH_SECRET)?.is_some(),
        has_key: key()?.is_some(),
        last_sync,
        last_error,
    })
}

fn record(conn: &rusqlite::Connection, error: Option<&str>) -> AppResult<()> {
    let now = clock::now();
    conn.execute(
        "INSERT INTO source_runs (source, last_try, last_ok, last_error) VALUES ('drive', ?1, CASE WHEN ?2 IS NULL THEN ?1 END, ?2)
         ON CONFLICT (source) DO UPDATE SET last_try = ?1,
            last_ok = CASE WHEN ?2 IS NULL THEN ?1 ELSE source_runs.last_ok END, last_error = ?2",
        rusqlite::params![now, error],
    )?;
    Ok(())
}

async fn token(state: &AppState) -> AppResult<(reqwest::Client, String)> {
    let oauth = with_db(state, |c| client(c))?.ok_or_else(|| AppError::Missing("google project".into()))?;
    let refresh = secrets::get(drive::REFRESH_SECRET)?.ok_or_else(|| AppError::Missing("google login".into()))?;
    let http = net::client()?;
    let access = match drive::access_token(&http, &oauth, &refresh).await {
        // The old login is useless now - forget it, so the settings show "Connect" again.
        Err(AppError::Missing(m)) if m == drive::LOGIN_EXPIRED => {
            secrets::delete(drive::REFRESH_SECRET)?;
            return Err(AppError::Missing(m));
        }
        other => other?,
    };
    Ok((http, access))
}

/// Saved as the last error while an automatic sync waits for "Sync now" to confirm a big deletion.
pub const CONFIRM_WAITING: &str = "waits for confirmation";

/// One sync job at a time: a new key during a sync would leave the file sealed with the old one,
/// two first syncs would make two files on Drive.
struct Syncing<'a>(&'a AppState);

impl<'a> Syncing<'a> {
    fn start(state: &'a AppState) -> AppResult<Self> {
        if state.syncing.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(AppError::Busy);
        }
        Ok(Syncing(state))
    }
}

impl Drop for Syncing<'_> {
    fn drop(&mut self) {
        self.0.syncing.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// "Sync now". `confirm` = the person agreed to delete several offers at once.
pub async fn run(state: &AppState, confirm: bool) -> AppResult<SyncOutcome> {
    let _busy = Syncing::start(state)?;
    let result = run_inner(state, confirm).await;
    // A big deletion waiting for the person is no successful sync: nothing was saved or sent.
    let error = match &result {
        Err(e) => Some(crate::text::cut(&e.to_string(), 300)),
        Ok(SyncOutcome::Confirm { removed }) => Some(format!("{CONFIRM_WAITING}: {removed}")),
        Ok(_) => None,
    };
    with_db(state, |c| record(c, error.as_deref()))?;
    result
}

async fn run_inner(state: &AppState, confirm: bool) -> AppResult<SyncOutcome> {
    let key = key_or_new()?;
    let (http, access) = token(state).await?;
    let now = clock::now();
    let local = with_db(state, |c| data::export(c, DEVICE, &now))?;
    let file = drive::find(&http, &access).await?;
    let remote = match &file {
        Some(f) => {
            let bytes = drive::download(&http, &access, f).await?;
            let plain = crypto::open(&key, &bytes)?;
            let remote = serde_json::from_slice::<merge::Snapshot>(&plain).map_err(|e| AppError::Invalid(format!("damaged sync file: {e}")))?;
            merge::check_versions(&remote, &local)?;
            remote
        }
        None => merge::Snapshot::default(),
    };
    let (merged, stats) = merge::merge(&local, &remote, &now);
    let removing = with_db(state, |c| data::offers_to_remove(c, &merged))?;
    if removing > ASK_ABOVE && !confirm {
        return Ok(SyncOutcome::Confirm { removed: removing });
    }
    with_db(state, |c| {
        data::apply(c, &merged, &now)?;
        // Ratings from the other device, and offers that are new here.
        crate::learn::refresh(c)
    })?;
    let json = serde_json::to_vec(&merged).map_err(|e| AppError::Invalid(e.to_string()))?;
    drive::upload(&http, &access, file.as_ref(), crypto::seal(&key, &json)?).await?;
    Ok(SyncOutcome::Done { changed: stats.changed, removed: stats.removed })
}

/// A key in use can change only with a login: the file on Drive must get the new key too.
fn may_change_key(has_key: bool, connected: bool) -> bool {
    !has_key || connected
}

/// "Disconnect devices": a new key - an old phone can no longer read the file.
/// The file on Drive is re-encrypted with the new key right away. When Google cannot be
/// reached (no network, expired or missing login), the old key stays: a new one would lock
/// this computer out of its own file on Drive.
pub async fn new_key(state: &AppState) -> AppResult<()> {
    let _busy = Syncing::start(state)?;
    let old = key()?;
    let connected = secrets::get(drive::REFRESH_SECRET)?.is_some();
    if !may_change_key(old.is_some(), connected) {
        return Err(AppError::Missing("google login".into()));
    }
    let fresh = crypto::new_key()?;
    if let (Some(old), true) = (old, connected) {
        let (http, access) = token(state).await?;
        if let Some(file) = drive::find(&http, &access).await? {
            let plain = crypto::open(&old, &drive::download(&http, &access, &file).await?)?;
            drive::upload(&http, &access, Some(&file), crypto::seal(&fresh, &plain)?).await?;
        }
    }
    secrets::set(KEY_SECRET, &crypto::key_to_hex(&fresh))
}

/// "Delete from Drive": removes the sync file (the data on this computer stays).
pub async fn delete_remote(state: &AppState) -> AppResult<()> {
    let _busy = Syncing::start(state)?;
    let (http, access) = token(state).await?;
    if let Some(file) = drive::find(&http, &access).await? {
        drive::delete(&http, &access, &file).await?;
    }
    Ok(())
}

const QR_KEY: &str = "tablica-ofert:sync:1:";
/// The key and the PC's own Google project, so nothing has to be typed on the phone.
const QR_KEY_AND_PROJECT: &str = "tablica-ofert:sync:2:";

/// What the phone's camera read from the PC's QR code.
pub struct Scanned {
    pub key: [u8; 32],
    pub client: Option<drive::OAuthClient>,
}

pub fn qr_payload(key: &[u8; 32], client: Option<&drive::OAuthClient>) -> String {
    match client {
        // The secret goes last: only it could ever hold a ':'.
        Some(c) => format!("{QR_KEY_AND_PROJECT}{}:{}:{}", crypto::key_to_hex(key), c.id, c.secret),
        None => format!("{QR_KEY}{}", crypto::key_to_hex(key)),
    }
}

/// A Google client secret: printable ASCII without spaces, not too long.
fn is_client_secret(v: &str) -> bool {
    (1..=200).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_graphic())
}

/// Checked field by field - the text comes from a camera.
pub fn parse_qr(text: &str) -> AppResult<Scanned> {
    let wrong = || AppError::Invalid("not a Tablica Ofert code".into());
    let text = text.trim();
    if text.len() > 1000 {
        return Err(wrong());
    }
    if let Some(hex) = text.strip_prefix(QR_KEY) {
        return Ok(Scanned { key: crypto::key_from_hex(hex)?, client: None });
    }
    let rest = text.strip_prefix(QR_KEY_AND_PROJECT).ok_or_else(wrong)?;
    let mut parts = rest.splitn(3, ':');
    let (hex, id, secret) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if id.is_empty() || id.len() > 200 || !settings::is_client_id(id) || !is_client_secret(secret) {
        return Err(wrong());
    }
    Ok(Scanned { key: crypto::key_from_hex(hex)?, client: Some(drive::OAuthClient { id: id.into(), secret: secret.into() }) })
}

/// The PC's own Google project (not a built-in one - the phone app has the same built in).
fn own_client(conn: &rusqlite::Connection) -> AppResult<Option<drive::OAuthClient>> {
    let id = settings::get(conn, "google_client_id")?;
    if id.is_empty() {
        return Ok(None);
    }
    Ok(secrets::get("google-client-secret")?.map(|secret| drive::OAuthClient { id, secret }))
}

/// Phone: saves what the QR code carried. A new Google project ends the old login (it belongs to one project).
pub fn save_scanned(state: &AppState, text: &str) -> AppResult<Option<String>> {
    let scanned = parse_qr(text)?;
    secrets::set(KEY_SECRET, &crypto::key_to_hex(&scanned.key))?;
    let Some(client) = scanned.client else { return Ok(None) };
    let old = with_db(state, |c| settings::get(c, "google_client_id"))?;
    if old != client.id {
        secrets::delete(drive::REFRESH_SECRET)?;
    }
    with_db(state, |c| settings::set(c, "google_client_id", &client.id))?;
    secrets::set("google-client-secret", &client.secret)?;
    Ok(Some(client.id))
}

/// QR code with the key for the phone (shown for 2 minutes, never sent anywhere).
pub fn qr_svg(state: &AppState) -> AppResult<String> {
    let key = key_or_new()?;
    let client = with_db(state, |c| own_client(c))?;
    let payload = qr_payload(&key, client.as_ref());
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|e| AppError::Invalid(e.to_string()))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(260, 260)
        .dark_color(qrcode::render::svg::Color("#111111"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "123456789012-abc.apps.googleusercontent.com";

    #[test]
    fn a_key_in_use_changes_only_with_a_login() {
        assert!(may_change_key(false, false));
        assert!(may_change_key(true, true));
        assert!(!may_change_key(true, false));
    }

    #[test]
    fn the_qr_code_carries_the_key_and_the_own_project() {
        let key = [7u8; 32];
        let client = drive::OAuthClient { id: ID.into(), secret: "GOCSPX-a:b_c".into() };
        let back = parse_qr(&qr_payload(&key, Some(&client))).unwrap();
        assert_eq!(back.key, key);
        let c = back.client.unwrap();
        assert_eq!((c.id.as_str(), c.secret.as_str()), (ID, "GOCSPX-a:b_c"));
        let plain = parse_qr(&qr_payload(&key, None)).unwrap();
        assert!(plain.client.is_none());
        assert_eq!(plain.key, key);
    }

    #[test]
    fn a_strange_qr_code_is_refused() {
        let hex = "ab".repeat(32);
        for text in [
            "https://example.com".to_string(),
            format!("tablica-ofert:sync:2:{hex}"),
            format!("tablica-ofert:sync:2:{hex}:{ID}:"),
            format!("tablica-ofert:sync:2:{hex}:bad id:secret"),
            format!("tablica-ofert:sync:2:{hex}:{ID}:sec ret"),
            format!("tablica-ofert:sync:2:short:{ID}:secret"),
            format!("tablica-ofert:sync:1:{hex}{}", "x".repeat(2000)),
        ] {
            assert!(parse_qr(&text).is_err(), "{text}");
        }
    }
}
