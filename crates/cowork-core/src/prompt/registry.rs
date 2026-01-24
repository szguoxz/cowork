//! Component Registry for Cowork
//!
//! This module provides unified component discovery and loading with scope priority.
//! The registry manages all prompt system components:
//! - Agents (specialized subagents)
//! - Skills (dynamic skill files)
//! - Commands (slash commands)
//! - Hooks (lifecycle hooks)
//!
//! # Discovery Order and Priority
//!
//! Components are loaded in order of increasing priority:
//! 1. Built-in (lowest priority) - compiled into the binary
//! 2. Plugin - from installed plugins
//! 3. User - from `~/.claude/`
//! 4. Project - from `.claude/` (highest priority)
//! 5. Enterprise - from enterprise config (if configured, overrides all)
//!
//! When components have the same name, higher priority sources override lower ones.
//!
//! # Example
//!
//! ```rust,ignore
//! use cowork_core::prompt::registry::{ComponentPaths, ComponentRegistry};
//!
//! let paths = ComponentPaths::for_project("/path/to/project");
//! let mut registry = ComponentRegistry::new();
//! registry.load_from_paths(&paths)?;
//!
//! // Access components
//! let agent = registry.get_agent("Explore");
//! let command = registry.get_command("commit");
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::prompt::agents::{AgentDefinition, AgentRegistry};
use crate::prompt::builder::SkillDefinition;
use crate::prompt::commands::{CommandDefinition, CommandRegistry};
use crate::prompt::hook_executor::load_hooks_config;
use crate::prompt::hooks::HooksConfig;
use crate::prompt::plugins::{DiscoverResult, PluginRegistry};
use crate::prompt::types::Scope;
use crate::skills::loader::{DynamicSkill, SkillSource};

// ================== Serializable Info Structs ==================

/// Serializable information about an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub description: String,
    pub scope: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
}

impl From<&AgentDefinition> for AgentInfo {
    fn from(agent: &AgentDefinition) -> Self {
        let tools: Vec<String> = agent
            .tool_restrictions()
            .allowed
            .iter()
            .map(|t| t.to_string())
            .collect();

        Self {
            name: agent.name().to_string(),
            description: agent.description().to_string(),
            scope: format!("{:?}", agent.scope).to_lowercase(),
            model: match agent.model() {
                crate::ModelPreference::Inherit => None,
                other => Some(format!("{:?}", other).to_lowercase()),
            },
            tools,
        }
    }
}

/// Serializable information about a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    pub scope: String,
}

impl From<&CommandDefinition> for CommandInfo {
    fn from(cmd: &CommandDefinition) -> Self {
        Self {
            name: cmd.name().to_string(),
            description: cmd.description().to_string(),
            scope: format!("{:?}", cmd.scope).to_lowercase(),
        }
    }
}

/// Serializable information about a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub source: String,
    pub user_invocable: bool,
    pub auto_triggers: Vec<String>,
}

impl From<&DynamicSkill> for SkillInfo {
    fn from(skill: &DynamicSkill) -> Self {
        Self {
            name: skill.frontmatter.name.clone(),
            description: skill.frontmatter.description.clone(),
            source: match skill.source {
                SkillSource::Project => "project".to_string(),
                SkillSource::User => "user".to_string(),
            },
            user_invocable: skill.frontmatter.user_invocable,
            auto_triggers: skill.frontmatter.auto_triggers.clone(),
        }
    }
}

/// Serializable information about a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub agent_count: usize,
    pub command_count: usize,
    pub skill_count: usize,
}

impl From<&crate::prompt::plugins::Plugin> for PluginInfo {
    fn from(plugin: &crate::prompt::plugins::Plugin) -> Self {
        Self {
            name: plugin.name().to_string(),
            version: plugin.version().to_string(),
            description: plugin.description().to_string(),
            author: plugin.manifest.author.clone(),
            enabled: plugin.is_enabled(),
            agent_count: plugin.agents.len(),
            command_count: plugin.commands.len(),
            skill_count: plugin.skills.len(),
        }
    }
}

/// Summary of all components in the registry (for easy serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySummary {
    pub agents: Vec<AgentInfo>,
    pub commands: Vec<CommandInfo>,
    pub skills: Vec<SkillInfo>,
    pub plugins: Vec<PluginInfo>,
    pub counts: RegistryCounts,
}

/// Component counts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryCounts {
    pub agents: usize,
    pub commands: usize,
    pub skills: usize,
    pub plugins: usize,
}

/// Paths for component discovery
///
/// Defines the filesystem locations where components are loaded from.
/// Each path type corresponds to a scope level.
#[derive(Debug, Clone, Default)]
pub struct ComponentPaths {
    /// Enterprise configuration path (highest priority)
    /// Set via environment variable or enterprise config
    pub enterprise_path: Option<PathBuf>,

    /// Project-level path (`.claude/` in project root)
    pub project_path: Option<PathBuf>,

    /// User-level path (`~/.claude/`)
    pub user_path: Option<PathBuf>,

    /// Plugin paths (installed plugins)
    pub plugin_paths: Vec<PathBuf>,
}

impl ComponentPaths {
    /// Create paths for a specific project root
    pub fn for_project(project_root: impl AsRef<Path>) -> Self {
        let project_root = project_root.as_ref();

        Self {
            enterprise_path: Self::find_enterprise_path(),
            project_path: Some(project_root.join(".claude")),
            user_path: dirs::home_dir().map(|h| h.join(".claude")),
            plugin_paths: Self::find_plugin_paths(project_root),
        }
    }

    /// Create paths without a project context (user-level only)
    pub fn user_only() -> Self {
        Self {
            enterprise_path: Self::find_enterprise_path(),
            project_path: None,
            user_path: dirs::home_dir().map(|h| h.join(".claude")),
            plugin_paths: Vec::new(),
        }
    }

