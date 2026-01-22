//! Agent Loop - Unified execution loop for CLI and UI
//!
//! The agent loop handles:
//! - Receiving user input and tool approvals
//! - Calling the LLM provider
//! - Executing tools based on approval config
//! - Emitting outputs for display
//! - Automatic context window management
//! - Saving session state on close

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::types::{SessionConfig, SessionId, SessionInput, SessionOutput};
use crate::approval::ToolApprovalConfig;
use crate::context::{
    CompactConfig, ContextMonitor, ConversationSummarizer, Message, MessageRole, SummarizerConfig,
};
use crate::error::Result;
use crate::orchestration::{ChatMessage, ChatSession, ToolCallInfo, ToolRegistryBuilder};
use crate::prompt::{ComponentRegistry, HookContext, HookEvent, HookExecutor, HooksConfig};
use crate::provider::{CompletionResult, GenAIProvider};
use crate::tools::{ToolDefinition, ToolRegistry};

/// Maximum number of agentic turns per user message
const MAX_ITERATIONS: usize = 100;

/// Maximum size for a single tool result in characters
/// This prevents a single tool output from exceeding the context limit
/// ~30k chars ≈ ~10k tokens, leaving room for conversation history
const MAX_TOOL_RESULT_SIZE: usize = 30_000;

/// Result from an LLM call
struct LlmCallResult {
    content: Option<String>,
    tool_calls: Vec<LlmToolCall>,
}

/// Tool call from the LLM
struct LlmToolCall {
    id: String,
    name: String,
    arguments: serde_json::Value,
}

/// Saved message for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<SavedToolCall>,
}

/// Saved tool call for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Saved session state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub id: String,
    pub name: String,
    pub messages: Vec<SavedMessage>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// The unified agent loop
pub struct AgentLoop {
    /// Session identifier
    session_id: SessionId,
    /// Input receiver
    input_rx: mpsc::Receiver<SessionInput>,
    /// Output sender
    output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
    /// LLM provider
    provider: GenAIProvider,
    /// Chat session with message history
    session: ChatSession,
    /// Tool registry
    tool_registry: ToolRegistry,
    /// Tool definitions for LLM
    tool_definitions: Vec<ToolDefinition>,
    /// Tool approval configuration
    approval_config: ToolApprovalConfig,
    /// Workspace path
    #[allow(dead_code)]
    workspace_path: std::path::PathBuf,
    /// Pending tool calls awaiting approval
    pending_approvals: Vec<ToolCallInfo>,
    /// Context monitor for tracking token usage
    context_monitor: ContextMonitor,
    /// Conversation summarizer for auto-compaction
    summarizer: ConversationSummarizer,
    /// Hook executor for running hooks at lifecycle points
    hook_executor: HookExecutor,
    /// Hooks configuration
    hooks_config: HooksConfig,
    /// Whether hooks are enabled
    hooks_enabled: bool,
    /// Component registry for agents, commands, skills
    #[allow(dead_code)]
    component_registry: Option<Arc<ComponentRegistry>>,
    /// When the session was created
    created_at: chrono::DateTime<chrono::Utc>,
}

