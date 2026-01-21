//! YAML frontmatter parser for markdown-based prompt files
//!
//! This module implements parsing of YAML frontmatter from markdown files,
//! following the standard format:
//!
//! ```markdown
//! ---
//! key: value
//! list:
//!   - item1
//!   - item2
//! ---
//!
//! # Markdown content here
//! ```

use serde_json::Value;
use std::collections::HashMap;

/// Result of parsing a markdown file with YAML frontmatter
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    /// Parsed YAML frontmatter as key-value pairs
    pub metadata: HashMap<String, Value>,
    /// Markdown content after the frontmatter
    pub content: String,
}

impl ParsedDocument {
    /// Get a string value from metadata
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).and_then(|v| v.as_str())
    }

    /// Get a boolean value from metadata
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.metadata.get(key).and_then(|v| v.as_bool())
    }

    /// Get an integer value from metadata
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.metadata.get(key).and_then(|v| v.as_i64())
    }

    /// Get a list of strings from metadata
    pub fn get_string_list(&self, key: &str) -> Option<Vec<String>> {
        self.metadata.get(key).and_then(|v| {
            // Handle both array format and comma-separated string format
            if let Some(arr) = v.as_array() {
                Some(arr.iter().filter_map(|item| item.as_str().map(String::from)).collect())
            } else {
                // Parse comma-separated values
                v.as_str().map(|s| {
                    s.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
            }
        })
    }
}

/// Error type for parsing failures
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid YAML frontmatter: {0}")]
    InvalidYaml(String),
    #[error("Missing closing frontmatter delimiter")]
    MissingDelimiter,
}

/// Parse YAML frontmatter from a markdown document
///
/// # Arguments
/// * `input` - The full document content
///
/// # Returns
/// * `Ok(ParsedDocument)` - Successfully parsed document with metadata and content
/// * `Ok(ParsedDocument { metadata: empty, content: input })` - No frontmatter found
/// * `Err(ParseError)` - Invalid frontmatter format
///
/// # Examples
///
/// ```
/// use cowork_core::prompt::parser::parse_frontmatter;
///
/// let doc = "---\nname: Example\nversion: 1\n---\n\n# Content here\n";
///
/// let parsed = parse_frontmatter(doc).unwrap();
/// assert_eq!(parsed.get_string("name"), Some("Example"));
/// assert!(parsed.content.contains("# Content here"));
/// ```
pub fn parse_frontmatter(input: &str) -> Result<ParsedDocument, ParseError> {
    let trimmed = input.trim_start();

    // Check if document starts with frontmatter delimiter
    if !trimmed.starts_with("---") {
        // No frontmatter, return content as-is
        return Ok(ParsedDocument {
            metadata: HashMap::new(),
            content: input.to_string(),
        });
    }

    // Find the opening delimiter
    let after_first = &trimmed[3..];
    let after_first = after_first.trim_start_matches(['\r', '\n']);

    // Find the closing delimiter
    let closing_pos = find_closing_delimiter(after_first)?;
    let yaml_content = &after_first[..closing_pos];
    let remaining = &after_first[closing_pos + 3..]; // Skip past "---"

    // Parse the YAML content
    let metadata = parse_yaml_to_hashmap(yaml_content)?;

    // Get the content after frontmatter, trimming leading newlines
    let content = remaining.trim_start_matches(['\r', '\n']).to_string();

    Ok(ParsedDocument { metadata, content })
}

/// Find the closing "---" delimiter
fn find_closing_delimiter(input: &str) -> Result<usize, ParseError> {
    // Look for "---" at the start of a line
    for (idx, line) in input.lines().enumerate() {
        if line.trim() == "---" {
            // Calculate byte position
            let mut pos = 0;
            for (i, l) in input.lines().enumerate() {
                if i == idx {
                    return Ok(pos);
                }
                pos += l.len() + 1; // +1 for newline
            }
        }
    }

    // Also check if the input ends with ---
    if input.trim_end().ends_with("---") {
        let trimmed_end = input.trim_end();
        if let Some(pos) = trimmed_end.rfind("---") {
            // Make sure it's at the start of a line
            if pos == 0 || trimmed_end.as_bytes().get(pos - 1) == Some(&b'\n') {
                return Ok(pos);
            }
        }
    }

    Err(ParseError::MissingDelimiter)
}

/// Parse YAML string into a HashMap<String, Value>
fn parse_yaml_to_hashmap(yaml: &str) -> Result<HashMap<String, Value>, ParseError> {
    if yaml.trim().is_empty() {
        return Ok(HashMap::new());
    }

    // Use serde_yml to parse YAML
    let yaml_value: serde_yml::Value = serde_yml::from_str(yaml)
        .map_err(|e| ParseError::InvalidYaml(e.to_string()))?;

    // Convert to JSON Value for easier handling
    let json_value = yaml_to_json(yaml_value);

    // Extract as object
    match json_value {
        Value::Object(map) => Ok(map.into_iter().collect()),
        Value::Null => Ok(HashMap::new()),
        _ => Err(ParseError::InvalidYaml("Frontmatter must be a YAML object".to_string())),
    }
}

