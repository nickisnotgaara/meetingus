//! Xiaomi MiMo chat client (OpenAI-compatible streaming API).
//! SSE payloads are parsed by the pure `parse_data_line` so streaming logic is
//! unit-testable without a network. Live call is env-gated (MIMO_API_KEY).
//! Reasoning (`reasoning_content`) is surfaced separately and filtered on render.

use anyhow::{bail, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

pub const DEFAULT_BASE_URL: &str = "https://token-plan-sgp.xiaomimimo.com/v1";
pub const MODEL_FLASH: &str = "mimo-v2.6-flash";
pub const MODEL_PRO: &str = "mimo-v2.6-pro";

#[derive(Debug, PartialEq, Eq)]
pub enum StreamEvent {
    Text(String),
    Thinking(String),
    Done,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
    #[serde(default)]
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

/// Parse one SSE `data:` payload. Returns None for keep-alives/non-JSON lines;
/// `[DONE]` terminates the stream.
pub fn parse_data_line(payload: &str) -> Option<StreamEvent> {
    let payload = payload.trim();
    if payload.is_empty() {
        return None;
    }
    if payload == "[DONE]" {
        return Some(StreamEvent::Done);
    }
    let chunk: StreamChunk = serde_json::from_str(payload).ok()?;
    let choice = chunk.choices.into_iter().next()?;
    if let Some(reasoning) = choice.delta.reasoning_content.filter(|s| !s.is_empty()) {
        return Some(StreamEvent::Thinking(reasoning));
    }
    if let Some(content) = choice.delta.content.filter(|s| !s.is_empty()) {
        return Some(StreamEvent::Text(content));
    }
    None
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    stream: bool,
    messages: Vec<ChatMessage<'a>>,
    /// Fast mode turns MiMo reasoning off (probe: TTFT 3.5s -> 1.0s).
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

pub struct MiMoClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl MiMoClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }

    /// Read config from env: MIMO_BASE_URL (default DEFAULT_BASE_URL), MIMO_API_KEY.
    pub fn from_env() -> Result<Self> {
        let base = std::env::var("MIMO_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let key = match std::env::var("MIMO_API_KEY") {
            Ok(k) if !k.trim().is_empty() => k,
            _ => bail!("MIMO_API_KEY is not set"),
        };
        Ok(Self::new(base, key))
    }

    /// Stream a chat completion; `on_event` receives Text/Thinking deltas in order.
    pub async fn stream_chat(
        &self,
        model: &str,
        system: &str,
        user: &str,
        on_event: impl FnMut(StreamEvent),
    ) -> Result<()> {
        self.stream_chat_opts(model, system, user, false, on_event).await
    }

    /// `disable_thinking` maps to MiMo's chat_template_kwargs switch. Fast mode
    /// (flash) uses it to cut TTFT; Smart mode leaves reasoning on.
    pub async fn stream_chat_opts(
        &self,
        model: &str,
        system: &str,
        user: &str,
        disable_thinking: bool,
        mut on_event: impl FnMut(StreamEvent),
    ) -> Result<()> {
        let body = ChatRequest {
            model,
            stream: true,
            messages: vec![
                ChatMessage { role: "system", content: system },
                ChatMessage { role: "user", content: user },
            ],
            chat_template_kwargs: disable_thinking
                .then(|| serde_json::json!({ "enable_thinking": false })),
        };
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("MiMo HTTP {}: {}", resp.status(), resp.text().await.unwrap_or_default());
        }
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim_end_matches('\r').to_string();
                buf.drain(..=pos);
                if let Some(rest) = line.strip_prefix("data:") {
                    if let Some(event) = parse_data_line(rest.trim()) {
                        let done = event == StreamEvent::Done;
                        on_event(event);
                        if done {
                            return Ok(());
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_delta() {
        let line = r#"{"choices":[{"delta":{"content":"Hello"}}]}"#;
        assert_eq!(parse_data_line(line), Some(StreamEvent::Text("Hello".into())));
    }

    #[test]
    fn parses_thinking_delta_separately() {
        let line = r#"{"choices":[{"delta":{"reasoning_content":"let me think"}}]}"#;
        assert_eq!(parse_data_line(line), Some(StreamEvent::Thinking("let me think".into())));
    }

    #[test]
    fn done_sentinel_and_empty_lines() {
        assert_eq!(parse_data_line("[DONE]"), Some(StreamEvent::Done));
        assert_eq!(parse_data_line(""), None);
        assert_eq!(parse_data_line("{}"), None);
        assert_eq!(parse_data_line("not json"), None);
    }

    #[test]
    fn finish_reason_alone_is_not_done_until_sentinel() {
        let line = r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        assert_eq!(parse_data_line(line), None, "[DONE] is the only terminator");
    }

    #[test]
    fn fast_mode_serializes_thinking_off_smart_mode_omits_it() {
        let fast = ChatRequest {
            model: MODEL_FLASH,
            stream: true,
            messages: vec![],
            chat_template_kwargs: Some(serde_json::json!({ "enable_thinking": false })),
        };
        let value = serde_json::to_value(&fast).expect("serialize");
        assert_eq!(value["chat_template_kwargs"]["enable_thinking"], false);

        let smart = ChatRequest {
            model: MODEL_PRO,
            stream: true,
            messages: vec![],
            chat_template_kwargs: None,
        };
        let value = serde_json::to_value(&smart).expect("serialize");
        assert!(
            value.get("chat_template_kwargs").is_none(),
            "smart mode keeps the provider default (reasoning on)"
        );
    }

    /// Live smoke: needs MIMO_API_KEY in env. `cargo test -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn live_stream_from_mimo() {
        let Ok(client) = MiMoClient::from_env() else {
            eprintln!("SKIP: MIMO_API_KEY not set");
            return;
        };
        let mut text = String::new();
        let result = client
            .stream_chat(MODEL_FLASH, "You are terse.", "Say hi in three words.", |ev| {
                if let StreamEvent::Text(t) = ev {
                    text.push_str(&t);
                }
            })
            .await;
        assert!(result.is_ok(), "stream failed: {result:?}");
        assert!(!text.is_empty(), "expected streamed text");
    }
}