impl AgentLoop {
    /// Create a new agent loop
    pub async fn new(
        session_id: SessionId,
        input_rx: mpsc::Receiver<SessionInput>,
        output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
        config: SessionConfig,
    ) -> Result<Self> {
        // Create the provider
        let provider = match &config.api_key {
            Some(key) => GenAIProvider::with_api_key(
                config.provider_type,
                key,
                config.model.as_deref(),
            ),
            None => GenAIProvider::new(config.provider_type, config.model.as_deref()),
        };

        // Add system prompt if provided
        let provider = match &config.system_prompt {
            Some(prompt) => provider.with_system_prompt(prompt),
            None => provider,
        };

        // Create chat session
        let session = match &config.system_prompt {
            Some(prompt) => ChatSession::with_system_prompt(prompt),
            None => ChatSession::new(),
        };

        // Create tool registry
        let mut tool_builder = ToolRegistryBuilder::new(config.workspace_path.clone())
            .with_provider(config.provider_type);

        // Add web search config if available
        if let Some(ws_config) = config.web_search_config.clone() {
            tool_builder = tool_builder.with_web_search_config(ws_config);
        }

        let tool_registry = tool_builder.build();

        let tool_definitions = tool_registry.list();

        // Initialize context monitor with provider and model for accurate limits
        let context_monitor = match &config.model {
            Some(model) => ContextMonitor::with_model(config.provider_type, model),
            None => ContextMonitor::new(config.provider_type),
        };

        // Initialize summarizer with default config
        let summarizer = ConversationSummarizer::new(SummarizerConfig::default());

        // Initialize hook executor and configuration
        let hook_executor = HookExecutor::new(config.workspace_path.clone());
        let hooks_config = config
            .component_registry
            .as_ref()
            .map(|r| r.get_hooks().clone())
            .unwrap_or_default();
        let hooks_enabled = config.prompt_config.enable_hooks;

        Ok(Self {
            session_id,
            input_rx,
            output_tx,
            provider,
            session,
            tool_registry,
            tool_definitions,
            approval_config: config.approval_config,
            workspace_path: config.workspace_path.clone(),
            pending_approvals: Vec::new(),
            context_monitor,
            summarizer,
            hook_executor,
            hooks_config,
            hooks_enabled,
            component_registry: config.component_registry.clone(),
            created_at: chrono::Utc::now(),
        })
    }

    /// Run the agent loop until Stop is received or channel closes
    pub async fn run(mut self) {
        info!("Agent loop starting for session: {}", self.session_id);

        // Execute SessionStart hooks
        if self.hooks_enabled {
            let context = HookContext::session_start(&self.session_id);
            if let Ok(Some(additional_context)) = self.execute_hooks(HookEvent::SessionStart, &context) {
                // Add hook context as a system reminder to the first message
                debug!("SessionStart hook added context: {} chars", additional_context.len());
                self.session.add_user_message(format!("<session-start-hook>\n{}\n</session-start-hook>", additional_context));
            }
        }

        loop {
            // Wait for input
            let input = match self.input_rx.recv().await {
                Some(input) => input,
                None => {
                    debug!("Input channel closed for session: {}", self.session_id);
                    break;
                }
            };

            match input {
                SessionInput::UserMessage { content } => {
                    if let Err(e) = self.handle_user_message(content).await {
                        self.emit(SessionOutput::error(e.to_string())).await;
                    }
                    // Only emit Idle if no tools are waiting for approval
                    // If tools are pending, Idle will be emitted after they're handled
                    if self.pending_approvals.is_empty() {
                        self.emit(SessionOutput::idle()).await;
                    }
                }
                SessionInput::ApproveTool { tool_call_id } => {
                    if let Err(e) = self.handle_approve_tool(&tool_call_id).await {
                        self.emit(SessionOutput::error(e.to_string())).await;
                        self.emit(SessionOutput::idle()).await;
                    }
                }
                SessionInput::RejectTool { tool_call_id, reason } => {
                    self.handle_reject_tool(&tool_call_id, reason).await;
                }
                SessionInput::AnswerQuestion { request_id, answers } => {
                    if let Err(e) = self.handle_answer_question(&request_id, answers).await {
                        self.emit(SessionOutput::error(e.to_string())).await;
                        self.emit(SessionOutput::idle()).await;
                    }
                }
            }
        }

        // Channel closed - save session before exiting
        info!("Saving session {} before exit", self.session_id);
        if let Err(e) = self.save_session().await {
            error!("Failed to save session {}: {}", self.session_id, e);
        }

        info!("Agent loop ended for session: {}", self.session_id);
    }

    /// Handle a user message - run the agentic loop
    async fn handle_user_message(&mut self, content: String) -> Result<()> {
        // Execute UserPromptSubmit hooks
        let mut content_with_hooks = content.clone();
        if self.hooks_enabled {
            match self.run_user_prompt_hook(&content) {
                Ok(Some(additional_context)) => {
                    // Append hook context to the message
                    content_with_hooks = format!("{}\n\n<user-prompt-submit-hook>\n{}\n</user-prompt-submit-hook>", content, additional_context);
                }
                Err(block_reason) => {
                    // Hook blocked the message
                    return Err(crate::error::Error::Agent(format!("Message blocked: {}", block_reason)));
                }
                Ok(None) => {}
            }
        }

        // Generate message ID
        let msg_id = uuid::Uuid::new_v4().to_string();

        // Echo the user message (original content, not with hooks)
        self.emit(SessionOutput::user_message(&msg_id, &content))
            .await;

        // Add to session (with hook context if any)
        self.session.add_user_message(&content_with_hooks);

        // Run the agentic loop
        self.run_agentic_loop().await
    }

