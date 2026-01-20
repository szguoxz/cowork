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

pub mod factory;
mod genai_provider;
mod model_listing;

pub use factory::{
    create_provider_from_config, create_provider_from_provider_config, create_provider_with_settings,
    get_api_key, get_model_tiers, has_api_key_configured,
};
pub use genai_provider::{
    create_provider, models, CompletionResult, GenAIProvider, PendingToolCall, ProviderType,
    StreamChunk,
};
pub use model_listing::{fetch_models, ModelInfo};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::tools::ToolDefinition;

/// Message for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
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
#[async_trait]
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
            model: "claude-sonnet-4-20250514".to_string(),
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

#[async_trait]
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
