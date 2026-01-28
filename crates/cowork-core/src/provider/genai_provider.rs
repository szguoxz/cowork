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

use genai::adapter::AdapterKind;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, Tool, ToolCall, ToolResponse};
use genai::resolver::{AuthData, AuthResolver, Endpoint};
use genai::ModelIden;
use genai::ServiceTarget;
use genai::WebConfig;
use genai::Client;
use std::time::Duration;
use tracing::{debug, error, warn};

/// Retry configuration for different error types
struct RetryConfig {
    /// Delay before retrying on empty response
    empty_response_delay: Duration,
    /// Delay before retrying on rate limit
    rate_limit_delay: Duration,
    /// Delay before retrying on JSON parse error
    json_error_delay: Duration,
    /// Maximum retries per error type
    max_retries: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            empty_response_delay: Duration::from_secs(5),
            rate_limit_delay: Duration::from_secs(60),
            json_error_delay: Duration::from_secs(5),
            max_retries: 1,
        }
    }
}

/// Check if an error indicates rate limiting (HTTP 429)
fn is_rate_limit_error(e: &genai::Error) -> bool {
    match e {
        genai::Error::WebModelCall { webc_error, .. }
        | genai::Error::WebAdapterCall { webc_error, .. } => matches!(
            webc_error,
            genai::webc::Error::ResponseFailedStatus { status, .. }
                if status.as_u16() == 429
        ),
        _ => false,
    }
}

/// Check if an error indicates a JSON parse failure
/// This happens when the provider returns HTTP 200 but with malformed/truncated JSON
fn is_json_parse_error(e: &genai::Error) -> bool {
    match e {
        genai::Error::WebModelCall { webc_error, .. }
        | genai::Error::WebAdapterCall { webc_error, .. } => matches!(
            webc_error,
            genai::webc::Error::ResponseFailedInvalidJson { .. }
                | genai::webc::Error::ResponseFailedNotJson { .. }
        ),
        _ => false,
    }
}

/// Extract detailed error information from a genai error
///
/// Uses Debug format to capture full error details.
/// Returns (error_message, full_debug_output)
fn extract_genai_error_details(e: &genai::Error) -> (String, Option<String>) {
    // Use Debug format to get all available error information
    // This is version-agnostic and captures nested error details
    let error_debug = format!("{:#?}", e);  // Pretty-printed debug

    // Always return the full debug output - it contains all available info
    // including any embedded body, status codes, headers, etc.
    (format!("{}", e), Some(error_debug))
}

use crate::error::{Error, Result};
use crate::tools::ToolDefinition;
use super::catalog;
use super::logging::{log_llm_interaction, LogConfig};
use super::model_listing::get_model_max_output;

/// Response from completion that may contain both content and tool calls
#[derive(Debug, Clone, Default)]
pub struct CompletionResult {
    /// Text content from the assistant (may be present even with tool calls)
    pub content: Option<String>,
    /// Tool calls that need approval before execution
    pub tool_calls: Vec<ToolCall>,
    /// Input tokens used for this request (from provider)
    pub input_tokens: Option<u64>,
    /// Output tokens used for this response (from provider)
    pub output_tokens: Option<u64>,
}

impl CompletionResult {
    /// Check if this result has any tool calls
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Check if this result has text content
    pub fn has_content(&self) -> bool {
        self.content.as_ref().map(|c| !c.is_empty()).unwrap_or(false)
    }
}

/// A provider implementation using genai
pub struct GenAIProvider {
    client: Client,
    /// Provider ID (e.g., "anthropic", "together")
    provider_id: String,
    /// The genai adapter to use
    adapter: AdapterKind,
    model: String,
    system_prompt: Option<String>,
}

impl GenAIProvider {
    /// Default timeout for LLM API requests (5 minutes)
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

    /// Create WebConfig with appropriate timeouts for LLM requests
    fn default_web_config() -> WebConfig {
        WebConfig::default()
            .with_timeout(Self::DEFAULT_TIMEOUT)
            .with_connect_timeout(Duration::from_secs(30))
    }

