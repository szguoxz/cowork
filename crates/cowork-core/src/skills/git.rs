//! Git-related skills for common git workflows

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::process::Command;

use super::{Skill, SkillContext, SkillInfo, SkillResult};

/// Commit skill - stages changes and creates a commit
pub struct CommitSkill {
    workspace: PathBuf,
}

impl CommitSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    async fn run_git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

#[async_trait]
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

    async fn execute(&self, ctx: SkillContext) -> SkillResult {
        // Get git status
        let status = match self.run_git(&["status", "--short"]).await {
            Ok(s) => s,
            Err(e) => return SkillResult::error(format!("Failed to get git status: {}", e)),
        };

        if status.trim().is_empty() {
            return SkillResult::success("No changes to commit. Working tree is clean.");
        }

        // Get diff for staged and unstaged changes
        let diff = match self.run_git(&["diff", "HEAD"]).await {
            Ok(d) => d,
            Err(e) => return SkillResult::error(format!("Failed to get diff: {}", e)),
        };

        // Get recent commits for style reference
        let recent_commits = self.run_git(&["log", "--oneline", "-5"]).await.unwrap_or_default();

        // Build response with context for LLM to generate commit message
        let response = format!(
            r#"I'll help you create a git commit. Here's the current state:

## Git Status
```
{}
```

## Changes (diff)
```diff
{}
```

## Recent Commits (for style reference)
```
{}
```

{}

Based on these changes, I'll:
1. Stage all modified files (git add -A)
2. Generate an appropriate commit message
3. Create the commit

Please confirm or provide additional context for the commit message."#,
            status.trim(),
            if diff.len() > 5000 { &diff[..5000] } else { &diff },
            recent_commits.trim(),
            if !ctx.args.is_empty() {
                format!("User hint: {}", ctx.args)
            } else {
                String::new()
            }
        );

        SkillResult::success(response)
            .with_data(serde_json::json!({
                "status": status.trim(),
                "has_changes": true,
                "suggested_actions": ["stage_all", "commit"]
            }))
    }

    fn prompt_template(&self) -> &str {
        r#"You are helping the user create a git commit. Analyze the changes and:
1. Summarize what changed
2. Suggest a commit message following conventional commit format
3. Ask for confirmation before committing

Format commit messages as:
- type(scope): description
- Types: feat, fix, docs, style, refactor, test, chore
- Keep the first line under 72 characters"#
    }
}

/// Push skill - pushes commits to remote
pub struct PushSkill {
    workspace: PathBuf,
}

impl PushSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    async fn run_git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

#[async_trait]
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

    async fn execute(&self, ctx: SkillContext) -> SkillResult {
        // Get current branch
        let branch = match self.run_git(&["branch", "--show-current"]).await {
            Ok(b) => b.trim().to_string(),
            Err(e) => return SkillResult::error(format!("Failed to get current branch: {}", e)),
        };

        // Check if there are commits to push
        let status = match self.run_git(&["status", "-sb"]).await {
            Ok(s) => s,
            Err(e) => return SkillResult::error(format!("Failed to get status: {}", e)),
        };

        // Parse remote and branch from args
        let parts: Vec<&str> = ctx.args.split_whitespace().collect();
        let remote = parts.get(0).unwrap_or(&"origin");
        let target_branch = parts.get(1).map(|s| s.to_string()).unwrap_or(branch.clone());

        let response = format!(
            r#"Ready to push to remote.

Current branch: {}
Target: {}/{}

Status:
```
{}
```

I'll run: git push {} {}

Please confirm to proceed."#,
            branch, remote, target_branch, status.trim(), remote, target_branch
        );

        SkillResult::success(response)
            .with_data(serde_json::json!({
                "branch": branch,
                "remote": remote,
                "target_branch": target_branch,
                "command": format!("git push {} {}", remote, target_branch)
            }))
    }

    fn prompt_template(&self) -> &str {
        "You are helping the user push commits to a remote repository. Confirm the target and execute the push."
    }
}

