//! Calls to AI providers. Two kinds of API cover all of them:
//! - OpenAI-compatible chat (Gemini, Groq, OpenRouter, Ollama, OpenAI),
//! - Anthropic Messages API (Claude).
//! Keys come from the credential store and go only into the request header.

use std::sync::Mutex;
use std::time::Duration;

use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::spend::Tokens;
use crate::error::{AppError, AppResult};
use crate::net;
use crate::text::one_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Gemini,
    Groq,
    OpenRouter,
    Ollama,
    Anthropic,
    OpenAi,
}

impl Provider {
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "gemini" => Self::Gemini,
            "groq" => Self::Groq,
            "openrouter" => Self::OpenRouter,
            "ollama" => Self::Ollama,
            "anthropic" => Self::Anthropic,
            "openai" => Self::OpenAi,
            _ => return None,
        })
    }

    pub fn key_name(self) -> Option<&'static str> {
        match self {
            Self::Gemini => Some("ai-key-gemini"),
            Self::Groq => Some("ai-key-groq"),
            Self::OpenRouter => Some("ai-key-openrouter"),
            Self::Anthropic => Some("ai-key-anthropic"),
            Self::OpenAi => Some("ai-key-openai"),
            Self::Ollama => None,
        }
    }

    /// Model used when the settings leave it empty (checked with the providers on 7.11.2026).
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Gemini => "gemini-3.5-flash-lite",
            Self::Groq => "openai/gpt-oss-120b",
            // Picks a free model that is not busy at the moment (one popular model is often rate-limited upstream).
            Self::OpenRouter => "openrouter/free",
            Self::Ollama => "llama3.1:8b",
            // Reads fields and rates as well as Opus here, at half its price (test of 11.11.2026).
            Self::Anthropic => "claude-sonnet-5-5",
            Self::OpenAi => "gpt-6-luna",
        }
    }

    /// Free models offered next to the "Model" field when the provider's own list cannot be
    /// read (checked on 7.11.2026 - models come and go, the list from the provider wins).
    /// Paid providers (Gemini among them) and Ollama (its own downloaded models) have none.
    pub fn free_models(self) -> &'static [&'static str] {
        match self {
            Self::Groq => &["openai/gpt-oss-120b", "openai/gpt-oss-20b", "llama-3.3-70b-versatile", "llama-3.1-8b-instant", "qwen/qwen3.8-27b"],
            Self::OpenRouter => &["openrouter/free", "google/gemma-4-31b-it:free", "google/gemma-4-26b-a4b-it:free", "nvidia/nemotron-3.5-super-vl:free"],
            Self::Ollama | Self::Anthropic | Self::OpenAi | Self::Gemini => &[],
        }
    }

    /// Pause between calls, so free per-minute limits are not hit.
    pub fn pause(self) -> Duration {
        Duration::from_millis(match self {
            // Its free plan reads 8 thousand tokens a minute and one rating takes about 4 thousand.
            Self::Groq => 30_000,
            Self::OpenRouter => 3_500,
            Self::Ollama => 0,
            // Paid (Gemini too: Google gives its free plan only to some accounts and projects).
            Self::Anthropic | Self::OpenAi | Self::Gemini => 500,
        })
    }

    pub fn usage_key(self) -> String {
        format!("ai:{}", self.as_str())
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gemini => "gemini",
            Self::Groq => "groq",
            Self::OpenRouter => "openrouter",
            Self::Ollama => "ollama",
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
        }
    }
}

pub struct Ai {
    pub provider: Provider,
    pub model: String,
    key: Option<String>,
    /// Only for Ollama.
    base_url: String,
    http: Client,
    /// Claude's `effort` - how long the model thinks before it answers.
    pub effort: &'static str,
    /// Tokens of Claude's answers not yet written to the spending counter (`take_spent`).
    spent: Mutex<Vec<(String, Tokens)>>,
}

