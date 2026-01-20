//! Agentic Loop - Continuous execution loop that runs until task complete
//!
//! The agentic loop is the core execution engine that:
//! - Continuously processes LLM responses
//! - Automatically executes safe/read-only tools
//! - Pauses for user approval on destructive tools
//! - Emits events to the frontend for real-time updates
//! - Monitors context usage and triggers auto-compact when needed

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};

use cowork_core::context::{
    CompactConfig, CompactResult, ContextMonitor, ContextUsage, ConversationSummarizer,
    Message, MessageRole, MonitorConfig, SummarizerConfig,
};
use cowork_core::provider::{LlmMessage, LlmRequest, ProviderType};
// Use shared approval config from cowork-core
use cowork_core::ToolApprovalConfig;

use crate::chat::{ChatMessage, ChatSession, ToolCallInfo, ToolCallStatus};

/// Loop states for the agentic execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopState {
    /// Loop is not running
    Idle,
    /// Waiting for LLM response
    WaitingForLlm,
    /// Waiting for user approval on tool calls
    WaitingForApproval,
    /// Waiting for user to answer a question
    WaitingForQuestion,
    /// Executing approved tools
    ExecutingTools,
    /// Loop completed successfully
    Completed,
    /// Loop was cancelled
    Cancelled,
    /// Loop encountered an error
    Error,
}

/// A question option for user interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

/// A question to ask the user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserQuestion {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

/// Events emitted by the agentic loop to the frontend
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoopEvent {
    /// Loop state changed
    StateChanged {
        session_id: String,
        state: LoopState,
    },
    /// New text chunk from LLM (for streaming)
    TextDelta {
        session_id: String,
        delta: String,
    },
    /// New message added
    MessageAdded {
        session_id: String,
        message: ChatMessage,
    },
    /// Tool call needs approval
    ToolApprovalNeeded {
        session_id: String,
        tool_calls: Vec<ToolCallInfo>,
    },
    /// Tool execution started
    ToolExecutionStarted {
        session_id: String,
        tool_call_id: String,
        tool_name: String,
    },
    /// Tool execution completed
    ToolExecutionCompleted {
        session_id: String,
        tool_call_id: String,
        result: String,
        success: bool,
    },
    /// AI is asking the user a question
    QuestionRequested {
        session_id: String,
        request_id: String,
        tool_call_id: String,
        questions: Vec<UserQuestion>,
    },
    /// Loop completed
    LoopCompleted {
        session_id: String,
    },
    /// Loop error
    LoopError {
        session_id: String,
        error: String,
    },
    /// Context usage update
    ContextUsage {
        session_id: String,
        usage: ContextUsage,
    },
    /// Auto-compact started
    AutoCompactStarted {
        session_id: String,
        tokens_before: usize,
    },
    /// Auto-compact completed
    AutoCompactCompleted {
        session_id: String,
        tokens_before: usize,
        tokens_after: usize,
        messages_removed: usize,
    },
}

/// Tool approval decision from the user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalDecision {
    /// Approve all pending tools
    ApproveAll,
    /// Approve specific tools
    ApproveSelected(Vec<String>),
    /// Reject all pending tools
    RejectAll,
    /// Reject specific tools
    RejectSelected(Vec<String>),
    /// Cancel the entire loop
    Cancel,
}

/// User's answer to questions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub request_id: String,
    /// Map of question index to answer(s)
    pub answers: std::collections::HashMap<String, String>,
}

/// Control commands for the agentic loop
#[derive(Debug)]
pub enum LoopCommand {
    /// User's approval decision
    Approval(ApprovalDecision),
    /// User's answer to questions
    QuestionAnswer(QuestionAnswer),
    /// Cancel the loop
    Cancel,
    /// Pause the loop
    Pause,
    /// Resume the loop
    Resume,
}

/// Re-export ApprovalLevel for use by other modules
pub use cowork_core::ApprovalLevel;

/// Type alias for backward compatibility with existing UI code
pub type ApprovalConfig = ToolApprovalConfig;

/// Extension trait to add UI-specific categorize_tools method
pub trait ApprovalConfigExt {
    /// Categorize tool calls into auto-approve and need-approval
    fn categorize_tools(&self, tool_calls: &[ToolCallInfo]) -> (Vec<String>, Vec<String>);
}

