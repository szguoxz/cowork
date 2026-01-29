//! Orchestration module for shared agentic loop logic
//!
//! This module contains shared code between CLI and UI for:
//! - System prompts
//! - Tool registry creation

pub mod system_prompt;
mod tool_registry;
mod tool_result;

pub use system_prompt::SystemPrompt;
pub use tool_registry::{create_standard_tool_registry, ToolRegistryBuilder, ToolScope};
pub use tool_result::format_tool_result_for_llm;
