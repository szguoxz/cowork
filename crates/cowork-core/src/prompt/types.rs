//! Core types for the prompt system
//!
//! This module defines foundational data structures for:
//! - Tool specifications and restrictions
//! - Scope hierarchy for component priority
//! - Model preferences

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;

use crate::provider::model_catalog;

/// Tool specification for matching tools by name or pattern
///
/// Patterns support glob-like syntax:
/// - `Bash(git:*)` - match Bash tool with commands starting with "git"
/// - `Write(src/*:*)` - match Write tool for paths under "src/"
/// - `*` - match any tool
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolSpec {
    /// Match all tools
    All,
    /// Match tool by exact name
    Name(String),
    /// Match tool with pattern-based argument matching
    /// Format: "ToolName(arg_pattern)" where arg_pattern uses glob-like syntax
    Pattern {
        tool: String,
        pattern: String,
    },
}

impl ToolSpec {
    /// Parse a tool specification from a string
    ///
    /// Supported formats:
    /// - `*` - match all tools
    /// - `ToolName` - match by exact name
    /// - `ToolName(pattern)` - match with argument pattern
    pub fn parse(s: &str) -> Self {
        let s = s.trim();

        if s == "*" {
            return ToolSpec::All;
        }

        // Check for pattern format: ToolName(pattern)
        if let Some(paren_start) = s.find('(')
            && let Some(inner) = s.strip_suffix(')')
        {
            let tool = s[..paren_start].to_string();
            let pattern = inner[paren_start + 1..].to_string();
            return ToolSpec::Pattern { tool, pattern };
        }

        ToolSpec::Name(s.to_string())
    }

    /// Check if this spec matches a tool invocation
    ///
    /// # Arguments
    /// * `tool_name` - The name of the tool being invoked
    /// * `args` - The JSON arguments being passed to the tool
    pub fn matches(&self, tool_name: &str, args: &Value) -> bool {
        match self {
            ToolSpec::All => true,
            ToolSpec::Name(name) => name == tool_name,
            ToolSpec::Pattern { tool, pattern } => {
                if tool != tool_name {
                    return false;
                }
                Self::match_pattern(pattern, args)
            }
        }
    }

    /// Match a pattern against tool arguments
    ///
    /// Pattern syntax:
    /// - `*` - match anything
    /// - `prefix:*` - for Bash, match commands starting with prefix
    /// - `path/*:*` - for file tools, match paths starting with path/
    fn match_pattern(pattern: &str, args: &Value) -> bool {
        // Handle Bash-style patterns: "git:*", "npm:*"
        if let Some((prefix, suffix_pattern)) = pattern.split_once(':') {
            // For Bash tool, check the "command" field
            if let Some(command) = args.get("command").and_then(|v| v.as_str())
                && suffix_pattern == "*"
            {
                // Check if command starts with the prefix
                return command.starts_with(prefix)
                    || command.split_whitespace().next() == Some(prefix);
            }

            // For file tools (Write, Read, Edit), check "file_path" or "path"
            let path = args
                .get("file_path")
                .or_else(|| args.get("path"))
                .and_then(|v| v.as_str());

            if let Some(path) = path
                && suffix_pattern == "*"
            {
                return Self::glob_match(prefix, path);
            }
        }

        // Simple glob pattern
        pattern == "*"
    }

    /// Simple glob matching for path patterns
    fn glob_match(pattern: &str, path: &str) -> bool {
        if let Some(prefix) = pattern.strip_suffix('*') {
            path.starts_with(prefix)
        } else if let Some(prefix) = pattern.strip_suffix('/') {
            path.starts_with(pattern) || path == prefix
        } else {
            path.starts_with(&format!("{}/", pattern)) || path == pattern
        }
    }
}

impl std::fmt::Display for ToolSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolSpec::All => write!(f, "*"),
            ToolSpec::Name(name) => write!(f, "{}", name),
            ToolSpec::Pattern { tool, pattern } => write!(f, "{}({})", tool, pattern),
        }
    }
}

/// Tool restrictions for controlling which tools can be used
///
/// Restrictions are evaluated as:
/// 1. If `allowed` is non-empty, tool must match at least one allowed spec
/// 2. If tool matches any `denied` spec, it is rejected
/// 3. Denied takes precedence over allowed
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolRestrictions {
    /// Tools that are explicitly allowed
    #[serde(default)]
    pub allowed: Vec<ToolSpec>,
    /// Tools that are explicitly denied
    #[serde(default)]
    pub denied: Vec<ToolSpec>,
}

