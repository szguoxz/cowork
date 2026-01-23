//! GenAI-based LLM provider implementation
//!
//! Uses the genai framework to support multiple LLM providers with manual tool control.
//! This gives us the ability to implement approval flows for tool execution.
//!
//! ## LLM Request/Response Logging
//!
//! Set the `LLM_LOG_FILE` environment variable to enable detailed logging of all
//! LLM requests and responses to a JSON file. This is useful for debugging context
//! issues, token usage, and model behavior.
//!
//! Example: `LLM_LOG_FILE=/tmp/llm.log cowork`

use async_trait::async_trait;
use futures::StreamExt;
use genai::chat::{ChatMessage, ChatRequest, ChatStreamEvent, Tool, ToolCall, ToolResponse};
use genai::resolver::{AuthData, AuthResolver};
use genai::Client;
use serde::{Deserialize, Serialize};
use std::io::Write;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::tools::ToolDefinition;
use super::model_catalog;

/// Log LLM request/response to file if LLM_LOG_FILE is set
fn log_llm_interaction(
    model: &str,
    messages: &[LlmMessage],
    tools: Option<&[ToolDefinition]>,
    result: Option<&CompletionResult>,
    error: Option<&str>,
) {
    let log_file = match std::env::var("LLM_LOG_FILE") {
        Ok(path) => path,
        Err(_) => return, // No logging if env var not set
    };

    let entry = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "model": model,
        "request": {
            "messages": messages,
            "message_count": messages.len(),
            "tools": tools.map(|t| t.iter().map(|tool| &tool.name).collect::<Vec<_>>()),
            "tool_count": tools.map(|t| t.len()).unwrap_or(0),
            // Estimate request size
            "estimated_chars": messages.iter()
                .map(|m| m.content_as_text().len())
                .sum::<usize>(),
        },
        "response": result.map(|r| match r {
            CompletionResult::Message(content) => serde_json::json!({
                "type": "message",
                "content_length": content.len(),
                "content_preview": if content.len() > 500 {
                    format!("{}...", &content[..500])
                } else {
                    content.clone()
                }
            }),
            CompletionResult::ToolCalls(calls) => serde_json::json!({
                "type": "tool_calls",
                "calls": calls.iter().map(|c| serde_json::json!({
                    "name": c.name,
                    "call_id": c.call_id,
                    "arguments": c.arguments
                })).collect::<Vec<_>>()
            }),
        }),
        "error": error,
    });

    // Append to log file
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        Ok(mut file) => {
            if let Err(e) = writeln!(file, "{}", serde_json::to_string(&entry).unwrap_or_default()) {
                warn!("Failed to write to LLM log file: {}", e);
            }
        }
        Err(e) => {
            warn!("Failed to open LLM log file {}: {}", log_file, e);
        }
    }

    debug!("Logged LLM interaction to {}", log_file);
}

use super::{ContentBlock, LlmMessage, LlmProvider, LlmRequest, LlmResponse, MessageContent, TokenUsage};

