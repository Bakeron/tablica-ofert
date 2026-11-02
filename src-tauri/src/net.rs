//! HTTP for aggregators and AI: one client, short timeouts, a size limit on every answer.

use std::time::Duration;

use reqwest::{Client, Response};
use serde::de::DeserializeOwned;

use crate::error::{AppError, AppResult};

/// No answer from a job feed or AI is ever this big - anything larger is refused.
pub const MAX_BODY: usize = 8 * 1024 * 1024;

pub fn client() -> AppResult<Client> {
    Client::builder()
        .user_agent(concat!("TablicaOfert/", env!("CARGO_PKG_VERSION"), " (personal job search app)"))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| AppError::Net(e.to_string()))
}

/// Reads the body, stopping at `max` bytes.
pub async fn read_limited(mut resp: Response, max: usize) -> AppResult<Vec<u8>> {
    if resp.content_length().is_some_and(|len| len as usize > max) {
        return Err(AppError::Net("the answer is too big".into()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(net_error)? {
        if body.len() + chunk.len() > max {
            return Err(AppError::Net("the answer is too big".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Turns a non-2xx answer into an error; 401/403 and 429 get their own codes for the UI.
pub async fn check_status(resp: Response) -> AppResult<Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    // A short piece of the body helps to understand the problem; never more.
    let body = read_limited(resp, 64 * 1024).await.unwrap_or_default();
    let snippet: String = String::from_utf8_lossy(&body).chars().take(300).collect();
    Err(match status.as_u16() {
        401 | 403 => AppError::Auth(format!("{status}: {snippet}")),
        429 => AppError::Limit(format!("{status}: {snippet}")),
        _ => AppError::Net(format!("{status}: {snippet}")),
    })
}

pub async fn json<T: DeserializeOwned>(resp: Response) -> AppResult<T> {
    let resp = check_status(resp).await?;
    let body = read_limited(resp, MAX_BODY).await?;
    serde_json::from_slice(&body).map_err(|e| AppError::Net(format!("unexpected answer: {e}")))
}

pub fn net_error(e: reqwest::Error) -> AppError {
    // Never include the address: it may carry an API key in the query.
    AppError::Net(e.without_url().to_string())
}
