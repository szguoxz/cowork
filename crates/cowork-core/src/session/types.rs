//! Session types for the unified agent loop architecture
//!
//! These types define the input/output protocol between frontends (CLI, UI)
//! and the agent sessions running in cowork-core.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::config::PromptSystemConfig;
use crate::formatting::DiffLine;
use crate::orchestration::ToolScope;
use crate::prompt::ComponentRegistry;

/// Unique identifier for a session
pub type SessionId = String;

/// Shared registry of session input senders, keyed by session ID.
///
/// Used to route approval/answer inputs to both top-level sessions and subagents.
pub type SessionRegistry = Arc<parking_lot::RwLock<HashMap<SessionId, mpsc::Sender<SessionInput>>>>;

/// Input messages sent TO an agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionInput {
    /// User sends a message
    UserMessage { content: String },
    /// User approves a tool execution
    ApproveTool { tool_call_id: String },
    /// User rejects a tool execution
    RejectTool {
        tool_call_id: String,
        reason: Option<String>,
    },
    /// User answers a question from ask_user_question tool
    AnswerQuestion {
        request_id: String,
        answers: HashMap<String, String>,
    },
    /// User cancels the current turn
    Cancel,
    /// User toggles plan mode
    SetPlanMode { active: bool },
}

impl SessionInput {
    /// Create a user message input
    pub fn user_message(content: impl Into<String>) -> Self {
        Self::UserMessage {
            content: content.into(),
        }
    }

    /// Create an approve tool input
    pub fn approve_tool(tool_call_id: impl Into<String>) -> Self {
        Self::ApproveTool {
            tool_call_id: tool_call_id.into(),
        }
    }

    /// Create a reject tool input
    pub fn reject_tool(tool_call_id: impl Into<String>, reason: Option<String>) -> Self {
        Self::RejectTool {
            tool_call_id: tool_call_id.into(),
            reason,
        }
    }

    /// Create an answer question input
    pub fn answer_question(request_id: impl Into<String>, answers: HashMap<String, String>) -> Self {
        Self::AnswerQuestion {
            request_id: request_id.into(),
            answers,
        }
    }

    /// Create a cancel input
    pub fn cancel() -> Self {
        Self::Cancel
    }

    /// Create a set plan mode input
    pub fn set_plan_mode(active: bool) -> Self {
        Self::SetPlanMode { active }
    }
}

/// Output messages sent FROM an agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionOutput {
    /// Session is ready to receive input
    Ready,
    /// Session is idle, waiting for input
    Idle,
    /// Echo of user message (for UI display)
    UserMessage { id: String, content: String },
    /// Assistant is thinking (streaming indicator)
    Thinking { content: String },
    /// Assistant message (complete)
    AssistantMessage { id: String, content: String },
    /// Tool execution starting (auto-approved or approved by user)
    ToolStart {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Tool needs user approval
    ToolPending {
        id: String,
        name: String,
        arguments: serde_json::Value,
        description: Option<String>,
    },
    /// Tool execution completed
    ToolDone {
        id: String,
        name: String,
        success: bool,
        output: String,
    },
    /// Tool call message (persistent, Claude Code style)
    ///
    /// Emitted when a tool is about to be executed, formatted for display.
    /// Unlike ToolStart (ephemeral), this is meant to be stored as a message.
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
        /// Formatted display string, e.g. "Read(/path/to/file.rs)"
        formatted: String,
    },
    /// Tool result message (persistent, Claude Code style)
    ///
    /// Emitted after tool execution, with a summary and optional diff.
    /// Unlike ToolDone (ephemeral), this is meant to be stored as a message.
    ToolResult {
        id: String,
        name: String,
        success: bool,
        /// The full tool output
        output: String,
        /// Short summary like "Read 20 lines" or "Added 5 lines"
        summary: String,
        /// For Edit tool: diff preview with +/- lines
        diff_preview: Option<Vec<DiffLine>>,
    },
    /// Question for the user (from ask_user_question tool)
    Question {
        request_id: String,
        questions: Vec<QuestionInfo>,
    },
    /// Error occurred
    Error { message: String },
    /// Turn was cancelled by user
    Cancelled,
    /// Plan mode state changed
    PlanModeChanged {
        active: bool,
        /// Path to the plan file (when entering plan mode)
        plan_file: Option<String>,
    },
}

