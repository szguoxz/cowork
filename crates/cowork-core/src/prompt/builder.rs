//! Prompt Builder for Cowork
//!
//! This module implements layered prompt composition with tool restriction intersection.
//! The builder follows a layered assembly approach:
//!
//! 1. Base system prompt
//! 2. Hook-injected context
//! 3. Agent system prompt
//! 4. Skill instructions
//! 5. Command content
//! 6. Tool filtering by intersected restrictions
//!
//! # Example
//!
//! ```rust,ignore
//! use cowork_core::prompt::builder::{PromptBuilder, AssembledPrompt};
//! use cowork_core::prompt::TemplateVars;
//!
//! let vars = TemplateVars::default();
//! let prompt = PromptBuilder::new("You are a helpful assistant.")
//!     .with_hook_context("Current time: 2024-01-01")
//!     .with_environment(&vars)
//!     .build();
//! ```

use serde::{Deserialize, Serialize};

use crate::prompt::agents::AgentDefinition;
use crate::prompt::commands::CommandDefinition;
use crate::prompt::types::{ModelPreference, ToolRestrictions, ToolSpec};
use crate::prompt::TemplateVars;
use crate::skills::loader::DynamicSkill;

/// Assembled prompt ready for use with an LLM
///
/// This is the output of the prompt builder, containing:
/// - The fully composed system prompt
/// - Filtered list of allowed tools
/// - Model preference
/// - Maximum turns configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledPrompt {
    /// The fully assembled system prompt
    pub system_prompt: String,

    /// Tool restrictions after intersection
    pub tool_restrictions: ToolRestrictions,

    /// Model preference (from agent/skill or inherited)
    pub model: ModelPreference,

    /// Maximum number of turns (from agent)
    pub max_turns: Option<usize>,

    /// Additional metadata about the assembly
    #[serde(default)]
    pub metadata: AssemblyMetadata,
}

/// Metadata about how the prompt was assembled
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssemblyMetadata {
    /// Whether an agent was used
    pub has_agent: bool,

    /// Name of the agent if used
    pub agent_name: Option<String>,

    /// Whether a command was used
    pub has_command: bool,

    /// Name of the command if used
    pub command_name: Option<String>,

    /// Number of skills included
    pub skill_count: usize,

    /// Hook context sections included
    pub hook_context_count: usize,
}

impl AssembledPrompt {
    /// Check if a tool is allowed by the assembled restrictions
    pub fn is_tool_allowed(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        self.tool_restrictions.is_allowed(tool_name, args)
    }

    /// Get the list of allowed tool names (if restrictions specify allowed list)
    pub fn allowed_tool_names(&self) -> Vec<String> {
        self.tool_restrictions
            .allowed
            .iter()
            .filter_map(|spec| match spec {
                ToolSpec::Name(name) => Some(name.clone()),
                ToolSpec::Pattern { tool, .. } => Some(tool.clone()),
                ToolSpec::All => None,
            })
            .collect()
    }

    /// Get the list of denied tool names
    pub fn denied_tool_names(&self) -> Vec<String> {
        self.tool_restrictions
            .denied
            .iter()
            .filter_map(|spec| match spec {
                ToolSpec::Name(name) => Some(name.clone()),
                ToolSpec::Pattern { tool, .. } => Some(tool.clone()),
                ToolSpec::All => None,
            })
            .collect()
    }

    /// Get model ID if not inheriting
    pub fn model_id(&self) -> Option<&str> {
        self.model.model_id()
    }
}

/// Skill definition for the builder (simplified view)
///
/// This is a trait-object-safe representation of skill data needed for building prompts.
#[derive(Debug, Clone)]
pub struct SkillDefinition {
    /// Skill name
    pub name: String,

    /// Skill instructions/body
    pub instructions: String,

    /// Tool restrictions for this skill
    pub tool_restrictions: ToolRestrictions,

    /// Model preference
    pub model: Option<String>,
}

