//! Rig-based LLM provider implementation
//!
//! This module provides LLM integration using the rig-core crate,
//! which handles the agentic loop automatically.
//!
//! Key design:
//! - Tools implement rig's Tool trait
//! - Approval logic happens inside Tool::call
//! - Event emission (ToolStart, ToolDone) happens inside Tool::call
//! - ToolContext provides channels for communication with frontend

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::future::Future;

use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError as RigToolError, ToolSet};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::approval::ToolApprovalConfig;
use crate::formatting::{format_tool_call, format_tool_result_summary};
// Re-exported from session/mod.rs
pub use crate::session::{SessionInput, SessionOutput};
use crate::tools::Tool as CoworkTool;

// Type alias for boxed futures (rig uses this for WASM compat)
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Context passed to tools for event emission and approval
#[derive(Clone)]
pub struct ToolContext {
    /// Channel to send events to the frontend
    output_tx: mpsc::Sender<SessionOutput>,
    /// Channel to receive input from the frontend (for approvals)
    input_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<SessionInput>>>,
    /// Approval configuration
    approval_config: ToolApprovalConfig,
    /// Workspace path
    pub workspace: PathBuf,
}

impl ToolContext {
    pub fn new(
        output_tx: mpsc::Sender<SessionOutput>,
        input_rx: mpsc::Receiver<SessionInput>,
        approval_config: ToolApprovalConfig,
        workspace: PathBuf,
    ) -> Self {
        Self {
            output_tx,
            input_rx: Arc::new(tokio::sync::Mutex::new(input_rx)),
            approval_config,
            workspace,
        }
    }

    /// Emit an event to the frontend
    pub async fn emit(&self, output: SessionOutput) {
        let _ = self.output_tx.send(output).await;
    }

    /// Check if a tool should be auto-approved based on config
    pub fn should_auto_approve(&self, tool_name: &str, args: &Value) -> bool {
        self.approval_config.should_auto_approve_with_args(tool_name, args)
    }

    /// Wait for approval of a tool call
    /// Returns true if approved, false if rejected
    pub async fn wait_for_approval(&self, tool_call_id: &str) -> bool {
        let mut rx: tokio::sync::MutexGuard<'_, mpsc::Receiver<SessionInput>> =
            self.input_rx.lock().await;
        while let Some(input) = rx.recv().await {
            match input {
                SessionInput::ApproveTool { tool_call_id: id } if id == tool_call_id => {
                    return true;
                }
                SessionInput::RejectTool { tool_call_id: id, .. } if id == tool_call_id => {
                    return false;
                }
                _ => continue, // Ignore other inputs
            }
        }
        false // Channel closed = rejection
    }
}

/// Wrapper that adapts a Cowork tool to rig's ToolDyn trait
pub struct RigToolWrapper {
    /// The underlying Cowork tool
    tool: Arc<dyn CoworkTool>,
    /// Context for event emission and approval
    context: ToolContext,
    /// Cached tool name (for ToolDyn::name)
    name: String,
}

impl RigToolWrapper {
    pub fn new(tool: Arc<dyn CoworkTool>, context: ToolContext) -> Self {
        let name = tool.name().to_string();
        Self { tool, context, name }
    }
}

// Implement ToolDyn directly for dynamic dispatch
impl ToolDyn for RigToolWrapper {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn definition<'a>(&'a self, _prompt: String) -> BoxFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: self.tool.name().to_string(),
                description: self.tool.description().to_string(),
                parameters: self.tool.parameters_schema(),
            }
        })
    }

    fn call<'a>(&'a self, args_str: String) -> BoxFuture<'a, Result<String, RigToolError>> {
        Box::pin(async move {
            // Parse args from JSON string
            let args: Value = serde_json::from_str(&args_str)
                .map_err(RigToolError::JsonError)?;

            let tool_call_id = uuid::Uuid::new_v4().to_string();
            let tool_name = self.tool.name();
            let formatted = format_tool_call(tool_name, &args);

            // Check if we need approval
            let needs_approval = !self.context.should_auto_approve(tool_name, &args);

            if needs_approval {
                // Emit pending event and wait for approval
                self.context
                    .emit(SessionOutput::tool_pending(
                        &tool_call_id,
                        tool_name,
                        args.clone(),
                        Some(formatted.clone()),
                    ))
                    .await;

                if !self.context.wait_for_approval(&tool_call_id).await {
                    return Err(RigToolError::ToolCallError(
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Tool rejected by user",
                        ))
                    ));
                }
            }

            // Emit tool start
            self.context
                .emit(SessionOutput::tool_start(&tool_call_id, tool_name, args.clone()))
                .await;

            // Clone args for later use (execute takes ownership)
            let args_for_summary = args.clone();

            self.context
                .emit(SessionOutput::tool_call(
                    &tool_call_id,
                    tool_name,
                    args.clone(),
                    formatted,
                ))
                .await;

            // Execute the tool
            let result = self.tool.execute(args).await;

            match result {
                Ok(output) => {
                    let output_str = output.content.to_string();
                    let (summary, diff) = format_tool_result_summary(
                        tool_name,
                        output.success,
                        &output_str,
                        &args_for_summary,
                    );

                    // Emit tool done and result
                    self.context
                        .emit(SessionOutput::tool_done(
                            &tool_call_id,
                            tool_name,
                            output.success,
                            &output_str,
                        ))
                        .await;

                    self.context
                        .emit(SessionOutput::tool_result(
                            &tool_call_id,
                            tool_name,
                            output.success,
                            &output_str,
                            summary,
                            diff,
                        ))
                        .await;

                    Ok(output_str)
                }
                Err(e) => {
                    let error_msg = format!("Error: {}", e);

                    self.context
                        .emit(SessionOutput::tool_done(&tool_call_id, tool_name, false, &error_msg))
                        .await;

                    Err(RigToolError::ToolCallError(Box::new(e)))
                }
            }
        })
    }
}

