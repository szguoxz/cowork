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
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::types::{SessionConfig, SessionId, SessionInput, SessionOutput};
use crate::approval::ToolApprovalConfig;
use crate::context::{
    CompactConfig, ContextMonitor, ConversationSummarizer, Message, MessageRole, SummarizerConfig,
};
use crate::error::Result;
use crate::orchestration::{ChatMessage, ChatSession, ToolCallInfo, ToolRegistryBuilder};
use crate::provider::{CompletionResult, GenAIProvider};
use crate::tools::{ToolDefinition, ToolRegistry};

/// Maximum number of agentic turns per user message
const MAX_ITERATIONS: usize = 100;

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
        let tool_registry = ToolRegistryBuilder::new(config.workspace_path.clone())
            .with_provider(config.provider_type)
            .build();

        let tool_definitions = tool_registry.list();

        // Initialize context monitor with provider and model for accurate limits
        let context_monitor = match &config.model {
            Some(model) => ContextMonitor::with_model(config.provider_type, model),
            None => ContextMonitor::new(config.provider_type),
        };

        // Initialize summarizer with default config
        let summarizer = ConversationSummarizer::new(SummarizerConfig::default());

        Ok(Self {
            session_id,
            input_rx,
            output_tx,
            provider,
            session,
            tool_registry,
            tool_definitions,
            approval_config: config.approval_config,
            workspace_path: config.workspace_path,
            pending_approvals: Vec::new(),
            context_monitor,
            summarizer,
        })
    }

    /// Run the agent loop until Stop is received or channel closes
    pub async fn run(mut self) {
        info!("Agent loop starting for session: {}", self.session_id);

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
                    self.emit(SessionOutput::idle()).await;
                }
                SessionInput::ApproveTool { tool_call_id } => {
                    if let Err(e) = self.handle_approve_tool(&tool_call_id).await {
                        self.emit(SessionOutput::error(e.to_string())).await;
                    }
                }
                SessionInput::RejectTool { tool_call_id, reason } => {
                    self.handle_reject_tool(&tool_call_id, reason).await;
                }
                SessionInput::AnswerQuestion { request_id, answers } => {
                    if let Err(e) = self.handle_answer_question(&request_id, answers).await {
                        self.emit(SessionOutput::error(e.to_string())).await;
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
        // Generate message ID
        let msg_id = uuid::Uuid::new_v4().to_string();

        // Echo the user message
        self.emit(SessionOutput::user_message(&msg_id, &content))
            .await;

        // Add to session
        self.session.add_user_message(&content);

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

            // Check for ask_user_question - it needs special handling
            let mut has_question = false;
            for tool_call in &tool_calls {
                if tool_call.name == "ask_user_question" {
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

            // Execute auto-approved tools
            for tool_call in &tool_calls {
                if auto_approved.contains(&tool_call.id) {
                    self.execute_tool(tool_call).await;
                }
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

    /// Execute a single tool
    async fn execute_tool(&mut self, tool_call: &ToolCallInfo) {
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

        // Add tool result to session
        self.session.add_tool_result(&tool_call.id, &result.1);

        // Emit tool done
        self.emit(SessionOutput::tool_done(
            &tool_call.id,
            &tool_call.name,
            result.0,
            result.1,
        ))
        .await;
    }

    /// Parse questions from ask_user_question tool arguments
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
        }

        Ok(())
    }

    /// Handle tool rejection
    async fn handle_reject_tool(&mut self, tool_call_id: &str, reason: Option<String>) {
        // Remove from pending
        self.pending_approvals.retain(|tc| tc.id != tool_call_id);

        // Mark as rejected in session
        self.session.reject_tool(tool_call_id);

        // Add rejection result
        let result = reason.unwrap_or_else(|| "Rejected by user".to_string());
        self.session.add_tool_result(tool_call_id, &result);

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

    /// Handle answer to a question from ask_user_question tool
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

        // Find the pending ask_user_question tool call by ID (request_id is the tool call ID)
        let tool_call = self
            .pending_approvals
            .iter()
            .find(|tc| tc.id == request_id || tc.name == "ask_user_question")
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
                "ask_user_question",
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
                Message::with_timestamp(role, &cm.content, cm.timestamp)
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
        let usage = self.context_monitor.calculate_usage(
            &messages,
            &self.session.system_prompt,
            None, // No memory content for now
        );

        debug!(
            "Context usage: {:.1}% ({}/{} tokens)",
            usage.used_percentage * 100.0,
            usage.used_tokens,
            usage.limit_tokens
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

        // Use simple compaction (without LLM) to avoid recursive LLM calls
        // during the agentic loop. LLM-powered compaction could cause issues
        // if we're already at context limit.
        let config = CompactConfig::auto().without_llm();
        let result = self
            .summarizer
            .compact(
                &messages,
                self.context_monitor.counter(),
                config,
                None, // Don't use LLM for auto-compaction to avoid recursion
            )
            .await?;

        info!(
            "Compaction complete: {} -> {} tokens ({} messages summarized, {} kept)",
            result.tokens_before,
            result.tokens_after,
            result.messages_summarized,
            result.messages_kept
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
    fn apply_compaction_result(&mut self, result: &crate::context::CompactResult) {
        // Clear existing messages
        self.session.clear();

        // Add the summary as a system message (but keep original system prompt)
        // The summary becomes part of the conversation context
        self.session.messages.push(ChatMessage::system(&result.summary.content));

        // Add back the kept messages, converting from Message to ChatMessage
        for msg in &result.kept_messages {
            let chat_msg = match msg.role {
                MessageRole::User => ChatMessage::user(&msg.content),
                MessageRole::Assistant => ChatMessage::assistant(&msg.content),
                MessageRole::System => ChatMessage::system(&msg.content),
                MessageRole::Tool => {
                    // Tool results were stored as user messages with special format
                    ChatMessage::user(&msg.content)
                }
            };
            self.session.messages.push(chat_msg);
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

        let now = chrono::Utc::now();
        let saved = SavedSession {
            id: self.session_id.clone(),
            name: format!("Session {}", self.session_id),
            messages,
            created_at: now, // TODO: track actual creation time
            updated_at: now,
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
        if path.extension().map_or(false, |ext| ext == "json") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_categorization() {
        let approval_config = ToolApprovalConfig::new(crate::approval::ApprovalLevel::Low);

        // read_file should be auto-approved
        assert!(approval_config.should_auto_approve("read_file"));
        assert!(approval_config.should_auto_approve("glob"));
        assert!(approval_config.should_auto_approve("grep"));

        // write operations should need approval
        assert!(!approval_config.should_auto_approve("write_file"));
        assert!(!approval_config.should_auto_approve("execute_command"));
    }
}
