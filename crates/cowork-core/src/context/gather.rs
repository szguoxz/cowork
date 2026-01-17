//! Project context gathering
//!
//! Gathers relevant project context like CLAUDE.md, git status, etc.

use std::path::Path;

/// Gathered project context
#[derive(Debug, Clone, Default)]
pub struct ProjectContext {
    /// Content from CLAUDE.md if found
    pub claude_md: Option<String>,
    /// Current git branch
    pub git_branch: Option<String>,
    /// Git status summary
    pub git_status: Option<String>,
    /// Recent git commits
    pub recent_commits: Vec<String>,
    /// Project type detected
    pub project_type: Option<String>,
    /// Main language detected
    pub main_language: Option<String>,
    /// Key files found (package.json, Cargo.toml, etc.)
    pub key_files: Vec<String>,
}

/// Gathers project context from a workspace
pub struct ContextGatherer {
    workspace: std::path::PathBuf,
}

impl ContextGatherer {
    pub fn new(workspace: impl Into<std::path::PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }

    /// Gather all available project context
    pub async fn gather(&self) -> ProjectContext {
        let mut context = ProjectContext::default();

        // Read CLAUDE.md if present
        context.claude_md = self.read_claude_md().await;

        // Get git info
        if let Some(git_info) = self.gather_git_info().await {
            context.git_branch = Some(git_info.branch);
            context.git_status = Some(git_info.status);
            context.recent_commits = git_info.commits;
        }

        // Detect project type and language
        let (project_type, language) = self.detect_project_type().await;
        context.project_type = project_type;
        context.main_language = language;

        // Find key configuration files
        context.key_files = self.find_key_files().await;

        context
    }

    /// Read CLAUDE.md content
    async fn read_claude_md(&self) -> Option<String> {
        let paths = [
            self.workspace.join("CLAUDE.md"),
            self.workspace.join(".claude/CLAUDE.md"),
            self.workspace.join("docs/CLAUDE.md"),
        ];

        for path in &paths {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                return Some(content);
            }
        }

        None
    }

    /// Gather git information
    async fn gather_git_info(&self) -> Option<GitInfo> {
        // Check if this is a git repository
        let git_dir = self.workspace.join(".git");
        if !git_dir.exists() {
            return None;
        }

        let mut info = GitInfo::default();

        // Get current branch
        if let Ok(output) = tokio::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&self.workspace)
            .output()
            .await
        {
            if output.status.success() {
                info.branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }

        // Get git status (short format)
        if let Ok(output) = tokio::process::Command::new("git")
            .args(["status", "--short"])
            .current_dir(&self.workspace)
            .output()
            .await
        {
            if output.status.success() {
                let status = String::from_utf8_lossy(&output.stdout).to_string();
                let lines: Vec<&str> = status.lines().collect();

                // Summarize status
                let modified = lines.iter().filter(|l| l.starts_with(" M")).count();
                let added = lines.iter().filter(|l| l.starts_with("A ") || l.starts_with("??")).count();
                let deleted = lines.iter().filter(|l| l.starts_with(" D")).count();

                if modified + added + deleted > 0 {
                    info.status = format!(
                        "{} modified, {} added, {} deleted",
                        modified, added, deleted
                    );
                } else {
                    info.status = "Clean".to_string();
                }
            }
        }

        // Get recent commits
        if let Ok(output) = tokio::process::Command::new("git")
            .args(["log", "--oneline", "-5"])
            .current_dir(&self.workspace)
            .output()
            .await
        {
            if output.status.success() {
                info.commits = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(|l| l.to_string())
                    .collect();
            }
        }

        Some(info)
    }

    /// Detect project type and main language
    async fn detect_project_type(&self) -> (Option<String>, Option<String>) {
        let mut project_type = None;
        let mut language = None;

        // Check for common project files
        let checks = [
            ("Cargo.toml", "Rust workspace", "Rust"),
            ("package.json", "Node.js project", "JavaScript/TypeScript"),
            ("pyproject.toml", "Python project", "Python"),
            ("go.mod", "Go module", "Go"),
            ("pom.xml", "Maven project", "Java"),
            ("build.gradle", "Gradle project", "Java/Kotlin"),
            ("Gemfile", "Ruby project", "Ruby"),
            ("composer.json", "PHP project", "PHP"),
            ("CMakeLists.txt", "C/C++ project", "C/C++"),
            ("Makefile", "Make project", "Unknown"),
        ];

        for (file, proj_type, lang) in &checks {
            if self.workspace.join(file).exists() {
                project_type = Some(proj_type.to_string());
                language = Some(lang.to_string());
                break;
            }
        }

        // If we found package.json, check for TypeScript
        if self.workspace.join("tsconfig.json").exists() {
            language = Some("TypeScript".to_string());
        }

        (project_type, language)
    }

    /// Find key configuration files
    async fn find_key_files(&self) -> Vec<String> {
        let key_files = [
            "Cargo.toml",
            "package.json",
            "tsconfig.json",
            "pyproject.toml",
            "requirements.txt",
            "go.mod",
            "Dockerfile",
            "docker-compose.yml",
            ".env.example",
            "README.md",
            "CLAUDE.md",
            ".github/workflows",
            "Makefile",
        ];

        let mut found = Vec::new();
        for file in &key_files {
            let path = self.workspace.join(file);
            if path.exists() {
                found.push(file.to_string());
            }
        }

        found
    }

    /// Format context as a system prompt section
    pub fn format_as_prompt(&self, context: &ProjectContext) -> String {
        let mut sections = Vec::new();

        // CLAUDE.md content takes priority
        if let Some(ref claude_md) = context.claude_md {
            sections.push(format!("=== Project Instructions (CLAUDE.md) ===\n{}", claude_md));
        }

        // Project info
        let mut project_info = Vec::new();
        if let Some(ref pt) = context.project_type {
            project_info.push(format!("Project: {}", pt));
        }
        if let Some(ref lang) = context.main_language {
            project_info.push(format!("Language: {}", lang));
        }
        if !context.key_files.is_empty() {
            project_info.push(format!("Key files: {}", context.key_files.join(", ")));
        }

        if !project_info.is_empty() {
            sections.push(format!("=== Project Info ===\n{}", project_info.join("\n")));
        }

        // Git info
        if context.git_branch.is_some() || context.git_status.is_some() {
            let mut git_info = Vec::new();
            if let Some(ref branch) = context.git_branch {
                git_info.push(format!("Branch: {}", branch));
            }
            if let Some(ref status) = context.git_status {
                git_info.push(format!("Status: {}", status));
            }
            if !context.recent_commits.is_empty() {
                git_info.push(format!(
                    "Recent commits:\n  {}",
                    context.recent_commits.join("\n  ")
                ));
            }
            sections.push(format!("=== Git Info ===\n{}", git_info.join("\n")));
        }

        sections.join("\n\n")
    }
}

#[derive(Debug, Default)]
struct GitInfo {
    branch: String,
    status: String,
    commits: Vec<String>,
}
