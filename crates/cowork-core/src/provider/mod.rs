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

/// Helper to create a tool result ChatMessage
pub fn tool_result_message(call_id: impl Into<String>, content: impl Into<String>) -> ChatMessage {
    ChatMessage::from(ToolResponse::new(call_id.into(), content.into()))
}

/// Helper to create an assistant message with tool calls
/// If content is provided, it's added as a text part before the tool calls
pub fn assistant_with_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> ChatMessage {
    let mut parts: Vec<ContentPart> = Vec::new();
    if let Some(text) = content
        && !text.is_empty()
    {
        parts.push(ContentPart::Text(text));
    }
    parts.extend(tool_calls.into_iter().map(ContentPart::ToolCall));
    ChatMessage::assistant(MessageContent::from_parts(parts))
}

/// Get text content from a ChatMessage (for logging/display)
/// This extracts text from regular messages and also from tool responses
pub fn message_text_content(msg: &ChatMessage) -> String {
    // First try to get joined texts (for regular messages)
    if let Some(text) = msg.content.joined_texts() {
        return text;
    }
    // For tool response messages, extract the content from ToolResponse parts
    let tool_responses = msg.content.tool_responses();
    if !tool_responses.is_empty() {
        return tool_responses.iter()
            .map(|tr| tr.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

/// Append text to a ChatMessage's content (for system reminders)
pub fn append_message_text(msg: &mut ChatMessage, text: &str) {
    msg.content = msg.content.clone().append(ContentPart::Text(text.to_string()));
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
