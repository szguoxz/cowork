//! Command System for Cowork
//!
//! This module implements user-triggered workflow orchestration via slash commands.
//! Commands are defined via markdown files with YAML frontmatter and can be loaded from:
//! - Built-in commands (hardcoded in the binary)
//! - Project-level commands (`.claude/commands/`)
//! - User-level commands (`~/.claude/commands/`)
//!
//! # Command Definition Format
//!
//! ```markdown
//! ---
//! name: commit
//! description: "Create a git commit with AI-generated message"
//! allowed_tools: Bash, Read
//! denied_tools: Write, Edit
//! argument_hint:
//!   - "<message>"
//!   - "--amend"
//! ---
//!
//! # Commit Command
//!
//! Create a commit following the project's conventions...
//! ```
//!
//! # Shell Substitutions
//!
//! Commands support shell substitutions using the `!` backtick syntax `` syntax:
//! - `` `git status` `` - replaced with command output
//! - `$ARGUMENTS` or `${ARGUMENTS}` - replaced with user-provided arguments
//!
//! # Usage
//!
//! Users invoke commands via `/command` or `/command args`:
//! ```text
//! /commit
//! /pr --draft
//! /review-pr 123
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::prompt::parser::{parse_frontmatter, parse_tool_list, ParseError, ParsedDocument};
use crate::prompt::types::{Scope, ToolRestrictions, ToolSpec};

/// Command metadata from YAML frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMetadata {
    /// Command name (used for invocation, e.g., "commit" for /commit)
    pub name: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// List of allowed tools (empty = all tools)
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// List of denied tools
    #[serde(default)]
    pub denied_tools: Vec<String>,
    /// Argument hints for CLI autocomplete and help
    #[serde(default)]
    pub argument_hint: Vec<String>,
}

impl CommandMetadata {
    /// Create tool restrictions based on the command's tool lists
    pub fn tool_restrictions(&self) -> ToolRestrictions {
        let allowed: Vec<ToolSpec> = self.allowed_tools.iter().map(|t| ToolSpec::parse(t)).collect();
        let denied: Vec<ToolSpec> = self.denied_tools.iter().map(|t| ToolSpec::parse(t)).collect();

        ToolRestrictions { allowed, denied }
    }
}

/// Complete command definition including metadata, content, and source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDefinition {
    /// Command metadata from frontmatter
    pub metadata: CommandMetadata,
    /// Command content (markdown with shell substitution placeholders)
    pub content: String,
    /// Source path (if loaded from filesystem)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<PathBuf>,
    /// Scope of this definition
    #[serde(default)]
    pub scope: Scope,
}

impl CommandDefinition {
    /// Get the command name
    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    /// Get the command description
    pub fn description(&self) -> &str {
        &self.metadata.description
    }

    /// Get the argument hints
    pub fn argument_hints(&self) -> &[String] {
        &self.metadata.argument_hint
    }

    /// Get tool restrictions for this command
    pub fn tool_restrictions(&self) -> ToolRestrictions {
        self.metadata.tool_restrictions()
    }

    /// Check if a tool is allowed by this command
    pub fn is_tool_allowed(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        self.tool_restrictions().is_allowed(tool_name, args)
    }

    /// Apply argument substitution to the command content
    pub fn substitute_arguments(&self, arguments: &str) -> String {
        self.content
            .replace("$ARGUMENTS", arguments)
            .replace("${ARGUMENTS}", arguments)
    }

    /// Get the command invocation string (e.g., "/commit")
    pub fn invocation(&self) -> String {
        format!("/{}", self.metadata.name)
    }

    /// Get help text for this command
    pub fn help_text(&self) -> String {
        let mut help = format!("/{}", self.metadata.name);

        if !self.metadata.argument_hint.is_empty() {
            help.push(' ');
            help.push_str(&self.metadata.argument_hint.join(" "));
        }

        if !self.metadata.description.is_empty() {
            help.push_str(" - ");
            help.push_str(&self.metadata.description);
        }

        help
    }
}

/// Error type for command parsing and loading
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Failed to parse command file: {0}")]
    ParseError(#[from] ParseError),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Failed to read command file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Command not found: {0}")]
    NotFound(String),

    #[error("Invalid command name '{0}': must be lowercase with hyphens, max 64 chars")]
    InvalidName(String),
}

