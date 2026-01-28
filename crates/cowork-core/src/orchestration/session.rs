//! Chat session management
//!
//! Provides shared session state for both CLI and UI.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::provider::ContentBlock;

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
    /// Message content (plain text, for backwards compatibility)
    pub content: String,
    /// Content blocks for structured content (tool_use, tool_result, etc.)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub content_blocks: Vec<ContentBlock>,
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
            content_blocks: Vec::new(),
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
            content_blocks: Vec::new(),
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
            content_blocks: Vec::new(),
            tool_calls,
            timestamp: Utc::now(),
        }
    }

    /// Create a tool result message with proper content blocks
    pub fn tool_result(tool_call_id: &str, result: &str, is_error: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(), // Tool results are sent as user messages
            content: String::new(),   // Use content_blocks instead
            content_blocks: vec![ContentBlock::tool_result(tool_call_id, result, is_error)],
            tool_calls: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Create a user message with multiple tool results (batched)
    pub fn tool_results(results: Vec<ContentBlock>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: String::new(),
            content_blocks: results,
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
            content_blocks: Vec::new(),
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

    /// Add a tool result (single)
    pub fn add_tool_result(&mut self, tool_call_id: &str, result: impl Into<String>) {
        self.add_tool_result_with_error(tool_call_id, result, false);
    }

    /// Add a tool result with error flag
    pub fn add_tool_result_with_error(&mut self, tool_call_id: &str, result: impl Into<String>, is_error: bool) {
        let result_str = result.into();

        // First, update the tool call status in the message that contains it
        for msg in &mut self.messages {
            for tc in &mut msg.tool_calls {
                if tc.id == tool_call_id {
                    if is_error {
                        tc.fail(result_str.clone());
                    } else {
                        tc.complete(result_str.clone());
                    }
                    break;
                }
            }
        }

        // Add the tool result as a message for the LLM
        self.messages.push(ChatMessage::tool_result(tool_call_id, &result_str, is_error));
    }

    /// Add multiple tool results as a single batched message
    /// This is the preferred method when the LLM requests multiple tools
    pub fn add_tool_results(&mut self, results: Vec<(String, String, bool)>) {
        // Build content blocks for all results
        let blocks: Vec<ContentBlock> = results.iter().map(|(id, content, is_error)| {
            // Update tool call status
            for msg in &mut self.messages {
                for tc in &mut msg.tool_calls {
                    if tc.id == *id {
                        if *is_error {
                            tc.fail(content.clone());
                        } else {
                            tc.complete(content.clone());
                        }
                        break;
                    }
                }
            }
            ContentBlock::tool_result(id, content, *is_error)
        }).collect();

        // Add single user message with all tool results
        self.messages.push(ChatMessage::tool_results(blocks));
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
        use crate::provider::{LlmMessage, MessageContent, Role};

        self.messages
            .iter()
            .map(|m| {
                let role = Role::parse(&m.role);

                // If message has content_blocks, use them
                if !m.content_blocks.is_empty() {
                    return LlmMessage {
                        role,
                        content: MessageContent::Blocks(m.content_blocks.clone()),
                        tool_calls: None,
                        tool_call_id: None,
                    };
                }

                // If message has tool calls (assistant message), convert them
                if !m.tool_calls.is_empty() {
                    // Build content blocks: text (if any) + tool_use blocks
                    let mut blocks = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(crate::provider::ContentBlock::text(&m.content));
                    }
                    for tc in &m.tool_calls {
                        blocks.push(crate::provider::ContentBlock::tool_use(
                            &tc.id,
                            &tc.name,
                            tc.arguments.clone(),
                        ));
                    }

                    return LlmMessage {
                        role,
                        content: MessageContent::Blocks(blocks),
                        tool_calls: Some(
                            m.tool_calls
                                .iter()
                                .map(|tc| crate::provider::ToolCall {
                                    id: tc.id.clone(),
                                    name: tc.name.clone(),
                                    arguments: tc.arguments.clone(),
                                })
                                .collect(),
                        ),
                        tool_call_id: None,
                    };
                }

                // Simple text message
                LlmMessage {
                    role,
                    content: MessageContent::Text(m.content.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call_info_creation() {
        let tc = ToolCallInfo::new("call_123", "read_file", serde_json::json!({"path": "/test.txt"}));
        assert_eq!(tc.id, "call_123");
        assert_eq!(tc.name, "read_file");
        assert_eq!(tc.status, ToolCallStatus::Pending);
        assert!(tc.result.is_none());
    }

    #[test]
    fn test_tool_call_info_complete() {
        let mut tc = ToolCallInfo::new("call_123", "read_file", serde_json::json!({}));
        tc.complete("File contents".to_string());
        assert_eq!(tc.status, ToolCallStatus::Completed);
        assert_eq!(tc.result, Some("File contents".to_string()));
    }

    #[test]
    fn test_tool_call_info_fail() {
        let mut tc = ToolCallInfo::new("call_123", "read_file", serde_json::json!({}));
        tc.fail("File not found".to_string());
        assert_eq!(tc.status, ToolCallStatus::Failed);
        assert_eq!(tc.result, Some("File not found".to_string()));
    }

    #[test]
    fn test_tool_call_info_reject() {
        let mut tc = ToolCallInfo::new("call_123", "delete_file", serde_json::json!({}));
        tc.reject();
        assert_eq!(tc.status, ToolCallStatus::Rejected);
        assert_eq!(tc.result, Some("Rejected by user".to_string()));
    }

    #[test]
    fn test_chat_message_user() {
        let msg = ChatMessage::user("Hello, world!");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello, world!");
        assert!(msg.content_blocks.is_empty());
        assert!(msg.tool_calls.is_empty());
    }

    #[test]
    fn test_chat_message_assistant() {
        let msg = ChatMessage::assistant("I can help with that.");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "I can help with that.");
        assert!(msg.content_blocks.is_empty());
        assert!(msg.tool_calls.is_empty());
    }

    #[test]
    fn test_chat_message_assistant_with_tools() {
        let tool_calls = vec![
            ToolCallInfo::new("call_1", "read_file", serde_json::json!({"path": "/test.txt"})),
            ToolCallInfo::new("call_2", "grep", serde_json::json!({"pattern": "foo"})),
        ];
        let msg = ChatMessage::assistant_with_tools("Let me do that.", tool_calls);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Let me do that.");
        assert_eq!(msg.tool_calls.len(), 2);
        assert_eq!(msg.tool_calls[0].name, "read_file");
        assert_eq!(msg.tool_calls[1].name, "grep");
    }

    #[test]
    fn test_chat_message_tool_result() {
        let msg = ChatMessage::tool_result("call_123", "Success result", false);
        assert_eq!(msg.role, "user");
        assert!(msg.content.is_empty()); // Uses content_blocks instead
        assert_eq!(msg.content_blocks.len(), 1);
        match &msg.content_blocks[0] {
            ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                assert_eq!(tool_use_id, "call_123");
                assert_eq!(content, "Success result");
                assert!(is_error.is_none());
            }
            _ => panic!("Expected ToolResult block"),
        }
    }

    #[test]
    fn test_chat_message_tool_result_with_error() {
        let msg = ChatMessage::tool_result("call_456", "Error: file not found", true);
        match &msg.content_blocks[0] {
            ContentBlock::ToolResult { is_error, .. } => {
                assert_eq!(is_error, &Some(true));
            }
            _ => panic!("Expected ToolResult block"),
        }
    }

    #[test]
    fn test_chat_message_tool_results_batched() {
        let results = vec![
            ContentBlock::tool_result("call_1", "Result 1", false),
            ContentBlock::tool_result("call_2", "Result 2", false),
            ContentBlock::tool_result("call_3", "Error 3", true),
        ];
        let msg = ChatMessage::tool_results(results);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content_blocks.len(), 3);
    }

    #[test]
    fn test_chat_session_new() {
        let session = ChatSession::new();
        assert_eq!(session.message_count(), 0);
        assert!(session.messages.is_empty());
    }

    #[test]
    fn test_chat_session_add_user_message() {
        let mut session = ChatSession::new();
        session.add_user_message("Hello!");
        assert_eq!(session.message_count(), 1);
        assert_eq!(session.messages[0].role, "user");
        assert_eq!(session.messages[0].content, "Hello!");
    }

    #[test]
    fn test_chat_session_add_assistant_message() {
        let mut session = ChatSession::new();
        let tool_calls = vec![
            ToolCallInfo::new("call_1", "read_file", serde_json::json!({})),
        ];
        session.add_assistant_message("Let me read that.", tool_calls);
        assert_eq!(session.message_count(), 1);
        assert_eq!(session.messages[0].role, "assistant");
        assert_eq!(session.messages[0].tool_calls.len(), 1);
    }

    #[test]
    fn test_chat_session_add_tool_result() {
        let mut session = ChatSession::new();
        session.add_user_message("Read the file");
        let tool_calls = vec![ToolCallInfo::new("call_1", "read_file", serde_json::json!({}))];
        session.add_assistant_message("Reading file...", tool_calls);
        session.add_tool_result("call_1", "File contents here");

        // Check that tool call status was updated
        let assistant_msg = &session.messages[1];
        assert_eq!(assistant_msg.tool_calls[0].status, ToolCallStatus::Completed);
        assert_eq!(assistant_msg.tool_calls[0].result, Some("File contents here".to_string()));

        // Check that tool result message was added
        let result_msg = &session.messages[2];
        assert_eq!(result_msg.role, "user");
        assert!(!result_msg.content_blocks.is_empty());
    }

    #[test]
    fn test_chat_session_add_tool_result_with_error() {
        let mut session = ChatSession::new();
        session.add_user_message("Delete file");
        let tool_calls = vec![ToolCallInfo::new("call_1", "delete_file", serde_json::json!({}))];
        session.add_assistant_message("Deleting...", tool_calls);
        session.add_tool_result_with_error("call_1", "Permission denied", true);

        // Check that tool call status was updated to failed
        let assistant_msg = &session.messages[1];
        assert_eq!(assistant_msg.tool_calls[0].status, ToolCallStatus::Failed);

        // Check that error flag is set
        let result_msg = &session.messages[2];
        match &result_msg.content_blocks[0] {
            ContentBlock::ToolResult { is_error, .. } => {
                assert_eq!(is_error, &Some(true));
            }
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_chat_session_add_tool_results_batched() {
        let mut session = ChatSession::new();
        session.add_user_message("Read two files");
        let tool_calls = vec![
            ToolCallInfo::new("call_1", "read_file", serde_json::json!({})),
            ToolCallInfo::new("call_2", "read_file", serde_json::json!({})),
        ];
        session.add_assistant_message("Reading both...", tool_calls);
        session.add_tool_results(vec![
            ("call_1".to_string(), "Contents 1".to_string(), false),
            ("call_2".to_string(), "Contents 2".to_string(), false),
        ]);

        // Both tool calls should be completed
        let assistant_msg = &session.messages[1];
        assert_eq!(assistant_msg.tool_calls[0].status, ToolCallStatus::Completed);
        assert_eq!(assistant_msg.tool_calls[1].status, ToolCallStatus::Completed);

        // Single batched result message
        assert_eq!(session.message_count(), 3);
        let result_msg = &session.messages[2];
        assert_eq!(result_msg.content_blocks.len(), 2);
    }

    #[test]
    fn test_chat_session_to_llm_messages_text_only() {
        use crate::provider::Role;

        let mut session = ChatSession::new();
        session.add_user_message("Hello");
        session.add_assistant_message("Hi there!", vec![]);

        let llm_messages = session.to_llm_messages();
        assert_eq!(llm_messages.len(), 2);
        assert_eq!(llm_messages[0].role, Role::User);
        assert_eq!(llm_messages[1].role, Role::Assistant);
    }

    #[test]
    fn test_chat_session_to_llm_messages_with_tool_calls() {
        use crate::provider::{MessageContent, Role};

        let mut session = ChatSession::new();
        session.add_user_message("Read file");
        let tool_calls = vec![ToolCallInfo::new("call_1", "read_file", serde_json::json!({"path": "/test.txt"}))];
        session.add_assistant_message("Reading...", tool_calls);
        session.add_tool_result("call_1", "File contents");

        let llm_messages = session.to_llm_messages();
        assert_eq!(llm_messages.len(), 3);

        // User message
        assert_eq!(llm_messages[0].role, Role::User);

        // Assistant message with tool call
        assert_eq!(llm_messages[1].role, Role::Assistant);
        match &llm_messages[1].content {
            MessageContent::Blocks(blocks) => {
                assert!(blocks.len() >= 2); // text + tool_use
            }
            _ => panic!("Expected Blocks for assistant with tools"),
        }

        // Tool result message
        assert_eq!(llm_messages[2].role, Role::User);
        match &llm_messages[2].content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult { tool_use_id, .. } => {
                        assert_eq!(tool_use_id, "call_1");
                    }
                    _ => panic!("Expected ToolResult block"),
                }
            }
            _ => panic!("Expected Blocks for tool result"),
        }
    }

    #[test]
    fn test_chat_session_reject_tool() {
        let mut session = ChatSession::new();
        session.add_user_message("Delete all files");
        let tool_calls = vec![ToolCallInfo::new("call_1", "delete_all", serde_json::json!({}))];
        session.add_assistant_message("Deleting...", tool_calls);
        session.reject_tool("call_1");

        let assistant_msg = &session.messages[1];
        assert_eq!(assistant_msg.tool_calls[0].status, ToolCallStatus::Rejected);
    }

    #[test]
    fn test_chat_session_clear() {
        let mut session = ChatSession::new();
        session.add_user_message("Hello");
        session.add_assistant_message("Hi", vec![]);
        assert_eq!(session.message_count(), 2);

        session.clear();
        assert_eq!(session.message_count(), 0);
    }

    #[test]
    fn test_chat_session_has_pending_tools() {
        let mut session = ChatSession::new();
        session.add_user_message("Hello");
        assert!(!session.has_pending_tools());

        let tool_calls = vec![ToolCallInfo::new("call_1", "read_file", serde_json::json!({}))];
        session.add_assistant_message("Reading...", tool_calls);
        assert!(session.has_pending_tools());

        session.add_tool_result("call_1", "Contents");
        assert!(!session.has_pending_tools());
    }

    #[test]
    fn test_chat_session_pending_tools() {
        let mut session = ChatSession::new();
        let tool_calls = vec![
            ToolCallInfo::new("call_1", "read_file", serde_json::json!({})),
            ToolCallInfo::new("call_2", "grep", serde_json::json!({})),
        ];
        session.add_assistant_message("Doing both...", tool_calls);

        let pending = session.pending_tools();
        assert_eq!(pending.len(), 2);
        let pending_ids: Vec<&str> = pending.iter().map(|tc| tc.id.as_str()).collect();
        assert!(pending_ids.contains(&"call_1"));
        assert!(pending_ids.contains(&"call_2"));

        session.add_tool_result("call_1", "Result 1");
        let pending = session.pending_tools();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "call_2");
    }
}