impl SessionOutput {
    /// Create a ready output
    pub fn ready() -> Self {
        Self::Ready
    }

    /// Create an idle output
    pub fn idle() -> Self {
        Self::Idle
    }

    /// Create a user message echo
    pub fn user_message(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::UserMessage {
            id: id.into(),
            content: content.into(),
        }
    }

    /// Create a thinking indicator
    pub fn thinking(content: impl Into<String>) -> Self {
        Self::Thinking {
            content: content.into(),
        }
    }

    /// Create an assistant message
    pub fn assistant_message(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::AssistantMessage {
            id: id.into(),
            content: content.into(),
        }
    }

    /// Create a tool start notification
    pub fn tool_start(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self::ToolStart {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// Create a tool pending notification
    pub fn tool_pending(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
        description: Option<String>,
    ) -> Self {
        Self::ToolPending {
            id: id.into(),
            name: name.into(),
            arguments,
            description,
        }
    }

    /// Create a tool done notification
    pub fn tool_done(
        id: impl Into<String>,
        name: impl Into<String>,
        success: bool,
        output: impl Into<String>,
    ) -> Self {
        Self::ToolDone {
            id: id.into(),
            name: name.into(),
            success,
            output: output.into(),
        }
    }

    /// Create an error output
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    /// Create a cancelled output
    pub fn cancelled() -> Self {
        Self::Cancelled
    }

    /// Create a plan mode changed output
    pub fn plan_mode_changed(active: bool, plan_file: Option<String>) -> Self {
        Self::PlanModeChanged { active, plan_file }
    }

    /// Create a tool call output (persistent message)
    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
        formatted: impl Into<String>,
    ) -> Self {
        Self::ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
            formatted: formatted.into(),
        }
    }

    /// Create a tool result output (persistent message)
    pub fn tool_result(
        id: impl Into<String>,
        name: impl Into<String>,
        success: bool,
        output: impl Into<String>,
        summary: impl Into<String>,
        diff_preview: Option<Vec<DiffLine>>,
    ) -> Self {
        Self::ToolResult {
            id: id.into(),
            name: name.into(),
            success,
            output: output.into(),
            summary: summary.into(),
            diff_preview,
        }
    }
}

/// Information about a question option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// Information about a question
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionInfo {
    pub question: String,
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

/// Configuration for creating a session
#[derive(Clone)]
pub struct SessionConfig {
    /// Path to the workspace root
    pub workspace_path: std::path::PathBuf,
    /// Tool approval configuration
    pub approval_config: crate::approval::ToolApprovalConfig,
    /// Optional custom system prompt
    pub system_prompt: Option<String>,
    /// Provider type to use
    pub provider_type: crate::provider::ProviderType,
    /// Optional model override
    pub model: Option<String>,
    /// Optional API key (if not using env var)
    pub api_key: Option<String>,
    /// Web search configuration
    pub web_search_config: Option<crate::config::WebSearchConfig>,
    /// Prompt system configuration
    pub prompt_config: PromptSystemConfig,
    /// Component registry (agents, commands, skills, hooks)
    pub component_registry: Option<Arc<ComponentRegistry>>,
    /// Tool scope — restricts which tools are registered (for subagents)
    pub tool_scope: Option<ToolScope>,
    /// Override whether hooks are enabled (None = use prompt_config default)
    pub enable_hooks: Option<bool>,
    /// Whether to persist the session to disk on exit (default: true)
    pub save_session: bool,
    /// Shared session registry for routing approvals to subagents
    pub session_registry: Option<SessionRegistry>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            workspace_path: std::env::current_dir().unwrap_or_default(),
            approval_config: crate::approval::ToolApprovalConfig::default(),
            system_prompt: None,
            provider_type: crate::provider::ProviderType::Anthropic,
            model: None,
            api_key: None,
            web_search_config: None,
            prompt_config: PromptSystemConfig::default(),
            component_registry: None,
            tool_scope: None,
            enable_hooks: None,
            save_session: true,
            session_registry: None,
        }
    }
}