/// Parse a command definition from markdown content
///
/// # Arguments
/// * `content` - The full markdown file content
/// * `source_path` - Optional path where the file was loaded from
/// * `scope` - The scope of this command definition
///
/// # Returns
/// * `Ok(CommandDefinition)` - Successfully parsed command
/// * `Err(CommandError)` - Parse or validation error
pub fn parse_command(
    content: &str,
    source_path: Option<PathBuf>,
    scope: Scope,
) -> Result<CommandDefinition, CommandError> {
    let doc = parse_frontmatter(content)?;
    parse_command_from_document(doc, source_path, scope)
}

/// Parse a command from content with an explicit name (for commands without name in frontmatter)
pub fn parse_command_named(
    content: &str,
    name: &str,
    source_path: Option<PathBuf>,
    scope: Scope,
) -> Result<CommandDefinition, CommandError> {
    let doc = parse_frontmatter(content)?;
    parse_command_from_document_with_name(doc, Some(name), source_path, scope)
}

/// Parse a command definition from a parsed document
fn parse_command_from_document(
    doc: ParsedDocument,
    source_path: Option<PathBuf>,
    scope: Scope,
) -> Result<CommandDefinition, CommandError> {
    parse_command_from_document_with_name(doc, None, source_path, scope)
}

/// Parse a command definition, optionally using a provided name if not in frontmatter
fn parse_command_from_document_with_name(
    doc: ParsedDocument,
    fallback_name: Option<&str>,
    source_path: Option<PathBuf>,
    scope: Scope,
) -> Result<CommandDefinition, CommandError> {
    // Extract name: from frontmatter, or use fallback
    let name = doc
        .get_string("name")
        .map(|s| s.to_string())
        .or_else(|| fallback_name.map(|s| s.to_string()))
        .ok_or_else(|| CommandError::MissingField("name".to_string()))?;

    // Validate name
    if !is_valid_command_name(&name) {
        return Err(CommandError::InvalidName(name));
    }

    let description = doc
        .get_string("description")
        .unwrap_or("")
        .to_string();

    // Extract optional fields (support both snake_case and kebab-case)
    let allowed_tools = doc
        .metadata
        .get("allowed_tools")
        .or_else(|| doc.metadata.get("allowed-tools"))
        .map(parse_tool_list)
        .unwrap_or_default();

    let denied_tools = doc
        .metadata
        .get("denied_tools")
        .or_else(|| doc.metadata.get("denied-tools"))
        .map(parse_tool_list)
        .unwrap_or_default();

    let argument_hint = doc
        .metadata
        .get("argument_hint")
        .or_else(|| doc.metadata.get("argument-hint"))
        .map(parse_string_list)
        .unwrap_or_default();

    let metadata = CommandMetadata {
        name,
        description,
        allowed_tools,
        denied_tools,
        argument_hint,
    };

    Ok(CommandDefinition {
        metadata,
        content: doc.content,
        source_path,
        scope,
    })
}

/// Parse a YAML value as a list of strings
fn parse_string_list(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        serde_json::Value::String(s) => s.split(',').map(|s| s.trim().to_string()).collect(),
        _ => vec![],
    }
}