/// Create a ToolSet from our Cowork tools
pub fn create_toolset(tools: Vec<Arc<dyn CoworkTool>>, context: ToolContext) -> ToolSet {
    let wrapped: Vec<Box<dyn ToolDyn>> = tools
        .into_iter()
        .map(|tool| {
            Box::new(RigToolWrapper::new(tool, context.clone())) as Box<dyn ToolDyn>
        })
        .collect();

    ToolSet::from_tools_boxed(wrapped)
}

/// Provider type for rig
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigProviderType {
    DeepSeek,
    OpenAI,
    Anthropic,
}

impl RigProviderType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "deepseek" => Some(Self::DeepSeek),
            "openai" => Some(Self::OpenAI),
            "anthropic" => Some(Self::Anthropic),
            _ => None,
        }
    }
}

/// Create wrapped tools as Vec<Box<dyn ToolDyn>> for use with rig's agent builder
pub fn create_wrapped_tools(
    tools: Vec<Arc<dyn CoworkTool>>,
    context: ToolContext,
) -> Vec<Box<dyn ToolDyn>> {
    tools
        .into_iter()
        .map(|tool| {
            Box::new(RigToolWrapper::new(tool, context.clone())) as Box<dyn ToolDyn>
        })
        .collect()
}

/// Configuration for the rig agent
pub struct RigAgentConfig {
    pub provider: RigProviderType,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub max_iterations: usize,
}

impl Default for RigAgentConfig {
    fn default() -> Self {
        Self {
            provider: RigProviderType::DeepSeek,
            api_key: None,
            model: None,
            system_prompt: None,
            max_iterations: 100,
        }
    }
}

/// Run a single prompt through the rig agent and return the result
///
/// This function creates an agent with the given tools, sends the prompt,
/// and handles the multi-turn agentic loop via rig's `.multi_turn()`.
///
/// Tool approval and event emission are handled inside `RigToolWrapper::call()`.
pub async fn run_rig_agent(
    config: RigAgentConfig,
    tools: Vec<Arc<dyn CoworkTool>>,
    context: ToolContext,
    prompt: &str,
) -> Result<String, RigAgentError> {
    // Wrap Cowork tools for rig
    let wrapped_tools = create_wrapped_tools(tools, context);

    match config.provider {
        RigProviderType::DeepSeek => {
            run_deepseek_agent(config, wrapped_tools, prompt).await
        }
        RigProviderType::OpenAI => {
            run_openai_agent(config, wrapped_tools, prompt).await
        }
        RigProviderType::Anthropic => {
            run_anthropic_agent(config, wrapped_tools, prompt).await
        }
    }
}

