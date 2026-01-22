//! Built-in prompt components for Cowork
//!
//! This module provides the literal prompt text used by the agent system.
//! These prompts are based on Claude Code's prompt system and can be loaded
//! at runtime.
//!
//! # Module Structure
//!
//! - `claude_code` - Pre-expanded prompts from Claude Code's prompt system
//! - `agents` - Agent definitions with YAML frontmatter
//! - `tools` - Tool descriptions
//! - `reminders` - System reminders
//! - `commands` - Slash command definitions

/// Claude Code prompt system (pre-expanded)
pub mod claude_code;

/// Main system prompt template - uses Claude Code's pre-expanded prompt
pub const SYSTEM_PROMPT: &str = claude_code::SYSTEM_PROMPT;

/// Agent definitions
pub mod agents {
    /// Explore agent - fast codebase searching
    pub const EXPLORE: &str = include_str!("agents/explore.md");
    /// Plan agent - implementation planning
    pub const PLAN: &str = include_str!("agents/plan.md");
    /// Bash agent - command execution
    pub const BASH: &str = include_str!("agents/bash.md");
    /// General-purpose agent
    pub const GENERAL: &str = include_str!("agents/general.md");
}

/// Tool descriptions
pub mod tools {
    /// Task tool - launch subagents
    pub const TASK: &str = include_str!("tools/task.md");
    /// Bash tool - execute commands
    pub const BASH: &str = include_str!("tools/bash.md");
    /// Read tool - read files
    pub const READ: &str = include_str!("tools/read.md");
    /// Write tool - write files
    pub const WRITE: &str = include_str!("tools/write.md");
    /// Edit tool - edit files
    pub const EDIT: &str = include_str!("tools/edit.md");
    /// Glob tool - find files by pattern
    pub const GLOB: &str = include_str!("tools/glob.md");
    /// Grep tool - search file contents
    pub const GREP: &str = include_str!("tools/grep.md");
    /// TodoWrite tool - task management
    pub const TODO_WRITE: &str = include_str!("tools/todo_write.md");
    /// AskUserQuestion tool - ask user questions
    pub const ASK_USER_QUESTION: &str = include_str!("tools/ask_user_question.md");
    /// WebFetch tool - fetch web content
    pub const WEB_FETCH: &str = include_str!("tools/web_fetch.md");
    /// WebSearch tool - search the web
    pub const WEB_SEARCH: &str = include_str!("tools/web_search.md");
    /// LSP tool - code intelligence
    pub const LSP: &str = include_str!("tools/lsp.md");
    /// EnterPlanMode tool
    pub const ENTER_PLAN_MODE: &str = include_str!("tools/enter_plan_mode.md");
    /// ExitPlanMode tool
    pub const EXIT_PLAN_MODE: &str = include_str!("tools/exit_plan_mode.md");
}

/// System reminders
pub mod reminders {
    /// Plan mode active reminder
    pub const PLAN_MODE_ACTIVE: &str = include_str!("reminders/plan_mode_active.md");
    /// Security policy
    pub const SECURITY_POLICY: &str = include_str!("reminders/security_policy.md");
    /// Conversation summarization instructions
    pub const CONVERSATION_SUMMARIZATION: &str = include_str!("reminders/conversation_summarization.md");
}

/// Built-in commands (slash commands)
pub mod commands {
    /// /commit command - create a git commit
    pub const COMMIT: &str = include_str!("commands/commit.md");
    /// /pr command - create a pull request
    pub const PR: &str = include_str!("commands/pr.md");
    /// /review-pr command - review a pull request
    pub const REVIEW_PR: &str = include_str!("commands/review-pr.md");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_loads() {
        assert!(!SYSTEM_PROMPT.is_empty());
        // System prompt comes from Claude Code
        assert!(SYSTEM_PROMPT.contains("You are Claude"));
        assert!(SYSTEM_PROMPT.contains("Security"));
    }

    #[test]
    fn test_agents_load() {
        assert!(!agents::EXPLORE.is_empty());
        assert!(!agents::PLAN.is_empty());
        assert!(!agents::BASH.is_empty());
        assert!(!agents::GENERAL.is_empty());
    }

    #[test]
    fn test_tools_load() {
        assert!(!tools::TASK.is_empty());
        assert!(!tools::BASH.is_empty());
        assert!(!tools::READ.is_empty());
        assert!(!tools::WRITE.is_empty());
        assert!(!tools::EDIT.is_empty());
        assert!(!tools::GLOB.is_empty());
        assert!(!tools::GREP.is_empty());
        assert!(!tools::TODO_WRITE.is_empty());
    }

    #[test]
    fn test_reminders_load() {
        assert!(!reminders::PLAN_MODE_ACTIVE.is_empty());
        assert!(!reminders::SECURITY_POLICY.is_empty());
        assert!(!reminders::CONVERSATION_SUMMARIZATION.is_empty());
    }

    #[test]
    fn test_commands_load() {
        assert!(!commands::COMMIT.is_empty());
        assert!(!commands::PR.is_empty());
        assert!(!commands::REVIEW_PR.is_empty());

        // Verify they have proper frontmatter
        assert!(commands::COMMIT.starts_with("---"));
        assert!(commands::PR.starts_with("---"));
        assert!(commands::REVIEW_PR.starts_with("---"));

        // Verify they contain expected content
        assert!(commands::COMMIT.contains("name: commit"));
        assert!(commands::PR.contains("name: pr"));
        assert!(commands::REVIEW_PR.contains("name: review-pr"));
    }
}
