//! Rig-based LLM provider implementation
//!
//! Uses rig-core for LLM API calls with proper JSON parsing and streaming support.
//! This provider has the same interface as GenAIProvider but uses rig under the hood.
//!
//! ## LLM Request/Response Logging
//!
//! Set the `LLM_LOG_FILE` environment variable to enable detailed logging of all
//! LLM requests and responses to a JSON file. Includes system prompt in requests.
//!
//! Example: `LLM_LOG_FILE=/tmp/llm.log cowork`

use futures::{Stream, StreamExt};
use rig::prelude::*;
use rig::completion::{CompletionRequestBuilder, ToolDefinition as RigToolDef};
use rig::message::{AssistantContent, Message, Text, ToolCall as RigToolCall, ToolFunction, ToolResult, ToolResultContent, UserContent};
use rig::streaming::StreamedAssistantContent;
use std::pin::Pin;
use tracing::{debug, info, warn};

use crate::error::{Error, Result};
use crate::tools::ToolDefinition;
use super::{ContentBlock, LlmMessage, MessageContent};
use super::genai_provider::{CompletionResult, PendingToolCall, ProviderType};
use super::logging::{log_llm_interaction, LogConfig};
use super::model_listing::get_model_max_output;

/// Event emitted during streaming completion
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text chunk/token from the assistant
    TextDelta(String),
    /// Tool call is complete (with id, name, and arguments)
    ToolCall(PendingToolCall),
    /// Reasoning content (for models that support it)
    Reasoning(String),
    /// Stream has completed with final result
    Done(CompletionResult),
    /// Error occurred during streaming
    Error(String),
}

/// Type alias for a boxed stream of StreamEvents
pub type StreamEventStream = Pin<Box<dyn Stream<Item = StreamEvent> + Send>>;

/// Rig-based LLM provider
///
/// Uses rig-core for API calls, providing better JSON parsing reliability
/// than genai, especially for streaming responses.
pub struct RigProvider {
    provider_type: ProviderType,
    model: String,
    system_prompt: Option<String>,
    api_key: Option<String>,
}

impl RigProvider {
    /// Create a new provider with default settings (uses environment variables for auth)
    pub fn new(provider_type: ProviderType, model: Option<&str>) -> Self {
        Self {
            provider_type,
            model: model.unwrap_or(provider_type.default_model()).to_string(),
            system_prompt: None,
            api_key: None,
        }
    }