    /// Find enterprise path from environment or config
    fn find_enterprise_path() -> Option<PathBuf> {
        // Check environment variable first
        if let Ok(path) = std::env::var("CLAUDE_ENTERPRISE_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Some(path);
            }
        }

        // Could also check common enterprise config locations
        // /etc/claude/, etc.
        None
    }

    /// Find plugin paths
    fn find_plugin_paths(project_root: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Project plugins
        let project_plugins = project_root.join(".claude").join("plugins");
        if project_plugins.exists() {
            paths.push(project_plugins);
        }

        // User plugins
        if let Some(home) = dirs::home_dir() {
            let user_plugins = home.join(".claude").join("plugins");
            if user_plugins.exists() {
                paths.push(user_plugins);
            }
        }

        paths
    }

    /// Get the agents directory for a path
    pub fn agents_dir(base: &Path) -> PathBuf {
        base.join("agents")
    }

    /// Get the skills directory for a path
    pub fn skills_dir(base: &Path) -> PathBuf {
        base.join("skills")
    }

    /// Get the commands directory for a path
    pub fn commands_dir(base: &Path) -> PathBuf {
        base.join("commands")
    }

    /// Get the hooks directory for a path
    pub fn hooks_dir(base: &Path) -> PathBuf {
        base.join("hooks")
    }

    /// Iterator over all paths in priority order (lowest to highest)
    pub fn iter_by_priority(&self) -> impl Iterator<Item = (&Path, Scope)> {
        let mut paths: Vec<(&Path, Scope)> = Vec::new();

        // Plugin paths (lowest after builtin)
        for path in &self.plugin_paths {
            paths.push((path.as_path(), Scope::Plugin));
        }

        // User path
        if let Some(ref path) = self.user_path {
            paths.push((path.as_path(), Scope::User));
        }

        // Project path
        if let Some(ref path) = self.project_path {
            paths.push((path.as_path(), Scope::Project));
        }

        // Enterprise path (highest)
        if let Some(ref path) = self.enterprise_path {
            paths.push((path.as_path(), Scope::Enterprise));
        }

        paths.into_iter()
    }
}

/// Unified registry for all prompt system components
///
/// The registry provides:
/// - Centralized access to agents, skills, commands, and hooks
/// - Automatic discovery from standard filesystem locations
/// - Scope-based override logic (higher scopes override lower)
/// - Plugin management and loading
/// - Thread-safe read access to components
#[derive(Debug, Default)]
pub struct ComponentRegistry {
    /// Registered agents by name
    agents: HashMap<String, AgentDefinition>,

    /// Registered skills by name
    skills: HashMap<String, DynamicSkill>,

    /// Registered commands by name
    commands: HashMap<String, CommandDefinition>,

    /// Hooks configuration (merged from all sources)
    hooks: HooksConfig,

    /// Paths used for discovery
    paths: Option<ComponentPaths>,

    /// Plugin registry
    plugins: PluginRegistry,
}

