//! Chat session management
//!
//! Provides shared session state for both CLI and UI.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::provider::{
    ChatMessage, ToolCall, tool_result_message, assistant_with_tool_calls,
};

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

/// A chat session containing conversation history and state
#[derive(Debug, Clone)]
pub struct ChatSession {
    /// Unique session ID
    pub id: String,
    /// Conversation messages (using genai's ChatMessage directly)
    pub messages: Vec<ChatMessage>,
    /// System prompt for this session
    pub system_prompt: String,
    /// Tool call status tracking (keyed by call_id)
    pub tool_status: HashMap<String, ToolCallStatus>,
}

impl ChatSession {
    /// Create a new chat session
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            messages: Vec::new(),
            system_prompt: super::system_prompt::DEFAULT_SYSTEM_PROMPT.to_string(),
            tool_status: HashMap::new(),
        }
    }

    /// Create with a custom system prompt
    pub fn with_system_prompt(system_prompt: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            messages: Vec::new(),
            system_prompt: system_prompt.into(),
            tool_status: HashMap::new(),
        }
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.messages.push(ChatMessage::user(content.into()));
    }

    /// Add an assistant message
    pub fn add_assistant_message(&mut self, content: impl Into<String>, tool_calls: Vec<ToolCall>) {
        if tool_calls.is_empty() {
            self.messages.push(ChatMessage::assistant(content.into()));
        } else {
            // Track tool call status
            for tc in &tool_calls {
                self.tool_status.insert(tc.call_id.clone(), ToolCallStatus::Pending);
            }
            let content_str = content.into();
            let content_opt = if content_str.is_empty() { None } else { Some(content_str) };
            self.messages.push(assistant_with_tool_calls(content_opt, tool_calls));
        }
    }

    /// Add a tool result
    pub fn add_tool_result(&mut self, call_id: &str, result: impl Into<String>, is_error: bool) {
        // Update status
        let status = if is_error { ToolCallStatus::Failed } else { ToolCallStatus::Completed };
        self.tool_status.insert(call_id.to_string(), status);

        // Add tool result message
        self.messages.push(tool_result_message(call_id, result.into()));
    }

    /// Add multiple tool results
    pub fn add_tool_results(&mut self, results: Vec<(String, String, bool)>) {
        for (call_id, result, is_error) in results {
            self.add_tool_result(&call_id, result, is_error);
        }
    }

    /// Mark a tool call as rejected
    pub fn reject_tool(&mut self, call_id: &str) {
        self.tool_status.insert(call_id.to_string(), ToolCallStatus::Rejected);
    }

    /// Mark a tool call as approved
    pub fn approve_tool(&mut self, call_id: &str) {
        self.tool_status.insert(call_id.to_string(), ToolCallStatus::Approved);
    }

    /// Check if there are pending tool calls
    pub fn has_pending_tools(&self) -> bool {
        self.tool_status.values().any(|s| *s == ToolCallStatus::Pending)
    }

    /// Get pending tool call IDs
    pub fn pending_tool_ids(&self) -> Vec<&String> {
        self.tool_status
            .iter()
            .filter(|(_, s)| **s == ToolCallStatus::Pending)
            .map(|(id, _)| id)
            .collect()
    }

    /// Get tool status
    pub fn get_tool_status(&self, call_id: &str) -> Option<ToolCallStatus> {
        self.tool_status.get(call_id).copied()
    }

    /// Get messages (already in LLM format)
    pub fn get_messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Clear conversation history
    pub fn clear(&mut self) {
        self.messages.clear();
        self.tool_status.clear();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = ChatSession::new();
        assert!(!session.id.is_empty());
        assert!(session.messages.is_empty());
    }

    #[test]
    fn test_add_messages() {
        let mut session = ChatSession::new();
        session.add_user_message("Hello");
        session.add_assistant_message("Hi there", vec![]);
        assert_eq!(session.message_count(), 2);
    }

    #[test]
    fn test_tool_status_tracking() {
        let mut session = ChatSession::new();

        let tool_call = ToolCall {
            call_id: "call_123".to_string(),
            fn_name: "read_file".to_string(),
            fn_arguments: serde_json::json!({"path": "/test.txt"}),
            thought_signatures: None,
        };

        session.add_assistant_message("", vec![tool_call]);
        assert!(session.has_pending_tools());
        assert_eq!(session.get_tool_status("call_123"), Some(ToolCallStatus::Pending));

        session.add_tool_result("call_123", "file contents", false);
        assert!(!session.has_pending_tools());
        assert_eq!(session.get_tool_status("call_123"), Some(ToolCallStatus::Completed));
    }

    #[test]
    fn test_tool_rejection() {
        let mut session = ChatSession::new();
        session.tool_status.insert("call_123".to_string(), ToolCallStatus::Pending);

        session.reject_tool("call_123");
        assert_eq!(session.get_tool_status("call_123"), Some(ToolCallStatus::Rejected));
    }
}
