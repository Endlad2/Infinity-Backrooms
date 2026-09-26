//! HTTP-клиент к локальному OpenAI-совместимому прокси http://localhost:9655.
//!
//! Модели проекта:
//!   * deepseek-chat-search — генерация уровней (основная, с поиском)
//!   * deepseek-chat        — генерация ассетов (SVG/OBJ)
//!   * deepseek-reasoner    — сложные случаи / fallback

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_ENDPOINT: &str = "http://localhost:9655/v1/chat/completions";
pub const MODEL_LEVEL_SEARCH: &str = "deepseek-chat-search";
pub const MODEL_ASSETS: &str = "deepseek-chat";
pub const MODEL_REASONER: &str = "deepseek-reasoner";

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(s: impl Into<String>) -> Self {
        Self { role: "system".into(), content: s.into() }
    }
    pub fn user(s: impl Into<String>) -> Self {
        Self { role: "user".into(), content: s.into() }
    }
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub stream: bool,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            messages: Vec::new(),
            temperature: 0.4,
            stream: false,
        }
    }
    pub fn system(mut self, s: impl Into<String>) -> Self {
        self.messages.push(ChatMessage::system(s));
        self
    }
    pub fn user(mut self, s: impl Into<String>) -> Self {
        self.messages.push(ChatMessage::user(s));
        self
    }
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub reasoning_content: Option<String>,
}

pub struct AiClient {
    pub endpoint: String,
    pub timeout: Duration,
    pub max_retries: u32,
}

impl Default for AiClient {
    fn default() -> Self {
        Self::new(DEFAULT_ENDPOINT)
    }
}

impl AiClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            timeout: Duration::from_secs(120),
            max_retries: 2,
        }
    }

    /// Сделать chat-запрос с retry и парсингом content + reasoning_content.
    pub fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let body = serde_json::json!({
            "model": req.model,
            "messages": req.messages.iter().map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content,
            })).collect::<Vec<_>>(),
            "temperature": req.temperature,
            "stream": req.stream,
        });

        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..=self.max_retries {
            match self.try_send(&body) {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    last_err = Some(e);
                    std::thread::sleep(Duration::from_millis(300 * (attempt as u64 + 1)));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("AI chat: неизвестная ошибка")))
    }

    fn try_send(&self, body: &serde_json::Value) -> Result<ChatResponse> {
        let resp = ureq::post(&self.endpoint)
            .timeout(self.timeout)
            .send_json(body.clone())
            .map_err(|e| anyhow!("AI backend error: {e}"))?;
        let v: ChatWireResponse = resp
            .into_json()
            .map_err(|e| anyhow!("AI json parse error: {e}"))?;
        let choice = v.choices.into_iter().next()
            .ok_or_else(|| anyhow!("AI ответ без choices"))?;
        Ok(ChatResponse {
            content: choice.message.content.unwrap_or_default(),
            reasoning_content: choice.message.reasoning_content,
        })
    }
}

// --- структуры ответа ---

#[derive(Debug, Deserialize)]
struct ChatWireResponse {
    #[serde(default)]
    choices: Vec<ChatWireChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatWireChoice {
    message: ChatWireMessage,
}

#[derive(Debug, Deserialize)]
struct ChatWireMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_has_system_and_user() {
        let r = ChatRequest::new(MODEL_ASSETS)
            .system("sys")
            .user("hello");
        assert_eq!(r.messages.len(), 2);
        assert_eq!(r.messages[0].role, "system");
        assert_eq!(r.messages[1].role, "user");
        assert_eq!(r.model, MODEL_ASSETS);
    }

    #[test]
    fn deserialize_wire_minimal() {
        let js = r#"{"choices":[{"message":{"content":"hi","reasoning_content":"think"}}]}"#;
        let v: ChatWireResponse = serde_json::from_str(js).unwrap();
        assert_eq!(v.choices[0].message.content.as_deref(), Some("hi"));
        assert_eq!(v.choices[0].message.reasoning_content.as_deref(), Some("think"));
    }

    #[test]
    fn deserialize_wire_missing_reasoning() {
        let js = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
        let v: ChatWireResponse = serde_json::from_str(js).unwrap();
        assert!(v.choices[0].message.reasoning_content.is_none());
    }
}