/// Errors that can occur when running a rig agent
#[derive(Debug, thiserror::Error)]
pub enum RigAgentError {
    #[error("API key not provided and not found in environment")]
    MissingApiKey,
    #[error("Completion error: {0}")]
    CompletionError(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

async fn run_deepseek_agent(
    config: RigAgentConfig,
    wrapped_tools: Vec<Box<dyn ToolDyn>>,
    prompt: &str,
) -> Result<String, RigAgentError> {
    use rig::prelude::*;
    use rig::completion::Prompt;
    use rig::providers::deepseek;

    // Create DeepSeek client - from_env() panics if not set, so check first
    let client = if let Some(ref api_key) = config.api_key {
        deepseek::Client::new(api_key)
            .map_err(|e| RigAgentError::ProviderError(e.to_string()))?
    } else {
        if std::env::var("DEEPSEEK_API_KEY").is_err() {
            return Err(RigAgentError::MissingApiKey);
        }
        deepseek::Client::from_env()
    };

    // Get model (default to deepseek-chat)
    let model_name = config.model.as_deref().unwrap_or(deepseek::DEEPSEEK_CHAT);

    // Build agent using client.agent() pattern (simpler API)
    let agent_builder = client
        .agent(model_name)
        .max_tokens(8192); // DeepSeek max output

    let agent_builder = if let Some(ref system_prompt) = config.system_prompt {
        agent_builder.preamble(system_prompt)
    } else {
        agent_builder
    };

    // Add tools if we have any and run
    if wrapped_tools.is_empty() {
        let agent = agent_builder.build();
        let result = agent
            .prompt(prompt)
            .multi_turn(config.max_iterations)
            .await
            .map_err(|e| RigAgentError::CompletionError(e.to_string()))?;
        Ok(result)
    } else {
        let agent = agent_builder.tools(wrapped_tools).build();
        let result = agent
            .prompt(prompt)
            .multi_turn(config.max_iterations)
            .await
            .map_err(|e| RigAgentError::CompletionError(e.to_string()))?;
        Ok(result)
    }
}

async fn run_openai_agent(
    config: RigAgentConfig,
    wrapped_tools: Vec<Box<dyn ToolDyn>>,
    prompt: &str,
) -> Result<String, RigAgentError> {
    use rig::prelude::*;
    use rig::completion::Prompt;
    use rig::providers::openai;

    // Create OpenAI client
    let client = if let Some(ref api_key) = config.api_key {
        openai::Client::new(api_key)
            .map_err(|e| RigAgentError::ProviderError(e.to_string()))?
    } else {
        if std::env::var("OPENAI_API_KEY").is_err() {
            return Err(RigAgentError::MissingApiKey);
        }
        openai::Client::from_env()
    };

    // Get model (default to gpt-4o)
    let model_name = config.model.as_deref().unwrap_or(openai::GPT_4O);

    // Build agent using client.agent() pattern
    let agent_builder = client
        .agent(model_name)
        .max_tokens(4096);

    let agent_builder = if let Some(ref system_prompt) = config.system_prompt {
        agent_builder.preamble(system_prompt)
    } else {
        agent_builder
    };

    if wrapped_tools.is_empty() {
        let agent = agent_builder.build();
        let result = agent
            .prompt(prompt)
            .multi_turn(config.max_iterations)
            .await
            .map_err(|e| RigAgentError::CompletionError(e.to_string()))?;
        Ok(result)
    } else {
        let agent = agent_builder.tools(wrapped_tools).build();
        let result = agent
            .prompt(prompt)
            .multi_turn(config.max_iterations)
            .await
            .map_err(|e| RigAgentError::CompletionError(e.to_string()))?;
        Ok(result)
    }
}

async fn run_anthropic_agent(
    config: RigAgentConfig,
    wrapped_tools: Vec<Box<dyn ToolDyn>>,
    prompt: &str,
) -> Result<String, RigAgentError> {
    use rig::prelude::*;
    use rig::completion::Prompt;
    use rig::providers::anthropic;

    // Create Anthropic client
    let client = if let Some(ref api_key) = config.api_key {
        anthropic::Client::new(api_key)
            .map_err(|e| RigAgentError::ProviderError(e.to_string()))?
    } else {
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            return Err(RigAgentError::MissingApiKey);
        }
        anthropic::Client::from_env()
    };

    // Get model (default to claude-3-5-sonnet)
    let model_name = config
        .model
        .as_deref()
        .unwrap_or(anthropic::completion::CLAUDE_3_5_SONNET);

    // Build agent using client.agent() pattern
    let agent_builder = client
        .agent(model_name)
        .max_tokens(8192);

    let agent_builder = if let Some(ref system_prompt) = config.system_prompt {
        agent_builder.preamble(system_prompt)
    } else {
        agent_builder
    };

    if wrapped_tools.is_empty() {
        let agent = agent_builder.build();
        let result = agent
            .prompt(prompt)
            .multi_turn(config.max_iterations)
            .await
            .map_err(|e| RigAgentError::CompletionError(e.to_string()))?;
        Ok(result)
    } else {
        let agent = agent_builder.tools(wrapped_tools).build();
        let result = agent
            .prompt(prompt)
            .multi_turn(config.max_iterations)
            .await
            .map_err(|e| RigAgentError::CompletionError(e.to_string()))?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_from_str() {
        assert_eq!(
            RigProviderType::from_str("deepseek"),
            Some(RigProviderType::DeepSeek)
        );
        assert_eq!(
            RigProviderType::from_str("DEEPSEEK"),
            Some(RigProviderType::DeepSeek)
        );
        assert_eq!(
            RigProviderType::from_str("openai"),
            Some(RigProviderType::OpenAI)
        );
        assert_eq!(RigProviderType::from_str("unknown"), None);
    }
}
