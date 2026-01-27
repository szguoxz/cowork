//! System prompt management
//!
//! Provides integration between the orchestration layer and the prompt system.
//! This module bridges the legacy static prompt approach with the new dynamic
//! prompt builder system.

use std::path::Path;
use std::sync::Arc;

use crate::config::PromptSystemConfig;
use crate::prompt::{
    builtin, AssembledPrompt, ComponentRegistry, HooksConfig,
    PromptBuilder, TemplateVars,
};

// Used in tests
#[cfg(test)]
use crate::prompt::ModelPreference;

/// System prompt configuration and generation
///
/// This struct provides both legacy compatibility (simple string prompts)
/// and integration with the new prompt system (PromptBuilder, hooks, etc.)
#[derive(Debug)]
pub struct SystemPrompt {
    /// Base system prompt (from builtin or custom)
    base: String,
    /// Additional context (e.g., workspace info)
    context: Option<String>,
    /// Template variables for substitution
    template_vars: Option<TemplateVars>,
    /// Component registry for agents, commands, skills (shared via Arc)
    registry: Option<Arc<ComponentRegistry>>,
    /// Hooks configuration
    hooks: Option<HooksConfig>,
}

impl Default for SystemPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemPrompt {
    /// Create a new system prompt with the default content
    pub fn new() -> Self {
        Self {
            base: builtin::strip_markdown_header(builtin::SYSTEM_PROMPT).to_string(),
            context: None,
            template_vars: None,
            registry: None,
            hooks: None,
        }
    }

    /// Create with custom base prompt
    pub fn with_base(base: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            context: None,
            template_vars: None,
            registry: None,
            hooks: None,
        }
    }

    /// Create a system prompt with full prompt system integration
    pub fn with_prompt_system(
        workspace_path: &Path,
        config: &PromptSystemConfig,
    ) -> Result<Self, crate::error::Error> {
        // Create component paths from config
        let paths = config.to_component_paths(workspace_path);

        // Initialize registry with builtins
        let mut registry = ComponentRegistry::with_builtins();

        // Load components from filesystem if paths exist
        if let Err(e) = registry.load_from_paths(&paths) {
            tracing::warn!("Failed to load some prompt components: {}", e);
        }

        // Get hooks from registry
        let hooks = registry.get_hooks().clone();

        // Use custom base prompt if configured, otherwise use builtin
        let base = config
            .base_system_prompt
            .clone()
            .unwrap_or_else(|| builtin::SYSTEM_PROMPT.to_string());

        Ok(Self {
            base,
            context: None,
            template_vars: None,
            registry: Some(Arc::new(registry)),
            hooks: Some(hooks),
        })
    }

    /// Add workspace context to the prompt
    pub fn with_workspace_context(mut self, workspace_path: &Path) -> Self {
        // Create template vars if not already set
        let mut vars = self.template_vars.take().unwrap_or_default();
        vars.working_directory = workspace_path.display().to_string();
        vars.is_git_repo = workspace_path.join(".git").exists();

        // Also add legacy context format
        let context = format!(
            "\n\n## Current Workspace\nYou are working in: {}",
            workspace_path.display()
        );
        self.context = Some(context);
        self.template_vars = Some(vars);
        self
    }

    /// Add template variables for substitution
    pub fn with_template_vars(mut self, vars: TemplateVars) -> Self {
        self.template_vars = Some(vars);
        self
    }

    /// Add custom context
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Set the component registry
    pub fn with_registry(mut self, registry: ComponentRegistry) -> Self {
        // Also extract hooks from registry
        self.hooks = Some(registry.get_hooks().clone());
        self.registry = Some(Arc::new(registry));
        self
    }

    /// Set the component registry (Arc version)
    pub fn with_registry_arc(mut self, registry: Arc<ComponentRegistry>) -> Self {
        // Also extract hooks from registry
        self.hooks = Some(registry.get_hooks().clone());
        self.registry = Some(registry);
        self
    }

    /// Build the final system prompt string (legacy interface)
    pub fn build(&self) -> String {
        let mut prompt = self.base.clone();

        // Apply template variable substitution if available
        if let Some(vars) = &self.template_vars {
            prompt = vars.substitute(&prompt);
        }

        // Append context if present
        if let Some(ctx) = &self.context {
            prompt.push_str(ctx);
        }

        prompt
    }

    /// Build an AssembledPrompt using the PromptBuilder
    ///
    /// This is the preferred method for new code, as it returns
    /// the full assembled prompt with tool restrictions and metadata.
    pub fn build_assembled(&self) -> AssembledPrompt {
        let base = if let Some(vars) = &self.template_vars {
            vars.substitute(&self.base)
        } else {
            self.base.clone()
        };

        let mut builder = PromptBuilder::new(base);

        // Add context if present
        if let Some(ctx) = &self.context {
            builder = builder.with_hook_context(ctx.clone());
        }

        // Add template vars if present
        if let Some(vars) = &self.template_vars {
            builder = builder.with_environment(vars);
        }

        builder.build()
    }

    /// Get the base prompt without context
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Get the component registry if available
    pub fn registry(&self) -> Option<&ComponentRegistry> {
        self.registry.as_deref()
    }

    /// Get the component registry Arc if available
    pub fn registry_arc(&self) -> Option<&Arc<ComponentRegistry>> {
        self.registry.as_ref()
    }

    /// Get the hooks configuration if available
    pub fn hooks(&self) -> Option<&HooksConfig> {
        self.hooks.as_ref()
    }

    /// Get template variables if set
    pub fn template_vars(&self) -> Option<&TemplateVars> {
        self.template_vars.as_ref()
    }

    /// Create a PromptBuilder from this system prompt
    ///
    /// This allows further customization before building the final prompt.
    pub fn to_builder(&self) -> PromptBuilder {
        let base = if let Some(vars) = &self.template_vars {
            vars.substitute(&self.base)
        } else {
            self.base.clone()
        };

        let mut builder = PromptBuilder::new(base);

        if let Some(ctx) = &self.context {
            builder = builder.with_hook_context(ctx.clone());
        }

        if let Some(vars) = &self.template_vars {
            builder = builder.with_environment(vars);
        }

        builder
    }

    /// Get an agent definition by name from the registry
    pub fn get_agent(&self, name: &str) -> Option<&crate::prompt::AgentDefinition> {
        self.registry()?.get_agent(name)
    }

    /// Get a command definition by name from the registry
    pub fn get_command(&self, name: &str) -> Option<&crate::prompt::CommandDefinition> {
        self.registry()?.get_command(name)
    }

    /// List all available agent names
    pub fn list_agents(&self) -> Vec<String> {
        self.registry()
            .map(|r| r.agent_names().map(String::from).collect())
            .unwrap_or_default()
    }

    /// List all available command names
    pub fn list_commands(&self) -> Vec<String> {
        self.registry()
            .map(|r| r.command_names().map(String::from).collect())
            .unwrap_or_default()
    }
}

