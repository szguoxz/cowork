//! Git-related skills for common git workflows
//!
//! These skills mirror Claude Code's commit-commands plugin:
//! - /commit - Stage changes and create a commit
//! - /commit-push-pr - Commit, push, and create a PR in one step
//! - /push - Push commits to remote
//! - /pr - Create a pull request
//! - /review - Review staged changes
//! - /clean-gone - Clean up branches deleted from remote

use std::path::PathBuf;
use tokio::process::Command;

use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};

/// Helper to run git commands
async fn run_git(workspace: &PathBuf, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .await
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Helper to run gh (GitHub CLI) commands
async fn run_gh(workspace: &PathBuf, args: &[&str]) -> Result<String, String> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(workspace)
        .output()
        .await
        .map_err(|e| format!("Failed to run gh: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// =============================================================================
// CommitSkill - /commit
// =============================================================================

/// Commit skill - stages changes and creates a commit
///
/// Mirrors Claude Code's /commit command:
/// - Analyzes current git status
/// - Reviews staged and unstaged changes
/// - Examines recent commits to match repository style
/// - Stages relevant files
/// - Creates the commit with appropriate message
pub struct CommitSkill {
    workspace: PathBuf,
}

impl CommitSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for CommitSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "commit".to_string(),
            display_name: "Git Commit".to_string(),
            description: "Stage changes and create a git commit with an auto-generated message".to_string(),
            usage: "/commit [optional message hint]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            // Gather context like Claude Code does
            let status = run_git(&self.workspace, &["status", "--short"]).await
                .unwrap_or_else(|e| format!("Error: {}", e));

            if status.trim().is_empty() {
                return SkillResult::success("No changes to commit. Working tree is clean.");
            }

            let diff = run_git(&self.workspace, &["diff", "HEAD"]).await
                .unwrap_or_else(|e| format!("Error: {}", e));

            let branch = run_git(&self.workspace, &["branch", "--show-current"]).await
                .unwrap_or_else(|_| "unknown".to_string());

            let recent_commits = run_git(&self.workspace, &["log", "--oneline", "-10"]).await
                .unwrap_or_default();

            // Build the prompt following Claude Code's format
            let prompt = format!(
                r#"Create a single git commit for the changes below.

## Context

**Allowed tools:** git add, git status, git commit

**Current branch:** {}

**Git status:**
```
{}
```

**Changes (diff):**
```diff
{}
```

**Recent commits (for style reference):**
```
{}
```

{}

## Task

Based on these changes:
1. Stage all relevant files (git add -A or git add specific files)
2. Create a commit with an appropriate message following conventional commit format

Commit message format:
- type(scope): description
- Types: feat, fix, docs, style, refactor, test, chore
- Keep the first line under 72 characters
- Match the style of recent commits in this repository

Do not commit files that might contain secrets (.env, credentials.json, etc.).

Execute the git commands now. Only output tool calls, no explanatory text."#,
                branch.trim(),
                status.trim(),
                truncate_diff(&diff, 8000),
                recent_commits.trim(),
                if !ctx.args.is_empty() {
                    format!("**User hint:** {}", ctx.args)
                } else {
                    String::new()
                }
            );

            SkillResult::success(prompt)
                .with_data(serde_json::json!({
                    "status": status.trim(),
                    "branch": branch.trim(),
                    "has_changes": true
                }))
        })
    }

    fn prompt_template(&self) -> &str {
        "Create a git commit based on the context provided."
    }

    fn allowed_tools(&self) -> Option<Vec<&str>> {
        Some(vec!["execute_command"])
    }
}

// =============================================================================
// CommitPushPrSkill - /commit-push-pr
// =============================================================================

/// Commit, push, and create PR in one step
///
/// Mirrors Claude Code's /commit-push-pr command:
/// - Creates new branch if on main
/// - Stages and commits changes
/// - Pushes branch to origin
/// - Creates PR using gh pr create
pub struct CommitPushPrSkill {
    workspace: PathBuf,
}