/// Supported LLM provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// OpenAI (GPT-4, GPT-4o, GPT-3.5, etc.)
    OpenAI,
    /// Anthropic (Claude 3.5, Claude 3, etc.)
    Anthropic,
    /// Google Gemini
    Gemini,
    /// Cohere (Command R, etc.)
    Cohere,
    /// Perplexity
    Perplexity,
    /// Groq (fast inference)
    Groq,
    /// xAI (Grok)
    XAI,
    /// DeepSeek
    DeepSeek,
    /// Together AI
    Together,
    /// Fireworks AI
    Fireworks,
    /// Zai (Zhipu AI) - GLM models
    Zai,
    /// Nebius AI Studio
    Nebius,
    /// MIMO (Xiaomi)
    MIMO,
    /// BigModel.cn (Zhipu AI China)
    BigModel,
    /// Ollama (local)
    Ollama,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::OpenAI => write!(f, "openai"),
            ProviderType::Anthropic => write!(f, "anthropic"),
            ProviderType::Gemini => write!(f, "gemini"),
            ProviderType::Cohere => write!(f, "cohere"),
            ProviderType::Perplexity => write!(f, "perplexity"),
            ProviderType::Groq => write!(f, "groq"),
            ProviderType::XAI => write!(f, "xai"),
            ProviderType::DeepSeek => write!(f, "deepseek"),
            ProviderType::Together => write!(f, "together"),
            ProviderType::Fireworks => write!(f, "fireworks"),
            ProviderType::Zai => write!(f, "zai"),
            ProviderType::Nebius => write!(f, "nebius"),
            ProviderType::MIMO => write!(f, "mimo"),
            ProviderType::BigModel => write!(f, "bigmodel"),
            ProviderType::Ollama => write!(f, "ollama"),
        }
    }
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(ProviderType::OpenAI),
            "anthropic" => Ok(ProviderType::Anthropic),
            "gemini" | "google" => Ok(ProviderType::Gemini),
            "cohere" => Ok(ProviderType::Cohere),
            "perplexity" => Ok(ProviderType::Perplexity),
            "groq" => Ok(ProviderType::Groq),
            "xai" | "grok" => Ok(ProviderType::XAI),
            "deepseek" => Ok(ProviderType::DeepSeek),
            "together" => Ok(ProviderType::Together),
            "fireworks" => Ok(ProviderType::Fireworks),
            "zai" | "zhipu" => Ok(ProviderType::Zai),
            "nebius" => Ok(ProviderType::Nebius),
            "mimo" => Ok(ProviderType::MIMO),
            "bigmodel" => Ok(ProviderType::BigModel),
            "ollama" => Ok(ProviderType::Ollama),
            _ => Err(format!("Unknown provider: {}", s)),
        }
    }
}

impl ProviderType {
    /// Get the default model for this provider
    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => model_catalog::OPENAI_BALANCED.0,
            ProviderType::Anthropic => model_catalog::ANTHROPIC_BALANCED.0,
            ProviderType::Gemini => model_catalog::GEMINI_FAST.0,
            ProviderType::Cohere => model_catalog::COHERE_BALANCED.0,
            ProviderType::Perplexity => model_catalog::PERPLEXITY_BALANCED.0,
            ProviderType::Groq => model_catalog::GROQ_BALANCED.0,
            ProviderType::XAI => model_catalog::XAI_BALANCED.0,
            ProviderType::DeepSeek => model_catalog::DEEPSEEK_BALANCED.0,
            ProviderType::Together => model_catalog::TOGETHER_BALANCED.0,
            ProviderType::Fireworks => model_catalog::FIREWORKS_BALANCED.0,
            ProviderType::Zai => model_catalog::ZAI_BALANCED.0,
            ProviderType::Nebius => model_catalog::NEBIUS_BALANCED.0,
            ProviderType::MIMO => model_catalog::MIMO_BALANCED.0,
            ProviderType::BigModel => model_catalog::BIGMODEL_BALANCED.0,
            ProviderType::Ollama => model_catalog::OLLAMA_BALANCED.0,
        }
    }

    /// Get the environment variable name for API key
    pub fn api_key_env(&self) -> Option<&'static str> {
        match self {
            ProviderType::OpenAI => Some("OPENAI_API_KEY"),
            ProviderType::Anthropic => Some("ANTHROPIC_API_KEY"),
            ProviderType::Gemini => Some("GEMINI_API_KEY"),
            ProviderType::Cohere => Some("COHERE_API_KEY"),
            ProviderType::Perplexity => Some("PERPLEXITY_API_KEY"),
            ProviderType::Groq => Some("GROQ_API_KEY"),
            ProviderType::XAI => Some("XAI_API_KEY"),
            ProviderType::DeepSeek => Some("DEEPSEEK_API_KEY"),
            ProviderType::Together => Some("TOGETHER_API_KEY"),
            ProviderType::Fireworks => Some("FIREWORKS_API_KEY"),
            ProviderType::Zai => Some("ZAI_API_KEY"),
            ProviderType::Nebius => Some("NEBIUS_API_KEY"),
            ProviderType::MIMO => Some("MIMO_API_KEY"),
            ProviderType::BigModel => Some("BIGMODEL_API_KEY"),
            ProviderType::Ollama => None, // Local, no API key needed
        }
    }

    /// Get the provider type as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::Gemini => "gemini",
            ProviderType::Cohere => "cohere",
            ProviderType::Perplexity => "perplexity",
            ProviderType::Groq => "groq",
            ProviderType::XAI => "xai",
            ProviderType::DeepSeek => "deepseek",
            ProviderType::Together => "together",
            ProviderType::Fireworks => "fireworks",
            ProviderType::Zai => "zai",
            ProviderType::Nebius => "nebius",
            ProviderType::MIMO => "mimo",
            ProviderType::BigModel => "bigmodel",
            ProviderType::Ollama => "ollama",
        }
    }
}

