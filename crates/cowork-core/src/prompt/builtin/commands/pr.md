---
name: pr
description: "Create a pull request on GitHub"
allowed_tools: Bash, Read, Glob, Grep
denied_tools: Write, Edit, Task
argument_hint:
  - "--draft"
  - "--title <title>"
  - "--base <branch>"
---

# Create Pull Request Command

Create a GitHub pull request for the current branch. Follow these steps:

## Step 1: Gather Information

Run the following commands to understand the current state:

1. `git status` - Check for uncommitted changes (never use -uall flag)
2. `git branch --show-current` - Get current branch name
3. `git log origin/main..HEAD --oneline` or `git log origin/master..HEAD --oneline` - See commits to be included
4. `git diff origin/main...HEAD` or `git diff origin/master...HEAD` - See all changes

## Step 2: Analyze Changes

Review all commits and changes that will be included in the PR:
- Identify the main purpose of the PR
- Note any breaking changes
- List key changes in bullet points

## Step 3: Push Branch

If the branch hasn't been pushed or has new commits:

```bash
git push -u origin <branch-name>
```

## Step 4: Create Pull Request

Use the GitHub CLI to create the PR:

```bash
gh pr create --title "<title>" --body "$(cat <<'EOF'
## Summary
<1-3 bullet points summarizing the changes>

## Changes
<List of key changes>

## Test plan
<How to test these changes>

---
Generated with Cowork
EOF
)"
```

## User Arguments

$ARGUMENTS

Handle these arguments:
- `--draft` - Create as draft PR
- `--title <title>` - Use specified title
- `--base <branch>` - Target a specific base branch

## Important Notes

- NEVER force push
- Ensure all commits are included in the PR description
- If there are uncommitted changes, ask user to commit first
- Return the PR URL when complete