    /// Run the agentic loop until no more tool calls
    async fn run_agentic_loop(&mut self) -> Result<()> {
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > MAX_ITERATIONS {
                return Err(crate::error::Error::Agent(
                    "Max iteration limit reached".to_string(),
                ));
            }

            // Check and compact context if needed before calling LLM
            if let Err(e) = self.check_and_compact_context().await {
                warn!("Context compaction failed: {}, continuing anyway", e);
            }

            // Call LLM
            self.emit(SessionOutput::thinking("Thinking...".to_string()))
                .await;

            let response = self.call_llm().await?;

            // Generate message ID
            let msg_id = uuid::Uuid::new_v4().to_string();

            // Emit assistant message
            let content = response.content.clone().unwrap_or_default();
            if !content.is_empty() {
                self.emit(SessionOutput::assistant_message(&msg_id, &content))
                    .await;
            }

            // Check for tool calls
            if response.tool_calls.is_empty() {
                // Add final assistant message to session history (important for multi-turn conversations)
                self.session.add_assistant_message(&content, Vec::new());
                // No tool calls, we're done
                break;
            }

            // Convert to ToolCallInfo
            let tool_calls: Vec<ToolCallInfo> = response
                .tool_calls
                .iter()
                .map(|tc| ToolCallInfo::new(&tc.id, &tc.name, tc.arguments.clone()))
                .collect();

            // Add assistant message with tool calls
            self.session.add_assistant_message(&content, tool_calls.clone());

            // Categorize tool calls
            let (auto_approved, needs_approval) = self.categorize_tools(&tool_calls);

            // Check for AskUserQuestion - it needs special handling
            let mut has_question = false;
            for tool_call in &tool_calls {
                if tool_call.name == "AskUserQuestion" {
                    // Parse questions from arguments
                    if let Some(questions) = self.parse_questions(&tool_call.arguments) {
                        self.emit(SessionOutput::Question {
                            request_id: tool_call.id.clone(),
                            questions,
                        })
                        .await;
                        self.pending_approvals.push(tool_call.clone());
                        has_question = true;
                    }
                }
            }
            if has_question {
                // Stop the loop - will continue when answers come in
                break;
            }

            // Execute auto-approved tools and batch results
            let auto_approved_tools: Vec<_> = tool_calls
                .iter()
                .filter(|tc| auto_approved.contains(&tc.id))
                .collect();

            if !auto_approved_tools.is_empty() {
                self.execute_tools_batched(&auto_approved_tools).await;
            }

            // If there are tools needing approval, pause and wait
            if !needs_approval.is_empty() {
                for tool_call in &tool_calls {
                    if needs_approval.contains(&tool_call.id) {
                        self.emit(SessionOutput::tool_pending(
                            &tool_call.id,
                            &tool_call.name,
                            tool_call.arguments.clone(),
                            None,
                        ))
                        .await;
                        self.pending_approvals.push(tool_call.clone());
                    }
                }
                // Stop the loop - will continue when approvals come in
                break;
            }

            // If all tools auto-approved, continue loop
        }