impl ApprovalConfigExt for ToolApprovalConfig {
    fn categorize_tools(&self, tool_calls: &[ToolCallInfo]) -> (Vec<String>, Vec<String>) {
        let mut auto_approved = Vec::new();
        let mut needs_approval = Vec::new();

        for tc in tool_calls {
            if self.should_auto_approve(&tc.name) {
                auto_approved.push(tc.id.clone());
            } else {
                needs_approval.push(tc.id.clone());
            }
        }

        (auto_approved, needs_approval)
    }
}

/// Configuration for context management in the loop
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// Whether auto-compact is enabled
    pub auto_compact_enabled: bool,
    /// Configuration for the context monitor
    pub monitor_config: MonitorConfig,
    /// Configuration for the summarizer
    pub summarizer_config: SummarizerConfig,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            auto_compact_enabled: true,
            monitor_config: MonitorConfig::default(),
            summarizer_config: SummarizerConfig::default(),
        }
    }
}

/// The agentic loop executor
pub struct AgenticLoop {
    session_id: String,
    app_handle: AppHandle,
    state: Arc<RwLock<LoopState>>,
    approval_config: ApprovalConfig,
    command_rx: mpsc::Receiver<LoopCommand>,
    command_tx: mpsc::Sender<LoopCommand>,
    max_iterations: usize,
    // Context management
    context_config: ContextConfig,
    context_monitor: Option<ContextMonitor>,
    summarizer: ConversationSummarizer,
}

