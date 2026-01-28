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
mod logging;
pub mod model_listing;

pub use factory::{
    create_provider_from_config, create_provider_from_provider_config,
    create_provider_with_settings, get_api_key, get_model_tiers, has_api_key_configured,
};
pub use genai_provider::{
    create_provider, CompletionResult, GenAIProvider,
};

pub use model_listing::{get_known_models, get_model_context_limit, ModelInfo};

use serde::{Deserialize, Serialize};

use crate::error::Result;

// Re-export ChatRole from genai as our Role type
pub use genai::chat::ChatRole;

// Re-export ToolCall from genai (uses call_id, fn_name, fn_arguments)
pub use genai::chat::ToolCall;

// Re-export Usage from genai as TokenUsage
pub use genai::chat::Usage as TokenUsage;

// Re-export message types from genai
pub use genai::chat::{ChatMessage, ContentPart, MessageContent, ToolResponse};

/// Message for LLM API calls
///
/// This enum wraps genai types to support both regular messages and tool results
/// in a single conversation history. genai uses different types for these:
/// - Regular messages: ChatMessage
/// - Tool results: ToolResponse
#[derive(Debug, Clone)]
pub enum LlmMessage {
    /// Regular chat message (user, assistant, system)
    Chat(ChatMessage),
    /// Tool result message
    ToolResult(ToolResponse),
    /// Assistant message with tool calls
    AssistantToolCalls {
        /// Optional text content before/alongside tool calls
        content: Option<String>,
        /// The tool calls
        tool_calls: Vec<ToolCall>,
    },
}

impl LlmMessage {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self::Chat(ChatMessage::user(content.into()))
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Chat(ChatMessage::assistant(content.into()))
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self::Chat(ChatMessage::system(content.into()))
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self::AssistantToolCalls { content, tool_calls }
    }

    /// Create a tool result message
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::ToolResult(ToolResponse::new(call_id.into(), content.into()))
    }

    /// Get the role of this message
    pub fn role(&self) -> ChatRole {
        match self {
            Self::Chat(msg) => msg.role.clone(),
            Self::ToolResult(_) => ChatRole::Tool,
            Self::AssistantToolCalls { .. } => ChatRole::Assistant,
        }
    }

    /// Get text content as a string (for logging/display)
    pub fn content_as_text(&self) -> String {
        match self {
            Self::Chat(msg) => {
                // Use genai's joined_texts() method which combines all text parts
                msg.content.joined_texts().unwrap_or_default()
            }
            Self::ToolResult(resp) => resp.content.to_string(),
            Self::AssistantToolCalls { content, .. } => content.clone().unwrap_or_default(),
        }
    }

    /// Append text to message content (for system reminders)
    pub fn append_text(&mut self, text: &str) {
        match self {
            Self::Chat(msg) => {
                // Use genai's append method to add text
                msg.content = msg.content.clone().append(ContentPart::Text(text.to_string()));
            }
            Self::ToolResult(_) => {
                // Can't append to tool results
            }
            Self::AssistantToolCalls { content, .. } => {
                let existing = content.take().unwrap_or_default();
                *content = Some(format!("{}{}", existing, text));
            }
        }
    }
}

/// Parse a role string into ChatRole
pub fn parse_role(s: &str) -> ChatRole {
    match s {
        "user" => ChatRole::User,
        "assistant" => ChatRole::Assistant,
        "system" => ChatRole::System,
        "tool" => ChatRole::Tool,
        _ => ChatRole::User, // Default for unknown roles
    }
}


/// Trait for LLM providers (simplified - mainly for health checks)
#[allow(async_fn_in_trait)]
pub trait LlmProvider: Send + Sync {
    /// Provider name (e.g., "openai", "anthropic")
    fn name(&self) -> &str;

    /// Check if the provider is available
    async fn health_check(&self) -> Result<bool>;
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider ID (e.g., "anthropic", "openai", "together")
    pub provider_id: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub default_max_tokens: u32,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: "anthropic".to_string(),
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

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_user() {
        let msg = ChatMessage::user("Hello");
        assert!(matches!(msg.role, ChatRole::User));
    }

    #[test]
    fn test_chat_message_assistant() {
        let msg = ChatMessage::assistant("Hi there");
        assert!(matches!(msg.role, ChatRole::Assistant));
    }
}