impl Ai {
    pub fn new(provider: Provider, model: String, key: Option<String>, ollama_url: String) -> AppResult<Self> {
        if provider.key_name().is_some() && key.is_none() {
            return Err(AppError::Missing("ai key".into()));
        }
        // Reading one offer is a simple task - low effort keeps it fast and cheap.
        Ok(Ai { provider, model, key, base_url: ollama_url.trim_end_matches('/').to_string(), http: net::client()?, effort: "low", spent: Mutex::default() })
    }

    fn chat_url(&self) -> String {
        match self.provider {
            Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".into(),
            Provider::Groq => "https://api.groq.com/openai/v1/chat/completions".into(),
            Provider::OpenRouter => "https://openrouter.ai/api/v1/chat/completions".into(),
            Provider::OpenAi => "https://api.openai.com/v1/chat/completions".into(),
            Provider::Ollama => format!("{}/v1/chat/completions", self.base_url),
            Provider::Anthropic => "https://api.anthropic.com/v1/messages".into(),
        }
    }

    /// One question, one answer. `json` = the answer must be a JSON object.
    pub async fn ask(&self, system: &str, user: &str, json: bool) -> AppResult<String> {
        Ok(self.ask_full(system, user, json).await?.0)
    }

    /// One question whose beginning (`stable`) is the same in many calls and whose end (`varying`)
    /// changes. Claude is told to keep the beginning in its prompt cache (read back about 10 times
    /// cheaper within 5 minutes); OpenAI-compatible providers cache the same beginning on their own.
    pub async fn ask_cached(&self, system: &str, stable: &str, varying: &str, json: bool) -> AppResult<String> {
        Ok(self.ask_cached_usage(system, stable, varying, json).await?.0)
    }

    /// `ask_cached` with the provider's token counts (`usage` of its answer, when it gives one).
    pub async fn ask_cached_usage(&self, system: &str, stable: &str, varying: &str, json: bool) -> AppResult<(String, Option<Value>)> {
        let (text, _, usage) = match self.provider {
            Provider::Anthropic => {
                let content = json!([
                    { "type": "text", "text": stable, "cache_control": { "type": "ephemeral" } },
                    { "type": "text", "text": varying },
                ]);
                self.ask_anthropic(system, content).await?
            }
            _ => self.ask_openai(system, &format!("{stable}\n\n{varying}"), json).await?,
        };
        Ok((text, usage))
    }

    /// The answer and the model that wrote it (as the provider names it).
    async fn ask_full(&self, system: &str, user: &str, json: bool) -> AppResult<(String, Option<String>)> {
        let (text, model, _) = match self.provider {
            Provider::Anthropic => self.ask_anthropic(system, json!(user)).await?,
            _ => self.ask_openai(system, user, json).await?,
        };
        Ok((text, model))
    }

    /// "Check connection": one tiny question with the model from the settings. Returns the model
    /// that answered. An empty answer still means the key, the API and the model work.
    pub async fn check(&self) -> AppResult<String> {
        match self.ask_full("Answer with the single word OK.", "Are you there?", false).await {
            Ok((_, model)) => Ok(model.filter(|m| !m.trim().is_empty()).map_or_else(|| self.model.clone(), |m| one_line(&m, 120))),
            Err(AppError::Net(msg)) if msg == EMPTY => Ok(self.model.clone()),
            Err(e) => Err(e),
        }
    }

