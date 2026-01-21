# Migration Guide for Cowork Prompt System

This guide helps existing Cowork users migrate to the new Claude Code-style prompt system.

## Overview

The new prompt system introduces:
- **Agents**: Specialized subagents with custom prompts and tool restrictions
- **Commands**: User-triggered workflows via `/command` syntax
- **Hooks**: Event-based prompt injection and tool interception
- **Plugins**: Packaged prompt components for distribution
- **Component Registry**: Unified discovery and loading system

## Breaking Changes

### None

The new prompt system is **fully backward compatible**. All existing functionality continues to work without modification.

## New Features

### 1. Custom Agents

Create custom agents by adding markdown files to `.claude/agents/`:

```markdown
---
name: my-agent
description: "Custom agent description"
model: sonnet
tools: Read, Write, Grep, Glob
context: fork
max_turns: 30
---

# My Agent

Your custom system prompt here...
```

**Location**:
- Project: `.claude/agents/*.md`
- User: `~/.claude/agents/*.md`

### 2. Custom Commands

Create custom commands by adding markdown files to `.claude/commands/`:

```markdown
---
name: my-command
description: "Run my custom workflow"
allowed_tools: Bash, Read
---

# My Command

Instructions for the command...

## Context

!`git status`  <!-- Shell substitution -->

## Arguments

$ARGUMENTS  <!-- User-provided arguments -->
```

**Invocation**: `/my-command [arguments]`

**Location**:
- Project: `.claude/commands/*.md`
- User: `~/.claude/commands/*.md`

### 3. Hooks Configuration

Create hooks to intercept and modify agent behavior:

**File**: `.claude/hooks/hooks.json`

```json
{
  "SessionStart": [
    {
      "hooks": [
        {
          "type": "command",
          "command": "./hooks/session-start.sh",
          "timeout_ms": 5000
        }
      ]
    }
  ],
  "PreToolUse": [
    {
      "matcher": "Write(*.env:*)",
      "hooks": [
        {
          "type": "prompt",
          "content": "BLOCKED: Cannot write to .env files"
        }
      ]
    }
  ]
}
```

**Hook Events**:
- `SessionStart`: Session begins
- `UserPromptSubmit`: User sends a message
- `PreToolUse`: Before tool execution (can block/modify)
- `PostToolUse`: After tool execution
- `Stop`: Agent completes
- `SubagentStop`: Subagent completes
- `PreCompact`: Before context compaction

### 4. Plugins

Package and share prompt components:

**Structure**:
```
my-plugin/
├── plugin.json
├── agents/
├── commands/
├── skills/
└── hooks/
```

**Manifest** (`plugin.json`):
```json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "Plugin description",
  "author": "Your Name"
}
```

**Location**:
- Project: `.claude/plugins/`
- User: `~/.claude/plugins/`

**CLI Commands**:
```bash
cowork plugin list
cowork plugin info <name>
cowork plugin enable <name>
cowork plugin disable <name>
```

### 5. Component Discovery

View all loaded components:

```bash
cowork components all      # List everything
cowork components agents   # List agents
cowork components commands # List commands
cowork components skills   # List skills
```

## Scope Priority

Components are loaded with priority (higher overrides lower):

1. **Enterprise** (highest) - `$COWORK_ENTERPRISE_CONFIG`
2. **Project** - `.claude/`
3. **User** - `~/.claude/`
4. **Plugin** - `.claude/plugins/`, `~/.claude/plugins/`
5. **Builtin** (lowest) - Compiled into binary

Same-named components from higher scopes override lower ones.

## Built-in Agents

The following agents are available out of the box:

| Agent | Description | Model | Tools |
|-------|-------------|-------|-------|
| `Explore` | Fast codebase exploration | Haiku | Glob, Grep, Read, WebSearch |
| `Plan` | Implementation planning | Inherit | Glob, Grep, Read, WebSearch, WebFetch |
| `Bash` | Terminal operations | Inherit | Bash, KillShell |
| `general-purpose` | Complex multi-step tasks | Inherit | All |

