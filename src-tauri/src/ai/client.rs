//! Calls to AI providers. Two kinds of API cover all of them:
//! - OpenAI-compatible chat (Gemini, Groq, OpenRouter, Ollama, OpenAI),
//! - Anthropic Messages API (Claude).
//! Keys come from the credential store and go only into the request header.

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{AppError, AppResult};
use crate::net;

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

    /// Model used when the settings leave it empty (orientacyjnie, stan na 11.2026).
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Gemini => "gemini-2.5-flash",
            Self::Groq => "openai/gpt-oss-120b",
            Self::OpenRouter => "openai/gpt-oss-20b:free",
            Self::Ollama => "llama3.1:8b",
            Self::Anthropic => "claude-opus-5-5",
            Self::OpenAi => "gpt-5-mini",
        }
    }

    /// Pause between calls, so free per-minute limits are not hit.
    pub fn pause(self) -> Duration {
        Duration::from_millis(match self {
            Self::Gemini => 7_000,
            Self::Groq => 2_500,
            Self::OpenRouter => 3_500,
            Self::Ollama => 0,
            Self::Anthropic | Self::OpenAi => 500,
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
}

impl Ai {
    pub fn new(provider: Provider, model: String, key: Option<String>, ollama_url: String) -> AppResult<Self> {
        if provider.key_name().is_some() && key.is_none() {
            return Err(AppError::Missing("ai key".into()));
        }
        Ok(Ai { provider, model, key, base_url: ollama_url.trim_end_matches('/').to_string(), http: net::client()? })
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
        match self.provider {
            Provider::Anthropic => self.ask_anthropic(system, user, json).await,
            _ => self.ask_openai(system, user, json).await,
        }
    }

    async fn ask_openai(&self, system: &str, user: &str, json: bool) -> AppResult<String> {
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
        let mut req = self.http.post(self.chat_url()).json(&body);
        if let Some(key) = &self.key {
            req = req.bearer_auth(key);
        }
        if self.provider == Provider::OpenRouter {
            req = req.header("X-Title", "Tablica Ofert");
        }
        let answer: OpenAiAnswer = net::json(req.send().await.map_err(net::net_error)?).await?;
        let choice = answer.choices.into_iter().next().ok_or_else(|| AppError::Net("empty answer".into()))?;
        if choice.finish_reason.as_deref() == Some("content_filter") {
            return Err(AppError::Net("the provider refused to answer".into()));
        }
        choice.message.content.filter(|c| !c.trim().is_empty()).ok_or_else(|| AppError::Net("empty answer".into()))
    }

    /// Claude: JSON is asked for in the prompt (the answer is checked field by field anyway).
    async fn ask_anthropic(&self, system: &str, user: &str, _json: bool) -> AppResult<String> {
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": system,
            "messages": [{ "role": "user", "content": user }],
            // Reading one offer is a simple task - low effort keeps it fast and cheap.
            "output_config": { "effort": "low" },
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
        let answer: AnthropicAnswer = net::json(req.send().await.map_err(net::net_error)?).await?;
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
            return Err(AppError::Net("empty answer".into()));
        }
        Ok(text)
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiAnswer {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
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
    fn providers_need_keys_except_ollama() {
        assert!(Ai::new(Provider::Gemini, "m".into(), None, String::new()).is_err());
        assert!(Ai::new(Provider::Ollama, "m".into(), None, "http://localhost:11434/".into()).is_ok());
        assert_eq!(Provider::parse("groq"), Some(Provider::Groq));
        assert_eq!(Provider::parse("x"), None);
    }
}