impl AgenticLoop {
    /// Create a new agentic loop for a session
    pub fn new(session_id: String, app_handle: AppHandle, approval_config: ApprovalConfig) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);
        let context_config = ContextConfig::default();
        let summarizer = ConversationSummarizer::new(context_config.summarizer_config.clone());

        Self {
            session_id,
            app_handle,
            state: Arc::new(RwLock::new(LoopState::Idle)),
            approval_config,
            command_rx,
            command_tx,
            max_iterations: 100, // Safety limit
            context_config,
            context_monitor: None,
            summarizer,
        }
    }

    /// Create a new agentic loop with custom context configuration
    pub fn with_context_config(
        session_id: String,
        app_handle: AppHandle,
        approval_config: ApprovalConfig,
        context_config: ContextConfig,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);
        let summarizer = ConversationSummarizer::new(context_config.summarizer_config.clone());

        Self {
            session_id,
            app_handle,
            state: Arc::new(RwLock::new(LoopState::Idle)),
            approval_config,
            command_rx,
            command_tx,
            max_iterations: 100,
            context_config,
            context_monitor: None,
            summarizer,
        }
    }

    /// Set the provider type for context monitoring
    pub fn set_provider_type(&mut self, provider_type: ProviderType) {
        self.context_monitor = Some(ContextMonitor::with_config(
            provider_type,
            self.context_config.monitor_config.clone(),
        ));
    }

    /// Enable or disable auto-compact
    pub fn set_auto_compact(&mut self, enabled: bool) {
        self.context_config.auto_compact_enabled = enabled;
    }

    /// Check context usage and perform auto-compact if needed
    ///
    /// Returns true if auto-compact was performed
    async fn check_and_compact_context(&mut self, session: &mut ChatSession) -> Result<bool, String> {
        // First, check if we should evaluate context (periodic check)
        let should_check = {
            match &mut self.context_monitor {
                Some(m) => m.should_check(),
                None => return Ok(false),
            }
        };

        if !should_check {
            return Ok(false);
        }

        // Convert ChatMessages to context Messages for the monitor
        let context_messages: Vec<Message> = session
            .messages
            .iter()
            .map(|m| Message {
                role: match m.role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "system" => MessageRole::System,
                    _ => MessageRole::Tool,
                },
                content: m.content.clone(),
                timestamp: m.timestamp,
            })
            .collect();

        // Calculate current usage (immutable borrow is fine here)
        let usage = {
            let monitor = self.context_monitor.as_ref().unwrap();
            monitor.calculate_usage(&context_messages, &session.system_prompt, None)
        };

        // Emit context usage event
        self.emit_event(LoopEvent::ContextUsage {
            session_id: self.session_id.clone(),
            usage: usage.clone(),
        });

        // Check if auto-compact should trigger
        if !usage.should_compact || !self.context_config.auto_compact_enabled {
            return Ok(false);
        }

        // Perform auto-compact
        tracing::info!(
            "Auto-compact triggered: {:.1}% context used ({} tokens)",
            usage.used_percentage * 100.0,
            usage.used_tokens
        );

        self.emit_event(LoopEvent::AutoCompactStarted {
            session_id: self.session_id.clone(),
            tokens_before: usage.used_tokens,
        });

        // Compact using simple summarization (no LLM to avoid recursion)
        let compact_config = CompactConfig::auto().without_llm();

        // Get counter reference for compaction
        let compact_result = {
            let monitor = self.context_monitor.as_ref().unwrap();
            self.summarizer
                .compact(&context_messages, monitor.counter(), compact_config, None)
                .await
                .map_err(|e| e.to_string())?
        };

        // Apply compaction to session
        self.apply_compact_result(session, &compact_result);

        self.emit_event(LoopEvent::AutoCompactCompleted {
            session_id: self.session_id.clone(),
            tokens_before: compact_result.tokens_before,
            tokens_after: compact_result.tokens_after,
            messages_removed: compact_result.messages_summarized,
        });

        tracing::info!(
            "Auto-compact complete: {} -> {} tokens ({} messages summarized)",
            compact_result.tokens_before,
            compact_result.tokens_after,
            compact_result.messages_summarized
        );

        Ok(true)
    }

    /// Apply compact result to the session
    fn apply_compact_result(&self, session: &mut ChatSession, result: &CompactResult) {
        // Create summary as a system message
        let summary_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "system".to_string(),
            content: result.summary.content.clone(),
            tool_calls: Vec::new(),
            timestamp: result.summary.timestamp,
        };

        // Convert kept messages back to ChatMessages
        let kept_chat_messages: Vec<ChatMessage> = result
            .kept_messages
            .iter()
            .map(|m| ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                    MessageRole::Tool => "tool",
                }
                .to_string(),
                content: m.content.clone(),
                tool_calls: Vec::new(),
                timestamp: m.timestamp,
            })
            .collect();

        // Replace session messages: summary + kept messages
        session.messages.clear();
        session.messages.push(summary_msg);
        session.messages.extend(kept_chat_messages);
    }

    /// Get a sender for commands
    pub fn command_sender(&self) -> mpsc::Sender<LoopCommand> {
        self.command_tx.clone()
    }

    /// Get current state
    pub async fn state(&self) -> LoopState {
        *self.state.read().await
    }

    /// Emit an event to the frontend
    fn emit_event(&self, event: LoopEvent) {
        let event_name = format!("loop:{}", self.session_id);
        if let Err(e) = self.app_handle.emit(&event_name, &event) {
            tracing::error!("Failed to emit loop event: {}", e);
        }
    }

    /// Set state and emit event
    async fn set_state(&self, new_state: LoopState) {
        *self.state.write().await = new_state;
        self.emit_event(LoopEvent::StateChanged {
            session_id: self.session_id.clone(),
            state: new_state,
        });
    }

    /// Run the agentic loop
    ///
    /// This continuously processes LLM responses and tool executions until:
    /// - The LLM returns a final message with no tool calls
    /// - The user cancels the loop
    /// - An error occurs
    /// - The max iteration limit is reached
    pub async fn run(
        &mut self,
        session: &mut ChatSession,
        initial_prompt: String,
        tool_executor: impl ToolExecutor,
    ) -> Result<(), String> {
        self.set_state(LoopState::WaitingForLlm).await;

        // Send initial prompt
        let mut last_response = session.send_message(initial_prompt).await?;
        self.emit_event(LoopEvent::MessageAdded {
            session_id: self.session_id.clone(),
            message: last_response.clone(),
        });

        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > self.max_iterations {
                self.set_state(LoopState::Error).await;
                return Err("Max iteration limit reached".to_string());
            }

            // Check context usage and auto-compact if needed
            if let Err(e) = self.check_and_compact_context(session).await {
                tracing::warn!("Context check failed: {}", e);
            }

            // Check if there are pending tool calls
            if last_response.tool_calls.is_empty() {
                // No tool calls - loop is complete
                self.set_state(LoopState::Completed).await;
                self.emit_event(LoopEvent::LoopCompleted {
                    session_id: self.session_id.clone(),
                });
                return Ok(());
            }

            // Categorize tool calls
            let pending_tools: Vec<_> = last_response
                .tool_calls
                .iter()
                .filter(|tc| matches!(tc.status, ToolCallStatus::Pending))
                .cloned()
                .collect();

            if pending_tools.is_empty() {
                // All tools processed, get next response
                self.set_state(LoopState::WaitingForLlm).await;
                last_response = self.get_next_response(session).await?;
                continue;
            }

            let (auto_approved, needs_approval) = self.approval_config.categorize_tools(&pending_tools);

            // Check for ask_user_question tools - handle them specially
            let question_tools: Vec<_> = pending_tools
                .iter()
                .filter(|tc| tc.name == "ask_user_question" && auto_approved.contains(&tc.id))
                .cloned()
                .collect();

            // Handle question tools first (they require user interaction)
            for tool_call in &question_tools {
                match self.execute_ask_user_question(tool_call).await {
                    Ok(result) => {
                        // Update session with result
                        if let Err(e) = session
                            .execute_tool_call(&tool_call.id, result.clone())
                            .await
                        {
                            tracing::error!("Failed to update tool result: {}", e);
                        }
                        self.emit_event(LoopEvent::ToolExecutionCompleted {
                            session_id: self.session_id.clone(),
                            tool_call_id: tool_call.id.clone(),
                            result,
                            success: true,
                        });
                    }
                    Err(e) => {
                        tracing::error!("Question tool error: {}", e);
                        if let Err(err) = session
                            .execute_tool_call(&tool_call.id, format!("Error: {}", e))
                            .await
                        {
                            tracing::error!("Failed to update tool error: {}", err);
                        }
                    }
                }
            }

            // Execute other auto-approved tools (not ask_user_question)
            let other_auto_approved: Vec<_> = auto_approved
                .iter()
                .filter(|id| !question_tools.iter().any(|q| &q.id == *id))
                .cloned()
                .collect();

            if !other_auto_approved.is_empty() {
                self.set_state(LoopState::ExecutingTools).await;
                for tool_id in &other_auto_approved {
                    if let Err(e) = self
                        .execute_tool(session, tool_id, &tool_executor)
                        .await
                    {
                        tracing::error!("Tool execution error: {}", e);
                    }
                }
            }

            // If there are tools needing approval, wait for user decision
            if !needs_approval.is_empty() {
                self.set_state(LoopState::WaitingForApproval).await;

                let approval_needed: Vec<_> = pending_tools
                    .iter()
                    .filter(|tc| needs_approval.contains(&tc.id))
                    .cloned()
                    .collect();

                self.emit_event(LoopEvent::ToolApprovalNeeded {
                    session_id: self.session_id.clone(),
                    tool_calls: approval_needed,
                });

                // Wait for approval decision
                match self.wait_for_approval().await {
                    Ok(decision) => match decision {
                        ApprovalDecision::ApproveAll => {
                            self.set_state(LoopState::ExecutingTools).await;
                            for tool_id in &needs_approval {
                                if let Err(e) = self
                                    .execute_tool(session, tool_id, &tool_executor)
                                    .await
                                {
                                    tracing::error!("Tool execution error: {}", e);
                                }
                            }
                        }
                        ApprovalDecision::ApproveSelected(ids) => {
                            self.set_state(LoopState::ExecutingTools).await;
                            for tool_id in ids.iter().filter(|id| needs_approval.contains(id)) {
                                if let Err(e) = self
                                    .execute_tool(session, tool_id, &tool_executor)
                                    .await
                                {
                                    tracing::error!("Tool execution error: {}", e);
                                }
                            }
                            // Mark rejected tools
                            self.mark_rejected(session, &needs_approval.into_iter()
                                .filter(|id| !ids.contains(id))
                                .collect::<Vec<_>>())
                                .await;
                        }
                        ApprovalDecision::RejectAll => {
                            self.mark_rejected(session, &needs_approval).await;
                        }
                        ApprovalDecision::RejectSelected(ids) => {
                            self.mark_rejected(session, &ids).await;
                            // Execute the rest
                            self.set_state(LoopState::ExecutingTools).await;
                            for tool_id in needs_approval.iter().filter(|id| !ids.contains(id)) {
                                if let Err(e) = self
                                    .execute_tool(session, tool_id, &tool_executor)
                                    .await
                                {
                                    tracing::error!("Tool execution error: {}", e);
                                }
                            }
                        }
                        ApprovalDecision::Cancel => {
                            self.set_state(LoopState::Cancelled).await;
                            return Ok(());
                        }
                    },
                    Err(e) => {
                        self.set_state(LoopState::Error).await;
                        self.emit_event(LoopEvent::LoopError {
                            session_id: self.session_id.clone(),
                            error: e.clone(),
                        });
                        return Err(e);
                    }
                }
            }

            // Get next LLM response
            self.set_state(LoopState::WaitingForLlm).await;
            last_response = self.get_next_response(session).await?;
            self.emit_event(LoopEvent::MessageAdded {
                session_id: self.session_id.clone(),
                message: last_response.clone(),
            });
        }
    }

    /// Execute a single tool
    async fn execute_tool(
        &self,
        session: &mut ChatSession,
        tool_call_id: &str,
        executor: &impl ToolExecutor,
    ) -> Result<(), String> {
        // Find the tool call
        let tool_call = session
            .messages
            .iter()
            .flat_map(|m| m.tool_calls.iter())
            .find(|tc| tc.id == tool_call_id)
            .cloned()
            .ok_or_else(|| format!("Tool call {} not found", tool_call_id))?;

        self.emit_event(LoopEvent::ToolExecutionStarted {
            session_id: self.session_id.clone(),
            tool_call_id: tool_call_id.to_string(),
            tool_name: tool_call.name.clone(),
        });

        // Execute the tool
        let result = executor.execute(&tool_call).await;

        let (result_str, success) = match result {
            Ok(output) => (output, true),
            Err(e) => (e.clone(), false),
        };

        // Update session with result
        session
            .execute_tool_call(tool_call_id, result_str.clone())
            .await?;

        self.emit_event(LoopEvent::ToolExecutionCompleted {
            session_id: self.session_id.clone(),
            tool_call_id: tool_call_id.to_string(),
            result: result_str,
            success,
        });

        Ok(())
    }

    /// Mark tool calls as rejected
    async fn mark_rejected(&self, session: &mut ChatSession, tool_ids: &[String]) {
        for msg in &mut session.messages {
            for tc in &mut msg.tool_calls {
                if tool_ids.contains(&tc.id) {
                    tc.status = ToolCallStatus::Rejected;
                }
            }
        }
    }

    /// Get the next LLM response after tool executions
    async fn get_next_response(&self, session: &mut ChatSession) -> Result<ChatMessage, String> {
        // Build the request with updated messages
        let llm_messages: Vec<LlmMessage> = session
            .messages
            .iter()
            .map(|m| LlmMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        let request = LlmRequest::new(llm_messages)
            .with_system(&session.system_prompt)
            .with_tools(session.available_tools.clone())
            .with_max_tokens(4096);

        let response = session
            .provider
            .complete(request)
            .await
            .map_err(|e| e.to_string())?;

        let tool_calls: Vec<ToolCallInfo> = response
            .tool_calls
            .iter()
            .map(|tc| ToolCallInfo {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                status: ToolCallStatus::Pending,
                result: None,
            })
            .collect();

        let assistant_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: response.content.unwrap_or_default(),
            tool_calls,
            timestamp: chrono::Utc::now(),
        };

        session.messages.push(assistant_msg.clone());
        Ok(assistant_msg)
    }

    /// Wait for user approval decision
    async fn wait_for_approval(&mut self) -> Result<ApprovalDecision, String> {
        // Wait for command from user
        loop {
            tokio::select! {
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(LoopCommand::Approval(decision)) => return Ok(decision),
                        Some(LoopCommand::Cancel) => return Ok(ApprovalDecision::Cancel),
                        Some(LoopCommand::Pause) => {
                            // Stay in waiting state
                            continue;
                        }
                        Some(LoopCommand::Resume) => {
                            // Continue waiting
                            continue;
                        }
                        Some(LoopCommand::QuestionAnswer(_)) => {
                            // Unexpected during approval, ignore
                            continue;
                        }
                        None => return Err("Command channel closed".to_string()),
                    }
                }
            }
        }
    }

    /// Wait for user to answer a question
    async fn wait_for_question_answer(&mut self, request_id: &str) -> Result<QuestionAnswer, String> {
        loop {
            tokio::select! {
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(LoopCommand::QuestionAnswer(answer)) => {
                            if answer.request_id == request_id {
                                return Ok(answer);
                            }
                            // Wrong request_id, keep waiting
                            continue;
                        }
                        Some(LoopCommand::Cancel) => {
                            return Err("User cancelled".to_string());
                        }
                        Some(LoopCommand::Pause) | Some(LoopCommand::Resume) => {
                            continue;
                        }
                        Some(LoopCommand::Approval(_)) => {
                            // Unexpected during question, ignore
                            continue;
                        }
                        None => return Err("Command channel closed".to_string()),
                    }
                }
            }
        }
    }

    /// Execute the ask_user_question tool
    async fn execute_ask_user_question(&mut self, tool_call: &ToolCallInfo) -> Result<String, String> {
        // Parse questions from arguments
        let questions_value = tool_call.arguments.get("questions")
            .ok_or_else(|| "Missing questions field".to_string())?;

        let questions: Vec<UserQuestion> = questions_value.as_array()
            .ok_or_else(|| "questions must be an array".to_string())?
            .iter()
            .map(|q| {
                let options = q.get("options")
                    .and_then(|o| o.as_array())
                    .map(|arr| {
                        arr.iter().map(|opt| QuestionOption {
                            label: opt.get("label").and_then(|l| l.as_str()).unwrap_or("").to_string(),
                            description: opt.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                        }).collect()
                    })
                    .unwrap_or_default();

                UserQuestion {
                    question: q.get("question").and_then(|q| q.as_str()).unwrap_or("").to_string(),
                    header: q.get("header").and_then(|h| h.as_str()).unwrap_or("").to_string(),
                    options,
                    multi_select: q.get("multiSelect").and_then(|m| m.as_bool()).unwrap_or(false),
                }
            })
            .collect();

        // Generate request ID
        let request_id = uuid::Uuid::new_v4().to_string();

        // Set state to waiting for question
        self.set_state(LoopState::WaitingForQuestion).await;

        // Emit question event to frontend
        self.emit_event(LoopEvent::QuestionRequested {
            session_id: self.session_id.clone(),
            request_id: request_id.clone(),
            tool_call_id: tool_call.id.clone(),
            questions,
        });

        // Wait for answer
        let answer = self.wait_for_question_answer(&request_id).await?;

        // Return the answer as JSON
        let result = serde_json::json!({
            "answered": true,
            "answers": answer.answers
        });

        Ok(serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string()))
    }
}

