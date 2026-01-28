//! Plugin System for Cowork
//!
//! This module provides plugin packaging and distribution for prompt components.
//! Plugins bundle agents, skills, commands, and hooks into distributable packages.
//!
//! # Plugin Structure
//!
//! A plugin is a directory containing:
//! - `plugin.json` - Plugin manifest with metadata
//! - `agents/` - Agent definition files (*.md)
//! - `skills/` - Skill directories (each with SKILL.md)
//! - `commands/` - Command definition files (*.md)
//! - `hooks/hooks.json` - Hook configurations
//!
//! # Example plugin.json
//!
//! ```json
//! {
//!   "name": "my-plugin",
//!   "version": "1.0.0",
//!   "description": "Plugin description",
//!   "author": "Author Name",
//!   "agents": ["agents/*.md"],
//!   "skills": ["skills/*/SKILL.md"],
//!   "commands": ["commands/*.md"],
//!   "hooks": "hooks/hooks.json"
//! }
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use cowork_core::prompt::plugins::{Plugin, PluginRegistry};
//!
//! let mut registry = PluginRegistry::new();
//! registry.discover(&["/path/to/.claude/plugins"])?;
//!
//! let plugin = registry.get("my-plugin");
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::prompt::agents::{AgentDefinition, AgentError};
use crate::prompt::commands::{CommandDefinition, CommandError};
use crate::prompt::hook_executor::load_hooks_config;
use crate::prompt::hooks::HooksConfig;
use crate::prompt::types::Scope;
use crate::skills::loader::{DynamicSkill, SkillSource};

/// Plugin manifest parsed from plugin.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (unique identifier)
    pub name: String,

    /// Plugin version (semver)
    pub version: String,

    /// Plugin description
    #[serde(default)]
    pub description: String,

    /// Plugin author
    #[serde(default)]
    pub author: String,

    /// Glob patterns for agent files (relative to plugin root)
    #[serde(default)]
    pub agents: Vec<String>,

    /// Glob patterns for skill directories (relative to plugin root)
    #[serde(default)]
    pub skills: Vec<String>,

    /// Glob patterns for command files (relative to plugin root)
    #[serde(default)]
    pub commands: Vec<String>,

    /// Path to hooks.json file (relative to plugin root)
    #[serde(default)]
    pub hooks: Option<String>,

    /// Whether the plugin is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Plugin homepage or repository URL
    #[serde(default)]
    pub homepage: Option<String>,

    /// Plugin license
    #[serde(default)]
    pub license: Option<String>,

    /// Minimum cowork version required
    #[serde(default)]
    pub min_cowork_version: Option<String>,

    /// Keywords for discovery
    #[serde(default)]
    pub keywords: Vec<String>,
}

fn default_enabled() -> bool {
    true
}

impl PluginManifest {
    /// Parse a manifest from JSON content
    pub fn parse(content: &str) -> Result<Self, PluginError> {
        serde_json::from_str(content).map_err(|e| PluginError::InvalidManifest(e.to_string()))
    }

    /// Load a manifest from a file
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| PluginError::IoError(path.to_path_buf(), e.to_string()))?;
        Self::parse(&content)
    }

    /// Validate the manifest
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.name.is_empty() {
            return Err(PluginError::ValidationError("Plugin name is required".to_string()));
        }

        if self.version.is_empty() {
            return Err(PluginError::ValidationError("Plugin version is required".to_string()));
        }

        // Validate plugin name (alphanumeric, hyphens, underscores)
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(PluginError::ValidationError(format!(
                "Invalid plugin name '{}': must contain only alphanumeric characters, hyphens, or underscores",
                self.name
            )));
        }

        Ok(())
    }
}

impl Default for PluginManifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: String::new(),
            description: String::new(),
            author: String::new(),
            agents: Vec::new(),
            skills: Vec::new(),
            commands: Vec::new(),
            hooks: None,
            enabled: true,
            homepage: None,
            license: None,
            min_cowork_version: None,
            keywords: Vec::new(),
        }
    }
}