impl ToolRestrictions {
    /// Create new empty restrictions (allow all)
    pub fn new() -> Self {
        Self::default()
    }

    /// Create restrictions that allow only specific tools
    pub fn allow_only(tools: Vec<ToolSpec>) -> Self {
        Self {
            allowed: tools,
            denied: Vec::new(),
        }
    }

    /// Create restrictions that deny specific tools
    pub fn deny(tools: Vec<ToolSpec>) -> Self {
        Self {
            allowed: Vec::new(),
            denied: tools,
        }
    }

    /// Check if a tool invocation is allowed
    ///
    /// Returns true if:
    /// - No allowed list OR tool matches something in allowed list
    /// - AND tool does not match anything in denied list
    pub fn is_allowed(&self, tool_name: &str, args: &Value) -> bool {
        // First check denied list - denied always wins
        for spec in &self.denied {
            if spec.matches(tool_name, args) {
                return false;
            }
        }

        // If no allowed list, everything (not denied) is allowed
        if self.allowed.is_empty() {
            return true;
        }

        // Check if tool matches any allowed spec
        for spec in &self.allowed {
            if spec.matches(tool_name, args) {
                return true;
            }
        }

        false
    }

    /// Compute the intersection of two restrictions
    ///
    /// The result is the most restrictive combination:
    /// - Allowed = intersection of both allowed lists (if both have allowed lists)
    /// - Denied = union of both denied lists
    pub fn intersect(&self, other: &Self) -> Self {
        // Union of denied lists
        let mut denied = self.denied.clone();
        for spec in &other.denied {
            if !denied.iter().any(|d| d == spec) {
                denied.push(spec.clone());
            }
        }

        // Intersection of allowed lists
        let allowed = if self.allowed.is_empty() && other.allowed.is_empty() {
            Vec::new()
        } else if self.allowed.is_empty() {
            other.allowed.clone()
        } else if other.allowed.is_empty() {
            self.allowed.clone()
        } else {
            // Keep specs that appear in both lists, or are more specific versions
            let mut result = Vec::new();
            for spec in &self.allowed {
                // Check if this spec or something compatible exists in other
                for other_spec in &other.allowed {
                    if Self::specs_compatible(spec, other_spec) {
                        // Use the more specific one
                        let more_specific = Self::more_specific(spec, other_spec);
                        if !result.iter().any(|r| r == more_specific) {
                            result.push(more_specific.clone());
                        }
                    }
                }
            }
            result
        };

        Self { allowed, denied }
    }

    /// Check if two specs are compatible (could potentially match the same tool)
    fn specs_compatible(a: &ToolSpec, b: &ToolSpec) -> bool {
        match (a, b) {
            (ToolSpec::All, _) | (_, ToolSpec::All) => true,
            (ToolSpec::Name(n1), ToolSpec::Name(n2)) => n1 == n2,
            (ToolSpec::Name(n), ToolSpec::Pattern { tool, .. }) => n == tool,
            (ToolSpec::Pattern { tool, .. }, ToolSpec::Name(n)) => tool == n,
            (ToolSpec::Pattern { tool: t1, .. }, ToolSpec::Pattern { tool: t2, .. }) => t1 == t2,
        }
    }

    /// Return the more specific of two compatible specs
    fn more_specific<'a>(a: &'a ToolSpec, b: &'a ToolSpec) -> &'a ToolSpec {
        match (a, b) {
            (ToolSpec::All, other) | (other, ToolSpec::All) => {
                if matches!(other, ToolSpec::All) { a } else { other }
            }
            (ToolSpec::Pattern { .. }, ToolSpec::Name(_)) => a,
            (ToolSpec::Name(_), ToolSpec::Pattern { .. }) => b,
            _ => a, // Same specificity, keep first
        }
    }

    /// Check if this restriction set is empty (allows all)
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty() && self.denied.is_empty()
    }
}

/// Priority scope for prompt components
///
/// Lower values have higher priority and can override higher values.
/// Enterprise settings override Project, which overrides User, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum Scope {
    /// Enterprise-level settings (highest priority)
    Enterprise = 0,
    /// Project-level settings (.claude/ directory)
    Project = 1,
    /// User-level settings (~/.claude/)
    User = 2,
    /// Plugin-provided components
    Plugin = 3,
    /// Built-in defaults (lowest priority)
    #[default]
    Builtin = 4,
}

