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
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

use super::types::{SessionConfig, SessionId, SessionInput, SessionOutput};
use crate::approval::ToolApprovalConfig;
use crate::formatting::{format_tool_call, format_tool_result_summary};
use crate::context::{
    CompactConfig, ContextMonitor, ConversationSummarizer, Message, MessageRole, SummarizerConfig,
};
use crate::error::Result;
use crate::orchestration::{ChatSession, ToolRegistryBuilder};
use crate::prompt::{HookContext, HookEvent, HookExecutor, HooksConfig};
use crate::provider::{ChatMessage, GenAIProvider, ToolCall};
use crate::skills::SkillRegistry;
use crate::tools::interaction::ASK_QUESTION_TOOL_NAME;
use crate::tools::planning::PlanModeState;
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
    tool_calls: Vec<ToolCall>,
    /// Input tokens for this request (from provider)
    input_tokens: Option<u64>,
    /// Output tokens for this response (from provider)
    output_tokens: Option<u64>,
}

/// Info for spawning a subagent from a skill with `context: fork`
struct SubagentSpawnInfo {
    /// The skill prompt to use as the task
    prompt: String,
    /// Skill name
    skill_name: String,
    /// Agent type (e.g., "Explore", "Plan", "general-purpose")
    agent_type: String,
    /// Model override (optional)
    model: Option<String>,
}

/// Result from a spawned tool execution
struct SpawnedToolResult {
    id: String,
    name: String,
    arguments: serde_json::Value,
    success: bool,
    output: String,
    /// For skill injection: (content, skill_name)
    inject_info: Option<(String, Option<String>)>,
    /// For skill subagent spawning (context: fork)
    subagent_info: Option<SubagentSpawnInfo>,
}

/// Saved session state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub id: String,
    pub name: String,
    /// Messages stored using genai's ChatMessage directly
    pub messages: Vec<ChatMessage>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// The unified agent loop
pub struct AgentLoop {
    /// Session identifier
    session_id: SessionId,
    /// Message receiver (questions from user)
    message_rx: mpsc::UnboundedReceiver<String>,
    /// Answer receiver (approvals/answers from user)
    answer_rx: mpsc::UnboundedReceiver<SessionInput>,
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
    /// Plan mode state (shared with EnterPlanMode/ExitPlanMode tools and /plan command)
    plan_mode_state: Arc<tokio::sync::RwLock<PlanModeState>>,
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
    /// Whether to persist the session on exit
    save_session: bool,
    /// When the session was created
    created_at: chrono::DateTime<chrono::Utc>,
}