/// A loaded plugin with all its components
#[derive(Debug, Clone)]
pub struct Plugin {
    /// Plugin manifest
    pub manifest: PluginManifest,

    /// Base path of the plugin
    pub base_path: PathBuf,

    /// Loaded agent definitions
    pub agents: Vec<AgentDefinition>,

    /// Loaded skill definitions
    pub skills: Vec<DynamicSkill>,

    /// Loaded command definitions
    pub commands: Vec<CommandDefinition>,

    /// Loaded hooks configuration
    pub hooks: HooksConfig,
}

impl Plugin {
    /// Load a plugin from a directory
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        let manifest_path = path.join("plugin.json");

        if !manifest_path.exists() {
            return Err(PluginError::MissingManifest(path.to_path_buf()));
        }

        let manifest = PluginManifest::load(&manifest_path)?;
        manifest.validate()?;

        let mut plugin = Self {
            manifest,
            base_path: path.to_path_buf(),
            agents: Vec::new(),
            skills: Vec::new(),
            commands: Vec::new(),
            hooks: HooksConfig::default(),
        };

        plugin.load_components()?;

        Ok(plugin)
    }

    /// Load all components defined in the manifest
    fn load_components(&mut self) -> Result<(), PluginError> {
        self.load_agents()?;
        self.load_skills()?;
        self.load_commands()?;
        self.load_hooks()?;
        Ok(())
    }

    /// Load agent definitions
    fn load_agents(&mut self) -> Result<(), PluginError> {
        for pattern in &self.manifest.agents {
            let full_pattern = self.base_path.join(pattern);
            let pattern_str = full_pattern.to_string_lossy();

            let paths = glob::glob(&pattern_str)
                .map_err(|e| PluginError::GlobError(pattern.clone(), e.to_string()))?;

            for entry in paths.filter_map(|e| e.ok()) {
                if entry.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }

                match crate::prompt::agents::load_agent_from_file(&entry, Scope::Plugin) {
                    Ok(agent) => {
                        tracing::debug!(
                            "Loaded agent '{}' from plugin '{}'",
                            agent.name(),
                            self.manifest.name
                        );
                        self.agents.push(agent);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load agent from {} in plugin '{}': {}",
                            entry.display(),
                            self.manifest.name,
                            e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Load skill definitions
    fn load_skills(&mut self) -> Result<(), PluginError> {
        for pattern in &self.manifest.skills {
            let full_pattern = self.base_path.join(pattern);
            let pattern_str = full_pattern.to_string_lossy();

            let paths = glob::glob(&pattern_str)
                .map_err(|e| PluginError::GlobError(pattern.clone(), e.to_string()))?;

            for entry in paths.filter_map(|e| e.ok()) {
                // Skills are directories with SKILL.md inside
                if entry.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
                    continue;
                }

                let skill_dir = entry.parent().unwrap_or(&entry);

                match DynamicSkill::load(skill_dir, SkillSource::User) {
                    Ok(skill) => {
                        tracing::debug!(
                            "Loaded skill '{}' from plugin '{}'",
                            skill.frontmatter.name,
                            self.manifest.name
                        );
                        self.skills.push(skill);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load skill from {} in plugin '{}': {}",
                            skill_dir.display(),
                            self.manifest.name,
                            e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Load command definitions
    fn load_commands(&mut self) -> Result<(), PluginError> {
        for pattern in &self.manifest.commands {
            let full_pattern = self.base_path.join(pattern);
            let pattern_str = full_pattern.to_string_lossy();

            let paths = glob::glob(&pattern_str)
                .map_err(|e| PluginError::GlobError(pattern.clone(), e.to_string()))?;

            for entry in paths.filter_map(|e| e.ok()) {
                if entry.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }

                match crate::prompt::commands::load_command_from_file(&entry, Scope::Plugin) {
                    Ok(command) => {
                        tracing::debug!(
                            "Loaded command '{}' from plugin '{}'",
                            command.name(),
                            self.manifest.name
                        );
                        self.commands.push(command);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load command from {} in plugin '{}': {}",
                            entry.display(),
                            self.manifest.name,
                            e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Load hooks configuration
    fn load_hooks(&mut self) -> Result<(), PluginError> {
        if let Some(ref hooks_path) = self.manifest.hooks {
            let full_path = self.base_path.join(hooks_path);
            if full_path.exists() {
                match load_hooks_config(&full_path) {
                    Ok(config) => {
                        tracing::debug!("Loaded hooks from plugin '{}'", self.manifest.name);
                        self.hooks = config;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load hooks from {} in plugin '{}': {}",
                            full_path.display(),
                            self.manifest.name,
                            e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the plugin name
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Get the plugin version
    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    /// Get the plugin description
    pub fn description(&self) -> &str {
        &self.manifest.description
    }

    /// Check if the plugin is enabled
    pub fn is_enabled(&self) -> bool {
        self.manifest.enabled
    }

    /// Get the number of components in this plugin
    pub fn component_count(&self) -> usize {
        self.agents.len()
            + self.skills.len()
            + self.commands.len()
            + if self.hooks.is_empty() { 0 } else { 1 }
    }
}

/// Registry for managing plugins
#[derive(Debug, Default)]
pub struct PluginRegistry {
    /// Loaded plugins by name
    plugins: HashMap<String, Plugin>,

    /// Disabled plugins (name -> reason)
    disabled: HashMap<String, String>,
}

impl PluginRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Discover and load plugins from the given directories
    ///
    /// Each directory should contain plugin subdirectories, each with a plugin.json manifest.
    pub fn discover(&mut self, plugin_dirs: &[PathBuf]) -> Result<DiscoverResult, PluginError> {
        let mut result = DiscoverResult::default();

        for dir in plugin_dirs {
            if !dir.exists() {
                continue;
            }

            let entries = std::fs::read_dir(dir)
                .map_err(|e| PluginError::IoError(dir.clone(), e.to_string()))?;

            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();

                // Only process directories
                if !path.is_dir() {
                    continue;
                }

                match Plugin::load(&path) {
                    Ok(plugin) => {
                        let name = plugin.name().to_string();

                        if !plugin.is_enabled() {
                            self.disabled.insert(name.clone(), "Disabled in manifest".to_string());
                            result.disabled += 1;
                            continue;
                        }

                        // Check for conflicts
                        if self.plugins.contains_key(&name) {
                            tracing::warn!(
                                "Plugin '{}' at {} conflicts with already loaded plugin, skipping",
                                name,
                                path.display()
                            );
                            result.conflicts += 1;
                            continue;
                        }

                        result.loaded += 1;
                        result.agents += plugin.agents.len();
                        result.skills += plugin.skills.len();
                        result.commands += plugin.commands.len();

                        self.plugins.insert(name, plugin);
                    }
                    Err(PluginError::MissingManifest(_)) => {
                        // Not a plugin directory, skip silently
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load plugin from {}: {}", path.display(), e);
                        result.failed += 1;
                    }
                }
            }
        }

        Ok(result)
    }

    /// Load a plugin from a specific path
    pub fn load_plugin(&mut self, path: &Path) -> Result<&Plugin, PluginError> {
        let plugin = Plugin::load(path)?;
        let name = plugin.name().to_string();

        if self.plugins.contains_key(&name) {
            return Err(PluginError::Conflict(name));
        }

        self.plugins.insert(name.clone(), plugin);
        Ok(self.plugins.get(&name).unwrap())
    }

    /// Get a plugin by name
    pub fn get(&self, name: &str) -> Option<&Plugin> {
        self.plugins.get(name)
    }

    /// List all loaded plugins
    pub fn list(&self) -> impl Iterator<Item = &Plugin> {
        self.plugins.values()
    }

    /// List all plugin names
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.plugins.keys().map(|s| s.as_str())
    }

    /// Get the number of loaded plugins
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Check if a plugin is loaded
    pub fn contains(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// Check if a plugin is disabled
    pub fn is_disabled(&self, name: &str) -> bool {
        self.disabled.contains_key(name)
    }

    /// Get the reason a plugin is disabled
    pub fn disabled_reason(&self, name: &str) -> Option<&str> {
        self.disabled.get(name).map(|s| s.as_str())
    }

    /// Enable a disabled plugin
    pub fn enable(&mut self, name: &str) -> Result<(), PluginError> {
        if let Some(plugin) = self.plugins.get_mut(name) {
            if plugin.manifest.enabled {
                return Ok(()); // Already enabled
            }
            plugin.manifest.enabled = true;
            self.disabled.remove(name);
            Ok(())
        } else if self.disabled.contains_key(name) {
            // Plugin is disabled and not loaded - would need to reload
            self.disabled.remove(name);
            // Note: Full reload would require knowing the original path
            Ok(())
        } else {
            Err(PluginError::NotFound(name.to_string()))
        }
    }

    /// Disable a loaded plugin
    pub fn disable(&mut self, name: &str, reason: &str) -> Result<(), PluginError> {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.manifest.enabled = false;
            self.disabled.insert(name.to_string(), reason.to_string());
            Ok(())
        } else {
            Err(PluginError::NotFound(name.to_string()))
        }
    }

    /// Unload a plugin
    pub fn unload(&mut self, name: &str) -> Option<Plugin> {
        self.plugins.remove(name)
    }

    /// Get all agents from all enabled plugins
    pub fn all_agents(&self) -> impl Iterator<Item = &AgentDefinition> {
        self.plugins
            .values()
            .filter(|p| p.is_enabled())
            .flat_map(|p| p.agents.iter())
    }

    /// Get all skills from all enabled plugins
    pub fn all_skills(&self) -> impl Iterator<Item = &DynamicSkill> {
        self.plugins
            .values()
            .filter(|p| p.is_enabled())
            .flat_map(|p| p.skills.iter())
    }

    /// Get all commands from all enabled plugins
    pub fn all_commands(&self) -> impl Iterator<Item = &CommandDefinition> {
        self.plugins
            .values()
            .filter(|p| p.is_enabled())
            .flat_map(|p| p.commands.iter())
    }

    /// Get merged hooks from all enabled plugins
    pub fn merged_hooks(&self) -> HooksConfig {
        let mut merged = HooksConfig::default();
        for plugin in self.plugins.values().filter(|p| p.is_enabled()) {
            merged.merge(plugin.hooks.clone());
        }
        merged
    }
}

/// Result of plugin discovery
#[derive(Debug, Default)]
pub struct DiscoverResult {
    /// Number of plugins successfully loaded
    pub loaded: usize,

    /// Number of plugins that failed to load
    pub failed: usize,

    /// Number of plugins disabled
    pub disabled: usize,

    /// Number of plugins with conflicts (same name)
    pub conflicts: usize,

    /// Total agents loaded from plugins
    pub agents: usize,

    /// Total skills loaded from plugins
    pub skills: usize,

    /// Total commands loaded from plugins
    pub commands: usize,
}

impl DiscoverResult {
    /// Total number of components loaded
    pub fn total_components(&self) -> usize {
        self.agents + self.skills + self.commands
    }

    /// Check if any plugins were loaded
    pub fn any_loaded(&self) -> bool {
        self.loaded > 0
    }
}

/// Error types for plugin operations
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Missing plugin.json manifest in {0}")]
    MissingManifest(PathBuf),

    #[error("Invalid plugin manifest: {0}")]
    InvalidManifest(String),

    #[error("Plugin validation error: {0}")]
    ValidationError(String),

    #[error("IO error for {0}: {1}")]
    IoError(PathBuf, String),

    #[error("Glob pattern error for '{0}': {1}")]
    GlobError(String, String),

    #[error("Plugin '{0}' conflicts with an already loaded plugin")]
    Conflict(String),

    #[error("Plugin '{0}' not found")]
    NotFound(String),

    #[error("Agent error: {0}")]
    AgentError(#[from] AgentError),

    #[error("Command error: {0}")]
    CommandError(#[from] CommandError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    mod plugin_manifest_tests {
        use super::*;

        #[test]
        fn test_parse_minimal_manifest() {
            let json = r#"{"name": "test-plugin", "version": "1.0.0"}"#;
            let manifest = PluginManifest::parse(json).unwrap();

            assert_eq!(manifest.name, "test-plugin");
            assert_eq!(manifest.version, "1.0.0");
            assert!(manifest.enabled);
            assert!(manifest.agents.is_empty());
            assert!(manifest.skills.is_empty());
            assert!(manifest.commands.is_empty());
            assert!(manifest.hooks.is_none());
        }

        #[test]
        fn test_parse_full_manifest() {
            let json = r#"{
                "name": "my-plugin",
                "version": "2.0.0",
                "description": "A test plugin",
                "author": "Test Author",
                "agents": ["agents/*.md"],
                "skills": ["skills/*/SKILL.md"],
                "commands": ["commands/*.md"],
                "hooks": "hooks/hooks.json",
                "enabled": true,
                "homepage": "https://example.com",
                "license": "MIT",
                "min_cowork_version": "0.1.0",
                "keywords": ["test", "plugin"]
            }"#;

            let manifest = PluginManifest::parse(json).unwrap();

            assert_eq!(manifest.name, "my-plugin");
            assert_eq!(manifest.version, "2.0.0");
            assert_eq!(manifest.description, "A test plugin");
            assert_eq!(manifest.author, "Test Author");
            assert_eq!(manifest.agents, vec!["agents/*.md"]);
            assert_eq!(manifest.skills, vec!["skills/*/SKILL.md"]);
            assert_eq!(manifest.commands, vec!["commands/*.md"]);
            assert_eq!(manifest.hooks, Some("hooks/hooks.json".to_string()));
            assert!(manifest.enabled);
            assert_eq!(manifest.homepage, Some("https://example.com".to_string()));
            assert_eq!(manifest.license, Some("MIT".to_string()));
            assert_eq!(manifest.min_cowork_version, Some("0.1.0".to_string()));
            assert_eq!(manifest.keywords, vec!["test", "plugin"]);
        }

        #[test]
        fn test_parse_disabled_manifest() {
            let json = r#"{"name": "test", "version": "1.0.0", "enabled": false}"#;
            let manifest = PluginManifest::parse(json).unwrap();
            assert!(!manifest.enabled);
        }

        #[test]
        fn test_parse_invalid_json() {
            let result = PluginManifest::parse("not json");
            assert!(result.is_err());
        }

        #[test]
        fn test_validate_empty_name() {
            let manifest = PluginManifest {
                name: String::new(),
                version: "1.0.0".to_string(),
                ..Default::default()
            };

            let result = manifest.validate();
            assert!(result.is_err());
            assert!(matches!(result, Err(PluginError::ValidationError(_))));
        }

        #[test]
        fn test_validate_empty_version() {
            let manifest = PluginManifest {
                name: "test".to_string(),
                version: String::new(),
                ..Default::default()
            };

            let result = manifest.validate();
            assert!(result.is_err());
        }

        #[test]
        fn test_validate_invalid_name() {
            let manifest = PluginManifest {
                name: "test plugin!".to_string(), // Invalid character
                version: "1.0.0".to_string(),
                ..Default::default()
            };

            let result = manifest.validate();
            assert!(result.is_err());
        }

        #[test]
        fn test_validate_valid_name_with_hyphen() {
            let manifest = PluginManifest {
                name: "my-test-plugin".to_string(),
                version: "1.0.0".to_string(),
                ..Default::default()
            };

            assert!(manifest.validate().is_ok());
        }

        #[test]
        fn test_validate_valid_name_with_underscore() {
            let manifest = PluginManifest {
                name: "my_test_plugin".to_string(),
                version: "1.0.0".to_string(),
                ..Default::default()
            };

            assert!(manifest.validate().is_ok());
        }

        #[test]
        fn test_load_manifest_from_file() {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("plugin.json");

            std::fs::write(&path, r#"{"name": "test", "version": "1.0.0"}"#).unwrap();

            let manifest = PluginManifest::load(&path).unwrap();
            assert_eq!(manifest.name, "test");
        }

        #[test]
        fn test_load_manifest_not_found() {
            let result = PluginManifest::load(Path::new("/nonexistent/plugin.json"));
            assert!(result.is_err());
        }
    }

    mod plugin_tests {
        use super::*;

        fn create_test_plugin(temp: &TempDir) -> PathBuf {
            let plugin_dir = temp.path().join("test-plugin");
            std::fs::create_dir_all(&plugin_dir).unwrap();

            // Create manifest
            let manifest = r#"{
                "name": "test-plugin",
                "version": "1.0.0",
                "description": "Test plugin",
                "agents": ["agents/*.md"],
                "commands": ["commands/*.md"]
            }"#;
            std::fs::write(plugin_dir.join("plugin.json"), manifest).unwrap();

            // Create agents directory with an agent
            let agents_dir = plugin_dir.join("agents");
            std::fs::create_dir_all(&agents_dir).unwrap();
            std::fs::write(
                agents_dir.join("test-agent.md"),
                "---\nname: TestAgent\ndescription: Test\n---\n\nAgent prompt",
            )
            .unwrap();

            // Create commands directory with a command
            let commands_dir = plugin_dir.join("commands");
            std::fs::create_dir_all(&commands_dir).unwrap();
            std::fs::write(
                commands_dir.join("test-cmd.md"),
                "---\nname: test-cmd\ndescription: Test\n---\n\nCommand content",
            )
            .unwrap();

            plugin_dir
        }

        #[test]
        fn test_load_plugin() {
            let temp = TempDir::new().unwrap();
            let plugin_dir = create_test_plugin(&temp);

            let plugin = Plugin::load(&plugin_dir).unwrap();

            assert_eq!(plugin.name(), "test-plugin");
            assert_eq!(plugin.version(), "1.0.0");
            assert_eq!(plugin.agents.len(), 1);
            assert_eq!(plugin.commands.len(), 1);
        }

        #[test]
        fn test_load_plugin_missing_manifest() {
            let temp = TempDir::new().unwrap();
            let empty_dir = temp.path().join("empty");
            std::fs::create_dir_all(&empty_dir).unwrap();

            let result = Plugin::load(&empty_dir);
            assert!(result.is_err());
            assert!(matches!(result, Err(PluginError::MissingManifest(_))));
        }

        #[test]
        fn test_plugin_component_count() {
            let temp = TempDir::new().unwrap();
            let plugin_dir = create_test_plugin(&temp);

            let plugin = Plugin::load(&plugin_dir).unwrap();
            assert_eq!(plugin.component_count(), 2); // 1 agent + 1 command
        }

        #[test]
        fn test_plugin_is_enabled() {
            let temp = TempDir::new().unwrap();
            let plugin_dir = create_test_plugin(&temp);

            let plugin = Plugin::load(&plugin_dir).unwrap();
            assert!(plugin.is_enabled());
        }

        #[test]
        fn test_load_disabled_plugin() {
            let temp = TempDir::new().unwrap();
            let plugin_dir = temp.path().join("disabled-plugin");
            std::fs::create_dir_all(&plugin_dir).unwrap();

            let manifest = r#"{"name": "disabled", "version": "1.0.0", "enabled": false}"#;
            std::fs::write(plugin_dir.join("plugin.json"), manifest).unwrap();

            let plugin = Plugin::load(&plugin_dir).unwrap();
            assert!(!plugin.is_enabled());
        }
    }

    mod plugin_registry_tests {
        use super::*;

        fn create_test_plugins(temp: &TempDir) -> PathBuf {
            let plugins_dir = temp.path().join("plugins");
            std::fs::create_dir_all(&plugins_dir).unwrap();

            // Plugin 1
            let plugin1 = plugins_dir.join("plugin1");
            std::fs::create_dir_all(&plugin1).unwrap();
            std::fs::write(
                plugin1.join("plugin.json"),
                r#"{"name": "plugin1", "version": "1.0.0"}"#,
            )
            .unwrap();

            // Plugin 2
            let plugin2 = plugins_dir.join("plugin2");
            std::fs::create_dir_all(&plugin2).unwrap();
            std::fs::write(
                plugin2.join("plugin.json"),
                r#"{"name": "plugin2", "version": "2.0.0"}"#,
            )
            .unwrap();

            // Disabled plugin
            let plugin3 = plugins_dir.join("plugin3");
            std::fs::create_dir_all(&plugin3).unwrap();
            std::fs::write(
                plugin3.join("plugin.json"),
                r#"{"name": "plugin3", "version": "1.0.0", "enabled": false}"#,
            )
            .unwrap();

            plugins_dir
        }

        #[test]
        fn test_new_registry() {
            let registry = PluginRegistry::new();
            assert_eq!(registry.count(), 0);
        }

        #[test]
        fn test_discover_plugins() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            let result = registry.discover(&[plugins_dir]).unwrap();

            assert_eq!(result.loaded, 2); // plugin1 and plugin2
            assert_eq!(result.disabled, 1); // plugin3
            assert_eq!(registry.count(), 2);
        }

        #[test]
        fn test_get_plugin() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            let plugin = registry.get("plugin1").unwrap();
            assert_eq!(plugin.name(), "plugin1");

            assert!(registry.get("nonexistent").is_none());
        }

        #[test]
        fn test_contains() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            assert!(registry.contains("plugin1"));
            assert!(registry.contains("plugin2"));
            assert!(!registry.contains("plugin3")); // Disabled, not loaded
            assert!(!registry.contains("nonexistent"));
        }

        #[test]
        fn test_is_disabled() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            assert!(!registry.is_disabled("plugin1"));
            assert!(registry.is_disabled("plugin3"));
        }

        #[test]
        fn test_list_plugins() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            let plugins: Vec<_> = registry.list().collect();
            assert_eq!(plugins.len(), 2);
        }

        #[test]
        fn test_names() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            let names: Vec<_> = registry.names().collect();
            assert!(names.contains(&"plugin1"));
            assert!(names.contains(&"plugin2"));
        }

        #[test]
        fn test_unload_plugin() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            assert!(registry.contains("plugin1"));

            let unloaded = registry.unload("plugin1");
            assert!(unloaded.is_some());
            assert!(!registry.contains("plugin1"));
        }

        #[test]
        fn test_disable_plugin() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            registry.disable("plugin1", "Testing").unwrap();
            assert!(registry.is_disabled("plugin1"));
            assert_eq!(registry.disabled_reason("plugin1"), Some("Testing"));
        }

        #[test]
        fn test_enable_plugin() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = create_test_plugins(&temp);

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            registry.disable("plugin1", "Testing").unwrap();
            assert!(registry.is_disabled("plugin1"));

            registry.enable("plugin1").unwrap();
            assert!(!registry.is_disabled("plugin1"));
        }

        #[test]
        fn test_discover_nonexistent_dir() {
            let mut registry = PluginRegistry::new();
            let result = registry.discover(&[PathBuf::from("/nonexistent")]).unwrap();

            assert_eq!(result.loaded, 0);
        }

        #[test]
        fn test_conflict_detection() {
            let temp = TempDir::new().unwrap();

            // Create two plugin directories with the same plugin name
            let plugins1 = temp.path().join("plugins1");
            std::fs::create_dir_all(&plugins1).unwrap();
            let plugin1 = plugins1.join("same-name");
            std::fs::create_dir_all(&plugin1).unwrap();
            std::fs::write(
                plugin1.join("plugin.json"),
                r#"{"name": "same-name", "version": "1.0.0"}"#,
            )
            .unwrap();

            let plugins2 = temp.path().join("plugins2");
            std::fs::create_dir_all(&plugins2).unwrap();
            let plugin2 = plugins2.join("same-name-copy");
            std::fs::create_dir_all(&plugin2).unwrap();
            std::fs::write(
                plugin2.join("plugin.json"),
                r#"{"name": "same-name", "version": "2.0.0"}"#,
            )
            .unwrap();

            let mut registry = PluginRegistry::new();
            let result = registry.discover(&[plugins1, plugins2]).unwrap();

            // First one loaded, second one conflicts
            assert_eq!(result.loaded, 1);
            assert_eq!(result.conflicts, 1);
        }
    }

    mod discover_result_tests {
        use super::*;

        #[test]
        fn test_default() {
            let result = DiscoverResult::default();
            assert_eq!(result.loaded, 0);
            assert_eq!(result.total_components(), 0);
            assert!(!result.any_loaded());
        }

        #[test]
        fn test_total_components() {
            let result = DiscoverResult {
                loaded: 1,
                agents: 2,
                skills: 3,
                commands: 4,
                ..Default::default()
            };

            assert_eq!(result.total_components(), 9);
        }

        #[test]
        fn test_any_loaded() {
            let result = DiscoverResult {
                loaded: 1,
                ..Default::default()
            };

            assert!(result.any_loaded());
        }
    }

    mod all_components_tests {
        use super::*;

        fn create_plugin_with_components(base_dir: &Path, name: &str) -> PathBuf {
            let plugin_dir = base_dir.join(name);
            std::fs::create_dir_all(&plugin_dir).unwrap();

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
                    "---\nname: {}-agent\ndescription: Test\n---\n\nPrompt",
                    name
                ),
            )
            .unwrap();

            // Create command
            let commands_dir = plugin_dir.join("commands");
            std::fs::create_dir_all(&commands_dir).unwrap();
            std::fs::write(
                commands_dir.join("cmd.md"),
                format!("---\nname: {}-cmd\ndescription: Test\n---\n\nContent", name),
            )
            .unwrap();

            plugin_dir
        }

        #[test]
        fn test_all_agents() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = temp.path().join("plugins");
            std::fs::create_dir_all(&plugins_dir).unwrap();

            create_plugin_with_components(&plugins_dir, "plugin1");
            create_plugin_with_components(&plugins_dir, "plugin2");

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            let agents: Vec<_> = registry.all_agents().collect();
            assert_eq!(agents.len(), 2);
        }

        #[test]
        fn test_all_commands() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = temp.path().join("plugins");
            std::fs::create_dir_all(&plugins_dir).unwrap();

            create_plugin_with_components(&plugins_dir, "plugin1");
            create_plugin_with_components(&plugins_dir, "plugin2");

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            let commands: Vec<_> = registry.all_commands().collect();
            assert_eq!(commands.len(), 2);
        }

        #[test]
        fn test_disabled_plugin_excluded() {
            let temp = TempDir::new().unwrap();
            let plugins_dir = temp.path().join("plugins");
            std::fs::create_dir_all(&plugins_dir).unwrap();

            create_plugin_with_components(&plugins_dir, "plugin1");

            let mut registry = PluginRegistry::new();
            registry.discover(&[plugins_dir]).unwrap();

            // Disable the plugin
            registry.disable("plugin1", "Test").unwrap();

            // Should exclude components from disabled plugin
            let agents: Vec<_> = registry.all_agents().collect();
            assert_eq!(agents.len(), 0);
        }
    }
}
