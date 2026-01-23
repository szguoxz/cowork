//! Claude Code prompt system
//!
//! This module contains prompts adapted from Claude Code's prompt system.
//! The prompts have been pre-expanded to replace build-time template variables
//! with their literal values, while keeping runtime variables (like ${WORKING_DIRECTORY})
//! for substitution at runtime.
//!
//! # Structure
//!
//! - `system/` - Main system prompt
//! - `tools/` - Tool descriptions
//! - `agents/` - Agent definitions
//! - `reminders/` - System reminders
//!
//! # Usage
//!
//! ```rust,ignore
//! use cowork_core::prompt::builtin::claude_code;
//!
//! // Get the main system prompt
//! let system = claude_code::SYSTEM_PROMPT;
//!
//! // Get a tool description
//! let bash_desc = claude_code::tools::BASH;
//!
//! // Get an agent definition
//! let explore_agent = claude_code::agents::EXPLORE;
//! ```
//!
//! # Runtime Variables
//!
//! The following variables are substituted at runtime by `TemplateVars`:
//!
//! - `${WORKING_DIRECTORY}` - Current working directory
//! - `${IS_GIT_REPO}` - Whether the directory is a git repo
//! - `${GIT_STATUS}` - Git status output
//! - `${CURRENT_BRANCH}` - Current git branch
//! - `${MAIN_BRANCH}` - Main/master branch name
//! - `${CURRENT_DATE}` - Today's date
//! - `${CURRENT_YEAR}` - Current year
//! - `${PLATFORM}` - Operating system
//! - `${OS_VERSION}` - OS version string
//! - `${MODEL_INFO}` - Model name and ID
//! - `${ASSISTANT_NAME}` - Assistant name
//! - `${SECURITY_POLICY}` - Security policy content

/// Main system prompt (pre-expanded from Claude Code)
pub const SYSTEM_PROMPT: &str = include_str!("system/main.md");

/// Tool descriptions
pub mod tools {
    /// Bash tool - execute shell commands
    pub const BASH: &str = include_str!("tools/bash.md");

    /// Read tool - read file contents
    pub const READ: &str = include_str!("tools/read.md");

    /// Write tool - write files
    pub const WRITE: &str = include_str!("tools/write.md");

    /// Edit tool - edit files with replacements
    pub const EDIT: &str = include_str!("tools/edit.md");

    /// Glob tool - find files by pattern
    pub const GLOB: &str = include_str!("tools/glob.md");

    /// Grep tool - search file contents
    pub const GREP: &str = include_str!("tools/grep.md");

    /// Task tool - launch subagents
    pub const TASK: &str = include_str!("tools/task.md");

    /// TodoWrite tool - task management
    pub const TODOWRITE: &str = include_str!("tools/todowrite.md");

    /// AskUserQuestion tool - ask user questions
    pub const ASK_USER_QUESTION: &str = include_str!("tools/askuserquestion.md");

    /// WebFetch tool - fetch web content
    pub const WEBFETCH: &str = include_str!("tools/webfetch.md");

    /// WebSearch tool - search the web
    pub const WEBSEARCH: &str = include_str!("tools/websearch.md");

    /// EnterPlanMode tool - enter planning mode
    pub const ENTER_PLAN_MODE: &str = include_str!("tools/enterplanmode.md");

    /// ExitPlanMode tool - exit planning mode
    pub const EXIT_PLAN_MODE: &str = include_str!("tools/exitplanmode.md");

    /// LSP tool - code intelligence via Language Server Protocol
    pub const LSP: &str = include_str!("tools/lsp.md");
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

/// System reminders
pub mod reminders {
    /// Security policy
    pub const SECURITY_POLICY: &str = include_str!("reminders/security_policy.md");

    /// Plan mode active reminder
    pub const PLAN_MODE_ACTIVE: &str = include_str!("reminders/plan_mode_active.md");

    /// Git commit guidelines
    pub const GIT_COMMIT: &str = include_str!("reminders/git_commit.md");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runtime variables that should NOT be expanded at build time
    const RUNTIME_VARS: &[&str] = &[
        "${WORKING_DIRECTORY}",
        "${IS_GIT_REPO}",
        "${GIT_STATUS}",
        "${CURRENT_BRANCH}",
        "${MAIN_BRANCH}",
        "${CURRENT_DATE}",
        "${CURRENT_YEAR}",
        "${PLATFORM}",
        "${OS_VERSION}",
        "${MODEL_INFO}",
        "${ASSISTANT_NAME}",
        "${SECURITY_POLICY}",
    ];

    /// Check if a variable pattern is a runtime variable
    fn is_runtime_var(var: &str) -> bool {
        RUNTIME_VARS.iter().any(|rv| var.contains(&rv[2..rv.len()-1]))
    }

