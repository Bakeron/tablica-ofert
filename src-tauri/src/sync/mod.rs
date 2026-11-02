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
const DEVICE: &str = "pc";

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
    let access = drive::access_token(&http, &oauth, &refresh).await?;
    Ok((http, access))
}

/// "Sync now". `confirm` = the person agreed to delete several offers at once.
pub async fn run(state: &AppState, confirm: bool) -> AppResult<SyncOutcome> {
    let result = run_inner(state, confirm).await;
    let error = result.as_ref().err().map(|e| crate::text::cut(&e.to_string(), 300));
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
            serde_json::from_slice::<merge::Snapshot>(&plain).map_err(|e| AppError::Invalid(format!("damaged sync file: {e}")))?
        }
        None => merge::Snapshot::default(),
    };
    let (merged, stats) = merge::merge(&local, &remote, &now);
    let removing = with_db(state, |c| data::offers_to_remove(c, &merged))?;
    if removing > ASK_ABOVE && !confirm {
        return Ok(SyncOutcome::Confirm { removed: removing });
    }
    with_db(state, |c| data::apply(c, &merged, &now))?;
    let json = serde_json::to_vec(&merged).map_err(|e| AppError::Invalid(e.to_string()))?;
    drive::upload(&http, &access, file.as_ref(), crypto::seal(&key, &json)?).await?;
    Ok(SyncOutcome::Done { changed: stats.changed, removed: stats.removed })
}

/// "Disconnect devices": a new key - an old phone can no longer read the file.
/// The file on Drive is re-encrypted with the new key right away.
pub async fn new_key(state: &AppState) -> AppResult<()> {
    let old = key()?;
    let fresh = crypto::new_key()?;
    if let (Some(old), Ok((http, access))) = (old, token(state).await) {
        if let Some(file) = drive::find(&http, &access).await? {
            let plain = crypto::open(&old, &drive::download(&http, &access, &file).await?)?;
            drive::upload(&http, &access, Some(&file), crypto::seal(&fresh, &plain)?).await?;
        }
    }
    secrets::set(KEY_SECRET, &crypto::key_to_hex(&fresh))
}

/// "Delete from Drive": removes the sync file (the data on this computer stays).
pub async fn delete_remote(state: &AppState) -> AppResult<()> {
    let (http, access) = token(state).await?;
    if let Some(file) = drive::find(&http, &access).await? {
        drive::delete(&http, &access, &file).await?;
    }
    Ok(())
}

/// QR code with the key for the phone (shown for 2 minutes, never sent anywhere).
pub fn qr_svg() -> AppResult<String> {
    let key = key_or_new()?;
    let payload = format!("tablica-ofert:sync:1:{}", crypto::key_to_hex(&key));
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|e| AppError::Invalid(e.to_string()))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(260, 260)
        .dark_color(qrcode::render::svg::Color("#111111"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}
