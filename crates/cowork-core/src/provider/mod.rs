//! LLM Provider abstraction using GenAI
//!
//! This module provides a unified interface to multiple LLM providers
//! through the genai framework. Supported providers include:
//! - OpenAI (GPT-4, GPT-4o, etc.)
//! - Anthropic (Claude 3.5, Claude 3, etc.)
//! - Google Gemini
//! - Cohere
//! - Groq
//! - DeepSeek
//! - xAI
//! - Ollama (local)

pub mod catalog;
pub mod factory;
mod genai_provider;
pub mod model_listing;
pub mod rig_provider;

// Re-export rig provider types for convenience
pub use rig_provider::{
    RigAgentConfig, RigAgentError, RigProviderType, ToolContext,
    create_wrapped_tools, run_rig_agent,
};

pub use factory::{
    create_provider_from_config, create_provider_from_provider_config, create_provider_with_settings,
    get_api_key, get_model_tiers, has_api_key_configured,
};
pub use genai_provider::{
    create_provider, CompletionResult, GenAIProvider, PendingToolCall, ProviderType,
};

pub use model_listing::{get_known_models, get_model_context_limit, ModelInfo};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::tools::ToolDefinition;

/// Content block types for messages (aligned with Anthropic API spec)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text content
    Text { text: String },
    /// Tool use request from assistant
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result from user
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

impl ContentBlock {
    /// Create a text content block
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text { text: text.into() }
    }

    /// Create a tool use content block
    pub fn tool_use(id: impl Into<String>, name: impl Into<String>, input: serde_json::Value) -> Self {
        ContentBlock::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    /// Create a tool result content block
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        ContentBlock::ToolResult {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: if is_error { Some(true) } else { None },
        }
    }
}

/// Message content can be either plain text or an array of content blocks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text content
    Text(String),
    /// Array of content blocks (for tool calls/results)
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    /// Check if content is empty
    pub fn is_empty(&self) -> bool {
        match self {
            MessageContent::Text(s) => s.is_empty(),
            MessageContent::Blocks(blocks) => blocks.is_empty(),
        }
    }

    /// Get text content if it's a simple text message
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s),
            MessageContent::Blocks(_) => None,
        }
    }

    /// Get content blocks if present
    pub fn as_blocks(&self) -> Option<&[ContentBlock]> {
        match self {
            MessageContent::Text(_) => None,
            MessageContent::Blocks(blocks) => Some(blocks),
        }
    }
}

impl Default for MessageContent {
    fn default() -> Self {
        MessageContent::Text(String::new())
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        MessageContent::Text(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        MessageContent::Text(s.to_string())
    }
}

impl From<Vec<ContentBlock>> for MessageContent {
    fn from(blocks: Vec<ContentBlock>) -> Self {
        MessageContent::Blocks(blocks)
    }
}

/// Message for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    /// Content can be simple text or array of content blocks
    pub content: MessageContent,
    /// Tool calls made by the assistant (only for role="assistant")
    /// Note: Also represented in content blocks, but kept for backwards compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call ID this message is responding to (only for role="tool")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl LlmMessage {
    /// Create a simple user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: MessageContent::Text(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: MessageContent::Text(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        let text = content.into();
        let mut blocks = Vec::new();
        if !text.is_empty() {
            blocks.push(ContentBlock::text(&text));
        }
        for tc in &tool_calls {
            blocks.push(ContentBlock::tool_use(&tc.id, &tc.name, tc.arguments.clone()));
        }
        Self {
            role: "assistant".to_string(),
            content: MessageContent::Blocks(blocks),
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            tool_call_id: None,
        }
    }

    /// Create a tool result message with proper content block
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        let id = tool_call_id.into();
        Self {
            role: "user".to_string(), // Tool results are user messages with content blocks
            content: MessageContent::Blocks(vec![
                ContentBlock::tool_result(&id, content, is_error)
            ]),
            tool_calls: None,
            tool_call_id: Some(id),
        }
    }

    /// Create a message with multiple tool results (batched)
    pub fn tool_results(results: Vec<ContentBlock>) -> Self {
        Self {
            role: "user".to_string(),
            content: MessageContent::Blocks(results),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Get content as text (for backwards compatibility)
    pub fn content_as_text(&self) -> String {
        match &self.content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Blocks(blocks) => {
                blocks.iter().filter_map(|b| {
                    match b {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    }
                }).collect::<Vec<_>>().join("")
            }
        }
    }
}

/// Request to an LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub system_prompt: Option<String>,
}

impl LlmRequest {
    pub fn new(messages: Vec<LlmMessage>) -> Self {
        Self {
            messages,
            tools: Vec::new(),
            max_tokens: None,
            temperature: None,
            system_prompt: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_system(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }
}

/// A tool call from the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Response from an LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
    pub usage: TokenUsage,
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Trait for LLM providers
#[allow(async_fn_in_trait)] // All impls are internal and Send
pub trait LlmProvider: Send + Sync {
    /// Provider name (e.g., "openai", "anthropic")
    fn name(&self) -> &str;

    /// Send a request and get a response
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse>;

    /// Check if the provider is available
    async fn health_check(&self) -> Result<bool>;
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_type: ProviderType,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub default_max_tokens: u32,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: ProviderType::Anthropic,
            api_key: None,
            base_url: None,
            model: catalog::default_model("anthropic").unwrap_or("claude-sonnet-4-5-20250929").to_string(),
            default_max_tokens: 4096,
        }
    }
}