impl Scope {
    /// Get the numeric priority (lower = higher priority)
    pub fn priority(&self) -> u8 {
        *self as u8
    }

    /// Check if this scope has higher priority than another
    pub fn overrides(&self, other: &Scope) -> bool {
        self.priority() < other.priority()
    }
}

impl Ord for Scope {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower priority number = higher priority = should come first
        self.priority().cmp(&other.priority())
    }
}

impl PartialOrd for Scope {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}


impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Enterprise => write!(f, "enterprise"),
            Scope::Project => write!(f, "project"),
            Scope::User => write!(f, "user"),
            Scope::Plugin => write!(f, "plugin"),
            Scope::Builtin => write!(f, "builtin"),
        }
    }
}

/// Model preference for agents and skills
///
/// Specifies which model should be used for a particular component.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelPreference {
    /// Inherit model from parent context
    #[default]
    Inherit,
    /// Use Claude Opus (most capable)
    Opus,
    /// Use Claude Sonnet (balanced)
    Sonnet,
    /// Use Claude Haiku (fastest)
    Haiku,
    /// Use a custom model by name
    Custom(String),
}

impl ModelPreference {
    /// Parse a model preference from a string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "inherit" | "" => ModelPreference::Inherit,
            "opus" => ModelPreference::Opus,
            "sonnet" => ModelPreference::Sonnet,
            "haiku" => ModelPreference::Haiku,
            other => ModelPreference::Custom(other.to_string()),
        }
    }

    /// Get the model identifier string
    pub fn model_id(&self) -> Option<&str> {
        match self {
            ModelPreference::Inherit => None,
            ModelPreference::Opus => Some(model_catalog::ANTHROPIC_POWERFUL.0),
            ModelPreference::Sonnet => Some(model_catalog::ANTHROPIC_BALANCED.0),
            ModelPreference::Haiku => Some(model_catalog::ANTHROPIC_FAST.0),
            ModelPreference::Custom(id) => Some(id),
        }
    }
}

