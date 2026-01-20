//! Streaming support for LLM responses
//!
//! Provides token-by-token streaming of LLM responses to the frontend.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use cowork_core::ToolCallInfo;

/// Events emitted during streaming
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Stream started
    Start {
        session_id: String,
        message_id: String,
    },
    /// Thinking/reasoning delta (for Claude, DeepSeek, etc.)
    ThinkingDelta {
        session_id: String,
        message_id: String,
        delta: String,
        accumulated: String,
    },
    /// Text delta received
    TextDelta {
        session_id: String,
        message_id: String,
        delta: String,
        accumulated: String,
    },
    /// Tool call started (arguments may still be streaming)
    ToolCallStart {
        session_id: String,
        message_id: String,
        tool_call_id: String,
        tool_name: String,
    },
    /// Tool call argument delta
    ToolCallDelta {
        session_id: String,
        message_id: String,
        tool_call_id: String,
        delta: String,
    },
    /// Tool call completed
    ToolCallComplete {
        session_id: String,
        message_id: String,
        tool_call: ToolCallInfo,
    },
    /// Stream ended normally
    End {
        session_id: String,
        message_id: String,
        finish_reason: String,
    },
    /// Stream error
    Error {
        session_id: String,
        message_id: String,
        error: String,
    },
}

/// Manages streaming state for a message
pub struct StreamingMessage {
    session_id: String,
    message_id: String,
    app_handle: AppHandle,
    accumulated_thinking: String,
    accumulated_text: String,
    tool_calls: Vec<StreamingToolCall>,
}

/// Tracks a tool call being streamed
struct StreamingToolCall {
    id: String,
    #[allow(dead_code)]
    name: String,
    arguments_json: String,
}

impl StreamingMessage {
    pub fn new(session_id: String, message_id: String, app_handle: AppHandle) -> Self {
        Self {
            session_id,
            message_id,
            app_handle,
            accumulated_thinking: String::new(),
            accumulated_text: String::new(),
            tool_calls: Vec::new(),
        }
    }

    /// Emit start event
    pub fn start(&self) {
        self.emit(StreamEvent::Start {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
        });
    }

    /// Add thinking/reasoning delta
    pub fn add_thinking(&mut self, delta: &str) {
        self.accumulated_thinking.push_str(delta);
        self.emit(StreamEvent::ThinkingDelta {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            delta: delta.to_string(),
            accumulated: self.accumulated_thinking.clone(),
        });
    }

    /// Add text delta
    pub fn add_text(&mut self, delta: &str) {
        self.accumulated_text.push_str(delta);
        self.emit(StreamEvent::TextDelta {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            delta: delta.to_string(),
            accumulated: self.accumulated_text.clone(),
        });
    }

    /// Start a tool call
    pub fn start_tool_call(&mut self, id: String, name: String) {
        self.tool_calls.push(StreamingToolCall {
            id: id.clone(),
            name: name.clone(),
            arguments_json: String::new(),
        });
        self.emit(StreamEvent::ToolCallStart {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_call_id: id,
            tool_name: name,
        });
    }

    /// Add tool call argument delta
    pub fn add_tool_arg(&mut self, tool_call_id: &str, delta: &str) {
        if let Some(tc) = self.tool_calls.iter_mut().find(|tc| tc.id == tool_call_id) {
            tc.arguments_json.push_str(delta);
        }
        self.emit(StreamEvent::ToolCallDelta {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_call_id: tool_call_id.to_string(),
            delta: delta.to_string(),
        });
    }

    /// Complete a tool call
    pub fn complete_tool_call(&self, tool_call: ToolCallInfo) {
        self.emit(StreamEvent::ToolCallComplete {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_call,
        });
    }

    /// End the stream
    pub fn end(&self, finish_reason: &str) {
        self.emit(StreamEvent::End {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            finish_reason: finish_reason.to_string(),
        });
    }

    /// Report an error
    pub fn error(&self, error: &str) {
        self.emit(StreamEvent::Error {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            error: error.to_string(),
        });
    }

    /// Get accumulated text
    pub fn text(&self) -> &str {
        &self.accumulated_text
    }

    /// Get accumulated thinking
    pub fn thinking(&self) -> &str {
        &self.accumulated_thinking
    }

    fn emit(&self, event: StreamEvent) {
        let event_name = format!("stream:{}", self.session_id);
        if let Err(e) = self.app_handle.emit(&event_name, &event) {
            tracing::error!("Failed to emit stream event: {}", e);
        }
    }
}

/// Stream handler for processing chunks from genai
pub struct StreamHandler {
    tx: mpsc::Sender<StreamChunk>,
}

#[derive(Debug, Clone)]
pub enum StreamChunk {
    Start,
    Thinking(String),
    Text(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, delta: String },
    ToolCallComplete { id: String },
    End { finish_reason: String },
    Error(String),
}

impl StreamHandler {
    pub fn new(tx: mpsc::Sender<StreamChunk>) -> Self {
        Self { tx }
    }

    pub async fn send_start(&self) {
        let _ = self.tx.send(StreamChunk::Start).await;
    }

    pub async fn send_thinking(&self, text: &str) {
        let _ = self.tx.send(StreamChunk::Thinking(text.to_string())).await;
    }

    pub async fn send_text(&self, text: &str) {
        let _ = self.tx.send(StreamChunk::Text(text.to_string())).await;
    }

    pub async fn send_tool_start(&self, id: &str, name: &str) {
        let _ = self
            .tx
            .send(StreamChunk::ToolCallStart {
                id: id.to_string(),
                name: name.to_string(),
            })
            .await;
    }

    pub async fn send_tool_delta(&self, id: &str, delta: &str) {
        let _ = self
            .tx
            .send(StreamChunk::ToolCallDelta {
                id: id.to_string(),
                delta: delta.to_string(),
            })
            .await;
    }

    pub async fn send_tool_complete(&self, id: &str) {
        let _ = self
            .tx
            .send(StreamChunk::ToolCallComplete { id: id.to_string() })
            .await;
    }

    pub async fn send_end(&self, finish_reason: &str) {
        let _ = self
            .tx
            .send(StreamChunk::End {
                finish_reason: finish_reason.to_string(),
            })
            .await;
    }

    pub async fn send_error(&self, error: &str) {
        let _ = self.tx.send(StreamChunk::Error(error.to_string())).await;
    }
}
