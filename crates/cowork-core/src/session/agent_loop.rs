//! Agent Loop - Unified execution loop for CLI and UI
//!
//! The agent loop handles:
//! - Receiving user input and tool approvals
//! - Calling the LLM provider
//! - Executing tools based on approval config
//! - Emitting outputs for display
//! - Automatic context window management
//! - Saving session state on close

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

use super::approval::{
    approval_channel, ApprovalReceiver, ApprovalRequest, ApprovalResponse,
    ApprovalSender, QuestionResponse, ToolExecutionContext,
};
use super::types::{SessionConfig, SessionId, SessionInput, SessionOutput};
use super::ChatSession;
use crate::context::{compact, context_limit, usage_stats};
use crate::error::Result;
use crate::formatting::{format_tool_call, format_tool_result_summary, truncate_tool_result};
use crate::orchestration::ToolRegistryBuilder;
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

/// Execute a tool and build the result
async fn execute_tool_task(
    tool: std::sync::Arc<dyn crate::tools::Tool>,
    id: String,
    name: String,
    arguments: serde_json::Value,
    ctx: ToolExecutionContext,
) -> SpawnedToolResult {
    match tool.execute(arguments.clone(), ctx).await {
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
                let inject = output.metadata.get(crate::tools::skill::INJECT_AS_MESSAGE)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let inject_info = if inject { Some((output_str.clone(), skill_name)) } else { None };
                (inject_info, None)
            };

            SpawnedToolResult { id, name, arguments, success: true, output: output_str, inject_info, subagent_info }
        }
        Err(e) => SpawnedToolResult {
            id, name, arguments, success: false,
            output: format!("Error: {}", e),
            inject_info: None, subagent_info: None,
        }
    }
}

/// Reject all pending approval and question requests
fn reject_all_pending(
    approvals: &mut std::collections::HashMap<String, tokio::sync::oneshot::Sender<ApprovalResponse>>,
    questions: &mut std::collections::HashMap<String, tokio::sync::oneshot::Sender<QuestionResponse>>,
    reason: &str,
) {
    for (_, tx) in approvals.drain() {
        let _ = tx.send(ApprovalResponse::Rejected { reason: Some(reason.to_string()) });
    }
    for (_, tx) in questions.drain() {
        let _ = tx.send(QuestionResponse { answers: std::collections::HashMap::new() });
    }
}

use super::persistence::{get_sessions_dir, SavedSession};

/// User input with optional image attachments
type UserInput = (String, Vec<super::ImageAttachment>);

/// The unified agent loop
pub struct AgentLoop {
    /// Session identifier
    session_id: SessionId,
    /// Message receiver (user text messages with optional images)
    message_rx: mpsc::UnboundedReceiver<UserInput>,
    /// Control receiver (approvals, rejections, question answers, cancel)
    control_rx: mpsc::UnboundedReceiver<SessionInput>,
    /// Output sender
    output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
    /// Approval channel sender (passed to tools and subagents)
    approval_tx: ApprovalSender,
    /// Approval channel receiver (for handling approval requests from tools)
    approval_rx: ApprovalReceiver,
    /// LLM provider
    provider: GenAIProvider,
    /// Chat session with message history
    session: ChatSession,
    /// Tool registry
    tool_registry: ToolRegistry,
    /// Tool definitions for LLM
    tool_definitions: Vec<ToolDefinition>,
    /// Plan mode state (shared with EnterPlanMode/ExitPlanMode tools and /plan command)
    plan_mode_state: Arc<tokio::sync::RwLock<PlanModeState>>,
    /// Context limit for this provider/model
    context_limit: usize,
    /// Last input tokens from LLM response
    last_input_tokens: u64,
    /// Last output tokens from LLM response
    last_output_tokens: u64,
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
        let (control_tx, control_rx) = mpsc::unbounded_channel();