    async fn ask_openai(&self, system: &str, user: &str, json: bool) -> AppResult<(String, Option<String>, Option<Value>)> {
        let mut body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });
        if json {
            body["response_format"] = json!({ "type": "json_object" });
        }
        // The same offer read again gives about the same score (the default is up to 1.0).
        // OpenAI's newest models take only the default, so it is not sent there.
        if self.provider != Provider::OpenAi {
            body["temperature"] = json!(0.2);
        }
        let mut req = self.http.post(self.chat_url()).json(&body);
        // A model on the processor alone needs a minute or more for one offer.
        if self.provider == Provider::Ollama {
            req = req.timeout(std::time::Duration::from_secs(300));
        }
        if let Some(key) = &self.key {
            req = req.bearer_auth(key);
        }
        if self.provider == Provider::OpenRouter {
            req = req.header("X-Title", "Tablica Ofert");
        }
        let answer: OpenAiAnswer = answer(req.send().await.map_err(net::net_error)?).await?;
        let (model, usage) = (answer.model, answer.usage);
        let choice = answer.choices.into_iter().next().ok_or_else(|| AppError::Net(EMPTY.into()))?;
        if choice.finish_reason.as_deref() == Some("content_filter") {
            return Err(AppError::Net("the provider refused to answer".into()));
        }
        let text = choice.message.content.filter(|c| !c.trim().is_empty()).ok_or_else(|| AppError::Net(EMPTY.into()))?;
        Ok((text, model, usage))
    }

    /// Claude: JSON is asked for in the prompt (the answer is checked field by field anyway).
    /// `content` - the user's text, or its blocks (with a prompt-cache mark).
    async fn ask_anthropic(&self, system: &str, content: Value) -> AppResult<(String, Option<String>, Option<Value>)> {
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": system,
            "messages": [{ "role": "user", "content": content }],
            "output_config": { "effort": self.effort },
            // A safety decline is retried on a fallback model chosen by the API.
            "fallbacks": "default",
        });
        let req = self
            .http
            .post(self.chat_url())
            .header("x-api-key", self.key.as_deref().unwrap_or_default())
            .header("anthropic-version", "2026-10-21")
            .header("anthropic-beta", "server-side-fallback-2026-07-01")
            .json(&body);
        let answer: AnthropicAnswer = answer(req.send().await.map_err(net::net_error)?).await?;
        // Paid even when the answer turns out refused or empty.
        self.note_spent(answer.model.as_deref(), answer.usage.as_ref());
        if answer.stop_reason.as_deref() == Some("refusal") {
            return Err(AppError::Net("the model refused to answer".into()));
        }
        let text: String = answer
            .content
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
        if text.trim().is_empty() {
            return Err(AppError::Net(EMPTY.into()));
        }
        Ok((text, answer.model, answer.usage))
    }

    /// Keeps the token counts of one answer. The model that answered sets the price (a safety
    /// fallback may answer instead of the one asked); without its name - the one asked.
    pub(super) fn note_spent(&self, answered_by: Option<&str>, usage: Option<&Value>) {
        if let Some(tokens) = usage.and_then(Tokens::from_usage) {
            let model = answered_by.filter(|m| !m.trim().is_empty()).unwrap_or(&self.model).to_string();
            self.spent.lock().unwrap_or_else(|e| e.into_inner()).push((model, tokens));
        }
    }

    /// Claude's token counts since the last call of this (model, tokens) - for the spending counter.
    pub fn take_spent(&self) -> Vec<(String, Tokens)> {
        std::mem::take(&mut *self.spent.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Free models this key can use, as the provider lists them today (paid providers: none).
    /// Ollama has its own list of downloaded models (`ollama::models`).
    pub async fn free_models(&self) -> AppResult<Vec<String>> {
        let mut names: Vec<String> = match self.provider {
            Provider::Groq => {
                let req = self.http.get("https://api.groq.com/openai/v1/models").bearer_auth(self.key.as_deref().unwrap_or_default());
                let list: OpenAiModels = answer(req.send().await.map_err(net::net_error)?).await?;
                list.data.into_iter().filter(|m| m.active != Some(false)).map(|m| m.id).filter(|n| chat_groq(n)).collect()
            }
            Provider::OpenRouter => return openrouter_free_models().await,
            Provider::Ollama | Provider::Anthropic | Provider::OpenAi | Provider::Gemini => Vec::new(),
        };
        tidy_names(&mut names);
        Ok(names)
    }
}

/// OpenRouter's free models - the list is public, so it works before a key is saved (no key is sent).
pub async fn openrouter_free_models() -> AppResult<Vec<String>> {
    let resp = net::client()?.get("https://openrouter.ai/api/v1/models").send().await.map_err(net::net_error)?;
    let list: OpenAiModels = answer(resp).await?;
    let mut names: Vec<String> = list.data.into_iter().map(|m| m.id).filter(|n| free_openrouter(n)).collect();
    tidy_names(&mut names);
    Ok(names)
}

/// Outside data: only plain model names, a short sorted list.
fn tidy_names(names: &mut Vec<String>) {
    names.retain(|n| is_model_name(n));
    names.sort();
    names.dedup();
    names.truncate(40);
}

const EMPTY: &str = "empty answer";

/// Model names as providers write them: letters, digits and `-_.:/@`.
fn is_model_name(v: &str) -> bool {
    !v.is_empty() && v.len() <= 120 && v.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:/@".contains(c))
}

/// Groq lists speech and safety models too - only chat models are offered.
fn chat_groq(name: &str) -> bool {
    !["whisper", "orpheus", "guard", "tts", "playai", "distil"].iter().any(|w| name.contains(w))
}

/// OpenRouter: free models end with ":free"; "openrouter/free" picks one of them itself.
fn free_openrouter(name: &str) -> bool {
    name.ends_with(":free") || name == "openrouter/free"
}

/// What went wrong with an AI provider, in words the settings can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    /// The provider refused the key.
    Key,
    /// The API is turned off in the provider's project (Google Cloud).
    ApiOff,
    /// Google gives this project no free plan (a new key in it does not help).
    NoFreeTier,
    /// The project waits for a phone verification in Google AI Studio.
    Verify,
    /// The provider refused this account or project for another reason.
    Denied,
    /// No such model, or not for this account or plan.
    Model,
    /// The provider's own limit is used.
    Limit,
    /// The model is busy for a moment (a free model shared by many people, an overloaded server) -
    /// not a daily limit: ask again in a few minutes or take another model.
    Busy,
    /// The daily limit from the settings is used.
    Cap,
    /// No key in the settings.
    NoKey,
    /// No connection, or the provider does not answer.
    Offline,
    /// An answer came, but empty or broken.
    Answer,
    Other,
}

