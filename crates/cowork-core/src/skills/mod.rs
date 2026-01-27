//! Skills/Commands system for common workflows
//!
//! Skills are prompt templates (SKILL.md format) that get expanded with context
//! and injected into the conversation for the LLM to follow. This matches
//! Claude Code's plugin/command system.
//!
//! Built-in skills are embedded strings. Custom skills are loaded from:
//! - User level: `~/.claude/skills/`
//! - Project level: `{workspace}/.cowork/skills/`

pub mod builtins;
pub mod installer;
pub mod loader;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Type alias for boxed futures (for object-safe async trait methods)
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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
pub trait Skill: Send + Sync {
    /// Get skill information
    fn info(&self) -> SkillInfo;

    /// Execute the skill - gathers context and returns a prompt for the LLM
    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult>;

    /// Get the prompt template for this skill (instructions for the LLM)
    fn prompt_template(&self) -> &str;

    /// Get the list of allowed tools for this skill (None = all tools allowed)
    fn allowed_tools(&self) -> Option<Vec<&str>> {
        None
    }

    /// Whether this skill should run in a forked subagent context
    /// Default is false (run inline in main loop)
    fn runs_in_subagent(&self) -> bool {
        false
    }

    /// Get the agent type to use when running in subagent mode
    /// Returns None to use the default "general-purpose" agent
    fn subagent_type(&self) -> Option<&str> {
        None
    }

    /// Get the model override for this skill (None = use default model)
    fn model_override(&self) -> Option<&str> {
        None
    }
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

    /// Create a registry with built-in skills and filesystem skills
    pub fn with_builtins(workspace: std::path::PathBuf) -> Self {
        let mut registry = Self::new();

        // Load embedded built-in skills (SKILL.md format)
        for skill in builtins::load_builtin_skills() {
            registry.register(skill);
        }

        // Load dynamic skills from filesystem
        // Project skills override user skills with the same name
        // User/project skills can override built-in skills
        let skill_loader = loader::SkillLoader::new(&workspace);
        for skill in skill_loader.load_all() {
            registry.register(skill);
        }

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
