//! Google Drive, only the files this app made (`drive.file` - the app cannot see any other file).
//! Not `drive.appdata` (the hidden app folder): Google's device login rejects it with
//! "invalid_scope" (seen 5.11.2026), although its documentation lists it.
//! Login with a code in the normal browser (google.com/device) - Google blocks logins inside
//! an app's own window. The refresh token lives in the credential store.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::net;

const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const FILE_NAME: &str = "tablica-ofert.sync";
pub const REFRESH_SECRET: &str = "google-refresh-token";
/// The login has ended (taken back by the person, or 7 days passed in a project in testing mode).
/// The interface looks for this text to ask for a new login instead of showing an error.
pub const LOGIN_EXPIRED: &str = "google login expired";

/// The Google project of the app (a "TVs and limited input devices" OAuth client).
/// Built in from environment variables outside git, or typed in the settings.
pub struct OAuthClient {
    pub id: String,
    pub secret: String,
}

pub fn built_in_client() -> Option<OAuthClient> {
    let id = option_env!("TABLICA_GOOGLE_CLIENT_ID")?;
    let secret = option_env!("TABLICA_GOOGLE_CLIENT_SECRET")?;
    Some(OAuthClient { id: id.into(), secret: secret.into() })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    #[serde(alias = "device_code")]
    pub device_code: String,
    #[serde(alias = "user_code")]
    pub user_code: String,
    #[serde(alias = "verification_url")]
    pub verification_url: String,
    #[serde(alias = "expires_in")]
    pub expires_in: u64,
    #[serde(default = "five")]
    pub interval: u64,
}

fn five() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct Token {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
}

/// Step 1: a short code the person types at google.com/device.
pub async fn start_login(http: &Client, client: &OAuthClient) -> AppResult<DeviceCode> {
    let resp = http
        .post("https://oauth2.googleapis.com/device/code")
        .form(&[("client_id", client.id.as_str()), ("scope", SCOPE)])
        .send()
        .await
        .map_err(net::net_error)?;
    net::json(resp).await
}

/// Step 2: asks once whether the person finished. `Ok(None)` = not yet, ask again later.
/// Returns the refresh token (to keep in the credential store).
pub async fn poll_login(http: &Client, client: &OAuthClient, device_code: &str) -> AppResult<Option<String>> {
    let resp = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client.id.as_str()),
            ("client_secret", client.secret.as_str()),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(net::net_error)?;
    if resp.status().is_success() {
        let token: Token = net::json(resp).await?;
        return token.refresh_token.map(Some).ok_or_else(|| AppError::Auth("Google gave no refresh token".into()));
    }
    let body = net::read_limited(resp, 64 * 1024).await?;
    let err: TokenError = serde_json::from_slice(&body).map_err(|_| AppError::Net("unexpected answer from Google".into()))?;
    match err.error.as_str() {
        "authorization_pending" | "slow_down" => Ok(None),
        "access_denied" => Err(AppError::Auth("access denied".into())),
        "expired_token" => Err(AppError::Auth("the code expired - start again".into())),
        other => Err(AppError::Net(other.to_string())),
    }
}

pub async fn access_token(http: &Client, client: &OAuthClient, refresh: &str) -> AppResult<String> {
    let resp = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client.id.as_str()),
            ("client_secret", client.secret.as_str()),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(net::net_error)?;
    if resp.status() == reqwest::StatusCode::BAD_REQUEST {
        let body = net::read_limited(resp, 64 * 1024).await?;
        return Err(refresh_refused(&body));
    }
    let token: Token = net::json(resp).await?;
    Ok(token.access_token)
}

/// Google refused the refresh token: `invalid_grant` = the login has ended, connect again.
fn refresh_refused(body: &[u8]) -> AppError {
    match serde_json::from_slice::<TokenError>(body) {
        Ok(e) if e.error == "invalid_grant" => AppError::Missing(LOGIN_EXPIRED.into()),
        Ok(e) => AppError::Net(e.error),
        Err(_) => AppError::Net("unexpected answer from Google".into()),
    }
}

#[derive(Debug, Deserialize)]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriveFile {
    pub id: String,
}

/// The sync file this app made, if there is one (the newest, should the user copy it).
pub async fn find(http: &Client, token: &str) -> AppResult<Option<DriveFile>> {
    let resp = http
        .get("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(token)
        .query(&[
            ("spaces", "drive"),
            ("q", &format!("name = '{FILE_NAME}' and trashed = false")),
            ("orderBy", "modifiedTime desc"),
            ("fields", "files(id)"),
        ])
        .send()
        .await
        .map_err(net::net_error)?;
    let list: FileList = net::json(resp).await?;
    Ok(list.files.into_iter().next())
}

pub async fn download(http: &Client, token: &str, file: &DriveFile) -> AppResult<Vec<u8>> {
    let resp = http
        .get(format!("https://www.googleapis.com/drive/v3/files/{}", file.id))
        .bearer_auth(token)
        .query(&[("alt", "media")])
        .send()
        .await
        .map_err(net::net_error)?;
    net::read_limited(net::check_status(resp).await?, 100 * 1024 * 1024).await
}

/// Creates the file or replaces its content.
pub async fn upload(http: &Client, token: &str, existing: Option<&DriveFile>, bytes: Vec<u8>) -> AppResult<()> {
    let resp = match existing {
        Some(file) => http
            .patch(format!("https://www.googleapis.com/upload/drive/v3/files/{}", file.id))
            .bearer_auth(token)
            .query(&[("uploadType", "media")])
            .header("Content-Type", "application/octet-stream")
            .body(bytes)
            .send()
            .await
            .map_err(net::net_error)?,
        None => {
            // Multipart: metadata (name; the file lands in "My Drive") and the content.
            let boundary = format!("tablica{}", crate::secrets::random_hex(8)?);
            let meta = serde_json::json!({ "name": FILE_NAME }).to_string();
            let mut body = format!(
                "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .into_bytes();
            body.extend_from_slice(&bytes);
            body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            http.post("https://www.googleapis.com/upload/drive/v3/files")
                .bearer_auth(token)
                .query(&[("uploadType", "multipart")])
                .header("Content-Type", format!("multipart/related; boundary={boundary}"))
                .body(body)
                .send()
                .await
                .map_err(net::net_error)?
        }
    };
    net::check_status(resp).await?;
    Ok(())
}

/// "Delete from Drive".
pub async fn delete(http: &Client, token: &str, file: &DriveFile) -> AppResult<()> {
    let resp = http
        .delete(format!("https://www.googleapis.com/drive/v3/files/{}", file.id))
        .bearer_auth(token)
        .send()
        .await
        .map_err(net::net_error)?;
    net::check_status(resp).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ended_login_asks_to_connect_again() {
        let ended = refresh_refused(br#"{"error": "invalid_grant", "error_description": "Token has been expired or revoked."}"#);
        assert!(matches!(ended, AppError::Missing(m) if m == LOGIN_EXPIRED));
        assert!(matches!(refresh_refused(br#"{"error": "invalid_client"}"#), AppError::Net(_)));
        assert!(matches!(refresh_refused(b"<html>"), AppError::Net(_)));
    }

    #[test]
    fn device_code_answer_is_read() {
        let json = r#"{"device_code":"AH-1","user_code":"ABCD-EFGH","verification_url":"https://www.google.com/device","expires_in":1800,"interval":5}"#;
        let code: DeviceCode = serde_json::from_str(json).unwrap();
        assert_eq!(code.user_code, "ABCD-EFGH");
        assert_eq!(code.interval, 5);
    }
}