impl AgentLoop {
    /// Create a new agent loop
    pub async fn new(
        session_id: SessionId,
        mut input_rx: mpsc::Receiver<SessionInput>,
        output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
        config: SessionConfig,
    ) -> Result<Self> {
        // Create internal channels for dispatching
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (answer_tx, answer_rx) = mpsc::unbounded_channel();

        // Create shared plan mode state
        let plan_mode_state = Arc::new(tokio::sync::RwLock::new(PlanModeState::default()));
        let plan_mode_for_dispatcher = plan_mode_state.clone();
        let output_for_dispatcher = output_tx.clone();
        let sid_for_dispatcher = session_id.clone();

        // Spawn Dispatcher Task
        // This task reads from the main input channel and routes messages to the correct internal channel
        let sid = session_id.clone();
        tokio::spawn(async move {
            info!("Dispatcher started for session: {}", sid);
            while let Some(input) = input_rx.recv().await {
                match input {
                    SessionInput::UserMessage { content } => {
                        if let Err(e) = message_tx.send(content) {
                            error!("Dispatcher: failed to send user message (receiver dropped?): {}", e);
                            break;
                        }
                    }
                    SessionInput::SetPlanMode { active } => {
                        // Update plan mode state
                        let plan_file = {
                            let mut state = plan_mode_for_dispatcher.write().await;
                            state.active = active;
                            if active && state.plan_file.is_none() {
                                // Generate a new plan file when entering plan mode
                                let plans_dir = crate::tools::planning::get_plans_dir();
                                let _ = std::fs::create_dir_all(&plans_dir);
                                Some(state.generate_plan_file())
                            } else if !active {
                                // Clear plan file when exiting plan mode
                                state.plan_file.take()
                            } else {
                                state.plan_file.clone()
                            }
                        };
                        // Emit plan mode changed event with plan file path
                        let _ = output_for_dispatcher.send((
                            sid_for_dispatcher.clone(),
                            SessionOutput::plan_mode_changed(active, plan_file.map(|p| p.to_string_lossy().to_string())),
                        )).await;
                        debug!("Plan mode set to {} for session {}", active, sid_for_dispatcher);
                    }
                    // All other inputs are answers/approvals
                    input => {
                        if let Err(e) = answer_tx.send(input) {
                            error!("Dispatcher: failed to send answer/approval (receiver dropped?): {:?}", e);
                            break;
                        }
                    }
                }
            }
            info!("Dispatcher ended for session: {} (input channel closed)", sid);
        });

        // Create the provider
        debug!(
            "AgentLoop config: provider={:?}, model={:?}, system_prompt_len={}",
            config.provider_id,
            config.model,
            config.system_prompt.as_ref().map(|s| s.len()).unwrap_or(0),
        );
        let provider = match config.api_key.as_deref() {
            Some(key) => GenAIProvider::with_api_key(&config.provider_id, key, config.model.as_deref())?,
            None => GenAIProvider::new(&config.provider_id, config.model.as_deref())?,
        };
        let provider = match config.system_prompt.as_deref() {
            Some(prompt) => provider.with_system_prompt(prompt),
            None => provider,
        };

        // Create chat session
        let session = match &config.system_prompt {
            Some(prompt) => ChatSession::with_system_prompt(prompt),
            None => ChatSession::new(),
        };

        // Create skill registry
        let skill_registry = Arc::new(SkillRegistry::with_builtins(config.workspace_path.clone()));

        // Create tool registry (plan_mode_state was created above before dispatcher)
        let mut tool_builder = ToolRegistryBuilder::new(config.workspace_path.clone())
            .with_provider(&config.provider_id)
            .with_skill_registry(skill_registry)
            .with_plan_mode_state(plan_mode_state.clone());

        if let Some(ref key) = config.api_key {
            tool_builder = tool_builder.with_api_key(key.clone());
        }

        // Add web search config if available
        if let Some(ws_config) = config.web_search_config.clone() {
            tool_builder = tool_builder.with_web_search_config(ws_config);
        }

        // Apply tool scope if set (for subagents)
        if let Some(scope) = config.tool_scope.clone() {
            tool_builder = tool_builder.with_tool_scope(scope);
        }

        // Wire progress channel so subagent activity is forwarded to TUI
        tool_builder = tool_builder.with_progress_channel(output_tx.clone(), session_id.clone());

        // Share session registry so subagents can register for approval routing
        if let Some(reg) = config.session_registry.clone() {
            tool_builder = tool_builder.with_session_registry(reg);
        }

        // Add MCP server manager if available
        if let Some(mcp_manager) = config.mcp_manager.clone() {
            tool_builder = tool_builder.with_mcp_manager(mcp_manager);
        }

        let tool_registry = tool_builder.build();

        let tool_definitions = tool_registry.list();

        // Initialize context monitor with provider and model for accurate limits
        let context_monitor = match &config.model {
            Some(model) => {
                debug!(
                    provider_id = %config.provider_id,
                    model = %model,
                    "Creating ContextMonitor with model"
                );
                ContextMonitor::with_model(&config.provider_id, model)
            }
            None => {
                debug!(
                    provider_id = %config.provider_id,
                    "Creating ContextMonitor without model"
                );
                ContextMonitor::new(&config.provider_id)
            }
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
        let hooks_enabled = config.enable_hooks.unwrap_or(config.prompt_config.enable_hooks);

        Ok(Self {
            session_id,
            message_rx,
            answer_rx,
            output_tx,
            provider,
            session,
            tool_registry,
            tool_definitions,
            approval_config: config.approval_config,
            plan_mode_state,
            context_monitor,
            summarizer,
            hook_executor,
            hooks_config,
            hooks_enabled,
            save_session: config.save_session,
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

        // Main Loop: Only cares about Questions (UserMessages)
        // The Agentic Loop (inside handle_user_message) handles Answers (Approvals)
        while let Some(content) = self.message_rx.recv().await {
            if let Err(e) = self.handle_user_message(content).await {
                self.emit(SessionOutput::error(e.to_string())).await;
            }
            // Emit Idle when the turn is complete
            self.emit(SessionOutput::idle()).await;
        }

        // Channel closed - this happens when the session is stopped or the dispatcher exits
        info!("Message channel closed for session: {}", self.session_id);

        // Save session before exiting (if enabled)
        if self.save_session {
            info!("Saving session {} before exit", self.session_id);
            if let Err(e) = self.save_session().await {
                error!("Failed to save session {}: {}", self.session_id, e);
            }
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

            // Check for cancellation before each iteration
            if self.check_cancelled() {
                self.emit(SessionOutput::cancelled()).await;
                return Ok(());
            }

            // Check and compact context if needed before calling LLM
            if let Err(e) = self.check_and_compact_context().await {
                warn!("Context compaction failed: {}, continuing anyway", e);
            }

            // Call LLM
            self.emit(SessionOutput::thinking("Thinking...".to_string()))
                .await;

            let response = self.call_llm().await?;

            // Update context monitor with LLM-reported token counts
            self.context_monitor.update_from_response(response.input_tokens, response.output_tokens);

            // Generate message ID
            let msg_id = uuid::Uuid::new_v4().to_string();

            // Emit assistant message with token usage appended to content
            let content = response.content.clone().unwrap_or_default();
            if !content.is_empty() {
                let context_limit = self.context_monitor.context_limit();

                debug!(
                    input_tokens = ?response.input_tokens,
                    output_tokens = ?response.output_tokens,
                    context_limit = context_limit,
                    "Emitting assistant message with tokens"
                );

                self.emit(SessionOutput::assistant_message_with_tokens(
                    &msg_id,
                    &content,
                    response.input_tokens,
                    response.output_tokens,
                    Some(context_limit),
                ))
                .await;
            }

            // Check for tool calls
            if response.tool_calls.is_empty() {
                // Add final assistant message to session history (important for multi-turn conversations)
                self.session.add_assistant_message(&content, Vec::new());

                // Warn if content suggests the model intended to make tool calls but didn't
                // This can happen when the response is truncated due to max_tokens
                let content_lower = content.to_lowercase();
                let suggests_action = content_lower.contains("let me ")
                    || content_lower.contains("i'll ")
                    || content_lower.contains("i will ")
                    || content_lower.contains("now i'll ")
                    || content_lower.contains("now let me ")
                    || content_lower.contains("let's ")
                    || content_lower.ends_with(":")
                    || content_lower.ends_with("...");

                if suggests_action && !content.is_empty() {
                    warn!(
                        content = %content,
                        iteration = iteration,
                        "Response has content suggesting tool usage but no tool calls - possible truncation"
                    );
                }

                // No tool calls, we're done
                break;
            }

            // Add assistant message with tool calls
            let tool_calls = response.tool_calls.clone();
            self.session.add_assistant_message(&content, tool_calls.clone());

            // Partition: auto-approved vs needs interaction (approval or question)
            let mut auto_approved = Vec::new();
            let mut needs_interaction = Vec::new();

            for tc in &tool_calls {
                // Log warning if tool call has empty or null arguments
                if tc.fn_arguments.is_null() ||
                   (tc.fn_arguments.is_object() && tc.fn_arguments.as_object().map(|o| o.is_empty()).unwrap_or(false)) {
                    warn!(
                        tool_name = %tc.fn_name,
                        tool_id = %tc.call_id,
                        arguments = ?tc.fn_arguments,
                        "Tool call received with empty or null arguments"
                    );
                }
                if tc.fn_name == ASK_QUESTION_TOOL_NAME {
                    needs_interaction.push(tc);
                } else if self.approval_config.should_auto_approve_with_args(&tc.fn_name, &tc.fn_arguments) {
                    auto_approved.push(tc);
                } else {
                    needs_interaction.push(tc);
                }
            }

            // Spawn auto-approved tools in parallel
            let mut join_set: JoinSet<SpawnedToolResult> = JoinSet::new();
            for tool_call in &auto_approved {
                // Emit tool_start (ephemeral) and tool_call (persistent) before spawning
                self.emit_tool_execution_start(tool_call).await;

                if let Some(tool) = self.tool_registry.get(&tool_call.fn_name) {
                    let id = tool_call.call_id.clone();
                    let name = tool_call.fn_name.clone();
                    let arguments = tool_call.fn_arguments.clone();

                    join_set.spawn(async move {
                        match tool.execute(arguments.clone()).await {
                            Ok(output) => {
                                let output_str = output.content.to_string();
                                let skill_name = output.metadata.get(crate::tools::skill::SKILL_NAME_KEY)
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());

                                // Check if this skill should spawn a subagent
                                let spawn_subagent = output.metadata.get(crate::tools::skill::SPAWN_SUBAGENT)
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);

                                let (inject_info, subagent_info) = if spawn_subagent {
                                    // Extract subagent configuration
                                    let agent_type = output.metadata.get(crate::tools::skill::SUBAGENT_TYPE)
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("general-purpose")
                                        .to_string();
                                    let model = output.metadata.get(crate::tools::skill::MODEL_OVERRIDE)
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    (None, Some(SubagentSpawnInfo {
                                        prompt: output_str.clone(),
                                        skill_name: skill_name.clone().unwrap_or_default(),
                                        agent_type,
                                        model,
                                    }))
                                } else {
                                    // Check for inline injection
                                    let inject = output.metadata.get(crate::tools::skill::INJECT_AS_MESSAGE)
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);
                                    let inject_info = if inject { Some((output_str.clone(), skill_name)) } else { None };
                                    (inject_info, None)
                                };

                                SpawnedToolResult {
                                    id,
                                    name,
                                    arguments,
                                    success: true,
                                    output: output_str,
                                    inject_info,
                                    subagent_info,
                                }
                            }
                            Err(e) => SpawnedToolResult {
                                id,
                                name,
                                arguments,
                                success: false,
                                output: format!("Error: {}", e),
                                inject_info: None,
                                subagent_info: None,
                            }
                        }
                    });
                } else {
                    // Tool not found - handle immediately
                    let error_msg = format!("Unknown tool: {}", tool_call.fn_name);
                    self.session.add_tool_result(&tool_call.call_id, &error_msg, true);
                    self.emit(SessionOutput::tool_done(&tool_call.call_id, &tool_call.fn_name, false, error_msg)).await;
                }
            }

            // Track completed tool IDs for cancel cleanup
            let mut completed_tool_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

            // Process interaction tools (while spawned tasks run in background)
            for (tool_idx, tool_call) in needs_interaction.iter().enumerate() {
                if tool_call.fn_name == ASK_QUESTION_TOOL_NAME {
                    if let Some(questions) = self.parse_questions(&tool_call.fn_arguments) {
                        self.emit(SessionOutput::Question {
                            request_id: tool_call.call_id.clone(),
                            questions,
                            subagent_id: None,
                        }).await;

                        // Wait for answer (loop to handle unexpected messages)
                        loop {
                            match self.answer_rx.recv().await {
                                Some(SessionInput::AnswerQuestion { answers, .. }) => {
                                    let result = serde_json::json!({ "answered": true, "answers": answers });
                                    self.session.add_tool_result(&tool_call.call_id, result.to_string(), false);
                                    self.emit(SessionOutput::tool_done(&tool_call.call_id, ASK_QUESTION_TOOL_NAME, true, result.to_string())).await;
                                    completed_tool_ids.insert(tool_call.call_id.clone());
                                    break;
                                }
                                Some(SessionInput::Cancel) => {
                                    // Cancelled - add results for remaining tools to keep message history valid
                                    self.handle_cancel_cleanup(&tool_calls, &needs_interaction, tool_idx, &mut completed_tool_ids, &mut join_set).await;
                                    self.emit(SessionOutput::cancelled()).await;
                                    return Ok(());
                                }
                                Some(other) => {
                                    warn!("Unexpected input while waiting for answer: {:?}", other);
                                }
                                None => {
                                    // Channel closed unexpectedly
                                    warn!("Answer channel closed while waiting for question response");
                                    self.emit(SessionOutput::error("Session interrupted: input channel closed".to_string())).await;
                                    return Ok(());
                                }
                            }
                        }
                    }
                } else {
                    // Needs approval
                    self.emit(SessionOutput::tool_pending(&tool_call.call_id, &tool_call.fn_name, tool_call.fn_arguments.clone(), None)).await;

                    // Wait for approval/rejection (loop to handle unexpected messages)
                    loop {
                        match self.answer_rx.recv().await {
                            Some(SessionInput::ApproveTool { .. }) => {
                                self.execute_tool(tool_call).await;
                                completed_tool_ids.insert(tool_call.call_id.clone());
                                break;
                            }
                            Some(SessionInput::RejectTool { reason, .. }) => {
                                let reason = reason.unwrap_or_else(|| "Rejected by user".to_string());
                                self.session.add_tool_result(&tool_call.call_id, &reason, true);
                                self.emit(SessionOutput::tool_done(&tool_call.call_id, &tool_call.fn_name, false, reason)).await;
                                completed_tool_ids.insert(tool_call.call_id.clone());
                                break;
                            }
                            Some(SessionInput::Cancel) => {
                                // Cancelled - add results for remaining tools to keep message history valid
                                self.handle_cancel_cleanup(&tool_calls, &needs_interaction, tool_idx, &mut completed_tool_ids, &mut join_set).await;
                                self.emit(SessionOutput::cancelled()).await;
                                return Ok(());
                            }
                            Some(other) => {
                                warn!("Unexpected input while waiting for approval: {:?}", other);
                            }
                            None => {
                                // Channel closed unexpectedly
                                warn!("Answer channel closed while waiting for tool approval");
                                self.emit(SessionOutput::error("Session interrupted: input channel closed".to_string())).await;
                                return Ok(());
                            }
                        }
                    }
                }
            }

            // Collect results from spawned auto-approved tools
            while let Some(result) = join_set.join_next().await {
                if let Ok(res) = result {
                    self.finalize_spawned_tool(res).await;
                }
            }
        }

        Ok(())
    }

