//! Built-in skills embedded in the binary
//!
//! Official Claude Code plugin commands loaded from .md files via include_str!().

use std::path::PathBuf;
use std::sync::Arc;

use super::loader::{DynamicSkill, SkillSource};
use super::Skill;

/// Built-in skill definitions: (name, content)
const BUILTIN_SKILLS: &[(&str, &str)] = &[
    ("commit", include_str!("../prompt/builtin/commands/commit.md")),
    ("commit-push-pr", include_str!("../prompt/builtin/commands/commit-push-pr.md")),
    ("clean_gone", include_str!("../prompt/builtin/commands/clean_gone.md")),
    ("code-review", include_str!("../prompt/builtin/commands/code-review.md")),
    ("feature-dev", include_str!("../prompt/builtin/commands/feature-dev.md")),
    ("review-pr", include_str!("../prompt/builtin/commands/review-pr.md")),
];

/// Load all built-in skills
pub fn load_builtin_skills() -> Vec<Arc<dyn Skill>> {
    BUILTIN_SKILLS
        .iter()
        .filter_map(|(name, content)| {
            match DynamicSkill::parse_with_name(content, name, PathBuf::from("<builtin>"), SkillSource::User) {
                Ok(skill) => Some(Arc::new(skill) as Arc<dyn Skill>),
                Err(e) => {
                    tracing::warn!("Failed to load built-in skill '{}': {}", name, e);
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_all_builtin_skills() {
        let skills = load_builtin_skills();
        assert_eq!(skills.len(), BUILTIN_SKILLS.len(), "All built-in skills should parse successfully");

        let names: Vec<_> = skills.iter().map(|s| s.info().name.clone()).collect();
        assert!(names.contains(&"commit".to_string()));
        assert!(names.contains(&"commit-push-pr".to_string()));
        assert!(names.contains(&"clean_gone".to_string()));
        assert!(names.contains(&"code-review".to_string()));
        assert!(names.contains(&"feature-dev".to_string()));
        assert!(names.contains(&"review-pr".to_string()));
    }

    #[test]
    fn test_builtin_skill_info() {
        let skills = load_builtin_skills();

        for skill in &skills {
            let info = skill.info();
            assert!(!info.name.is_empty());
            assert!(!info.description.is_empty());
        }
    }

    #[test]
    fn test_commit_skill_has_substitutions() {
        let skill = DynamicSkill::parse_with_name(
            BUILTIN_SKILLS[0].1,
            "commit",
            PathBuf::from("<builtin>"),
            SkillSource::User,
        )
        .expect("Failed to parse commit skill");

        assert_eq!(skill.frontmatter.name, "commit");
        // Should contain command substitution markers
        assert!(skill.body.contains("!`git status`"));
        assert!(skill.body.contains("!`git diff HEAD`"));
    }
}
