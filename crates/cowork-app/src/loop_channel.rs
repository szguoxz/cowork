//! Simplified loop channel types
//!
//! Two channels:
//! - LoopInput: frontend → backend (user messages, commands)
//! - LoopOutput: backend → frontend (all outputs)

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Messages sent from frontend to the loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoopInput {
    /// User typed a message
    UserMessage(String),
    /// User approved tool execution
    ApproveTool(String),
    /// User rejected tool execution
    RejectTool(String),
    /// User answered a question
    AnswerQuestion { request_id: String, answers: std::collections::HashMap<String, String> },
    /// Stop the loop
    Stop,
}

/// Messages sent from loop to frontend
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../frontend/src/bindings/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoopOutput {
    /// Loop started and ready for input
    Ready,
    /// User message (echo for display)
    UserMessage {
        id: String,
        content: String,
    },
    /// Assistant is thinking (streaming text)
    Thinking {
        content: String,
    },
    /// Assistant message
    AssistantMessage {
        id: String,
        content: String,
    },
    /// Tool execution starting
    ToolStart {
        id: String,
        name: String,
        #[ts(type = "Record<string, unknown>")]
        arguments: serde_json::Value,
    },
    /// Tool needs approval before execution
    ToolPending {
        id: String,
        name: String,
        #[ts(type = "Record<string, unknown>")]
        arguments: serde_json::Value,
    },
    /// Tool execution completed
    ToolDone {
        id: String,
        name: String,
        success: bool,
        output: String,
    },
    /// Loop is idle, waiting for user input
    Idle,
    /// Error occurred
    Error {
        message: String,
    },
    /// Loop stopped
    Stopped,
}