/// Convert serde_yml::Value to serde_json::Value
fn yaml_to_json(yaml: serde_yml::Value) -> Value {
    match yaml {
        serde_yml::Value::Null => Value::Null,
        serde_yml::Value::Bool(b) => Value::Bool(b),
        serde_yml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
        serde_yml::Value::String(s) => Value::String(s),
        serde_yml::Value::Sequence(seq) => {
            Value::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        serde_yml::Value::Mapping(map) => {
            let obj: serde_json::Map<String, Value> = map
                .into_iter()
                .map(|(k, v)| {
                    let key = match k {
                        serde_yml::Value::String(s) => s,
                        other => format!("{:?}", other),
                    };
                    (key, yaml_to_json(v))
                })
                .collect();
            Value::Object(obj)
        }
        serde_yml::Value::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}

/// Parse tool list from various formats
///
/// Supports:
/// - Comma-separated string: "Read, Write, Glob"
/// - YAML list: ["Read", "Write", "Glob"]
/// - Single tool: "Read"
pub fn parse_tool_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        Value::String(s) => s
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_frontmatter() {
        let input = r#"---
name: TestAgent
description: A test agent
---

# Agent Content

This is the agent prompt.
"#;

        let result = parse_frontmatter(input).unwrap();
        assert_eq!(result.get_string("name"), Some("TestAgent"));
        assert_eq!(result.get_string("description"), Some("A test agent"));
        assert!(result.content.contains("# Agent Content"));
        assert!(result.content.contains("This is the agent prompt."));
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let input = "# Just Markdown\n\nNo frontmatter here.";
        let result = parse_frontmatter(input).unwrap();
        assert!(result.metadata.is_empty());
        assert_eq!(result.content, input);
    }

    #[test]
    fn test_parse_complex_frontmatter() {
        let input = r#"---
name: Explorer
model: haiku
max_turns: 30
tools:
  - Read
  - Glob
  - Grep
nested:
  key: value
---

Content
"#;

        let result = parse_frontmatter(input).unwrap();
        assert_eq!(result.get_string("name"), Some("Explorer"));
        assert_eq!(result.get_string("model"), Some("haiku"));
        assert_eq!(result.get_i64("max_turns"), Some(30));

        let tools = result.get_string_list("tools").unwrap();
        assert_eq!(tools, vec!["Read", "Glob", "Grep"]);
    }

    #[test]
    fn test_parse_comma_separated_tools() {
        let input = r#"---
name: Agent
tools: Read, Write, Glob
---

Content
"#;

        let result = parse_frontmatter(input).unwrap();
        let tools = result.get_string_list("tools").unwrap();
        assert_eq!(tools, vec!["Read", "Write", "Glob"]);
    }

    #[test]
    fn test_parse_boolean_values() {
        let input = r#"---
enabled: true
disabled: false
---

Content
"#;

        let result = parse_frontmatter(input).unwrap();
        assert_eq!(result.get_bool("enabled"), Some(true));
        assert_eq!(result.get_bool("disabled"), Some(false));
    }

    #[test]
    fn test_parse_missing_closing_delimiter() {
        let input = r#"---
name: Test
"#;

        let result = parse_frontmatter(input);
        assert!(matches!(result, Err(ParseError::MissingDelimiter)));
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let input = r#"---
name: [invalid yaml
---

Content
"#;

        let result = parse_frontmatter(input);
        assert!(matches!(result, Err(ParseError::InvalidYaml(_))));
    }

    #[test]
    fn test_parse_empty_frontmatter() {
        let input = r#"---
---

Content
"#;

        let result = parse_frontmatter(input).unwrap();
        assert!(result.metadata.is_empty());
        assert!(result.content.contains("Content"));
    }

    #[test]
    fn test_parse_frontmatter_with_windows_line_endings() {
        let input = "---\r\nname: Test\r\n---\r\n\r\nContent";
        let result = parse_frontmatter(input).unwrap();
        assert_eq!(result.get_string("name"), Some("Test"));
        assert!(result.content.contains("Content"));
    }

    #[test]
    fn test_parse_tool_list_array() {
        let value = serde_json::json!(["Read", "Write", "Glob"]);
        let tools = parse_tool_list(&value);
        assert_eq!(tools, vec!["Read", "Write", "Glob"]);
    }

    #[test]
    fn test_parse_tool_list_string() {
        let value = serde_json::json!("Read, Write, Glob");
        let tools = parse_tool_list(&value);
        assert_eq!(tools, vec!["Read", "Write", "Glob"]);
    }

    #[test]
    fn test_parse_tool_list_empty() {
        let value = serde_json::json!(42);
        let tools = parse_tool_list(&value);
        assert!(tools.is_empty());
    }

    #[test]
    fn test_real_agent_file_format() {
        // Test with the actual format from explore.md
        let input = r#"---
name: Explore
description: "Fast agent specialized for exploring codebases."
model: haiku
color: cyan
tools: Glob, Grep, Read, LSP, WebFetch, WebSearch
context: fork
max_turns: 30
---

# Explore Agent

You are a file search specialist.
"#;

        let result = parse_frontmatter(input).unwrap();
        assert_eq!(result.get_string("name"), Some("Explore"));
        assert_eq!(result.get_string("model"), Some("haiku"));
        assert_eq!(result.get_string("color"), Some("cyan"));
        assert_eq!(result.get_string("context"), Some("fork"));
        assert_eq!(result.get_i64("max_turns"), Some(30));

        let tools = result.get_string_list("tools").unwrap();
        assert_eq!(tools.len(), 6);
        assert!(tools.contains(&"Glob".to_string()));
        assert!(tools.contains(&"WebSearch".to_string()));

        assert!(result.content.contains("# Explore Agent"));
        assert!(result.content.contains("file search specialist"));
    }
}