/// Placeholder provider for development/testing
pub struct MockProvider {
    name: String,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            name: "mock".to_string(),
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse> {
        Ok(LlmResponse {
            content: Some("Mock response".to_string()),
            tool_calls: Vec::new(),
            finish_reason: "stop".to_string(),
            usage: TokenUsage::default(),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_block_text_serialization() {
        let block = ContentBlock::text("Hello world");
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"text\":\"Hello world\""));
    }

    #[test]
    fn test_content_block_tool_use_serialization() {
        let block = ContentBlock::tool_use(
            "call_123",
            "read_file",
            serde_json::json!({"path": "/test.txt"}),
        );
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"tool_use\""));
        assert!(json.contains("\"id\":\"call_123\""));
        assert!(json.contains("\"name\":\"read_file\""));
    }

    #[test]
    fn test_content_block_tool_result_serialization() {
        let block = ContentBlock::tool_result("call_123", "file contents", false);
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"tool_result\""));
        assert!(json.contains("\"tool_use_id\":\"call_123\""));
        assert!(json.contains("\"content\":\"file contents\""));
        // is_error should not be present when false
        assert!(!json.contains("is_error"));
    }

    #[test]
    fn test_content_block_tool_result_with_error_serialization() {
        let block = ContentBlock::tool_result("call_123", "Error message", true);
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"is_error\":true"));
    }

    #[test]
    fn test_content_block_deserialization() {
        let json = r#"{"type":"text","text":"Hello"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text block"),
        }
    }

    #[test]
    fn test_content_block_tool_result_deserialization() {
        let json = r#"{"type":"tool_result","tool_use_id":"abc","content":"result","is_error":true}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                assert_eq!(tool_use_id, "abc");
                assert_eq!(content, "result");
                assert_eq!(is_error, Some(true));
            }
            _ => panic!("Expected ToolResult block"),
        }
    }

    #[test]
    fn test_message_content_text() {
        let content = MessageContent::Text("Hello".to_string());
        assert!(!content.is_empty());
        assert_eq!(content.as_text(), Some("Hello"));
        assert!(content.as_blocks().is_none());
    }

    #[test]
    fn test_message_content_blocks() {
        let content = MessageContent::Blocks(vec![
            ContentBlock::text("Hello"),
            ContentBlock::text("World"),
        ]);
        assert!(!content.is_empty());
        assert!(content.as_text().is_none());
        assert!(content.as_blocks().is_some());
        assert_eq!(content.as_blocks().unwrap().len(), 2);
    }

    #[test]
    fn test_message_content_serialization() {
        let text_content = MessageContent::Text("Hello".to_string());
        let json = serde_json::to_string(&text_content).unwrap();
        assert_eq!(json, "\"Hello\"");

        let blocks_content = MessageContent::Blocks(vec![
            ContentBlock::text("Hello"),
        ]);
        let json = serde_json::to_string(&blocks_content).unwrap();
        assert!(json.contains("\"type\":\"text\""));
    }

    #[test]
    fn test_llm_message_user() {
        let msg = LlmMessage::user("Hello");
        assert_eq!(msg.role, "user");
        match msg.content {
            MessageContent::Text(s) => assert_eq!(s, "Hello"),
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_llm_message_tool_result() {
        let msg = LlmMessage::tool_result("call_123", "Result content", false);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
        match &msg.content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                        assert_eq!(tool_use_id, "call_123");
                        assert_eq!(content, "Result content");
                        assert!(is_error.is_none());
                    }
                    _ => panic!("Expected ToolResult block"),
                }
            }
            _ => panic!("Expected Blocks content"),
        }
    }

    #[test]
    fn test_llm_message_tool_results_batched() {
        let results = vec![
            ContentBlock::tool_result("call_1", "Result 1", false),
            ContentBlock::tool_result("call_2", "Result 2", true),
        ];
        let msg = LlmMessage::tool_results(results);
        assert_eq!(msg.role, "user");
        match &msg.content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
            }
            _ => panic!("Expected Blocks content"),
        }
    }

    #[test]
    fn test_llm_message_assistant_with_tools() {
        let tool_calls = vec![
            ToolCall {
                id: "call_1".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({"path": "/test.txt"}),
            }
        ];
        let msg = LlmMessage::assistant_with_tools("Let me read that file", tool_calls);
        assert_eq!(msg.role, "assistant");
        assert!(msg.tool_calls.is_some());
        match &msg.content {
            MessageContent::Blocks(blocks) => {
                // Should have text + tool_use blocks
                assert!(blocks.len() >= 2);
            }
            _ => panic!("Expected Blocks content"),
        }
    }

    #[test]
    fn test_content_as_text() {
        let msg = LlmMessage::user("Hello");
        assert_eq!(msg.content_as_text(), "Hello");

        let msg_blocks = LlmMessage {
            role: "user".to_string(),
            content: MessageContent::Blocks(vec![
                ContentBlock::text("Hello "),
                ContentBlock::text("World"),
            ]),
            tool_calls: None,
            tool_call_id: None,
        };
        assert_eq!(msg_blocks.content_as_text(), "Hello World");
    }
}