        // Create approval channel for tools to request approval
        // Subagents use parent's channel; main sessions create their own
        let (approval_tx, approval_rx) = if let Some(parent_tx) = config.parent_approval_channel {
            // Subagent: use parent's channel for tool requests
            // Create a dummy local receiver (won't receive - parent handles approvals)
            let (_dummy_tx, dummy_rx) = approval_channel();
            (parent_tx, dummy_rx)
        } else {
            // Main session: create own channel
            approval_channel()
        };

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
                        if let Err(e) = message_tx.send((content, vec![])) {
                            error!("Dispatcher: failed to send user message (receiver dropped?): {}", e);
                            break;
                        }
                    }
                    SessionInput::UserMessageWithImages { content, images } => {
                        if let Err(e) = message_tx.send((content, images)) {
                            error!("Dispatcher: failed to send user message with images (receiver dropped?): {}", e);
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
                    // All other inputs are control messages (approvals, answers, cancel)
                    input => {
                        if let Err(e) = control_tx.send(input) {
                            error!("Dispatcher: failed to send control message (receiver dropped?): {:?}", e);
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

        // Get context limit for this provider/model
        let ctx_limit = context_limit(&config.provider_id, config.model.as_deref());
        debug!(
            provider_id = %config.provider_id,
            model = ?config.model,
            context_limit = ctx_limit,
            "Context limit initialized"
        );

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
            control_rx,
            output_tx,
            approval_tx,
            approval_rx,
            provider,
            session,
            tool_registry,
            tool_definitions,
            plan_mode_state,
            context_limit: ctx_limit,
            last_input_tokens: 0,
            last_output_tokens: 0,
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
        while let Some((content, images)) = self.message_rx.recv().await {
            if let Err(e) = self.handle_user_message(content, images).await {
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
    async fn handle_user_message(
        &mut self,
        content: String,
        images: Vec<super::ImageAttachment>,
    ) -> Result<()> {
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

        // Echo the user message (with image count if any)
        let display_content = if images.is_empty() {
            content.clone()
        } else {
            format!("{} [{} image(s)]", content, images.len())
        };
        self.emit(SessionOutput::user_message(&msg_id, &display_content))
            .await;

        // Add to session (with hook context if any, and images)
        if images.is_empty() {
            self.session.add_user_message(&content_with_hooks);
        } else {
            self.session
                .add_user_message_with_images(&content_with_hooks, images);
        }

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

            // Store token counts from LLM response
            if let Some(input) = response.input_tokens {
                self.last_input_tokens = input;
            }
            if let Some(output) = response.output_tokens {
                self.last_output_tokens = output;
            }

            // Generate message ID
            let msg_id = uuid::Uuid::new_v4().to_string();

            // Emit assistant message with token usage appended to content
            let content = response.content.clone().unwrap_or_default();
            if !content.is_empty() {
                debug!(
                    input_tokens = ?response.input_tokens,
                    output_tokens = ?response.output_tokens,
                    context_limit = self.context_limit,
                    "Emitting assistant message with tokens"
                );

                self.emit(SessionOutput::assistant_message_with_tokens(
                    &msg_id,
                    &content,
                    response.input_tokens,
                    response.output_tokens,
                    Some(self.context_limit),
                ))
                .await;
            }

            // Add assistant message with tool calls
            let tool_calls = response.tool_calls.clone();
            self.session.add_assistant_message(&content, tool_calls.clone());

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                return Ok(());
            }

            // Spawn ALL tools in parallel
            let mut join_set: JoinSet<SpawnedToolResult> = JoinSet::new();
            for tool_call in &tool_calls {
                // Log warning if tool call has empty or null arguments
                if tool_call.fn_arguments.is_null() ||
                   (tool_call.fn_arguments.is_object() && tool_call.fn_arguments.as_object().map(|o| o.is_empty()).unwrap_or(false)) {
                    warn!(
                        tool_name = %tool_call.fn_name,
                        tool_id = %tool_call.call_id,
                        arguments = ?tool_call.fn_arguments,
                        "Tool call received with empty or null arguments"
                    );
                }

                // Emit tool_start (ephemeral) and tool_call (persistent) before spawning
                self.emit_tool_execution_start(tool_call).await;

                if let Some(tool) = self.tool_registry.get(&tool_call.fn_name) {
                    let id = tool_call.call_id.clone();
                    let name = tool_call.fn_name.clone();
                    let arguments = tool_call.fn_arguments.clone();
                    let ctx = ToolExecutionContext::new(
                        self.approval_tx.clone(),
                        id.clone(),
                        name.clone(),
                    );
                    join_set.spawn(execute_tool_task(tool, id, name, arguments, ctx));
                } else {
                    // Tool not found - handle immediately
                    let error_msg = format!("Unknown tool: {}", tool_call.fn_name);
                    self.session.add_tool_result(&tool_call.call_id, &error_msg, true);
                    self.emit(SessionOutput::tool_done(&tool_call.call_id, &tool_call.fn_name, false, error_msg)).await;
                }
            }

            // Track pending approval/question requests by ID
            let mut pending_approvals: std::collections::HashMap<String, tokio::sync::oneshot::Sender<ApprovalResponse>> = std::collections::HashMap::new();
            let mut pending_questions: std::collections::HashMap<String, tokio::sync::oneshot::Sender<QuestionResponse>> = std::collections::HashMap::new();

            // Track completed tool IDs for cancel cleanup
            let mut completed_tool_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

            // Process tools: single select! loop handles everything
            loop {
                tokio::select! {
                    // Handle tool completion
                    result = join_set.join_next() => {
                        match result {
                            Some(Ok(res)) => {
                                completed_tool_ids.insert(res.id.clone());
                                self.finalize_spawned_tool(res).await;
                            }
                            Some(Err(e)) => {
                                error!("Tool task failed: {:?}", e);
                            }
                            None => {
                                // JoinSet is empty - all tools completed
                                break;
                            }
                        }
                    }

                    // Handle approval/question requests from tools
                    request = self.approval_rx.recv() => {
                        match request {
                            Some(ApprovalRequest::ToolApproval { tool_call_id, tool_name, arguments, description, response_tx }) => {
                                // Store oneshot and emit pending event
                                pending_approvals.insert(tool_call_id.clone(), response_tx);
                                self.emit(SessionOutput::tool_pending(&tool_call_id, &tool_name, arguments, description)).await;
                            }
                            Some(ApprovalRequest::Question { request_id, questions, response_tx }) => {
                                // Store oneshot and emit question event
                                pending_questions.insert(request_id.clone(), response_tx);
                                self.emit(SessionOutput::Question {
                                    request_id,
                                    questions,
                                    subagent_id: None,
                                }).await;
                            }
                            None => {
                                error!("Approval channel closed unexpectedly");
                            }
                        }
                    }

                    // Handle control messages (approvals, answers, cancel)
                    input = self.control_rx.recv() => {
                        match input {
                            Some(SessionInput::ApproveTool { tool_call_id }) => {
                                if let Some(tx) = pending_approvals.remove(&tool_call_id) {
                                    let _ = tx.send(ApprovalResponse::Approved);
                                } else {
                                    warn!("Received approval for unknown tool_call_id: {}", tool_call_id);
                                }
                            }
                            Some(SessionInput::RejectTool { tool_call_id, reason }) => {
                                if let Some(tx) = pending_approvals.remove(&tool_call_id) {
                                    let _ = tx.send(ApprovalResponse::Rejected { reason });
                                } else {
                                    warn!("Received rejection for unknown tool_call_id: {}", tool_call_id);
                                }
                            }
                            Some(SessionInput::AnswerQuestion { request_id, answers }) => {
                                if let Some(tx) = pending_questions.remove(&request_id) {
                                    let _ = tx.send(QuestionResponse { answers });
                                } else {
                                    warn!("Received answer for unknown request_id: {}", request_id);
                                }
                            }
                            Some(SessionInput::Cancel) => {
                                reject_all_pending(&mut pending_approvals, &mut pending_questions, "Cancelled by user");
                                self.handle_cancel_cleanup(&tool_calls, &mut completed_tool_ids, &mut join_set).await;
                                self.emit(SessionOutput::cancelled()).await;
                                return Ok(());
                            }
                            Some(other) => {
                                debug!("Unexpected control input: {:?}", other);
                            }
                            None => {
                                reject_all_pending(&mut pending_approvals, &mut pending_questions, "Session ended");
                                self.emit(SessionOutput::error("Session interrupted".to_string())).await;
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
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

    // ========================================================================
    // Context Management
    // ========================================================================

    /// Check context usage and compact if necessary
    ///
    /// This implements automatic context management similar to Claude Code:
    /// - Uses LLM-reported token counts for accurate tracking
    /// - If above threshold (75%), triggers auto-compaction
    /// - Uses LLM-powered summarization when possible, falls back to heuristics
    async fn check_and_compact_context(&mut self) -> Result<()> {
        // Get usage stats from stored token counts
        let usage = usage_stats(self.last_input_tokens, self.last_output_tokens, self.context_limit);

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

        // Perform compaction using LLM
        let result = compact(&self.session.messages, None, &self.provider).await?;

        info!(
            "Compaction complete: {} -> {} chars ({} messages summarized)",
            result.chars_before,
            result.chars_after,
            result.messages_summarized
        );

        // Replace session messages with compacted version
        self.apply_compaction_result(&result);

        // Reset token counts - next LLM response will update
        self.last_input_tokens = 0;
        self.last_output_tokens = 0;

        // Emit completion notification
        self.emit(SessionOutput::thinking(format!(
            "Compacted {} messages into summary ({} -> {} chars)",
            result.messages_summarized, result.chars_before, result.chars_after
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
        self.session.messages.push(ChatMessage::user(&result.summary));
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

    /// Handle cancellation cleanup
    ///
    /// Collects any completed results, aborts remaining tasks, and adds "Cancelled"
    /// results for all tools that didn't complete.
    async fn handle_cancel_cleanup(
        &mut self,
        all_tool_calls: &[ToolCall],
        completed_tool_ids: &mut std::collections::HashSet<String>,
        join_set: &mut JoinSet<SpawnedToolResult>,
    ) {
        // Collect any completed results from the JoinSet
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

#[cfg(test)]
mod tests {
    use crate::approval::ToolApprovalConfig;

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
