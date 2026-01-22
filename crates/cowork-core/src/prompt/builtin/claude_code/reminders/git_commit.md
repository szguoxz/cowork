# Git Commit Guidelines

When creating a git commit, follow these steps:

## 1. Gather Information (in parallel)

```bash
git status                    # See untracked files (never use -uall)
git diff                      # See changes to be committed
git log --oneline -5          # See recent commit style
```

## 2. Analyze and Draft Message

- Summarize the nature of changes (new feature, enhancement, bug fix, refactoring, etc.)
- Focus on the "why" rather than the "what"
- Keep it concise (1-2 sentences)
- Match the repository's commit style

## 3. Create the Commit

```bash
git add <files>
git commit -m "$(cat <<'EOF'
Your commit message here.

Co-Authored-By: ${ASSISTANT_NAME} <noreply@anthropic.com>
EOF
)"
git status  # Verify success
```

## Safety Rules

- NEVER use git commit --amend unless explicitly requested
- NEVER use -uall flag
- NEVER commit files with secrets (.env, credentials, etc.)
- NEVER use --no-verify unless requested
- NEVER commit unless explicitly asked

## If Commit Fails

If the commit fails due to pre-commit hooks:
1. Read the error message
2. Fix the issue
3. Create a NEW commit (don't amend)
