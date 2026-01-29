//! Agent System for Cowork
//!
//! This module implements specialized subagents with custom prompts and tool restrictions.
//! Agents are defined via markdown files with YAML frontmatter and can be loaded from:
//! - Built-in agents (hardcoded in the binary)
//! - Project-level agents (`.claude/agents/`)
//! - User-level agents (`~/.claude/agents/`)
//!
//! # Agent Definition Format
//!
//! ```markdown
//! ---
//! name: Explore
//! description: "Fast agent for exploring codebases"
//! model: haiku
//! color: cyan
//! tools: Glob, Grep, Read, LSP, WebFetch
//! context: fork
//! max_turns: 30
//! ---
//!
//! # Explore Agent
//!
//! You are a file search specialist...
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::prompt::parser::{parse_frontmatter, parse_tool_list, ParseError, ParsedDocument};
use crate::prompt::types::{ModelPreference, Scope, ToolRestrictions, ToolSpec};

/// Context mode for agents - how they receive conversation context
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextMode {
    /// Fork current context - agent sees full conversation history
    #[default]
    Fork,
    /// Inherit context - agent continues in same context (not recommended)
    Inherit,
}

impl ContextMode {
    /// Parse context mode from a string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "inherit" => ContextMode::Inherit,
            _ => ContextMode::Fork, // Default to fork
        }
    }
}

impl std::fmt::Display for ContextMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextMode::Fork => write!(f, "fork"),
            ContextMode::Inherit => write!(f, "inherit"),
        }
    }
}

/// Color for agent UI display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentColor {
    /// Cyan - typically used for Explore agent
    Cyan,
    /// Blue - typically used for Plan agent
    Blue,
    /// Green - typically used for Bash agent
    Green,
    /// Purple - typically used for general-purpose agent
    #[default]
    Purple,
    /// Yellow
    Yellow,
    /// Red
    Red,
    /// Orange
    Orange,
    /// Pink
    Pink,
    /// Gray/default
    Gray,
}

impl AgentColor {
    /// Parse color from a string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cyan" => AgentColor::Cyan,
            "blue" => AgentColor::Blue,
            "green" => AgentColor::Green,
            "purple" => AgentColor::Purple,
            "yellow" => AgentColor::Yellow,
            "red" => AgentColor::Red,
            "orange" => AgentColor::Orange,
            "pink" => AgentColor::Pink,
            "gray" | "grey" | "default" => AgentColor::Gray,
            _ => AgentColor::Purple, // Default
        }
    }

    /// Get ANSI color code for terminal display
    pub fn ansi_code(&self) -> &'static str {
        match self {
            AgentColor::Cyan => "\x1b[36m",
            AgentColor::Blue => "\x1b[34m",
            AgentColor::Green => "\x1b[32m",
            AgentColor::Purple => "\x1b[35m",
            AgentColor::Yellow => "\x1b[33m",
            AgentColor::Red => "\x1b[31m",
            AgentColor::Orange => "\x1b[38;5;208m",
            AgentColor::Pink => "\x1b[38;5;213m",
            AgentColor::Gray => "\x1b[90m",
        }
    }

    /// Get hex color code for UI display
    pub fn hex_code(&self) -> &'static str {
        match self {
            AgentColor::Cyan => "#00d4aa",
            AgentColor::Blue => "#5b9bd5",
            AgentColor::Green => "#70c056",
            AgentColor::Purple => "#b07cd0",
            AgentColor::Yellow => "#d9a644",
            AgentColor::Red => "#e05858",
            AgentColor::Orange => "#e08050",
            AgentColor::Pink => "#e070b0",
            AgentColor::Gray => "#808080",
        }
    }
}

impl std::fmt::Display for AgentColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentColor::Cyan => write!(f, "cyan"),
            AgentColor::Blue => write!(f, "blue"),
            AgentColor::Green => write!(f, "green"),
            AgentColor::Purple => write!(f, "purple"),
            AgentColor::Yellow => write!(f, "yellow"),
            AgentColor::Red => write!(f, "red"),
            AgentColor::Orange => write!(f, "orange"),
            AgentColor::Pink => write!(f, "pink"),
            AgentColor::Gray => write!(f, "gray"),
        }
    }
}

/// Agent metadata from YAML frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    /// Unique agent name (used for `subagent_type` parameter)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Model preference for this agent
    #[serde(default)]
    pub model: ModelPreference,
    /// UI display color
    #[serde(default)]
    pub color: AgentColor,
    /// List of allowed tools (empty = all tools)
    #[serde(default)]
    pub tools: Vec<String>,
    /// Context mode (fork or inherit)
    #[serde(default)]
    pub context: ContextMode,
    /// Maximum number of turns before stopping
    #[serde(default)]
    pub max_turns: Option<usize>,
}

