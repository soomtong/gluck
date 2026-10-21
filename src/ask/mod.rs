//! Ask: explain the visible diff/file page or a mouse-selected block with
//! an OpenAI-compatible LLM. The request runs on its own thread and streams
//! deltas back over a per-request channel; dropping the receiver (panel
//! closed or replaced) stops the worker at its next chunk.

pub mod client;
pub mod context;
pub mod panel;
pub mod prompt;
pub mod provider;

use provider::Endpoint;
use serde::Serialize;
use std::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatMessage {
    pub role: &'static str,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: String) -> Self {
        Self {
            role: "system",
            content,
        }
    }

    pub fn user(content: String) -> Self {
        Self {
            role: "user",
            content,
        }
    }
}

/// A full conversation, so follow-up turns can be appended later.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    /// None = omit (some reasoning models reject the field).
    pub temperature: Option<f32>,
}

impl ChatRequest {
    pub fn to_json(&self, model: &str) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": model,
            "messages": self.messages,
            "stream": true,
            "max_tokens": self.max_tokens,
        });
        if let Some(t) = self.temperature {
            body["temperature"] = serde_json::json!(t);
        }
        body
    }
}

pub enum AskMessage {
    Delta(String),
    Done { truncated: bool },
    Failed(String),
}

pub fn spawn_ask(endpoint: Endpoint, request: ChatRequest) -> mpsc::Receiver<AskMessage> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = client::stream_chat(&endpoint, &request, |delta| {
            tx.send(AskMessage::Delta(delta.to_string())).is_ok()
        });
        let msg = match result {
            Ok(end) => AskMessage::Done {
                truncated: end.truncated,
            },
            Err(e) => {
                tracing::warn!(target: "ask", "{e}");
                AskMessage::Failed(e)
            }
        };
        let _ = tx.send(msg);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_json_omits_temperature_when_none() {
        let req = ChatRequest {
            messages: vec![ChatMessage::user("hi".into())],
            max_tokens: 10,
            temperature: None,
        };
        let v = req.to_json("m");
        assert_eq!(v["model"], "m");
        assert_eq!(v["stream"], true);
        assert_eq!(v["messages"][0]["role"], "user");
        assert!(v.get("temperature").is_none());
        let with = ChatRequest {
            temperature: Some(0.5),
            ..req
        };
        assert_eq!(with.to_json("m")["temperature"], 0.5);
    }
}
