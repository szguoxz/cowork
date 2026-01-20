//! Development workflow skills
//!
//! These skills detect the project type and run appropriate commands:
//! - /test - Run project tests
//! - /build - Build the project
//! - /lint - Run linter
//! - /format - Format code

use std::path::{Path, PathBuf};
use tokio::process::Command;

use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};

/// Project type detection result
#[derive(Debug, Clone, PartialEq)]
enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Unknown,
}

/// Detect the project type based on common files
fn detect_project_type(workspace: &Path) -> ProjectType {
    if workspace.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if workspace.join("package.json").exists() {
        ProjectType::Node
    } else if workspace.join("pyproject.toml").exists()
        || workspace.join("setup.py").exists()
        || workspace.join("requirements.txt").exists()
    {
        ProjectType::Python
    } else if workspace.join("go.mod").exists() {
        ProjectType::Go
    } else {
        ProjectType::Unknown
    }
}

/// Run a command in the workspace
async fn run_cmd(workspace: &PathBuf, cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(cmd)
        .args(args)
        .current_dir(workspace)
        .output()
        .await
        .map_err(|e| format!("Failed to run {}: {}", cmd, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(format!("{}{}", stdout, stderr))
    } else {
        Err(format!("Command failed:\n{}{}", stdout, stderr))
    }
}

/// Check if a command exists
async fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// =============================================================================
// TestSkill - /test
// =============================================================================

/// Test skill - runs project tests
pub struct TestSkill {
    workspace: PathBuf,
}

impl TestSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for TestSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "test".to_string(),
            display_name: "Run Tests".to_string(),
            description: "Detect test framework and run project tests".to_string(),
            usage: "/test [args]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let project_type = detect_project_type(&self.workspace);
            let extra_args: Vec<&str> = ctx.args.split_whitespace().collect();

            let result = match project_type {
                ProjectType::Rust => {
                    let mut args = vec!["test"];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "cargo", &args).await
                }
                ProjectType::Node => {
                    // Check for package.json scripts
                    let mut args = vec!["test"];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "npm", &args).await
                }
                ProjectType::Python => {
                    // Try pytest first, then python -m unittest
                    if command_exists("pytest").await {
                        let mut args = vec![];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "pytest", &args).await
                    } else {
                        let mut args = vec!["-m", "unittest", "discover"];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "python", &args).await
                    }
                }
                ProjectType::Go => {
                    let mut args = vec!["test", "./..."];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "go", &args).await
                }
                ProjectType::Unknown => {
                    return SkillResult::error(
                        "Could not detect project type. Supported: Rust, Node, Python, Go"
                    );
                }
            };

            match result {
                Ok(output) => SkillResult::success(format!("Tests completed:\n\n{}", output.trim())),
                Err(e) => SkillResult::error(format!("Tests failed:\n\n{}", e)),
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

// =============================================================================
// BuildSkill - /build
// =============================================================================

/// Build skill - builds the project
pub struct BuildSkill {
    workspace: PathBuf,
}

impl BuildSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for BuildSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "build".to_string(),
            display_name: "Build Project".to_string(),
            description: "Detect build system and build the project".to_string(),
            usage: "/build [args]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let project_type = detect_project_type(&self.workspace);
            let extra_args: Vec<&str> = ctx.args.split_whitespace().collect();

            let result = match project_type {
                ProjectType::Rust => {
                    let mut args = vec!["build"];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "cargo", &args).await
                }
                ProjectType::Node => {
                    // npm run build or npm build
                    let mut args = vec!["run", "build"];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "npm", &args).await
                }
                ProjectType::Python => {
                    // Python typically uses pip install or build module
                    if self.workspace.join("setup.py").exists() {
                        run_cmd(&self.workspace, "python", &["setup.py", "build"]).await
                    } else if self.workspace.join("pyproject.toml").exists() {
                        let mut args = vec!["-m", "build"];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "python", &args).await
                    } else {
                        return SkillResult::success("No build configuration found for Python project.");
                    }
                }
                ProjectType::Go => {
                    let mut args = vec!["build", "./..."];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "go", &args).await
                }
                ProjectType::Unknown => {
                    // Try make if Makefile exists
                    if self.workspace.join("Makefile").exists() {
                        let mut args = vec![];
                        args.extend(extra_args);
                        run_cmd(&self.workspace, "make", &args).await
                    } else {
                        return SkillResult::error(
                            "Could not detect project type. Supported: Rust, Node, Python, Go, Make"
                        );
                    }
                }
            };

            match result {
                Ok(output) => {
                    let msg = if output.trim().is_empty() {
                        "Build completed successfully.".to_string()
                    } else {
                        format!("Build completed:\n\n{}", output.trim())
                    };
                    SkillResult::success(msg)
                }
                Err(e) => SkillResult::error(format!("Build failed:\n\n{}", e)),
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

// =============================================================================
// LintSkill - /lint
// =============================================================================

/// Lint skill - runs linter
pub struct LintSkill {
    workspace: PathBuf,
}

impl LintSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for LintSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "lint".to_string(),
            display_name: "Run Linter".to_string(),
            description: "Detect and run project linter".to_string(),
            usage: "/lint [args]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let project_type = detect_project_type(&self.workspace);
            let extra_args: Vec<&str> = ctx.args.split_whitespace().collect();

            let result = match project_type {
                ProjectType::Rust => {
                    let mut args = vec!["clippy"];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "cargo", &args).await
                }
                ProjectType::Node => {
                    // Try eslint first
                    let mut args = vec!["run", "lint"];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "npm", &args).await
                }
                ProjectType::Python => {
                    // Try ruff, then flake8, then pylint
                    if command_exists("ruff").await {
                        let mut args = vec!["check", "."];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "ruff", &args).await
                    } else if command_exists("flake8").await {
                        let mut args = vec!["."];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "flake8", &args).await
                    } else if command_exists("pylint").await {
                        let mut args = vec!["."];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "pylint", &args).await
                    } else {
                        return SkillResult::error(
                            "No Python linter found. Install ruff, flake8, or pylint."
                        );
                    }
                }
                ProjectType::Go => {
                    // Use golangci-lint if available, otherwise go vet
                    if command_exists("golangci-lint").await {
                        let mut args = vec!["run"];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "golangci-lint", &args).await
                    } else {
                        let mut args = vec!["vet", "./..."];
                        args.extend(extra_args);
                        run_cmd(&self.workspace, "go", &args).await
                    }
                }
                ProjectType::Unknown => {
                    return SkillResult::error(
                        "Could not detect project type. Supported: Rust, Node, Python, Go"
                    );
                }
            };

            match result {
                Ok(output) => {
                    let msg = if output.trim().is_empty() {
                        "No lint issues found.".to_string()
                    } else {
                        format!("Lint results:\n\n{}", output.trim())
                    };
                    SkillResult::success(msg)
                }
                Err(e) => SkillResult::error(format!("Lint failed:\n\n{}", e)),
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

// =============================================================================
// FormatSkill - /format
// =============================================================================

/// Format skill - formats code
pub struct FormatSkill {
    workspace: PathBuf,
}

impl FormatSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for FormatSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "format".to_string(),
            display_name: "Format Code".to_string(),
            description: "Detect and run code formatter".to_string(),
            usage: "/format [args]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let project_type = detect_project_type(&self.workspace);
            let extra_args: Vec<&str> = ctx.args.split_whitespace().collect();

            let result = match project_type {
                ProjectType::Rust => {
                    let mut args = vec!["fmt"];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "cargo", &args).await
                }
                ProjectType::Node => {
                    // Try prettier or npm format script
                    if command_exists("prettier").await {
                        let mut args = vec!["--write", "."];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "prettier", &args).await
                    } else {
                        let mut args = vec!["run", "format"];
                        args.extend(extra_args);
                        run_cmd(&self.workspace, "npm", &args).await
                    }
                }
                ProjectType::Python => {
                    // Try ruff format, then black
                    if command_exists("ruff").await {
                        let mut args = vec!["format", "."];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "ruff", &args).await
                    } else if command_exists("black").await {
                        let mut args = vec!["."];
                        args.extend(extra_args.iter().copied());
                        run_cmd(&self.workspace, "black", &args).await
                    } else {
                        return SkillResult::error(
                            "No Python formatter found. Install ruff or black."
                        );
                    }
                }
                ProjectType::Go => {
                    let mut args = vec!["fmt", "./..."];
                    args.extend(extra_args);
                    run_cmd(&self.workspace, "go", &args).await
                }
                ProjectType::Unknown => {
                    return SkillResult::error(
                        "Could not detect project type. Supported: Rust, Node, Python, Go"
                    );
                }
            };

            match result {
                Ok(output) => {
                    let msg = if output.trim().is_empty() {
                        "Code formatted successfully.".to_string()
                    } else {
                        format!("Format completed:\n\n{}", output.trim())
                    };
                    SkillResult::success(msg)
                }
                Err(e) => SkillResult::error(format!("Format failed:\n\n{}", e)),
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}