impl std::fmt::Display for ModelPreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelPreference::Inherit => write!(f, "inherit"),
            ModelPreference::Opus => write!(f, "opus"),
            ModelPreference::Sonnet => write!(f, "sonnet"),
            ModelPreference::Haiku => write!(f, "haiku"),
            ModelPreference::Custom(id) => write!(f, "{}", id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    mod tool_spec_tests {
        use super::*;

        #[test]
        fn test_parse_all() {
            assert_eq!(ToolSpec::parse("*"), ToolSpec::All);
        }

        #[test]
        fn test_parse_name() {
            assert_eq!(ToolSpec::parse("Bash"), ToolSpec::Name("Bash".to_string()));
            assert_eq!(ToolSpec::parse("Read"), ToolSpec::Name("Read".to_string()));
        }

        #[test]
        fn test_parse_pattern() {
            let spec = ToolSpec::parse("Bash(git:*)");
            assert_eq!(spec, ToolSpec::Pattern {
                tool: "Bash".to_string(),
                pattern: "git:*".to_string(),
            });

            let spec = ToolSpec::parse("Write(src/*:*)");
            assert_eq!(spec, ToolSpec::Pattern {
                tool: "Write".to_string(),
                pattern: "src/*:*".to_string(),
            });
        }

        #[test]
        fn test_matches_all() {
            let spec = ToolSpec::All;
            assert!(spec.matches("Bash", &json!({})));
            assert!(spec.matches("Read", &json!({})));
            assert!(spec.matches("AnyTool", &json!({})));
        }

        #[test]
        fn test_matches_name() {
            let spec = ToolSpec::Name("Bash".to_string());
            assert!(spec.matches("Bash", &json!({})));
            assert!(!spec.matches("Read", &json!({})));
        }

        #[test]
        fn test_matches_bash_pattern() {
            let spec = ToolSpec::parse("Bash(git:*)");

            // Should match git commands
            assert!(spec.matches("Bash", &json!({"command": "git status"})));
            assert!(spec.matches("Bash", &json!({"command": "git commit -m 'test'"})));

            // Should not match other commands
            assert!(!spec.matches("Bash", &json!({"command": "npm install"})));
            assert!(!spec.matches("Bash", &json!({"command": "ls -la"})));

            // Should not match other tools
            assert!(!spec.matches("Read", &json!({"command": "git status"})));
        }

        #[test]
        fn test_matches_file_pattern() {
            let spec = ToolSpec::parse("Write(src/*:*)");

            // Should match paths under src/
            assert!(spec.matches("Write", &json!({"file_path": "src/main.rs"})));
            assert!(spec.matches("Write", &json!({"file_path": "src/lib/utils.rs"})));

            // Should not match other paths
            assert!(!spec.matches("Write", &json!({"file_path": "tests/test.rs"})));
            assert!(!spec.matches("Write", &json!({"file_path": "Cargo.toml"})));
        }

        #[test]
        fn test_display() {
            assert_eq!(ToolSpec::All.to_string(), "*");
            assert_eq!(ToolSpec::Name("Bash".to_string()).to_string(), "Bash");
            assert_eq!(
                ToolSpec::Pattern {
                    tool: "Bash".to_string(),
                    pattern: "git:*".to_string(),
                }.to_string(),
                "Bash(git:*)"
            );
        }

        #[test]
        fn test_parse_with_whitespace() {
            assert_eq!(ToolSpec::parse("  *  "), ToolSpec::All);
            assert_eq!(ToolSpec::parse("  Bash  "), ToolSpec::Name("Bash".to_string()));
        }

        #[test]
        fn test_matches_path_field() {
            // Some tools use "path" instead of "file_path"
            let spec = ToolSpec::parse("Glob(src/*:*)");
            assert!(spec.matches("Glob", &json!({"path": "src/main.rs"})));
            assert!(!spec.matches("Glob", &json!({"path": "tests/test.rs"})));
        }

        #[test]
        fn test_matches_npm_pattern() {
            let spec = ToolSpec::parse("Bash(npm:*)");
            assert!(spec.matches("Bash", &json!({"command": "npm install"})));
            assert!(spec.matches("Bash", &json!({"command": "npm run build"})));
            assert!(!spec.matches("Bash", &json!({"command": "yarn install"})));
        }

        #[test]
        fn test_wildcard_pattern() {
            // Just "*" pattern should match anything
            let spec = ToolSpec::Pattern {
                tool: "Bash".to_string(),
                pattern: "*".to_string(),
            };
            assert!(spec.matches("Bash", &json!({"command": "any command"})));
            assert!(!spec.matches("Read", &json!({"command": "any command"})));
        }

        #[test]
        fn test_exact_path_match() {
            // Test exact path matching without wildcard
            let spec = ToolSpec::parse("Write(src:*)");
            // Should match "src/..." paths
            assert!(spec.matches("Write", &json!({"file_path": "src/main.rs"})));
        }
    }

    mod tool_restrictions_tests {
        use super::*;

        #[test]
        fn test_empty_allows_all() {
            let restrictions = ToolRestrictions::new();
            assert!(restrictions.is_allowed("Bash", &json!({})));
            assert!(restrictions.is_allowed("Read", &json!({})));
            assert!(restrictions.is_allowed("Write", &json!({})));
        }

        #[test]
        fn test_allow_only() {
            let restrictions = ToolRestrictions::allow_only(vec![
                ToolSpec::Name("Read".to_string()),
                ToolSpec::Name("Glob".to_string()),
            ]);

            assert!(restrictions.is_allowed("Read", &json!({})));
            assert!(restrictions.is_allowed("Glob", &json!({})));
            assert!(!restrictions.is_allowed("Write", &json!({})));
            assert!(!restrictions.is_allowed("Bash", &json!({})));
        }

        #[test]
        fn test_deny() {
            let restrictions = ToolRestrictions::deny(vec![
                ToolSpec::Name("Write".to_string()),
                ToolSpec::Name("Bash".to_string()),
            ]);

            assert!(restrictions.is_allowed("Read", &json!({})));
            assert!(restrictions.is_allowed("Glob", &json!({})));
            assert!(!restrictions.is_allowed("Write", &json!({})));
            assert!(!restrictions.is_allowed("Bash", &json!({})));
        }

        #[test]
        fn test_denied_overrides_allowed() {
            let restrictions = ToolRestrictions {
                allowed: vec![ToolSpec::All],
                denied: vec![ToolSpec::Name("Bash".to_string())],
            };

            assert!(restrictions.is_allowed("Read", &json!({})));
            assert!(!restrictions.is_allowed("Bash", &json!({})));
        }

        #[test]
        fn test_pattern_restrictions() {
            let restrictions = ToolRestrictions {
                allowed: vec![ToolSpec::parse("Bash(git:*)")],
                denied: vec![],
            };

            assert!(restrictions.is_allowed("Bash", &json!({"command": "git status"})));
            assert!(!restrictions.is_allowed("Bash", &json!({"command": "rm -rf /"})));
        }

        #[test]
        fn test_intersect_denied() {
            let r1 = ToolRestrictions::deny(vec![ToolSpec::Name("A".to_string())]);
            let r2 = ToolRestrictions::deny(vec![ToolSpec::Name("B".to_string())]);
            let result = r1.intersect(&r2);

            assert!(!result.is_allowed("A", &json!({})));
            assert!(!result.is_allowed("B", &json!({})));
            assert!(result.is_allowed("C", &json!({})));
        }

        #[test]
        fn test_intersect_allowed() {
            let r1 = ToolRestrictions::allow_only(vec![
                ToolSpec::Name("A".to_string()),
                ToolSpec::Name("B".to_string()),
            ]);
            let r2 = ToolRestrictions::allow_only(vec![
                ToolSpec::Name("B".to_string()),
                ToolSpec::Name("C".to_string()),
            ]);
            let result = r1.intersect(&r2);

            assert!(!result.is_allowed("A", &json!({})));
            assert!(result.is_allowed("B", &json!({})));
            assert!(!result.is_allowed("C", &json!({})));
        }

        #[test]
        fn test_is_empty() {
            assert!(ToolRestrictions::new().is_empty());
            assert!(!ToolRestrictions::allow_only(vec![ToolSpec::All]).is_empty());
        }

        #[test]
        fn test_intersect_with_empty() {
            // Intersecting with empty should preserve the non-empty restrictions
            let r1 = ToolRestrictions::allow_only(vec![
                ToolSpec::Name("A".to_string()),
                ToolSpec::Name("B".to_string()),
            ]);
            let r2 = ToolRestrictions::new(); // Empty
            let result = r1.intersect(&r2);

            // Should preserve r1's allowed list
            assert!(result.is_allowed("A", &json!({})));
            assert!(result.is_allowed("B", &json!({})));
            assert!(!result.is_allowed("C", &json!({})));
        }

        #[test]
        fn test_intersect_both_empty() {
            let r1 = ToolRestrictions::new();
            let r2 = ToolRestrictions::new();
            let result = r1.intersect(&r2);

            // Should allow everything
            assert!(result.is_allowed("Anything", &json!({})));
            assert!(result.is_empty());
        }

        #[test]
        fn test_intersect_with_all_spec() {
            // One restriction allows all, other allows specific tools
            let r1 = ToolRestrictions::allow_only(vec![ToolSpec::All]);
            let r2 = ToolRestrictions::allow_only(vec![
                ToolSpec::Name("Read".to_string()),
            ]);
            let result = r1.intersect(&r2);

            // Should only allow what both allow - Read
            assert!(result.is_allowed("Read", &json!({})));
            assert!(!result.is_allowed("Write", &json!({})));
        }

        #[test]
        fn test_intersect_pattern_and_name() {
            // Pattern vs Name for same tool
            let r1 = ToolRestrictions::allow_only(vec![
                ToolSpec::parse("Bash(git:*)"),
            ]);
            let r2 = ToolRestrictions::allow_only(vec![
                ToolSpec::Name("Bash".to_string()),
            ]);
            let result = r1.intersect(&r2);

            // Result should be the more specific one (pattern)
            assert!(result.is_allowed("Bash", &json!({"command": "git status"})));
            // The pattern restriction should limit to git commands only
        }
    }

    mod scope_tests {
        use super::*;

        #[test]
        fn test_priority_order() {
            assert!(Scope::Enterprise.priority() < Scope::Project.priority());
            assert!(Scope::Project.priority() < Scope::User.priority());
            assert!(Scope::User.priority() < Scope::Plugin.priority());
            assert!(Scope::Plugin.priority() < Scope::Builtin.priority());
        }

        #[test]
        fn test_overrides() {
            assert!(Scope::Enterprise.overrides(&Scope::Project));
            assert!(Scope::Project.overrides(&Scope::User));
            assert!(!Scope::User.overrides(&Scope::Enterprise));
            assert!(!Scope::Builtin.overrides(&Scope::Plugin));
        }

        #[test]
        fn test_ord() {
            let mut scopes = vec![Scope::User, Scope::Enterprise, Scope::Builtin, Scope::Project];
            scopes.sort();
            assert_eq!(scopes, vec![Scope::Enterprise, Scope::Project, Scope::User, Scope::Builtin]);
        }

        #[test]
        fn test_display() {
            assert_eq!(Scope::Enterprise.to_string(), "enterprise");
            assert_eq!(Scope::Project.to_string(), "project");
            assert_eq!(Scope::User.to_string(), "user");
            assert_eq!(Scope::Plugin.to_string(), "plugin");
            assert_eq!(Scope::Builtin.to_string(), "builtin");
        }

        #[test]
        fn test_default() {
            assert_eq!(Scope::default(), Scope::Builtin);
        }
    }

    mod model_preference_tests {
        use super::*;

        #[test]
        fn test_parse() {
            assert_eq!(ModelPreference::parse("inherit"), ModelPreference::Inherit);
            assert_eq!(ModelPreference::parse(""), ModelPreference::Inherit);
            assert_eq!(ModelPreference::parse("opus"), ModelPreference::Opus);
            assert_eq!(ModelPreference::parse("OPUS"), ModelPreference::Opus);
            assert_eq!(ModelPreference::parse("sonnet"), ModelPreference::Sonnet);
            assert_eq!(ModelPreference::parse("haiku"), ModelPreference::Haiku);
            assert_eq!(
                ModelPreference::parse("claude-3-5-sonnet-20241022"),
                ModelPreference::Custom("claude-3-5-sonnet-20241022".to_string())
            );
        }

        #[test]
        fn test_model_id() {
            assert_eq!(ModelPreference::Inherit.model_id(), None);
            assert_eq!(ModelPreference::Opus.model_id(), Some(model_catalog::ANTHROPIC_POWERFUL.0));
            assert_eq!(ModelPreference::Sonnet.model_id(), Some(model_catalog::ANTHROPIC_BALANCED.0));
            assert_eq!(ModelPreference::Haiku.model_id(), Some(model_catalog::ANTHROPIC_FAST.0));
            assert_eq!(
                ModelPreference::Custom("custom-model".to_string()).model_id(),
                Some("custom-model")
            );
        }

        #[test]
        fn test_display() {
            assert_eq!(ModelPreference::Inherit.to_string(), "inherit");
            assert_eq!(ModelPreference::Opus.to_string(), "opus");
            assert_eq!(ModelPreference::Custom("custom".to_string()).to_string(), "custom");
        }

        #[test]
        fn test_default() {
            assert_eq!(ModelPreference::default(), ModelPreference::Inherit);
        }
    }

    mod serialization_tests {
        use super::*;

        #[test]
        fn test_tool_spec_serde() {
            let spec = ToolSpec::Name("Bash".to_string());
            let json = serde_json::to_string(&spec).unwrap();
            let deserialized: ToolSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(spec, deserialized);
        }

        #[test]
        fn test_scope_serde() {
            let scope = Scope::Project;
            let json = serde_json::to_string(&scope).unwrap();
            assert_eq!(json, "\"Project\"");
            let deserialized: Scope = serde_json::from_str(&json).unwrap();
            assert_eq!(scope, deserialized);
        }

        #[test]
        fn test_model_preference_serde() {
            let pref = ModelPreference::Haiku;
            let json = serde_json::to_string(&pref).unwrap();
            assert_eq!(json, "\"haiku\"");
            let deserialized: ModelPreference = serde_json::from_str(&json).unwrap();
            assert_eq!(pref, deserialized);
        }

        #[test]
        fn test_tool_restrictions_serde() {
            let restrictions = ToolRestrictions {
                allowed: vec![ToolSpec::Name("Read".to_string())],
                denied: vec![ToolSpec::Name("Bash".to_string())],
            };
            let json = serde_json::to_string(&restrictions).unwrap();
            let deserialized: ToolRestrictions = serde_json::from_str(&json).unwrap();
            assert_eq!(restrictions.allowed.len(), deserialized.allowed.len());
            assert_eq!(restrictions.denied.len(), deserialized.denied.len());
        }
    }
}