## Built-in Commands

| Command | Description |
|---------|-------------|
| `/commit` | Create a git commit |
| `/pr` | Create a pull request |
| `/review-pr` | Review a pull request |

## Tool Restrictions

Components can specify tool restrictions:

```yaml
# Allow only specific tools
tools: Read, Write, Grep

# Or in separate fields
allowed_tools: Read, Write, Grep
denied_tools: Bash, Delete
```

When components are combined, restrictions **intersect** (most restrictive wins).

## Shell Substitution

Commands support shell substitution with `` !`command` `` syntax:

```markdown
Current status:
!`git status`

Recent commits:
!`git log --oneline -5`
```

## Template Variables

Available in prompts:

| Variable | Description |
|----------|-------------|
| `${WORKING_DIRECTORY}` | Current working directory |
| `${IS_GIT_REPO}` | "Yes" or "No" |
| `${PLATFORM}` | Operating system |
| `${OS_VERSION}` | OS version string |
| `${CURRENT_DATE}` | Today's date |
| `${MODEL_INFO}` | Model identification |
| `$ARGUMENTS` | Command arguments |

## Configuration

### Session Configuration

```rust
use cowork_core::session::SessionConfig;
use cowork_core::prompt::PromptSystemConfig;

let config = SessionConfig {
    prompt_system: Some(PromptSystemConfig {
        enabled: true,
        hooks_enabled: true,
        plugins_enabled: true,
        ..Default::default()
    }),
    ..Default::default()
};
```

### Programmatic Registry Access

```rust
use cowork_core::prompt::{ComponentRegistry, ComponentPaths};

// Create registry with built-in components
let mut registry = ComponentRegistry::with_builtins();

// Load from filesystem
let paths = ComponentPaths::for_project("/path/to/project");
registry.load_from_paths(&paths)?;

// Access components
if let Some(agent) = registry.get_agent("Explore") {
    println!("Found agent: {}", agent.metadata.description);
}

for command in registry.list_commands() {
    println!("Command: /{}", command.metadata.name);
}
```

## Migrating Skills

Existing skills continue to work. New skill features:

```yaml
---
name: my-skill
description: "Skill description"
# New fields:
denied_tools: Delete, Move      # Explicitly deny tools
context: fork                    # Context isolation
agent: Explore                   # Run in specific agent
disable_model_invocation: true   # Prevent auto-invocation
auto_triggers:                   # Patterns for auto-invocation
  - "review"
  - "code review"
argument_hint:                   # CLI autocomplete hints
  - "files"
  - "directories"
---
```

## Example Directory Structure

```
project/
├── .claude/
│   ├── settings.json           # Project settings
│   ├── agents/
│   │   └── code-reviewer.md    # Custom agent
│   ├── commands/
│   │   └── deploy.md           # Custom command
│   ├── skills/
│   │   └── test-runner/
│   │       └── SKILL.md
│   ├── hooks/
│   │   └── hooks.json          # Hook configuration
│   └── plugins/
│       └── my-plugin/
│           ├── plugin.json
│           └── agents/
│               └── specialist.md
```

## Troubleshooting

### Components Not Loading

1. Check file location (`.claude/` or `~/.claude/`)
2. Verify YAML frontmatter syntax
3. Check for required fields (name, description)
4. Run `cowork components all` to see what loaded

### Hooks Not Executing

1. Verify `hooks/hooks.json` location
2. Check JSON syntax
3. Ensure command scripts are executable
4. Check hook matcher patterns

### Plugin Issues

1. Verify `plugin.json` exists and is valid JSON
2. Check required fields (name, version)
3. Run `cowork plugin list` to see status
4. Check for conflicts with existing components

## Support

- Documentation: `docs/PROMPT_SYSTEM_IMPLEMENTATION.md`
- Design: `docs/CLAUDE_CODE_PROMPT_SYSTEM.md`
- Roadmap: `docs/PROMPT_SYSTEM_ROADMAP.md`