impl CommitPushPrSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for CommitPushPrSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "commit-push-pr".to_string(),
            display_name: "Commit, Push & PR".to_string(),
            description: "Commit changes, push to remote, and create a pull request".to_string(),
            usage: "/commit-push-pr".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, _ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            // Gather context
            let status = run_git(&self.workspace, &["status", "--short"]).await
                .unwrap_or_else(|e| format!("Error: {}", e));

            let diff = run_git(&self.workspace, &["diff", "HEAD"]).await
                .unwrap_or_else(|e| format!("Error: {}", e));

            let branch = run_git(&self.workspace, &["branch", "--show-current"]).await
                .unwrap_or_else(|_| "unknown".to_string());

            let prompt = format!(
                r#"Complete the following git workflow in a single response.

## Context

**Allowed tools:** git (branch, add, status, commit, push), gh pr create

**Current branch:** {}

**Git status:**
```
{}
```

**Changes (diff):**
```diff
{}
```

## Task (execute ALL steps in one response)

1. **Branch Management**: If on main/master, create a new feature branch
2. **Commit**: Stage all changes and create a commit with appropriate message
3. **Push**: Push the branch to origin with -u flag
4. **Pull Request**: Create PR using `gh pr create` with:
   - Descriptive title
   - Body with summary and test plan

PR body format:
```
## Summary
<1-3 bullet points>

## Test plan
- [ ] Test item 1
- [ ] Test item 2

🤖 Generated with Cowork
```

Execute all commands now. Only output tool calls, no explanatory text."#,
                branch.trim(),
                status.trim(),
                truncate_diff(&diff, 5000),
            );

            SkillResult::success(prompt)
        })
    }

    fn prompt_template(&self) -> &str {
        "Commit, push, and create a pull request."
    }

    fn allowed_tools(&self) -> Option<Vec<&str>> {
        Some(vec!["execute_command"])
    }
}

// =============================================================================
// PushSkill - /push
// =============================================================================

/// Push skill - pushes commits to remote
pub struct PushSkill {
    workspace: PathBuf,
}

impl PushSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for PushSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "push".to_string(),
            display_name: "Git Push".to_string(),
            description: "Push commits to the remote repository".to_string(),
            usage: "/push [remote] [branch]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let branch = run_git(&self.workspace, &["branch", "--show-current"]).await
                .unwrap_or_else(|_| "unknown".to_string());

            let status = run_git(&self.workspace, &["status", "-sb"]).await
                .unwrap_or_else(|e| format!("Error: {}", e));

            // Parse remote and branch from args
            let parts: Vec<&str> = ctx.args.split_whitespace().collect();
            let remote = parts.first().unwrap_or(&"origin");
            let target_branch = parts.get(1).map(|s| s.to_string()).unwrap_or_else(|| branch.trim().to_string());

            let prompt = format!(
                r#"Push commits to remote repository.

## Context

**Current branch:** {}
**Target:** {}/{}

**Status:**
```
{}
```

## Task

Run: `git push {} {}`

If the branch doesn't have an upstream, use: `git push -u {} {}`

Execute the push command now."#,
                branch.trim(), remote, target_branch,
                status.trim(),
                remote, target_branch,
                remote, target_branch
            );

            SkillResult::success(prompt)
                .with_data(serde_json::json!({
                    "branch": branch.trim(),
                    "remote": remote,
                    "target_branch": target_branch
                }))
        })
    }

    fn prompt_template(&self) -> &str {
        "Push commits to remote repository."
    }

    fn allowed_tools(&self) -> Option<Vec<&str>> {
        Some(vec!["execute_command"])
    }
}

// =============================================================================
// PullRequestSkill - /pr
// =============================================================================

/// Pull Request skill - creates a PR
pub struct PullRequestSkill {
    workspace: PathBuf,
}