    /// Create a provider with a specific API key
    pub fn with_api_key(provider_type: ProviderType, api_key: &str, model: Option<&str>) -> Self {
        Self {
            provider_type,
            model: model.unwrap_or(provider_type.default_model()).to_string(),
            system_prompt: None,
            api_key: Some(api_key.to_string()),
        }
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

    /// Get the system prompt
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Log a streaming interaction after the stream has completed
    ///
    /// This should be called by the agent loop after consuming the stream
    /// to ensure streaming interactions are logged just like non-streaming ones.
    pub fn log_streaming_interaction(
        &self,
        messages: &[LlmMessage],
        tools: Option<&[ToolDefinition]>,
        result: Option<&CompletionResult>,
        error: Option<&str>,
    ) {
        log_llm_interaction(LogConfig {
            model: &self.model,
            provider: Some("rig"),
            system_prompt: self.system_prompt.as_deref(),
            messages,
            tools,
            result,
            error,
            ..Default::default()
        });
    }

    /// Execute a chat completion and return either a message or tool calls
    pub async fn chat(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResult> {
        match self.provider_type {
            ProviderType::DeepSeek => self.chat_deepseek(messages, tools).await,
            ProviderType::OpenAI => self.chat_openai(messages, tools).await,
            ProviderType::Anthropic => self.chat_anthropic(messages, tools).await,
            _ => Err(Error::Provider(format!(
                "Provider {:?} not yet supported by rig provider",
                self.provider_type
            ))),
        }
    }

    /// Execute a streaming chat completion
    ///
    /// Returns a stream of `StreamEvent` that yields text deltas as they arrive,
    /// followed by any tool calls, and finally a `Done` event with the complete result.
    pub async fn chat_stream(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<StreamEventStream> {
        match self.provider_type {
            ProviderType::DeepSeek => self.stream_deepseek(messages, tools).await,
            ProviderType::OpenAI => self.stream_openai(messages, tools).await,
            ProviderType::Anthropic => self.stream_anthropic(messages, tools).await,
            _ => Err(Error::Provider(format!(
                "Provider {:?} not yet supported by rig provider for streaming",
                self.provider_type
            ))),
        }
    }

    /// Chat with DeepSeek
    async fn chat_deepseek(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResult> {
        use rig::providers::deepseek;

        let client = if let Some(ref api_key) = self.api_key {
            deepseek::Client::new(api_key)
                .map_err(|e| Error::Provider(format!("Failed to create DeepSeek client: {}", e)))?
        } else {
            if std::env::var("DEEPSEEK_API_KEY").is_err() {
                return Err(Error::Provider("DEEPSEEK_API_KEY not set".to_string()));
            }
            deepseek::Client::from_env()
        };

        let model = client.completion_model(&self.model);
        self.execute_completion(model, messages, tools).await
    }

    /// Chat with OpenAI
    async fn chat_openai(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResult> {
        use rig::providers::openai;

        let client = if let Some(ref api_key) = self.api_key {
            openai::Client::new(api_key)
                .map_err(|e| Error::Provider(format!("Failed to create OpenAI client: {}", e)))?
        } else {
            if std::env::var("OPENAI_API_KEY").is_err() {
                return Err(Error::Provider("OPENAI_API_KEY not set".to_string()));
            }
            openai::Client::from_env()
        };

        let model = client.completion_model(&self.model);
        self.execute_completion(model, messages, tools).await
    }

    /// Chat with Anthropic
    async fn chat_anthropic(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResult> {
        use rig::providers::anthropic;

        let client = if let Some(ref api_key) = self.api_key {
            anthropic::Client::new(api_key)
                .map_err(|e| Error::Provider(format!("Failed to create Anthropic client: {}", e)))?
        } else {
            if std::env::var("ANTHROPIC_API_KEY").is_err() {
                return Err(Error::Provider("ANTHROPIC_API_KEY not set".to_string()));
            }
            anthropic::Client::from_env()
        };

        let model = client.completion_model(&self.model);
        self.execute_completion(model, messages, tools).await
    }

    /// Stream with DeepSeek
    async fn stream_deepseek(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<StreamEventStream> {
        use rig::providers::deepseek;

        let client = if let Some(ref api_key) = self.api_key {
            deepseek::Client::new(api_key)
                .map_err(|e| Error::Provider(format!("Failed to create DeepSeek client: {}", e)))?
        } else {
            if std::env::var("DEEPSEEK_API_KEY").is_err() {
                return Err(Error::Provider("DEEPSEEK_API_KEY not set".to_string()));
            }
            deepseek::Client::from_env()
        };

        let model = client.completion_model(&self.model);
        self.execute_stream(model, messages, tools).await
    }

    /// Stream with OpenAI
    async fn stream_openai(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<StreamEventStream> {
        use rig::providers::openai;

        let client = if let Some(ref api_key) = self.api_key {
            openai::Client::new(api_key)
                .map_err(|e| Error::Provider(format!("Failed to create OpenAI client: {}", e)))?
        } else {
            if std::env::var("OPENAI_API_KEY").is_err() {
                return Err(Error::Provider("OPENAI_API_KEY not set".to_string()));
            }
            openai::Client::from_env()
        };

        let model = client.completion_model(&self.model);
        self.execute_stream(model, messages, tools).await
    }

    /// Stream with Anthropic
    async fn stream_anthropic(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<StreamEventStream> {
        use rig::providers::anthropic;

        let client = if let Some(ref api_key) = self.api_key {
            anthropic::Client::new(api_key)
                .map_err(|e| Error::Provider(format!("Failed to create Anthropic client: {}", e)))?
        } else {
            if std::env::var("ANTHROPIC_API_KEY").is_err() {
                return Err(Error::Provider("ANTHROPIC_API_KEY not set".to_string()));
            }
            anthropic::Client::from_env()
        };

        let model = client.completion_model(&self.model);
        self.execute_stream(model, messages, tools).await
    }

    /// Execute streaming completion with a rig model
    async fn execute_stream<M>(
        &self,
        model: M,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<StreamEventStream>
    where
        M: rig::completion::CompletionModel,
        M::StreamingResponse: 'static,
    {
        // Convert messages to rig format
        let rig_messages = self.convert_messages(messages)?;

        // The last message should be the prompt, previous messages are history
        if rig_messages.is_empty() {
            return Err(Error::Provider("No messages provided".to_string()));
        }

        let (history, prompt) = if rig_messages.len() == 1 {
            (Vec::new(), rig_messages.into_iter().next().unwrap())
        } else {
            let mut msgs = rig_messages;
            let prompt = msgs.pop().unwrap();
            (msgs, prompt)
        };

        // Build the request
        let mut builder: CompletionRequestBuilder<M> = model.completion_request(prompt);

        if let Some(ref system) = self.system_prompt {
            builder = builder.preamble(system.clone());
        }

        // Use max_output from catalog, default to 8192 if not found
        let max_output = get_model_max_output(self.provider_type, &self.model).unwrap_or(8192) as u64;
        builder = builder.max_tokens(max_output);

        for msg in history {
            builder = builder.message(msg);
        }

        if let Some(tool_defs) = tools {
            for tool in tool_defs {
                let rig_tool = RigToolDef {
                    name: tool.name,
                    description: tool.description,
                    parameters: tool.parameters,
                };
                builder = builder.tool(rig_tool);
            }
        }

        // Execute streaming request
        let request = builder.build();
        let stream_response = model.stream(request).await
            .map_err(|e| Error::Provider(format!("Stream error: {}", e)))?;

        // Transform rig's streaming response into our StreamEvent stream
        // Use shared counters to track event types for debugging
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let text_count = Arc::new(AtomicUsize::new(0));
        let tool_call_count = Arc::new(AtomicUsize::new(0));
        let tool_delta_count = Arc::new(AtomicUsize::new(0));
        let reasoning_count = Arc::new(AtomicUsize::new(0));
        let final_count = Arc::new(AtomicUsize::new(0));

        let tc_clone = tool_call_count.clone();
        let td_clone = tool_delta_count.clone();
        let txt_clone = text_count.clone();
        let r_clone = reasoning_count.clone();
        let f_clone = final_count.clone();

        let event_stream = stream_response.map(move |result| {
            match result {
                Ok(content) => match content {
                    StreamedAssistantContent::Text(text) => {
                        txt_clone.fetch_add(1, Ordering::Relaxed);
                        StreamEvent::TextDelta(text.text)
                    }
                    StreamedAssistantContent::ToolCall(tc) => {
                        let count = tc_clone.fetch_add(1, Ordering::Relaxed) + 1;
                        info!(
                            tool_name = %tc.function.name,
                            tool_id = %tc.id,
                            count = count,
                            "STREAM: Received complete tool call"
                        );
                        StreamEvent::ToolCall(PendingToolCall {
                            call_id: tc.id,
                            name: tc.function.name,
                            arguments: tc.function.arguments,
                        })
                    }
                    StreamedAssistantContent::ToolCallDelta { id, content } => {
                        let count = td_clone.fetch_add(1, Ordering::Relaxed) + 1;
                        debug!(tool_id = %id, delta_count = count, content = ?content, "STREAM: Tool call delta");
                        // Ignore deltas - we'll get the full tool call when content_block_stop arrives
                        StreamEvent::TextDelta(String::new())
                    }
                    StreamedAssistantContent::Reasoning(reasoning) => {
                        r_clone.fetch_add(1, Ordering::Relaxed);
                        StreamEvent::Reasoning(reasoning.reasoning.join(""))
                    }
                    StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                        r_clone.fetch_add(1, Ordering::Relaxed);
                        StreamEvent::Reasoning(reasoning)
                    }
                    StreamedAssistantContent::Final(_) => {
                        f_clone.fetch_add(1, Ordering::Relaxed);
                        debug!("STREAM: Received Final event");
                        // Final response - we'll construct Done event from accumulated state
                        StreamEvent::TextDelta(String::new())
                    }
                },
                Err(e) => {
                    warn!(error = %e, "STREAM: Error in streaming response");
                    StreamEvent::Error(e.to_string())
                }
            }
        });

        // Log summary after stream ends (note: this logs immediately, actual counts update during stream)
        debug!(
            "Stream setup complete - counters will be populated as events arrive"
        );

        Ok(Box::pin(event_stream))
    }

    /// Execute completion with a rig model
    async fn execute_completion<M: rig::completion::CompletionModel>(
        &self,
        model: M,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResult> {
        // Keep copies for logging
        let messages_for_log = messages.clone();
        let tools_for_log = tools.clone();

        // Convert messages to rig format
        let rig_messages = self.convert_messages(messages)?;

        // The last message should be the prompt, previous messages are history
        if rig_messages.is_empty() {
            return Err(Error::Provider("No messages provided".to_string()));
        }

        let (history, prompt) = if rig_messages.len() == 1 {
            (Vec::new(), rig_messages.into_iter().next().unwrap())
        } else {
            let mut msgs = rig_messages;
            let prompt = msgs.pop().unwrap();
            (msgs, prompt)
        };

        // Build the request starting with the prompt
        let mut builder: CompletionRequestBuilder<M> = model.completion_request(prompt);

        // Add system prompt
        if let Some(ref system) = self.system_prompt {
            builder = builder.preamble(system.clone());
        }

        // Set max tokens from catalog (default to 8192 if not found)
        let max_output = get_model_max_output(self.provider_type, &self.model).unwrap_or(8192) as u64;
        builder = builder.max_tokens(max_output);

        // Add chat history
        for msg in history {
            builder = builder.message(msg);
        }

        // Add tools if provided
        if let Some(ref tool_defs) = tools_for_log {
            for tool in tool_defs {
                let rig_tool = RigToolDef {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.clone(),
                };
                builder = builder.tool(rig_tool);
            }
        }

        // Execute the request
        let request = builder.build();
        let response = model.completion(request).await;

        match response {
            Ok(resp) => {
                // Extract content and tool calls from response
                let result = self.parse_response(resp)?;

                // For raw response, we serialize what we have (rig deserializes internally)
                let raw_response = serde_json::to_string(&serde_json::json!({
                    "content": &result.content,
                    "tool_calls": result.tool_calls.iter().map(|tc| serde_json::json!({
                        "call_id": tc.call_id,
                        "name": tc.name,
                        "arguments": tc.arguments
                    })).collect::<Vec<_>>()
                })).unwrap_or_else(|_| "serialization_error".to_string());

                // Log successful interaction
                log_llm_interaction(LogConfig {
                    model: &self.model,
                    provider: Some("rig"),
                    system_prompt: self.system_prompt.as_deref(),
                    messages: &messages_for_log,
                    tools: tools_for_log.as_deref(),
                    result: Some(&result),
                    raw_response: Some(&raw_response),
                    ..Default::default()
                });

                Ok(result)
            }
            Err(e) => {
                let error_msg = format!("Completion error: {}", e);

                // Log failed interaction
                log_llm_interaction(LogConfig {
                    model: &self.model,
                    provider: Some("rig"),
                    system_prompt: self.system_prompt.as_deref(),
                    messages: &messages_for_log,
                    tools: tools_for_log.as_deref(),
                    error: Some(&error_msg),
                    ..Default::default()
                });

                Err(Error::Provider(error_msg))
            }
        }
    }

    /// Convert our LlmMessage format to rig Message format
    fn convert_messages(&self, messages: Vec<LlmMessage>) -> Result<Vec<Message>> {
        let mut rig_messages = Vec::new();

        for msg in messages {
            match msg.role.as_str() {
                "user" => {
                    let content = self.convert_user_content(&msg)?;
                    rig_messages.push(Message::User { content });
                }
                "assistant" => {
                    let content = self.convert_assistant_content(&msg)?;
                    rig_messages.push(Message::Assistant { id: None, content });
                }
                "system" => {
                    // System messages are handled via preamble, skip
                }
                _ => {
                    // Unknown role, treat as user
                    let text = msg.content_as_text();
                    rig_messages.push(Message::User {
                        content: rig::OneOrMany::one(UserContent::Text(Text { text })),
                    });
                }
            }
        }

        Ok(rig_messages)
    }

    /// Convert user message content to rig format
    fn convert_user_content(&self, msg: &LlmMessage) -> Result<rig::OneOrMany<UserContent>> {
        match &msg.content {
            MessageContent::Text(text) => {
                Ok(rig::OneOrMany::one(UserContent::Text(Text { text: text.clone() })))
            }
            MessageContent::Blocks(blocks) => {
                let mut contents = Vec::new();
                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => {
                            contents.push(UserContent::Text(Text { text: text.clone() }));
                        }
                        ContentBlock::ToolResult { tool_use_id, content, is_error: _ } => {
                            contents.push(UserContent::ToolResult(ToolResult {
                                id: tool_use_id.clone(),
                                call_id: None,
                                content: rig::OneOrMany::one(ToolResultContent::Text(Text { text: content.clone() })),
                            }));
                        }
                        ContentBlock::ToolUse { .. } => {
                            // Tool use in user message is unusual, skip
                        }
                    }
                }
                if contents.is_empty() {
                    Ok(rig::OneOrMany::one(UserContent::Text(Text { text: String::new() })))
                } else if contents.len() == 1 {
                    Ok(rig::OneOrMany::one(contents.remove(0)))
                } else {
                    Ok(rig::OneOrMany::many(contents).unwrap())
                }
            }
        }
    }

    /// Convert assistant message content to rig format
    fn convert_assistant_content(&self, msg: &LlmMessage) -> Result<rig::OneOrMany<AssistantContent>> {
        let mut contents = Vec::new();

        // Extract text content
        let text = msg.content_as_text();
        if !text.is_empty() {
            contents.push(AssistantContent::Text(Text { text }));
        }

        // Extract tool calls
        if let Some(tool_calls) = &msg.tool_calls {
            for tc in tool_calls {
                contents.push(AssistantContent::ToolCall(RigToolCall::new(
                    tc.id.clone(),
                    ToolFunction::new(tc.name.clone(), tc.arguments.clone()),
                )));
            }
        }

        // Also check content blocks for tool calls
        if let MessageContent::Blocks(blocks) = &msg.content {
            for block in blocks {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    // Avoid duplicates if already added from tool_calls field
                    let already_added = contents.iter().any(|c| {
                        if let AssistantContent::ToolCall(tc) = c {
                            tc.id == *id
                        } else {
                            false
                        }
                    });
                    if !already_added {
                        contents.push(AssistantContent::ToolCall(RigToolCall::new(
                            id.clone(),
                            ToolFunction::new(name.clone(), input.clone()),
                        )));
                    }
                }
            }
        }

        if contents.is_empty() {
            Ok(rig::OneOrMany::one(AssistantContent::Text(Text { text: String::new() })))
        } else if contents.len() == 1 {
            Ok(rig::OneOrMany::one(contents.remove(0)))
        } else {
            Ok(rig::OneOrMany::many(contents).unwrap())
        }
    }

    /// Parse rig completion response into our CompletionResult format
    fn parse_response<R>(&self, response: rig::completion::CompletionResponse<R>) -> Result<CompletionResult> {
        let mut content = None;
        let mut tool_calls = Vec::new();

        // Log usage info
        debug!(
            usage_input = response.usage.input_tokens,
            usage_output = response.usage.output_tokens,
            choice_count = response.choice.len(),
            "Parsing rig completion response"
        );

        // CompletionResponse.choice is OneOrMany<AssistantContent>
        for ac in response.choice.iter() {
            self.extract_assistant_content(ac, &mut content, &mut tool_calls);
        }

        // Log warning if we got content but no tool calls
        if content.is_some() && tool_calls.is_empty() {
            warn!(
                content = ?content,
                input_tokens = response.usage.input_tokens,
                output_tokens = response.usage.output_tokens,
                "Response has content but no tool calls"
            );
        }

        Ok(CompletionResult { content, tool_calls })
    }

    /// Extract content and tool calls from AssistantContent
    fn extract_assistant_content(
        &self,
        ac: &AssistantContent,
        content: &mut Option<String>,
        tool_calls: &mut Vec<PendingToolCall>,
    ) {
        match ac {
            AssistantContent::Text(Text { text }) => {
                if let Some(c) = content {
                    c.push_str(text);
                } else {
                    *content = Some(text.clone());
                }
            }
            AssistantContent::ToolCall(tc) => {
                tool_calls.push(PendingToolCall {
                    call_id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                });
            }
            AssistantContent::Reasoning(_) => {
                // Reasoning content, skip for now (could log or process separately)
            }
            AssistantContent::Image(_) => {
                // Image content in assistant response, skip
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = RigProvider::new(ProviderType::DeepSeek, None);
        assert_eq!(provider.provider_type(), ProviderType::DeepSeek);
    }

    #[test]
    fn test_with_api_key() {
        let provider = RigProvider::with_api_key(ProviderType::OpenAI, "test-key", Some("gpt-4"));
        assert_eq!(provider.model(), "gpt-4");
    }
}
