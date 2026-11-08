//! HTTP for aggregators and AI: one client, short timeouts, a size limit on every answer.

use std::time::Duration;

use reqwest::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;

use crate::error::{AppError, AppResult};

/// No answer from a job feed or AI is ever this big - anything larger is refused.
pub const MAX_BODY: usize = 8 * 1024 * 1024;

/// The start of the message of a question that got no answer in time.
pub const TIMED_OUT: &str = "no answer in time";
/// The start of the message when the server could not be reached at all (no network, no DNS) -
/// the question never got there.
pub const NO_CONNECTION: &str = "no connection";

pub fn client() -> AppResult<Client> {
    let builder = Client::builder();
    // Android: rustls (tls.rs) instead of the bundled OpenSSL.
    #[cfg(target_os = "android")]
    let builder = builder.tls_backend_preconfigured(crate::tls::http_config()?);
    builder
        .user_agent(concat!("TablicaOfert/", env!("CARGO_PKG_VERSION"), " (personal job search app)"))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| AppError::Net(e.to_string()))
}

/// Sends once more after a short pause when the server says it is down for a moment
/// (502 / 503 / 504) - one failed answer should not wait hours for the next fetch.
pub async fn send_retry(req: RequestBuilder) -> AppResult<Response> {
    let again = req.try_clone();
    let resp = req.send().await.map_err(net_error)?;
    match again {
        Some(again) if passing_failure(resp.status().as_u16()) => {
            tokio::time::sleep(Duration::from_secs(3)).await;
            again.send().await.map_err(net_error)
        }
        _ => Ok(resp),
    }
}

/// The server's own short break, not a problem with what was asked.
fn passing_failure(status: u16) -> bool {
    matches!(status, 502..=504)
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
    let snippet = snippet(&body);
    Err(match status.as_u16() {
        401 | 403 => AppError::Auth(format!("{status}: {snippet}")),
        429 => AppError::Limit(format!("{status}: {snippet}")),
        _ => AppError::Net(format!("{status}: {snippet}")),
    })
}

/// The start of an error answer as one line of text. An HTML error page (a server down)
/// becomes its visible text, not markup.
pub fn snippet(body: &[u8]) -> String {
    let raw = String::from_utf8_lossy(body);
    let start: String = raw.trim_start().chars().take(14).collect::<String>().to_ascii_lowercase();
    let text = if start.starts_with("<!doctype html") || start.starts_with("<html") {
        crate::text::html_to_text(&raw)
    } else {
        raw.into_owned()
    };
    crate::text::one_line(&text, 300)
}

pub async fn json<T: DeserializeOwned>(resp: Response) -> AppResult<T> {
    let resp = check_status(resp).await?;
    let body = read_limited(resp, MAX_BODY).await?;
    serde_json::from_slice(&body).map_err(|e| AppError::Net(format!("unexpected answer: {e}")))
}

pub fn net_error(e: reqwest::Error) -> AppError {
    // Never include the address: it may carry an API key in the query.
    // A server that took the question but did not answer in time is told apart from no connection.
    let slow = e.is_timeout() && !e.is_connect();
    let offline = e.is_connect();
    let text = e.without_url().to_string();
    AppError::Net(if slow {
        format!("{TIMED_OUT}: {text}")
    } else if offline {
        format!("{NO_CONNECTION}: {text}")
    } else {
        text
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_short_break_of_the_server_is_asked_again() {
        assert!(passing_failure(503));
        assert!(passing_failure(502));
        assert!(!passing_failure(500));
        assert!(!passing_failure(429));
        assert!(!passing_failure(401));
    }

    #[test]
    fn html_error_page_becomes_text() {
        let page = "<!DOCTYPE html>\n<html> <!-- managed by a tool -->\n<head><meta charset=\"UTF-8\"><title>Oops</title></head>\n<body><h1>Something went wrong</h1><p>Try again later.</p></body></html>";
        assert_eq!(snippet(page.as_bytes()), "Something went wrong Try again later.");
        assert_eq!(snippet(b"{\"error\":\"bad key\"}"), "{\"error\":\"bad key\"}");
        assert_eq!(snippet(b"<html>"), "");
        assert_eq!(snippet("x".repeat(500).as_bytes()).chars().count(), 300);
    }
}