impl AgentMetadata {
    /// Create tool restrictions based on the agent's tools list
    pub fn tool_restrictions(&self) -> ToolRestrictions {
        if self.tools.is_empty() {
            // Empty means all tools allowed
            return ToolRestrictions::new();
        }

        // Check for wildcard
        if self.tools.len() == 1 && self.tools[0] == "*" {
            return ToolRestrictions::new();
        }

        // Parse tools as ToolSpec
        let specs: Vec<ToolSpec> = self.tools.iter().map(|t| ToolSpec::parse(t)).collect();
        ToolRestrictions::allow_only(specs)
    }
}

/// Complete agent definition including metadata, prompt, and source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Agent metadata from frontmatter
    pub metadata: AgentMetadata,
    /// System prompt content (markdown after frontmatter)
    pub system_prompt: String,
    /// Source path (if loaded from filesystem)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<PathBuf>,
    /// Scope of this definition
    #[serde(default)]
    pub scope: Scope,
}

impl AgentDefinition {
    /// Get the agent name
    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    /// Get the agent description
    pub fn description(&self) -> &str {
        &self.metadata.description
    }

    /// Get the model preference
    pub fn model(&self) -> &ModelPreference {
        &self.metadata.model
    }

    /// Get the context mode
    pub fn context_mode(&self) -> ContextMode {
        self.metadata.context
    }

    /// Get the maximum turns
    pub fn max_turns(&self) -> Option<usize> {
        self.metadata.max_turns
    }

    /// Get the UI color
    pub fn color(&self) -> AgentColor {
        self.metadata.color
    }

    /// Get tool restrictions for this agent
    pub fn tool_restrictions(&self) -> ToolRestrictions {
        self.metadata.tool_restrictions()
    }

    /// Check if a tool is allowed by this agent
    pub fn is_tool_allowed(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        self.tool_restrictions().is_allowed(tool_name, args)
    }
}

/// Error type for agent parsing and loading
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Failed to parse agent file: {0}")]
    ParseError(#[from] ParseError),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Failed to read agent file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Agent not found: {0}")]
    NotFound(String),
}

/// Parse an agent definition from markdown content
///
/// # Arguments
/// * `content` - The full markdown file content
/// * `source_path` - Optional path where the file was loaded from
/// * `scope` - The scope of this agent definition
///
/// # Returns
/// * `Ok(AgentDefinition)` - Successfully parsed agent
/// * `Err(AgentError)` - Parse or validation error
pub fn parse_agent(
    content: &str,
    source_path: Option<PathBuf>,
    scope: Scope,
) -> Result<AgentDefinition, AgentError> {
    let doc = parse_frontmatter(content)?;
    parse_agent_from_document(doc, source_path, scope)
}

/// Parse an agent definition from a parsed document
fn parse_agent_from_document(
    doc: ParsedDocument,
    source_path: Option<PathBuf>,
    scope: Scope,
) -> Result<AgentDefinition, AgentError> {
    // Extract required fields
    let name = doc
        .get_string("name")
        .ok_or_else(|| AgentError::MissingField("name".to_string()))?
        .to_string();

    let description = doc
        .get_string("description")
        .unwrap_or("No description")
        .to_string();

    // Extract optional fields
    let model = doc
        .get_string("model")
        .map(ModelPreference::parse)
        .unwrap_or_default();

    let color = doc
        .get_string("color")
        .map(AgentColor::parse)
        .unwrap_or_default();

    let tools = doc
        .metadata
        .get("tools")
        .map(parse_tool_list)
        .unwrap_or_default();

    let context = doc
        .get_string("context")
        .map(ContextMode::parse)
        .unwrap_or_default();

    let max_turns = doc.get_i64("max_turns").map(|v| v as usize);

    let metadata = AgentMetadata {
        name,
        description,
        model,
        color,
        tools,
        context,
        max_turns,
    };

    Ok(AgentDefinition {
        metadata,
        system_prompt: doc.content,
        source_path,
        scope,
    })
}

