# Prompt System Implementation Guide

## Table of Contents

1. [Overview](#overview)
2. [Built-in Prompts](#built-in-prompts)
3. [Data Structures](#data-structures)
4. [File Formats](#file-formats)
5. [Component Implementations](#component-implementations)
6. [Prompt Assembly Pipeline](#prompt-assembly-pipeline)
7. [Hook System](#hook-system)
8. [Agent System](#agent-system)
9. [Skill System](#skill-system)
10. [Command System](#command-system)
11. [Plugin System](#plugin-system)
12. [Configuration Management](#configuration-management)
13. [Extension Points](#extension-points)

---

## Overview

This document provides implementation details for building a modular prompt system similar to Claude Code. The system enables dynamic composition of system prompts through hooks, agents, skills, and commands.

**IMPORTANT**: Cowork uses the same literal prompt text as Claude Code to ensure consistent agent behavior. The built-in prompts are located in `crates/cowork-core/src/prompt/builtin/`.

---

## Built-in Prompts

Cowork includes built-in prompt files that mirror Claude Code's prompt system. These are compiled into the binary using `include_str!()`.

### Directory Structure

```
crates/cowork-core/src/prompt/
├── mod.rs                      # Main module with TemplateVars
└── builtin/
    ├── mod.rs                  # Builtin module exports
    ├── system_prompt.md        # Main system prompt
    ├── agents/
    │   ├── explore.md          # Explore agent (fast codebase search)
    │   ├── plan.md             # Plan agent (implementation planning)
    │   ├── bash.md             # Bash agent (command execution)
    │   └── general.md          # General-purpose agent
    ├── tools/
    │   ├── task.md             # Task tool description
    │   ├── bash.md             # Bash tool description
    │   ├── read.md             # Read tool description
    │   ├── write.md            # Write tool description
    │   ├── edit.md             # Edit tool description
    │   ├── glob.md             # Glob tool description
    │   ├── grep.md             # Grep tool description
    │   ├── todo_write.md       # TodoWrite tool description
    │   ├── ask_user_question.md
    │   ├── web_fetch.md
    │   ├── web_search.md
    │   ├── lsp.md
    │   ├── enter_plan_mode.md
    │   └── exit_plan_mode.md
    └── reminders/
        ├── plan_mode_active.md
        ├── security_policy.md
        └── conversation_summarization.md
```

### Usage

```rust
use cowork_core::prompt::builtin;
use cowork_core::prompt::TemplateVars;

// Load the main system prompt
let template = builtin::SYSTEM_PROMPT;

// Create template variables
let vars = TemplateVars {
    working_directory: "/home/user/project".to_string(),
    is_git_repo: true,
    ..Default::default()
};

// Substitute variables
let system_prompt = vars.substitute(template);

// Load agent prompts
let explore_agent = builtin::agents::EXPLORE;
let plan_agent = builtin::agents::PLAN;

// Load tool descriptions
let bash_tool = builtin::tools::BASH;
let task_tool = builtin::tools::TASK;

// Load reminders
let security_policy = builtin::reminders::SECURITY_POLICY;
```

### Template Variables

The prompts support these template variables:

| Variable | Description |
|----------|-------------|
| `${WORKING_DIRECTORY}` | Current working directory |
| `${IS_GIT_REPO}` | "Yes" or "No" |
| `${PLATFORM}` | Operating system (linux, macos, windows) |
| `${OS_VERSION}` | OS version string |
| `${CURRENT_DATE}` | Today's date (YYYY-MM-DD) |
| `${CURRENT_YEAR}` | Current year (YYYY) |
| `${MODEL_INFO}` | Model identification |
| `${GIT_STATUS}` | Git status output |
| `${ASSISTANT_NAME}` | Name of the assistant ("Cowork") |
| `${SECURITY_POLICY}` | Security policy content |
| `${PLAN_FILE_PATH}` | Path to the plan file (in plan mode) |

### Goals

- **Extensibility**: Users can add custom prompts without modifying core code
- **Composability**: Components can be combined in flexible ways
- **Security**: Principle of least privilege for tool access
- **Discoverability**: Auto-loading from well-known paths

---

## Data Structures

### Core Types

```rust
// src/prompt/types.rs

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Unique identifier for prompt components
pub type ComponentId = String;

/// Model selection for agents
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelPreference {
    #[default]
    Inherit,  // Use parent's model
    Opus,
    Sonnet,
    Haiku,
    #[serde(untagged)]
    Custom(String),
}

/// Tool restriction specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolSpec {
    /// Allow all uses of a tool
    Name(String),
    /// Allow specific patterns: "Bash(git:*)" or "Write(src/*:*)"
    Pattern(String),
}

impl ToolSpec {
    /// Check if a tool call matches this specification
    pub fn matches(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        match self {
            ToolSpec::Name(name) => name == tool_name,
            ToolSpec::Pattern(pattern) => {
                // Parse pattern like "Bash(git:*)" or "Write(src/*:*)"
                Self::match_pattern(pattern, tool_name, args)
            }
        }
    }

    fn match_pattern(pattern: &str, tool_name: &str, args: &serde_json::Value) -> bool {
        // Pattern format: "ToolName(arg_pattern:value_pattern)"
        if let Some(paren_idx) = pattern.find('(') {
            let name = &pattern[..paren_idx];
            if name != tool_name {
                return false;
            }

            let inner = &pattern[paren_idx + 1..pattern.len() - 1];
            // Parse arg:value patterns and match against args
            Self::match_args(inner, args)
        } else {
            pattern == tool_name
        }
    }

    fn match_args(pattern: &str, args: &serde_json::Value) -> bool {
        // Implement glob-style matching for arguments
        // "git:*" matches command starting with "git"
        // "*:*" matches anything
        if pattern == "*:*" {
            return true;
        }

        for part in pattern.split(',') {
            let parts: Vec<&str> = part.trim().split(':').collect();
            if parts.len() == 2 {
                let arg_name = parts[0];
                let value_pattern = parts[1];

                if let Some(value) = args.get(arg_name) {
                    if let Some(s) = value.as_str() {
                        if !glob_match(value_pattern, s) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
}

/// Tool restrictions for a component
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolRestrictions {
    /// Allowed tools (if empty, all tools allowed)
    #[serde(default)]
    pub allowed: Vec<ToolSpec>,
    /// Explicitly denied tools
    #[serde(default)]
    pub denied: Vec<ToolSpec>,
}

impl ToolRestrictions {
    /// Check if a tool call is permitted
    pub fn is_allowed(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        // Check denied first
        for spec in &self.denied {
            if spec.matches(tool_name, args) {
                return false;
            }
        }

        // If no allowed list, everything (not denied) is allowed
        if self.allowed.is_empty() {
            return true;
        }

        // Check allowed list
        self.allowed.iter().any(|spec| spec.matches(tool_name, args))
    }

    /// Intersect two restrictions (most restrictive wins)
    pub fn intersect(&self, other: &ToolRestrictions) -> ToolRestrictions {
        let mut result = ToolRestrictions::default();

        // Combine denied lists
        result.denied = self.denied.clone();
        result.denied.extend(other.denied.clone());

        // Intersect allowed lists
        if !self.allowed.is_empty() && !other.allowed.is_empty() {
            // Both have restrictions - find intersection
            result.allowed = self.allowed.iter()
                .filter(|a| other.allowed.iter().any(|b| specs_overlap(a, b)))
                .cloned()
                .collect();
        } else if !self.allowed.is_empty() {
            result.allowed = self.allowed.clone();
        } else {
            result.allowed = other.allowed.clone();
        }

        result
    }
}

/// Color for visual identification
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AgentColor {
    #[default]
    Blue,
    Green,
    Yellow,
    Orange,
    Red,
    Purple,
    Cyan,
    #[serde(untagged)]
    Custom(String),
}

/// Context isolation mode
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ContextMode {
    #[default]
    Inherit,  // Share parent context
    Fork,     // Create isolated context
}

/// Source scope for a component
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Enterprise = 0,  // Highest priority
    Project = 1,
    User = 2,
    Plugin = 3,
    Builtin = 4,     // Lowest priority
}
```

### Hook Types

```rust
// src/prompt/hooks.rs

use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Hook event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    /// Session starts
    SessionStart,
    /// User submits a prompt
    UserPromptSubmit,
    /// Before tool execution
    PreToolUse,
    /// After tool execution
    PostToolUse,
    /// Agent stops
    Stop,
    /// Subagent stops
    SubagentStop,
    /// Before context compaction
    PreCompact,
    /// Notification event
    Notification,
}

/// Hook handler types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookHandler {
    /// Shell command handler
    Command {
        command: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    /// Inline prompt injection
    Prompt {
        content: String,
    },
    /// MCP tool invocation
    McpTool {
        server: String,
        tool: String,
        args: serde_json::Value,
    },
}

/// Single hook definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
    /// The handler to execute
    pub handler: HookHandler,
    /// Tool matcher for PreToolUse/PostToolUse
    #[serde(default)]
    pub matcher: Option<String>,
}

/// Hook registration for an event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRegistration {
    /// Optional matcher (for tool-specific hooks)
    #[serde(default)]
    pub matcher: Option<String>,
    /// List of hooks to execute
    pub hooks: Vec<HookDefinition>,
}

/// Complete hooks configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub SessionStart: Vec<HookRegistration>,
    #[serde(default)]
    pub UserPromptSubmit: Vec<HookRegistration>,
    #[serde(default)]
    pub PreToolUse: Vec<HookRegistration>,
    #[serde(default)]
    pub PostToolUse: Vec<HookRegistration>,
    #[serde(default)]
    pub Stop: Vec<HookRegistration>,
    #[serde(default)]
    pub SubagentStop: Vec<HookRegistration>,
    #[serde(default)]
    pub PreCompact: Vec<HookRegistration>,
}

/// Result from hook execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// The event that triggered this hook
    #[serde(rename = "hookEventName")]
    pub hook_event_name: HookEvent,
    /// Additional context to inject into prompt
    #[serde(default, rename = "additionalContext")]
    pub additional_context: Option<String>,
    /// Whether to block the action (for Pre* hooks)
    #[serde(default)]
    pub block: bool,
    /// Reason for blocking
    #[serde(default)]
    pub block_reason: Option<String>,
    /// Modified tool arguments (for PreToolUse)
    #[serde(default)]
    pub modified_args: Option<serde_json::Value>,
}
```

### Agent Types

```rust
// src/prompt/agents.rs

use super::types::*;
use serde::{Deserialize, Serialize};

/// Agent metadata from YAML frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    /// Unique identifier (lowercase, hyphenated)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Model preference
    #[serde(default)]
    pub model: ModelPreference,
    /// Visual color
    #[serde(default)]
    pub color: AgentColor,
    /// Allowed tools (comma-separated or list)
    #[serde(default)]
    pub tools: ToolList,
    /// Context isolation mode
    #[serde(default)]
    pub context: ContextMode,
    /// Maximum turns before stopping
    #[serde(default)]
    pub max_turns: Option<usize>,
}

/// Tool list can be string or array
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolList {
    #[default]
    All,
    Csv(String),
    List(Vec<String>),
}

impl ToolList {
    pub fn to_restrictions(&self) -> ToolRestrictions {
        match self {
            ToolList::All => ToolRestrictions::default(),
            ToolList::Csv(s) => ToolRestrictions {
                allowed: s.split(',')
                    .map(|t| ToolSpec::Name(t.trim().to_string()))
                    .collect(),
                denied: vec![],
            },
            ToolList::List(list) => ToolRestrictions {
                allowed: list.iter()
                    .map(|t| ToolSpec::Name(t.clone()))
                    .collect(),
                denied: vec![],
            },
        }
    }
}

/// Complete agent definition
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    /// Metadata from frontmatter
    pub metadata: AgentMetadata,
    /// System prompt content (markdown after frontmatter)
    pub system_prompt: String,
    /// Source file path
    pub source_path: PathBuf,
    /// Scope (where it was loaded from)
    pub scope: Scope,
}

impl AgentDefinition {
    /// Parse from file content
    pub fn parse(content: &str, source_path: PathBuf, scope: Scope) -> Result<Self, ParseError> {
        let (metadata, system_prompt) = parse_yaml_frontmatter(content)?;
        Ok(Self {
            metadata,
            system_prompt,
            source_path,
            scope,
        })
    }
}

/// Built-in agent definitions
pub fn builtin_agents() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            metadata: AgentMetadata {
                name: "Explore".to_string(),
                description: "Fast codebase exploration. Use for searching files and code.".to_string(),
                model: ModelPreference::Haiku,
                color: AgentColor::Cyan,
                tools: ToolList::Csv("Glob, Grep, Read, WebSearch".to_string()),
                context: ContextMode::Fork,
                max_turns: Some(20),
            },
            system_prompt: include_str!("builtin/explore.md").to_string(),
            source_path: PathBuf::from("<builtin>/explore"),
            scope: Scope::Builtin,
        },
        AgentDefinition {
            metadata: AgentMetadata {
                name: "Plan".to_string(),
                description: "Research and planning. Use for designing implementations.".to_string(),
                model: ModelPreference::Inherit,
                color: AgentColor::Purple,
                tools: ToolList::Csv("Glob, Grep, Read, WebSearch, WebFetch".to_string()),
                context: ContextMode::Fork,
                max_turns: Some(30),
            },
            system_prompt: include_str!("builtin/plan.md").to_string(),
            source_path: PathBuf::from("<builtin>/plan"),
            scope: Scope::Builtin,
        },
        AgentDefinition {
            metadata: AgentMetadata {
                name: "Bash".to_string(),
                description: "Terminal operations. Use for git, npm, and shell commands.".to_string(),
                model: ModelPreference::Inherit,
                color: AgentColor::Green,
                tools: ToolList::Csv("Bash, KillShell".to_string()),
                context: ContextMode::Inherit,
                max_turns: Some(10),
            },
            system_prompt: include_str!("builtin/bash.md").to_string(),
            source_path: PathBuf::from("<builtin>/bash"),
            scope: Scope::Builtin,
        },
        AgentDefinition {
            metadata: AgentMetadata {
                name: "general-purpose".to_string(),
                description: "Complex multi-step tasks. Use when other agents don't fit.".to_string(),
                model: ModelPreference::Inherit,
                color: AgentColor::Blue,
                tools: ToolList::All,
                context: ContextMode::Fork,
                max_turns: Some(50),
            },
            system_prompt: include_str!("builtin/general.md").to_string(),
            source_path: PathBuf::from("<builtin>/general"),
            scope: Scope::Builtin,
        },
    ]
}
```

### Skill Types

```rust
// src/prompt/skills.rs

use super::types::*;
use serde::{Deserialize, Serialize};

/// Skill metadata from YAML frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Unique identifier
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Can Claude auto-invoke this skill?
    #[serde(default, rename = "disable-model-invocation")]
    pub disable_model_invocation: bool,
    /// Can user invoke with /skill-name?
    #[serde(default = "default_true", rename = "user-invocable")]
    pub user_invocable: bool,
    /// Allowed tools
    #[serde(default, rename = "allowed-tools")]
    pub allowed_tools: ToolList,
    /// Context mode
    #[serde(default)]
    pub context: ContextMode,
    /// Agent type to use
    #[serde(default)]
    pub agent: Option<String>,
    /// Argument hints for autocomplete
    #[serde(default, rename = "argument-hint")]
    pub argument_hint: Vec<String>,
}

fn default_true() -> bool { true }

/// Complete skill definition
#[derive(Debug, Clone)]
pub struct SkillDefinition {
    /// Metadata from frontmatter
    pub metadata: SkillMetadata,
    /// Instructions content (markdown)
    pub instructions: String,
    /// Source path
    pub source_path: PathBuf,
    /// Scope
    pub scope: Scope,
}

impl SkillDefinition {
    /// Parse from SKILL.md content
    pub fn parse(content: &str, source_path: PathBuf, scope: Scope) -> Result<Self, ParseError> {
        let (metadata, instructions) = parse_yaml_frontmatter(content)?;
        Ok(Self {
            metadata,
            instructions,
            source_path,
            scope,
        })
    }

    /// Apply variable substitution to instructions
    pub fn render(&self, context: &SubstitutionContext) -> String {
        let mut result = self.instructions.clone();

        // Variable substitution
        result = result.replace("$ARGUMENTS", &context.arguments);
        result = result.replace("${CLAUDE_SESSION_ID}", &context.session_id);
        result = result.replace("${CLAUDE_PLUGIN_ROOT}",
            &context.plugin_root.to_string_lossy());

        // Shell command substitution: !`command`
        result = substitute_shell_commands(&result);

        result
    }
}

/// Context for variable substitution
pub struct SubstitutionContext {
    pub arguments: String,
    pub session_id: String,
    pub plugin_root: PathBuf,
    pub workspace: PathBuf,
}
```

### Command Types

```rust
// src/prompt/commands.rs

use super::types::*;
use serde::{Deserialize, Serialize};

/// Command metadata (parsed from markdown headers)
#[derive(Debug, Clone, Default)]
pub struct CommandMetadata {
    /// Command name
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Allowed tools
    pub allowed_tools: ToolRestrictions,
    /// Agent to use
    pub agent: Option<String>,
}

/// Complete command definition
#[derive(Debug, Clone)]
pub struct CommandDefinition {
    /// Metadata
    pub metadata: CommandMetadata,
    /// Full markdown content
    pub content: String,
    /// Source path
    pub source_path: PathBuf,
    /// Scope
    pub scope: Scope,
}

impl CommandDefinition {
    /// Parse command from markdown file
    pub fn parse(content: &str, source_path: PathBuf, scope: Scope) -> Result<Self, ParseError> {
        let metadata = Self::extract_metadata(content)?;
        Ok(Self {
            metadata,
            content: content.to_string(),
            source_path,
            scope,
        })
    }

    fn extract_metadata(content: &str) -> Result<CommandMetadata, ParseError> {
        let mut metadata = CommandMetadata::default();

        for line in content.lines() {
            if line.starts_with("**Allowed Tools**:") || line.starts_with("- **Allowed Tools**:") {
                let tools_str = line.split(':').nth(1).unwrap_or("").trim();
                metadata.allowed_tools = parse_tool_restrictions(tools_str);
            } else if line.starts_with("**Description**:") || line.starts_with("- **Description**:") {
                metadata.description = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
            } else if line.starts_with("**Agent**:") || line.starts_with("- **Agent**:") {
                metadata.agent = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
            } else if line.starts_with("# ") {
                // First H1 is the command name
                if metadata.name.is_empty() {
                    metadata.name = line[2..].trim().to_string();
                }
            }
        }

        Ok(metadata)
    }

    /// Render command with substitutions
    pub fn render(&self, context: &SubstitutionContext) -> String {
        let mut result = self.content.clone();

        // Variable substitution
        result = result.replace("$ARGUMENTS", &context.arguments);

        // Shell command substitution
        result = substitute_shell_commands(&result);

        result
    }
}
```

---

## File Formats

### Hooks Configuration (hooks.json)

```json
{
  "SessionStart": [
    {
      "description": "Inject project context",
      "hooks": [
        {
          "type": "command",
          "command": "${CLAUDE_PLUGIN_ROOT}/hooks/session-start.sh",
          "timeout_ms": 5000
        }
      ]
    }
  ],
  "PreToolUse": [
    {
      "matcher": "Bash",
      "hooks": [
        {
          "type": "command",
          "command": "./hooks/validate-bash.sh"
        }
      ]
    },
    {
      "matcher": "Write(*.env:*)",
      "hooks": [
        {
          "type": "prompt",
          "content": "BLOCKED: Cannot write to .env files"
        }
      ]
    }
  ],
  "PostToolUse": [
    {
      "matcher": "Bash(git commit:*)",
      "hooks": [
        {
          "type": "command",
          "command": "./hooks/post-commit.sh"
        }
      ]
    }
  ]
}
```

### Agent Definition (agents/code-architect.md)

```markdown
---
name: code-architect
description: "Design feature architectures and implementation plans"
model: sonnet
color: blue
tools: Glob, Grep, Read, WebSearch, WebFetch
context: fork
max_turns: 30
---

# Code Architect

You are a senior software architect. Your role is to design clean,
maintainable implementations that follow existing patterns.

## Workflow

### 1. Pattern Discovery

First, examine the codebase to understand:
- Directory structure and organization
- Naming conventions
- Common patterns and abstractions
- Technology choices

### 2. Deliberate Design

Make confident decisions:
- Choose ONE approach (not multiple options)
- Align with existing patterns
- Consider error handling from the start

### 3. Actionable Blueprint

Deliver specific details:
- File-by-file implementation plan
- Exact code patterns to follow
- Integration points with existing code

## Output Format

Always structure your response as:

```
## Analysis
[What you learned about the codebase]

## Design Decision
[Your chosen approach and why]

## Implementation Plan
1. [First file] - [What to create/modify]
2. [Second file] - [What to create/modify]
...

## Code Examples
[Specific code snippets showing key patterns]
```
```

### Skill Definition (skills/code-review/SKILL.md)

```markdown
---
name: code-review
description: "Review code for quality, bugs, and security issues"
disable-model-invocation: false
user-invocable: true
allowed-tools: Read, Grep, Glob, LSP
context: fork
agent: Explore
argument-hint: [files, directories]
---

# Code Review Instructions

When reviewing code, follow this systematic approach:

## 1. Structure Analysis

- Check code organization
- Verify naming conventions
- Assess module boundaries

## 2. Logic Review

- Trace execution paths
- Identify edge cases
- Check error handling

## 3. Security Scan

- Input validation
- Injection vulnerabilities
- Authentication/authorization

## 4. Quality Assessment

- Code duplication
- Complexity metrics
- Test coverage

## Output Format

For each issue found:

```
### [SEVERITY] Issue Title

**Location**: `file:line`
**Category**: [bug|security|style|performance]
**Confidence**: [0-100]

**Description**: What's wrong

**Suggestion**: How to fix it

**Code**:
```language
// Suggested fix
```
```

Rate overall quality: [0-100]
```

### Command Definition (commands/commit.md)

```markdown
# Git Commit

**Allowed Tools**: Bash(git add:*), Bash(git status:*), Bash(git diff:*), Bash(git commit:*), Bash(git log:*)
**Description**: Create a git commit with proper message

## Current Context

Repository status:
!`git status`

Staged changes:
!`git diff --cached --stat`

Unstaged changes:
!`git diff --stat`

Recent commits (for style reference):
!`git log --oneline -5`

## Instructions

1. Review the changes to understand their purpose
2. Stage relevant files if needed
3. Analyze recent commits for message style
4. Create a commit message that:
   - Summarizes the "why" not the "what"
   - Follows the repository's conventions
   - Is concise (1-2 sentences)
5. Execute the commit
6. Verify success with git status

## Commit Message Format

```
<type>: <description>

[optional body]

Co-Authored-By: AI Assistant <noreply@example.com>
```

Types: feat, fix, docs, style, refactor, test, chore
```

### Plugin Manifest (plugin.json)

```json
{
  "name": "code-quality",
  "version": "1.0.0",
  "description": "Code quality tools including review, refactoring, and analysis",
  "author": {
    "name": "Developer Name",
    "email": "dev@example.com"
  },
  "homepage": "https://github.com/example/plugin",
  "keywords": ["code-review", "refactoring", "analysis"],
  "engines": {
    "cowork": ">=0.1.0"
  },
  "dependencies": {
    "mcp-servers": ["@anthropic/code-analysis"]
  }
}
```

---

## Component Implementations

### YAML Frontmatter Parser

```rust
// src/prompt/parser.rs

use serde::de::DeserializeOwned;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Missing YAML frontmatter")]
    MissingFrontmatter,
    #[error("Invalid YAML: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),
    #[error("Invalid frontmatter delimiter")]
    InvalidDelimiter,
}

/// Parse YAML frontmatter from markdown content
///
/// Format:
/// ```
/// ---
/// key: value
/// ---
///
/// Markdown content here
/// ```
pub fn parse_yaml_frontmatter<T: DeserializeOwned>(
    content: &str
) -> Result<(T, String), ParseError> {
    let content = content.trim();

    // Must start with ---
    if !content.starts_with("---") {
        return Err(ParseError::MissingFrontmatter);
    }

    // Find closing ---
    let rest = &content[3..];
    let end_idx = rest.find("\n---")
        .ok_or(ParseError::InvalidDelimiter)?;

    let yaml_content = &rest[..end_idx];
    let markdown_content = &rest[end_idx + 4..];

    let metadata: T = serde_yaml::from_str(yaml_content)?;
    let markdown = markdown_content.trim().to_string();

    Ok((metadata, markdown))
}

/// Substitute shell commands in content
///
/// Format: !`command`
pub fn substitute_shell_commands(content: &str) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '!' && chars.peek() == Some(&'`') {
            chars.next(); // consume `

            // Read until closing `
            let mut command = String::new();
            while let Some(c) = chars.next() {
                if c == '`' {
                    break;
                }
                command.push(c);
            }

            // Execute command and insert output
            match execute_shell_command(&command) {
                Ok(output) => result.push_str(&output),
                Err(e) => result.push_str(&format!("[Error: {}]", e)),
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn execute_shell_command(command: &str) -> Result<String, std::io::Error> {
    use std::process::Command;

    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

### Hook Executor

```rust
// src/prompt/hook_executor.rs

use super::hooks::*;
use super::types::*;
use std::process::Command;
use std::time::Duration;
use tokio::time::timeout;

pub struct HookExecutor {
    workspace: PathBuf,
    plugin_root: Option<PathBuf>,
    timeout: Duration,
}

impl HookExecutor {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            plugin_root: None,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_plugin_root(mut self, root: PathBuf) -> Self {
        self.plugin_root = Some(root);
        self
    }

    /// Execute hooks for an event
    pub async fn execute(
        &self,
        event: HookEvent,
        hooks: &[HookRegistration],
        context: &HookContext,
    ) -> Vec<HookResult> {
        let mut results = Vec::new();

        for registration in hooks {
            // Check matcher for tool-related events
            if let Some(matcher) = &registration.matcher {
                if !self.matches_tool(matcher, context) {
                    continue;
                }
            }

            for hook in &registration.hooks {
                match self.execute_hook(&hook.handler, event, context).await {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        tracing::error!("Hook execution failed: {}", e);
                    }
                }
            }
        }

        results
    }

    async fn execute_hook(
        &self,
        handler: &HookHandler,
        event: HookEvent,
        context: &HookContext,
    ) -> Result<HookResult, HookError> {
        match handler {
            HookHandler::Command { command, timeout_ms } => {
                self.execute_command_handler(command, event, context, *timeout_ms).await
            }
            HookHandler::Prompt { content } => {
                Ok(HookResult {
                    hook_event_name: event,
                    additional_context: Some(content.clone()),
                    block: false,
                    block_reason: None,
                    modified_args: None,
                })
            }
            HookHandler::McpTool { server, tool, args } => {
                self.execute_mcp_handler(server, tool, args, event, context).await
            }
        }
    }

    async fn execute_command_handler(
        &self,
        command: &str,
        event: HookEvent,
        context: &HookContext,
        timeout_ms: Option<u64>,
    ) -> Result<HookResult, HookError> {
        // Expand variables in command
        let expanded = self.expand_command(command);

        let timeout_duration = timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(self.timeout);

        // Prepare environment
        let mut env = std::env::vars().collect::<HashMap<_, _>>();
        env.insert("HOOK_EVENT".to_string(), format!("{:?}", event));
        env.insert("WORKSPACE".to_string(), self.workspace.to_string_lossy().to_string());

        if let Some(tool_name) = &context.tool_name {
            env.insert("TOOL_NAME".to_string(), tool_name.clone());
        }
        if let Some(tool_args) = &context.tool_args {
            env.insert("TOOL_ARGS".to_string(), tool_args.to_string());
        }

        // Execute with timeout
        let output = timeout(timeout_duration, async {
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&expanded)
                .current_dir(&self.workspace)
                .envs(env)
                .output()
                .await
        }).await??;

        // Parse JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Ok(HookResult {
                hook_event_name: event,
                additional_context: None,
                block: false,
                block_reason: None,
                modified_args: None,
            });
        }

        serde_json::from_str(&stdout)
            .map_err(|e| HookError::InvalidOutput(e.to_string()))
    }

    fn expand_command(&self, command: &str) -> String {
        let mut result = command.to_string();

        if let Some(root) = &self.plugin_root {
            result = result.replace("${CLAUDE_PLUGIN_ROOT}", &root.to_string_lossy());
        }

        result = result.replace("${WORKSPACE}", &self.workspace.to_string_lossy());

        result
    }

    fn matches_tool(&self, matcher: &str, context: &HookContext) -> bool {
        if let Some(tool_name) = &context.tool_name {
            if let Some(tool_args) = &context.tool_args {
                let spec = ToolSpec::Pattern(matcher.to_string());
                return spec.matches(tool_name, tool_args);
            }
        }
        false
    }
}

/// Context passed to hooks
pub struct HookContext {
    pub tool_name: Option<String>,
    pub tool_args: Option<serde_json::Value>,
    pub user_prompt: Option<String>,
    pub session_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("Command failed: {0}")]
    CommandFailed(#[from] std::io::Error),
    #[error("Timeout")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("Invalid output: {0}")]
    InvalidOutput(String),
}
```

---

## Prompt Assembly Pipeline

### Prompt Builder

```rust
// src/prompt/builder.rs

use super::*;

/// Assembled prompt ready for LLM
pub struct AssembledPrompt {
    /// System prompt content
    pub system: String,
    /// Tool definitions (filtered by restrictions)
    pub tools: Vec<ToolDefinition>,
    /// Active restrictions
    pub restrictions: ToolRestrictions,
    /// Model preference
    pub model: ModelPreference,
    /// Context mode
    pub context_mode: ContextMode,
}

/// Prompt builder with layered composition
pub struct PromptBuilder {
    /// Base system prompt
    base_prompt: String,
    /// Hook-injected context
    hook_context: Vec<String>,
    /// Active agent
    agent: Option<AgentDefinition>,
    /// Active skills
    skills: Vec<SkillDefinition>,
    /// Active command
    command: Option<CommandDefinition>,
    /// Cumulative tool restrictions
    restrictions: ToolRestrictions,
    /// Substitution context
    sub_context: SubstitutionContext,
}

impl PromptBuilder {
    pub fn new(base_prompt: String, sub_context: SubstitutionContext) -> Self {
        Self {
            base_prompt,
            hook_context: Vec::new(),
            agent: None,
            skills: Vec::new(),
            command: None,
            restrictions: ToolRestrictions::default(),
            sub_context,
        }
    }

    /// Add hook-injected context
    pub fn with_hook_context(mut self, context: String) -> Self {
        self.hook_context.push(context);
        self
    }

    /// Set the active agent
    pub fn with_agent(mut self, agent: AgentDefinition) -> Self {
        // Apply agent's tool restrictions
        let agent_restrictions = agent.metadata.tools.to_restrictions();
        self.restrictions = self.restrictions.intersect(&agent_restrictions);
        self.agent = Some(agent);
        self
    }

    /// Add a skill
    pub fn with_skill(mut self, skill: SkillDefinition) -> Self {
        // Apply skill's tool restrictions
        let skill_restrictions = skill.metadata.allowed_tools.to_restrictions();
        self.restrictions = self.restrictions.intersect(&skill_restrictions);
        self.skills.push(skill);
        self
    }

    /// Set the active command
    pub fn with_command(mut self, command: CommandDefinition) -> Self {
        // Apply command's tool restrictions
        self.restrictions = self.restrictions.intersect(&command.metadata.allowed_tools);
        self.command = Some(command);
        self
    }

    /// Build the final prompt
    pub fn build(self, all_tools: &[ToolDefinition]) -> AssembledPrompt {
        let mut system_parts = Vec::new();

        // 1. Base prompt
        system_parts.push(self.base_prompt);

        // 2. Hook-injected context
        for context in &self.hook_context {
            system_parts.push(format!("\n<injected-context>\n{}\n</injected-context>", context));
        }

        // 3. Agent system prompt
        let model = if let Some(agent) = &self.agent {
            system_parts.push(format!("\n<agent name=\"{}\">\n{}\n</agent>",
                agent.metadata.name,
                agent.system_prompt
            ));
            agent.metadata.model.clone()
        } else {
            ModelPreference::Inherit
        };

        // 4. Skill instructions
        for skill in &self.skills {
            let rendered = skill.render(&self.sub_context);
            system_parts.push(format!("\n<skill name=\"{}\">\n{}\n</skill>",
                skill.metadata.name,
                rendered
            ));
        }

        // 5. Command content
        let context_mode = if let Some(command) = &self.command {
            let rendered = command.render(&self.sub_context);
            system_parts.push(format!("\n<command>\n{}\n</command>", rendered));

            // Commands typically run in fork context
            ContextMode::Fork
        } else if let Some(agent) = &self.agent {
            agent.metadata.context.clone()
        } else {
            ContextMode::Inherit
        };

        // Filter tools by restrictions
        let filtered_tools = all_tools.iter()
            .filter(|tool| {
                self.restrictions.is_allowed(&tool.name, &serde_json::Value::Null)
            })
            .cloned()
            .collect();

        AssembledPrompt {
            system: system_parts.join("\n\n"),
            tools: filtered_tools,
            restrictions: self.restrictions,
            model,
            context_mode,
        }
    }
}
```

### Prompt Pipeline

```rust
// src/prompt/pipeline.rs

use super::*;

/// Main prompt assembly pipeline
pub struct PromptPipeline {
    /// Component registry
    registry: ComponentRegistry,
    /// Hook executor
    hook_executor: HookExecutor,
    /// Configuration
    config: PromptConfig,
}

impl PromptPipeline {
    pub fn new(
        registry: ComponentRegistry,
        hook_executor: HookExecutor,
        config: PromptConfig,
    ) -> Self {
        Self { registry, hook_executor, config }
    }

    /// Assemble prompt for a request
    pub async fn assemble(
        &self,
        request: &PromptRequest,
        all_tools: &[ToolDefinition],
    ) -> Result<AssembledPrompt, PipelineError> {
        let sub_context = SubstitutionContext {
            arguments: request.arguments.clone().unwrap_or_default(),
            session_id: request.session_id.clone(),
            plugin_root: self.config.plugin_root.clone(),
            workspace: self.config.workspace.clone(),
        };

        let mut builder = PromptBuilder::new(
            self.config.base_system_prompt.clone(),
            sub_context.clone(),
        );

        // 1. Execute SessionStart hooks (if new session)
        if request.is_session_start {
            let hook_context = HookContext {
                tool_name: None,
                tool_args: None,
                user_prompt: None,
                session_id: request.session_id.clone(),
            };

            let results = self.hook_executor.execute(
                HookEvent::SessionStart,
                &self.registry.hooks.SessionStart,
                &hook_context,
            ).await;

            for result in results {
                if let Some(context) = result.additional_context {
                    builder = builder.with_hook_context(context);
                }
            }
        }

        // 2. Execute UserPromptSubmit hooks
        if let Some(user_prompt) = &request.user_prompt {
            let hook_context = HookContext {
                tool_name: None,
                tool_args: None,
                user_prompt: Some(user_prompt.clone()),
                session_id: request.session_id.clone(),
            };

            let results = self.hook_executor.execute(
                HookEvent::UserPromptSubmit,
                &self.registry.hooks.UserPromptSubmit,
                &hook_context,
            ).await;

            for result in results {
                if result.block {
                    return Err(PipelineError::Blocked(
                        result.block_reason.unwrap_or_default()
                    ));
                }
                if let Some(context) = result.additional_context {
                    builder = builder.with_hook_context(context);
                }
            }
        }

        // 3. Resolve agent
        if let Some(agent_name) = &request.agent {
            if let Some(agent) = self.registry.get_agent(agent_name) {
                builder = builder.with_agent(agent.clone());
            }
        } else if let Some(command_name) = &request.command {
            // Commands may specify an agent
            if let Some(command) = self.registry.get_command(command_name) {
                if let Some(agent_name) = &command.metadata.agent {
                    if let Some(agent) = self.registry.get_agent(agent_name) {
                        builder = builder.with_agent(agent.clone());
                    }
                }
                builder = builder.with_command(command.clone());
            }
        }

        // 4. Auto-select relevant skills
        for skill in self.registry.auto_invocable_skills() {
            if self.skill_matches_request(skill, request) {
                builder = builder.with_skill(skill.clone());
            }
        }

        // 5. Add explicitly requested skills
        for skill_name in &request.skills {
            if let Some(skill) = self.registry.get_skill(skill_name) {
                builder = builder.with_skill(skill.clone());
            }
        }

        // 6. Build final prompt
        Ok(builder.build(all_tools))
    }

    /// Check PreToolUse hooks
    pub async fn check_tool_use(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        session_id: &str,
    ) -> Result<Option<serde_json::Value>, PipelineError> {
        let hook_context = HookContext {
            tool_name: Some(tool_name.to_string()),
            tool_args: Some(tool_args.clone()),
            user_prompt: None,
            session_id: session_id.to_string(),
        };

        let results = self.hook_executor.execute(
            HookEvent::PreToolUse,
            &self.registry.hooks.PreToolUse,
            &hook_context,
        ).await;

        for result in results {
            if result.block {
                return Err(PipelineError::ToolBlocked {
                    tool: tool_name.to_string(),
                    reason: result.block_reason.unwrap_or_default(),
                });
            }
            if result.modified_args.is_some() {
                return Ok(result.modified_args);
            }
        }

        Ok(None)
    }

    fn skill_matches_request(&self, skill: &SkillDefinition, request: &PromptRequest) -> bool {
        if skill.metadata.disable_model_invocation {
            return false;
        }

        // Simple keyword matching - could be enhanced with embeddings
        if let Some(prompt) = &request.user_prompt {
            let prompt_lower = prompt.to_lowercase();
            let desc_lower = skill.metadata.description.to_lowercase();

            // Check if skill description keywords appear in prompt
            desc_lower.split_whitespace()
                .filter(|w| w.len() > 4)
                .any(|w| prompt_lower.contains(w))
        } else {
            false
        }
    }
}

/// Request for prompt assembly
pub struct PromptRequest {
    pub session_id: String,
    pub user_prompt: Option<String>,
    pub agent: Option<String>,
    pub command: Option<String>,
    pub skills: Vec<String>,
    pub arguments: Option<String>,
    pub is_session_start: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Request blocked: {0}")]
    Blocked(String),
    #[error("Tool blocked: {tool} - {reason}")]
    ToolBlocked { tool: String, reason: String },
    #[error("Hook error: {0}")]
    HookError(#[from] HookError),
}
```

---

## Component Registry

```rust
// src/prompt/registry.rs

use super::*;
use std::collections::HashMap;

/// Registry for all prompt components
pub struct ComponentRegistry {
    /// Agents by name
    agents: HashMap<String, AgentDefinition>,
    /// Skills by name
    skills: HashMap<String, SkillDefinition>,
    /// Commands by name
    commands: HashMap<String, CommandDefinition>,
    /// Hooks configuration
    pub hooks: HooksConfig,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            agents: HashMap::new(),
            skills: HashMap::new(),
            commands: HashMap::new(),
            hooks: HooksConfig::default(),
        };

        // Register built-in agents
        for agent in builtin_agents() {
            registry.register_agent(agent);
        }

        registry
    }

    /// Load components from filesystem
    pub fn load_from_paths(&mut self, paths: &ComponentPaths) -> Result<(), LoadError> {
        // Load in order of scope priority (highest first)

        // 1. Enterprise (if configured)
        if let Some(enterprise_path) = &paths.enterprise {
            self.load_directory(enterprise_path, Scope::Enterprise)?;
        }

        // 2. Project level
        self.load_directory(&paths.project, Scope::Project)?;

        // 3. User level
        self.load_directory(&paths.user, Scope::User)?;

        // 4. Plugins
        for plugin_path in &paths.plugins {
            self.load_plugin(plugin_path)?;
        }

        Ok(())
    }

    fn load_directory(&mut self, path: &Path, scope: Scope) -> Result<(), LoadError> {
        // Load agents
        let agents_dir = path.join("agents");
        if agents_dir.exists() {
            for entry in std::fs::read_dir(&agents_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    let content = std::fs::read_to_string(&path)?;
                    if let Ok(agent) = AgentDefinition::parse(&content, path.clone(), scope) {
                        self.register_agent(agent);
                    }
                }
            }
        }

        // Load skills
        let skills_dir = path.join("skills");
        if skills_dir.exists() {
            for entry in std::fs::read_dir(&skills_dir)? {
                let entry = entry?;
                let skill_path = entry.path();
                if skill_path.is_dir() {
                    let skill_file = skill_path.join("SKILL.md");
                    if skill_file.exists() {
                        let content = std::fs::read_to_string(&skill_file)?;
                        if let Ok(skill) = SkillDefinition::parse(&content, skill_file, scope) {
                            self.register_skill(skill);
                        }
                    }
                }
            }
        }

        // Load commands
        let commands_dir = path.join("commands");
        if commands_dir.exists() {
            for entry in std::fs::read_dir(&commands_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    let content = std::fs::read_to_string(&path)?;
                    if let Ok(command) = CommandDefinition::parse(&content, path.clone(), scope) {
                        self.register_command(command);
                    }
                }
            }
        }

        // Load hooks
        let hooks_file = path.join("hooks").join("hooks.json");
        if hooks_file.exists() {
            let content = std::fs::read_to_string(&hooks_file)?;
            let hooks: HooksConfig = serde_json::from_str(&content)?;
            self.merge_hooks(hooks);
        }

        Ok(())
    }

    fn load_plugin(&mut self, plugin_path: &Path) -> Result<(), LoadError> {
        // Verify plugin manifest
        let manifest_path = plugin_path.join(".claude-plugin").join("plugin.json");
        if !manifest_path.exists() {
            return Err(LoadError::InvalidPlugin("Missing plugin.json".into()));
        }

        // Load as lowest scope
        self.load_directory(plugin_path, Scope::Plugin)
    }

    pub fn register_agent(&mut self, agent: AgentDefinition) {
        let name = agent.metadata.name.clone();

        // Only replace if higher or equal scope
        if let Some(existing) = self.agents.get(&name) {
            if agent.scope <= existing.scope {
                self.agents.insert(name, agent);
            }
        } else {
            self.agents.insert(name, agent);
        }
    }

    pub fn register_skill(&mut self, skill: SkillDefinition) {
        let name = skill.metadata.name.clone();

        if let Some(existing) = self.skills.get(&name) {
            if skill.scope <= existing.scope {
                self.skills.insert(name, skill);
            }
        } else {
            self.skills.insert(name, skill);
        }
    }

    pub fn register_command(&mut self, command: CommandDefinition) {
        let name = command.metadata.name.clone();

        if let Some(existing) = self.commands.get(&name) {
            if command.scope <= existing.scope {
                self.commands.insert(name, command);
            }
        } else {
            self.commands.insert(name, command);
        }
    }

    fn merge_hooks(&mut self, new_hooks: HooksConfig) {
        // Hooks are additive - all registered hooks execute
        self.hooks.SessionStart.extend(new_hooks.SessionStart);
        self.hooks.UserPromptSubmit.extend(new_hooks.UserPromptSubmit);
        self.hooks.PreToolUse.extend(new_hooks.PreToolUse);
        self.hooks.PostToolUse.extend(new_hooks.PostToolUse);
        self.hooks.Stop.extend(new_hooks.Stop);
        self.hooks.SubagentStop.extend(new_hooks.SubagentStop);
        self.hooks.PreCompact.extend(new_hooks.PreCompact);
    }

    pub fn get_agent(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name)
    }

    pub fn get_skill(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    pub fn get_command(&self, name: &str) -> Option<&CommandDefinition> {
        self.commands.get(name)
    }

    pub fn auto_invocable_skills(&self) -> impl Iterator<Item = &SkillDefinition> {
        self.skills.values()
            .filter(|s| !s.metadata.disable_model_invocation)
    }

    pub fn user_invocable_skills(&self) -> impl Iterator<Item = &SkillDefinition> {
        self.skills.values()
            .filter(|s| s.metadata.user_invocable)
    }

    pub fn list_agents(&self) -> Vec<&AgentDefinition> {
        self.agents.values().collect()
    }

    pub fn list_skills(&self) -> Vec<&SkillDefinition> {
        self.skills.values().collect()
    }

    pub fn list_commands(&self) -> Vec<&CommandDefinition> {
        self.commands.values().collect()
    }
}

/// Paths for loading components
pub struct ComponentPaths {
    pub enterprise: Option<PathBuf>,
    pub project: PathBuf,
    pub user: PathBuf,
    pub plugins: Vec<PathBuf>,
}

impl ComponentPaths {
    pub fn default_for_workspace(workspace: &Path) -> Self {
        Self {
            enterprise: std::env::var("COWORK_ENTERPRISE_CONFIG")
                .ok()
                .map(PathBuf::from),
            project: workspace.join(".claude"),
            user: dirs::home_dir()
                .unwrap_or_default()
                .join(".claude"),
            plugins: Self::discover_plugins(workspace),
        }
    }

    fn discover_plugins(workspace: &Path) -> Vec<PathBuf> {
        let mut plugins = Vec::new();

        // Project plugins
        let project_plugins = workspace.join(".claude").join("plugins");
        if project_plugins.exists() {
            if let Ok(entries) = std::fs::read_dir(&project_plugins) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        plugins.push(entry.path());
                    }
                }
            }
        }

        // User plugins
        let user_plugins = dirs::home_dir()
            .unwrap_or_default()
            .join(".claude")
            .join("plugins");
        if user_plugins.exists() {
            if let Ok(entries) = std::fs::read_dir(&user_plugins) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        plugins.push(entry.path());
                    }
                }
            }
        }

        plugins
    }
}
```

---

## Configuration Management

### Settings Structure

```rust
// src/prompt/config.rs

use super::*;
use serde::{Deserialize, Serialize};

/// Complete prompt system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptConfig {
    /// Base system prompt
    #[serde(default)]
    pub base_system_prompt: String,
    /// Workspace directory
    pub workspace: PathBuf,
    /// Plugin root (for variable expansion)
    #[serde(default)]
    pub plugin_root: PathBuf,
    /// Permission settings
    #[serde(default)]
    pub permissions: PermissionConfig,
    /// Hook settings
    #[serde(default)]
    pub hooks: HooksConfig,
}

/// Permission configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    /// Explicitly allowed actions
    #[serde(default)]
    pub allow: Vec<String>,
    /// Explicitly denied actions
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Settings file structure (matches .claude/settings.json)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsFile {
    #[serde(default)]
    pub hooks: HooksConfig,
    #[serde(default)]
    pub permissions: PermissionConfig,
    #[serde(default)]
    pub agents: HashMap<String, AgentOverride>,
    #[serde(default)]
    pub skills: HashMap<String, SkillOverride>,
}

/// Override settings for an agent
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentOverride {
    pub model: Option<ModelPreference>,
    pub tools: Option<ToolList>,
    pub enabled: Option<bool>,
}

/// Override settings for a skill
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillOverride {
    #[serde(rename = "disable-model-invocation")]
    pub disable_model_invocation: Option<bool>,
    #[serde(rename = "user-invocable")]
    pub user_invocable: Option<bool>,
    pub enabled: Option<bool>,
}

/// Load and merge settings from multiple sources
pub fn load_settings(workspace: &Path) -> SettingsFile {
    let mut settings = SettingsFile::default();

    // 1. Load user settings
    let user_settings = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("settings.json");
    if let Ok(content) = std::fs::read_to_string(&user_settings) {
        if let Ok(user) = serde_json::from_str::<SettingsFile>(&content) {
            settings = merge_settings(settings, user);
        }
    }

    // 2. Load project settings (higher priority)
    let project_settings = workspace.join(".claude").join("settings.json");
    if let Ok(content) = std::fs::read_to_string(&project_settings) {
        if let Ok(project) = serde_json::from_str::<SettingsFile>(&content) {
            settings = merge_settings(settings, project);
        }
    }

    // 3. Load local settings (highest priority, not committed)
    let local_settings = workspace.join(".claude").join("settings.local.json");
    if let Ok(content) = std::fs::read_to_string(&local_settings) {
        if let Ok(local) = serde_json::from_str::<SettingsFile>(&content) {
            settings = merge_settings(settings, local);
        }
    }

    settings
}

fn merge_settings(base: SettingsFile, overlay: SettingsFile) -> SettingsFile {
    SettingsFile {
        hooks: HooksConfig {
            SessionStart: [base.hooks.SessionStart, overlay.hooks.SessionStart].concat(),
            UserPromptSubmit: [base.hooks.UserPromptSubmit, overlay.hooks.UserPromptSubmit].concat(),
            PreToolUse: [base.hooks.PreToolUse, overlay.hooks.PreToolUse].concat(),
            PostToolUse: [base.hooks.PostToolUse, overlay.hooks.PostToolUse].concat(),
            Stop: [base.hooks.Stop, overlay.hooks.Stop].concat(),
            SubagentStop: [base.hooks.SubagentStop, overlay.hooks.SubagentStop].concat(),
            PreCompact: [base.hooks.PreCompact, overlay.hooks.PreCompact].concat(),
        },
        permissions: PermissionConfig {
            allow: [base.permissions.allow, overlay.permissions.allow].concat(),
            deny: [base.permissions.deny, overlay.permissions.deny].concat(),
        },
        agents: {
            let mut merged = base.agents;
            merged.extend(overlay.agents);
            merged
        },
        skills: {
            let mut merged = base.skills;
            merged.extend(overlay.skills);
            merged
        },
    }
}
```

---

## Directory Structure

```
project/
├── .claude/
│   ├── settings.json           # Project settings (committed)
│   ├── settings.local.json     # Local overrides (gitignored)
│   ├── agents/
│   │   ├── code-architect.md
│   │   └── code-reviewer.md
│   ├── skills/
│   │   ├── code-review/
│   │   │   └── SKILL.md
│   │   └── test-runner/
│   │       └── SKILL.md
│   ├── commands/
│   │   ├── commit.md
│   │   └── review-pr.md
│   ├── hooks/
│   │   └── hooks.json
│   ├── hooks-handlers/
│   │   ├── session-start.sh
│   │   └── validate-bash.sh
│   └── plugins/
│       └── my-plugin/
│           ├── .claude-plugin/
│           │   └── plugin.json
│           ├── agents/
│           ├── skills/
│           ├── commands/
│           └── hooks/
│
~/.claude/
├── settings.json               # User-level settings
├── agents/                     # User-level agents
├── skills/                     # User-level skills
├── commands/                   # User-level commands
└── plugins/                    # User-level plugins
```

---

## Integration Example

```rust
// src/prompt/mod.rs

pub mod types;
pub mod hooks;
pub mod agents;
pub mod skills;
pub mod commands;
pub mod parser;
pub mod hook_executor;
pub mod builder;
pub mod pipeline;
pub mod registry;
pub mod config;

pub use types::*;
pub use hooks::*;
pub use agents::*;
pub use skills::*;
pub use commands::*;
pub use builder::*;
pub use pipeline::*;
pub use registry::*;
pub use config::*;

/// Initialize the prompt system for a workspace
pub fn init(workspace: &Path) -> Result<PromptPipeline, InitError> {
    // Load settings
    let settings = load_settings(workspace);

    // Create component registry
    let mut registry = ComponentRegistry::new();

    // Load from paths
    let paths = ComponentPaths::default_for_workspace(workspace);
    registry.load_from_paths(&paths)?;

    // Merge hooks from settings
    registry.merge_hooks(settings.hooks);

    // Create hook executor
    let hook_executor = HookExecutor::new(workspace.to_path_buf());

    // Create config
    let config = PromptConfig {
        base_system_prompt: include_str!("base_prompt.md").to_string(),
        workspace: workspace.to_path_buf(),
        plugin_root: PathBuf::new(),
        permissions: settings.permissions,
        hooks: HooksConfig::default(),
    };

    Ok(PromptPipeline::new(registry, hook_executor, config))
}

// Usage in agent loop
pub async fn example_usage() {
    let workspace = Path::new("/path/to/project");
    let pipeline = init(workspace).unwrap();

    // Get all available tools
    let all_tools = get_tool_definitions();

    // Assemble prompt for a request
    let request = PromptRequest {
        session_id: "session-123".to_string(),
        user_prompt: Some("Review the authentication code".to_string()),
        agent: None,
        command: None,
        skills: vec![],
        arguments: None,
        is_session_start: true,
    };

    let assembled = pipeline.assemble(&request, &all_tools).await.unwrap();

    // Use assembled.system as system prompt
    // Use assembled.tools as available tools
    // Use assembled.model for model selection

    // Before tool execution, check hooks
    let tool_name = "Bash";
    let tool_args = serde_json::json!({"command": "git status"});

    match pipeline.check_tool_use(tool_name, &tool_args, &request.session_id).await {
        Ok(None) => { /* proceed normally */ }
        Ok(Some(modified_args)) => { /* use modified args */ }
        Err(PipelineError::ToolBlocked { reason, .. }) => { /* tool was blocked */ }
        Err(e) => { /* other error */ }
    }
}
```

---

## Summary

This implementation provides:

1. **Modular Components**: Agents, Skills, Commands, Hooks as separate units
2. **Layered Composition**: Components combine through the PromptBuilder
3. **Scope Hierarchy**: Enterprise > Project > User > Plugin > Builtin
4. **Tool Restrictions**: Intersected when combining components
5. **Dynamic Context**: Shell substitution and variable expansion
6. **Event-Driven Hooks**: Intercept and modify behavior at key points
7. **Filesystem Discovery**: Auto-load from well-known paths

The system is designed to be:
- **Extensible**: Add components without modifying core code
- **Secure**: Principle of least privilege for tool access
- **Discoverable**: Components loaded from standard locations
- **Composable**: Mix and match components freely