/// The kind of an error from AI (also for errors that never reached the provider).
pub fn problem(e: &AppError) -> Problem {
    match e {
        AppError::Provider { kind, .. } => *kind,
        AppError::Missing(_) => Problem::NoKey,
        AppError::Limit(_) => Problem::Cap,
        AppError::Net(msg)
            if msg == EMPTY || msg.contains("JSON") || msg.contains("refused") || msg.starts_with("unexpected answer") =>
        {
            Problem::Answer
        }
        AppError::Net(_) => Problem::Offline,
        _ => Problem::Other,
    }
}

/// How long the provider asks to wait ("Please try again in 7.5s", "in 1m2.5s"), when it says so.
pub fn retry_after(e: &AppError) -> Option<std::time::Duration> {
    let AppError::Provider { message, .. } = e else { return None };
    let lower = message.to_lowercase();
    // Groq, OpenAI: "try again in 7.5s"; Gemini: "Please retry in 33s".
    let start = ["try again in ", "retry in "].iter().find_map(|w| lower.find(w).map(|i| i + w.len()))?;
    let rest = &lower[start..];
    let mut seconds = 0.0;
    let mut number = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '0'..='9' | '.' => number.push(c),
            // "260ms" - milliseconds, not minutes.
            'm' if !number.is_empty() && chars.peek() == Some(&'s') => {
                seconds += number.parse::<f64>().ok()? / 1000.0;
                break;
            }
            'm' if !number.is_empty() => {
                seconds += number.parse::<f64>().ok()? * 60.0;
                number.clear();
            }
            's' if !number.is_empty() => {
                seconds += number.parse::<f64>().ok()?;
                break;
            }
            _ => break,
        }
    }
    (seconds > 0.0 && seconds.is_finite()).then(|| std::time::Duration::from_secs_f64(seconds.min(3600.0)))
}

/// The provider's own words about the problem, without the technical prefix.
pub fn problem_message(e: &AppError) -> String {
    match e {
        AppError::Provider { message, .. } => message.clone(),
        AppError::Limit(_) | AppError::Missing(_) => String::new(),
        other => one_line(&other.to_string(), 500),
    }
}

