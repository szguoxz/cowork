//! Dynamic skill loader for Claude-standard SKILL.md files
//!
//! Loads skills from:
//! - User level: `~/.claude/skills/`
//! - Project level: `{workspace}/.cowork/skills/`
//!
//! SKILL.md format (Claude Code standard):
//! ```markdown
//! ---
//! name: skill-name
//! description: What the skill does
//! allowed-tools: Read, Bash, Write
//! user-invocable: true
//! ---
//!
//! # Skill Name
//!
//! Instructions for Claude...
//! ```

use crate::skills::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};

/// Frontmatter parsed from SKILL.md
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SkillFrontmatter {
    /// Skill name (lowercase, hyphens only)
    pub name: String,

    /// Description (used for auto-discovery)
    pub description: String,

    /// Allowed tools (comma-separated string or list)
    #[serde(default)]
    pub allowed_tools: AllowedTools,

    /// Can users invoke via /command?
    #[serde(default = "default_true")]
    pub user_invocable: bool,

    /// Specific model to use
    #[serde(default)]
    pub model: Option<String>,

    /// Context mode: "fork" for isolated sub-agent
    #[serde(default)]
    pub context: Option<String>,

    /// Agent type for forked context
    #[serde(default)]
    pub agent: Option<String>,

    /// Usage hint shown in help
    #[serde(default)]
    pub usage: Option<String>,

    /// Custom metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

/// Allowed tools can be a comma-separated string or a list
#[derive(Debug, Clone, Default)]
pub struct AllowedTools(pub Vec<String>);

impl<'de> Deserialize<'de> for AllowedTools {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct AllowedToolsVisitor;

        impl<'de> Visitor<'de> for AllowedToolsVisitor {
            type Value = AllowedTools;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or list of tool names")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let tools: Vec<String> = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                Ok(AllowedTools(tools))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut tools = Vec::new();
                while let Some(tool) = seq.next_element::<String>()? {
                    tools.push(tool);
                }
                Ok(AllowedTools(tools))
            }
        }

        deserializer.deserialize_any(AllowedToolsVisitor)
    }
}

/// A skill loaded from a SKILL.md file
#[derive(Debug, Clone)]
pub struct DynamicSkill {
    /// Parsed frontmatter
    pub frontmatter: SkillFrontmatter,

    /// Markdown body (instructions)
    pub body: String,

    /// Path to the skill directory
    pub path: PathBuf,

    /// Source: "user" or "project"
    pub source: SkillSource,
}

/// Where the skill was loaded from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSource {
    /// ~/.claude/skills/
    User,
    /// {workspace}/.cowork/skills/
    Project,
}

impl std::fmt::Display for SkillSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillSource::User => write!(f, "user"),
            SkillSource::Project => write!(f, "project"),
        }
    }
}

impl DynamicSkill {
    /// Load a skill from a directory containing SKILL.md
    pub fn load(skill_dir: &Path, source: SkillSource) -> Result<Self, SkillLoadError> {
        let skill_file = skill_dir.join("SKILL.md");

        if !skill_file.exists() {
            return Err(SkillLoadError::NotFound(skill_dir.to_path_buf()));
        }

        let content = std::fs::read_to_string(&skill_file)
            .map_err(|e| SkillLoadError::ReadError(skill_file.clone(), e))?;

        Self::parse(&content, skill_dir.to_path_buf(), source)
    }

    /// Parse a SKILL.md file content
    pub fn parse(content: &str, path: PathBuf, source: SkillSource) -> Result<Self, SkillLoadError> {
        // Split frontmatter and body
        let (frontmatter_str, body) = Self::split_frontmatter(content)?;

        // Parse YAML frontmatter
        let frontmatter: SkillFrontmatter = serde_yml::from_str(&frontmatter_str)
            .map_err(|e| SkillLoadError::ParseError(path.clone(), e.to_string()))?;

        // Validate name
        if !Self::is_valid_name(&frontmatter.name) {
            return Err(SkillLoadError::InvalidName(frontmatter.name.clone()));
        }

        Ok(Self {
            frontmatter,
            body: body.trim().to_string(),
            path,
            source,
        })
    }

    /// Split content into frontmatter and body
    fn split_frontmatter(content: &str) -> Result<(String, String), SkillLoadError> {
        let content = content.trim();

        if !content.starts_with("---") {
            return Err(SkillLoadError::MissingFrontmatter);
        }

        // Find the closing ---
        let rest = &content[3..];
        let end_idx = rest
            .find("\n---")
            .ok_or(SkillLoadError::MissingFrontmatter)?;

        let frontmatter = rest[..end_idx].trim().to_string();
        let body = rest[end_idx + 4..].to_string();

        Ok((frontmatter, body))
    }

    /// Check if skill name is valid (lowercase, hyphens, max 64 chars)
    fn is_valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    /// Read additional file from skill directory (e.g., reference.md)
    pub fn read_file(&self, filename: &str) -> Option<String> {
        let file_path = self.path.join(filename);
        std::fs::read_to_string(file_path).ok()
    }