impl From<&DynamicSkill> for SkillDefinition {
    fn from(skill: &DynamicSkill) -> Self {
        Self {
            name: skill.frontmatter.name.clone(),
            instructions: skill.body.clone(),
            tool_restrictions: skill.frontmatter.tool_restrictions(),
            model: skill.frontmatter.model.clone(),
        }
    }
}

/// Builder for assembling prompts from multiple components
///
/// Uses a fluent builder pattern to compose prompts from:
/// - Base system prompt
/// - Hook-injected context
/// - Agent system prompts
/// - Skill instructions
/// - Command content
/// - Template variable substitution
#[derive(Debug, Clone, Default)]
pub struct PromptBuilder {
    /// Base system prompt
    base_prompt: String,

    /// Additional context from hooks
    hook_contexts: Vec<String>,

    /// Agent definition (if running as agent)
    agent: Option<AgentDefinition>,

    /// Skills to include
    skills: Vec<SkillDefinition>,

    /// Command being executed (if any)
    command: Option<CommandDefinition>,

    /// Command arguments (if command is set)
    command_args: String,

    /// Template variables for substitution
    template_vars: Option<TemplateVars>,

    /// Additional restrictions to apply
    additional_restrictions: Vec<ToolRestrictions>,

    /// User message to prepend (for command/skill instructions)
    user_message: Option<String>,
}

impl PromptBuilder {
    /// Create a new builder with a base system prompt
    pub fn new(base_prompt: impl Into<String>) -> Self {
        Self {
            base_prompt: base_prompt.into(),
            hook_contexts: Vec::new(),
            agent: None,
            skills: Vec::new(),
            command: None,
            command_args: String::new(),
            template_vars: None,
            additional_restrictions: Vec::new(),
            user_message: None,
        }
    }

    /// Create an empty builder
    pub fn empty() -> Self {
        Self::default()
    }