impl PullRequestSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for PullRequestSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "pr".to_string(),
            display_name: "Pull Request".to_string(),
            description: "Create a pull request with auto-generated description".to_string(),
            usage: "/pr [title]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let branch = run_git(&self.workspace, &["branch", "--show-current"]).await
                .unwrap_or_else(|_| "unknown".to_string());

            // Determine base branch
            let main_branch = if run_git(&self.workspace, &["rev-parse", "--verify", "main"]).await.is_ok() {
                "main"
            } else {
                "master"
            };

            let commits = run_git(&self.workspace, &["log", &format!("{}..HEAD", main_branch), "--oneline"]).await
                .unwrap_or_else(|_| "Unable to get commits".to_string());

            let diff_stat = run_git(&self.workspace, &["diff", "--stat", main_branch]).await
                .unwrap_or_else(|_| "Unable to get diff".to_string());

            let title = if ctx.args.is_empty() {
                branch.trim()
                    .replace('-', " ")
                    .replace('_', " ")
                    .split('/')
                    .last()
                    .unwrap_or(branch.trim())
                    .to_string()
            } else {
                ctx.args.clone()
            };

            let prompt = format!(
                r#"Create a pull request.

## Context

**Current branch:** {}
**Base branch:** {}

**Commits in this PR:**
```
{}
```

**Changes summary:**
```
{}
```

**Suggested title:** {}

## Task

Create a PR using `gh pr create` with:
- Title: "{}"
- Base: {}
- Body with summary (1-3 bullets) and test plan

Body format:
```
## Summary
<bullet points summarizing the changes>

## Test plan
- [ ] Test instructions

🤖 Generated with Cowork
```

Execute the gh command now."#,
                branch.trim(), main_branch,
                commits.trim(),
                diff_stat.trim(),
                title, title, main_branch
            );

            SkillResult::success(prompt)
                .with_data(serde_json::json!({
                    "branch": branch.trim(),
                    "base": main_branch,
                    "title": title
                }))
        })
    }

    fn prompt_template(&self) -> &str {
        "Create a pull request with auto-generated description."
    }

    fn allowed_tools(&self) -> Option<Vec<&str>> {
        Some(vec!["execute_command"])
    }
}

// =============================================================================
// ReviewSkill - /review
// =============================================================================

/// Review skill - reviews staged changes
pub struct ReviewSkill {
    workspace: PathBuf,
}

impl ReviewSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for ReviewSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "review".to_string(),
            display_name: "Code Review".to_string(),
            description: "Review staged changes and provide feedback".to_string(),
            usage: "/review".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, _ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            // Get staged diff first, fall back to unstaged
            let diff = match run_git(&self.workspace, &["diff", "--cached"]).await {
                Ok(d) if !d.trim().is_empty() => d,
                _ => match run_git(&self.workspace, &["diff"]).await {
                    Ok(d) if !d.trim().is_empty() => d,
                    _ => return SkillResult::success("No changes to review. Stage some changes first with `git add`."),
                }
            };

            let prompt = format!(
                r#"Review the following code changes.

## Changes to Review

```diff
{}
```

## Review Checklist

Analyze the changes for:
1. **Correctness** - Logic errors, bugs, edge cases
2. **Security** - Injection vulnerabilities, secrets exposure, OWASP issues
3. **Performance** - Inefficient algorithms, N+1 queries, memory leaks
4. **Code quality** - Readability, naming, duplication
5. **Error handling** - Missing error cases, poor error messages
6. **Tests** - Missing test coverage, test quality

## Task

Provide a code review with:
1. Summary of changes (what the code does)
2. Issues found (if any) with severity and line numbers
3. Suggestions for improvement
4. Overall verdict: ✅ Approve, ⚠️ Request changes, or 💬 Comment

Focus on HIGH-SIGNAL issues only - bugs, security problems, clear violations.
Skip minor style preferences."#,
                truncate_diff(&diff, 10000)
            );

            SkillResult::success(prompt)
        })
    }

    fn prompt_template(&self) -> &str {
        "Review code changes and provide feedback."
    }
}

// =============================================================================
// CleanGoneSkill - /clean-gone
// =============================================================================

/// Clean up local branches that have been deleted from remote
///
/// Mirrors Claude Code's /clean_gone command:
/// - Lists branches to identify [gone] status
/// - Removes worktrees associated with gone branches
/// - Deletes stale local branches
pub struct CleanGoneSkill {
    workspace: PathBuf,
}