/// Pull Request skill - creates a PR
pub struct PullRequestSkill {
    workspace: PathBuf,
}

impl PullRequestSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    async fn run_git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    async fn run_gh(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("gh")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| format!("Failed to run gh: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

#[async_trait]
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

    async fn execute(&self, ctx: SkillContext) -> SkillResult {
        // Get current branch
        let branch = match self.run_git(&["branch", "--show-current"]).await {
            Ok(b) => b.trim().to_string(),
            Err(e) => return SkillResult::error(format!("Failed to get current branch: {}", e)),
        };

        // Get main branch (try main, then master)
        let main_branch = if self.run_git(&["rev-parse", "--verify", "main"]).await.is_ok() {
            "main"
        } else {
            "master"
        };

        // Get commits in this branch
        let commits = match self.run_git(&["log", &format!("{}..HEAD", main_branch), "--oneline"]).await {
            Ok(c) => c,
            Err(_) => "Unable to get commit log".to_string(),
        };

        // Get diff summary
        let diff_stat = match self.run_git(&["diff", "--stat", main_branch]).await {
            Ok(d) => d,
            Err(_) => "Unable to get diff stats".to_string(),
        };

        let title = if ctx.args.is_empty() {
            // Generate title from branch name
            branch
                .replace('-', " ")
                .replace('_', " ")
                .split('/')
                .last()
                .unwrap_or(&branch)
                .to_string()
        } else {
            ctx.args.clone()
        };

        let response = format!(
            r#"I'll help you create a pull request.

## Branch Info
- Current branch: {}
- Base branch: {}

## Commits in this PR
```
{}
```

## Changes Summary
```
{}
```

## Suggested PR Title
{}

I'll create the PR with:
- Title: "{}"
- Base: {}

I'll generate a description based on the commits and changes. Please confirm or provide additional context."#,
            branch, main_branch, commits.trim(), diff_stat.trim(), title, title, main_branch
        );

        SkillResult::success(response)
            .with_data(serde_json::json!({
                "branch": branch,
                "base": main_branch,
                "title": title,
                "commits": commits.lines().count()
            }))
    }

    fn prompt_template(&self) -> &str {
        r#"You are helping the user create a pull request. Generate:
1. A concise title (if not provided)
2. A description with:
   - Summary of changes (1-3 bullet points)
   - Test plan
3. Use the gh CLI to create the PR"#
    }
}

/// Review skill - reviews staged changes
pub struct ReviewSkill {
    workspace: PathBuf,
}

impl ReviewSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    async fn run_git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

#[async_trait]
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

    async fn execute(&self, _ctx: SkillContext) -> SkillResult {
        // Get staged diff
        let staged_diff = match self.run_git(&["diff", "--cached"]).await {
            Ok(d) if !d.trim().is_empty() => d,
            Ok(_) => {
                // No staged changes, try unstaged
                match self.run_git(&["diff"]).await {
                    Ok(d) if !d.trim().is_empty() => d,
                    _ => return SkillResult::success("No changes to review. Stage some changes first with `git add`."),
                }
            }
            Err(e) => return SkillResult::error(format!("Failed to get diff: {}", e)),
        };

        let response = format!(
            r#"I'll review the following changes:

```diff
{}
```

## Review Checklist
I'll check for:
- [ ] Code correctness and logic errors
- [ ] Security issues (injection, secrets, etc.)
- [ ] Performance concerns
- [ ] Code style and best practices
- [ ] Missing error handling
- [ ] Test coverage considerations

Analyzing..."#,
            if staged_diff.len() > 10000 {
                format!("{}...\n[truncated - {} more characters]", &staged_diff[..10000], staged_diff.len() - 10000)
            } else {
                staged_diff
            }
        );

        SkillResult::success(response)
    }

    fn prompt_template(&self) -> &str {
        r#"You are a code reviewer. Analyze the diff and provide:
1. Summary of changes
2. Issues found (bugs, security, performance)
3. Suggestions for improvement
4. Overall assessment (approve, request changes, or comment)

Be specific and reference line numbers when possible."#
    }
}