        Ok(())
    }

    /// Call the LLM and get a response
    async fn call_llm(&self) -> Result<LlmCallResult> {
        let llm_messages = self.session.to_llm_messages();

        let tools = if self.tool_definitions.is_empty() {
            None
        } else {
            Some(self.tool_definitions.clone())
        };

        match self.provider.chat(llm_messages, tools).await {
            Ok(CompletionResult::Message(content)) => Ok(LlmCallResult {
                content: Some(content),
                tool_calls: Vec::new(),
            }),
            Ok(CompletionResult::ToolCalls(pending)) => Ok(LlmCallResult {
                content: None,
                tool_calls: pending
                    .into_iter()
                    .map(|tc| LlmToolCall {
                        id: tc.call_id,
                        name: tc.name,
                        arguments: tc.arguments,
                    })
                    .collect(),
            }),
            Err(e) => Err(crate::error::Error::Provider(e.to_string())),
        }
    }

    /// Categorize tools into auto-approved and needs-approval
    fn categorize_tools(&self, tool_calls: &[ToolCallInfo]) -> (Vec<String>, Vec<String>) {
        let mut auto_approved = Vec::new();
        let mut needs_approval = Vec::new();

        for tc in tool_calls {
            if self.approval_config.should_auto_approve(&tc.name) {
                auto_approved.push(tc.id.clone());
            } else {
                needs_approval.push(tc.id.clone());
            }
        }

        (auto_approved, needs_approval)
    }

    /// Execute multiple tools and batch results into a single message
    /// This is more efficient for the LLM as it sees all results together
    async fn execute_tools_batched(&mut self, tool_calls: &[&ToolCallInfo]) {
        let mut results: Vec<(String, String, bool)> = Vec::new();

        for tool_call in tool_calls {
            // Execute PreToolUse hooks
            if self.hooks_enabled
                && let Err(block_reason) = self.run_pre_tool_hook(&tool_call.name, &tool_call.arguments)
            {
                // Hook blocked the tool execution
                warn!("Tool {} blocked by hook: {}", tool_call.name, block_reason);
                let error_msg = format!("Tool blocked: {}", block_reason);
                self.emit(SessionOutput::tool_done(
                    &tool_call.id,
                    &tool_call.name,
                    false,
                    error_msg.clone(),
                ))
                .await;
                results.push((tool_call.id.clone(), error_msg, true));
                continue;
            }

            // Emit tool start
            self.emit(SessionOutput::tool_start(
                &tool_call.id,
                &tool_call.name,
                tool_call.arguments.clone(),
            ))
            .await;

            // Find and execute the tool
            let (success, output) = if let Some(tool) = self.tool_registry.get(&tool_call.name) {
                match tool.execute(tool_call.arguments.clone()).await {
                    Ok(output) => {
                        let output_str = output.content.to_string();
                        debug!(
                            "Tool {} completed: {} chars",
                            tool_call.name,
                            output_str.len()
                        );
                        (true, output_str)
                    }
                    Err(e) => {
                        warn!("Tool {} failed: {}", tool_call.name, e);
                        (false, format!("Error: {}", e))
                    }
                }
            } else {
                (false, format!("Unknown tool: {}", tool_call.name))
            };

            // Execute PostToolUse hooks
            let final_output = if self.hooks_enabled {
                if let Some(additional_context) = self.run_post_tool_hook(&tool_call.name, &tool_call.arguments, &output) {
                    format!("{}\n\n<post-tool-hook>\n{}\n</post-tool-hook>", output, additional_context)
                } else {
                    output
                }
            } else {
                output
            };

            // Truncate large results to prevent context overflow
            let truncated_output = truncate_tool_result(&final_output, MAX_TOOL_RESULT_SIZE);
            if truncated_output.len() < final_output.len() {
                info!(
                    "Truncated {} result from {} to {} chars",
                    tool_call.name,
                    final_output.len(),
                    truncated_output.len()
                );
            }

            // Emit tool done (with truncated output for display too)
            self.emit(SessionOutput::tool_done(
                &tool_call.id,
                &tool_call.name,
                success,
                truncated_output.clone(),
            ))
            .await;

            // Collect result for batching (truncated to prevent context overflow)
            results.push((tool_call.id.clone(), truncated_output, !success));
        }

        // Add all tool results as a single batched message
        self.session.add_tool_results(results);
    }

    /// Execute a single tool (used for individual tool approvals)
    async fn execute_tool(&mut self, tool_call: &ToolCallInfo) {
        // Execute PreToolUse hooks
        if self.hooks_enabled {
            match self.run_pre_tool_hook(&tool_call.name, &tool_call.arguments) {
                Err(block_reason) => {
                    // Hook blocked the tool execution
                    warn!("Tool {} blocked by hook: {}", tool_call.name, block_reason);
                    let error_msg = format!("Tool blocked: {}", block_reason);
                    self.session.add_tool_result_with_error(&tool_call.id, &error_msg, true);
                    self.emit(SessionOutput::tool_done(
                        &tool_call.id,
                        &tool_call.name,
                        false,
                        error_msg,
                    ))
                    .await;
                    return;
                }
                Ok(Some(ctx)) => {
                    debug!("PreToolUse hook added context for {}: {} chars", tool_call.name, ctx.len());
                }
                Ok(None) => {}
            }
        }

        // Emit tool start
        self.emit(SessionOutput::tool_start(
            &tool_call.id,
            &tool_call.name,
            tool_call.arguments.clone(),
        ))
        .await;

        // Find and execute the tool
        let result = if let Some(tool) = self.tool_registry.get(&tool_call.name) {
            match tool.execute(tool_call.arguments.clone()).await {
                Ok(output) => {
                    let output_str = output.content.to_string();
                    debug!(
                        "Tool {} completed: {} chars",
                        tool_call.name,
                        output_str.len()
                    );
                    (true, output_str)
                }
                Err(e) => {
                    warn!("Tool {} failed: {}", tool_call.name, e);
                    (false, format!("Error: {}", e))
                }
            }
        } else {
            (false, format!("Unknown tool: {}", tool_call.name))
        };

        // Execute PostToolUse hooks
        let mut final_result = result.clone();
        if self.hooks_enabled
            && let Some(additional_context) = self.run_post_tool_hook(&tool_call.name, &tool_call.arguments, &result.1)
        {
            // Append hook context to the tool result
            final_result.1 = format!("{}\n\n<post-tool-hook>\n{}\n</post-tool-hook>", result.1, additional_context);
        }

        // Truncate large results to prevent context overflow
        let truncated_result = truncate_tool_result(&final_result.1, MAX_TOOL_RESULT_SIZE);
        if truncated_result.len() < final_result.1.len() {
            info!(
                "Truncated {} result from {} to {} chars",
                tool_call.name,
                final_result.1.len(),
                truncated_result.len()
            );
        }

        // Add tool result to session with proper error flag (truncated to prevent context overflow)
        let is_error = !final_result.0;
        self.session.add_tool_result_with_error(&tool_call.id, &truncated_result, is_error);

        // Emit tool done (with truncated result)
        self.emit(SessionOutput::tool_done(
            &tool_call.id,
            &tool_call.name,
            final_result.0,
            truncated_result,
        ))
        .await;
    }

    /// Parse questions from AskUserQuestion tool arguments
    fn parse_questions(
        &self,
        args: &serde_json::Value,
    ) -> Option<Vec<super::types::QuestionInfo>> {
        let questions_arr = args.get("questions")?.as_array()?;
        let mut result = Vec::new();

        for q in questions_arr {
            let question = q.get("question")?.as_str()?.to_string();
            let header = q.get("header").and_then(|h| h.as_str()).map(|s| s.to_string());
            let multi_select = q.get("multiSelect").and_then(|m| m.as_bool()).unwrap_or(false);

            let options = q.get("options")?.as_array()?;
            let mut parsed_options = Vec::new();

            for opt in options {
                let label = opt.get("label")?.as_str()?.to_string();
                let description = opt
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());
                parsed_options.push(super::types::QuestionOption { label, description });
            }

            result.push(super::types::QuestionInfo {
                question,
                header,
                options: parsed_options,
                multi_select,
            });
        }

        Some(result)
    }

    /// Handle tool approval
    async fn handle_approve_tool(&mut self, tool_call_id: &str) -> Result<()> {
        // Find the pending tool call
        let tool_call = self
            .pending_approvals
            .iter()
            .find(|tc| tc.id == tool_call_id)
            .cloned();

        if let Some(tool_call) = tool_call {
            // Remove from pending
            self.pending_approvals.retain(|tc| tc.id != tool_call_id);

            // Execute the tool
            self.execute_tool(&tool_call).await;

            // If no more pending, continue the agentic loop
            if self.pending_approvals.is_empty() {
                self.run_agentic_loop().await?;
                self.emit(SessionOutput::idle()).await;
            }
        } else {
            // Tool not found in pending - this shouldn't happen
            warn!(
                "Tool approval received for unknown tool_call_id: {}. Pending: {:?}",
                tool_call_id,
                self.pending_approvals.iter().map(|t| &t.id).collect::<Vec<_>>()
            );
            // Emit idle to prevent CLI from hanging
            self.emit(SessionOutput::idle()).await;
        }

        Ok(())
    }

    /// Handle tool rejection
    async fn handle_reject_tool(&mut self, tool_call_id: &str, reason: Option<String>) {
        // Remove from pending
        self.pending_approvals.retain(|tc| tc.id != tool_call_id);

        // Mark as rejected in session
        self.session.reject_tool(tool_call_id);

        // Add rejection result (with is_error=true since it was rejected)
        let result = reason.unwrap_or_else(|| "Rejected by user".to_string());
        self.session.add_tool_result_with_error(tool_call_id, &result, true);

        // Emit done with rejection
        self.emit(SessionOutput::tool_done(tool_call_id, "", false, result))
            .await;

        // If no more pending, continue the agentic loop
        if self.pending_approvals.is_empty() {
            if let Err(e) = self.run_agentic_loop().await {
                self.emit(SessionOutput::error(e.to_string())).await;
            }
            self.emit(SessionOutput::idle()).await;
        }
    }

    /// Handle answer to a question from AskUserQuestion tool
    async fn handle_answer_question(
        &mut self,
        request_id: &str,
        answers: std::collections::HashMap<String, String>,
    ) -> Result<()> {
        // Format the answer as a tool result
        let result = serde_json::json!({
            "answered": true,
            "request_id": request_id,
            "answers": answers
        });

        // Find the pending AskUserQuestion tool call by ID (request_id is the tool call ID)
        let tool_call = self
            .pending_approvals
            .iter()
            .find(|tc| tc.id == request_id || tc.name == "AskUserQuestion")
            .cloned();

        if let Some(tool_call) = tool_call {
            // Remove from pending by ID, not by name (fixes bug with multiple questions)
            self.pending_approvals
                .retain(|tc| tc.id != tool_call.id);

            // Add result to session
            self.session
                .add_tool_result(&tool_call.id, result.to_string());

            // Emit done
            self.emit(SessionOutput::tool_done(
                &tool_call.id,
                "AskUserQuestion",
                true,
                result.to_string(),
            ))
            .await;

            // Continue the agentic loop
            if self.pending_approvals.is_empty() {
                self.run_agentic_loop().await?;
                self.emit(SessionOutput::idle()).await;
            }
        }

        Ok(())
    }

    // ========================================================================
    // Context Management
    // ========================================================================

    /// Convert ChatMessages to context Messages for token counting
    ///
    /// IMPORTANT: This includes content_blocks and tool_calls in the content
    /// because they contribute significantly to the actual token count sent to the LLM.
    fn chat_messages_to_context_messages(&self) -> Vec<Message> {
        self.session
            .messages
            .iter()
            .map(|cm| {
                let role = match cm.role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "system" => MessageRole::System,
                    _ => MessageRole::Tool,
                };

                // Build full content including content_blocks and tool_calls
                let mut full_content = cm.content.clone();

                // Add content blocks (tool_result, text, etc.)
                for block in &cm.content_blocks {
                    if let Ok(json) = serde_json::to_string(block) {
                        full_content.push_str(&json);
                    }
                }

                // Add tool calls (tool_use blocks)
                for tc in &cm.tool_calls {
                    // Tool calls include name and arguments
                    full_content.push_str(&tc.name);
                    if let Ok(json) = serde_json::to_string(&tc.arguments) {
                        full_content.push_str(&json);
                    }
                }

                Message::with_timestamp(role, &full_content, cm.timestamp)
            })
            .collect()
    }

    /// Check context usage and compact if necessary
    ///
    /// This implements automatic context management similar to Claude Code:
    /// - Calculates current token usage
    /// - If above threshold (75%), triggers auto-compaction
    /// - Uses LLM-powered summarization when possible, falls back to heuristics
    async fn check_and_compact_context(&mut self) -> Result<()> {
        let messages = self.chat_messages_to_context_messages();

        // Serialize tool definitions to count their tokens
        // Tool definitions are sent with every request and can be quite large
        let tool_defs_json = serde_json::to_string(&self.tool_definitions).unwrap_or_default();

        let usage = self.context_monitor.calculate_usage(
            &messages,
            &self.session.system_prompt,
            Some(&tool_defs_json), // Include tool definitions in token count
        );

        // Log detailed breakdown for debugging context issues
        info!(
            "Context usage: {:.1}% ({}/{} tokens) - breakdown: system={}, memory/tools={}, conversation={}, tool_results={}",
            usage.used_percentage * 100.0,
            usage.used_tokens,
            usage.limit_tokens,
            usage.breakdown.system_tokens,
            usage.breakdown.memory_tokens,
            usage.breakdown.conversation_tokens,
            usage.breakdown.tool_tokens,
        );

        if !usage.should_compact {
            return Ok(());
        }

        info!(
            "Context threshold exceeded ({:.1}%), initiating auto-compaction",
            usage.used_percentage * 100.0
        );

        // Emit compaction notification
        self.emit(SessionOutput::thinking(format!(
            "Context at {:.0}% - compacting conversation history...",
            usage.used_percentage * 100.0
        )))
        .await;

        // Use LLM-powered compaction for better context preservation
        // This is a separate API call, not recursive - Claude Code does this too
        let config = CompactConfig::auto();
        let result = self
            .summarizer
            .compact(
                &messages,
                self.context_monitor.counter(),
                config,
                Some(&self.provider), // Use LLM for better summaries
            )
            .await?;

        info!(
            "Compaction complete: {} -> {} tokens ({} messages summarized)",
            result.tokens_before,
            result.tokens_after,
            result.messages_summarized
        );

        // Replace session messages with compacted version
        self.apply_compaction_result(&result);

        // Emit completion notification
        self.emit(SessionOutput::thinking(format!(
            "Compacted {} messages into summary ({} -> {} tokens)",
            result.messages_summarized, result.tokens_before, result.tokens_after
        )))
        .await;

        Ok(())
    }

    /// Apply compaction result to the session
    ///
    /// Following Anthropic SDK approach: replace entire conversation with
    /// a single USER message containing the summary wrapped in <summary> tags.
    fn apply_compaction_result(&mut self, result: &crate::context::CompactResult) {
        // Clear existing messages
        self.session.clear();

        // Add the summary as a single USER message (following Anthropic SDK)
        // The summary contains <summary>...</summary> tags
        self.session.messages.push(ChatMessage::user(&result.summary.content));
    }

    // ========================================================================
    // Hook Execution
    // ========================================================================

    /// Execute hooks for a given event
    ///
    /// Returns any additional context from hooks to inject into the conversation.
    /// Returns an error if a hook blocks the action.
    fn execute_hooks(&self, event: HookEvent, context: &HookContext) -> std::result::Result<Option<String>, String> {
        if !self.hooks_enabled || self.hooks_config.is_empty() {
            return Ok(None);
        }

        let results = self.hook_executor.execute(event, &self.hooks_config, context);

        let mut additional_context = Vec::new();

        for result in results {
            match result {
                Ok(hook_result) => {
                    // Check for block action
                    if hook_result.block {
                        return Err(hook_result.block_reason.unwrap_or_else(|| "Blocked by hook".to_string()));
                    }

                    // Collect additional context
                    if let Some(ctx) = hook_result.additional_context {
                        additional_context.push(ctx);
                    }
                }
                Err(e) => {
                    // Log but don't fail on hook errors
                    warn!("Hook execution failed: {}", e);
                }
            }
        }

        if additional_context.is_empty() {
            Ok(None)
        } else {
            Ok(Some(additional_context.join("\n\n")))
        }
    }

    /// Execute PreToolUse hooks
    ///
    /// Returns Ok(Some(ctx)) if additional context should be added
    /// Returns Ok(None) if no context
    /// Returns Err if the tool should be blocked
    fn run_pre_tool_hook(&self, tool_name: &str, args: &serde_json::Value) -> std::result::Result<Option<String>, String> {
        let context = HookContext::pre_tool_use(&self.session_id, tool_name, args.clone());
        self.execute_hooks(HookEvent::PreToolUse, &context)
    }

    /// Execute PostToolUse hooks
    fn run_post_tool_hook(&self, tool_name: &str, args: &serde_json::Value, result: &str) -> Option<String> {
        let context = HookContext::post_tool_use(&self.session_id, tool_name, args.clone(), result);
        self.execute_hooks(HookEvent::PostToolUse, &context).ok().flatten()
    }

    /// Execute UserPromptSubmit hooks
    fn run_user_prompt_hook(&self, prompt: &str) -> std::result::Result<Option<String>, String> {
        let context = HookContext::user_prompt(&self.session_id, prompt);
        self.execute_hooks(HookEvent::UserPromptSubmit, &context)
    }

    /// Emit an output
    async fn emit(&self, output: SessionOutput) {
        if let Err(e) = self
            .output_tx
            .send((self.session_id.clone(), output))
            .await
        {
            error!("Failed to emit output: {}", e);
        }
    }

    /// Save session to disk
    async fn save_session(&self) -> Result<()> {
        // Don't save empty sessions
        if self.session.messages.is_empty() {
            debug!("Session {} has no messages, skipping save", self.session_id);
            return Ok(());
        }

        // Get sessions directory
        let sessions_dir = get_sessions_dir()?;
        std::fs::create_dir_all(&sessions_dir)?;

        // Convert ChatMessages to SavedMessages
        let messages: Vec<SavedMessage> = self
            .session
            .messages
            .iter()
            .map(|cm| SavedMessage {
                role: cm.role.clone(),
                content: cm.content.clone(),
                tool_calls: cm
                    .tool_calls
                    .iter()
                    .map(|tc| SavedToolCall {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    })
                    .collect(),
            })
            .collect();

        let saved = SavedSession {
            id: self.session_id.clone(),
            name: format!("Session {}", self.session_id),
            messages,
            created_at: self.created_at,
            updated_at: chrono::Utc::now(),
        };

        // Write to file
        let path = sessions_dir.join(format!("{}.json", self.session_id));
        let json = serde_json::to_string_pretty(&saved)?;
        std::fs::write(&path, json)?;

        info!("Saved session {} to {:?}", self.session_id, path);
        Ok(())
    }
}