    /// Tools allowed when plan mode is active
    /// Note: Write is allowed for writing the plan file to ~/.claude/plans/
    const PLAN_MODE_TOOLS: &'static [&'static str] = &[
        "Read", "Glob", "Grep", "LSP", "WebFetch", "WebSearch", "Write",
        ASK_QUESTION_TOOL_NAME, "ExitPlanMode", "TodoWrite",
    ];

    /// Call the LLM and get a response
    async fn call_llm(&self) -> Result<LlmCallResult> {
        let mut llm_messages = self.session.get_messages().to_vec();

        // Check plan mode state — filter tools and inject reminder
        let plan_state = self.plan_mode_state.read().await;
        let plan_active = plan_state.active;
        let plan_file = plan_state.plan_file.clone();
        drop(plan_state); // Release the lock

        let tools = if self.tool_definitions.is_empty() {
            None
        } else if plan_active {
            // Filter to only plan-mode-allowed tools
            let filtered: Vec<_> = self.tool_definitions.iter()
                .filter(|td| Self::PLAN_MODE_TOOLS.contains(&td.name.as_str()))
                .cloned()
                .collect();
            if filtered.is_empty() { None } else { Some(filtered) }
        } else {
            Some(self.tool_definitions.clone())
        };

        // Inject plan mode reminder into the messages
        if plan_active {
            let base_reminder = crate::prompt::builtin::claude_code::reminders::PLAN_MODE_ACTIVE;
            // Add plan file path to the reminder
            let reminder = if let Some(ref pf) = plan_file {
                format!("{}\n\nA plan file exists from plan mode at: {}\n\nWrite your plan to this file using the Write tool.",
                    base_reminder, pf.to_string_lossy())
            } else {
                base_reminder.to_string()
            };
            // Append as a system reminder on the last user message
            if let Some(last_user) = llm_messages.iter_mut().rev()
                .find(|m| matches!(m.role, crate::provider::ChatRole::User))
            {
                let suffix = format!("\n\n<system-reminder>\n{}\n</system-reminder>", reminder);
                crate::provider::append_message_text(last_user, &suffix);
            }
        }

        // Use non-streaming for now to get accurate token counts from provider
        // TODO: Re-enable streaming once we capture usage from Final event
        match self.provider.chat(llm_messages, tools).await {
            Ok(result) => Ok(LlmCallResult {
                content: result.content,
                tool_calls: result.tool_calls,
                input_tokens: result.input_tokens,
                output_tokens: result.output_tokens,
            }),
            Err(e) => Err(crate::error::Error::Provider(e.to_string())),
        }
    }

    /// Execute a single tool
    async fn execute_tool(&mut self, tool_call: &ToolCall) {
        // Execute PreToolUse hooks
        if self.hooks_enabled {
            match self.run_pre_tool_hook(&tool_call.fn_name, &tool_call.fn_arguments) {
                Err(block_reason) => {
                    // Hook blocked the tool execution
                    warn!("Tool {} blocked by hook: {}", tool_call.fn_name, block_reason);
                    let error_msg = format!("Tool blocked: {}", block_reason);
                    self.session.add_tool_result(&tool_call.call_id, &error_msg, true);
                    self.emit(SessionOutput::tool_done(
                        &tool_call.call_id,
                        &tool_call.fn_name,
                        false,
                        &error_msg,
                    ))
                    .await;
                    // Also emit tool_result for the blocked case
                    self.emit(SessionOutput::tool_result(
                        &tool_call.call_id,
                        &tool_call.fn_name,
                        false,
                        &error_msg,
                        format!("Blocked: {}", block_reason),
                        None,
                    ))
                    .await;
                    return;
                }
                Ok(Some(ctx)) => {
                    debug!("PreToolUse hook added context for {}: {} chars", tool_call.fn_name, ctx.len());
                }
                Ok(None) => {}
            }
        }

        // Emit tool start (ephemeral) and tool call (persistent)
        self.emit_tool_execution_start(&tool_call).await;

        // Find and execute the tool
        let (result, inject_message) = if let Some(tool) = self.tool_registry.get(&tool_call.fn_name) {
            match tool.execute(tool_call.fn_arguments.clone()).await {
                Ok(tool_output) => {
                    let inject = tool_output.metadata.get(crate::tools::skill::INJECT_AS_MESSAGE)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let skill_name = tool_output.metadata.get(crate::tools::skill::SKILL_NAME_KEY)
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let output_str = tool_output.content.to_string();
                    debug!(
                        "Tool {} completed: {} chars",
                        tool_call.fn_name,
                        output_str.len()
                    );
                    let inject_info = if inject { Some((output_str.clone(), skill_name)) } else { None };
                    ((true, output_str), inject_info)
                }
                Err(e) => {
                    debug!("Tool {} failed: {}", tool_call.fn_name, e);
                    ((false, format!("Error: {}", e)), None)
                }
            }
        } else {
            ((false, format!("Unknown tool: {}", tool_call.fn_name)), None)
        };

        // Handle skill message injection
        if let Some((content, skill_name)) = inject_message {
            let name = skill_name.as_deref().unwrap_or("unknown");
            let brief_result = format!("Skill '{}' loaded. Follow the instructions below.", name);

            self.session.add_tool_result(&tool_call.call_id, &brief_result, false);

            self.emit(SessionOutput::tool_done(
                &tool_call.call_id,
                &tool_call.fn_name,
                true,
                brief_result,
            ))
            .await;

            // Inject skill content as a user message with command-name tag
            let injected = format!("<command-name>/{}</command-name>\n\n{}", name, content);
            self.session.add_user_message(&injected);
            return;
        }

        // Execute PostToolUse hooks
        let mut final_result = result.clone();
        if self.hooks_enabled
            && let Some(additional_context) = self.run_post_tool_hook(&tool_call.fn_name, &tool_call.fn_arguments, &result.1)
        {
            // Append hook context to the tool result
            final_result.1 = format!("{}\n\n<post-tool-hook>\n{}\n</post-tool-hook>", result.1, additional_context);
        }

        // Truncate large results to prevent context overflow
        let truncated_result = truncate_tool_result(&final_result.1, MAX_TOOL_RESULT_SIZE);
        if truncated_result.len() < final_result.1.len() {
            info!(
                "Truncated {} result from {} to {} chars",
                tool_call.fn_name,
                final_result.1.len(),
                truncated_result.len()
            );
        }

        // Generate summary and diff for tool result
        let (summary, diff_preview) = format_tool_result_summary(
            &tool_call.fn_name,
            final_result.0,
            &truncated_result,
            &tool_call.fn_arguments,
        );

        // Add tool result to session with proper error flag (truncated to prevent context overflow)
        let is_error = !final_result.0;
        self.session.add_tool_result(&tool_call.call_id, &truncated_result, is_error);

        // Emit tool done (ephemeral, with truncated result)
        self.emit(SessionOutput::tool_done(
            &tool_call.call_id,
            &tool_call.fn_name,
            final_result.0,
            &truncated_result,
        ))
        .await;

        // Emit tool result (persistent message)
        self.emit(SessionOutput::tool_result(
            &tool_call.call_id,
            &tool_call.fn_name,
            final_result.0,
            &truncated_result,
            summary,
            diff_preview,
        ))
        .await;
    }

    /// Finalize a spawned tool execution result
    ///
    /// Handles skill injection, subagent spawning, truncation, session update, and emits tool_done.
    /// Used for tools that were executed in parallel.
    async fn finalize_spawned_tool(&mut self, res: SpawnedToolResult) {
        // Handle skill subagent spawning (context: fork)
        if let Some(info) = res.subagent_info {
            let brief_result = format!(
                "Skill '{}' delegated to {} subagent.",
                info.skill_name, info.agent_type
            );

            self.session.add_tool_result(&res.id, &brief_result, false);
            self.emit(SessionOutput::tool_done(&res.id, &res.name, true, brief_result.clone())).await;

            // Add the task as a user message so the LLM sees it needs to be executed
            // The LLM will then call the Task tool with the appropriate parameters
            let model_hint = info.model.as_ref()
                .map(|m| format!(" with model=\"{}\"", m))
                .unwrap_or_default();
            let task_instruction = format!(
                "Execute this skill in a subagent:\n\n<skill name=\"{}\" agent=\"{}\">\n{}\n</skill>\n\nUse the Task tool with subagent_type=\"{}\"{} to execute this.",
                info.skill_name, info.agent_type, info.prompt, info.agent_type, model_hint
            );
            self.session.add_user_message(&task_instruction);
            return;
        }

        // Handle skill message injection (inline execution)
        if let Some((content, skill_name)) = res.inject_info {
            let name = skill_name.as_deref().unwrap_or("unknown");
            let brief_result = format!("Skill '{}' loaded. Follow the instructions below.", name);

            self.session.add_tool_result(&res.id, &brief_result, false);
            self.emit(SessionOutput::tool_done(&res.id, &res.name, true, brief_result)).await;

            // Inject skill content as a user message with command-name tag
            let injected = format!("<command-name>/{}</command-name>\n\n{}", name, content);
            self.session.add_user_message(&injected);
            return;
        }

        // Run post-tool hooks (if enabled)
        let mut final_output = res.output;
        if self.hooks_enabled
            && let Some(additional_context) = self.run_post_tool_hook(&res.name, &res.arguments, &final_output)
        {
            final_output = format!("{}\n\n<post-tool-hook>\n{}\n</post-tool-hook>", final_output, additional_context);
        }

        // Truncate large results
        let truncated = truncate_tool_result(&final_output, MAX_TOOL_RESULT_SIZE);
        if truncated.len() < final_output.len() {
            info!("Truncated {} result from {} to {} chars", res.name, final_output.len(), truncated.len());
        }

        // Generate summary and diff for tool result
        let (summary, diff_preview) = format_tool_result_summary(
            &res.name,
            res.success,
            &truncated,
            &res.arguments,
        );

        // Update session and emit
        self.session.add_tool_result(&res.id, &truncated, !res.success);

        // Emit tool done (ephemeral)
        self.emit(SessionOutput::tool_done(&res.id, &res.name, res.success, &truncated)).await;

        // Emit tool result (persistent message)
        self.emit(SessionOutput::tool_result(
            &res.id,
            &res.name,
            res.success,
            &truncated,
            summary,
            diff_preview,
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

    // ========================================================================
    // Context Management
    // ========================================================================

    /// Convert ChatMessages to context Messages for token counting
    ///
    /// IMPORTANT: This includes tool calls and tool responses in the content
    /// because they contribute significantly to the actual token count sent to the LLM.
    fn chat_messages_to_context_messages(&self) -> Vec<Message> {
        use crate::provider::ChatRole;

        self.session
            .messages
            .iter()
            .map(|cm| {
                let role = match cm.role {
                    ChatRole::User => MessageRole::User,
                    ChatRole::Assistant => MessageRole::Assistant,
                    ChatRole::System => MessageRole::System,
                    ChatRole::Tool => MessageRole::Tool,
                };

                // Build full content from MessageContent
                let mut full_content = cm.content.joined_texts().unwrap_or_default();

                // Add tool calls
                for tc in cm.content.tool_calls() {
                    full_content.push_str(&tc.fn_name);
                    if let Ok(json) = serde_json::to_string(&tc.fn_arguments) {
                        full_content.push_str(&json);
                    }
                }

                // Add tool responses
                for tr in cm.content.tool_responses() {
                    full_content.push_str(&tr.content);
                }

                Message::new(role, &full_content)
            })
            .collect()
    }

    /// Check context usage and compact if necessary
    ///
    /// This implements automatic context management similar to Claude Code:
    /// - Uses LLM-reported token counts for accurate tracking
    /// - If above threshold (75%), triggers auto-compaction
    /// - Uses LLM-powered summarization when possible, falls back to heuristics
    async fn check_and_compact_context(&mut self) -> Result<()> {
        // Get usage from last LLM response (stored in monitor)
        let usage = self.context_monitor.current_usage();

        // Log context usage for debugging
        info!(
            "Context usage: {:.1}% ({}/{} tokens) - input={}, output={}",
            usage.used_percentage * 100.0,
            usage.input_tokens + usage.output_tokens,
            usage.limit_tokens,
            usage.input_tokens,
            usage.output_tokens,
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

        // Convert session messages to context messages for summarization
        let messages = self.chat_messages_to_context_messages();

        // Use LLM-powered compaction for better context preservation
        let config = CompactConfig::auto();

        // Perform compaction using LLM for better context preservation
        let result = self.summarizer
            .compact(
                &messages,
                self.context_monitor.counter(),
                config,
                Some(&self.provider),
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

        // Reset token counts after compaction - next LLM response will update
        self.context_monitor.reset_tokens();

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

    /// Check if the user has requested cancellation
    ///
    /// Uses try_recv() to non-blocking check for a Cancel input.
    fn check_cancelled(&mut self) -> bool {
        loop {
            match self.answer_rx.try_recv() {
                Ok(SessionInput::Cancel) => return true,
                Ok(other) => {
                    // Log and discard unexpected inputs during cancel check
                    debug!("Discarding input during cancel check: {:?}", other);
                }
                Err(mpsc::error::TryRecvError::Empty) => return false,
                Err(mpsc::error::TryRecvError::Disconnected) => return false,
            }
        }
    }

    /// Handle cancellation cleanup - add "Cancelled" results for all pending tools
    /// to keep the message history valid for Claude's strict sequencing requirements.
    async fn handle_cancel_cleanup(
        &mut self,
        all_tool_calls: &[ToolCall],
        needs_interaction: &[&ToolCall],
        current_interaction_idx: usize,
        completed_tool_ids: &mut std::collections::HashSet<String>,
        join_set: &mut JoinSet<SpawnedToolResult>,
    ) {
        // First, collect any completed results from the JoinSet
        while let Some(result) = join_set.try_join_next() {
            if let Ok(res) = result {
                completed_tool_ids.insert(res.id.clone());
                self.finalize_spawned_tool(res).await;
            }
        }

        // Abort remaining spawned tasks
        join_set.abort_all();

        // Add "Cancelled" results for all tools that didn't complete
        for tc in all_tool_calls {
            if !completed_tool_ids.contains(&tc.call_id) {
                let cancel_msg = "Cancelled by user";
                self.session.add_tool_result(&tc.call_id, cancel_msg, true);
                self.emit(SessionOutput::tool_result(
                    &tc.call_id,
                    &tc.fn_name,
                    false,
                    cancel_msg.to_string(),
                    "Cancelled".to_string(),
                    None,
                )).await;
            }
        }

        // Clear the modal state for current and remaining interaction tools
        for tc in needs_interaction.iter().skip(current_interaction_idx) {
            debug!("Cancelled tool {} before completion", tc.call_id);
        }
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

    /// Emit tool execution start events (both ephemeral tool_start and persistent tool_call)
    async fn emit_tool_execution_start(&self, tool_call: &ToolCall) {
        let formatted = format_tool_call(&tool_call.fn_name, &tool_call.fn_arguments);

        self.emit(SessionOutput::tool_start(
            &tool_call.call_id,
            &tool_call.fn_name,
            tool_call.fn_arguments.clone(),
        ))
        .await;

        self.emit(SessionOutput::tool_call(
            &tool_call.call_id,
            &tool_call.fn_name,
            tool_call.fn_arguments.clone(),
            formatted,
        ))
        .await;
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

        let saved = SavedSession {
            id: self.session_id.clone(),
            name: format!("Session {}", self.session_id),
            messages: self.session.messages.clone(),
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
///
/// For JSON results, we truncate safely by summarizing the structure
/// rather than cutting mid-string which would produce invalid JSON.
fn truncate_tool_result(result: &str, max_size: usize) -> String {
    if result.len() <= max_size {
        return result.to_string();
    }

    let trimmed = result.trim();

    // Check if this looks like JSON
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        // Try to parse and summarize the JSON safely
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return truncate_json_value(&json, max_size);
        }
        // If parsing fails, it might be malformed JSON - fall through to line-based truncation
    }

    // For non-JSON or malformed JSON, truncate at line boundaries to avoid breaking structure
    truncate_at_line_boundary(result, max_size)
}

/// Truncate a JSON value safely, preserving valid JSON structure
fn truncate_json_value(value: &serde_json::Value, max_size: usize) -> String {
    use serde_json::Value;

    match value {
        Value::Array(arr) => {
            // For arrays, include first N elements that fit
            let mut result = Vec::new();
            let mut current_size = 2; // Account for [ ]

            for (i, item) in arr.iter().enumerate() {
                let item_str = serde_json::to_string(item).unwrap_or_default();
                let item_size = item_str.len() + 2; // +2 for comma and space

                if current_size + item_size > max_size && !result.is_empty() {
                    // Add truncation notice
                    let remaining = arr.len() - i;
                    let notice = format!(
                        "\n\n[Array truncated - showing {} of {} items, {} more not shown]",
                        i, arr.len(), remaining
                    );
                    let partial_json = serde_json::to_string_pretty(&Value::Array(result))
                        .unwrap_or_else(|_| "[]".to_string());
                    return format!("{}{}", partial_json, notice);
                }

                result.push(item.clone());
                current_size += item_size;
            }

            serde_json::to_string_pretty(&Value::Array(result)).unwrap_or_else(|_| "[]".to_string())
        }
        Value::Object(obj) => {
            // For objects, include first N key-value pairs that fit
            let mut result = serde_json::Map::new();
            let mut current_size = 2; // Account for { }

            for (i, (key, val)) in obj.iter().enumerate() {
                let pair_str = format!("\"{}\": {}", key, serde_json::to_string(val).unwrap_or_default());
                let pair_size = pair_str.len() + 2; // +2 for comma and space

                if current_size + pair_size > max_size && !result.is_empty() {
                    let remaining = obj.len() - i;
                    let notice = format!(
                        "\n\n[Object truncated - showing {} of {} keys, {} more not shown]",
                        i, obj.len(), remaining
                    );
                    let partial_json = serde_json::to_string_pretty(&Value::Object(result))
                        .unwrap_or_else(|_| "{}".to_string());
                    return format!("{}{}", partial_json, notice);
                }

                result.insert(key.clone(), val.clone());
                current_size += pair_size;
            }

            serde_json::to_string_pretty(&Value::Object(result)).unwrap_or_else(|_| "{}".to_string())
        }
        Value::String(s) => {
            // For large strings, truncate the string content
            if s.len() > max_size {
                let truncate_at = s
                    .char_indices()
                    .take_while(|(i, _)| *i < max_size - 50)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(max_size - 50);
                format!(
                    "\"{}...\"\n\n[String truncated - {} chars total]",
                    &s[..truncate_at],
                    s.len()
                )
            } else {
                serde_json::to_string(value).unwrap_or_default()
            }
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Truncate at a line boundary to avoid breaking mid-line or mid-string
fn truncate_at_line_boundary(result: &str, max_size: usize) -> String {
    let mut truncate_at = 0;
    let mut last_newline = 0;

    for (i, c) in result.char_indices() {
        if i >= max_size {
            break;
        }
        if c == '\n' {
            last_newline = i;
        }
        truncate_at = i + c.len_utf8();
    }

    // Prefer truncating at last newline if it's reasonably close
    let cut_point = if last_newline > max_size / 2 {
        last_newline
    } else {
        truncate_at
    };

    format!(
        "{}\n\n[Result truncated - {} chars total, showing first {}]",
        &result[..cut_point],
        result.len(),
        cut_point
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_tool_result_json_array() {
        // Create a large JSON array
        let items: Vec<serde_json::Value> = (0..100)
            .map(|i| serde_json::json!({"id": i, "name": format!("item_{}", i)}))
            .collect();
        let json = serde_json::to_string(&items).unwrap();

        // Truncate to a small size
        let truncated = truncate_tool_result(&json, 500);

        // Should be valid JSON or have a truncation notice
        assert!(truncated.contains("[Array truncated") || truncated.len() <= 500);
        // Should not have broken JSON (no unmatched quotes in the JSON portion)
        if let Some(json_end) = truncated.find("\n\n[Array truncated") {
            let json_part = &truncated[..json_end];
            assert!(serde_json::from_str::<serde_json::Value>(json_part).is_ok());
        }
    }

    #[test]
    fn test_truncate_tool_result_json_object() {
        // Create a large JSON object
        let mut obj = serde_json::Map::new();
        for i in 0..50 {
            obj.insert(
                format!("key_{}", i),
                serde_json::json!({"value": format!("some_long_value_{}", i)}),
            );
        }
        let json = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap();

        let truncated = truncate_tool_result(&json, 500);

        assert!(truncated.contains("[Object truncated") || truncated.len() <= 500);
    }

    #[test]
    fn test_truncate_tool_result_plain_text() {
        let text = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n";
        let repeated = text.repeat(100);

        let truncated = truncate_tool_result(&repeated, 100);

        // Should have truncation notice
        assert!(truncated.contains("[Result truncated"));
        // Should be shorter than original
        assert!(truncated.len() < repeated.len());
        // Should not cut in the middle of "line" word (indicating mid-line cut)
        let content_end = truncated.find("\n\n[Result truncated").unwrap_or(truncated.len());
        let content = &truncated[..content_end];
        // Content should not end with partial "lin" (mid-word)
        assert!(!content.ends_with("lin"), "Should not cut mid-word");
    }

    #[test]
    fn test_truncate_tool_result_small_input() {
        let small = "small result";
        let result = truncate_tool_result(small, 1000);
        assert_eq!(result, small);
    }

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
