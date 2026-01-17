//! Skills/Commands system for common workflows
//!
//! Skills are predefined workflows triggered by slash commands like /commit, /pr, /review.
//! Each skill has a prompt template and may use specific tools.

pub mod git;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Information about a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    /// Skill name (e.g., "commit")
    pub name: String,
    /// Display name (e.g., "Git Commit")
    pub display_name: String,
    /// Description of what the skill does
    pub description: String,
    /// Example usage
    pub usage: String,
    /// Whether this skill is user-invocable (via slash command)
    pub user_invocable: bool,
}

/// Result of executing a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResult {
    /// Whether the skill succeeded
    pub success: bool,
    /// The response/output from the skill
    pub response: String,
    /// Any additional data
    pub data: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
}

impl SkillResult {
    pub fn success(response: impl Into<String>) -> Self {
        Self {
            success: true,
            response: response.into(),
            data: None,
            error: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn error(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self {
            success: false,
            response: String::new(),
            data: None,
            error: Some(msg),
        }
    }
}

/// Context provided to skills
pub struct SkillContext {
    /// Working directory
    pub workspace: std::path::PathBuf,
    /// Arguments passed to the skill
    pub args: String,
    /// Additional context data
    pub data: HashMap<String, serde_json::Value>,
}

/// Trait for implementing skills
#[async_trait]
pub trait Skill: Send + Sync {
    /// Get skill information
    fn info(&self) -> SkillInfo;

    /// Execute the skill
    async fn execute(&self, ctx: SkillContext) -> SkillResult;

    /// Get the prompt template for this skill
    fn prompt_template(&self) -> &str;
}

/// Registry of available skills
#[derive(Default)]
pub struct SkillRegistry {
    skills: HashMap<String, Arc<dyn Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with built-in skills
    pub fn with_builtins(workspace: std::path::PathBuf) -> Self {
        let mut registry = Self::new();

        // Register git skills
        registry.register(Arc::new(git::CommitSkill::new(workspace.clone())));
        registry.register(Arc::new(git::PushSkill::new(workspace.clone())));
        registry.register(Arc::new(git::PullRequestSkill::new(workspace.clone())));
        registry.register(Arc::new(git::ReviewSkill::new(workspace.clone())));

        // Register help skill
        registry.register(Arc::new(HelpSkill::new()));

        registry
    }

    /// Register a skill
    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        let info = skill.info();
        self.skills.insert(info.name.clone(), skill);
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Skill>> {
        self.skills.get(name).cloned()
    }

    /// List all skills
    pub fn list(&self) -> Vec<SkillInfo> {
        self.skills.values().map(|s| s.info()).collect()
    }

    /// List user-invocable skills
    pub fn list_user_invocable(&self) -> Vec<SkillInfo> {
        self.skills
            .values()
            .map(|s| s.info())
            .filter(|i| i.user_invocable)
            .collect()
    }

    /// Execute a skill by name
    pub async fn execute(&self, name: &str, ctx: SkillContext) -> SkillResult {
        match self.get(name) {
            Some(skill) => skill.execute(ctx).await,
            None => SkillResult::error(format!("Unknown skill: {}. Use /help to see available commands.", name)),
        }
    }

    /// Parse a slash command and execute it
    pub async fn execute_command(&self, command: &str, workspace: std::path::PathBuf) -> SkillResult {
        let command = command.trim();

        if !command.starts_with('/') {
            return SkillResult::error("Commands must start with /");
        }

        let parts: Vec<&str> = command[1..].splitn(2, ' ').collect();
        let name = parts[0];
        let args = parts.get(1).unwrap_or(&"").to_string();

        let ctx = SkillContext {
            workspace,
            args,
            data: HashMap::new(),
        };

        self.execute(name, ctx).await
    }
}

/// Built-in help skill
struct HelpSkill;

impl HelpSkill {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for HelpSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "help".to_string(),
            display_name: "Help".to_string(),
            description: "List available commands and their usage".to_string(),
            usage: "/help".to_string(),
            user_invocable: true,
        }
    }

    async fn execute(&self, _ctx: SkillContext) -> SkillResult {
        let help_text = r#"
Available Commands:

/commit         - Stage changes and create a git commit with a generated message
/push           - Push commits to the remote repository
/pr [title]     - Create a pull request with auto-generated description
/review         - Review staged changes and provide feedback
/help           - Show this help message

Keyboard Shortcuts:

Y               - Approve all pending tool calls
N               - Reject all pending tool calls
Escape          - Cancel the current operation
Ctrl+Enter      - Send message

Tips:
- Commands can be combined with additional instructions
- Example: "/commit and then push to remote"
"#;

        SkillResult::success(help_text.trim())
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}