/// Get the sessions directory path
pub fn get_sessions_dir() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .map(|p| p.join("cowork"))
        .unwrap_or_else(|| PathBuf::from(".cowork"));
    Ok(base.join("sessions"))
}

/// Load a saved session by ID
pub fn load_session(session_id: &str) -> Result<Option<SavedSession>> {
    let path = get_sessions_dir()?.join(format!("{}.json", session_id));
    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read_to_string(&path)?;
    let saved: SavedSession = serde_json::from_str(&json)?;
    Ok(Some(saved))
}

/// List all saved sessions
pub fn list_saved_sessions() -> Result<Vec<SavedSession>> {
    let sessions_dir = get_sessions_dir()?;
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(json) => match serde_json::from_str::<SavedSession>(&json) {
                    Ok(session) => sessions.push(session),
                    Err(e) => warn!("Failed to parse session {:?}: {}", path, e),
                },
                Err(e) => warn!("Failed to read session {:?}: {}", path, e),
            }
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

/// Truncate a tool result to prevent context overflow
///
/// Large tool outputs (e.g., listing 3000+ files) can exceed the model's
/// context limit in a single response. This function truncates results
/// to a safe size while preserving useful information.
fn truncate_tool_result(result: &str, max_size: usize) -> String {
    if result.len() <= max_size {
        return result.to_string();
    }

    // Find a safe truncation point (avoid cutting mid-character)
    let truncate_at = result
        .char_indices()
        .take_while(|(i, _)| *i < max_size)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max_size);

    format!(
        "{}...\n\n[Result truncated - {} chars total, showing first {}]",
        &result[..truncate_at],
        result.len(),
        truncate_at
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_categorization() {
        let approval_config = ToolApprovalConfig::new(crate::approval::ApprovalLevel::Low);

        // Read should be auto-approved (PascalCase tool names)
        assert!(approval_config.should_auto_approve("Read"));
        assert!(approval_config.should_auto_approve("Glob"));
        assert!(approval_config.should_auto_approve("Grep"));

        // Write operations should need approval
        assert!(!approval_config.should_auto_approve("Write"));
        assert!(!approval_config.should_auto_approve("Bash"));
    }
}