    /// Apply string substitutions to the body
    fn substitute(&self, text: &str, ctx: &SkillContext) -> String {
        text.replace("$ARGUMENTS", &ctx.args)
            .replace("${ARGUMENTS}", &ctx.args)
    }
}

impl Skill for DynamicSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: self.frontmatter.name.clone(),
            display_name: self
                .frontmatter
                .name
                .replace('-', " ")
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_uppercase().chain(chars).collect(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            description: self.frontmatter.description.clone(),
            usage: self
                .frontmatter
                .usage
                .clone()
                .unwrap_or_else(|| format!("/{}", self.frontmatter.name)),
            user_invocable: self.frontmatter.user_invocable,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            // Apply substitutions to the body
            let prompt = self.substitute(&self.body, &ctx);

            // Build metadata
            let mut data = serde_json::json!({
                "skill_name": self.frontmatter.name,
                "skill_source": self.source.to_string(),
                "skill_path": self.path.display().to_string(),
            });

            // Add any custom metadata
            if !self.frontmatter.metadata.is_empty() {
                data["metadata"] = serde_json::to_value(&self.frontmatter.metadata)
                    .unwrap_or(serde_json::Value::Null);
            }

            SkillResult::success(prompt).with_data(data)
        })
    }

    fn prompt_template(&self) -> &str {
        &self.body
    }

    fn allowed_tools(&self) -> Option<Vec<&str>> {
        if self.frontmatter.allowed_tools.0.is_empty() {
            None
        } else {
            Some(
                self.frontmatter
                    .allowed_tools
                    .0
                    .iter()
                    .map(|s| s.as_str())
                    .collect(),
            )
        }
    }
}

/// Errors that can occur when loading skills
#[derive(Debug, thiserror::Error)]
pub enum SkillLoadError {
    #[error("Skill directory not found: {0}")]
    NotFound(PathBuf),

    #[error("Failed to read {0}: {1}")]
    ReadError(PathBuf, std::io::Error),

    #[error("Missing YAML frontmatter (must start and end with ---)")]
    MissingFrontmatter,

    #[error("Failed to parse skill at {0}: {1}")]
    ParseError(PathBuf, String),

    #[error("Invalid skill name '{0}': must be lowercase with hyphens, max 64 chars")]
    InvalidName(String),
}

/// Loader for dynamic skills from filesystem
pub struct SkillLoader {
    /// User skills directory (~/.claude/skills/)
    user_dir: Option<PathBuf>,

    /// Project skills directory ({workspace}/.cowork/skills/)
    project_dir: Option<PathBuf>,
}

impl SkillLoader {
    /// Create a new skill loader
    pub fn new(workspace: &Path) -> Self {
        // User skills: ~/.claude/skills/
        let user_dir = dirs::home_dir().map(|h| h.join(".claude").join("skills"));

        // Project skills: {workspace}/.cowork/skills/
        let project_dir = Some(workspace.join(".cowork").join("skills"));

        Self {
            user_dir,
            project_dir,
        }
    }

    /// Load all skills from both directories
    pub fn load_all(&self) -> Vec<Arc<dyn Skill>> {
        let mut skills: Vec<Arc<dyn Skill>> = Vec::new();
        let mut loaded_names: HashMap<String, SkillSource> = HashMap::new();

        // Load project skills first (higher priority)
        if let Some(ref dir) = self.project_dir {
            for skill in self.load_from_dir(dir, SkillSource::Project) {
                let name = skill.frontmatter.name.clone();
                debug!("Loaded project skill: {}", name);
                loaded_names.insert(name, SkillSource::Project);
                skills.push(Arc::new(skill));
            }
        }

        // Load user skills (skip if already loaded from project)
        if let Some(ref dir) = self.user_dir {
            for skill in self.load_from_dir(dir, SkillSource::User) {
                let name = skill.frontmatter.name.clone();
                if loaded_names.contains_key(&name) {
                    debug!(
                        "Skipping user skill '{}' (overridden by project skill)",
                        name
                    );
                    continue;
                }
                debug!("Loaded user skill: {}", name);
                loaded_names.insert(name, SkillSource::User);
                skills.push(Arc::new(skill));
            }
        }

        skills
    }

    /// Load skills from a directory
    fn load_from_dir(&self, dir: &Path, source: SkillSource) -> Vec<DynamicSkill> {
        let mut skills = Vec::new();

        if !dir.exists() {
            debug!("Skills directory does not exist: {}", dir.display());
            return skills;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                warn!("Failed to read skills directory {}: {}", dir.display(), e);
                return skills;
            }
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Only process directories
            if !path.is_dir() {
                continue;
            }

            // Try to load skill
            match DynamicSkill::load(&path, source) {
                Ok(skill) => skills.push(skill),
                Err(e) => {
                    warn!("Failed to load skill from {}: {}", path.display(), e);
                }
            }
        }

