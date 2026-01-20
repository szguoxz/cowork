//! Chat session management
//!
//! Provides shared session state for both CLI and UI.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Status of a tool call
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    /// Tool call is pending execution
    Pending,
    /// Tool call was approved
    Approved,
    /// Tool call was rejected by user
    Rejected,
    /// Tool call is currently executing
    Executing,
    /// Tool call completed successfully
    Completed,
    /// Tool call failed
    Failed,
}

/// Information about a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    /// Unique ID for this tool call
    pub id: String,
    /// Name of the tool
    pub name: String,
    /// Arguments passed to the tool
    pub arguments: serde_json::Value,
    /// Current status
    pub status: ToolCallStatus,
    /// Result of the tool call (if completed)
    pub result: Option<String>,
}

impl ToolCallInfo {
    /// Create a new pending tool call
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
            status: ToolCallStatus::Pending,
            result: None,
        }
    }

    /// Mark as completed with result
    pub fn complete(&mut self, result: String) {
        self.status = ToolCallStatus::Completed;
        self.result = Some(result);
    }

    /// Mark as failed with error
    pub fn fail(&mut self, error: String) {
        self.status = ToolCallStatus::Failed;
        self.result = Some(error);
    }

    /// Mark as rejected
    pub fn reject(&mut self) {
        self.status = ToolCallStatus::Rejected;
        self.result = Some("Rejected by user".to_string());
    }
}

/// A message in the chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unique ID for this message
    pub id: String,
    /// Role: "user", "assistant", "system", or "tool"
    pub role: String,
    /// Message content
    pub content: String,
    /// Tool calls made by this message (if assistant)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCallInfo>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl ChatMessage {
    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Create a new assistant message with tool calls
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCallInfo>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: content.into(),
            tool_calls,
            timestamp: Utc::now(),
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: &str, result: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(), // Tool results are sent as user messages
            content: format!(
                "[Tool result for {}]\n{}\n[End of tool result. Please summarize the above for the user.]",
                tool_call_id,
                result
            ),
            tool_calls: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "system".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Check if this message has pending tool calls
    pub fn has_pending_tools(&self) -> bool {
        self.tool_calls.iter().any(|tc| tc.status == ToolCallStatus::Pending)
    }

    /// Get pending tool calls
    pub fn pending_tools(&self) -> Vec<&ToolCallInfo> {
        self.tool_calls
            .iter()
            .filter(|tc| tc.status == ToolCallStatus::Pending)
            .collect()
    }
}

/// A chat session containing conversation history and state
#[derive(Debug, Clone)]
pub struct ChatSession {
    /// Unique session ID
    pub id: String,
    /// Conversation messages
    pub messages: Vec<ChatMessage>,
    /// System prompt for this session
    pub system_prompt: String,
}

impl ChatSession {
    /// Create a new chat session
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            messages: Vec::new(),
            system_prompt: super::system_prompt::DEFAULT_SYSTEM_PROMPT.to_string(),
        }
    }

    /// Create with a custom system prompt
    pub fn with_system_prompt(system_prompt: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            messages: Vec::new(),
            system_prompt: system_prompt.into(),
        }
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: impl Into<String>) -> &ChatMessage {
        self.messages.push(ChatMessage::user(content));
        self.messages.last().unwrap()
    }

    /// Add an assistant message
    pub fn add_assistant_message(&mut self, content: impl Into<String>, tool_calls: Vec<ToolCallInfo>) -> &ChatMessage {
        if tool_calls.is_empty() {
            self.messages.push(ChatMessage::assistant(content));
        } else {
            self.messages.push(ChatMessage::assistant_with_tools(content, tool_calls));
        }
        self.messages.last().unwrap()
    }

    /// Add a tool result
    pub fn add_tool_result(&mut self, tool_call_id: &str, result: impl Into<String>) {
        let result_str = result.into();

        // First, update the tool call status in the message that contains it
        for msg in &mut self.messages {
            for tc in &mut msg.tool_calls {
                if tc.id == tool_call_id {
                    tc.complete(result_str.clone());
                    break;
                }
            }
        }

        // Add the tool result as a message for the LLM
        self.messages.push(ChatMessage::tool_result(tool_call_id, &result_str));
    }

    /// Mark a tool call as rejected
    pub fn reject_tool(&mut self, tool_call_id: &str) {
        for msg in &mut self.messages {
            for tc in &mut msg.tool_calls {
                if tc.id == tool_call_id {
                    tc.reject();
                    return;
                }
            }
        }
    }

    /// Check if there are pending tool calls
    pub fn has_pending_tools(&self) -> bool {
        self.messages.iter().any(|m| m.has_pending_tools())
    }

    /// Get all pending tool calls
    pub fn pending_tools(&self) -> Vec<&ToolCallInfo> {
        self.messages
            .iter()
            .flat_map(|m| m.pending_tools())
            .collect()
    }

    /// Convert messages to LLM format
    pub fn to_llm_messages(&self) -> Vec<crate::provider::LlmMessage> {
        self.messages
            .iter()
            .map(|m| crate::provider::LlmMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect()
    }

    /// Clear conversation history
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

impl Default for ChatSession {
    fn default() -> Self {
        Self::new()
    }
}
