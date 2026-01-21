//! Project context gathering
//!
//! Gathers relevant project context like CLAUDE.md, git status, etc.
//! Implements Claude Code's 4-tier memory hierarchy:
//!
//! | Priority | Tier       | Location                                    |
//! |----------|------------|---------------------------------------------|
//! | 1        | Enterprise | `/etc/claude-code/CLAUDE.md`                |
//! | 2        | Project    | `./CLAUDE.md`, `./.claude/CLAUDE.md`        |
//! | 3        | Rules      | `./.claude/rules/*.md` (glob patterns)      |
//! | 4        | User       | `~/.claude/CLAUDE.md`, `./CLAUDE.local.md`  |

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Memory tier priority levels (lower = higher priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MemoryTier {
    /// Enterprise-level configuration (e.g., /etc/claude-code/CLAUDE.md)
    Enterprise = 1,
    /// Project-level configuration (e.g., ./CLAUDE.md)
    Project = 2,
    /// Rules directory (e.g., ./.claude/rules/*.md)
    Rules = 3,
    /// User-level configuration (e.g., ~/.claude/CLAUDE.md)
    User = 4,
}

impl std::fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryTier::Enterprise => write!(f, "enterprise"),
            MemoryTier::Project => write!(f, "project"),
            MemoryTier::Rules => write!(f, "rules"),
            MemoryTier::User => write!(f, "user"),
        }
    }
}

/// A single memory file with its content and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFile {
    /// Path to the file
    pub path: PathBuf,
    /// Content of the file
    pub content: String,
    /// Which tier this file belongs to
    pub tier: MemoryTier,
    /// Size in bytes
    pub size: usize,
}

/// Complete memory hierarchy with all loaded files
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryHierarchy {
    /// All memory files, sorted by priority (enterprise first)
    pub files: Vec<MemoryFile>,
    /// Combined content from all files, with section headers
    pub combined_content: String,
    /// Total size of all memory content
    pub total_size: usize,
}

impl MemoryHierarchy {
    /// Check if any memory files were found
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Get the number of memory files
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get files from a specific tier
    pub fn files_in_tier(&self, tier: MemoryTier) -> Vec<&MemoryFile> {
        self.files.iter().filter(|f| f.tier == tier).collect()
    }

    /// Format as a summary string
    pub fn summary(&self) -> String {
        if self.files.is_empty() {
            return "No memory files found.".to_string();
        }

        let mut summary = format!("Memory hierarchy: {} files, {} bytes total\n", self.files.len(), self.total_size);

        for tier in [MemoryTier::Enterprise, MemoryTier::Project, MemoryTier::Rules, MemoryTier::User] {
            let tier_files: Vec<_> = self.files.iter().filter(|f| f.tier == tier).collect();
            if !tier_files.is_empty() {
                summary.push_str(&format!("\n  [{}]:\n", tier));
                for file in tier_files {
                    summary.push_str(&format!("    - {} ({} bytes)\n", file.path.display(), file.size));
                }
            }
        }

        summary
    }
}

/// Gathered project context
#[derive(Debug, Clone, Default)]
pub struct ProjectContext {
    /// Content from CLAUDE.md if found (legacy single-file support)
    pub claude_md: Option<String>,
    /// Full memory hierarchy
    pub memory_hierarchy: Option<MemoryHierarchy>,
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
    workspace: PathBuf,
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

        // Gather the full memory hierarchy
        let hierarchy = self.gather_memory_hierarchy().await;
        if !hierarchy.is_empty() {
            context.claude_md = Some(hierarchy.combined_content.clone());
            context.memory_hierarchy = Some(hierarchy);
        }

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

    /// Gather the 4-tier memory hierarchy
    ///
    /// Priority order (1 = highest):
    /// 1. Enterprise: /etc/claude-code/CLAUDE.md
    /// 2. Project: ./CLAUDE.md, ./.claude/CLAUDE.md
    /// 3. Rules: ./.claude/rules/*.md
    /// 4. User: ~/.claude/CLAUDE.md, ./CLAUDE.local.md
    pub async fn gather_memory_hierarchy(&self) -> MemoryHierarchy {
        let mut files = Vec::new();

        // Tier 1: Enterprise (platform-specific paths)
        let mut enterprise_paths = Vec::new();

        // Unix paths
        #[cfg(not(windows))]
        {
            enterprise_paths.push(PathBuf::from("/etc/claude-code/CLAUDE.md"));
            enterprise_paths.push(PathBuf::from("/etc/cowork/CLAUDE.md"));
        }

        // Windows paths (ProgramData)
        #[cfg(windows)]
        {
            if let Some(program_data) = std::env::var_os("ProgramData") {
                let pd = PathBuf::from(program_data);
                enterprise_paths.push(pd.join("claude-code").join("CLAUDE.md"));
                enterprise_paths.push(pd.join("cowork").join("CLAUDE.md"));
            }
        }

        for path in &enterprise_paths {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let size = content.len();
                files.push(MemoryFile {
                    path: path.clone(),
                    content,
                    tier: MemoryTier::Enterprise,
                    size,
                });
            }
        }

