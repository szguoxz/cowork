---
name: commit
description: "Create a git commit with AI-generated message"
allowed_tools: Bash, Read, Glob, Grep
denied_tools: Write, Edit, Task
argument_hint:
  - "-m <message>"
  - "--amend"
---

# Git Commit Command

Create a git commit following the project's conventions. Follow these steps:

## Step 1: Gather Information

Run the following git commands to understand the current state:

1. `git status` - See all untracked and modified files (never use -uall flag)
2. `git diff --staged` - See staged changes
3. `git diff` - See unstaged changes
4. `git log --oneline -10` - See recent commit message style

## Step 2: Analyze Changes

- Summarize the nature of the changes (new feature, bug fix, refactoring, test, docs, etc.)
- Identify the main purpose and impact of the changes
- Check for any files that should NOT be committed (.env, credentials, etc.)

## Step 3: Create Commit

Draft a concise commit message that:
- Focuses on the "why" rather than the "what"
- Follows the repository's existing commit message style
- Is 1-2 sentences maximum
- Uses appropriate prefix if the repo uses conventional commits

Use a HEREDOC for the commit message to ensure proper formatting:

```bash
git commit -m "$(cat <<'EOF'
Your commit message here.

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

## User Arguments

$ARGUMENTS

If the user provided `-m <message>`, use their message as the commit message.
If the user provided `--amend`, use `git commit --amend` instead.

## Important Notes

- NEVER update git config
- NEVER use --no-verify or skip hooks unless explicitly requested
- NEVER use git commands with -i flag (interactive mode not supported)
- If there are no changes to commit, inform the user
- Do NOT push to remote unless explicitly asked