impl CleanGoneSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Skill for CleanGoneSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "clean-gone".to_string(),
            display_name: "Clean Gone Branches".to_string(),
            description: "Clean up local branches that have been deleted from remote".to_string(),
            usage: "/clean-gone".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, _ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            // First fetch and prune to update tracking info
            let _ = run_git(&self.workspace, &["fetch", "--prune"]).await;

            // Get list of branches with their tracking status
            let branches = run_git(&self.workspace, &["branch", "-vv"]).await
                .unwrap_or_else(|e| format!("Error: {}", e));

            let prompt = format!(
                r#"Clean up local branches that have been deleted from remote.

## Context

**Branch status:**
```
{}
```

## Task

1. Identify branches marked as `[gone]` (deleted from remote)
2. For each gone branch:
   - Check if it has an associated worktree
   - Remove the worktree if present: `git worktree remove <path>`
   - Delete the branch: `git branch -D <branch-name>`
3. Report what was cleaned up

**Safety rules:**
- Never delete main/master branches
- Skip the currently checked out branch
- Only delete branches with `[gone]` tracking status

If no branches need cleanup, report that the repository is clean.

Execute the cleanup now."#,
                branches.trim()
            );

            SkillResult::success(prompt)
        })
    }

    fn prompt_template(&self) -> &str {
        "Clean up branches deleted from remote."
    }

    fn allowed_tools(&self) -> Option<Vec<&str>> {
        Some(vec!["execute_command"])
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Truncate diff to max length, preserving meaningful content
fn truncate_diff(diff: &str, max_len: usize) -> String {
    if diff.len() <= max_len {
        diff.to_string()
    } else {
        format!(
            "{}...\n\n[truncated - {} more characters]",
            &diff[..max_len],
            diff.len() - max_len
        )
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().expect("Failed to create temp dir");

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to init git");

        // Configure git user for commits
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to config email");

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to config name");

        // Create initial commit
        std::fs::write(dir.path().join("README.md"), "# Test\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .expect("Failed to add");
        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to commit");

        dir
    }

    #[tokio::test]
    async fn test_commit_skill_no_changes() {
        let dir = setup_git_repo();
        let skill = CommitSkill::new(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("clean"));
    }

    #[tokio::test]
    async fn test_commit_skill_with_changes() {
        let dir = setup_git_repo();

        // Create a change
        std::fs::write(dir.path().join("new_file.txt"), "Hello\n").unwrap();

        let skill = CommitSkill::new(dir.path().to_path_buf());
        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("git add"));
        assert!(result.response.contains("new_file.txt"));
    }

    #[tokio::test]
    async fn test_review_skill_no_changes() {
        let dir = setup_git_repo();
        let skill = ReviewSkill::new(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("No changes to review"));
    }

    #[tokio::test]
    async fn test_review_skill_with_changes() {
        let dir = setup_git_repo();

        // Modify an existing tracked file (README.md was created in setup)
        std::fs::write(dir.path().join("README.md"), "# Test\n\nModified content.\n").unwrap();

        let skill = ReviewSkill::new(dir.path().to_path_buf());
        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("Review"));
        assert!(result.response.contains("README"));
    }

    #[tokio::test]
    async fn test_push_skill() {
        let dir = setup_git_repo();
        let skill = PushSkill::new(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("git push"));
    }

    #[tokio::test]
    async fn test_clean_gone_skill() {
        let dir = setup_git_repo();
        let skill = CleanGoneSkill::new(dir.path().to_path_buf());

        let ctx = SkillContext {
            workspace: dir.path().to_path_buf(),
            args: String::new(),
            data: HashMap::new(),
        };

        let result = skill.execute(ctx).await;
        assert!(result.success);
        assert!(result.response.contains("gone"));
    }

    #[test]
    fn test_skill_info() {
        let dir = TempDir::new().unwrap();

        let commit = CommitSkill::new(dir.path().to_path_buf());
        assert_eq!(commit.info().name, "commit");
        assert!(commit.info().user_invocable);

        let push = PushSkill::new(dir.path().to_path_buf());
        assert_eq!(push.info().name, "push");

        let pr = PullRequestSkill::new(dir.path().to_path_buf());
        assert_eq!(pr.info().name, "pr");

        let review = ReviewSkill::new(dir.path().to_path_buf());
        assert_eq!(review.info().name, "review");

        let clean = CleanGoneSkill::new(dir.path().to_path_buf());
        assert_eq!(clean.info().name, "clean-gone");
    }

    #[test]
    fn test_allowed_tools() {
        let dir = TempDir::new().unwrap();

        let commit = CommitSkill::new(dir.path().to_path_buf());
        assert!(commit.allowed_tools().is_some());
        assert!(commit.allowed_tools().unwrap().contains(&"execute_command"));

        let review = ReviewSkill::new(dir.path().to_path_buf());
        assert!(review.allowed_tools().is_none()); // Review doesn't need to execute commands
    }
}