        // Tier 2: Project
        let project_paths = [
            self.workspace.join("CLAUDE.md"),
            self.workspace.join(".claude/CLAUDE.md"),
            self.workspace.join(".cowork/CLAUDE.md"),
        ];

        for path in &project_paths {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let size = content.len();
                files.push(MemoryFile {
                    path: path.clone(),
                    content,
                    tier: MemoryTier::Project,
                    size,
                });
            }
        }

        // Tier 3: Rules (.claude/rules/*.md)
        let rules_dirs = [
            self.workspace.join(".claude/rules"),
            self.workspace.join(".cowork/rules"),
        ];

        for rules_dir in &rules_dirs {
            if rules_dir.exists()
                && let Ok(mut entries) = tokio::fs::read_dir(rules_dir).await {
                    let mut rule_paths = Vec::new();

                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        if path.extension().map(|e| e == "md").unwrap_or(false) {
                            rule_paths.push(path);
                        }
                    }

                    // Sort rule files alphabetically for consistent ordering
                    rule_paths.sort();

                    for path in rule_paths {
                        if let Ok(content) = tokio::fs::read_to_string(&path).await {
                            let size = content.len();
                            files.push(MemoryFile {
                                path,
                                content,
                                tier: MemoryTier::Rules,
                                size,
                            });
                        }
                    }
                }
        }

        // Tier 4: User
        let mut user_paths = vec![
            self.workspace.join("CLAUDE.local.md"),
            self.workspace.join(".claude/CLAUDE.local.md"),
        ];

        // Add home directory paths
        if let Some(home) = dirs::home_dir() {
            user_paths.push(home.join(".claude/CLAUDE.md"));
            user_paths.push(home.join(".cowork/CLAUDE.md"));
        }

        for path in &user_paths {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let size = content.len();
                files.push(MemoryFile {
                    path: path.clone(),
                    content,
                    tier: MemoryTier::User,
                    size,
                });
            }
        }

        // Sort by tier (priority order)
        files.sort_by_key(|f| f.tier);

        // Calculate total size and build combined content
        let total_size: usize = files.iter().map(|f| f.size).sum();
        let combined_content = self.build_combined_content(&files);

        MemoryHierarchy {
            files,
            combined_content,
            total_size,
        }
    }

    /// Build combined content from memory files with section headers
    fn build_combined_content(&self, files: &[MemoryFile]) -> String {
        if files.is_empty() {
            return String::new();
        }

        let mut sections = Vec::new();
        let mut current_tier: Option<MemoryTier> = None;

        for file in files {
            // Add tier header when tier changes
            if current_tier != Some(file.tier) {
                current_tier = Some(file.tier);
                let tier_header = match file.tier {
                    MemoryTier::Enterprise => "=== Enterprise Configuration ===",
                    MemoryTier::Project => "=== Project Instructions ===",
                    MemoryTier::Rules => "=== Project Rules ===",
                    MemoryTier::User => "=== User Configuration ===",
                };
                sections.push(tier_header.to_string());
            }

            // Add file header for rules tier (multiple files)
            if file.tier == MemoryTier::Rules
                && let Some(filename) = file.path.file_name() {
                    sections.push(format!("\n--- {} ---", filename.to_string_lossy()));
                }

            sections.push(file.content.clone());
        }

        sections.join("\n\n")
    }

    /// Read CLAUDE.md content (legacy single-file support)
    #[allow(dead_code)]
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
            && output.status.success() {
                info.branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }

        // Get git status (short format)
        if let Ok(output) = tokio::process::Command::new("git")
            .args(["status", "--short"])
            .current_dir(&self.workspace)
            .output()
            .await
            && output.status.success() {
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

        // Get recent commits
        if let Ok(output) = tokio::process::Command::new("git")
            .args(["log", "--oneline", "-5"])
            .current_dir(&self.workspace)
            .output()
            .await
            && output.status.success() {
                info.commits = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(|l| l.to_string())
                    .collect();
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