        skills
    }

    /// Reload a specific skill by name
    pub fn reload(&self, name: &str) -> Option<Arc<dyn Skill>> {
        // Check project directory first
        if let Some(ref dir) = self.project_dir {
            let skill_dir = dir.join(name);
            if skill_dir.exists() {
                if let Ok(skill) = DynamicSkill::load(&skill_dir, SkillSource::Project) {
                    return Some(Arc::new(skill));
                }
            }
        }

        // Check user directory
        if let Some(ref dir) = self.user_dir {
            let skill_dir = dir.join(name);
            if skill_dir.exists() {
                if let Ok(skill) = DynamicSkill::load(&skill_dir, SkillSource::User) {
                    return Some(Arc::new(skill));
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_skill_file(dir: &Path, name: &str, content: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn test_parse_basic_skill() {
        let content = r#"---
name: test-skill
description: A test skill
---

# Test Skill

Do something with $ARGUMENTS.
"#;

        let skill = DynamicSkill::parse(content, PathBuf::from("/test"), SkillSource::User).unwrap();

        assert_eq!(skill.frontmatter.name, "test-skill");
        assert_eq!(skill.frontmatter.description, "A test skill");
        assert!(skill.frontmatter.user_invocable);
        assert!(skill.body.contains("Do something"));
    }

    #[test]
    fn test_parse_skill_with_tools() {
        let content = r#"---
name: build-skill
description: Build the project
allowed-tools: Bash, Read, Write
user-invocable: false
---

Build instructions here.
"#;

        let skill =
            DynamicSkill::parse(content, PathBuf::from("/test"), SkillSource::Project).unwrap();

        assert_eq!(skill.frontmatter.name, "build-skill");
        assert!(!skill.frontmatter.user_invocable);
        assert_eq!(
            skill.frontmatter.allowed_tools.0,
            vec!["Bash", "Read", "Write"]
        );
    }

    #[test]
    fn test_parse_skill_with_tools_list() {
        let content = r#"---
name: deploy-skill
description: Deploy the project
allowed-tools:
  - Bash
  - Read
---

Deploy instructions.
"#;

        let skill =
            DynamicSkill::parse(content, PathBuf::from("/test"), SkillSource::User).unwrap();

        assert_eq!(skill.frontmatter.allowed_tools.0, vec!["Bash", "Read"]);
    }

    #[test]
    fn test_invalid_name() {
        let content = r#"---
name: Invalid Name
description: Bad name
---

Body.
"#;

        let result = DynamicSkill::parse(content, PathBuf::from("/test"), SkillSource::User);
        assert!(matches!(result, Err(SkillLoadError::InvalidName(_))));
    }

    #[test]
    fn test_missing_frontmatter() {
        let content = "# Just markdown\n\nNo frontmatter here.";

        let result = DynamicSkill::parse(content, PathBuf::from("/test"), SkillSource::User);
        assert!(matches!(result, Err(SkillLoadError::MissingFrontmatter)));
    }

    #[test]
    fn test_load_from_directory() {
        let dir = TempDir::new().unwrap();

        create_skill_file(
            dir.path(),
            "my-skill",
            r#"---
name: my-skill
description: My custom skill
---

Instructions here.
"#,
        );

        let skill = DynamicSkill::load(&dir.path().join("my-skill"), SkillSource::User).unwrap();
        assert_eq!(skill.frontmatter.name, "my-skill");
    }

    #[test]
    fn test_skill_loader() {
        let workspace = TempDir::new().unwrap();
        let skills_dir = workspace.path().join(".cowork").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_skill_file(
            &skills_dir,
            "project-skill",
            r#"---
name: project-skill
description: A project skill
---

Project skill body.
"#,
        );

        let loader = SkillLoader::new(workspace.path());
        let skills = loader.load_all();

        assert!(!skills.is_empty());
        assert!(skills.iter().any(|s| s.info().name == "project-skill"));
    }

    #[test]
    fn test_substitution() {
        let content = r#"---
name: greet
description: Greet someone
---

Hello $ARGUMENTS! Welcome to ${ARGUMENTS}.
"#;

        let skill =
            DynamicSkill::parse(content, PathBuf::from("/test"), SkillSource::User).unwrap();

        let ctx = SkillContext {
            workspace: PathBuf::from("/workspace"),
            args: "World".to_string(),
            data: HashMap::new(),
        };

        let result = skill.substitute(&skill.body, &ctx);
        assert!(result.contains("Hello World!"));
        assert!(result.contains("Welcome to World"));
    }

    #[tokio::test]
    async fn test_skill_execute() {
        let content = r#"---
name: echo
description: Echo arguments
---

Echo: $ARGUMENTS
"#;

        let skill =
            DynamicSkill::parse(content, PathBuf::from("/test"), SkillSource::User).unwrap();

        let ctx = SkillContext {
            workspace: PathBuf::from("/workspace"),
            args: "hello world".to_string(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Echo: hello world"));
    }
}
