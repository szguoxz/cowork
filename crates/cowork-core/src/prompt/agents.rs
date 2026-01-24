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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::prompt::parser::{parse_frontmatter, parse_tool_list, ParseError, ParsedDocument};
use crate::prompt::types::{ModelPreference, Scope, ToolRestrictions, ToolSpec};
use crate::prompt::builtin;

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

/// Registry for managing agent definitions
#[derive(Debug, Default)]
pub struct AgentRegistry {
    /// Registered agents by name
    agents: HashMap<String, AgentDefinition>,
}

impl AgentRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new registry with built-in agents loaded
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.load_builtins();
        registry
    }

    /// Load all built-in agents
    pub fn load_builtins(&mut self) {
        // Load Explore agent
        if let Ok(agent) = parse_agent(builtin::agents::EXPLORE, None, Scope::Builtin) {
            self.register(agent);
        }

        // Load Plan agent
        if let Ok(agent) = parse_agent(builtin::agents::PLAN, None, Scope::Builtin) {
            self.register(agent);
        }

        // Load Bash agent
        if let Ok(agent) = parse_agent(builtin::agents::BASH, None, Scope::Builtin) {
            self.register(agent);
        }

        // Load General agent
        if let Ok(agent) = parse_agent(builtin::agents::GENERAL, None, Scope::Builtin) {
            self.register(agent);
        }
    }

    /// Register an agent definition
    ///
    /// If an agent with the same name exists, it will be replaced only if
    /// the new agent has higher priority (lower scope value).
    pub fn register(&mut self, agent: AgentDefinition) {
        let name = agent.name().to_string();

        // Check if we should replace existing
        if let Some(existing) = self.agents.get(&name) {
            // Only replace if new agent has higher priority
            if !agent.scope.overrides(&existing.scope) {
                return;
            }
        }

        self.agents.insert(name, agent);
    }

    /// Get an agent by name
    pub fn get(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name)
    }

    /// List all registered agents
    pub fn list(&self) -> impl Iterator<Item = &AgentDefinition> {
        self.agents.values()
    }

    /// List agent names
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.agents.keys().map(|s| s.as_str())
    }

    /// Get the number of registered agents
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Load agents from a directory
    ///
    /// Scans the directory for `.md` files and attempts to parse each as an agent.
    /// Invalid files are logged and skipped.
    pub fn load_from_directory(&mut self, dir: &Path, scope: Scope) -> std::io::Result<usize> {
        if !dir.exists() {
            return Ok(0);
        }

        let mut loaded = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only process .md files
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            match load_agent_from_file(&path, scope) {
                Ok(agent) => {
                    self.register(agent);
                    loaded += 1;
                }
                Err(e) => {
                    // Log but don't fail on individual agent errors
                    tracing::warn!("Failed to load agent from {}: {}", path.display(), e);
                }
            }
        }

        Ok(loaded)
    }

    /// Discover and load agents from standard locations
    ///
    /// Loads from:
    /// 1. Built-in agents (if not already loaded)
    /// 2. User agents from `~/.claude/agents/`
    /// 3. Project agents from `.claude/agents/`
    ///
    /// Higher priority sources override lower priority ones.
    pub fn discover(&mut self, project_root: Option<&Path>) -> std::io::Result<()> {
        // Load built-ins first (lowest priority)
        if self.is_empty() {
            self.load_builtins();
        }

        // Load user agents
        if let Some(home) = dirs::home_dir() {
            let user_agents_dir = home.join(".claude").join("agents");
            let _ = self.load_from_directory(&user_agents_dir, Scope::User);
        }

        // Load project agents (highest priority among filesystem)
        if let Some(root) = project_root {
            let project_agents_dir = root.join(".claude").join("agents");
            let _ = self.load_from_directory(&project_agents_dir, Scope::Project);
        }

        Ok(())
    }
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

    mod agent_registry_tests {
        use super::*;

        #[test]
        fn test_new_registry() {
            let registry = AgentRegistry::new();
            assert!(registry.is_empty());
            assert_eq!(registry.len(), 0);
        }

        #[test]
        fn test_with_builtins() {
            let registry = AgentRegistry::with_builtins();
            assert!(!registry.is_empty());
            assert!(registry.len() >= 4);

            // Check built-in agents are loaded
            assert!(registry.get("Explore").is_some());
            assert!(registry.get("Plan").is_some());
            assert!(registry.get("Bash").is_some());
            assert!(registry.get("general-purpose").is_some());
        }

        #[test]
        fn test_register_and_get() {
            let mut registry = AgentRegistry::new();

            let agent = parse_agent(
                "---\nname: TestAgent\n---\n\nPrompt",
                None,
                Scope::Project,
            )
            .unwrap();

            registry.register(agent);

            let retrieved = registry.get("TestAgent").unwrap();
            assert_eq!(retrieved.name(), "TestAgent");
        }

        #[test]
        fn test_scope_priority() {
            let mut registry = AgentRegistry::new();

            // Register a builtin agent
            let builtin_agent = parse_agent(
                "---\nname: TestAgent\ndescription: Builtin version\n---\n\nBuiltin",
                None,
                Scope::Builtin,
            )
            .unwrap();
            registry.register(builtin_agent);

            // Try to register a user agent with same name (should override)
            let user_agent = parse_agent(
                "---\nname: TestAgent\ndescription: User version\n---\n\nUser",
                None,
                Scope::User,
            )
            .unwrap();
            registry.register(user_agent);

            let agent = registry.get("TestAgent").unwrap();
            assert_eq!(agent.description(), "User version");

            // Try to register another builtin (should NOT override user)
            let another_builtin = parse_agent(
                "---\nname: TestAgent\ndescription: Another builtin\n---\n\nBuiltin2",
                None,
                Scope::Builtin,
            )
            .unwrap();
            registry.register(another_builtin);

            let agent = registry.get("TestAgent").unwrap();
            assert_eq!(agent.description(), "User version");
        }

        #[test]
        fn test_list_agents() {
            let registry = AgentRegistry::with_builtins();
            let agents: Vec<_> = registry.list().collect();
            assert!(agents.len() >= 4);
        }

        #[test]
        fn test_names() {
            let registry = AgentRegistry::with_builtins();
            let names: Vec<_> = registry.names().collect();
            assert!(names.contains(&"Explore"));
            assert!(names.contains(&"Plan"));
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
