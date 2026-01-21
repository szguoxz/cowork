---
name: review-pr
description: "Review a pull request on GitHub"
allowed_tools: Bash, Read, Glob, Grep, WebFetch
denied_tools: Write, Edit, Task
argument_hint:
  - "<pr-number>"
  - "<pr-url>"
---

# Review Pull Request Command

Review a GitHub pull request and provide feedback. Follow these steps:

## Step 1: Get PR Information

Use the GitHub CLI to fetch PR details:

```bash
# Get PR details
gh pr view <pr-number-or-url> --json title,body,author,baseRefName,headRefName,additions,deletions,changedFiles

# Get the diff
gh pr diff <pr-number-or-url>

# Get PR comments
gh api repos/{owner}/{repo}/pulls/{pr-number}/comments
```

## Step 2: Analyze the Changes

Review the diff carefully:
- Check for potential bugs or logic errors
- Look for security vulnerabilities
- Verify code style consistency
- Check for missing error handling
- Look for performance issues
- Verify tests are included if appropriate

## Step 3: Provide Review

Structure your review as follows:

### Summary
Brief overview of what the PR does and overall assessment.

### Strengths
What's done well in this PR.

### Issues Found
List any problems found, categorized by severity:
- **Critical**: Must fix before merge
- **Major**: Should fix, but not blocking
- **Minor**: Nice to have improvements
- **Nitpick**: Style/preference suggestions

### Questions
Any clarifying questions for the author.

### Recommendation
One of: APPROVE, REQUEST_CHANGES, or COMMENT

## User Arguments

$ARGUMENTS

The argument should be either:
- A PR number (e.g., `123`)
- A PR URL (e.g., `https://github.com/owner/repo/pull/123`)

## Important Notes

- Be constructive and respectful in feedback
- Focus on the code, not the person
- Explain the "why" behind suggestions
- Acknowledge good work when you see it
- If the PR is large, focus on the most critical areas first
