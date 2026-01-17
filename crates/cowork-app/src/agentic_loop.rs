//! Agentic Loop - Continuous execution loop that runs until task complete
//!
//! The agentic loop is the core execution engine that:
//! - Continuously processes LLM responses
//! - Automatically executes safe/read-only tools
//! - Pauses for user approval on destructive tools
//! - Emits events to the frontend for real-time updates

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};

use cowork_core::provider::{LlmMessage, LlmProvider, LlmRequest};
use cowork_core::tools::ToolDefinition;

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
    /// Executing approved tools
    ExecutingTools,
    /// Loop completed successfully
    Completed,
    /// Loop was cancelled
    Cancelled,
    /// Loop encountered an error
    Error,
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
    /// Loop completed
    LoopCompleted {
        session_id: String,
    },
    /// Loop error
    LoopError {
        session_id: String,
        error: String,
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

/// Control commands for the agentic loop
#[derive(Debug)]
pub enum LoopCommand {
    /// User's approval decision
    Approval(ApprovalDecision),
    /// Cancel the loop
    Cancel,
    /// Pause the loop
    Pause,
    /// Resume the loop
    Resume,
}

/// Configuration for auto-approval of tools
#[derive(Debug, Clone)]
pub struct ApprovalConfig {
    /// Tools that are automatically approved (read-only, safe)
    pub auto_approve: HashSet<String>,
    /// Tools that always require approval (destructive)
    pub always_require_approval: HashSet<String>,
    /// Auto-approve level: none, low, medium, high
    pub level: ApprovalLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalLevel {
    /// No auto-approval - all tools require approval
    None,
    /// Low - only read operations auto-approved
    Low,
    /// Medium - read and list operations auto-approved
    Medium,
    /// High - most operations auto-approved except destructive
    High,
}

impl Default for ApprovalConfig {
    fn default() -> Self {
        // Default: auto-approve read-only tools
        let mut auto_approve = HashSet::new();
        auto_approve.insert("read_file".to_string());
        auto_approve.insert("list_directory".to_string());
        auto_approve.insert("search_files".to_string());
        auto_approve.insert("glob".to_string());
        auto_approve.insert("grep".to_string());

        let mut always_require = HashSet::new();
        always_require.insert("write_file".to_string());
        always_require.insert("execute_command".to_string());
        always_require.insert("edit".to_string());
        always_require.insert("delete_file".to_string());

        Self {
            auto_approve,
            always_require_approval: always_require,
            level: ApprovalLevel::Low,
        }
    }
}

impl ApprovalConfig {
    /// Check if a tool should be auto-approved
    pub fn should_auto_approve(&self, tool_name: &str) -> bool {
        match self.level {
            ApprovalLevel::None => false,
            ApprovalLevel::Low => self.auto_approve.contains(tool_name),
            ApprovalLevel::Medium => {
                self.auto_approve.contains(tool_name)
                    && !self.always_require_approval.contains(tool_name)
            }
            ApprovalLevel::High => !self.always_require_approval.contains(tool_name),
        }
    }

    /// Categorize tool calls into auto-approve and need-approval
    pub fn categorize_tools(&self, tool_calls: &[ToolCallInfo]) -> (Vec<String>, Vec<String>) {
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

/// The agentic loop executor
pub struct AgenticLoop {
    session_id: String,
    app_handle: AppHandle,
    state: Arc<RwLock<LoopState>>,
    approval_config: ApprovalConfig,
    command_rx: mpsc::Receiver<LoopCommand>,
    command_tx: mpsc::Sender<LoopCommand>,
    max_iterations: usize,
}

impl AgenticLoop {
    /// Create a new agentic loop for a session
    pub fn new(session_id: String, app_handle: AppHandle, approval_config: ApprovalConfig) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);

        Self {
            session_id,
            app_handle,
            state: Arc::new(RwLock::new(LoopState::Idle)),
            approval_config,
            command_rx,
            command_tx,
            max_iterations: 100, // Safety limit
        }
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

            // Execute auto-approved tools
            if !auto_approved.is_empty() {
                self.set_state(LoopState::ExecutingTools).await;
                for tool_id in &auto_approved {
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
                        None => return Err("Command channel closed".to_string()),
                    }
                }
            }
        }
    }
}

/// Trait for executing tools
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool_call: &ToolCallInfo) -> Result<String, String>;
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

#[async_trait::async_trait]
impl ToolExecutor for DefaultToolExecutor {
    async fn execute(&self, tool_call: &ToolCallInfo) -> Result<String, String> {
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
}
