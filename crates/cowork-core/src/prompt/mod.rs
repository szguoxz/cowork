//! Prompt System for Cowork
//!
//! This module implements a Claude Code-style prompt system with:
//! - Built-in prompts (system prompt, agents, tools, reminders)
//! - Prompt composition and assembly
//! - Template variable substitution
//! - Tool restrictions and scope hierarchy
//!
//! # Architecture
//!
//! The prompt system follows a layered composition approach:
//!
//! 1. **Base System Prompt** - Core instructions for the assistant
//! 2. **Security Policy** - Security guidelines and restrictions
//! 3. **Agent Prompts** - Specialized subagent instructions
//! 4. **Tool Descriptions** - How to use each tool
//! 5. **System Reminders** - Context-specific reminders (plan mode, etc.)
//!
//! # Core Types
//!
//! - [`ToolSpec`] - Specification for matching tools by name or pattern
//! - [`ToolRestrictions`] - Control which tools can be used
//! - [`Scope`] - Priority hierarchy for component overrides
//! - [`ModelPreference`] - Model selection for agents and skills
//!
//! # Parsing
//!
//! - [`parser::parse_frontmatter`] - Parse YAML frontmatter from markdown files
//! - [`substitution::substitute_commands`] - Execute shell command substitutions
//!
//! # Usage
//!
//! ```rust,ignore
//! use cowork_core::prompt::{builtin, types::*, parser::*};
//!
//! // Load the main system prompt
//! let system_prompt = builtin::SYSTEM_PROMPT;
//!
//! // Parse an agent file with frontmatter
//! let doc = parse_frontmatter(builtin::agents::EXPLORE).unwrap();
//! let name = doc.get_string("name"); // Some("Explore")
//!
//! // Create tool restrictions
//! let restrictions = ToolRestrictions::allow_only(vec![
//!     ToolSpec::Name("Read".to_string()),
//!     ToolSpec::Name("Glob".to_string()),
//! ]);
//! ```

pub mod agents;
pub mod builder;
pub mod builtin;
pub mod commands;
pub mod hook_executor;
pub mod hooks;
pub mod parser;
pub mod pipeline;
pub mod plugins;
pub mod registry;
pub mod substitution;
pub mod types;

// Re-export commonly used types
pub use parser::{parse_frontmatter, parse_tool_list, ParsedDocument, ParseError};
pub use substitution::{substitute_commands, extract_commands, has_substitutions};
pub use types::{ModelPreference, Scope, ToolRestrictions, ToolSpec};

// Re-export hook types
pub use hooks::{
    HookDefinition, HookEvent, HookHandler, HookMatcher, HookRegistration,
    HookResult, HooksConfig,
};
pub use hook_executor::{HookContext, HookError, HookExecutor, load_hooks_config, load_hooks_from_paths};

// Re-export agent types
pub use agents::{
    AgentColor, AgentDefinition, AgentError, AgentMetadata, AgentRegistry,
    ContextMode, parse_agent, load_agent_from_file,
};

// Re-export command types
pub use commands::{
    CommandDefinition, CommandError, CommandMetadata, CommandRegistry,
    parse_command, load_command_from_file,
};

// Re-export builder types
pub use builder::{AssembledPrompt, AssemblyMetadata, PromptBuilder, SkillDefinition};

// Re-export pipeline types
pub use pipeline::{
    HookResults, InputType, PipelineConfig, PipelineError, ProcessedInput, PromptPipeline,
};

// Re-export registry types
pub use registry::{
    AgentInfo, CommandInfo, ComponentPaths, ComponentRegistry, LoadResult, PluginInfo,
    RegistryCounts, RegistryError, RegistrySummary, SkillInfo,
};

// Re-export plugin types
pub use plugins::{
    DiscoverResult, Plugin, PluginError, PluginManifest, PluginRegistry,
};

/// Template variables that can be substituted in prompts
///
/// These variables are substituted at runtime in prompt templates using
/// `${VARIABLE_NAME}` syntax.
#[derive(Debug, Clone)]
pub struct TemplateVars {
    /// Current working directory
    pub working_directory: String,
    /// Whether the directory is a git repo
    pub is_git_repo: bool,
    /// Platform name (linux, macos, windows)
    pub platform: String,
    /// OS version
    pub os_version: String,
    /// Current date (YYYY-MM-DD format)
    pub current_date: String,
    /// Current year (YYYY format)
    pub current_year: String,
    /// Model information (name and ID)
    pub model_info: String,
    /// Git status output
    pub git_status: String,
    /// Assistant name (e.g., "Cowork", "Claude")
    pub assistant_name: String,
    /// Security policy content
    pub security_policy: String,
    /// Current git branch name
    pub current_branch: String,
    /// Main/master branch name
    pub main_branch: String,
    /// Recent git commits (for commit style reference)
    pub recent_commits: String,
}

impl Default for TemplateVars {
    fn default() -> Self {
        Self {
            working_directory: String::new(),
            is_git_repo: false,
            platform: std::env::consts::OS.to_string(),
            os_version: get_os_version(),
            current_date: chrono::Local::now().format("%Y-%m-%d").to_string(),
            current_year: chrono::Local::now().format("%Y").to_string(),
            model_info: String::new(),
            git_status: String::new(),
            assistant_name: "Cowork".to_string(),
            security_policy: builtin::reminders::SECURITY_POLICY.to_string(),
            current_branch: String::new(),
            main_branch: "main".to_string(),
            recent_commits: String::new(),
        }
    }
}

/// Get the OS version string (cross-platform)
fn get_os_version() -> String {
    #[cfg(target_os = "linux")]
    {
        // Try to get Linux kernel version
        std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| format!("Linux {}", s.trim()))
            .unwrap_or_else(|| "Linux".to_string())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| format!("macOS {}", s.trim()))
            .unwrap_or_else(|| "macOS".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        // Get Windows version from ver command
        std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Windows".to_string())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        std::env::consts::OS.to_string()
    }
}

impl TemplateVars {
    /// Substitute template variables in a string
    ///
    /// Replaces all `${VARIABLE_NAME}` patterns with their corresponding values.
    pub fn substitute(&self, template: &str) -> String {
        template
            .replace("${WORKING_DIRECTORY}", &self.working_directory)
            .replace("${IS_GIT_REPO}", if self.is_git_repo { "Yes" } else { "No" })
            .replace("${PLATFORM}", &self.platform)
            .replace("${OS_VERSION}", &self.os_version)
            .replace("${CURRENT_DATE}", &self.current_date)
            .replace("${CURRENT_YEAR}", &self.current_year)
            .replace("${MODEL_INFO}", &self.model_info)
            .replace("${GIT_STATUS}", &self.git_status)
            .replace("${ASSISTANT_NAME}", &self.assistant_name)
            .replace("${SECURITY_POLICY}", &self.security_policy)
            .replace("${CURRENT_BRANCH}", &self.current_branch)
            .replace("${MAIN_BRANCH}", &self.main_branch)
            .replace("${RECENT_COMMITS}", &self.recent_commits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_substitution() {
        let vars = TemplateVars {
            working_directory: "/home/user/project".to_string(),
            is_git_repo: true,
            platform: "linux".to_string(),
            ..Default::default()
        };

        let template = "Working dir: ${WORKING_DIRECTORY}, Git: ${IS_GIT_REPO}";
        let result = vars.substitute(template);
        assert!(result.contains("/home/user/project"));
        assert!(result.contains("Yes"));
    }
}
