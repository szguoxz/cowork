//! Orchestration module for shared agentic loop logic
//!
//! This module contains shared code between CLI and UI for:
//! - System prompts
//! - Chat session management
//! - Tool result formatting
//! - Agentic loop abstractions

mod system_prompt;
mod session;
mod tool_result;

pub use system_prompt::SystemPrompt;
pub use session::{ChatSession, ChatMessage, ToolCallInfo, ToolCallStatus};
pub use tool_result::format_tool_result_for_llm;