impl ComponentRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new registry with built-in components loaded
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.load_builtins();
        registry
    }

    /// Create a fully initialized registry for a workspace
    ///
    /// This is the main entry point for CLI and UI apps. It:
    /// 1. Creates a registry with built-in components
    /// 2. Discovers and loads components from standard paths
    /// 3. Returns the ready-to-use registry
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let registry = ComponentRegistry::for_workspace("/path/to/project")?;
    /// let agents = registry.summary().agents;
    /// ```
    pub fn for_workspace(workspace: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let paths = ComponentPaths::for_project(workspace);
        let mut registry = Self::with_builtins();
        registry.load_from_paths(&paths)?;
        Ok(registry)
    }

    /// Get a serializable summary of all components
    ///
    /// This is useful for CLI display and Tauri commands.
    pub fn summary(&self) -> RegistrySummary {
        let mut agents: Vec<AgentInfo> = self.agents.values().map(AgentInfo::from).collect();
        agents.sort_by(|a, b| a.name.cmp(&b.name));

        let mut commands: Vec<CommandInfo> = self.commands.values().map(CommandInfo::from).collect();
        commands.sort_by(|a, b| a.name.cmp(&b.name));

        let mut skills: Vec<SkillInfo> = self.skills.values().map(SkillInfo::from).collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));

        let plugins: Vec<PluginInfo> = self.plugins.list().map(PluginInfo::from).collect();

        RegistrySummary {
            counts: RegistryCounts {
                agents: agents.len(),
                commands: commands.len(),
                skills: skills.len(),
                plugins: plugins.len(),
            },
            agents,
            commands,
            skills,
            plugins,
        }
    }

    /// Load built-in components (agents and commands)
    pub fn load_builtins(&mut self) {
        // Load built-in agents
        let agent_registry = AgentRegistry::with_builtins();
        for agent in agent_registry.list() {
            self.agents.insert(agent.name().to_string(), agent.clone());
        }

        // Load built-in commands
        let command_registry = CommandRegistry::with_builtins();
        for command in command_registry.list() {
            self.commands.insert(command.name().to_string(), command.clone());
        }

        // Note: Skills don't have built-ins currently
    }

    /// Load all components from the specified paths
    ///
    /// This is the main entry point for component discovery. It loads:
    /// - Agents from `{path}/agents/*.md`
    /// - Skills from `{path}/skills/*/SKILL.md`
    /// - Commands from `{path}/commands/*.md`
    /// - Hooks from `{path}/hooks/hooks.json`
    pub fn load_from_paths(&mut self, paths: &ComponentPaths) -> Result<LoadResult, RegistryError> {
        self.paths = Some(paths.clone());

        let mut result = LoadResult::default();

        // Load in priority order (lowest to highest)
        for (base_path, scope) in paths.iter_by_priority() {
            // Load agents
            let agents_dir = ComponentPaths::agents_dir(base_path);
            if agents_dir.exists() {
                result.agents_loaded += self.load_agents_from_dir(&agents_dir, scope)?;
            }

            // Load skills
            let skills_dir = ComponentPaths::skills_dir(base_path);
            if skills_dir.exists() {
                result.skills_loaded += self.load_skills_from_dir(&skills_dir, scope)?;
            }

            // Load commands
            let commands_dir = ComponentPaths::commands_dir(base_path);
            if commands_dir.exists() {
                result.commands_loaded += self.load_commands_from_dir(&commands_dir, scope)?;
            }

            // Load hooks
            let hooks_file = ComponentPaths::hooks_dir(base_path).join("hooks.json");
            if hooks_file.exists()
                && let Ok(config) = load_hooks_config(&hooks_file)
            {
                self.hooks.merge(config);
                result.hooks_loaded += 1;
            }
        }

        // Load plugins from plugin paths
        if !paths.plugin_paths.is_empty() {
            let plugin_result = self.load_plugins(&paths.plugin_paths)?;
            result.plugins_loaded = plugin_result.loaded;
        }

        Ok(result)
    }

    /// Load plugins from the specified directories
    ///
    /// Each directory should contain plugin subdirectories, each with a plugin.json manifest.
    pub fn load_plugins(&mut self, plugin_dirs: &[PathBuf]) -> Result<DiscoverResult, RegistryError> {
        let result = self
            .plugins
            .discover(plugin_dirs)
            .map_err(|e| RegistryError::PluginError(e.to_string()))?;

        // Register components from loaded plugins (with Plugin scope, so they can be overridden)
        for agent in self.plugins.all_agents() {
            let name = agent.name().to_string();
            if self.should_override_agent(&name, Scope::Plugin) {
                self.agents.insert(name, agent.clone());
            }
        }

        for skill in self.plugins.all_skills() {
            let name = skill.frontmatter.name.clone();
            if self.should_override_skill(&name, Scope::Plugin) {
                self.skills.insert(name, skill.clone());
            }
        }

        for command in self.plugins.all_commands() {
            let name = command.name().to_string();
            if self.should_override_command(&name, Scope::Plugin) {
                self.commands.insert(name, command.clone());
            }
        }

        // Merge plugin hooks
        self.hooks.merge(self.plugins.merged_hooks());

        Ok(result)
    }

    /// Load agents from a directory
    fn load_agents_from_dir(&mut self, dir: &Path, scope: Scope) -> Result<usize, RegistryError> {
        let mut loaded = 0;

        if !dir.exists() {
            return Ok(0);
        }

        let entries = std::fs::read_dir(dir).map_err(RegistryError::IoError)?;

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Only process .md files
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            match crate::prompt::agents::load_agent_from_file(&path, scope) {
                Ok(agent) => {
                    let name = agent.name().to_string();

                    // Only insert if higher priority than existing
                    if self.should_override_agent(&name, scope) {
                        self.agents.insert(name, agent);
                        loaded += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load agent from {}: {}", path.display(), e);
                }
            }
        }

        Ok(loaded)
    }

    /// Load skills from a directory (expects subdirectories with SKILL.md)
    fn load_skills_from_dir(&mut self, dir: &Path, scope: Scope) -> Result<usize, RegistryError> {
        let mut loaded = 0;

        if !dir.exists() {
            return Ok(0);
        }

        let entries = std::fs::read_dir(dir).map_err(RegistryError::IoError)?;

        // Map scope to SkillSource
        let source = match scope {
            Scope::Project => SkillSource::Project,
            _ => SkillSource::User,
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Only process directories
            if !path.is_dir() {
                continue;
            }

            match DynamicSkill::load(&path, source) {
                Ok(skill) => {
                    let name = skill.frontmatter.name.clone();

                    // Only insert if higher priority than existing
                    if self.should_override_skill(&name, scope) {
                        self.skills.insert(name, skill);
                        loaded += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load skill from {}: {}", path.display(), e);
                }
            }
        }

        Ok(loaded)
    }

    /// Load commands from a directory
    fn load_commands_from_dir(&mut self, dir: &Path, scope: Scope) -> Result<usize, RegistryError> {
        let mut loaded = 0;

        if !dir.exists() {
            return Ok(0);
        }

        let entries = std::fs::read_dir(dir).map_err(RegistryError::IoError)?;

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Only process .md files
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            match crate::prompt::commands::load_command_from_file(&path, scope) {
                Ok(command) => {
                    let name = command.name().to_string();

                    // Only insert if higher priority than existing
                    if self.should_override_command(&name, scope) {
                        self.commands.insert(name, command);
                        loaded += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load command from {}: {}", path.display(), e);
                }
            }
        }

        Ok(loaded)
    }

    /// Check if a new agent should override an existing one
    fn should_override_agent(&self, name: &str, new_scope: Scope) -> bool {
        match self.agents.get(name) {
            Some(existing) => new_scope.overrides(&existing.scope),
            None => true,
        }
    }

    /// Check if a new skill should override an existing one
    fn should_override_skill(&self, name: &str, new_scope: Scope) -> bool {
        match self.skills.get(name) {
            Some(existing) => {
                // Skills use SkillSource, map to Scope for comparison
                let existing_scope = match existing.source {
                    SkillSource::Project => Scope::Project,
                    SkillSource::User => Scope::User,
                };
                new_scope.overrides(&existing_scope)
            }
            None => true,
        }
    }

    /// Check if a new command should override an existing one
    fn should_override_command(&self, name: &str, new_scope: Scope) -> bool {
        match self.commands.get(name) {
            Some(existing) => new_scope.overrides(&existing.scope),
            None => true,
        }
    }

    // ================== Getters ==================

    /// Get an agent by name
    pub fn get_agent(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name)
    }

    /// Get a skill by name
    pub fn get_skill(&self, name: &str) -> Option<&DynamicSkill> {
        self.skills.get(name)
    }

    /// Get a command by name
    pub fn get_command(&self, name: &str) -> Option<&CommandDefinition> {
        // Support both with and without leading slash
        let name = name.trim_start_matches('/');
        self.commands.get(name)
    }

    /// Get the hooks configuration
    pub fn get_hooks(&self) -> &HooksConfig {
        &self.hooks
    }

    // ================== Listing ==================

    /// List all agents
    pub fn list_agents(&self) -> impl Iterator<Item = &AgentDefinition> {
        self.agents.values()
    }

    /// List all skills
    pub fn list_skills(&self) -> impl Iterator<Item = &DynamicSkill> {
        self.skills.values()
    }

    /// List all commands
    pub fn list_commands(&self) -> impl Iterator<Item = &CommandDefinition> {
        self.commands.values()
    }

    /// List agent names
    pub fn agent_names(&self) -> impl Iterator<Item = &str> {
        self.agents.keys().map(|s| s.as_str())
    }

    /// List skill names
    pub fn skill_names(&self) -> impl Iterator<Item = &str> {
        self.skills.keys().map(|s| s.as_str())
    }

    /// List command names
    pub fn command_names(&self) -> impl Iterator<Item = &str> {
        self.commands.keys().map(|s| s.as_str())
    }

    // ================== Counts ==================

    /// Get the number of agents
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Get the number of skills
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }

    /// Get the number of commands
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty() && self.skills.is_empty() && self.commands.is_empty()
    }

    // ================== Special Queries ==================

    /// Get skills that can be auto-invoked
    pub fn auto_invocable_skills(&self) -> impl Iterator<Item = &DynamicSkill> {
        self.skills.values().filter(|s| !s.frontmatter.disable_model_invocation)
    }

    /// Get user-invocable skills (for /command completion)
    pub fn user_invocable_skills(&self) -> impl Iterator<Item = &DynamicSkill> {
        self.skills.values().filter(|s| s.frontmatter.user_invocable)
    }

    /// Find skills matching a user input trigger
    pub fn find_matching_skills(&self, user_input: &str) -> Vec<&DynamicSkill> {
        self.skills
            .values()
            .filter(|s| s.frontmatter.matches_auto_trigger(user_input))
            .collect()
    }

    /// Convert a skill to a SkillDefinition for the builder
    pub fn skill_to_definition(skill: &DynamicSkill) -> SkillDefinition {
        SkillDefinition::from(skill)
    }

    // ================== Registration ==================

    /// Register an agent
    pub fn register_agent(&mut self, agent: AgentDefinition) {
        let name = agent.name().to_string();
        if self.should_override_agent(&name, agent.scope) {
            self.agents.insert(name, agent);
        }
    }

    /// Register a skill
    pub fn register_skill(&mut self, skill: DynamicSkill) {
        let name = skill.frontmatter.name.clone();
        let scope = match skill.source {
            SkillSource::Project => Scope::Project,
            SkillSource::User => Scope::User,
        };
        if self.should_override_skill(&name, scope) {
            self.skills.insert(name, skill);
        }
    }

    /// Register a command
    pub fn register_command(&mut self, command: CommandDefinition) {
        let name = command.name().to_string();
        if self.should_override_command(&name, command.scope) {
            self.commands.insert(name, command);
        }
    }

    /// Merge hooks configuration
    pub fn merge_hooks(&mut self, config: HooksConfig) {
        self.hooks.merge(config);
    }

    // ================== Conversion ==================

    /// Create an AgentRegistry view of the agents
    pub fn to_agent_registry(&self) -> AgentRegistry {
        let mut registry = AgentRegistry::new();
        for agent in self.agents.values() {
            registry.register(agent.clone());
        }
        registry
    }

    /// Create a CommandRegistry view of the commands
    pub fn to_command_registry(&self) -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        for command in self.commands.values() {
            registry.register(command.clone());
        }
        registry
    }

    // ================== Plugin Access ==================

    /// Get the plugin registry
    pub fn plugins(&self) -> &PluginRegistry {
        &self.plugins
    }

    /// Get a mutable reference to the plugin registry
    pub fn plugins_mut(&mut self) -> &mut PluginRegistry {
        &mut self.plugins
    }

    /// Get a plugin by name
    pub fn get_plugin(&self, name: &str) -> Option<&crate::prompt::plugins::Plugin> {
        self.plugins.get(name)
    }

    /// List all loaded plugins
    pub fn list_plugins(&self) -> impl Iterator<Item = &crate::prompt::plugins::Plugin> {
        self.plugins.list()
    }

    /// Get the number of loaded plugins
    pub fn plugin_count(&self) -> usize {
        self.plugins.count()
    }
}

