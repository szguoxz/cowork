//! Cowork Core - Multi-agent orchestration for desktop automation
//!
//! This crate provides the core functionality for the Cowork application:
//! - Agent definitions and implementations
//! - Tool system for file, shell, browser, and document operations
//! - Task planning and execution
//! - Human-in-the-loop approval system
//! - Context management

pub mod agent;
pub mod approval;
pub mod config;
pub mod context;
pub mod error;
pub mod mcp_manager;
pub mod orchestration;
pub mod prompt;
pub mod provider;
pub mod session;
pub mod skills;
pub mod task;
pub mod tools;

pub use agent::{Agent, AgentRegistry};
pub use approval::{ApprovalLevel, ApprovalPolicy, ApprovalRequest, ToolApprovalConfig};
pub use config::{defaults, Config, ConfigManager, McpServerConfig, ModelTiers, ProviderConfig};
pub use context::{messages_from_ui, Context, Message, MessageRole, Workspace};
pub use mcp_manager::{McpServerInfo, McpServerManager, McpServerStatus, McpToolInfo};
pub use error::{Error, Result};
pub use provider::{
    create_provider_from_config, create_provider_from_provider_config, create_provider_with_settings,
    get_api_key, get_model_tiers, has_api_key_configured, ProviderType,
};
pub use skills::{Skill, SkillContext, SkillRegistry, SkillResult};
pub use task::{Task, TaskExecutor, TaskPlanner, TaskStatus, TaskStep};
pub use tools::{standard_tool_definitions, Tool, ToolDefinition, ToolOutput, ToolRegistry};

// Prompt system exports
pub use prompt::{
    builtin, extract_commands, has_substitutions, parse_frontmatter, parse_tool_list,
    substitute_commands, ModelPreference, ParseError, ParsedDocument, Scope, TemplateVars,
    ToolRestrictions, ToolSpec,
};

// Orchestration exports
pub use orchestration::{
    create_standard_tool_registry, format_command_result, format_directory_result,
    format_file_content, format_generic_json, format_glob_result, format_grep_result, format_size,
    format_status_result, format_tool_result, format_tool_result_for_llm, truncate_result,
    ChatMessage, ChatSession, SystemPrompt, ToolCallInfo, ToolCallStatus, ToolRegistryBuilder,
};

// Session exports (unified agent loop architecture)
pub use session::{
    AgentLoop, QuestionInfo, QuestionOption, SessionConfig, SessionId,
    SessionInput, SessionManager, SessionOutput,
};
