# Bash Tool Description

Executes a given bash command in a persistent shell session with optional timeout, ensuring proper handling and security measures.

IMPORTANT: This tool is for terminal operations like git, npm, docker, etc. DO NOT use it for file operations (reading, writing, editing, searching, finding files) - use the specialized tools for this instead.

## Before Executing Commands

1. **Directory Verification:**
   - If the command will create new directories or files, first use `ls` to verify the parent directory exists and is the correct location
   - For example, before running "mkdir foo/bar", first use `ls foo` to check that "foo" exists

2. **Command Execution:**
   - Always quote file paths that contain spaces with double quotes (e.g., `cd "path with spaces/file.txt"`)
   - Examples of proper quoting:
     - `cd "/Users/name/My Documents"` (correct)
     - `cd /Users/name/My Documents` (incorrect - will fail)
     - `python "/path/with spaces/script.py"` (correct)
     - `python /path/with spaces/script.py` (incorrect - will fail)

## Usage Notes

- The command argument is required
- You can specify an optional timeout in milliseconds (up to 600000ms / 10 minutes). Default: 120000ms (2 minutes)
- Write a clear, concise description of what this command does
- If output exceeds 30000 characters, it will be truncated
- You can use `run_in_background` parameter to run the command in the background
- Avoid using Bash for `find`, `grep`, `cat`, `head`, `tail`, `sed`, `awk`, or `echo` commands - use dedicated tools instead:
  - File search: Use Glob (NOT find or ls)
  - Content search: Use Grep (NOT grep or rg)
  - Read files: Use Read (NOT cat/head/tail)
  - Edit files: Use Edit (NOT sed/awk)
  - Write files: Use Write (NOT echo >/cat <<EOF)

## When Issuing Multiple Commands

- If commands are independent and can run in parallel, make multiple Bash tool calls in a single message
- If commands depend on each other and must run sequentially, use a single Bash call with '&&' to chain them
- Use ';' only when you need to run commands sequentially but don't care if earlier commands fail
- DO NOT use newlines to separate commands

## Git Safety Protocol

- NEVER update the git config
- NEVER run destructive/irreversible git commands (like push --force, hard reset) unless explicitly requested
- NEVER skip hooks (--no-verify, --no-gpg-sign) unless explicitly requested
- NEVER force push to main/master, warn if requested
- CRITICAL: ALWAYS create NEW commits. NEVER use git commit --amend unless explicitly requested
- NEVER commit changes unless explicitly asked

## Creating Git Commits

When the user asks you to create a new git commit:

1. Run these bash commands in parallel:
   - `git status` to see all untracked files (never use -uall flag)
   - `git diff` to see staged and unstaged changes
   - `git log --oneline -5` to see recent commit messages for style reference

2. Analyze changes and draft a commit message:
   - Summarize the nature of changes (new feature, enhancement, bug fix, refactoring, etc.)
   - Do not commit files that likely contain secrets (.env, credentials.json, etc.)
   - Draft a concise (1-2 sentences) commit message focusing on the "why"

3. Run these commands:
   - Add relevant untracked files to staging
   - Create the commit with message ending with: `Co-Authored-By: ${ASSISTANT_NAME} <noreply@example.com>`
   - Run `git status` after to verify success

4. If commit fails due to pre-commit hook: fix the issue and create a NEW commit

Always pass commit messages via HEREDOC:
```bash
git commit -m "$(cat <<'EOF'
Commit message here.

Co-Authored-By: Assistant <noreply@example.com>
EOF
)"
```

## Creating Pull Requests

Use the gh command for all GitHub-related tasks.

When creating a pull request:

1. Run these bash commands in parallel:
   - `git status` (never use -uall flag)
   - `git diff` to see changes
   - Check if current branch tracks a remote
   - `git log` and `git diff [base-branch]...HEAD` for full commit history

2. Analyze all changes for the PR, looking at ALL commits

3. Run these commands in parallel:
   - Create new branch if needed
   - Push to remote with -u flag if needed
   - Create PR using `gh pr create` with HEREDOC for body:

```bash
gh pr create --title "the pr title" --body "$(cat <<'EOF'
## Summary
<1-3 bullet points>

## Test plan
[Bulleted markdown checklist of TODOs for testing...]
EOF
)"
```

Return the PR URL when done.