/// What an error answer says - read from the whole answer, before it is cut for display.
fn problem_in(status: u16, body: &str) -> Problem {
    let t = body.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&["free_tier_verification_required", "verification needed", "verification required"]) {
        return Problem::Verify;
    }
    if has(&["free tier is not available", "free tier not available", "not eligible for the free tier"]) {
        return Problem::NoFreeTier;
    }
    if has(&["service_disabled", "has not been used in project", "it is disabled", "api is not enabled", "api has not been enabled"]) {
        return Problem::ApiOff;
    }
    if has(&[
        "api_key_invalid", "api key not valid", "api key expired", "invalid api key", "incorrect api key", "invalid_api_key",
        "invalid x-api-key", "authentication_error", "reported as leaked", "api key was blocked",
    ]) {
        return Problem::Key;
    }
    // Google answers "limit: 0" when the free plan of this account does not include the model.
    if t.split("limit:").skip(1).any(|rest| rest.trim_start().starts_with('0') && !rest.trim_start()[1..].starts_with(|c: char| c.is_ascii_digit())) {
        return Problem::Model;
    }
    // A limit per minute (Groq: tokens or requests per minute) or a model busy for everybody
    // (OpenRouter's free models) - not the daily limit: a short wait is enough.
    if has(&["rate-limited upstream", "temporarily rate-limited", "free-models-per-min", "overloaded", "per minute", "perminute", "(tpm)", "(rpm)"]) {
        return Problem::Busy;
    }
    let model_gone = has(&[
        "not found", "does not exist", "not supported", "no longer available", "not available", "decommissioned", "no endpoints found",
        "invalid model", "unknown model", "is not a valid model", "model_not_found",
    ]);
    if status == 404 || (t.contains("model") && model_gone) {
        return Problem::Model;
    }
    match status {
        401 => Problem::Key,
        429 => Problem::Limit,
        _ if has(&["resource_exhausted", "quota", "rate limit"]) => Problem::Limit,
        403 => Problem::Denied,
        _ => Problem::Other,
    }
}

/// The message inside an error answer: OpenAI-style `{"error": {"message"}}`, Google's list of
/// such objects, Anthropic's `{"error": {"message"}}`, or `{"error": "..."}`.
fn provider_message(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body.trim()).ok()?;
    let v = if let Value::Array(items) = v { items.into_iter().next()? } else { v };
    let text = v
        .get("error")
        .and_then(|e| e.get("message").and_then(Value::as_str).or_else(|| e.as_str()))
        .or_else(|| v.get("message").and_then(Value::as_str))
        .or_else(|| v.get("detail").and_then(Value::as_str))?;
    let line = one_line(text, 500);
    (!line.is_empty()).then_some(line)
}

pub fn answer_error(status: u16, body: &[u8]) -> AppError {
    let text = String::from_utf8_lossy(body);
    let message = provider_message(&text).unwrap_or_else(|| net::snippet(body));
    AppError::Provider { kind: problem_in(status, &text), status, message }
}

/// A provider's answer as JSON; an error answer becomes `AppError::Provider` with its kind.
async fn answer<T: DeserializeOwned>(resp: Response) -> AppResult<T> {
    let status = resp.status();
    if status.is_success() {
        return net::json(resp).await;
    }
    let body = net::read_limited(resp, 64 * 1024).await.unwrap_or_default();
    Err(answer_error(status.as_u16(), &body))
}

