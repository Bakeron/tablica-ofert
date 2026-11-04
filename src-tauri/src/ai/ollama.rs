//! Ollama on this computer: which models are there, and downloading one (with progress).
//! Only the address from the settings (checked `http(s)://host:port`) is ever called.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::model::{OnProgress, Progress};
use crate::net;

#[derive(Deserialize)]
struct Tags {
    #[serde(default)]
    models: Vec<Tag>,
}

#[derive(Deserialize)]
struct Tag {
    name: String,
}

/// Models already downloaded; an error = Ollama is not installed or not running.
pub async fn models(base: &str) -> AppResult<Vec<String>> {
    let resp = net::client()?
        .get(format!("{}/api/tags", base.trim_end_matches('/')))
        .send()
        .await
        .map_err(net::net_error)?;
    let tags: Tags = net::json(net::check_status(resp).await?).await?;
    Ok(tags.models.into_iter().map(|m| m.name).take(200).collect())
}

/// Model names as Ollama writes them: letters, digits and `-_.:/`.
pub fn is_model_name(v: &str) -> bool {
    !v.is_empty() && v.len() <= 120 && v.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:/".contains(c))
}

#[derive(Deserialize)]
struct PullLine {
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

/// Downloads a model (several GB - no overall time limit, only for the connection);
/// `progress` hears the percent of the current part.
pub async fn pull(base: &str, model: &str, progress: OnProgress<'_>) -> AppResult<()> {
    if !is_model_name(model) {
        return Err(AppError::Invalid("model name".into()));
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(net::net_error)?;
    let resp = client
        .post(format!("{}/api/pull", base.trim_end_matches('/')))
        .json(&json!({ "model": model, "stream": true }))
        .send()
        .await
        .map_err(net::net_error)?;
    let mut resp = net::check_status(resp).await?;
    // The answer is one JSON object per line.
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(net::net_error)? {
        buf.extend_from_slice(&chunk);
        while let Some(end) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=end).collect();
            let Ok(step) = serde_json::from_slice::<PullLine>(&line) else { continue };
            if let Some(e) = step.error {
                return Err(AppError::Net(crate::text::cut(&e, 300)));
            }
            if let (Some(total), Some(done)) = (step.total, step.completed) {
                if total > 0 {
                    let percent = (done.min(total) * 100 / total) as usize;
                    progress(Some(Progress::new("pull", Some(model), percent, 100)));
                }
            }
        }
        // A line never gets this long - something else answers.
        if buf.len() > 64 * 1024 {
            return Err(AppError::Net("unexpected answer from Ollama".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names() {
        assert!(is_model_name("qwen3:14b"));
        assert!(is_model_name("library/llama3.1:8b"));
        assert!(!is_model_name(""));
        assert!(!is_model_name("a b"));
        assert!(!is_model_name("x\"; rm"));
    }
}
