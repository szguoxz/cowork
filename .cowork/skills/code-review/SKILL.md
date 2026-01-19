---
name: code-review
description: Comprehensive code review for a pull request, checking for bugs, CLAUDE.md compliance, and code quality with high signal-to-noise ratio
usage: /code-review [PR number or branch]
allowed-tools: Bash, Glob, Grep, Read, TodoWrite
user-invocable: true
---

# Pull Request Code Review

Provide a thorough code review for the given pull request.

## Target

$ARGUMENTS

## Review Process

### Step 1: Gather Context

First, understand what's being reviewed:
- Run `git status` to see current state
- Run `git diff` or `git diff [branch]` to see changes
- Check for any CLAUDE.md files in the project root and in directories with changed files

### Step 2: Review Criteria

Focus on **HIGH SIGNAL issues only**. Flag issues where:
- The code will fail to compile or parse (syntax errors, type errors, missing imports, unresolved references)
- The code will definitely produce wrong results regardless of inputs (clear logic errors)
- Clear, unambiguous CLAUDE.md violations where you can quote the exact rule being broken
- Security vulnerabilities (injection, XSS, authentication issues)
- Memory leaks or resource handling issues

**Do NOT flag:**
- Code style or quality concerns (unless explicitly in CLAUDE.md)
- Potential issues that depend on specific inputs or state
- Subjective suggestions or improvements
- Pre-existing issues
- Issues that a linter will catch

### Step 3: Review Categories

Review the changes for:

**1. CLAUDE.md Compliance**
- Check all CLAUDE.md files that share a file path with the changes
- Quote the exact rule being violated if flagging

**2. Bug Detection**
- Look for obvious bugs in the introduced code
- Focus only on the diff itself without reading excessive context
- Flag only significant bugs; ignore nitpicks and likely false positives

**3. Security Issues**
- Check for security vulnerabilities in the changed code
- Look for injection, XSS, auth bypass, etc.

### Step 4: Validate Findings

For each potential issue:
- Verify it's actually a problem, not a false positive
- Check if it's a pre-existing issue vs. newly introduced
- Confirm the issue will actually be hit in practice

### Step 5: Report Results

If issues were found, provide for each:
- Brief description of the issue
- File and line number
- Why it's a problem
- Suggested fix

**Format:**
```
## Code Review Results

### Critical Issues
1. **[File:Line]** Description
   - Why: explanation
   - Fix: suggestion

### Important Issues
1. **[File:Line]** Description
   - Why: explanation
   - Fix: suggestion

### Summary
[Brief overall assessment]
```

If NO issues found:
```
## Code Review

No issues found. Checked for bugs and CLAUDE.md compliance.

Changes reviewed:
- [list of files]
```

## False Positive List

Do NOT flag these (common false positives):
- Pre-existing issues
- Something that appears to be a bug but is actually correct
- Pedantic nitpicks that a senior engineer would not flag
- Issues that a linter will catch
- General code quality concerns unless explicitly required in CLAUDE.md
- Issues mentioned in CLAUDE.md but explicitly silenced in the code (e.g., via a lint ignore comment)