    /// Set the base system prompt
    pub fn with_base_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.base_prompt = prompt.into();
        self
    }

    /// Add hook-injected context
    ///
    /// Context from hooks is added after the base prompt and before agent/skill prompts.
    pub fn with_hook_context(mut self, context: impl Into<String>) -> Self {
        self.hook_contexts.push(context.into());
        self
    }

    /// Add multiple hook contexts
    pub fn with_hook_contexts(mut self, contexts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.hook_contexts.extend(contexts.into_iter().map(|c| c.into()));
        self
    }

    /// Set the agent for this prompt
    ///
    /// The agent's system prompt will be appended, and its tool restrictions applied.
    pub fn with_agent(mut self, agent: AgentDefinition) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Add a skill to the prompt
    ///
    /// Skills are appended in order, and their tool restrictions are intersected.
    pub fn with_skill(mut self, skill: SkillDefinition) -> Self {
        self.skills.push(skill);
        self
    }

    /// Add multiple skills
    pub fn with_skills(mut self, skills: impl IntoIterator<Item = SkillDefinition>) -> Self {
        self.skills.extend(skills);
        self
    }

    /// Set the command being executed
    ///
    /// The command content (with argument substitution) is appended to the prompt.
    pub fn with_command(mut self, command: CommandDefinition, args: impl Into<String>) -> Self {
        self.command = Some(command);
        self.command_args = args.into();
        self
    }

    /// Set template variables for substitution
    ///
    /// Variables are substituted in the final assembled prompt.
    pub fn with_environment(mut self, vars: &TemplateVars) -> Self {
        self.template_vars = Some(vars.clone());
        self
    }

    /// Add additional tool restrictions
    ///
    /// These are intersected with restrictions from agents, skills, and commands.
    pub fn with_restrictions(mut self, restrictions: ToolRestrictions) -> Self {
        self.additional_restrictions.push(restrictions);
        self
    }

    /// Set a user message to prepend to the final output
    pub fn with_user_message(mut self, message: impl Into<String>) -> Self {
        self.user_message = Some(message.into());
        self
    }

    /// Build the assembled prompt
    ///
    /// Assembly order:
    /// 1. Base system prompt
    /// 2. Hook-injected context
    /// 3. Agent system prompt
    /// 4. Skill instructions
    /// 5. Command content (with argument substitution)
    /// 6. Template variable substitution
    pub fn build(self) -> AssembledPrompt {
        let mut sections = Vec::new();
        let mut metadata = AssemblyMetadata::default();

        // 1. Base system prompt
        if !self.base_prompt.is_empty() {
            sections.push(self.base_prompt.clone());
        }

        // 2. Hook-injected context
        metadata.hook_context_count = self.hook_contexts.len();
        for ctx in &self.hook_contexts {
            if !ctx.is_empty() {
                sections.push(ctx.clone());
            }
        }

        // 3. Agent system prompt
        let mut model = ModelPreference::Inherit;
        let mut max_turns = None;

        if let Some(ref agent) = self.agent {
            metadata.has_agent = true;
            metadata.agent_name = Some(agent.name().to_string());

            if !agent.system_prompt.is_empty() {
                sections.push(agent.system_prompt.clone());
            }

            model = agent.model().clone();
            max_turns = agent.max_turns();
        }

        // 4. Skill instructions
        metadata.skill_count = self.skills.len();
        for skill in &self.skills {
            if !skill.instructions.is_empty() {
                sections.push(skill.instructions.clone());
            }

            // Update model from skill if not already set
            if model == ModelPreference::Inherit
                && let Some(ref skill_model) = skill.model
            {
                model = ModelPreference::parse(skill_model);
            }
        }

        // 5. Command content
        if let Some(ref command) = self.command {
            metadata.has_command = true;
            metadata.command_name = Some(command.name().to_string());

            let content = command.substitute_arguments(&self.command_args);
            if !content.is_empty() {
                sections.push(content);
            }
        }

        // Join sections
        let mut system_prompt = sections.join("\n\n");

        // 6. Template variable substitution
        if let Some(ref vars) = self.template_vars {
            system_prompt = vars.substitute(&system_prompt);
        }

        // Compute tool restrictions
        let tool_restrictions = self.compute_restrictions();

        AssembledPrompt {
            system_prompt,
            tool_restrictions,
            model,
            max_turns,
            metadata,
        }
    }

    /// Compute the intersection of all tool restrictions
    fn compute_restrictions(&self) -> ToolRestrictions {
        let mut restrictions = ToolRestrictions::new();

        // Apply agent restrictions
        if let Some(ref agent) = self.agent {
            let agent_restrictions = agent.tool_restrictions();
            restrictions = restrictions.intersect(&agent_restrictions);
        }

        // Apply skill restrictions
        for skill in &self.skills {
            restrictions = restrictions.intersect(&skill.tool_restrictions);
        }

        // Apply command restrictions
        if let Some(ref command) = self.command {
            let cmd_restrictions = command.tool_restrictions();
            restrictions = restrictions.intersect(&cmd_restrictions);
        }

        // Apply additional restrictions
        for extra in &self.additional_restrictions {
            restrictions = restrictions.intersect(extra);
        }

        restrictions
    }

    /// Get the user message if set
    pub fn user_message(&self) -> Option<&str> {
        self.user_message.as_deref()
    }

    /// Check if an agent is set
    pub fn has_agent(&self) -> bool {
        self.agent.is_some()
    }

    /// Check if a command is set
    pub fn has_command(&self) -> bool {
        self.command.is_some()
    }

    /// Get the number of skills
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::agents::{AgentColor, AgentMetadata, ContextMode};
    use crate::prompt::commands::CommandMetadata;
    use crate::prompt::types::Scope;
    use serde_json::json;

    mod assembled_prompt_tests {
        use super::*;

        #[test]
        fn test_is_tool_allowed_empty_restrictions() {
            let prompt = AssembledPrompt {
                system_prompt: "Test".to_string(),
                tool_restrictions: ToolRestrictions::new(),
                model: ModelPreference::Inherit,
                max_turns: None,
                metadata: AssemblyMetadata::default(),
            };

            assert!(prompt.is_tool_allowed("Bash", &json!({})));
            assert!(prompt.is_tool_allowed("Read", &json!({})));
            assert!(prompt.is_tool_allowed("Write", &json!({})));
        }

        #[test]
        fn test_is_tool_allowed_with_restrictions() {
            let prompt = AssembledPrompt {
                system_prompt: "Test".to_string(),
                tool_restrictions: ToolRestrictions::allow_only(vec![
                    ToolSpec::Name("Read".to_string()),
                    ToolSpec::Name("Glob".to_string()),
                ]),
                model: ModelPreference::Inherit,
                max_turns: None,
                metadata: AssemblyMetadata::default(),
            };

            assert!(prompt.is_tool_allowed("Read", &json!({})));
            assert!(prompt.is_tool_allowed("Glob", &json!({})));
            assert!(!prompt.is_tool_allowed("Write", &json!({})));
            assert!(!prompt.is_tool_allowed("Bash", &json!({})));
        }

        #[test]
        fn test_allowed_tool_names() {
            let prompt = AssembledPrompt {
                system_prompt: "Test".to_string(),
                tool_restrictions: ToolRestrictions::allow_only(vec![
                    ToolSpec::Name("Read".to_string()),
                    ToolSpec::Name("Glob".to_string()),
                    ToolSpec::parse("Bash(git:*)"),
                ]),
                model: ModelPreference::Inherit,
                max_turns: None,
                metadata: AssemblyMetadata::default(),
            };

            let names = prompt.allowed_tool_names();
            assert!(names.contains(&"Read".to_string()));
            assert!(names.contains(&"Glob".to_string()));
            assert!(names.contains(&"Bash".to_string()));
        }

        #[test]
        fn test_denied_tool_names() {
            let prompt = AssembledPrompt {
                system_prompt: "Test".to_string(),
                tool_restrictions: ToolRestrictions::deny(vec![
                    ToolSpec::Name("Write".to_string()),
                    ToolSpec::Name("Edit".to_string()),
                ]),
                model: ModelPreference::Inherit,
                max_turns: None,
                metadata: AssemblyMetadata::default(),
            };

            let names = prompt.denied_tool_names();
            assert!(names.contains(&"Write".to_string()));
            assert!(names.contains(&"Edit".to_string()));
        }

        #[test]
        fn test_model_id() {
            let prompt = AssembledPrompt {
                system_prompt: "Test".to_string(),
                tool_restrictions: ToolRestrictions::new(),
                model: ModelPreference::Haiku,
                max_turns: None,
                metadata: AssemblyMetadata::default(),
            };

            assert_eq!(prompt.model_id(), crate::provider::catalog::model_id("anthropic", crate::provider::catalog::ModelTier::Fast));
        }
    }

    mod prompt_builder_tests {
        use super::*;

        #[test]
        fn test_new() {
            let builder = PromptBuilder::new("Base prompt");
            assert!(!builder.has_agent());
            assert!(!builder.has_command());
            assert_eq!(builder.skill_count(), 0);
        }

        #[test]
        fn test_empty() {
            let builder = PromptBuilder::empty();
            let prompt = builder.build();
            assert!(prompt.system_prompt.is_empty());
        }

        #[test]
        fn test_build_base_only() {
            let prompt = PromptBuilder::new("You are a helpful assistant.")
                .build();

            assert_eq!(prompt.system_prompt, "You are a helpful assistant.");
            assert!(prompt.tool_restrictions.is_empty());
            assert_eq!(prompt.model, ModelPreference::Inherit);
        }

        #[test]
        fn test_build_with_hook_contexts() {
            let prompt = PromptBuilder::new("Base prompt")
                .with_hook_context("Hook 1")
                .with_hook_context("Hook 2")
                .build();

            assert!(prompt.system_prompt.contains("Base prompt"));
            assert!(prompt.system_prompt.contains("Hook 1"));
            assert!(prompt.system_prompt.contains("Hook 2"));
            assert_eq!(prompt.metadata.hook_context_count, 2);
        }

        #[test]
        fn test_build_with_agent() {
            let agent = AgentDefinition {
                metadata: AgentMetadata {
                    name: "TestAgent".to_string(),
                    description: "Test agent".to_string(),
                    model: ModelPreference::Haiku,
                    color: AgentColor::default(),
                    tools: vec!["Read".to_string(), "Glob".to_string()],
                    context: ContextMode::Fork,
                    max_turns: Some(30),
                },
                system_prompt: "You are a test agent.".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            let prompt = PromptBuilder::new("Base prompt")
                .with_agent(agent)
                .build();

            assert!(prompt.system_prompt.contains("Base prompt"));
            assert!(prompt.system_prompt.contains("You are a test agent."));
            assert_eq!(prompt.model, ModelPreference::Haiku);
            assert_eq!(prompt.max_turns, Some(30));
            assert!(prompt.metadata.has_agent);
            assert_eq!(prompt.metadata.agent_name, Some("TestAgent".to_string()));

            // Tool restrictions from agent
            assert!(prompt.is_tool_allowed("Read", &json!({})));
            assert!(prompt.is_tool_allowed("Glob", &json!({})));
            assert!(!prompt.is_tool_allowed("Write", &json!({})));
        }

        #[test]
        fn test_build_with_skill() {
            let skill = SkillDefinition {
                name: "test-skill".to_string(),
                instructions: "Follow these skill instructions.".to_string(),
                tool_restrictions: ToolRestrictions::allow_only(vec![
                    ToolSpec::Name("Bash".to_string()),
                ]),
                model: None,
            };

            let prompt = PromptBuilder::new("Base prompt")
                .with_skill(skill)
                .build();

            assert!(prompt.system_prompt.contains("Base prompt"));
            assert!(prompt.system_prompt.contains("skill instructions"));
            assert_eq!(prompt.metadata.skill_count, 1);
            assert!(prompt.is_tool_allowed("Bash", &json!({})));
            assert!(!prompt.is_tool_allowed("Write", &json!({})));
        }

        #[test]
        fn test_build_with_command() {
            let command = CommandDefinition {
                metadata: CommandMetadata {
                    name: "test-cmd".to_string(),
                    description: "Test command".to_string(),
                    allowed_tools: vec!["Bash".to_string(), "Read".to_string()],
                    denied_tools: vec![],
                    argument_hint: vec![],
                },
                content: "Execute with args: $ARGUMENTS".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            let prompt = PromptBuilder::new("Base prompt")
                .with_command(command, "arg1 arg2")
                .build();

            assert!(prompt.system_prompt.contains("Base prompt"));
            assert!(prompt.system_prompt.contains("Execute with args: arg1 arg2"));
            assert!(prompt.metadata.has_command);
            assert_eq!(prompt.metadata.command_name, Some("test-cmd".to_string()));
        }

        #[test]
        fn test_build_with_template_vars() {
            let vars = TemplateVars {
                working_directory: "/home/user/project".to_string(),
                is_git_repo: true,
                platform: "linux".to_string(),
                ..Default::default()
            };

            let prompt = PromptBuilder::new("Working in: ${WORKING_DIRECTORY}")
                .with_environment(&vars)
                .build();

            assert!(prompt.system_prompt.contains("/home/user/project"));
        }

        #[test]
        fn test_build_full_assembly() {
            let agent = AgentDefinition {
                metadata: AgentMetadata {
                    name: "Agent".to_string(),
                    description: "Agent".to_string(),
                    model: ModelPreference::Haiku,
                    color: AgentColor::default(),
                    tools: vec!["Read".to_string(), "Glob".to_string(), "Bash".to_string()],
                    context: ContextMode::Fork,
                    max_turns: Some(20),
                },
                system_prompt: "Agent instructions.".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            let skill = SkillDefinition {
                name: "skill".to_string(),
                instructions: "Skill instructions.".to_string(),
                tool_restrictions: ToolRestrictions::allow_only(vec![
                    ToolSpec::Name("Read".to_string()),
                    ToolSpec::Name("Bash".to_string()),
                ]),
                model: None,
            };

            let command = CommandDefinition {
                metadata: CommandMetadata {
                    name: "cmd".to_string(),
                    description: "".to_string(),
                    allowed_tools: vec!["Bash".to_string()],
                    denied_tools: vec![],
                    argument_hint: vec![],
                },
                content: "Command: $ARGUMENTS".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            let prompt = PromptBuilder::new("Base system prompt.")
                .with_hook_context("Hook context.")
                .with_agent(agent)
                .with_skill(skill)
                .with_command(command, "test-arg")
                .build();

            // Check assembly order in the output
            let sys = &prompt.system_prompt;
            let base_pos = sys.find("Base system prompt").unwrap();
            let hook_pos = sys.find("Hook context").unwrap();
            let agent_pos = sys.find("Agent instructions").unwrap();
            let skill_pos = sys.find("Skill instructions").unwrap();
            let cmd_pos = sys.find("Command: test-arg").unwrap();

            assert!(base_pos < hook_pos);
            assert!(hook_pos < agent_pos);
            assert!(agent_pos < skill_pos);
            assert!(skill_pos < cmd_pos);

            // Check metadata
            assert!(prompt.metadata.has_agent);
            assert!(prompt.metadata.has_command);
            assert_eq!(prompt.metadata.skill_count, 1);
            assert_eq!(prompt.metadata.hook_context_count, 1);

            // Check model and max_turns from agent
            assert_eq!(prompt.model, ModelPreference::Haiku);
            assert_eq!(prompt.max_turns, Some(20));

            // Check tool restrictions intersection
            // Agent: Read, Glob, Bash
            // Skill: Read, Bash
            // Command: Bash
            // Result: Bash (intersection)
            assert!(prompt.is_tool_allowed("Bash", &json!({})));
            assert!(!prompt.is_tool_allowed("Read", &json!({})));
            assert!(!prompt.is_tool_allowed("Glob", &json!({})));
        }
    }

    mod restriction_intersection_tests {
        use super::*;

        #[test]
        fn test_intersect_agent_and_skill() {
            let agent = AgentDefinition {
                metadata: AgentMetadata {
                    name: "Agent".to_string(),
                    description: "".to_string(),
                    model: ModelPreference::Inherit,
                    color: AgentColor::default(),
                    tools: vec!["A".to_string(), "B".to_string(), "C".to_string()],
                    context: ContextMode::Fork,
                    max_turns: None,
                },
                system_prompt: "".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            let skill = SkillDefinition {
                name: "skill".to_string(),
                instructions: "".to_string(),
                tool_restrictions: ToolRestrictions::allow_only(vec![
                    ToolSpec::Name("B".to_string()),
                    ToolSpec::Name("C".to_string()),
                    ToolSpec::Name("D".to_string()),
                ]),
                model: None,
            };

            let prompt = PromptBuilder::new("")
                .with_agent(agent)
                .with_skill(skill)
                .build();

            // Agent allows: A, B, C
            // Skill allows: B, C, D
            // Intersection should allow: B, C
            assert!(!prompt.is_tool_allowed("A", &json!({})));
            assert!(prompt.is_tool_allowed("B", &json!({})));
            assert!(prompt.is_tool_allowed("C", &json!({})));
            assert!(!prompt.is_tool_allowed("D", &json!({})));
        }

        #[test]
        fn test_intersect_with_denied() {
            let agent = AgentDefinition {
                metadata: AgentMetadata {
                    name: "Agent".to_string(),
                    description: "".to_string(),
                    model: ModelPreference::Inherit,
                    color: AgentColor::default(),
                    tools: vec![], // Allow all
                    context: ContextMode::Fork,
                    max_turns: None,
                },
                system_prompt: "".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            let prompt = PromptBuilder::new("")
                .with_agent(agent)
                .with_restrictions(ToolRestrictions::deny(vec![
                    ToolSpec::Name("Dangerous".to_string()),
                ]))
                .build();

            assert!(prompt.is_tool_allowed("Safe", &json!({})));
            assert!(!prompt.is_tool_allowed("Dangerous", &json!({})));
        }

        #[test]
        fn test_multiple_skills_intersection() {
            let skill1 = SkillDefinition {
                name: "skill1".to_string(),
                instructions: "".to_string(),
                tool_restrictions: ToolRestrictions::allow_only(vec![
                    ToolSpec::Name("A".to_string()),
                    ToolSpec::Name("B".to_string()),
                ]),
                model: None,
            };

            let skill2 = SkillDefinition {
                name: "skill2".to_string(),
                instructions: "".to_string(),
                tool_restrictions: ToolRestrictions::allow_only(vec![
                    ToolSpec::Name("B".to_string()),
                    ToolSpec::Name("C".to_string()),
                ]),
                model: None,
            };

            let prompt = PromptBuilder::new("")
                .with_skill(skill1)
                .with_skill(skill2)
                .build();

            // Skill1: A, B
            // Skill2: B, C
            // Intersection: B
            assert!(!prompt.is_tool_allowed("A", &json!({})));
            assert!(prompt.is_tool_allowed("B", &json!({})));
            assert!(!prompt.is_tool_allowed("C", &json!({})));
        }

        #[test]
        fn test_no_restrictions_allows_all() {
            let prompt = PromptBuilder::new("Base prompt").build();

            assert!(prompt.is_tool_allowed("Bash", &json!({})));
            assert!(prompt.is_tool_allowed("Read", &json!({})));
            assert!(prompt.is_tool_allowed("Write", &json!({})));
            assert!(prompt.is_tool_allowed("AnyTool", &json!({})));
        }
    }

    mod skill_definition_tests {
        use super::*;

        #[test]
        fn test_skill_definition_from_dynamic_skill() {
            use crate::skills::loader::{DynamicSkill, SkillSource};
            use std::path::PathBuf;

            let content = r#"---
name: test-skill
description: Test skill
allowed-tools: Read, Glob
denied-tools: Write
---

Test instructions.
"#;

            let skill = DynamicSkill::parse(content, PathBuf::from("/test"), SkillSource::User)
                .unwrap();
            let def = SkillDefinition::from(&skill);

            assert_eq!(def.name, "test-skill");
            assert!(def.instructions.contains("Test instructions"));
            assert!(def.tool_restrictions.is_allowed("Read", &json!({})));
            assert!(def.tool_restrictions.is_allowed("Glob", &json!({})));
            assert!(!def.tool_restrictions.is_allowed("Write", &json!({})));
        }
    }

    mod serialization_tests {
        use super::*;

        #[test]
        fn test_assembled_prompt_serde() {
            let prompt = AssembledPrompt {
                system_prompt: "Test prompt".to_string(),
                tool_restrictions: ToolRestrictions::allow_only(vec![
                    ToolSpec::Name("Read".to_string()),
                ]),
                model: ModelPreference::Haiku,
                max_turns: Some(30),
                metadata: AssemblyMetadata {
                    has_agent: true,
                    agent_name: Some("TestAgent".to_string()),
                    has_command: false,
                    command_name: None,
                    skill_count: 1,
                    hook_context_count: 2,
                },
            };

            let json = serde_json::to_string(&prompt).unwrap();
            let deserialized: AssembledPrompt = serde_json::from_str(&json).unwrap();

            assert_eq!(prompt.system_prompt, deserialized.system_prompt);
            assert_eq!(prompt.model, deserialized.model);
            assert_eq!(prompt.max_turns, deserialized.max_turns);
            assert_eq!(prompt.metadata.has_agent, deserialized.metadata.has_agent);
        }
    }
}