/// Tool call from the LLM that needs approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl From<ToolCall> for PendingToolCall {
    fn from(tc: ToolCall) -> Self {
        Self {
            call_id: tc.call_id,
            name: tc.fn_name,
            arguments: tc.fn_arguments, // Already a serde_json::Value
        }
    }
}

/// Response from completion that may contain tool calls
#[derive(Debug, Clone)]
pub enum CompletionResult {
    /// Simple text response
    Message(String),
    /// Tool calls that need approval before execution
    ToolCalls(Vec<PendingToolCall>),
}

/// A provider implementation using genai
pub struct GenAIProvider {
    client: Client,
    provider_type: ProviderType,
    model: String,
    system_prompt: Option<String>,
}

impl GenAIProvider {
    /// Create a new provider with default settings (uses environment variables for auth)
    pub fn new(provider_type: ProviderType, model: Option<&str>) -> Self {
        let client = Client::default();
        Self {
            client,
            provider_type,
            model: model.unwrap_or(provider_type.default_model()).to_string(),
            system_prompt: None,
        }
    }

    /// Create a provider with a specific API key
    pub fn with_api_key(provider_type: ProviderType, api_key: &str, model: Option<&str>) -> Self {
        let api_key = api_key.to_string();
        let auth_resolver = AuthResolver::from_resolver_fn(
            move |_model_iden| -> std::result::Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(api_key.clone())))
            },
        );

        let client = Client::builder().with_auth_resolver(auth_resolver).build();

        Self {
            client,
            provider_type,
            model: model.unwrap_or(provider_type.default_model()).to_string(),
            system_prompt: None,
        }
    }

    /// Create a provider with API key and optional custom base URL
    ///
    /// Note: Custom base_url support is limited and depends on the provider.
    /// For most providers, the default API endpoint is used.
    pub fn with_config(
        provider_type: ProviderType,
        api_key: &str,
        model: Option<&str>,
        _base_url: Option<&str>,
    ) -> Self {
        // Note: base_url is accepted but not fully supported by genai yet
        // Future: implement custom endpoint support per provider
        Self::with_api_key(provider_type, api_key, model)
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Get the provider type
    pub fn provider_type(&self) -> ProviderType {
        self.provider_type
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Convert a user message (possibly with tool results) to genai format
    fn convert_user_message(&self, msg: &LlmMessage, chat_req: ChatRequest) -> ChatRequest {
        match &msg.content {
            MessageContent::Text(text) => {
                chat_req.append_message(ChatMessage::user(text))
            }
            MessageContent::Blocks(blocks) => {
                // Process content blocks - especially tool_result blocks
                let mut req = chat_req;
                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => {
                            req = req.append_message(ChatMessage::user(text));
                        }
                        ContentBlock::ToolResult { tool_use_id, content, is_error: _ } => {
                            // Tool results sent via ToolResponse for Anthropic/OpenAI compatibility
                            let tool_response = ToolResponse::new(tool_use_id.clone(), content.clone());
                            req = req.append_message(tool_response);
                        }
                        ContentBlock::ToolUse { .. } => {
                            // Tool use blocks in user messages are unusual, skip
                        }
                    }
                }
                req
            }
        }
    }

    /// Convert an assistant message (possibly with tool calls) to genai format
    fn convert_assistant_message(&self, msg: &LlmMessage, chat_req: ChatRequest) -> ChatRequest {
        // Check if this assistant message has tool calls (via tool_calls field or content blocks)
        let has_tool_calls = msg.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false);

        // Extract tool calls from content blocks if present
        let tool_calls_from_blocks: Vec<ToolCall> = match &msg.content {
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolUse { id, name, input } => Some(ToolCall {
                        call_id: id.clone(),
                        fn_name: name.clone(),
                        fn_arguments: input.clone(),
                        thought_signatures: None,
                    }),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };

        let has_tool_calls_in_blocks = !tool_calls_from_blocks.is_empty();

        match &msg.content {
            MessageContent::Text(text) if !has_tool_calls => {
                // Simple text response - no tool calls
                chat_req.append_message(ChatMessage::assistant(text))
            }
            MessageContent::Text(_text) => {
                // Text with tool calls - for DeepSeek/OpenAI, we need tool calls as a single message
                // The text content in tool call messages is usually empty or reasoning
                // genai handles this by converting Vec<ToolCall> to an assistant message with tool_calls
                let mut req = chat_req;
                if let Some(tool_calls) = &msg.tool_calls {
                    let genai_tool_calls: Vec<ToolCall> = tool_calls
                        .iter()
                        .map(|tc| ToolCall {
                            call_id: tc.id.clone(),
                            fn_name: tc.name.clone(),
                            fn_arguments: tc.arguments.clone(),
                            thought_signatures: None,
                        })
                        .collect();
                    req = req.append_message(genai_tool_calls);
                }
                req
            }
            MessageContent::Blocks(blocks) => {
                let mut req = chat_req;
                let mut text_content = String::new();

                for block in blocks {
                    if let ContentBlock::Text { text } = block {
                        text_content.push_str(text);
                    }
                }

                // If we have tool calls, use them as a single message (skip separate text)
                // For OpenAI/DeepSeek format, tool_calls must be in a single assistant message
                if has_tool_calls_in_blocks {
                    req = req.append_message(tool_calls_from_blocks);
                } else if let Some(tool_calls) = &msg.tool_calls {
                    // Tool calls from field (not blocks)
                    let genai_tool_calls: Vec<ToolCall> = tool_calls
                        .iter()
                        .map(|tc| ToolCall {
                            call_id: tc.id.clone(),
                            fn_name: tc.name.clone(),
                            fn_arguments: tc.arguments.clone(),
                            thought_signatures: None,
                        })
                        .collect();
                    req = req.append_message(genai_tool_calls);
                } else if !text_content.is_empty() {
                    // No tool calls, just text
                    req = req.append_message(ChatMessage::assistant(&text_content));
                }
                req
            }
        }
    }

    /// Execute a chat completion and return either a message or tool calls
    pub async fn chat(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResult> {
        // Keep copies for logging
        let messages_for_log = messages.clone();
        let tools_for_log = tools.clone();

        let mut chat_req = ChatRequest::default();

        // Add system prompt if set
        if let Some(system) = &self.system_prompt {
            chat_req = chat_req.with_system(system.as_str());
        }

        // Convert messages with proper tool call/result handling
        for msg in messages {
            match msg.role.as_str() {
                "user" => {
                    chat_req = self.convert_user_message(&msg, chat_req);
                }
                "assistant" => {
                    chat_req = self.convert_assistant_message(&msg, chat_req);
                }
                "tool" => {
                    // Tool result message (legacy format, kept for compatibility)
                    if let Some(call_id) = &msg.tool_call_id {
                        let content = msg.content_as_text();
                        let tool_response = ToolResponse::new(call_id.clone(), content);
                        chat_req = chat_req.append_message(tool_response);
                    }
                }
                "system" => {
                    let content = msg.content_as_text();
                    chat_req = chat_req.append_message(ChatMessage::system(&content));
                }
                _ => {
                    let content = msg.content_as_text();
                    chat_req = chat_req.append_message(ChatMessage::user(&content));
                }
            }
        }

        // Add tools if provided
        if let Some(tool_defs) = tools {
            let genai_tools: Vec<Tool> = tool_defs
                .into_iter()
                .map(|t| {
                    Tool::new(&t.name)
                        .with_description(&t.description)
                        .with_schema(t.parameters.clone())
                })
                .collect();
            chat_req = chat_req.with_tools(genai_tools);
        }

        // Execute the chat
        let chat_res = self
            .client
            .exec_chat(&self.model, chat_req, None)
            .await;

        // Handle result and log
        match chat_res {
            Ok(res) => {
                // Check for tool calls first (need to clone since into_tool_calls consumes)
                let tool_calls = res.clone().into_tool_calls();
                let result = if !tool_calls.is_empty() {
                    let pending: Vec<PendingToolCall> = tool_calls.into_iter().map(Into::into).collect();
                    CompletionResult::ToolCalls(pending)
                } else {
                    // Get text content
                    let content = res.first_text().unwrap_or("").to_string();
                    CompletionResult::Message(content)
                };

                // Log successful interaction
                log_llm_interaction(
                    &self.model,
                    &messages_for_log,
                    tools_for_log.as_deref(),
                    Some(&result),
                    None,
                );

                Ok(result)
            }
            Err(e) => {
                let error_msg = format!("GenAI error: {}", e);
                // Log failed interaction
                log_llm_interaction(
                    &self.model,
                    &messages_for_log,
                    tools_for_log.as_deref(),
                    None,
                    Some(&error_msg),
                );
                Err(Error::Provider(error_msg))
            }
        }
    }

    /// Continue a conversation after tool execution
    /// Takes the original request, the tool calls that were made, and the results
    pub async fn continue_with_tool_results(
        &self,
        mut chat_req: ChatRequest,
        tool_calls: Vec<PendingToolCall>,
        results: Vec<(String, String)>, // (call_id, result)
    ) -> Result<CompletionResult> {
        // Convert PendingToolCall back to genai ToolCall for the message
        let genai_tool_calls: Vec<ToolCall> = tool_calls
            .into_iter()
            .map(|tc| ToolCall {
                call_id: tc.call_id,
                fn_name: tc.name,
                fn_arguments: tc.arguments,
                thought_signatures: None,
            })
            .collect();

        // Add tool calls as assistant message
        chat_req = chat_req.append_message(genai_tool_calls);

        // Add tool results
        for (call_id, result) in results {
            let tool_response = ToolResponse::new(call_id, result);
            chat_req = chat_req.append_message(tool_response);
        }

        // Execute the chat again
        let chat_res = self
            .client
            .exec_chat(&self.model, chat_req, None)
            .await
            .map_err(|e| Error::Provider(format!("GenAI error: {}", e)))?;

        // Check for more tool calls
        let tool_calls = chat_res.clone().into_tool_calls();
        if !tool_calls.is_empty() {
            let pending: Vec<PendingToolCall> = tool_calls.into_iter().map(Into::into).collect();
            Ok(CompletionResult::ToolCalls(pending))
        } else {
            let content = chat_res.first_text().unwrap_or("").to_string();
            Ok(CompletionResult::Message(content))
        }
    }

    /// Execute a streaming chat completion
    /// Sends chunks to the provided channel as they arrive
    pub async fn chat_stream(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
        chunk_tx: mpsc::Sender<StreamChunk>,
    ) -> Result<CompletionResult> {
        let mut chat_req = ChatRequest::default();

        // Add system prompt if set
        if let Some(system) = &self.system_prompt {
            chat_req = chat_req.with_system(system.as_str());
        }

        // Convert messages with proper tool call/result handling (same as non-streaming)
        for msg in messages {
            match msg.role.as_str() {
                "user" => {
                    chat_req = self.convert_user_message(&msg, chat_req);
                }
                "assistant" => {
                    chat_req = self.convert_assistant_message(&msg, chat_req);
                }
                "tool" => {
                    // Tool result message (legacy format)
                    if let Some(call_id) = &msg.tool_call_id {
                        let content = msg.content_as_text();
                        let tool_response = ToolResponse::new(call_id.clone(), content);
                        chat_req = chat_req.append_message(tool_response);
                    }
                }
                "system" => {
                    let content = msg.content_as_text();
                    chat_req = chat_req.append_message(ChatMessage::system(&content));
                }
                _ => {
                    let content = msg.content_as_text();
                    chat_req = chat_req.append_message(ChatMessage::user(&content));
                }
            }
        }

        // Add tools if provided
        if let Some(tool_defs) = tools {
            let genai_tools: Vec<Tool> = tool_defs
                .into_iter()
                .map(|t| {
                    Tool::new(&t.name)
                        .with_description(&t.description)
                        .with_schema(t.parameters.clone())
                })
                .collect();
            chat_req = chat_req.with_tools(genai_tools);
        }

        // Execute streaming chat
        let stream_response = self
            .client
            .exec_chat_stream(&self.model, chat_req, None)
            .await
            .map_err(|e| Error::Provider(format!("GenAI stream error: {}", e)))?;

        // Get the actual stream from the response
        let mut stream = stream_response.stream;

        let mut accumulated_text = String::new();
        let mut tool_calls: Vec<PendingToolCall> = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(event) => match event {
                    ChatStreamEvent::Start => {
                        let _ = chunk_tx.send(StreamChunk::Start).await;
                    }
                    ChatStreamEvent::Chunk(chunk) => {
                        accumulated_text.push_str(&chunk.content);
                        let _ = chunk_tx
                            .send(StreamChunk::TextDelta(chunk.content))
                            .await;
                    }
                    ChatStreamEvent::ReasoningChunk(reasoning) => {
                        // Emit reasoning/thinking content for display
                        let _ = chunk_tx
                            .send(StreamChunk::Thinking(reasoning.content))
                            .await;
                    }
                    ChatStreamEvent::ThoughtSignatureChunk(_) => {
                        // Thought signatures are internal, not displayed to user
                    }
                    ChatStreamEvent::ToolCallChunk(tc_chunk) => {
                        // Tool call received - genai sends complete tool calls, not deltas
                        let tc = tc_chunk.tool_call;
                        let call_id = tc.call_id.clone();
                        let name = tc.fn_name.clone();

                        let _ = chunk_tx
                            .send(StreamChunk::ToolCallStart {
                                id: call_id.clone(),
                                name: name.clone(),
                            })
                            .await;

                        let args_str = tc.fn_arguments.to_string();
                        let _ = chunk_tx
                            .send(StreamChunk::ToolCallDelta {
                                id: call_id.clone(),
                                delta: args_str,
                            })
                            .await;

                        tool_calls.push(PendingToolCall {
                            call_id: call_id.clone(),
                            name,
                            arguments: tc.fn_arguments,
                        });

                        let _ = chunk_tx.send(StreamChunk::ToolCallComplete(call_id)).await;
                    }
                    ChatStreamEvent::End(end_info) => {
                        // Determine finish reason
                        let reason = if !tool_calls.is_empty() {
                            "tool_calls"
                        } else {
                            "stop"
                        };
                        let _ = chunk_tx.send(StreamChunk::End(reason.to_string())).await;

                        // If we have captured content from the end event, use it
                        if let Some(content) = end_info.captured_content {
                            // Update tool calls from captured content if available
                            let captured_tool_calls = content.into_tool_calls();
                            if !captured_tool_calls.is_empty() && tool_calls.is_empty() {
                                tool_calls = captured_tool_calls
                                    .into_iter()
                                    .map(|tc| PendingToolCall {
                                        call_id: tc.call_id,
                                        name: tc.fn_name,
                                        arguments: tc.fn_arguments,
                                    })
                                    .collect();
                            }
                        }
                    }
                },
                Err(e) => {
                    let _ = chunk_tx
                        .send(StreamChunk::Error(e.to_string()))
                        .await;
                    return Err(Error::Provider(format!("Stream error: {}", e)));
                }
            }
        }

        // Return result
        if !tool_calls.is_empty() {
            Ok(CompletionResult::ToolCalls(tool_calls))
        } else {
            Ok(CompletionResult::Message(accumulated_text))
        }
    }
}