/// Load an agent from a file path
pub fn load_agent_from_file(path: &Path, scope: Scope) -> Result<AgentDefinition, AgentError> {
    let content = std::fs::read_to_string(path)?;
    parse_agent(&content, Some(path.to_path_buf()), scope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    mod context_mode_tests {
        use super::*;

        #[test]
        fn test_parse_fork() {
            assert_eq!(ContextMode::parse("fork"), ContextMode::Fork);
            assert_eq!(ContextMode::parse("Fork"), ContextMode::Fork);
            assert_eq!(ContextMode::parse("FORK"), ContextMode::Fork);
        }

        #[test]
        fn test_parse_inherit() {
            assert_eq!(ContextMode::parse("inherit"), ContextMode::Inherit);
            assert_eq!(ContextMode::parse("Inherit"), ContextMode::Inherit);
        }

        #[test]
        fn test_parse_default() {
            // Unknown values default to Fork
            assert_eq!(ContextMode::parse("unknown"), ContextMode::Fork);
            assert_eq!(ContextMode::parse(""), ContextMode::Fork);
        }

        #[test]
        fn test_display() {
            assert_eq!(ContextMode::Fork.to_string(), "fork");
            assert_eq!(ContextMode::Inherit.to_string(), "inherit");
        }

        #[test]
        fn test_default_trait() {
            assert_eq!(ContextMode::default(), ContextMode::Fork);
        }
    }

    mod agent_color_tests {
        use super::*;

        #[test]
        fn test_parse_colors() {
            assert_eq!(AgentColor::parse("cyan"), AgentColor::Cyan);
            assert_eq!(AgentColor::parse("Cyan"), AgentColor::Cyan);
            assert_eq!(AgentColor::parse("blue"), AgentColor::Blue);
            assert_eq!(AgentColor::parse("green"), AgentColor::Green);
            assert_eq!(AgentColor::parse("purple"), AgentColor::Purple);
            assert_eq!(AgentColor::parse("yellow"), AgentColor::Yellow);
            assert_eq!(AgentColor::parse("red"), AgentColor::Red);
            assert_eq!(AgentColor::parse("orange"), AgentColor::Orange);
            assert_eq!(AgentColor::parse("pink"), AgentColor::Pink);
            assert_eq!(AgentColor::parse("gray"), AgentColor::Gray);
            assert_eq!(AgentColor::parse("grey"), AgentColor::Gray);
        }

        #[test]
        fn test_parse_default() {
            assert_eq!(AgentColor::parse("unknown"), AgentColor::Purple);
        }

        #[test]
        fn test_ansi_codes() {
            assert!(AgentColor::Cyan.ansi_code().contains("36"));
            assert!(AgentColor::Blue.ansi_code().contains("34"));
            assert!(AgentColor::Green.ansi_code().contains("32"));
        }

        #[test]
        fn test_hex_codes() {
            assert!(AgentColor::Cyan.hex_code().starts_with('#'));
            assert_eq!(AgentColor::Cyan.hex_code().len(), 7);
        }

        #[test]
        fn test_display() {
            assert_eq!(AgentColor::Cyan.to_string(), "cyan");
            assert_eq!(AgentColor::Purple.to_string(), "purple");
        }

        #[test]
        fn test_default_trait() {
            assert_eq!(AgentColor::default(), AgentColor::Purple);
        }
    }

    mod agent_metadata_tests {
        use super::*;

        #[test]
        fn test_tool_restrictions_empty() {
            let meta = AgentMetadata {
                name: "Test".to_string(),
                description: "Test agent".to_string(),
                model: ModelPreference::default(),
                color: AgentColor::default(),
                tools: vec![],
                context: ContextMode::default(),
                max_turns: None,
            };

            let restrictions = meta.tool_restrictions();
            assert!(restrictions.is_empty());
            assert!(restrictions.is_allowed("AnyTool", &json!({})));
        }

        #[test]
        fn test_tool_restrictions_wildcard() {
            let meta = AgentMetadata {
                name: "Test".to_string(),
                description: "Test agent".to_string(),
                model: ModelPreference::default(),
                color: AgentColor::default(),
                tools: vec!["*".to_string()],
                context: ContextMode::default(),
                max_turns: None,
            };

            let restrictions = meta.tool_restrictions();
            assert!(restrictions.is_empty());
        }

        #[test]
        fn test_tool_restrictions_specific() {
            let meta = AgentMetadata {
                name: "Test".to_string(),
                description: "Test agent".to_string(),
                model: ModelPreference::default(),
                color: AgentColor::default(),
                tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
                context: ContextMode::default(),
                max_turns: None,
            };

            let restrictions = meta.tool_restrictions();
            assert!(restrictions.is_allowed("Read", &json!({})));
            assert!(restrictions.is_allowed("Glob", &json!({})));
            assert!(restrictions.is_allowed("Grep", &json!({})));
            assert!(!restrictions.is_allowed("Write", &json!({})));
            assert!(!restrictions.is_allowed("Bash", &json!({})));
        }
    }

    mod parse_agent_tests {
        use super::*;
        use crate::prompt::builtin;

        #[test]
        fn test_parse_minimal_agent() {
            let content = r#"---
name: Minimal
---

Just a system prompt.
"#;

            let agent = parse_agent(content, None, Scope::Builtin).unwrap();
            assert_eq!(agent.name(), "Minimal");
            assert_eq!(agent.description(), "No description");
            assert_eq!(*agent.model(), ModelPreference::Inherit);
            assert_eq!(agent.color(), AgentColor::Purple);
            assert_eq!(agent.context_mode(), ContextMode::Fork);
            assert!(agent.max_turns().is_none());
        }

        #[test]
        fn test_parse_full_agent() {
            let content = r#"---
name: FullAgent
description: "A fully configured agent"
model: haiku
color: cyan
tools: Read, Glob, Grep
context: fork
max_turns: 50
---

# Full Agent

This is the system prompt content.
"#;

            let agent = parse_agent(content, None, Scope::Project).unwrap();
            assert_eq!(agent.name(), "FullAgent");
            assert_eq!(agent.description(), "A fully configured agent");
            assert_eq!(*agent.model(), ModelPreference::Haiku);
            assert_eq!(agent.color(), AgentColor::Cyan);
            assert_eq!(agent.context_mode(), ContextMode::Fork);
            assert_eq!(agent.max_turns(), Some(50));
            assert!(agent.system_prompt.contains("# Full Agent"));
            assert!(agent.system_prompt.contains("system prompt content"));
        }

        #[test]
        fn test_parse_missing_name() {
            let content = r#"---
description: No name field
---

Content
"#;

            let result = parse_agent(content, None, Scope::Builtin);
            assert!(matches!(result, Err(AgentError::MissingField(_))));
        }

        #[test]
        fn test_parse_explore_agent() {
            let agent = parse_agent(builtin::agents::EXPLORE, None, Scope::Builtin).unwrap();
            assert_eq!(agent.name(), "Explore");
            assert_eq!(*agent.model(), ModelPreference::Haiku);
            assert_eq!(agent.color(), AgentColor::Cyan);
            assert_eq!(agent.max_turns(), Some(30));

            // Check tool restrictions
            let restrictions = agent.tool_restrictions();
            assert!(restrictions.is_allowed("Glob", &json!({})));
            assert!(restrictions.is_allowed("Grep", &json!({})));
            assert!(restrictions.is_allowed("Read", &json!({})));
            assert!(!restrictions.is_allowed("Write", &json!({})));
            assert!(!restrictions.is_allowed("Edit", &json!({})));
        }

        #[test]
        fn test_parse_bash_agent() {
            let agent = parse_agent(builtin::agents::BASH, None, Scope::Builtin).unwrap();
            assert_eq!(agent.name(), "Bash");
            assert_eq!(agent.color(), AgentColor::Green);

            let restrictions = agent.tool_restrictions();
            assert!(restrictions.is_allowed("Bash", &json!({})));
            assert!(!restrictions.is_allowed("KillShell", &json!({})));
            assert!(!restrictions.is_allowed("Read", &json!({})));
        }

        #[test]
        fn test_parse_general_agent() {
            let agent = parse_agent(builtin::agents::GENERAL, None, Scope::Builtin).unwrap();
            assert_eq!(agent.name(), "general-purpose");
            assert_eq!(agent.color(), AgentColor::Purple);

            // General agent has "*" for tools, so all are allowed
            let restrictions = agent.tool_restrictions();
            assert!(restrictions.is_allowed("Bash", &json!({})));
            assert!(restrictions.is_allowed("Write", &json!({})));
            assert!(restrictions.is_allowed("Edit", &json!({})));
        }
    }

    mod agent_definition_tests {
        use super::*;

        #[test]
        fn test_is_tool_allowed() {
            let content = r#"---
name: ReadOnly
tools: Read, Glob
---

Read-only agent.
"#;
            let agent = parse_agent(content, None, Scope::Builtin).unwrap();

            assert!(agent.is_tool_allowed("Read", &json!({})));
            assert!(agent.is_tool_allowed("Glob", &json!({})));
            assert!(!agent.is_tool_allowed("Write", &json!({})));
        }
    }

    mod serialization_tests {
        use super::*;

        #[test]
        fn test_context_mode_serde() {
            let mode = ContextMode::Fork;
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, "\"fork\"");
            let deserialized: ContextMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }

        #[test]
        fn test_agent_color_serde() {
            let color = AgentColor::Cyan;
            let json = serde_json::to_string(&color).unwrap();
            assert_eq!(json, "\"cyan\"");
            let deserialized: AgentColor = serde_json::from_str(&json).unwrap();
            assert_eq!(color, deserialized);
        }

        #[test]
        fn test_agent_metadata_serde() {
            let meta = AgentMetadata {
                name: "Test".to_string(),
                description: "Test agent".to_string(),
                model: ModelPreference::Haiku,
                color: AgentColor::Cyan,
                tools: vec!["Read".to_string(), "Glob".to_string()],
                context: ContextMode::Fork,
                max_turns: Some(30),
            };

            let json = serde_json::to_string(&meta).unwrap();
            let deserialized: AgentMetadata = serde_json::from_str(&json).unwrap();

            assert_eq!(meta.name, deserialized.name);
            assert_eq!(meta.tools.len(), deserialized.tools.len());
        }
    }
}
