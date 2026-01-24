//! Prompt Pipeline for Cowork
//!
//! This module provides high-level orchestration for the prompt system,
//! coordinating hooks, agents, skills, commands, and the prompt builder.
//!
//! # Pipeline Flow
//!
//! ```text
//! User Input
//!     │
//!     ▼
//! ┌─────────────────┐
//! │ SessionStart    │ ← Run session start hooks
//! │ Hooks           │
//! └────────┬────────┘
//!          │
//!     ▼
//! ┌─────────────────┐
//! │ UserPrompt      │ ← Run user prompt hooks
//! │ Hooks           │   (may inject context)
//! └────────┬────────┘
//!          │
//!     ▼
//! ┌─────────────────┐
//! │ Command/Skill   │ ← Parse /command or match skill triggers
//! │ Detection       │
//! └────────┬────────┘
//!          │
//!     ▼
//! ┌─────────────────┐
//! │ PromptBuilder   │ ← Assemble prompt with all components
//! │                 │   (base + hooks + agent + skills + command)
//! └────────┬────────┘
//!          │
//!     ▼
//! AssembledPrompt
//! ```

use std::path::{Path, PathBuf};

use crate::prompt::agents::AgentRegistry;
use crate::prompt::builder::{AssembledPrompt, PromptBuilder, SkillDefinition};
use crate::prompt::commands::{CommandError, CommandRegistry};
use crate::prompt::hook_executor::{HookContext, HookError, HookExecutor};
use crate::prompt::hooks::{HookEvent, HookResult, HooksConfig};
use crate::prompt::types::ToolRestrictions;
use crate::prompt::TemplateVars;
use crate::prompt::builtin;
use crate::skills::loader::DynamicSkill;

