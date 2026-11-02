//! Offer page preview: a link opens in a separate window, like in a browser - a person looks
//! at the page and clicks "Take the text from the page". Not a robot: one page, on a click.
//!
//! The preview window has no access to the app (it is not in any capability). The text comes
//! back by a navigation to a made-up address with a one-time code, which the app cancels and
//! reads - the page itself never gets a way to call the app.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

use crate::error::{AppError, AppResult};
use crate::text::{cut, one_line, squeeze};

pub const LABEL: &str = "preview";
const CAPTURE_HOST: &str = "capture.tablica-ofert.invalid";
const MAX_TEXT: usize = 60_000;

/// Waiting for the text of the page: one-time code and where to send the answer.
#[derive(Default)]
pub struct Pending(pub Mutex<Option<(String, oneshot::Sender<Captured>)>>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Captured {
    pub url: String,
    pub title: String,
    pub text: String,
}

/// Opens (or reuses) the preview window with an offer page.
pub fn open(app: &AppHandle, url: &str) -> AppResult<()> {
    let url = crate::offers::safe_url(Some(url.to_string()))?.ok_or_else(|| AppError::Invalid("the link is empty".into()))?;
    let parsed = Url::parse(&url).map_err(|_| AppError::Invalid("the link must start with http:// or https://".into()))?;
    if let Some(window) = app.get_webview_window(LABEL) {
        window.navigate(parsed).map_err(|e| AppError::Invalid(e.to_string()))?;
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    let handle = app.clone();
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(parsed))
        .title("Tablica Ofert - podgląd strony")
        .inner_size(1100.0, 800.0)
        .on_navigation(move |target| {
            if target.host_str() != Some(CAPTURE_HOST) {
                // Only web pages - no `file:`, no custom schemes.
                return matches!(target.scheme(), "http" | "https");
            }
            receive(&handle, target);
            false
        })
        .build()
        .map_err(|e| AppError::Invalid(e.to_string()))?;
    Ok(())
}

fn receive(app: &AppHandle, target: &Url) {
    let Some(pending) = app.try_state::<Pending>() else { return };
    let Ok(mut slot) = pending.0.lock() else { return };
    let code = target.path().trim_start_matches('/');
    let matches = slot.as_ref().is_some_and(|(expected, _)| expected == code);
    if !matches {
        return;
    }
    let Some((_, sender)) = slot.take() else { return };
    let Some(fragment) = target.fragment() else { return };
    let decoded = percent_decode(fragment);
    if let Some(captured) = parse_capture(&decoded) {
        let _ = sender.send(captured);
    }
}

/// Text sent by the page - checked like any outside data.
pub fn parse_capture(json: &str) -> Option<Captured> {
    #[derive(Deserialize)]
    struct Raw {
        u: String,
        h: String,
        t: String,
    }
    let raw: Raw = serde_json::from_str(json).ok()?;
    let url = crate::offers::safe_url(Some(raw.u)).ok().flatten()?;
    Some(Captured { url, title: one_line(&raw.h, 300), text: cut(&squeeze(&raw.t), MAX_TEXT) })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// "Take the text from the page": asks the preview window for its text and waits for it.
pub async fn capture(app: &AppHandle) -> AppResult<Captured> {
    let window = app.get_webview_window(LABEL).ok_or_else(|| AppError::Invalid("the preview is closed".into()))?;
    let code = crate::secrets::random_hex(16)?;
    let (sender, receiver) = oneshot::channel();
    {
        let pending = app.state::<Pending>();
        let mut slot = pending.0.lock().map_err(|_| AppError::Invalid("internal state is broken".into()))?;
        *slot = Some((code.clone(), sender));
    }
    // `code` is hex - safe inside the script.
    let script = format!(
        "(function(){{try{{var b=document.body?document.body.innerText:'';\
var h=document.querySelector('h1');h=h?h.innerText:document.title;\
var d=JSON.stringify({{u:location.href,h:String(h||'').slice(0,300),t:String(b).slice(0,{MAX_TEXT})}});\
location.href='https://{CAPTURE_HOST}/{code}#'+encodeURIComponent(d);}}catch(e){{}}}})();"
    );
    window.eval(&script).map_err(|e| AppError::Invalid(e.to_string()))?;
    match tokio::time::timeout(Duration::from_secs(8), receiver).await {
        Ok(Ok(captured)) => Ok(captured),
        _ => Err(AppError::Invalid("the page did not give its text - copy and paste it instead".into())),
    }
}

pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_is_checked() {
        let ok = parse_capture(r#"{"u":"https://example.com/oferta/1","h":"  Frontend\nDeveloper ","t":"Opis\n\n\n\noferty\u0007"}"#).unwrap();
        assert_eq!(ok.title, "Frontend Developer");
        assert_eq!(ok.text, "Opis\n\noferty");
        assert!(parse_capture(r#"{"u":"javascript:alert(1)","h":"x","t":"y"}"#).is_none());
        assert!(parse_capture("not json").is_none());
    }

    #[test]
    fn decoding() {
        assert_eq!(percent_decode("%7B%22a%22%3A%22%C5%82%22%7D"), "{\"a\":\"ł\"}");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
