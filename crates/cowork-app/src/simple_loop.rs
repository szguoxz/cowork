//! Simplified Agentic Loop
//!
//! A clean, channel-based loop that:
//! 1. Waits for user input (blocks on intoloop channel)
//! 2. Processes with LLM
//! 3. Executes tools (auto-approve based on config)
//! 4. Sends all output through outputfromloop channel
//! 5. Returns to step 1

use std::sync::Arc;
use tokio::sync::mpsc;
use tauri::{AppHandle, Emitter};

use cowork_core::provider::{LlmMessage, LlmProvider, LlmRequest};
use cowork_core::tools::{standard_tool_definitions, ToolDefinition};
use cowork_core::orchestration::SystemPrompt;
use cowork_core::ToolApprovalConfig;

use crate::loop_channel::{LoopInput, LoopOutput};

/// The simple loop runner
pub struct SimpleLoop {
    /// Channel to receive input from frontend
    input_rx: mpsc::Receiver<LoopInput>,
    /// App handle for emitting events
    app: AppHandle,
    /// Event name for output
    event_name: String,
    /// LLM provider
    provider: Arc<dyn LlmProvider>,
    /// Conversation history
    messages: Vec<LlmMessage>,
    /// System prompt
    system_prompt: String,
    /// Available tools
    tools: Vec<ToolDefinition>,
    /// Tool approval config
    approval_config: ToolApprovalConfig,
    /// Workspace path for tool execution
    workspace_path: std::path::PathBuf,
}

impl SimpleLoop {
    pub fn new(
        input_rx: mpsc::Receiver<LoopInput>,
        app: AppHandle,
        event_name: String,
        provider: Arc<dyn LlmProvider>,
        workspace_path: std::path::PathBuf,
        approval_config: ToolApprovalConfig,
    ) -> Self {
        Self {
            input_rx,
            app,
            event_name,
            provider,
            messages: Vec::new(),
            system_prompt: SystemPrompt::new().build(),
            tools: standard_tool_definitions(&workspace_path),
            approval_config,
            workspace_path,
        }
    }

    /// Send output to frontend
    fn emit(&self, output: LoopOutput) {
        tracing::debug!("Emitting event '{}': {:?}", self.event_name, output);
        if let Err(e) = self.app.emit(&self.event_name, &output) {
            tracing::error!("Failed to emit: {}", e);
        }
    }

    /// Run the loop - this blocks until Stop is received
    pub async fn run(mut self) {
        tracing::info!("SimpleLoop starting, emitting Ready and Idle...");
        self.emit(LoopOutput::Ready);
        self.emit(LoopOutput::Idle);
        self.run_loop().await;
    }

    /// Run without emitting initial Ready/Idle (used when caller emits them synchronously)
    pub async fn run_without_initial_events(mut self) {
        tracing::info!("SimpleLoop starting (initial events already emitted)...");
        tracing::info!("Workspace path: {:?}", self.workspace_path);
        self.run_loop().await;
    }

    /// Internal loop logic
    async fn run_loop(&mut self) {
        tracing::info!("SimpleLoop now waiting for input...");

        loop {
            // Wait for user input (this blocks)
            let input = match self.input_rx.recv().await {
                Some(input) => input,
                None => {
                    tracing::info!("Input channel closed, stopping loop");
                    break;
                }
            };

            match input {
                LoopInput::Stop => {
                    self.emit(LoopOutput::Stopped);
                    break;
                }
                LoopInput::UserMessage(content) => {
                    self.handle_user_message(content).await;
                }
                LoopInput::ApproveTool(tool_id) => {
                    self.handle_tool_approval(&tool_id, true).await;
                }
                LoopInput::RejectTool(tool_id) => {
                    self.handle_tool_approval(&tool_id, false).await;
                }
                LoopInput::AnswerQuestion { request_id, answers } => {
                    self.handle_question_answer(&request_id, answers).await;
                }
            }
        }
    }

    /// Handle a user message
    async fn handle_user_message(&mut self, content: String) {
        let msg_id = uuid::Uuid::new_v4().to_string();

        // Echo user message
        self.emit(LoopOutput::UserMessage {
            id: msg_id,
            content: content.clone(),
        });

        // Add to history
        self.messages.push(LlmMessage {
            role: "user".to_string(),
            content,
        });

        // Process with LLM (may loop for tool calls)
        self.process_llm_response().await;

        // Back to idle
        self.emit(LoopOutput::Idle);
    }