use std::future::Future;
use std::pin::Pin;

/// Type alias for boxed futures
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trait for executing tools
pub trait ToolExecutor: Send + Sync {
    fn execute<'a>(&'a self, tool_call: &'a ToolCallInfo) -> BoxFuture<'a, Result<String, String>>;
}

/// Default tool executor that handles built-in tools
pub struct DefaultToolExecutor {
    workspace_path: std::path::PathBuf,
}

impl DefaultToolExecutor {
    pub fn new(workspace_path: std::path::PathBuf) -> Self {
        Self { workspace_path }
    }
}

impl ToolExecutor for DefaultToolExecutor {
    fn execute<'a>(&'a self, tool_call: &'a ToolCallInfo) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            match tool_call.name.as_str() {
                "read_file" => {
                    let path = tool_call.arguments["path"]
                        .as_str()
                        .ok_or("Missing path parameter")?;
                    let full_path = self.workspace_path.join(path);
                    tokio::fs::read_to_string(&full_path)
                        .await
                        .map_err(|e| e.to_string())
                }
                "write_file" => {
                    let path = tool_call.arguments["path"]
                        .as_str()
                        .ok_or("Missing path parameter")?;
                    let content = tool_call.arguments["content"]
                        .as_str()
                        .ok_or("Missing content parameter")?;
                    let full_path = self.workspace_path.join(path);
                    tokio::fs::write(&full_path, content)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(format!("Successfully wrote to {}", path))
                }
                "list_directory" => {
                    let path = tool_call.arguments["path"].as_str().unwrap_or(".");
                    let full_path = self.workspace_path.join(path);
                    let mut entries = Vec::new();
                    let mut dir = tokio::fs::read_dir(&full_path)
                        .await
                        .map_err(|e| e.to_string())?;
                    while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
                        let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                        let name = entry.file_name().to_string_lossy().to_string();
                        entries.push(if is_dir {
                            format!("{}/", name)
                        } else {
                            name
                        });
                    }
                    Ok(entries.join("\n"))
                }
                "execute_command" => {
                    let command = tool_call.arguments["command"]
                        .as_str()
                        .ok_or("Missing command parameter")?;
                    let working_dir = tool_call.arguments["working_dir"]
                        .as_str()
                        .map(|d| self.workspace_path.join(d))
                        .unwrap_or_else(|| self.workspace_path.clone());

                    #[cfg(windows)]
                    let output = tokio::process::Command::new("cmd")
                        .args(["/C", command])
                        .current_dir(&working_dir)
                        .output()
                        .await
                        .map_err(|e| e.to_string())?;

                    #[cfg(not(windows))]
                    let output = tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(command)
                        .current_dir(&working_dir)
                        .output()
                        .await
                        .map_err(|e| e.to_string())?;

                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let exit_code = output.status.code().unwrap_or(-1);

                    Ok(format!(
                        "Exit code: {}\nStdout:\n{}\nStderr:\n{}",
                        exit_code, stdout, stderr
                    ))
                }
                "search_files" => {
                    let pattern = tool_call.arguments["pattern"]
                        .as_str()
                        .ok_or("Missing pattern parameter")?;
                    let path = tool_call.arguments["path"].as_str().unwrap_or(".");
                    let full_path = self.workspace_path.join(path);

                    let glob_pattern = full_path.join(pattern).to_string_lossy().to_string();
                    let matches: Vec<String> = glob::glob(&glob_pattern)
                        .map_err(|e| e.to_string())?
                        .filter_map(|entry| entry.ok())
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();

                    if matches.is_empty() {
                        Ok("No files found matching the pattern".to_string())
                    } else {
                        Ok(matches.join("\n"))
                    }
                }
                _ => Err(format!("Unknown tool: {}", tool_call.name)),
            }
        })
    }
}