/// Result of loading components
#[derive(Debug, Default)]
pub struct LoadResult {
    /// Number of agents loaded
    pub agents_loaded: usize,

    /// Number of skills loaded
    pub skills_loaded: usize,

    /// Number of commands loaded
    pub commands_loaded: usize,

    /// Number of hooks files loaded
    pub hooks_loaded: usize,

    /// Number of plugins loaded
    pub plugins_loaded: usize,
}

impl LoadResult {
    /// Total number of components loaded
    pub fn total(&self) -> usize {
        self.agents_loaded
            + self.skills_loaded
            + self.commands_loaded
            + self.hooks_loaded
            + self.plugins_loaded
    }

    /// Check if any components were loaded
    pub fn any_loaded(&self) -> bool {
        self.total() > 0
    }
}

/// Error type for registry operations
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Agent error: {0}")]
    AgentError(#[from] crate::prompt::agents::AgentError),

    #[error("Command error: {0}")]
    CommandError(#[from] crate::prompt::commands::CommandError),

    #[error("Skill error: {0}")]
    SkillError(String),

    #[error("Plugin error: {0}")]
    PluginError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    mod component_paths_tests {
        use super::*;

        #[test]
        fn test_for_project() {
            let temp = TempDir::new().unwrap();
            let paths = ComponentPaths::for_project(temp.path());

            assert!(paths.project_path.is_some());
            assert_eq!(paths.project_path.unwrap(), temp.path().join(".claude"));
        }

        #[test]
        fn test_user_only() {
            let paths = ComponentPaths::user_only();

            assert!(paths.project_path.is_none());
            // User path depends on home directory
            if dirs::home_dir().is_some() {
                assert!(paths.user_path.is_some());
            }
        }

        #[test]
        fn test_agents_dir() {
            let base = PathBuf::from("/test");
            assert_eq!(ComponentPaths::agents_dir(&base), PathBuf::from("/test/agents"));
        }

        #[test]
        fn test_skills_dir() {
            let base = PathBuf::from("/test");
            assert_eq!(ComponentPaths::skills_dir(&base), PathBuf::from("/test/skills"));
        }

        #[test]
        fn test_commands_dir() {
            let base = PathBuf::from("/test");
            assert_eq!(ComponentPaths::commands_dir(&base), PathBuf::from("/test/commands"));
        }

        #[test]
        fn test_hooks_dir() {
            let base = PathBuf::from("/test");
            assert_eq!(ComponentPaths::hooks_dir(&base), PathBuf::from("/test/hooks"));
        }

        #[test]
        fn test_iter_by_priority_with_project() {
            let temp = TempDir::new().unwrap();
            let paths = ComponentPaths {
                enterprise_path: None,
                project_path: Some(temp.path().to_path_buf()),
                user_path: Some(PathBuf::from("/user/.claude")),
                plugin_paths: vec![],
            };

            let items: Vec<_> = paths.iter_by_priority().collect();

            // Should be: user (lower) -> project (higher)
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].1, Scope::User);
            assert_eq!(items[1].1, Scope::Project);
        }

        #[test]
        fn test_iter_by_priority_with_enterprise() {
            let paths = ComponentPaths {
                enterprise_path: Some(PathBuf::from("/enterprise")),
                project_path: Some(PathBuf::from("/project")),
                user_path: Some(PathBuf::from("/user")),
                plugin_paths: vec![PathBuf::from("/plugin")],
            };

            let items: Vec<_> = paths.iter_by_priority().collect();

            // Order: plugin -> user -> project -> enterprise
            assert_eq!(items.len(), 4);
            assert_eq!(items[0].1, Scope::Plugin);
            assert_eq!(items[1].1, Scope::User);
            assert_eq!(items[2].1, Scope::Project);
            assert_eq!(items[3].1, Scope::Enterprise);
        }
    }

    mod component_registry_tests {
        use super::*;

        #[test]
        fn test_new() {
            let registry = ComponentRegistry::new();
            assert!(registry.is_empty());
            assert_eq!(registry.agent_count(), 0);
            assert_eq!(registry.skill_count(), 0);
            assert_eq!(registry.command_count(), 0);
        }

        #[test]
        fn test_with_builtins() {
            let registry = ComponentRegistry::with_builtins();

            // Should have built-in agents
            assert!(registry.agent_count() >= 4);
            assert!(registry.get_agent("Explore").is_some());
            assert!(registry.get_agent("Plan").is_some());
            assert!(registry.get_agent("Bash").is_some());

            // Should have built-in commands
            assert_eq!(registry.command_count(), 6);
            assert!(registry.get_command("commit").is_some());
            assert!(registry.get_command("commit-push-pr").is_some());
            assert!(registry.get_command("review-pr").is_some());
        }

        #[test]
        fn test_get_command_with_slash() {
            let registry = ComponentRegistry::with_builtins();

            // Both with and without slash should work
            assert!(registry.get_command("commit").is_some());
            assert!(registry.get_command("/commit").is_some());
        }

        #[test]
        fn test_register_agent() {
            let mut registry = ComponentRegistry::new();

            let agent = crate::prompt::agents::parse_agent(
                "---\nname: TestAgent\ndescription: Test\n---\n\nPrompt",
                None,
                Scope::User,
            )
            .unwrap();

            registry.register_agent(agent);

            assert_eq!(registry.agent_count(), 1);
            assert!(registry.get_agent("TestAgent").is_some());
        }

        #[test]
        fn test_register_command() {
            let mut registry = ComponentRegistry::new();

            let command = crate::prompt::commands::parse_command(
                "---\nname: test-cmd\n---\n\nContent",
                None,
                Scope::User,
            )
            .unwrap();

            registry.register_command(command);

            assert_eq!(registry.command_count(), 1);
            assert!(registry.get_command("test-cmd").is_some());
        }

        #[test]
        fn test_agent_scope_override() {
            let mut registry = ComponentRegistry::new();

            // Register builtin agent
            let builtin = crate::prompt::agents::parse_agent(
                "---\nname: Test\ndescription: Builtin\n---\n\nBuiltin prompt",
                None,
                Scope::Builtin,
            )
            .unwrap();
            registry.register_agent(builtin);

            assert_eq!(registry.get_agent("Test").unwrap().description(), "Builtin");

            // Register user agent (should override)
            let user = crate::prompt::agents::parse_agent(
                "---\nname: Test\ndescription: User\n---\n\nUser prompt",
                None,
                Scope::User,
            )
            .unwrap();
            registry.register_agent(user);

            assert_eq!(registry.get_agent("Test").unwrap().description(), "User");

            // Register another builtin (should NOT override user)
            let builtin2 = crate::prompt::agents::parse_agent(
                "---\nname: Test\ndescription: Builtin2\n---\n\nBuiltin2 prompt",
                None,
                Scope::Builtin,
            )
            .unwrap();
            registry.register_agent(builtin2);

            assert_eq!(registry.get_agent("Test").unwrap().description(), "User");
        }

        #[test]
        fn test_command_scope_override() {
            let mut registry = ComponentRegistry::new();

            // Register builtin command
            let builtin = crate::prompt::commands::parse_command(
                "---\nname: test\ndescription: Builtin\n---\n\nBuiltin",
                None,
                Scope::Builtin,
            )
            .unwrap();
            registry.register_command(builtin);

            assert_eq!(registry.get_command("test").unwrap().description(), "Builtin");

            // Register project command (should override)
            let project = crate::prompt::commands::parse_command(
                "---\nname: test\ndescription: Project\n---\n\nProject",
                None,
                Scope::Project,
            )
            .unwrap();
            registry.register_command(project);

            assert_eq!(registry.get_command("test").unwrap().description(), "Project");
        }

        #[test]
        fn test_list_agents() {
            let registry = ComponentRegistry::with_builtins();
            let agents: Vec<_> = registry.list_agents().collect();
            assert!(!agents.is_empty());
        }

        #[test]
        fn test_list_commands() {
            let registry = ComponentRegistry::with_builtins();
            let commands: Vec<_> = registry.list_commands().collect();
            assert!(!commands.is_empty());
        }

        #[test]
        fn test_agent_names() {
            let registry = ComponentRegistry::with_builtins();
            let names: Vec<_> = registry.agent_names().collect();
            assert!(names.contains(&"Explore"));
            assert!(names.contains(&"Plan"));
        }

        #[test]
        fn test_command_names() {
            let registry = ComponentRegistry::with_builtins();
            let names: Vec<_> = registry.command_names().collect();
            assert!(names.contains(&"commit"));
            assert!(names.contains(&"commit-push-pr"));
            assert!(names.contains(&"review-pr"));
        }

        #[test]
        fn test_to_agent_registry() {
            let registry = ComponentRegistry::with_builtins();
            let agent_registry = registry.to_agent_registry();

            assert!(agent_registry.get("Explore").is_some());
            assert!(agent_registry.get("Plan").is_some());
        }

        #[test]
        fn test_to_command_registry() {
            let registry = ComponentRegistry::with_builtins();
            let command_registry = registry.to_command_registry();

            assert!(command_registry.get("commit").is_some());
            assert!(command_registry.get("review-pr").is_some());
        }
    }

    mod load_result_tests {
        use super::*;

        #[test]
        fn test_default() {
            let result = LoadResult::default();
            assert_eq!(result.total(), 0);
            assert!(!result.any_loaded());
        }

        #[test]
        fn test_total() {
            let result = LoadResult {
                agents_loaded: 2,
                skills_loaded: 3,
                commands_loaded: 4,
                hooks_loaded: 1,
                plugins_loaded: 0,
            };

            assert_eq!(result.total(), 10);
            assert!(result.any_loaded());
        }

        #[test]
        fn test_total_with_plugins() {
            let result = LoadResult {
                agents_loaded: 1,
                skills_loaded: 1,
                commands_loaded: 1,
                hooks_loaded: 1,
                plugins_loaded: 2,
            };

            assert_eq!(result.total(), 6);
        }
    }

    mod filesystem_tests {
        use super::*;

        fn create_agent_file(dir: &Path, name: &str, description: &str) {
            std::fs::create_dir_all(dir).unwrap();
            let content = format!(
                "---\nname: {}\ndescription: {}\n---\n\nAgent prompt",
                name, description
            );
            std::fs::write(dir.join(format!("{}.md", name)), content).unwrap();
        }

        fn create_command_file(dir: &Path, name: &str, description: &str) {
            std::fs::create_dir_all(dir).unwrap();
            let content = format!(
                "---\nname: {}\ndescription: {}\n---\n\nCommand content",
                name, description
            );
            std::fs::write(dir.join(format!("{}.md", name)), content).unwrap();
        }

        fn create_skill_file(dir: &Path, name: &str, description: &str) {
            let skill_dir = dir.join(name);
            std::fs::create_dir_all(&skill_dir).unwrap();
            let content = format!(
                "---\nname: {}\ndescription: {}\n---\n\nSkill instructions",
                name, description
            );
            std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
        }

        #[test]
        fn test_load_agents_from_dir() {
            let temp = TempDir::new().unwrap();
            let agents_dir = temp.path().join("agents");

            create_agent_file(&agents_dir, "test-agent", "Test agent");

            let mut registry = ComponentRegistry::new();
            let loaded = registry.load_agents_from_dir(&agents_dir, Scope::User).unwrap();

            assert_eq!(loaded, 1);
            assert!(registry.get_agent("test-agent").is_some());
        }

        #[test]
        fn test_load_commands_from_dir() {
            let temp = TempDir::new().unwrap();
            let commands_dir = temp.path().join("commands");

            create_command_file(&commands_dir, "test-cmd", "Test command");

            let mut registry = ComponentRegistry::new();
            let loaded = registry.load_commands_from_dir(&commands_dir, Scope::User).unwrap();

            assert_eq!(loaded, 1);
            assert!(registry.get_command("test-cmd").is_some());
        }

        #[test]
        fn test_load_skills_from_dir() {
            let temp = TempDir::new().unwrap();
            let skills_dir = temp.path().join("skills");

            create_skill_file(&skills_dir, "test-skill", "Test skill");

            let mut registry = ComponentRegistry::new();
            let loaded = registry.load_skills_from_dir(&skills_dir, Scope::User).unwrap();

            assert_eq!(loaded, 1);
            assert!(registry.get_skill("test-skill").is_some());
        }

        #[test]
        fn test_load_from_paths() {
            let temp = TempDir::new().unwrap();
            let base = temp.path().join(".claude");

            // Create agents
            create_agent_file(&base.join("agents"), "my-agent", "My agent");

            // Create commands
            create_command_file(&base.join("commands"), "my-cmd", "My command");

            // Create skills
            create_skill_file(&base.join("skills"), "my-skill", "My skill");

            let paths = ComponentPaths {
                enterprise_path: None,
                project_path: Some(base),
                user_path: None,
                plugin_paths: vec![],
            };

            let mut registry = ComponentRegistry::with_builtins();
            let result = registry.load_from_paths(&paths).unwrap();

            assert!(result.agents_loaded >= 1);
            assert_eq!(result.skills_loaded, 1);
            assert!(result.commands_loaded >= 1);

            assert!(registry.get_agent("my-agent").is_some());
            assert!(registry.get_command("my-cmd").is_some());
            assert!(registry.get_skill("my-skill").is_some());
        }

        #[test]
        fn test_scope_override_during_load() {
            let temp = TempDir::new().unwrap();

            // Create user agent
            let user_dir = temp.path().join("user").join(".claude");
            create_agent_file(&user_dir.join("agents"), "shared", "User version");

            // Create project agent
            let project_dir = temp.path().join("project").join(".claude");
            create_agent_file(&project_dir.join("agents"), "shared", "Project version");

            let paths = ComponentPaths {
                enterprise_path: None,
                project_path: Some(project_dir),
                user_path: Some(user_dir),
                plugin_paths: vec![],
            };

            let mut registry = ComponentRegistry::new();
            registry.load_from_paths(&paths).unwrap();

            // Project version should win
            let agent = registry.get_agent("shared").unwrap();
            assert_eq!(agent.description(), "Project version");
        }

        #[test]
        fn test_load_from_nonexistent_paths() {
            let paths = ComponentPaths {
                enterprise_path: None,
                project_path: Some(PathBuf::from("/nonexistent")),
                user_path: None,
                plugin_paths: vec![],
            };

            let mut registry = ComponentRegistry::new();
            let result = registry.load_from_paths(&paths).unwrap();

            // Should succeed with zero loaded
            assert_eq!(result.total(), 0);
        }
    }

    mod skill_query_tests {
        use super::*;

        fn create_test_skill(name: &str, auto_triggers: Vec<&str>, disable: bool) -> DynamicSkill {
            let triggers: Vec<String> = auto_triggers.into_iter().map(|s| s.to_string()).collect();
            let triggers_yaml = if triggers.is_empty() {
                String::new()
            } else {
                format!(
                    "auto-triggers:\n{}",
                    triggers
                        .iter()
                        .map(|t| format!("  - \"{}\"", t))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };

            let content = format!(
                "---\nname: {}\ndescription: Test\ndisable-model-invocation: {}\n{}\n---\n\nBody",
                name, disable, triggers_yaml
            );

            DynamicSkill::parse(&content, PathBuf::from("/test"), SkillSource::User).unwrap()
        }

        #[test]
        fn test_auto_invocable_skills() {
            let mut registry = ComponentRegistry::new();

            registry.register_skill(create_test_skill("enabled", vec![], false));
            registry.register_skill(create_test_skill("disabled", vec![], true));

            let auto_invocable: Vec<_> = registry.auto_invocable_skills().collect();
            assert_eq!(auto_invocable.len(), 1);
            assert_eq!(auto_invocable[0].frontmatter.name, "enabled");
        }

        #[test]
        fn test_find_matching_skills() {
            let mut registry = ComponentRegistry::new();

            registry.register_skill(create_test_skill("build", vec!["build the project", "compile"], false));
            registry.register_skill(create_test_skill("test", vec!["run tests", "test"], false));
            registry.register_skill(create_test_skill("deploy", vec!["deploy"], true)); // disabled

            let matches = registry.find_matching_skills("please build the project");
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].frontmatter.name, "build");

            let matches = registry.find_matching_skills("run tests please");
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].frontmatter.name, "test");

            // Deploy is disabled, should not match
            let matches = registry.find_matching_skills("deploy the app");
            assert!(matches.is_empty());
        }
    }

    mod plugin_integration_tests {
        use super::*;

        fn create_plugin_dir(temp: &TempDir, name: &str) -> PathBuf {
            let plugins_dir = temp.path().join("plugins");
            std::fs::create_dir_all(&plugins_dir).unwrap();

            let plugin_dir = plugins_dir.join(name);
            std::fs::create_dir_all(&plugin_dir).unwrap();

            // Create manifest
            let manifest = format!(
                r#"{{
                    "name": "{}",
                    "version": "1.0.0",
                    "agents": ["agents/*.md"],
                    "commands": ["commands/*.md"]
                }}"#,
                name
            );
            std::fs::write(plugin_dir.join("plugin.json"), manifest).unwrap();

            // Create agent
            let agents_dir = plugin_dir.join("agents");
            std::fs::create_dir_all(&agents_dir).unwrap();
            std::fs::write(
                agents_dir.join("agent.md"),
                format!(
                    "---\nname: {}-agent\ndescription: Plugin agent\n---\n\nPrompt",
                    name
                ),
            )
            .unwrap();

            // Create command
            let commands_dir = plugin_dir.join("commands");
            std::fs::create_dir_all(&commands_dir).unwrap();
            std::fs::write(
                commands_dir.join("cmd.md"),
                format!(
                    "---\nname: {}-cmd\ndescription: Plugin command\n---\n\nContent",
                    name
                ),
            )
            .unwrap();

            plugins_dir
        }

        #[test]
        fn test_load_plugins_into_registry() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_plugin_dir(&temp, "test-plugin");

            let mut registry = ComponentRegistry::new();
            let result = registry.load_plugins(&[plugins_dir]).unwrap();

            assert_eq!(result.loaded, 1);
            assert!(registry.get_agent("test-plugin-agent").is_some());
            assert!(registry.get_command("test-plugin-cmd").is_some());
        }

        #[test]
        fn test_plugin_components_accessible() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_plugin_dir(&temp, "my-plugin");

            let paths = ComponentPaths {
                enterprise_path: None,
                project_path: None,
                user_path: None,
                plugin_paths: vec![plugins_dir],
            };

            let mut registry = ComponentRegistry::new();
            let result = registry.load_from_paths(&paths).unwrap();

            assert_eq!(result.plugins_loaded, 1);
            assert!(registry.get_agent("my-plugin-agent").is_some());
            assert!(registry.get_command("my-plugin-cmd").is_some());
        }

        #[test]
        fn test_plugin_count() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_plugin_dir(&temp, "plugin1");

            // Add a second plugin
            let plugin2_dir = plugins_dir.join("plugin2");
            std::fs::create_dir_all(&plugin2_dir).unwrap();
            std::fs::write(
                plugin2_dir.join("plugin.json"),
                r#"{"name": "plugin2", "version": "1.0.0"}"#,
            )
            .unwrap();

            let mut registry = ComponentRegistry::new();
            registry.load_plugins(&[plugins_dir]).unwrap();

            assert_eq!(registry.plugin_count(), 2);
        }

        #[test]
        fn test_get_plugin() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_plugin_dir(&temp, "test-plugin");

            let mut registry = ComponentRegistry::new();
            registry.load_plugins(&[plugins_dir]).unwrap();

            let plugin = registry.get_plugin("test-plugin");
            assert!(plugin.is_some());
            assert_eq!(plugin.unwrap().name(), "test-plugin");
        }

        #[test]
        fn test_list_plugins() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_plugin_dir(&temp, "plugin1");

            let plugin2_dir = plugins_dir.join("plugin2");
            std::fs::create_dir_all(&plugin2_dir).unwrap();
            std::fs::write(
                plugin2_dir.join("plugin.json"),
                r#"{"name": "plugin2", "version": "1.0.0"}"#,
            )
            .unwrap();

            let mut registry = ComponentRegistry::new();
            registry.load_plugins(&[plugins_dir]).unwrap();

            let plugins: Vec<_> = registry.list_plugins().collect();
            assert_eq!(plugins.len(), 2);
        }

        #[test]
        fn test_plugin_overrides_builtin() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = temp.path().join("plugins");
            let plugin_dir = plugins_dir.join("override-plugin");
            std::fs::create_dir_all(&plugin_dir).unwrap();

            // Create manifest
            std::fs::write(
                plugin_dir.join("plugin.json"),
                r#"{
                    "name": "override-plugin",
                    "version": "1.0.0",
                    "agents": ["agents/*.md"]
                }"#,
            )
            .unwrap();

            // Create agent that overrides built-in Explore
            let agents_dir = plugin_dir.join("agents");
            std::fs::create_dir_all(&agents_dir).unwrap();
            std::fs::write(
                agents_dir.join("explore.md"),
                "---\nname: Explore\ndescription: Plugin override\n---\n\nCustom explore",
            )
            .unwrap();

            // First load builtins
            let mut registry = ComponentRegistry::with_builtins();

            // Builtin Explore should be present
            let builtin_explore = registry.get_agent("Explore").unwrap();
            assert_ne!(builtin_explore.description(), "Plugin override");

            // Now load plugins - Plugin scope is higher than Builtin
            registry.load_plugins(&[plugins_dir]).unwrap();

            // Plugin override should have replaced builtin
            let agent = registry.get_agent("Explore").unwrap();
            assert_eq!(agent.description(), "Plugin override");
        }

        #[test]
        fn test_user_overrides_plugin() {
            let temp = TempDir::new().unwrap();

            // Create plugin
            let plugins_dir = temp.path().join("plugins");
            let plugin_dir = plugins_dir.join("my-plugin");
            std::fs::create_dir_all(&plugin_dir).unwrap();
            std::fs::write(
                plugin_dir.join("plugin.json"),
                r#"{
                    "name": "my-plugin",
                    "version": "1.0.0",
                    "agents": ["agents/*.md"]
                }"#,
            )
            .unwrap();
            let plugin_agents_dir = plugin_dir.join("agents");
            std::fs::create_dir_all(&plugin_agents_dir).unwrap();
            std::fs::write(
                plugin_agents_dir.join("shared.md"),
                "---\nname: SharedAgent\ndescription: From plugin\n---\n\nPlugin prompt",
            )
            .unwrap();

            // Create user agent
            let user_dir = temp.path().join("user").join("agents");
            std::fs::create_dir_all(&user_dir).unwrap();
            std::fs::write(
                user_dir.join("shared.md"),
                "---\nname: SharedAgent\ndescription: From user\n---\n\nUser prompt",
            )
            .unwrap();

            let paths = ComponentPaths {
                enterprise_path: None,
                project_path: None,
                user_path: Some(temp.path().join("user")),
                plugin_paths: vec![plugins_dir],
            };

            let mut registry = ComponentRegistry::new();
            registry.load_from_paths(&paths).unwrap();

            // User version should win over plugin
            let agent = registry.get_agent("SharedAgent").unwrap();
            assert_eq!(agent.description(), "From user");
        }
    }
}