/// Check if command name is valid (lowercase, hyphens/underscores, digits, max 64 chars)
fn is_valid_command_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Load a command from a file path
pub fn load_command_from_file(path: &Path, scope: Scope) -> Result<CommandDefinition, CommandError> {
    let content = std::fs::read_to_string(path)?;
    parse_command(&content, Some(path.to_path_buf()), scope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    mod command_metadata_tests {
        use super::*;

        #[test]
        fn test_empty_tool_restrictions() {
            let meta = CommandMetadata {
                name: "test".to_string(),
                description: "Test command".to_string(),
                allowed_tools: vec![],
                denied_tools: vec![],
                argument_hint: vec![],
            };

            let restrictions = meta.tool_restrictions();
            assert!(restrictions.is_empty());
            assert!(restrictions.is_allowed("AnyTool", &json!({})));
        }

        #[test]
        fn test_allowed_tools_restrictions() {
            let meta = CommandMetadata {
                name: "test".to_string(),
                description: "Test command".to_string(),
                allowed_tools: vec!["Bash".to_string(), "Read".to_string()],
                denied_tools: vec![],
                argument_hint: vec![],
            };

            let restrictions = meta.tool_restrictions();
            assert!(restrictions.is_allowed("Bash", &json!({})));
            assert!(restrictions.is_allowed("Read", &json!({})));
            assert!(!restrictions.is_allowed("Write", &json!({})));
        }

        #[test]
        fn test_denied_tools_restrictions() {
            let meta = CommandMetadata {
                name: "test".to_string(),
                description: "Test command".to_string(),
                allowed_tools: vec![],
                denied_tools: vec!["Write".to_string(), "Edit".to_string()],
                argument_hint: vec![],
            };

            let restrictions = meta.tool_restrictions();
            assert!(restrictions.is_allowed("Bash", &json!({})));
            assert!(!restrictions.is_allowed("Write", &json!({})));
            assert!(!restrictions.is_allowed("Edit", &json!({})));
        }
    }

    mod command_definition_tests {
        use super::*;

        #[test]
        fn test_substitute_arguments() {
            let cmd = CommandDefinition {
                metadata: CommandMetadata {
                    name: "test".to_string(),
                    description: "Test".to_string(),
                    allowed_tools: vec![],
                    denied_tools: vec![],
                    argument_hint: vec![],
                },
                content: "Run with: $ARGUMENTS and also ${ARGUMENTS}".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            let result = cmd.substitute_arguments("hello world");
            assert_eq!(result, "Run with: hello world and also hello world");
        }

        #[test]
        fn test_invocation() {
            let cmd = CommandDefinition {
                metadata: CommandMetadata {
                    name: "commit".to_string(),
                    description: "".to_string(),
                    allowed_tools: vec![],
                    denied_tools: vec![],
                    argument_hint: vec![],
                },
                content: "".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            assert_eq!(cmd.invocation(), "/commit");
        }

        #[test]
        fn test_help_text() {
            let cmd = CommandDefinition {
                metadata: CommandMetadata {
                    name: "pr".to_string(),
                    description: "Create a pull request".to_string(),
                    allowed_tools: vec![],
                    denied_tools: vec![],
                    argument_hint: vec!["<title>".to_string(), "--draft".to_string()],
                },
                content: "".to_string(),
                source_path: None,
                scope: Scope::Builtin,
            };

            let help = cmd.help_text();
            assert!(help.contains("/pr"));
            assert!(help.contains("<title>"));
            assert!(help.contains("--draft"));
            assert!(help.contains("Create a pull request"));
        }
    }

    mod parse_command_tests {
        use super::*;

        #[test]
        fn test_parse_minimal_command() {
            let content = r#"---
name: minimal
---

Just some content.
"#;

            let cmd = parse_command(content, None, Scope::Builtin).unwrap();
            assert_eq!(cmd.name(), "minimal");
            assert_eq!(cmd.description(), "");
            assert!(cmd.metadata.allowed_tools.is_empty());
            assert!(cmd.content.contains("Just some content"));
        }

        #[test]
        fn test_parse_full_command() {
            let content = r#"---
name: full-command
description: "A fully configured command"
allowed_tools: Bash, Read, Glob
denied_tools: Write
argument_hint:
  - "<file>"
  - "--force"
---

# Full Command

Execute with arguments: $ARGUMENTS
"#;

            let cmd = parse_command(content, None, Scope::Project).unwrap();
            assert_eq!(cmd.name(), "full-command");
            assert_eq!(cmd.description(), "A fully configured command");
            assert_eq!(cmd.metadata.allowed_tools.len(), 3);
            assert_eq!(cmd.metadata.denied_tools.len(), 1);
            assert_eq!(cmd.metadata.argument_hint.len(), 2);
            assert!(cmd.content.contains("# Full Command"));
        }

        #[test]
        fn test_parse_missing_name() {
            let content = r#"---
description: No name field
---

Content
"#;

            let result = parse_command(content, None, Scope::Builtin);
            assert!(matches!(result, Err(CommandError::MissingField(_))));
        }

        #[test]
        fn test_parse_invalid_name() {
            let content = r#"---
name: Invalid Name
---

Content
"#;

            let result = parse_command(content, None, Scope::Builtin);
            assert!(matches!(result, Err(CommandError::InvalidName(_))));
        }

        #[test]
        fn test_parse_command_with_source_path() {
            let content = r#"---
name: test
---

Content
"#;

            let path = PathBuf::from("/some/path/test.md");
            let cmd = parse_command(content, Some(path.clone()), Scope::User).unwrap();
            assert_eq!(cmd.source_path, Some(path));
            assert_eq!(cmd.scope, Scope::User);
        }
    }

    mod valid_name_tests {
        use super::*;

        #[test]
        fn test_valid_names() {
            assert!(is_valid_command_name("commit"));
            assert!(is_valid_command_name("pr"));
            assert!(is_valid_command_name("review-pr"));
            assert!(is_valid_command_name("build-and-test"));
            assert!(is_valid_command_name("test123"));
            assert!(is_valid_command_name("a"));
        }

        #[test]
        fn test_invalid_names() {
            assert!(!is_valid_command_name(""));
            assert!(!is_valid_command_name("Invalid"));
            assert!(!is_valid_command_name("has spaces"));
            assert!(!is_valid_command_name("has.dot"));
            assert!(!is_valid_command_name(&"a".repeat(65)));
        }

        #[test]
        fn test_valid_names_with_underscore() {
            assert!(is_valid_command_name("clean_gone"));
            assert!(is_valid_command_name("my_command"));
        }
    }

    mod serialization_tests {
        use super::*;

        #[test]
        fn test_command_metadata_serde() {
            let meta = CommandMetadata {
                name: "test".to_string(),
                description: "Test command".to_string(),
                allowed_tools: vec!["Bash".to_string()],
                denied_tools: vec!["Write".to_string()],
                argument_hint: vec!["<arg>".to_string()],
            };

            let json = serde_json::to_string(&meta).unwrap();
            let deserialized: CommandMetadata = serde_json::from_str(&json).unwrap();

            assert_eq!(meta.name, deserialized.name);
            assert_eq!(meta.allowed_tools.len(), deserialized.allowed_tools.len());
        }

        #[test]
        fn test_command_definition_serde() {
            let cmd = CommandDefinition {
                metadata: CommandMetadata {
                    name: "test".to_string(),
                    description: "Test".to_string(),
                    allowed_tools: vec![],
                    denied_tools: vec![],
                    argument_hint: vec![],
                },
                content: "Content".to_string(),
                source_path: Some(PathBuf::from("/path")),
                scope: Scope::User,
            };

            let json = serde_json::to_string(&cmd).unwrap();
            let deserialized: CommandDefinition = serde_json::from_str(&json).unwrap();

            assert_eq!(cmd.name(), deserialized.name());
            assert_eq!(cmd.scope, deserialized.scope);
        }
    }

    mod builtin_command_tests {
        use super::*;
        use crate::prompt::builtin;

        #[test]
        fn test_parse_commit_command() {
            let cmd = parse_command_named(builtin::commands::COMMIT, "commit", None, Scope::Builtin).unwrap();
            assert_eq!(cmd.name(), "commit");
            assert!(!cmd.description().is_empty());
            assert!(cmd.content.contains("git"));

            // Check tool restrictions — commit only allows specific Bash commands
            let restrictions = cmd.tool_restrictions();
            assert!(restrictions.is_allowed("Bash", &serde_json::json!({"command": "git status"})));
        }

        #[test]
        fn test_parse_commit_push_pr_command() {
            let cmd = parse_command_named(builtin::commands::COMMIT_PUSH_PR, "commit-push-pr", None, Scope::Builtin).unwrap();
            assert_eq!(cmd.name(), "commit-push-pr");
            assert!(!cmd.description().is_empty());
            assert!(cmd.content.contains("gh pr create"));
        }

        #[test]
        fn test_parse_review_pr_command() {
            let cmd = parse_command_named(builtin::commands::REVIEW_PR, "review-pr", None, Scope::Builtin).unwrap();
            assert_eq!(cmd.name(), "review-pr");
            assert!(!cmd.description().is_empty());
            assert!(cmd.content.contains("Comprehensive PR Review"));
        }

        #[test]
        fn test_argument_substitution_in_command() {
            let cmd = parse_command(
                "---\nname: greet\n---\n\nHello $ARGUMENTS!",
                None,
                Scope::Builtin,
            ).unwrap();
            let result = cmd.substitute_arguments("World");
            assert_eq!(result, "Hello World!");
        }
    }

    mod filesystem_tests {
        use super::*;
        use tempfile::TempDir;

        #[test]
        fn test_load_command_from_file() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("test.md");

            std::fs::write(&file_path, "---\nname: file-cmd\n---\n\nContent").unwrap();

            let cmd = load_command_from_file(&file_path, Scope::Project).unwrap();
            assert_eq!(cmd.name(), "file-cmd");
            assert_eq!(cmd.source_path, Some(file_path));
        }
    }
}