    /// Create a new provider with default settings (uses environment variables for auth)
    ///
    /// The provider_id is looked up in the catalog to get the adapter and default model.
    pub fn new(provider_id: &str, model: Option<&str>) -> Self {
        let provider = catalog::get(provider_id)
            .unwrap_or_else(|| panic!("Unknown provider: {}", provider_id));
        let adapter = provider.adapter;

        // Use model mapper to force the correct adapter for this provider
        let client = Client::builder()
            .with_web_config(Self::default_web_config())
            .with_model_mapper_fn(move |model_iden: ModelIden| -> std::result::Result<ModelIden, genai::resolver::Error> {
                Ok(ModelIden::new(adapter, model_iden.model_name.clone()))
            })
            .build();

        Self {
            client,
            provider_id: provider_id.to_string(),
            adapter: provider.adapter,
            model: model.unwrap_or(&provider.default_model().id).to_string(),
            system_prompt: None,
        }
    }

    /// Create a provider with a specific API key
    pub fn with_api_key(provider_id: &str, api_key: &str, model: Option<&str>) -> Self {
        let provider = catalog::get(provider_id)
            .unwrap_or_else(|| panic!("Unknown provider: {}", provider_id));
        let adapter = provider.adapter;

        let api_key = api_key.to_string();
        let auth_resolver = AuthResolver::from_resolver_fn(
            move |_model_iden| -> std::result::Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(api_key)))
            },
        );

        // Use model mapper to force the correct adapter for this provider
        let client = Client::builder()
            .with_web_config(Self::default_web_config())
            .with_auth_resolver(auth_resolver)
            .with_model_mapper_fn(move |model_iden: ModelIden| -> std::result::Result<ModelIden, genai::resolver::Error> {
                Ok(ModelIden::new(adapter, model_iden.model_name.clone()))
            })
            .build();

        Self {
            client,
            provider_id: provider_id.to_string(),
            adapter: provider.adapter,
            model: model.unwrap_or(&provider.default_model().id).to_string(),
            system_prompt: None,
        }
    }

    /// Create a provider with API key and optional custom base URL
    ///
    /// The `base_url` should be the API endpoint prefix. For example:
    /// - Anthropic: `https://api.anthropic.com/v1/` (genai appends `messages`)
    /// - OpenAI: `https://api.openai.com/v1/` (genai appends `chat/completions`)
    ///
    /// If `base_url` is None, uses the default from the catalog.
    pub fn with_config(
        provider_id: &str,
        api_key: &str,
        model: Option<&str>,
        base_url: Option<&str>,
    ) -> Self {
        let provider = catalog::get(provider_id)
            .unwrap_or_else(|| panic!("Unknown provider: {}", provider_id));
        let adapter = provider.adapter;

        let api_key_owned = api_key.to_string();
        let auth_resolver = AuthResolver::from_resolver_fn(
            move |_model_iden| -> std::result::Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(api_key_owned)))
            },
        );

        // Use provided base_url or fall back to catalog default
        let effective_url = base_url.unwrap_or(&provider.base_url);
        let endpoint = Endpoint::from_owned(effective_url.to_string());

        // Use model mapper to force the correct adapter for this provider
        let client = Client::builder()
            .with_web_config(Self::default_web_config())
            .with_auth_resolver(auth_resolver)
            .with_model_mapper_fn(move |model_iden: ModelIden| -> std::result::Result<ModelIden, genai::resolver::Error> {
                Ok(ModelIden::new(adapter, model_iden.model_name.clone()))
            })
            .with_service_target_resolver_fn(move |mut target: ServiceTarget| {
                target.endpoint = endpoint;
                Ok(target)
            })
            .build();

        Self {
            client,
            provider_id: provider_id.to_string(),
            adapter: provider.adapter,
            model: model.unwrap_or(&provider.default_model().id).to_string(),
            system_prompt: None,
        }
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Get the provider ID (e.g., "anthropic", "together")
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Get the genai adapter kind
    pub fn adapter(&self) -> AdapterKind {
        self.adapter
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Execute a chat completion and return either a message or tool calls
    pub async fn chat(
        &self,
        messages: Vec<super::LlmMessage>,
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

        // Convert messages to ChatRequest
        for msg in messages {
            match msg {
                super::LlmMessage::Chat(chat_msg) => {
                    chat_req = chat_req.append_message(chat_msg);
                }
                super::LlmMessage::ToolResult(tool_response) => {
                    chat_req = chat_req.append_message(tool_response);
                }
                super::LlmMessage::AssistantToolCalls { content, tool_calls } => {
                    // First add text content if present
                    if let Some(text) = content {
                        if !text.is_empty() {
                            chat_req = chat_req.append_message(ChatMessage::assistant(&text));
                        }
                    }
                    // Then add tool calls as a separate message
                    if !tool_calls.is_empty() {
                        chat_req = chat_req.append_message(tool_calls);
                    }
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

        // Log request size for debugging truncation issues
        let request_size_estimate: usize = messages_for_log.iter()
            .map(|m| m.content_as_text().len())
            .sum();
        debug!(
            model = %self.model,
            message_count = messages_for_log.len(),
            request_size_chars = request_size_estimate,
            tool_count = tools_for_log.as_ref().map(|t| t.len()).unwrap_or(0),
            "Sending LLM request"
        );

        // Configure chat options with max_tokens from catalog
        // Different models have different limits (4K-32K), so use the catalog value
        let max_output = get_model_max_output(&self.provider_id, &self.model).unwrap_or(8192);
        let chat_options = ChatOptions::default().with_max_tokens(max_output as u32);

        // Retry configuration
        let retry_config = RetryConfig::default();
        let mut empty_retries = 0u32;
        let mut rate_limit_retries = 0u32;
        let mut json_error_retries = 0u32;

        // Execute with retry logic
        // Note: The client's model_mapper will ensure the correct adapter is used
        loop {
            let chat_res = self
                .client
                .exec_chat(&self.model, chat_req.clone(), Some(&chat_options))
                .await;

            match chat_res {
                Ok(response) => {
                    // Extract token usage BEFORE consuming response
                    let input_tokens = response.usage.prompt_tokens.map(|t| t as u64);
                    let output_tokens = response.usage.completion_tokens.map(|t| t as u64);

                    // Extract content
                    let content = response.first_text().map(|s| s.to_string());

                    // Check for empty response - retry if configured
                    let is_empty = content.as_ref().map(|c| c.trim().is_empty()).unwrap_or(true);
                    let has_tool_calls = !response.tool_calls().is_empty();

                    if is_empty && !has_tool_calls && empty_retries < retry_config.max_retries {
                        empty_retries += 1;
                        warn!(
                            model = %self.model,
                            retry = empty_retries,
                            delay_secs = retry_config.empty_response_delay.as_secs(),
                            "Empty response received, retrying"
                        );
                        tokio::time::sleep(retry_config.empty_response_delay).await;
                        continue;
                    }

                    // Extract tool calls
                    let tool_calls: Vec<ToolCall> = response
                        .into_tool_calls()
                        .into_iter()
                        .filter(|tc| {
                            if tc.fn_name.is_empty() {
                                warn!("Received tool call with empty name, skipping");
                                return false;
                            }
                            debug!(
                                tool_name = %tc.fn_name,
                                call_id = %tc.call_id,
                                arguments = ?tc.fn_arguments,
                                "Received tool call"
                            );
                            true
                        })
                        .collect();

                    let result = CompletionResult {
                        content,
                        tool_calls,
                        input_tokens,
                        output_tokens,
                    };

                    // Log successful interaction (no raw HTTP body available from genai)
                    log_llm_interaction(LogConfig {
                        model: &self.model,
                        provider: Some("genai"),
                        system_prompt: self.system_prompt.as_deref(),
                        messages: &messages_for_log,
                        tools: tools_for_log.as_deref(),
                        result: Some(&result),
                        ..Default::default()
                    });

                    return Ok(result);
                }
                Err(e) => {
                    // Check for rate limit error - retry if configured
                    if is_rate_limit_error(&e) && rate_limit_retries < retry_config.max_retries {
                        rate_limit_retries += 1;
                        warn!(
                            model = %self.model,
                            retry = rate_limit_retries,
                            delay_secs = retry_config.rate_limit_delay.as_secs(),
                            "Rate limit hit, retrying"
                        );
                        tokio::time::sleep(retry_config.rate_limit_delay).await;
                        continue;
                    }

                    // Check for JSON parse error - retry if configured
                    // This can happen when provider returns truncated/malformed response
                    if is_json_parse_error(&e) && json_error_retries < retry_config.max_retries {
                        json_error_retries += 1;
                        warn!(
                            model = %self.model,
                            retry = json_error_retries,
                            delay_secs = retry_config.json_error_delay.as_secs(),
                            error = %e,
                            "JSON parse error, retrying"
                        );
                        tokio::time::sleep(retry_config.json_error_delay).await;
                        continue;
                    }

                    // No retry - extract detailed error information
                    let (error_details, raw_body) = extract_genai_error_details(&e);
                    let error_msg = format!("GenAI error: {}", error_details);

                    // Log failed interaction with request context and raw response
                    log_llm_interaction(LogConfig {
                        model: &self.model,
                        provider: Some("genai"),
                        system_prompt: self.system_prompt.as_deref(),
                        messages: &messages_for_log,
                        tools: tools_for_log.as_deref(),
                        error: Some(&error_msg),
                        raw_response: raw_body.as_deref(),
                        ..Default::default()
                    });

                    // Log with tracing for stack context
                    if let Some(body) = &raw_body {
                        error!(
                            error = %error_details,
                            model = %self.model,
                            message_count = messages_for_log.len(),
                            request_size_chars = request_size_estimate,
                            raw_body_len = body.len(),
                            raw_body_preview = %if body.len() > 1000 { &body[..1000] } else { body },
                            "LLM request failed with raw response"
                        );
                    } else {
                        error!(
                            error = %error_details,
                            model = %self.model,
                            message_count = messages_for_log.len(),
                            request_size_chars = request_size_estimate,
                            "LLM request failed"
                        );
                    }

                    return Err(Error::Provider(error_msg));
                }
            }
        }
    }

    /// Continue a conversation after tool execution
    /// Takes the original request, the tool calls that were made, and the results
    pub async fn continue_with_tool_results(
        &self,
        mut chat_req: ChatRequest,
        tool_calls: Vec<ToolCall>,
        results: Vec<(String, String)>, // (call_id, result)
    ) -> Result<CompletionResult> {
        // Add tool calls as assistant message
        chat_req = chat_req.append_message(tool_calls);

        // Add tool results
        for (call_id, result) in results {
            let tool_response = ToolResponse::new(call_id, result);
            chat_req = chat_req.append_message(tool_response);
        }

        // Configure chat options with max_tokens from catalog
        let max_output = get_model_max_output(&self.provider_id, &self.model).unwrap_or(8192);
        let chat_options = ChatOptions::default().with_max_tokens(max_output as u32);

        // Execute the chat again (non-streaming)
        // Note: The client's model_mapper will ensure the correct adapter is used
        let response = self
            .client
            .exec_chat(&self.model, chat_req, Some(&chat_options))
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, model = %self.model, "LLM continuation request failed");
                Error::Provider(format!("GenAI error: {:?}", e))
            })?;

        // Extract content
        let content = response.first_text().map(|s| s.to_string());

        // Extract tool calls
        let tool_calls: Vec<ToolCall> = response
            .into_tool_calls()
            .into_iter()
            .filter(|tc| {
                if tc.fn_name.is_empty() {
                    warn!("Received tool call with empty name, skipping");
                    return false;
                }
                debug!(
                    tool_name = %tc.fn_name,
                    call_id = %tc.call_id,
                    arguments = ?tc.fn_arguments,
                    "Received tool call (continuation)"
                );
                true
            })
            .collect();

        Ok(CompletionResult {
            content,
            tool_calls,
            input_tokens: None,
            output_tokens: None,
        })
    }

}

// Implement LlmProvider trait for compatibility with existing code
impl super::LlmProvider for GenAIProvider {
    fn name(&self) -> &str {
        &self.provider_id
    }

    async fn health_check(&self) -> Result<bool> {
        let messages = vec![super::LlmMessage::user("Hi")];

        match self.chat(messages, None).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Create a provider from configuration
pub fn create_provider(
    provider_id: &str,
    api_key: Option<&str>,
    model: Option<&str>,
    system_prompt: Option<&str>,
) -> Result<GenAIProvider> {
    let provider = if let Some(key) = api_key {
        GenAIProvider::with_api_key(provider_id, key, model)
    } else {
        GenAIProvider::new(provider_id, model)
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