/// Streaming chunk types
#[derive(Debug, Clone)]
pub enum StreamChunk {
    Start,
    Thinking(String),
    TextDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, delta: String },
    ToolCallComplete(String),
    End(String),
    Error(String),
}

// Implement LlmProvider trait for compatibility with existing code
#[async_trait]
impl LlmProvider for GenAIProvider {
    fn name(&self) -> &str {
        match self.provider_type {
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::Gemini => "gemini",
            ProviderType::Cohere => "cohere",
            ProviderType::Perplexity => "perplexity",
            ProviderType::Groq => "groq",
            ProviderType::XAI => "xai",
            ProviderType::DeepSeek => "deepseek",
            ProviderType::Together => "together",
            ProviderType::Fireworks => "fireworks",
            ProviderType::Zai => "zai",
            ProviderType::Nebius => "nebius",
            ProviderType::MIMO => "mimo",
            ProviderType::BigModel => "bigmodel",
            ProviderType::Ollama => "ollama",
        }
    }

    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Convert tools
        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(request.tools.clone())
        };

        // Add system prompt from request if present
        let mut messages = request.messages.clone();
        if let Some(system) = &request.system_prompt {
            messages.insert(
                0,
                LlmMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text(system.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                },
            );
        }

        match self.chat(messages, tools).await? {
            CompletionResult::Message(content) => Ok(LlmResponse {
                content: Some(content),
                tool_calls: Vec::new(),
                finish_reason: "stop".to_string(),
                usage: TokenUsage::default(),
            }),
            CompletionResult::ToolCalls(pending) => Ok(LlmResponse {
                content: None,
                tool_calls: pending
                    .into_iter()
                    .map(|tc| super::ToolCall {
                        id: tc.call_id,
                        name: tc.name,
                        arguments: tc.arguments,
                    })
                    .collect(),
                finish_reason: "tool_calls".to_string(),
                usage: TokenUsage::default(),
            }),
        }
    }

    async fn health_check(&self) -> Result<bool> {
        let request = LlmRequest::new(vec![LlmMessage::user("Hi")]);

        match self.complete(request).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Create a provider from configuration
pub fn create_provider(
    provider_type: ProviderType,
    api_key: Option<&str>,
    model: Option<&str>,
    system_prompt: Option<&str>,
) -> Result<GenAIProvider> {
    let provider = if let Some(key) = api_key {
        GenAIProvider::with_api_key(provider_type, key, model)
    } else {
        GenAIProvider::new(provider_type, model)
    };

    let provider = if let Some(prompt) = system_prompt {
        provider.with_system_prompt(prompt)
    } else {
        provider.with_system_prompt(
            "You are Cowork, a helpful AI assistant for software development tasks.",
        )
    };

    Ok(provider)
}