/// Error type for pipeline operations
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Hook error: {0}")]
    HookError(#[from] HookError),

    #[error("Command error: {0}")]
    CommandError(#[from] CommandError),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Action blocked by hook: {0}")]
    Blocked(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result of running hooks
#[derive(Debug, Default)]
pub struct HookResults {
    /// Additional context to inject into the prompt
    pub context: Vec<String>,

    /// Whether the action was blocked
    pub blocked: bool,

    /// Reason for blocking (if blocked)
    pub block_reason: Option<String>,

    /// Any errors that occurred
    pub errors: Vec<HookError>,
}

impl HookResults {
    /// Create a new empty result
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge with results from another hook execution
    pub fn merge(&mut self, results: Vec<Result<HookResult, HookError>>) {
        for result in results {
            match result {
                Ok(hook_result) => {
                    if let Some(ctx) = hook_result.additional_context {
                        self.context.push(ctx);
                    }
                    if hook_result.block {
                        self.blocked = true;
                        if hook_result.block_reason.is_some() {
                            self.block_reason = hook_result.block_reason;
                        }
                    }
                }
                Err(e) => {
                    self.errors.push(e);
                }
            }
        }
    }
}

/// Configuration for the pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Base system prompt
    pub base_prompt: String,

    /// Template variables for substitution
    pub template_vars: TemplateVars,

    /// Hooks configuration
    pub hooks: HooksConfig,

    /// Whether to run hooks
    pub enable_hooks: bool,

    /// Project root for discovery
    pub project_root: Option<PathBuf>,

    /// Working directory for hook execution
    pub workspace: PathBuf,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            base_prompt: builtin::SYSTEM_PROMPT.to_string(),
            template_vars: TemplateVars::default(),
            hooks: HooksConfig::default(),
            enable_hooks: true,
            project_root: None,
            workspace: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}

/// Prompt Pipeline - high-level orchestration for the prompt system
///
/// The pipeline coordinates:
/// - Hook execution at lifecycle points
/// - Command detection and expansion
/// - Skill matching and loading
/// - Agent configuration
/// - Final prompt assembly
pub struct PromptPipeline {
    /// Configuration
    config: PipelineConfig,

    /// Hook executor
    hook_executor: HookExecutor,

    /// Agent registry
    agents: AgentRegistry,

    /// Command registry
    commands: CommandRegistry,

    /// Session ID for hook context
    session_id: String,
}

impl PromptPipeline {
    /// Create a new pipeline with default configuration
    pub fn new() -> Self {
        Self::with_config(PipelineConfig::default())
    }

    /// Create a new pipeline with custom configuration
    pub fn with_config(config: PipelineConfig) -> Self {
        let hook_executor = HookExecutor::new(config.workspace.clone());
        Self {
            hook_executor,
            agents: AgentRegistry::with_builtins(),
            commands: CommandRegistry::with_builtins(),
            session_id: uuid::Uuid::new_v4().to_string(),
            config,
        }
    }

    /// Initialize the pipeline, loading all components
    pub fn init(&mut self, project_root: Option<&Path>) -> Result<(), PipelineError> {
        // Discover agents
        self.agents.discover(project_root)?;

        // Discover commands
        self.commands.discover(project_root)?;

        // Load hooks from standard paths
        if let Some(root) = project_root {
            self.config.project_root = Some(root.to_path_buf());
            self.config.workspace = root.to_path_buf();

            // Build paths for hooks discovery
            let mut paths = Vec::new();

            // Project hooks
            paths.push(root.join(".claude"));

            // User hooks
            if let Some(home) = dirs::home_dir() {
                paths.push(home.join(".claude"));
            }

            self.config.hooks = self::load_hooks_from_paths(&paths);

            // Re-create hook executor with correct workspace
            self.hook_executor = HookExecutor::new(root.to_path_buf());
        }

        Ok(())
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the agent registry
    pub fn agents(&self) -> &AgentRegistry {
        &self.agents
    }

    /// Get the command registry
    pub fn commands(&self) -> &CommandRegistry {
        &self.commands
    }

    /// Get mutable access to the command registry
    pub fn commands_mut(&mut self) -> &mut CommandRegistry {
        &mut self.commands
    }

    /// Get the hooks configuration
    pub fn hooks(&self) -> &HooksConfig {
        &self.config.hooks
    }

    /// Update the template variables
    pub fn set_template_vars(&mut self, vars: TemplateVars) {
        self.config.template_vars = vars;
    }

    /// Run session start hooks
    pub fn run_session_start_hooks(&self) -> HookResults {
        let mut results = HookResults::new();

        if !self.config.enable_hooks {
            return results;
        }

        let ctx = HookContext::session_start(&self.session_id);
        let hook_results = self.hook_executor.execute(
            HookEvent::SessionStart,
            &self.config.hooks,
            &ctx,
        );

        results.merge(hook_results);
        results
    }

    /// Run user prompt hooks
    pub fn run_user_prompt_hooks(&self, user_message: &str) -> HookResults {
        let mut results = HookResults::new();

        if !self.config.enable_hooks {
            return results;
        }

        let ctx = HookContext::user_prompt(&self.session_id, user_message);
        let hook_results = self.hook_executor.execute(
            HookEvent::UserPromptSubmit,
            &self.config.hooks,
            &ctx,
        );

        results.merge(hook_results);
        results
    }

    /// Run pre-tool-use hooks
    pub fn run_pre_tool_hooks(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
    ) -> HookResults {
        let mut results = HookResults::new();

        if !self.config.enable_hooks {
            return results;
        }

        let ctx = HookContext::pre_tool_use(&self.session_id, tool_name, tool_args.clone());
        let hook_results = self.hook_executor.execute(
            HookEvent::PreToolUse,
            &self.config.hooks,
            &ctx,
        );

        results.merge(hook_results);
        results
    }

    /// Run post-tool-use hooks
    pub fn run_post_tool_hooks(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        tool_result: &str,
    ) -> HookResults {
        let mut results = HookResults::new();

        if !self.config.enable_hooks {
            return results;
        }

        let ctx = HookContext::post_tool_use(&self.session_id, tool_name, tool_args.clone(), tool_result);
        let hook_results = self.hook_executor.execute(
            HookEvent::PostToolUse,
            &self.config.hooks,
            &ctx,
        );

        results.merge(hook_results);
        results
    }

    /// Run stop hooks
    pub fn run_stop_hooks(&self) -> HookResults {
        let mut results = HookResults::new();

        if !self.config.enable_hooks {
            return results;
        }

        let ctx = HookContext::session_start(&self.session_id); // Reuse session context
        let hook_results = self.hook_executor.execute(
            HookEvent::Stop,
            &self.config.hooks,
            &ctx,
        );

        results.merge(hook_results);
        results
    }

    /// Check if input is a command and get the command name
    pub fn parse_command<'a>(&self, input: &'a str) -> Option<(&'a str, &'a str)> {
        CommandRegistry::parse_invocation(input)
    }

    /// Build a prompt for a command invocation
    pub fn build_command_prompt(
        &self,
        command_name: &str,
        args: &str,
        hook_context: Option<Vec<String>>,
    ) -> Result<AssembledPrompt, PipelineError> {
        let command = self.commands
            .get(command_name)
            .ok_or_else(|| CommandError::NotFound(command_name.to_string()))?
            .clone();

        let mut builder = PromptBuilder::new(&self.config.base_prompt)
            .with_environment(&self.config.template_vars)
            .with_command(command, args);

        // Add hook contexts
        if let Some(contexts) = hook_context {
            builder = builder.with_hook_contexts(contexts);
        }

        Ok(builder.build())
    }

    /// Build a prompt for an agent invocation
    pub fn build_agent_prompt(
        &self,
        agent_name: &str,
        hook_context: Option<Vec<String>>,
        additional_restrictions: Option<ToolRestrictions>,
    ) -> Result<AssembledPrompt, PipelineError> {
        let agent = self.agents
            .get(agent_name)
            .ok_or_else(|| PipelineError::AgentNotFound(agent_name.to_string()))?
            .clone();

        let mut builder = PromptBuilder::new(&self.config.base_prompt)
            .with_environment(&self.config.template_vars)
            .with_agent(agent);

        // Add hook contexts
        if let Some(contexts) = hook_context {
            builder = builder.with_hook_contexts(contexts);
        }

        // Add additional restrictions
        if let Some(restrictions) = additional_restrictions {
            builder = builder.with_restrictions(restrictions);
        }

        Ok(builder.build())
    }

    /// Build a prompt with skills
    pub fn build_skill_prompt(
        &self,
        skills: Vec<&DynamicSkill>,
        hook_context: Option<Vec<String>>,
    ) -> AssembledPrompt {
        let skill_defs: Vec<SkillDefinition> = skills
            .into_iter()
            .map(SkillDefinition::from)
            .collect();

        let mut builder = PromptBuilder::new(&self.config.base_prompt)
            .with_environment(&self.config.template_vars)
            .with_skills(skill_defs);

        // Add hook contexts
        if let Some(contexts) = hook_context {
            builder = builder.with_hook_contexts(contexts);
        }

        builder.build()
    }

    /// Build a default prompt (no agent, command, or skills)
    pub fn build_default_prompt(&self, hook_context: Option<Vec<String>>) -> AssembledPrompt {
        let mut builder = PromptBuilder::new(&self.config.base_prompt)
            .with_environment(&self.config.template_vars);

        // Add hook contexts
        if let Some(contexts) = hook_context {
            builder = builder.with_hook_contexts(contexts);
        }

        builder.build()
    }

    /// Process user input and build the appropriate prompt
    ///
    /// This is the main entry point for processing user input. It:
    /// 1. Runs user prompt hooks
    /// 2. Checks for command invocation
    /// 3. Builds the appropriate prompt
    pub fn process_input(&self, user_input: &str) -> Result<ProcessedInput, PipelineError> {
        // 1. Run user prompt hooks
        let hook_results = self.run_user_prompt_hooks(user_input);

        // Check if blocked
        if hook_results.blocked {
            return Err(PipelineError::Blocked(
                hook_results.block_reason.unwrap_or_else(|| "Blocked by hook".to_string())
            ));
        }

        // 2. Check for command invocation
        if let Some((cmd_name, args)) = self.parse_command(user_input) {
            let prompt = self.build_command_prompt(
                cmd_name,
                args,
                if hook_results.context.is_empty() { None } else { Some(hook_results.context) },
            )?;

            return Ok(ProcessedInput {
                prompt,
                input_type: InputType::Command {
                    name: cmd_name.to_string(),
                    args: args.to_string(),
                },
                original_input: user_input.to_string(),
            });
        }

        // 3. Build default prompt
        let prompt = self.build_default_prompt(
            if hook_results.context.is_empty() { None } else { Some(hook_results.context) },
        );

        Ok(ProcessedInput {
            prompt,
            input_type: InputType::Regular,
            original_input: user_input.to_string(),
        })
    }
}

impl Default for PromptPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to load hooks from standard paths
fn load_hooks_from_paths(paths: &[PathBuf]) -> HooksConfig {
    use crate::prompt::hook_executor::load_hooks_config;

    let mut config = HooksConfig::new();

    for path in paths {
        let hooks_file = path.join("hooks").join("hooks.json");
        if hooks_file.exists()
            && let Ok(loaded) = load_hooks_config(&hooks_file)
        {
            config.merge(loaded);
        }
    }

    config
}

/// Result of processing user input
#[derive(Debug)]
pub struct ProcessedInput {
    /// The assembled prompt
    pub prompt: AssembledPrompt,

    /// Type of input detected
    pub input_type: InputType,

    /// Original user input
    pub original_input: String,
}

/// Type of user input
#[derive(Debug, Clone)]
pub enum InputType {
    /// Regular chat message
    Regular,

    /// Command invocation (/command args)
    Command {
        name: String,
        args: String,
    },

    /// Skill-triggered input
    Skill {
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::types::ToolSpec;

    mod pipeline_config_tests {
        use super::*;

        #[test]
        fn test_default_config() {
            let config = PipelineConfig::default();
            assert!(!config.base_prompt.is_empty());
            assert!(config.enable_hooks);
            assert!(config.project_root.is_none());
        }
    }

    mod hook_results_tests {
        use super::*;
        use crate::prompt::hooks::HookEvent;

        #[test]
        fn test_new() {
            let results = HookResults::new();
            assert!(results.context.is_empty());
            assert!(!results.blocked);
            assert!(results.block_reason.is_none());
            assert!(results.errors.is_empty());
        }

        #[test]
        fn test_merge_context() {
            let mut results = HookResults::new();

            results.merge(vec![
                Ok(HookResult {
                    hook_event_name: HookEvent::SessionStart,
                    additional_context: Some("Context 1".to_string()),
                    block: false,
                    block_reason: None,
                    modified_args: None,
                }),
                Ok(HookResult {
                    hook_event_name: HookEvent::SessionStart,
                    additional_context: Some("Context 2".to_string()),
                    block: false,
                    block_reason: None,
                    modified_args: None,
                }),
            ]);

            assert_eq!(results.context.len(), 2);
            assert!(results.context.contains(&"Context 1".to_string()));
            assert!(results.context.contains(&"Context 2".to_string()));
        }

        #[test]
        fn test_merge_blocked() {
            let mut results = HookResults::new();

            results.merge(vec![
                Ok(HookResult {
                    hook_event_name: HookEvent::PreToolUse,
                    additional_context: None,
                    block: true,
                    block_reason: Some("Not allowed".to_string()),
                    modified_args: None,
                }),
            ]);

            assert!(results.blocked);
            assert_eq!(results.block_reason, Some("Not allowed".to_string()));
        }

        #[test]
        fn test_merge_errors() {
            let mut results = HookResults::new();

            results.merge(vec![
                Err(HookError::CommandFailed("test error".to_string())),
            ]);

            assert_eq!(results.errors.len(), 1);
        }
    }

    mod pipeline_tests {
        use super::*;

        #[test]
        fn test_new() {
            let pipeline = PromptPipeline::new();
            assert!(!pipeline.session_id().is_empty());
        }

        #[test]
        fn test_with_config() {
            let config = PipelineConfig {
                enable_hooks: false,
                ..Default::default()
            };

            let pipeline = PromptPipeline::with_config(config);
            assert!(!pipeline.config.enable_hooks);
        }

        #[test]
        fn test_agents_loaded() {
            let pipeline = PromptPipeline::new();
            let agents = pipeline.agents();

            // Built-in agents should be loaded
            assert!(agents.get("Explore").is_some());
            assert!(agents.get("Plan").is_some());
            assert!(agents.get("Bash").is_some());
        }

        #[test]
        fn test_commands_loaded() {
            let pipeline = PromptPipeline::new();
            let commands = pipeline.commands();

            // Built-in commands should be loaded
            assert!(commands.get("commit").is_some());
            assert!(commands.get("commit-push-pr").is_some());
            assert!(commands.get("review-pr").is_some());
        }

        #[test]
        fn test_parse_command() {
            let pipeline = PromptPipeline::new();

            let result = pipeline.parse_command("/commit -m 'test'");
            assert_eq!(result, Some(("commit", "-m 'test'")));

            let result = pipeline.parse_command("regular message");
            assert!(result.is_none());
        }

        #[test]
        fn test_build_default_prompt() {
            let pipeline = PromptPipeline::new();
            let prompt = pipeline.build_default_prompt(None);

            assert!(!prompt.system_prompt.is_empty());
            assert!(prompt.tool_restrictions.is_empty());
        }

        #[test]
        fn test_build_default_prompt_with_hook_context() {
            let pipeline = PromptPipeline::new();
            let prompt = pipeline.build_default_prompt(Some(vec![
                "Hook context 1".to_string(),
                "Hook context 2".to_string(),
            ]));

            assert!(prompt.system_prompt.contains("Hook context 1"));
            assert!(prompt.system_prompt.contains("Hook context 2"));
        }

        #[test]
        fn test_build_command_prompt() {
            let pipeline = PromptPipeline::new();
            let result = pipeline.build_command_prompt("commit", "-m 'test'", None);

            assert!(result.is_ok());
            let prompt = result.unwrap();
            assert!(prompt.metadata.has_command);
            assert_eq!(prompt.metadata.command_name, Some("commit".to_string()));
        }

        #[test]
        fn test_build_command_prompt_not_found() {
            let pipeline = PromptPipeline::new();
            let result = pipeline.build_command_prompt("nonexistent", "", None);

            assert!(result.is_err());
        }

        #[test]
        fn test_build_agent_prompt() {
            let pipeline = PromptPipeline::new();
            let result = pipeline.build_agent_prompt("Explore", None, None);

            assert!(result.is_ok());
            let prompt = result.unwrap();
            assert!(prompt.metadata.has_agent);
            assert_eq!(prompt.metadata.agent_name, Some("Explore".to_string()));
        }

        #[test]
        fn test_build_agent_prompt_not_found() {
            let pipeline = PromptPipeline::new();
            let result = pipeline.build_agent_prompt("NonexistentAgent", None, None);

            assert!(result.is_err());
        }

        #[test]
        fn test_build_agent_prompt_with_restrictions() {
            let pipeline = PromptPipeline::new();
            let restrictions = ToolRestrictions::deny(vec![
                ToolSpec::Name("Dangerous".to_string()),
            ]);

            let result = pipeline.build_agent_prompt("Explore", None, Some(restrictions));

            assert!(result.is_ok());
            let prompt = result.unwrap();
            assert!(!prompt.is_tool_allowed("Dangerous", &serde_json::json!({})));
        }

        #[test]
        fn test_run_session_start_hooks_disabled() {
            let config = PipelineConfig {
                enable_hooks: false,
                ..Default::default()
            };

            let pipeline = PromptPipeline::with_config(config);
            let results = pipeline.run_session_start_hooks();

            assert!(results.context.is_empty());
            assert!(!results.blocked);
        }

        #[test]
        fn test_run_user_prompt_hooks_disabled() {
            let config = PipelineConfig {
                enable_hooks: false,
                ..Default::default()
            };

            let pipeline = PromptPipeline::with_config(config);
            let results = pipeline.run_user_prompt_hooks("test message");

            assert!(results.context.is_empty());
            assert!(!results.blocked);
        }

        #[test]
        fn test_process_input_regular() {
            let config = PipelineConfig {
                enable_hooks: false,
                ..Default::default()
            };

            let pipeline = PromptPipeline::with_config(config);
            let result = pipeline.process_input("Hello, world!");

            assert!(result.is_ok());
            let processed = result.unwrap();
            assert!(matches!(processed.input_type, InputType::Regular));
            assert_eq!(processed.original_input, "Hello, world!");
        }

        #[test]
        fn test_process_input_command() {
            let config = PipelineConfig {
                enable_hooks: false,
                ..Default::default()
            };

            let pipeline = PromptPipeline::with_config(config);
            let result = pipeline.process_input("/commit -m 'test'");

            assert!(result.is_ok());
            let processed = result.unwrap();
            if let InputType::Command { name, args } = processed.input_type {
                assert_eq!(name, "commit");
                assert_eq!(args, "-m 'test'");
            } else {
                panic!("Expected Command input type");
            }
        }
    }

    mod input_type_tests {
        use super::*;

        #[test]
        fn test_input_type_variants() {
            let regular = InputType::Regular;
            assert!(matches!(regular, InputType::Regular));

            let command = InputType::Command {
                name: "commit".to_string(),
                args: "-m 'test'".to_string(),
            };
            if let InputType::Command { name, args } = command {
                assert_eq!(name, "commit");
                assert_eq!(args, "-m 'test'");
            } else {
                panic!("Expected Command variant");
            }

            let skill = InputType::Skill {
                name: "build".to_string(),
            };
            if let InputType::Skill { name } = skill {
                assert_eq!(name, "build");
            } else {
                panic!("Expected Skill variant");
            }
        }
    }
}