/// Default system prompt used by both CLI and UI (legacy constant)
///
/// For new code, prefer using `builtin::SYSTEM_PROMPT` directly or
/// the `SystemPrompt::new()` constructor.
pub const DEFAULT_SYSTEM_PROMPT: &str = builtin::SYSTEM_PROMPT;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_new_system_prompt() {
        let prompt = SystemPrompt::new();
        let built = prompt.build();
        // System prompt is now Claude Code's pre-expanded prompt
        assert!(built.contains("Claude"));
        assert!(built.contains("Security"));
    }

    #[test]
    fn test_with_base() {
        let prompt = SystemPrompt::with_base("Custom base prompt");
        assert_eq!(prompt.base(), "Custom base prompt");
        assert_eq!(prompt.build(), "Custom base prompt");
    }

    #[test]
    fn test_with_context() {
        let prompt = SystemPrompt::new().with_context("Additional context here");
        let built = prompt.build();
        assert!(built.contains("Additional context here"));
    }

    #[test]
    fn test_with_workspace_context() {
        let workspace = PathBuf::from("/test/workspace");
        let prompt = SystemPrompt::new().with_workspace_context(&workspace);
        let built = prompt.build();
        assert!(built.contains("/test/workspace"));
    }

    #[test]
    fn test_template_vars_substitution() {
        let prompt = SystemPrompt::with_base("Working in: ${WORKING_DIRECTORY}")
            .with_template_vars(TemplateVars {
                working_directory: "/my/project".to_string(),
                ..Default::default()
            });
        let built = prompt.build();
        assert!(built.contains("/my/project"));
        assert!(!built.contains("${WORKING_DIRECTORY}"));
    }

    #[test]
    fn test_build_assembled() {
        let prompt = SystemPrompt::new();
        let assembled = prompt.build_assembled();
        assert!(!assembled.system_prompt.is_empty());
        assert!(matches!(assembled.model, ModelPreference::Inherit));
    }

    #[test]
    fn test_to_builder() {
        let prompt = SystemPrompt::with_base("Base prompt")
            .with_context("Extra context");
        let builder = prompt.to_builder();
        let assembled = builder.build();
        assert!(assembled.system_prompt.contains("Base prompt"));
        assert!(assembled.system_prompt.contains("Extra context"));
    }

    #[test]
    fn test_default_prompt_constant() {
        // Ensure the constant matches the builtin
        assert_eq!(DEFAULT_SYSTEM_PROMPT, builtin::SYSTEM_PROMPT);
    }

    #[test]
    fn test_registry_access() {
        let prompt = SystemPrompt::new();
        // Without registry set, should return None
        assert!(prompt.registry().is_none());
        assert!(prompt.get_agent("Explore").is_none());
        assert!(prompt.list_agents().is_empty());
    }

    #[test]
    fn test_with_registry() {
        let registry = ComponentRegistry::with_builtins();
        let prompt = SystemPrompt::new().with_registry(registry);

        // Now registry should be available
        assert!(prompt.registry().is_some());

        // Should have builtin agents
        let agents = prompt.list_agents();
        assert!(!agents.is_empty());
        assert!(agents.contains(&"Explore".to_string()) || agents.contains(&"explore".to_string()));
    }
}