#[derive(Debug, Deserialize)]
struct OpenAiModels {
    #[serde(default)]
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
    #[serde(default)]
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OpenAiAnswer {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicAnswer {
    #[serde(default)]
    content: Vec<Value>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<Value>,
}

/// The JSON object inside an answer - models sometimes wrap it in ``` fences or add a sentence.
pub fn json_object(text: &str) -> AppResult<Value> {
    let start = text.find('{').ok_or_else(|| AppError::Net("the answer has no JSON".into()))?;
    let end = text.rfind('}').ok_or_else(|| AppError::Net("the answer has no JSON".into()))?;
    if end < start {
        return Err(AppError::Net("the answer has no JSON".into()));
    }
    serde_json::from_str(&text[start..=end]).map_err(|e| AppError::Net(format!("broken JSON from AI: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_is_found_in_a_chatty_answer() {
        let v = json_object("Sure! ```json\n{\"a\": 1, \"b\": {\"c\": \"}\"}}\n``` hope it helps").unwrap();
        assert_eq!(v["a"], 1);
        assert!(json_object("no json here").is_err());
        assert!(json_object("} {").is_err());
    }

    #[test]
    fn error_answers_are_named_in_plain_words() {
        let gemini_key = r#"[{"error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT", "details": [{"reason": "API_KEY_INVALID"}]}}]"#;
        let e = answer_error(400, gemini_key.as_bytes());
        assert_eq!(problem(&e), Problem::Key);
        assert_eq!(e.code(), "auth");
        assert_eq!(problem_message(&e), "API key not valid. Please pass a valid API key.");
        let no_free = r#"{"error": {"code": 403, "message": "The Gemini API free tier is not available for this project. To use the Gemini API, please set up billing to upgrade to the paid tier at https://aistudio.google.com/billing. For more information, see https://ai.google.dev/gemini-api/docs/troubleshooting#project-unavailable.", "status": "PERMISSION_DENIED"}}"#;
        let e = answer_error(403, no_free.as_bytes());
        assert_eq!((problem(&e), e.code()), (Problem::NoFreeTier, "auth"));
        let verify = r#"{"error": {"code": 403, "message": "Your project has been denied access.", "status": "PERMISSION_DENIED", "details": [{"reason": "FREE_TIER_VERIFICATION_REQUIRED"}]}}"#;
        assert_eq!(problem(&answer_error(403, verify.as_bytes())), Problem::Verify);
        let denied = r#"{"error": {"code": 403, "message": "Your project has been denied access.", "status": "PERMISSION_DENIED"}}"#;
        assert_eq!(problem(&answer_error(403, denied.as_bytes())), Problem::Denied);
        let off = r#"{"error": {"code": 403, "message": "Generative Language API has not been used in project 123 before or it is disabled.", "status": "PERMISSION_DENIED", "details": [{"reason": "SERVICE_DISABLED"}]}}"#;
        assert_eq!(problem(&answer_error(403, off.as_bytes())), Problem::ApiOff);
        let zero = r#"{"error": {"code": 429, "message": "You exceeded your current quota, please check your plan and billing details. For more information on this error, head to: https://ai.google.dev/gemini-api/docs/rate-limits. To monitor your current usage, head to: https://ai.dev/rate-limit. \n* Quota exceeded for metric: generativelanguage.googleapis.com/generate_content_free_tier_requests, limit: 0, model: gemini-2.5-flash", "status": "RESOURCE_EXHAUSTED"}}"#;
        let e = answer_error(429, zero.as_bytes());
        assert_eq!(problem(&e), Problem::Model);
        assert_eq!(e.code(), "net");
        let upstream = r#"{"error":{"message":"Provider returned error","code":429,"metadata":{"raw":"google/gemma-4-31b-it:free is temporarily rate-limited upstream. Please retry shortly, or add your own key to accumulate your rate limits","provider_name":"Google AI Studio"}}}"#;
        let e = answer_error(429, upstream.as_bytes());
        assert_eq!((problem(&e), e.code()), (Problem::Busy, "net"));
        let groq_tpm = r#"{"error":{"message":"Rate limit reached for model `openai/gpt-oss-120b` in organization `org_x` service tier `on_demand` on tokens per minute (TPM): Limit 8000, Used 6500, Requested 3200. Please try again in 12.75s. Need more tokens?","type":"tokens","code":"rate_limit_exceeded"}}"#;
        let e = answer_error(429, groq_tpm.as_bytes());
        assert_eq!(problem(&e), Problem::Busy);
        assert_eq!(retry_after(&e), Some(std::time::Duration::from_secs_f64(12.75)));
        let groq_tpd = r#"{"error":{"message":"Rate limit reached for model `x` on tokens per day (TPD): Limit 200000, Used 199000. Please try again in 1m2.5s.","code":"rate_limit_exceeded"}}"#;
        let e = answer_error(429, groq_tpd.as_bytes());
        assert_eq!(problem(&e), Problem::Limit);
        assert_eq!(retry_after(&e), Some(std::time::Duration::from_secs_f64(62.5)));
        assert_eq!(retry_after(&AppError::Net("timeout".into())), None);
        // Milliseconds are not minutes.
        let groq_ms = r#"{"error":{"message":"Rate limit reached on requests per minute (RPM). Please try again in 260ms.","code":"rate_limit_exceeded"}}"#;
        assert_eq!(retry_after(&answer_error(429, groq_ms.as_bytes())), Some(std::time::Duration::from_secs_f64(0.26)));
        // Every kind of unusable answer is "Answer" (the offer is marked, the queue goes on).
        assert_eq!(problem(&AppError::Net("unexpected answer: missing field `choices`".into())), Problem::Answer);
        assert_eq!(problem(&AppError::Net("the model refused to answer".into())), Problem::Answer);
        let per_day = r#"{"error":{"message":"Rate limit exceeded: free-models-per-day. Add 10 credits to unlock 1000 free model requests per day","code":429}}"#;
        assert_eq!(problem(&answer_error(429, per_day.as_bytes())), Problem::Limit);
        let quota = r#"{"error": {"code": 429, "message": "Quota exceeded for metric: x, limit: 20, model: gemini-3.5-flash-lite", "status": "RESOURCE_EXHAUSTED"}}"#;
        let e = answer_error(429, quota.as_bytes());
        assert_eq!((problem(&e), e.code()), (Problem::Limit, "limit"));
        let gone = r#"{"error": {"code": 404, "message": "models/gemini-9 is not found for API version v1beta, or is not supported for generateContent.", "status": "NOT_FOUND"}}"#;
        assert_eq!(problem(&answer_error(404, gone.as_bytes())), Problem::Model);
        let groq = r#"{"error":{"message":"The model `x` does not exist or you do not have access to it.","type":"invalid_request_error","code":"model_not_found"}}"#;
        assert_eq!(problem(&answer_error(404, groq.as_bytes())), Problem::Model);
        let anthropic = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert_eq!(problem(&answer_error(401, anthropic.as_bytes())), Problem::Key);
        let ollama = r#"{"error":{"message":"model \"llama9\" not found, try pulling it first","type":"api_error"}}"#;
        assert_eq!(problem(&answer_error(404, ollama.as_bytes())), Problem::Model);
        let e = answer_error(500, b"<html><body><h1>Server error</h1></body></html>");
        assert_eq!((problem(&e), problem_message(&e).as_str()), (Problem::Other, "Server error"));
        assert_eq!(problem(&AppError::Net("timeout".into())), Problem::Offline);
        assert_eq!(problem(&AppError::Net(EMPTY.into())), Problem::Answer);
        assert_eq!(problem(&AppError::Missing("ai key".into())), Problem::NoKey);
        assert_eq!(problem(&AppError::Limit("daily limit of AI calls".into())), Problem::Cap);
    }

    #[test]
    fn only_free_chat_models_are_offered() {
        assert!(chat_groq("llama-3.3-70b-versatile"));
        assert!(!chat_groq("whisper-large-v3"));
        assert!(!chat_groq("openai/gpt-oss-safeguard-20b"));
        assert!(free_openrouter("google/gemma-4-31b-it:free"));
        assert!(!free_openrouter("google/gemma-4-31b-it"));
        assert!(is_model_name("openai/gpt-oss-20b:free"));
        assert!(!is_model_name("<b>x</b>"));
        for p in [Provider::Groq, Provider::OpenRouter] {
            assert!(p.free_models().contains(&p.default_model()), "{p:?}");
        }
    }

    #[test]
    fn providers_need_keys_except_ollama() {
        assert!(Ai::new(Provider::Gemini, "m".into(), None, String::new()).is_err());
        assert!(Ai::new(Provider::Ollama, "m".into(), None, "http://localhost:11434/".into()).is_ok());
        assert_eq!(Provider::parse("groq"), Some(Provider::Groq));
        assert_eq!(Provider::parse("x"), None);
    }
}