    /// Process LLM response and handle tool calls
    async fn process_llm_response(&mut self) {
        loop {
            // Build request
            let request = LlmRequest::new(self.messages.clone())
                .with_system(&self.system_prompt)
                .with_tools(self.tools.clone())
                .with_max_tokens(4096);

            // Call LLM
            let response = match self.provider.complete(request).await {
                Ok(r) => r,
                Err(e) => {
                    self.emit(LoopOutput::Error {
                        message: format!("LLM error: {}", e),
                    });
                    return;
                }
            };

            let msg_id = uuid::Uuid::new_v4().to_string();
            let content = response.content.clone().unwrap_or_default();

            // Emit assistant message if there's text content
            if !content.is_empty() {
                self.emit(LoopOutput::AssistantMessage {
                    id: msg_id.clone(),
                    content: content.clone(),
                });
            }

            // Handle tool calls
            if response.tool_calls.is_empty() {
                // No tools, we're done - add to history and return
                self.messages.push(LlmMessage {
                    role: "assistant".to_string(),
                    content,
                });
                return;
            }

            // Build assistant message with tool calls included
            // This ensures the LLM knows it requested these tools
            let tool_calls_str: Vec<String> = response.tool_calls.iter()
                .map(|tc| format!("[Calling tool '{}' with args: {}]", tc.name, tc.arguments))
                .collect();
            let assistant_content = if content.is_empty() {
                tool_calls_str.join("\n")
            } else {
                format!("{}\n{}", content, tool_calls_str.join("\n"))
            };

            self.messages.push(LlmMessage {
                role: "assistant".to_string(),
                content: assistant_content,
            });

            // Process each tool call
            for tc in &response.tool_calls {
                let should_auto = self.approval_config.should_auto_approve(&tc.name);

                if should_auto {
                    // Auto-execute
                    self.emit(LoopOutput::ToolStart {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    });

                    let result = self.execute_tool(&tc.name, &tc.arguments).await;
                    let (success, output) = match result {
                        Ok(out) => (true, out),
                        Err(e) => (false, e),
                    };

                    self.emit(LoopOutput::ToolDone {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        success,
                        output: output.clone(),
                    });

                    // Add tool result to history
                    self.messages.push(LlmMessage {
                        role: "user".to_string(),
                        content: format!("[Tool result for {}]: {}", tc.id, output),
                    });
                } else {
                    // Need approval - emit pending and wait
                    self.emit(LoopOutput::ToolPending {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    });
                    // For now, we'll need the frontend to send ApproveTool/RejectTool
                    // This breaks the simple loop model slightly, but maintains safety
                    // TODO: Could queue pending tools and wait for all approvals
                    return;
                }
            }

            // Continue loop to get next LLM response after tool execution
        }
    }

    /// Execute a tool
    async fn execute_tool(&self, name: &str, args: &serde_json::Value) -> Result<String, String> {
        match name {
            "read_file" => {
                let path = args["path"].as_str().ok_or("Missing path")?;
                let full_path = self.workspace_path.join(path);
                tokio::fs::read_to_string(&full_path)
                    .await
                    .map_err(|e| format!("{} (tried: {:?})", e, full_path))
            }
            "write_file" => {
                let path = args["path"].as_str().ok_or("Missing path")?;
                let content = args["content"].as_str().ok_or("Missing content")?;
                let full_path = self.workspace_path.join(path);
                if let Some(parent) = full_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                tokio::fs::write(&full_path, content)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(format!("Wrote to {}", path))
            }
            "list_directory" => {
                let path = args["path"].as_str().unwrap_or(".");
                let full_path = self.workspace_path.join(path);
                let mut entries = Vec::new();
                let mut dir = tokio::fs::read_dir(&full_path)
                    .await
                    .map_err(|e| e.to_string())?;
                while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
                    let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                    let name = entry.file_name().to_string_lossy().to_string();
                    entries.push(if is_dir { format!("{}/", name) } else { name });
                }
                entries.sort();
                Ok(entries.join("\n"))
            }
            "execute_command" => {
                let command = args["command"].as_str().ok_or("Missing command")?;
                let working_dir = args["working_dir"]
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
                Ok(format!("Exit: {}\n{}{}",
                    output.status.code().unwrap_or(-1),
                    stdout,
                    if stderr.is_empty() { String::new() } else { format!("\nStderr: {}", stderr) }
                ))
            }
            "glob" | "search_files" => {
                let pattern = args["pattern"].as_str().ok_or("Missing pattern")?;
                let path = args["path"].as_str().unwrap_or(".");
                let full_path = self.workspace_path.join(path);
                let glob_pattern = full_path.join(pattern).to_string_lossy().to_string();

                let matches: Vec<String> = glob::glob(&glob_pattern)
                    .map_err(|e| e.to_string())?
                    .filter_map(|entry| entry.ok())
                    .map(|p| p.strip_prefix(&self.workspace_path)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .to_string())
                    .collect();

                if matches.is_empty() {
                    Ok(format!("No files found matching '{}' in {:?}", pattern, full_path))
                } else {
                    Ok(matches.join("\n"))
                }
            }
            _ => Err(format!("Unknown tool: {}. Workspace is: {:?}", name, self.workspace_path)),
        }
    }

    /// Handle tool approval
    async fn handle_tool_approval(&mut self, _tool_id: &str, _approved: bool) {
        // TODO: Implement queued tool approval
        // For now, this is a placeholder
    }

    /// Handle question answer
    async fn handle_question_answer(
        &mut self,
        _request_id: &str,
        _answers: std::collections::HashMap<String, String>,
    ) {
        // TODO: Implement question handling
        // For now, this is a placeholder
    }
}

/// Handle for sending input to the loop
#[derive(Clone)]
pub struct LoopInputHandle {
    tx: mpsc::Sender<LoopInput>,
}

impl LoopInputHandle {
    pub fn new(tx: mpsc::Sender<LoopInput>) -> Self {
        Self { tx }
    }

    pub async fn send(&self, input: LoopInput) -> Result<(), String> {
        self.tx.send(input).await.map_err(|e| e.to_string())
    }

    pub async fn send_message(&self, content: String) -> Result<(), String> {
        self.send(LoopInput::UserMessage(content)).await
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.send(LoopInput::Stop).await
    }
}