    /// Find all ${...} patterns in a string
    fn find_variables(content: &str) -> Vec<String> {
        let mut vars = Vec::new();
        let mut start = 0;

        while let Some(pos) = content[start..].find("${") {
            let abs_pos = start + pos;
            if let Some(end) = content[abs_pos..].find('}') {
                let var = &content[abs_pos..abs_pos + end + 1];
                vars.push(var.to_string());
                start = abs_pos + end + 1;
            } else {
                break;
            }
        }

        vars
    }

    #[test]
    fn test_system_prompt_loads() {
        assert!(!SYSTEM_PROMPT.is_empty());
        // Should contain key sections
        assert!(SYSTEM_PROMPT.contains("Security"));
        assert!(SYSTEM_PROMPT.contains("Tool"));
    }

    #[test]
    fn test_tools_load() {
        assert!(!tools::BASH.is_empty());
        assert!(!tools::READ.is_empty());
        assert!(!tools::WRITE.is_empty());
        assert!(!tools::EDIT.is_empty());
        assert!(!tools::GLOB.is_empty());
        assert!(!tools::GREP.is_empty());
        assert!(!tools::TASK.is_empty());
        assert!(!tools::TODOWRITE.is_empty());
        assert!(!tools::ASK_USER_QUESTION.is_empty());
        assert!(!tools::WEBFETCH.is_empty());
        assert!(!tools::WEBSEARCH.is_empty());
        assert!(!tools::ENTER_PLAN_MODE.is_empty());
        assert!(!tools::EXIT_PLAN_MODE.is_empty());
        assert!(!tools::LSP.is_empty());
    }

    #[test]
    fn test_agents_load() {
        assert!(!agents::EXPLORE.is_empty());
        assert!(!agents::PLAN.is_empty());
        assert!(!agents::BASH.is_empty());
        assert!(!agents::GENERAL.is_empty());

        // Verify they have proper frontmatter
        assert!(agents::EXPLORE.starts_with("---"));
        assert!(agents::PLAN.starts_with("---"));
        assert!(agents::BASH.starts_with("---"));
        assert!(agents::GENERAL.starts_with("---"));
    }

    #[test]
    fn test_reminders_load() {
        assert!(!reminders::SECURITY_POLICY.is_empty());
        assert!(!reminders::PLAN_MODE_ACTIVE.is_empty());
        assert!(!reminders::GIT_COMMIT.is_empty());
    }

    #[test]
    fn test_no_unexpanded_build_time_variables() {
        let prompts = [
            ("SYSTEM_PROMPT", SYSTEM_PROMPT),
            ("tools::BASH", tools::BASH),
            ("tools::READ", tools::READ),
            ("tools::WRITE", tools::WRITE),
            ("tools::EDIT", tools::EDIT),
            ("tools::GLOB", tools::GLOB),
            ("tools::GREP", tools::GREP),
            ("tools::TASK", tools::TASK),
            ("tools::TODOWRITE", tools::TODOWRITE),
            ("tools::ASK_USER_QUESTION", tools::ASK_USER_QUESTION),
            ("tools::WEBFETCH", tools::WEBFETCH),
            ("tools::WEBSEARCH", tools::WEBSEARCH),
            ("tools::ENTER_PLAN_MODE", tools::ENTER_PLAN_MODE),
            ("tools::EXIT_PLAN_MODE", tools::EXIT_PLAN_MODE),
            ("tools::LSP", tools::LSP),
            ("agents::EXPLORE", agents::EXPLORE),
            ("agents::PLAN", agents::PLAN),
            ("agents::BASH", agents::BASH),
            ("agents::GENERAL", agents::GENERAL),
            ("reminders::SECURITY_POLICY", reminders::SECURITY_POLICY),
            ("reminders::PLAN_MODE_ACTIVE", reminders::PLAN_MODE_ACTIVE),
            ("reminders::GIT_COMMIT", reminders::GIT_COMMIT),
        ];

        for (name, content) in prompts {
            let vars = find_variables(content);
            let unexpanded: Vec<_> = vars
                .iter()
                .filter(|v| !is_runtime_var(v))
                .collect();

            assert!(
                unexpanded.is_empty(),
                "Found unexpanded build-time variables in {}: {:?}",
                name,
                unexpanded
            );
        }
    }

    #[test]
    fn test_runtime_variables_present_where_expected() {
        // System prompt should have runtime variables
        assert!(SYSTEM_PROMPT.contains("${WORKING_DIRECTORY}"));
        assert!(SYSTEM_PROMPT.contains("${CURRENT_DATE}"));
        assert!(SYSTEM_PROMPT.contains("${PLATFORM}"));
        assert!(SYSTEM_PROMPT.contains("${MODEL_INFO}"));

        // WebSearch should have date/year variables
        assert!(tools::WEBSEARCH.contains("${CURRENT_DATE}"));
        assert!(tools::WEBSEARCH.contains("${CURRENT_YEAR}"));

        // Bash tool should have assistant name for co-author
        assert!(tools::BASH.contains("${ASSISTANT_NAME}"));
    }
}
