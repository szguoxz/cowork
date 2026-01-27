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

/// Strip markdown title header from prompt content.
///
/// Removes the first line if it starts with `# ` and is followed by a blank line.
/// This is used to strip file title comments (e.g., "# Claude Code System Prompt")
/// that are meant for human readers but waste tokens when sent to the LLM.
///
/// # Example
/// ```
/// use cowork_core::prompt::builtin::strip_markdown_header;
///
/// let content = "# Title\n\nActual content here.";
/// assert_eq!(strip_markdown_header(content), "Actual content here.");
///
/// // Preserves content without header
/// let no_header = "Actual content here.";
/// assert_eq!(strip_markdown_header(no_header), "Actual content here.");
///
/// // Preserves ## section headers (not file titles)
/// let section = "## Section\n\nContent";
/// assert_eq!(strip_markdown_header(section), "## Section\n\nContent");
/// ```
pub fn strip_markdown_header(content: &str) -> &str {
    // Check if content starts with "# " (h1 header)
    if !content.starts_with("# ") {
        return content;
    }

    // Find the end of the first line
    if let Some(newline_pos) = content.find('\n') {
        let after_first_line = &content[newline_pos + 1..];

        // Check if followed by blank line (either \n or \r\n)
        if after_first_line.starts_with('\n') {
            return &after_first_line[1..];
        } else if after_first_line.starts_with("\r\n") {
            return &after_first_line[2..];
        }
    }

    content
}

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

/// Built-in commands (slash commands) — official Claude Code plugin commands
pub mod commands {
    /// /commit command - create a git commit
    pub const COMMIT: &str = include_str!("commands/commit.md");
    /// /commit-push-pr command - commit, push, and open a PR
    pub const COMMIT_PUSH_PR: &str = include_str!("commands/commit-push-pr.md");
    /// /clean_gone command - clean up local branches deleted from remote
    pub const CLEAN_GONE: &str = include_str!("commands/clean_gone.md");
    /// /code-review command - code review a pull request
    pub const CODE_REVIEW: &str = include_str!("commands/code-review.md");
    /// /feature-dev command - guided feature development
    pub const FEATURE_DEV: &str = include_str!("commands/feature-dev.md");
    /// /review-pr command - comprehensive PR review
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
    fn test_strip_markdown_header() {
        // Should strip h1 header followed by blank line
        let with_header = "# Title\n\nContent here.";
        assert_eq!(strip_markdown_header(with_header), "Content here.");

        // Should preserve content without header
        let no_header = "Content here.";
        assert_eq!(strip_markdown_header(no_header), "Content here.");

        // Should preserve h2 headers (section headers, not file titles)
        let section = "## Section\n\nContent";
        assert_eq!(strip_markdown_header(section), "## Section\n\nContent");

        // Should not strip h1 without blank line after
        let no_blank = "# Title\nContent here.";
        assert_eq!(strip_markdown_header(no_blank), "# Title\nContent here.");

        // Should handle Windows line endings
        let windows = "# Title\r\n\r\nContent here.";
        assert_eq!(strip_markdown_header(windows), "Content here.");

        // Should handle multiline content
        let multi = "# Title\n\nFirst line.\nSecond line.";
        assert_eq!(strip_markdown_header(multi), "First line.\nSecond line.");
    }

    #[test]
    fn test_system_prompt_header_stripped() {
        // Verify the actual system prompt gets its header stripped
        let stripped = strip_markdown_header(SYSTEM_PROMPT);
        // Should start with content, not "# "
        assert!(stripped.starts_with("You are Claude") || stripped.starts_with("You are Cowork"),
            "System prompt should start with 'You are...' after stripping header, got: {}",
            &stripped[..stripped.len().min(50)]);
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
        assert!(!commands::COMMIT_PUSH_PR.is_empty());
        assert!(!commands::CLEAN_GONE.is_empty());
        assert!(!commands::CODE_REVIEW.is_empty());
        assert!(!commands::FEATURE_DEV.is_empty());
        assert!(!commands::REVIEW_PR.is_empty());

        // Verify they have proper frontmatter
        assert!(commands::COMMIT.starts_with("---"));
        assert!(commands::COMMIT_PUSH_PR.starts_with("---"));
        assert!(commands::CLEAN_GONE.starts_with("---"));
        assert!(commands::CODE_REVIEW.starts_with("---"));
        assert!(commands::FEATURE_DEV.starts_with("---"));
        assert!(commands::REVIEW_PR.starts_with("---"));

        // Verify they contain expected descriptions
        assert!(commands::COMMIT.contains("Create a git commit"));
        assert!(commands::COMMIT_PUSH_PR.contains("Commit, push, and open a PR"));
        assert!(commands::CLEAN_GONE.contains("Cleans up all git branches"));
        assert!(commands::CODE_REVIEW.contains("Code review a pull request"));
        assert!(commands::FEATURE_DEV.contains("Guided feature development"));
        assert!(commands::REVIEW_PR.contains("Comprehensive PR review"));
    }
}
