//! Built-in skills embedded in the binary
//!
//! All skills are SKILL.md-format strings parsed as DynamicSkill.
//! This matches the Anthropic skill standard: prompt templates with
//! command substitution (`!`command``) for gathering context.

use std::path::PathBuf;
use std::sync::Arc;

use super::loader::{DynamicSkill, SkillSource};
use super::Skill;

// =============================================================================
// Git Action Skills (matching Claude Code's commit-commands plugin)
// =============================================================================

const COMMIT_SKILL: &str = r#"---
name: commit
description: Create a git commit
allowed-tools: Bash(git add:*), Bash(git status:*), Bash(git commit:*)
---

## Context

- Current git status: !`git status`
- Current git diff (staged and unstaged changes): !`git diff HEAD`
- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -10`

## Your task

Based on the above changes, create a single git commit.

You have the capability to call multiple tools in a single response. Stage and create the commit using a single message. Do not use any other tools or do anything else. Do not send any other text or messages besides these tool calls.
"#;

const COMMIT_PUSH_PR_SKILL: &str = r#"---
name: commit-push-pr
description: Commit, push, and open a PR
allowed-tools: Bash(git checkout:*), Bash(git add:*), Bash(git status:*), Bash(git push:*), Bash(git commit:*), Bash(gh pr create:*)
---

## Context

- Current git status: !`git status`
- Current git diff (staged and unstaged changes): !`git diff HEAD`
- Current branch: !`git branch --show-current`

## Your task

Based on the above changes:

1. Create a new branch if on main
2. Create a single commit with an appropriate message
3. Push the branch to origin
4. Create a pull request using `gh pr create`
5. You have the capability to call multiple tools in a single response. You MUST do all of the above in a single message. Do not use any other tools or do anything else. Do not send any other text or messages besides these tool calls.
"#;

const CLEAN_GONE_SKILL: &str = r#"---
name: clean-gone
description: Clean up local branches deleted from remote
allowed-tools: Bash(git fetch:*), Bash(git branch:*), Bash(git worktree:*)
---

## Context

- Remote tracking status: !`git fetch --prune 2>&1 && git branch -vv`

## Your task

Clean up all local branches marked as [gone] (branches deleted on remote but still exist locally). Also remove any associated worktrees. Do not delete the current branch.
"#;

const PUSH_SKILL: &str = r#"---
name: push
description: Push commits to remote
allowed-tools: Bash(git push:*), Bash(git status:*), Bash(git log:*)
---

## Context

- Current branch: !`git branch --show-current`
- Unpushed commits: !`git log @{u}..HEAD --oneline 2>/dev/null || echo "No upstream set"`
- Remote status: !`git status -sb`

## Your task

Push the current branch to the remote. If no upstream is set, use `git push -u origin <branch>`.
"#;

const PR_SKILL: &str = r#"---
name: pr
description: Create a pull request with auto-generated description
allowed-tools: Bash(git log:*), Bash(git diff:*), Bash(gh pr create:*), Bash(git push:*)
---

## Context

- Current branch: !`git branch --show-current`
- Commits not on main: !`git log main..HEAD --oneline 2>/dev/null || git log master..HEAD --oneline 2>/dev/null || echo "Could not determine commits"`
- Changes summary: !`git diff main..HEAD --stat 2>/dev/null || git diff master..HEAD --stat 2>/dev/null || echo "Could not diff against main"`

## Your task

Create a pull request for the current branch. $ARGUMENTS

1. Push the branch if not already pushed
2. Create a PR with `gh pr create` using an appropriate title and description based on the commits
3. Return the PR URL
"#;

const REVIEW_SKILL: &str = r#"---
name: review
description: Review staged changes and provide feedback
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*)
---

## Context

- Current changes: !`git diff --cached 2>/dev/null || git diff`
- Files changed: !`git diff --cached --name-only 2>/dev/null || git diff --name-only`

## Your task

Review the changes shown above. Provide feedback on:
- Potential bugs or logic errors
- Code quality issues
- Missing error handling
- Security concerns

Be concise. Only flag issues with high confidence.
"#;

// =============================================================================
// Dev Workflow Skills
// =============================================================================

const TEST_SKILL: &str = r#"---
name: test
description: Run project tests (auto-detects framework)
allowed-tools: Bash
---

## Context

- Project files: !`ls Cargo.toml package.json Makefile pyproject.toml setup.py go.mod 2>/dev/null || echo "No recognized project file"`

## Your task

Run the project tests. Auto-detect the test framework:
- Rust: `cargo test`
- Node.js: `npm test` or `yarn test`
- Python: `pytest` or `python -m pytest`
- Go: `go test ./...`
- Make: `make test`

$ARGUMENTS

Report the results concisely.
"#;

const BUILD_SKILL: &str = r#"---
name: build
description: Build the project (auto-detects build system)
allowed-tools: Bash
---

## Context

- Project files: !`ls Cargo.toml package.json Makefile pyproject.toml go.mod CMakeLists.txt 2>/dev/null || echo "No recognized project file"`

## Your task

Build the project. Auto-detect the build system:
- Rust: `cargo build`
- Node.js: `npm run build` or `yarn build`
- Go: `go build ./...`
- Make: `make`
- CMake: `cmake --build build`

$ARGUMENTS

Report the results concisely.
"#;

const LINT_SKILL: &str = r#"---
name: lint
description: "Run linter (auto-detects: clippy, eslint, ruff, etc.)"
allowed-tools: Bash
---

## Context

- Project files: !`ls Cargo.toml package.json .eslintrc* pyproject.toml setup.cfg .flake8 2>/dev/null || echo "No recognized config"`

## Your task

Run the appropriate linter:
- Rust: `cargo clippy`
- Node.js: `npx eslint .` or configured lint script
- Python: `ruff check .` or `flake8`

$ARGUMENTS

Report issues found.
"#;

const FORMAT_SKILL: &str = r#"---
name: format
description: "Format code (auto-detects: rustfmt, prettier, black, etc.)"
allowed-tools: Bash
---

## Context

- Project files: !`ls Cargo.toml package.json .prettierrc* pyproject.toml 2>/dev/null || echo "No recognized config"`

## Your task

Format the code:
- Rust: `cargo fmt`
- Node.js: `npx prettier --write .`
- Python: `black .` or `ruff format .`

$ARGUMENTS
"#;

// =============================================================================
// Feature Development & Code Analysis Skills (existing)
// =============================================================================

const FEATURE_DEV_SKILL: &str = r#"---
name: feature-dev
description: Guided feature development with codebase understanding and architecture focus
usage: /feature-dev [feature description]
allowed-tools: Read, Write, Edit, Bash, Glob, Grep, TodoWrite
user-invocable: true
---

# Feature Development

You are helping a developer implement a new feature. Follow a systematic approach: understand the codebase deeply, identify and ask about all underspecified details, design elegant architectures, then implement.

## Core Principles

- **Ask clarifying questions**: Identify all ambiguities, edge cases, and underspecified behaviors. Ask specific, concrete questions rather than making assumptions. Wait for user answers before proceeding with implementation. Ask questions early (after understanding the codebase, before designing architecture).
- **Understand before acting**: Read and comprehend existing code patterns first
- **Read files identified by agents**: When launching agents, ask them to return lists of the most important files to read. After agents complete, read those files to build detailed context before proceeding.
- **Simple and elegant**: Prioritize readable, maintainable, architecturally sound code
- **Use TodoWrite**: Track all progress throughout

---

## Phase 1: Discovery

**Goal**: Understand what needs to be built

Initial request: $ARGUMENTS

**Actions**:
1. Create todo list with all phases
2. If feature unclear, ask user for:
   - What problem are they solving?
   - What should the feature do?
   - Any constraints or requirements?
3. Summarize understanding and confirm with user

---

## Phase 2: Codebase Exploration

**Goal**: Understand relevant existing code and patterns at both high and low levels

**Actions**:
1. Use Read, Glob, and Grep tools to explore:
   - Find features similar to the requested feature
   - Map the architecture and abstractions for the relevant area
   - Analyze the current implementation of related features
   - Identify UI patterns, testing approaches, or extension points

2. Build a list of 5-10 key files to understand
3. Read all identified files to build deep understanding
4. Present comprehensive summary of findings and patterns discovered

---

## Phase 3: Clarifying Questions

**Goal**: Fill in gaps and resolve all ambiguities before designing

**CRITICAL**: This is one of the most important phases. DO NOT SKIP.

**Actions**:
1. Review the codebase findings and original feature request
2. Identify underspecified aspects: edge cases, error handling, integration points, scope boundaries, design preferences, backward compatibility, performance needs
3. **Present all questions to the user in a clear, organized list**
4. **Wait for answers before proceeding to architecture design**

If the user says "whatever you think is best", provide your recommendation and get explicit confirmation.

---

## Phase 4: Architecture Design

**Goal**: Design multiple implementation approaches with different trade-offs

**Actions**:
1. Design 2-3 approaches with different focuses:
   - **Minimal changes**: Smallest change, maximum reuse
   - **Clean architecture**: Maintainability, elegant abstractions
   - **Pragmatic balance**: Speed + quality
2. Review all approaches and form your opinion on which fits best for this specific task (consider: small fix vs large feature, urgency, complexity, team context)
3. Present to user: brief summary of each approach, trade-offs comparison, **your recommendation with reasoning**, concrete implementation differences
4. **Ask user which approach they prefer**

---

## Phase 5: Implementation

**Goal**: Build the feature

**DO NOT START WITHOUT USER APPROVAL**

**Actions**:
1. Wait for explicit user approval
2. Read all relevant files identified in previous phases
3. Implement following chosen architecture
4. Follow codebase conventions strictly
5. Write clean, well-documented code
6. Update todos as you progress

---

## Phase 6: Quality Review

**Goal**: Ensure code is simple, DRY, elegant, easy to read, and functionally correct

**Actions**:
1. Review the implementation for:
   - Simplicity/DRY/elegance
   - Bugs/functional correctness
   - Project conventions/abstractions
2. Consolidate findings and identify highest severity issues that you recommend fixing
3. **Present findings to user and ask what they want to do** (fix now, fix later, or proceed as-is)
4. Address issues based on user decision

---

## Phase 7: Summary

**Goal**: Document what was accomplished

**Actions**:
1. Mark all todos complete
2. Summarize:
   - What was built
   - Key decisions made
   - Files modified
   - Suggested next steps

---
"#;

const CODE_EXPLORER_SKILL: &str = r#"---
name: code-explorer
description: Deeply analyzes existing codebase features by tracing execution paths, mapping architecture layers, understanding patterns and abstractions, and documenting dependencies to inform new development
usage: /code-explorer [feature or area to explore]
allowed-tools: Glob, Grep, Read, TodoWrite
user-invocable: true
---

# Code Explorer

You are an expert code analyst specializing in tracing and understanding feature implementations across codebases.

## What to Explore

$ARGUMENTS

## Analysis Approach

**1. Feature Discovery**
- Find entry points (APIs, UI components, CLI commands)
- Locate core implementation files
- Map feature boundaries and configuration

**2. Code Flow Tracing**
- Follow call chains from entry to output
- Trace data transformations at each step
- Identify all dependencies and integrations
- Document state changes and side effects

**3. Architecture Analysis**
- Map abstraction layers (presentation -> business logic -> data)
- Identify design patterns and architectural decisions
- Document interfaces between components

## Output Format

Provide a comprehensive analysis including:
- **Entry points** with file:line references
- **Step-by-step execution flow**
- **Key components** and their responsibilities
- **Architecture insights**: patterns, layers, design decisions
- **Essential files list**: 5-10 files essential to understand the topic
"#;

const CODE_ARCHITECT_SKILL: &str = r#"---
name: code-architect
description: Designs feature architectures by analyzing existing codebase patterns and conventions, then providing comprehensive implementation blueprints
usage: /code-architect [feature to design]
allowed-tools: Glob, Grep, Read, TodoWrite
user-invocable: true
---

# Code Architect

You are a senior software architect who delivers comprehensive, actionable architecture blueprints.

## Feature to Design

$ARGUMENTS

## Core Process

**1. Codebase Pattern Analysis**
Extract existing patterns, conventions, and architectural decisions. Find similar features to understand established approaches.

**2. Architecture Design**
Design the complete feature architecture. Make decisive choices - pick one approach and commit. Ensure seamless integration with existing code.

**3. Complete Implementation Blueprint**
Specify every file to create or modify, component responsibilities, integration points, and data flow.

## Output Format

### Patterns & Conventions Found
- Existing patterns with file:line references

### Architecture Decision
- Chosen approach with rationale

### Implementation Map
- New files to create
- Existing files to modify
- Build sequence (phased implementation steps)
"#;

const CODE_REVIEWER_SKILL: &str = r#"---
name: code-reviewer
description: Reviews code for bugs, logic errors, security vulnerabilities, and adherence to project conventions with confidence-based filtering
usage: /code-reviewer [files or scope to review]
allowed-tools: Glob, Grep, Read, Bash, TodoWrite
user-invocable: true
---

# Code Reviewer

Review code with high precision to minimize false positives.

## Review Scope

$ARGUMENTS

By default, review unstaged changes from `git diff`.

## Confidence Scoring

Rate each issue 0-100. **Only report issues with confidence >= 80.**

- **75+**: Verified real issue that will be hit in practice
- **100**: Definitely a real issue, evidence confirms

## Output Format

For each high-confidence issue:
- File path and line number
- Clear description with confidence score
- Concrete fix suggestion

Group by severity: Critical (must fix) vs Important (should fix).
"#;

const CODE_REVIEW_SKILL: &str = r#"---
name: code-review
description: Comprehensive code review for a pull request, checking for bugs and CLAUDE.md compliance
usage: /code-review [PR number or branch]
allowed-tools: Bash, Glob, Grep, Read, TodoWrite
user-invocable: true
---

# Pull Request Code Review

## Target

$ARGUMENTS

## Process

1. Run `git diff` to see changes
2. Check for CLAUDE.md files in affected directories
3. Review for: compile errors, logic bugs, CLAUDE.md violations, security issues
4. Do NOT flag: style concerns, potential issues, pre-existing issues, linter catches

## Output

For each issue: file:line, description, why it matters, suggested fix.
If no issues: confirm code reviewed and list files checked.
"#;

const PR_TEST_ANALYZER_SKILL: &str = r#"---
name: pr-test-analyzer
description: Analyzes test coverage quality, looking for critical gaps in behavioral coverage
usage: /pr-test-analyzer [files or PR to analyze]
allowed-tools: Glob, Grep, Read, Bash, TodoWrite
user-invocable: true
---

# Test Coverage Analyzer

## Target

$ARGUMENTS

By default, analyze tests for recently changed files.

## Focus

- Behavioral vs line coverage - do tests verify behavior?
- Critical gap detection - untested error paths and edge cases
- Test quality - meaningful assertions vs "does not throw"

## Gap Severity (only report >= 6)

- **7-9**: Important to test, significant risk
- **10**: Critical, must add tests

## Output

For each gap: feature/function, severity, what's missing, suggested test.
"#;

const SILENT_FAILURE_HUNTER_SKILL: &str = r#"---
name: silent-failure-hunter
description: Finds silent failures, inadequate error handling, and catch blocks that swallow errors
usage: /silent-failure-hunter [files or scope]
allowed-tools: Glob, Grep, Read, TodoWrite
user-invocable: true
---

# Silent Failure Hunter

Find error handling issues that cause silent failures.

## Target

$ARGUMENTS

## What to Look For

1. Empty catch blocks / ignored Results
2. Catch-and-continue (log but don't handle)
3. Incorrect fallback values (unwrap_or_default where default is wrong)
4. Missing error propagation (let _ = ...)
5. Async fire-and-forget (spawned tasks with ignored errors)

## Severity

- **Critical**: Causes data loss or corruption
- **High**: Leads to undefined behavior
- **Medium**: Incorrect fallback

## Output

For each issue: file:line, pattern found, risk, fix suggestion.
"#;

const CODE_SIMPLIFIER_SKILL: &str = r#"---
name: code-simplifier
description: Simplifies and refines code for clarity, consistency, and maintainability while preserving functionality
usage: /code-simplifier [files or scope]
allowed-tools: Read, Write, Edit, Glob, Grep, TodoWrite
user-invocable: true
---

# Code Simplifier

## Target

$ARGUMENTS

By default, focus on recently modified code.

## Principles

1. Clarity over cleverness
2. Remove unnecessary complexity (flatten nesting, extract conditions)
3. Match existing codebase patterns
4. DRY - but don't over-abstract
5. Remove dead code

## Process

1. Read target files
2. Apply safe simplifications directly
3. List suggestions needing approval
4. Verify no functionality changed
"#;

// =============================================================================
// All Built-in Skills
// =============================================================================

/// All built-in skill definitions
const BUILTIN_SKILLS: &[&str] = &[
    // Git actions (Claude Code commit-commands style)
    COMMIT_SKILL,
    COMMIT_PUSH_PR_SKILL,
    CLEAN_GONE_SKILL,
    PUSH_SKILL,
    PR_SKILL,
    REVIEW_SKILL,
    // Dev workflow
    TEST_SKILL,
    BUILD_SKILL,
    LINT_SKILL,
    FORMAT_SKILL,
    // Feature development & analysis
    FEATURE_DEV_SKILL,
    CODE_EXPLORER_SKILL,
    CODE_ARCHITECT_SKILL,
    CODE_REVIEWER_SKILL,
    CODE_REVIEW_SKILL,
    PR_TEST_ANALYZER_SKILL,
    SILENT_FAILURE_HUNTER_SKILL,
    CODE_SIMPLIFIER_SKILL,
];

/// Load all built-in skills
pub fn load_builtin_skills() -> Vec<Arc<dyn Skill>> {
    BUILTIN_SKILLS
        .iter()
        .filter_map(|content| {
            match DynamicSkill::parse(content, PathBuf::from("<builtin>"), SkillSource::User) {
                Ok(skill) => Some(Arc::new(skill) as Arc<dyn Skill>),
                Err(e) => {
                    tracing::warn!("Failed to load built-in skill: {}", e);
                    None
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_all_builtin_skills() {
        let skills = load_builtin_skills();
        assert_eq!(skills.len(), BUILTIN_SKILLS.len(), "All built-in skills should parse successfully");

        let names: Vec<_> = skills.iter().map(|s| s.info().name.clone()).collect();
        // Git actions
        assert!(names.contains(&"commit".to_string()));
        assert!(names.contains(&"commit-push-pr".to_string()));
        assert!(names.contains(&"clean-gone".to_string()));
        assert!(names.contains(&"push".to_string()));
        assert!(names.contains(&"pr".to_string()));
        assert!(names.contains(&"review".to_string()));
        // Dev workflow
        assert!(names.contains(&"test".to_string()));
        assert!(names.contains(&"build".to_string()));
        assert!(names.contains(&"lint".to_string()));
        assert!(names.contains(&"format".to_string()));
        // Analysis
        assert!(names.contains(&"feature-dev".to_string()));
        assert!(names.contains(&"code-explorer".to_string()));
    }

    #[test]
    fn test_builtin_skill_info() {
        let skills = load_builtin_skills();

        for skill in &skills {
            let info = skill.info();
            assert!(!info.name.is_empty());
            assert!(!info.description.is_empty());
        }
    }

    #[test]
    fn test_commit_skill_has_substitutions() {
        let skill =
            DynamicSkill::parse(COMMIT_SKILL, PathBuf::from("<builtin>"), SkillSource::User)
                .expect("Failed to parse commit skill");

        assert_eq!(skill.frontmatter.name, "commit");
        // Should contain command substitution markers
        assert!(skill.body.contains("!`git status`"));
        assert!(skill.body.contains("!`git diff HEAD`"));
    }
}
