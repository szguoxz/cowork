//! Chat service for handling conversations with the LLM

use std::sync::Arc;

use cowork_core::context::{
    ContextMonitor, ContextUsage, Message, MessageRole, MonitorConfig,
};
use cowork_core::provider::{
    create_provider, LlmMessage, LlmProvider, LlmRequest, ProviderType,
};
use cowork_core::tools::ToolDefinition;

use crate::state::ProviderSettings;

/// A message in the conversation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCallInfo>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Tool call information for display
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub status: ToolCallStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ToolCallStatus {
    Pending,
    Approved,
    Rejected,
    Executing,
    Completed,
    Failed,
}

/// Chat session state
pub struct ChatSession {
    pub id: String,
    pub messages: Vec<ChatMessage>,
    pub provider: Arc<dyn LlmProvider>,
    pub system_prompt: String,
    pub available_tools: Vec<ToolDefinition>,
    /// Context monitor for tracking token usage
    context_monitor: Option<ContextMonitor>,
    /// Provider type for the session
    provider_type: ProviderType,
}

impl ChatSession {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            messages: Vec::new(),
            provider,
            system_prompt: default_system_prompt(),
            available_tools: default_tools(),
            context_monitor: None,
            provider_type: ProviderType::Anthropic,
        }
    }

    /// Create a new session with a specific provider type
    pub fn with_provider_type(provider: Arc<dyn LlmProvider>, provider_type: ProviderType) -> Self {
        let context_monitor = Some(ContextMonitor::new(provider_type.clone()));
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            messages: Vec::new(),
            provider,
            system_prompt: default_system_prompt(),
            available_tools: default_tools(),
            context_monitor,
            provider_type,
        }
    }

    /// Get current context usage
    pub fn context_usage(&self) -> Option<ContextUsage> {
        let monitor = self.context_monitor.as_ref()?;

        // Convert ChatMessages to context Messages
        let context_messages: Vec<Message> = self
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

        Some(monitor.calculate_usage(&context_messages, &self.system_prompt, None))
    }

    /// Check if context should be compacted
    pub fn should_compact(&self) -> bool {
        self.context_usage()
            .map(|u| u.should_compact)
            .unwrap_or(false)
    }

    /// Enable context monitoring with optional custom config
    pub fn enable_context_monitoring(&mut self, config: Option<MonitorConfig>) {
        let cfg = config.unwrap_or_default();
        self.context_monitor = Some(ContextMonitor::with_config(self.provider_type.clone(), cfg));
    }

    /// Get the provider type
    pub fn provider_type(&self) -> ProviderType {
        self.provider_type.clone()
    }

    /// Add a user message and get an assistant response
    pub async fn send_message(&mut self, content: String) -> Result<ChatMessage, String> {
        // Add user message
        let user_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: content.clone(),
            tool_calls: Vec::new(),
            timestamp: chrono::Utc::now(),
        };
        self.messages.push(user_msg.clone());

        // Build LLM request
        let llm_messages: Vec<LlmMessage> = self
            .messages
            .iter()
            .map(|m| LlmMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        let request = LlmRequest::new(llm_messages)
            .with_system(&self.system_prompt)
            .with_tools(self.available_tools.clone())
            .with_max_tokens(4096);

        // Get response from LLM
        let response = self
            .provider
            .complete(request)
            .await
            .map_err(|e| e.to_string())?;

        // Convert tool calls
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

        // Create assistant message
        let assistant_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: response.content.unwrap_or_default(),
            tool_calls,
            timestamp: chrono::Utc::now(),
        };
        self.messages.push(assistant_msg.clone());

        Ok(assistant_msg)
    }

    /// Execute a tool call and continue the conversation
    pub async fn execute_tool_call(
        &mut self,
        tool_call_id: &str,
        result: String,
    ) -> Result<Option<ChatMessage>, String> {
        // Find and update the tool call status
        for msg in &mut self.messages {
            for tc in &mut msg.tool_calls {
                if tc.id == tool_call_id {
                    tc.status = ToolCallStatus::Completed;
                    tc.result = Some(result.clone());
                }
            }
        }

        // Add tool result as a message
        let tool_result_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: format!("Tool result for {}: {}", tool_call_id, result),
            tool_calls: Vec::new(),
            timestamp: chrono::Utc::now(),
        };
        self.messages.push(tool_result_msg);

        // Check if there are more pending tool calls
        let has_pending = self
            .messages
            .iter()
            .any(|m| m.tool_calls.iter().any(|tc| matches!(tc.status, ToolCallStatus::Pending)));

        if has_pending {
            return Ok(None);
        }

        // Get next response from LLM
        let llm_messages: Vec<LlmMessage> = self
            .messages
            .iter()
            .map(|m| LlmMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        let request = LlmRequest::new(llm_messages)
            .with_system(&self.system_prompt)
            .with_tools(self.available_tools.clone())
            .with_max_tokens(4096);

        let response = self
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
        self.messages.push(assistant_msg.clone());

        Ok(Some(assistant_msg))
    }
}

/// Create an LLM provider from core config
pub fn create_provider_from_config(
    config: &cowork_core::config::ProviderConfig,
) -> Result<Arc<dyn LlmProvider>, String> {
    let provider_type: ProviderType = config
        .provider_type
        .parse()
        .map_err(|e: String| e)?;

    let api_key = config.get_api_key();

    let provider = create_provider(
        provider_type,
        api_key.as_deref(),
        Some(&config.model),
        None, // Use default preamble
    )
    .map_err(|e| e.to_string())?;

    Ok(Arc::new(provider))
}

/// Create an LLM provider from settings (used by commands)
pub fn create_provider_from_settings(settings: &ProviderSettings) -> Result<Arc<dyn LlmProvider>, String> {
    let provider_type: ProviderType = settings
        .provider_type
        .parse()
        .map_err(|e: String| e)?;

    let provider = create_provider(
        provider_type,
        settings.api_key.as_deref(),
        Some(&settings.model),
        None, // Use default preamble
    )
    .map_err(|e| e.to_string())?;

    Ok(Arc::new(provider))
}

fn default_system_prompt() -> String {
    r#"You are Cowork, an AI assistant that helps users with software development tasks.

You have access to various tools:
- read_file: Read contents of a file
- write_file: Write content to a file
- list_directory: List files in a directory
- execute_command: Run shell commands
- search_files: Search for files by pattern

When the user asks you to perform a task:
1. Analyze what needs to be done
2. Use the appropriate tools to complete the task
3. Explain what you're doing and why

Always ask for clarification if the request is ambiguous.
Be careful with destructive operations - explain what will happen before executing."#
        .to_string()
}

fn default_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "list_directory".to_string(),
            description: "List files and directories in a given path".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory to list"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "execute_command".to_string(),
            description: "Execute a shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Working directory for the command"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "search_files".to_string(),
            description: "Search for files matching a pattern".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to search for (e.g., '*.rs', '**/*.ts')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in"
                    }
                },
                "required": ["pattern"]
            }),
        },
    ]
}
