//! Orchestration module for shared agentic loop logic
//!
//! This module contains shared code between CLI and UI for:
//! - System prompts
//! - Chat session management
//! - Tool result formatting
//! - Tool registry creation
//! - Agentic loop abstractions

mod formatting;
mod session;
mod system_prompt;
mod tool_registry;
mod tool_result;

pub use formatting::{
    format_command_result, format_directory_result, format_file_content, format_generic_json,
    format_glob_result, format_grep_result, format_size, format_status_result, format_tool_result,
    truncate_result,
};
pub use session::{ChatMessage, ChatSession, ToolCallInfo, ToolCallStatus};
pub use system_prompt::SystemPrompt;
pub use tool_registry::{create_standard_tool_registry, ToolRegistryBuilder, ToolScope};
pub use tool_result::format_tool_result_for_llm;
