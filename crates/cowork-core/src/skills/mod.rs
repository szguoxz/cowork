//! Skills/Commands system for common workflows
//!
//! Skills are predefined workflows triggered by slash commands like /commit, /pr, /review.
//! Each skill has a prompt template and may use specific tools.
//!
//! This system is inspired by Claude Code's plugin/command system where skills are
//! essentially prompt templates that get expanded with context and sent to the LLM.

pub mod builtins;
pub mod context;
pub mod dev;
pub mod git;
pub mod installer;
pub mod loader;
pub mod mcp;
pub mod memory;
pub mod permissions;
pub mod session;
pub mod skill_cmd;
pub mod vim;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::mcp_manager::McpServerManager;

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
        Self::with_builtins_and_mcp(workspace, None)
    }

    /// Create a registry with built-in skills and optional MCP manager
    pub fn with_builtins_and_mcp(
        workspace: std::path::PathBuf,
        mcp_manager: Option<Arc<McpServerManager>>,
    ) -> Self {
        let mut registry = Self::new();

        // Register git skills (mirroring Claude Code's commit-commands plugin)
        registry.register(Arc::new(git::CommitSkill::new(workspace.clone())));
        registry.register(Arc::new(git::CommitPushPrSkill::new(workspace.clone())));
        registry.register(Arc::new(git::PushSkill::new(workspace.clone())));
        registry.register(Arc::new(git::PullRequestSkill::new(workspace.clone())));
        registry.register(Arc::new(git::ReviewSkill::new(workspace.clone())));
        registry.register(Arc::new(git::CleanGoneSkill::new(workspace.clone())));

        // Register git info skills
        registry.register(Arc::new(git::StatusSkill::new(workspace.clone())));
        registry.register(Arc::new(git::DiffSkill::new(workspace.clone())));
        registry.register(Arc::new(git::LogSkill::new(workspace.clone())));
        registry.register(Arc::new(git::BranchSkill::new(workspace.clone())));

        // Register context management skills
        registry.register(Arc::new(context::CompactSkill::new(workspace.clone())));
        registry.register(Arc::new(context::ClearSkill::new(workspace.clone())));
        registry.register(Arc::new(context::ContextSkill::new(workspace.clone())));

        // Register development workflow skills
        registry.register(Arc::new(dev::TestSkill::new(workspace.clone())));
        registry.register(Arc::new(dev::BuildSkill::new(workspace.clone())));
        registry.register(Arc::new(dev::LintSkill::new(workspace.clone())));
        registry.register(Arc::new(dev::FormatSkill::new(workspace.clone())));

        // Register session management skills
        registry.register(Arc::new(session::ConfigSkill::new(workspace.clone())));
        registry.register(Arc::new(session::ModelSkill::new(workspace.clone())));
        registry.register(Arc::new(session::ProviderSkill::new(workspace.clone())));

        // Register memory management skill
        registry.register(Arc::new(memory::MemorySkill::new(workspace.clone())));

        // Register permissions skill
        registry.register(Arc::new(permissions::PermissionsSkill::new(workspace.clone())));

        // Register vim mode skill
        registry.register(Arc::new(vim::VimSkill::new(workspace.clone())));

        // Register MCP skill if manager is provided
        if let Some(manager) = mcp_manager {
            registry.register(Arc::new(mcp::McpSkill::new(manager)));
        }

        // Register skill management command
        registry.register(Arc::new(skill_cmd::SkillCmdSkill::new(workspace.clone())));

        // Register help skill
        registry.register(Arc::new(HelpSkill::new()));

        // Register built-in Claude Code-style skills
        // These are always available without external files
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

/// Built-in help skill
struct HelpSkill;

impl HelpSkill {
    fn new() -> Self {
        Self
    }
}

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

    fn execute(&self, _ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let help_text = r#"
Available Commands:

Git Commands:
  /commit           - Stage changes and create a git commit with a generated message
  /commit-push-pr   - Commit, push, and create a pull request in one step
  /push             - Push commits to the remote repository
  /pr [title]       - Create a pull request with auto-generated description
  /review           - Review staged changes and provide feedback
  /clean-gone       - Clean up local branches deleted from remote

Git Info Commands:
  /status           - Show current git status
  /diff [--staged]  - Show current changes
  /log [count]      - Show recent commits (default: 10)
  /branch [name]    - List, create, or switch branches

Development Workflow:
  /feature-dev [desc] - Guided 7-phase feature development workflow
  /code-explorer      - Deep codebase analysis and exploration
  /code-architect     - Design architecture blueprints for features

Code Review:
  /code-review [scope] - Comprehensive PR review for bugs and compliance
  /code-reviewer       - Review code with confidence-based filtering
  /pr-test-analyzer    - Analyze test coverage quality
  /silent-failure-hunter - Find silent failures and error handling issues
  /code-simplifier     - Simplify and refine code for clarity

Context Commands:
  /compact [focus]  - Summarize conversation (optionally preserve specific content)
  /clear            - Clear conversation history, keep memory files
  /context          - Show context usage statistics and memory hierarchy

Development Commands:
  /test             - Run project tests (auto-detects framework)
  /build            - Build the project
  /lint             - Run linter (clippy, eslint, ruff, etc.)
  /format           - Format code (rustfmt, prettier, black, etc.)

Session Commands:
  /config           - View current configuration
  /model            - Show or switch the active model
  /provider         - Show or switch the active provider
  /memory           - Manage CLAUDE.md memory files
  /permissions      - View tool permission settings
  /vim              - Toggle vim keybindings mode

MCP Server Commands:
  /mcp list         - List configured MCP servers and status
  /mcp add <name> <cmd> - Add a new MCP server
  /mcp remove <name>    - Remove an MCP server
  /mcp start <name>     - Start an MCP server
  /mcp stop <name>      - Stop a running server
  /mcp tools [server]   - List tools from MCP servers

Skill Management:
  /skill list           - List installed custom skills
  /skill add <url>      - Install a skill from a zip URL
  /skill remove <name>  - Remove an installed skill
  /skill info <name>    - Show details about a skill

General:
  /help             - Show this help message

Command Line Options:
  cowork [OPTIONS] [COMMAND]

  Options:
    -w, --workspace <PATH>  Workspace directory (default: .)
    -p, --provider <NAME>   LLM provider: anthropic, openai (default: anthropic)
    -m, --model <MODEL>     Model to use (default: provider's default)
    -v, --verbose           Verbose output
        --auto-approve      Auto-approve all tool calls (use with caution!)
        --one-shot <PROMPT> Execute single prompt and exit

  Commands:
    chat     Interactive chat mode (default)
    run      Execute a shell command
    list     List files in workspace
    read     Read a file
    search   Search for files
    tools    Show available tools
    config   Show configuration

  Examples:
    cowork                              # Start interactive chat
    cowork -p openai -m gpt-4.1          # Use OpenAI with GPT-4.1
    cowork --one-shot "explain main.rs" # Single prompt, then exit
    cowork tools                        # List available tools
    cowork config                       # Show current config

Keyboard Shortcuts:
  Y               - Approve all pending tool calls
  N               - Reject all pending tool calls
  Escape          - Cancel the current operation
  Ctrl+Enter      - Send message

Configuration:
  Config file: ~/.config/cowork/config.toml

  Example config:
    default_provider = "anthropic"

    [providers.anthropic]
    provider_type = "anthropic"
    model = "claude-sonnet-4-20250514"
    api_key_env = "ANTHROPIC_API_KEY"

    [providers.openai]
    provider_type = "openai"
    model = "gpt-4.1"
    api_key_env = "OPENAI_API_KEY"

    [approval]
    auto_approve_level = "medium"  # none, low, medium, high, all

    [browser]
    headless = true

    [general]
    log_level = "info"

  To edit: $EDITOR ~/.config/cowork/config.toml
  See /config for current settings, /permissions for approval levels.

Memory Files:
  Project instructions: ./CLAUDE.md or ./.claude/CLAUDE.md
  Personal settings:    ./CLAUDE.local.md (gitignored)
  User global:          ~/.claude/CLAUDE.md
  See /memory for details.

Tips:
- Commands can be combined with additional instructions
- Example: "/commit and then push to remote"
- Example: "/compact keep the API design decisions"
- Context is auto-compacted when usage exceeds 75%
"#;

            SkillResult::success(help_text.trim())
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}