/// Handles for controlling an active loop
#[derive(Clone)]
pub struct LoopHandle {
    session_id: String,
    command_tx: mpsc::Sender<LoopCommand>,
}

impl LoopHandle {
    pub fn new(session_id: String, command_tx: mpsc::Sender<LoopCommand>) -> Self {
        Self {
            session_id,
            command_tx,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub async fn approve_all(&self) -> Result<(), String> {
        self.command_tx
            .send(LoopCommand::Approval(ApprovalDecision::ApproveAll))
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn approve_selected(&self, tool_ids: Vec<String>) -> Result<(), String> {
        self.command_tx
            .send(LoopCommand::Approval(ApprovalDecision::ApproveSelected(
                tool_ids,
            )))
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn reject_all(&self) -> Result<(), String> {
        self.command_tx
            .send(LoopCommand::Approval(ApprovalDecision::RejectAll))
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn reject_selected(&self, tool_ids: Vec<String>) -> Result<(), String> {
        self.command_tx
            .send(LoopCommand::Approval(ApprovalDecision::RejectSelected(
                tool_ids,
            )))
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn cancel(&self) -> Result<(), String> {
        self.command_tx
            .send(LoopCommand::Cancel)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn answer_question(&self, answer: QuestionAnswer) -> Result<(), String> {
        self.command_tx
            .send(LoopCommand::QuestionAnswer(answer))
            .await
            .map_err(|e| e.to_string())
    }
}