impl SessionConfig {
    /// Create a new session config with the given workspace path
    pub fn new(workspace_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            workspace_path: workspace_path.into(),
            ..Default::default()
        }
    }

    /// Set the provider type
    pub fn with_provider(mut self, provider_type: crate::provider::ProviderType) -> Self {
        self.provider_type = provider_type;
        self
    }

    /// Set the model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the API key
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the approval config
    pub fn with_approval_config(mut self, config: crate::approval::ToolApprovalConfig) -> Self {
        self.approval_config = config;
        self
    }

    /// Set the web search config
    pub fn with_web_search_config(mut self, config: crate::config::WebSearchConfig) -> Self {
        self.web_search_config = Some(config);
        self
    }

    /// Set the prompt system config
    pub fn with_prompt_config(mut self, config: PromptSystemConfig) -> Self {
        self.prompt_config = config;
        self
    }

    /// Set the component registry
    pub fn with_component_registry(mut self, registry: Arc<ComponentRegistry>) -> Self {
        self.component_registry = Some(registry);
        self
    }

    /// Set the tool scope (restricts available tools for subagents)
    pub fn with_tool_scope(mut self, scope: ToolScope) -> Self {
        self.tool_scope = Some(scope);
        self
    }

    /// Override hook enablement
    pub fn with_enable_hooks(mut self, enabled: bool) -> Self {
        self.enable_hooks = Some(enabled);
        self
    }

    /// Set whether to save the session to disk on exit
    pub fn with_save_session(mut self, save: bool) -> Self {
        self.save_session = save;
        self
    }

    /// Set the shared session registry (for subagent approval routing)
    pub fn with_session_registry(mut self, registry: SessionRegistry) -> Self {
        self.session_registry = Some(registry);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_input_creation() {
        let msg = SessionInput::user_message("Hello");
        match msg {
            SessionInput::UserMessage { content } => assert_eq!(content, "Hello"),
            _ => panic!("Expected UserMessage"),
        }

        let approve = SessionInput::approve_tool("tool-123");
        match approve {
            SessionInput::ApproveTool { tool_call_id } => assert_eq!(tool_call_id, "tool-123"),
            _ => panic!("Expected ApproveTool"),
        }
    }

    #[test]
    fn test_session_output_creation() {
        let ready = SessionOutput::ready();
        assert!(matches!(ready, SessionOutput::Ready));

        let msg = SessionOutput::assistant_message("msg-1", "Hello!");
        match msg {
            SessionOutput::AssistantMessage { id, content } => {
                assert_eq!(id, "msg-1");
                assert_eq!(content, "Hello!");
            }
            _ => panic!("Expected AssistantMessage"),
        }
    }

    #[test]
    fn test_session_input_serialization() {
        let input = SessionInput::user_message("test");
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("user_message"));
        assert!(json.contains("test"));

        let deserialized: SessionInput = serde_json::from_str(&json).unwrap();
        match deserialized {
            SessionInput::UserMessage { content } => assert_eq!(content, "test"),
            _ => panic!("Deserialization failed"),
        }
    }

    #[test]
    fn test_session_output_serialization() {
        let output = SessionOutput::tool_done("t1", "Read", true, "file contents");
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("tool_done"));
        assert!(json.contains("Read"));

        let deserialized: SessionOutput = serde_json::from_str(&json).unwrap();
        match deserialized {
            SessionOutput::ToolDone {
                id,
                name,
                success,
                output,
            } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "Read");
                assert!(success);
                assert_eq!(output, "file contents");
            }
            _ => panic!("Deserialization failed"),
        }
    }

    #[test]
    fn test_session_config_builder() {
        let config = SessionConfig::new("/tmp/workspace")
            .with_provider(crate::provider::ProviderType::OpenAI)
            .with_model("gpt-4")
            .with_system_prompt("Custom prompt");

        assert_eq!(
            config.workspace_path,
            std::path::PathBuf::from("/tmp/workspace")
        );
        assert_eq!(config.provider_type, crate::provider::ProviderType::OpenAI);
        assert_eq!(config.model, Some("gpt-4".to_string()));
        assert_eq!(config.system_prompt, Some("Custom prompt".to_string()));
    }
}
