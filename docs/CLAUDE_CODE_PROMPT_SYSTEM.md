# Claude Code Prompt System - Design Document

## Overview

Claude Code uses a modular, layered prompt architecture that enables dynamic composition of system prompts through multiple extension points. This document analyzes the prompt builder system, its flow, and key components.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    PROMPT COMPOSITION FLOW                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐                                               │
│  │ Base System  │  Claude's default system prompt               │
│  │   Prompt     │                                               │
│  └──────┬───────┘                                               │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                               │
│  │    Hooks     │  Event-based prompt injection                 │
│  │  (Runtime)   │  SessionStart, PreToolUse, etc.               │
│  └──────┬───────┘                                               │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                               │
│  │   Agents     │  Specialized subagents with custom prompts    │
│  │ (Subagents)  │  Isolated context, tool restrictions          │
│  └──────┬───────┘                                               │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                               │
│  │   Skills     │  Reusable instruction modules                 │
│  │              │  Auto or manual invocation                    │
│  └──────┬───────┘                                               │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                               │
│  │  Commands    │  User-triggered workflows                     │
│  │              │  Orchestrate multiple components              │
│  └──────────────┘                                               │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Hooks - Event-Based Prompt Injection

Hooks intercept lifecycle events and inject additional context/instructions into the prompt.

**Hook Events:**
| Event | When | Use Case |
|-------|------|----------|
| `SessionStart` | Session begins | Inject global instructions, style modes |
| `UserPromptSubmit` | Before processing user input | Validate, transform, add context |
| `PreToolUse` | Before tool execution | Validate, modify, or block tool calls |
| `PostToolUse` | After tool execution | Post-process results |
| `Stop` | Agent finishes | Cleanup, logging |
| `PreCompact` | Before context compaction | Preserve important context |

**Hook Configuration (hooks.json):**
```json
{
  "SessionStart": [
    {
      "description": "Inject custom instructions",
      "handler": {
        "type": "command",
        "command": "${CLAUDE_PLUGIN_ROOT}/hooks-handlers/session-start.sh"
      }
    }
  ],
  "PreToolUse": [
    {
      "matcher": "Bash",
      "hooks": [
        {
          "type": "command",
          "command": "./validate-bash.sh"
        }
      ]
    }
  ]
}
```

**Hook Handler Output:**
```json
{
  "hookEventName": "SessionStart",
  "additionalContext": "You are in explanatory mode. Provide educational insights..."
}
```

The `additionalContext` field is the **primary prompt injection mechanism**.

### 2. Agents (Subagents) - Specialized Prompt Builders

Agents are specialized AI assistants with:
- **Isolated context window** (separate from main conversation)
- **Custom system prompt** (YAML frontmatter + markdown)
- **Tool restrictions** (principle of least privilege)
- **Model selection** (Sonnet, Opus, Haiku)

**Agent Definition Format (YAML + Markdown):**
```yaml
---
name: code-architect
description: "Design feature architectures. Use for new features."
model: sonnet
color: blue
tools: Glob, Grep, Read, WebSearch
---

# Code Architect Agent

You are a senior software architect specializing in...

## Your Workflow

1. **Pattern Discovery** - Examine codebase for conventions
2. **Deliberate Design** - Make confident architectural decisions
3. **Actionable Blueprints** - Deliver specific implementation plans

## Output Format

Always provide:
- File-by-file implementation details
- Specific code patterns to follow
- Error handling strategies
```

**Built-in Subagents:**
| Agent | Model | Tools | Purpose |
|-------|-------|-------|---------|
| `Explore` | Haiku | Read-only | Fast codebase searching |
| `Plan` | Inherit | Read-only | Research during planning |
| `general-purpose` | Inherit | All | Complex multi-step tasks |
| `Bash` | Inherit | Bash only | Terminal operations |

### 3. Skills - Reusable Instruction Modules

Skills are composable instruction units that extend agent capabilities.

**Skill Definition (SKILL.md):**
```yaml
---
name: code-review
description: "Reviews code for quality, bugs, security"
disable-model-invocation: false  # Can Claude auto-invoke?
user-invocable: true             # Can user trigger with /skill?
allowed-tools: Read, Grep, Glob
context: fork                    # Run in isolated subagent
agent: Explore
argument-hint: [filenames]
---

# Code Review Instructions

When reviewing code:

1. **Structure Analysis**
   - Check code organization
   - Verify naming conventions

2. **Error Handling**
   - Ensure proper error propagation
   - Check edge cases

3. **Security Scan**
   - Look for injection vulnerabilities
   - Check input validation
```

**Skill Scope Priority (highest to lowest):**
1. Enterprise-managed settings
2. Project-level: `.claude/skills/<name>/SKILL.md`
3. User-level: `~/.claude/skills/<name>/SKILL.md`
4. Plugin skills

**Dynamic Value Substitution:**
- `$ARGUMENTS` - Arguments passed when invoking
- `${CLAUDE_SESSION_ID}` - Current session ID
- `` !`command` `` - Shell command output (runtime context)

### 4. Commands - Workflow Orchestration

Commands trigger complex workflows combining multiple components.

**Command Definition:**
```markdown
# Git Commit Command

**Allowed Tools**: Bash(git add:*), Bash(git status:*), Bash(git commit:*)
**Description**: Create a git commit with proper message

## Context

Current status:
!`git status`

Recent commits for style reference:
!`git log --oneline -5`

Staged changes:
!`git diff --cached`

## Instructions

1. Review the changes and understand their purpose
2. Stage relevant files if not already staged
3. Create a commit message following the repository's style
4. Execute the commit
```

### 5. Plugins - Distribution Packaging

Plugins bundle all components for distribution.

**Plugin Structure:**
```
my-plugin/
├── .claude-plugin/
│   └── plugin.json           # Metadata
├── agents/                   # Custom subagents
│   ├── code-architect.md
│   └── code-reviewer.md
├── skills/                   # Instruction modules
│   └── skill-name/SKILL.md
├── commands/                 # Workflow definitions
│   └── commit.md
├── hooks/
│   └── hooks.json           # Hook registration
├── hooks-handlers/          # Hook implementations
│   └── session-start.sh
├── .mcp.json                # MCP server config
└── README.md
```

## Prompt Assembly Flow

### Step-by-Step Process

```
1. SESSION INITIALIZATION
   │
   ├─→ Load plugin configurations
   ├─→ Register hooks from hooks.json
   └─→ Execute SessionStart hooks
       └─→ Collect additionalContext from handlers

2. USER INPUT RECEIVED
   │
   ├─→ Execute UserPromptSubmit hooks
   │   └─→ Validate/transform input
   │   └─→ Inject additional context
   │
   └─→ Determine execution path:
       ├─→ Command invocation (/commit, /review)
       ├─→ Skill invocation (/skill-name)
       └─→ Direct conversation

3. AGENT SELECTION
   │
   ├─→ Match task to agent descriptions
   ├─→ Load agent YAML frontmatter
   │   ├─→ Extract model preference
   │   ├─→ Extract allowed tools
   │   └─→ Extract color/metadata
   │
   └─→ Load agent markdown content (system prompt)

4. SKILL INJECTION
   │
   ├─→ Find matching skills (auto or explicit)
   ├─→ Load skill YAML configuration
   ├─→ Apply tool restrictions (intersection)
   └─→ Append skill instructions to prompt

5. PROMPT COMPILATION
   │
   ├─→ Base system prompt (Claude defaults)
   ├─→ + Hook-injected context (additionalContext)
   ├─→ + Agent system prompt (markdown)
   ├─→ + Skill instructions (markdown)
   ├─→ + Dynamic context (shell substitutions)
   └─→ + Tool definitions (filtered by restrictions)

6. EXECUTION
   │
   ├─→ Send compiled prompt to Claude
   ├─→ For each tool call:
   │   ├─→ Execute PreToolUse hooks
   │   ├─→ Validate against allowed tools
   │   ├─→ Execute tool
   │   └─→ Execute PostToolUse hooks
   │
   └─→ Return response

7. CLEANUP
   │
   ├─→ Execute Stop hooks
   ├─→ Check context usage (~95% triggers compaction)
   └─→ Execute PreCompact hooks if needed
```

### Visual Flow

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Plugins   │───▶│    Hooks    │───▶│   Agents    │
│  (Config)   │    │  (Events)   │    │ (Prompts)   │
└─────────────┘    └─────────────┘    └──────┬──────┘
                                             │
                         ┌───────────────────┴───────────────────┐
                         │                                       │
                         ▼                                       ▼
                  ┌─────────────┐                         ┌─────────────┐
                  │   Skills    │                         │  Commands   │
                  │(Instructions)│                         │ (Workflows) │
                  └──────┬──────┘                         └──────┬──────┘
                         │                                       │
                         └───────────────────┬───────────────────┘
                                             │
                                             ▼
                                    ┌─────────────────┐
                                    │ Compiled Prompt │
                                    │ + Tool Defs     │
                                    └────────┬────────┘
                                             │
                                             ▼
                                    ┌─────────────────┐
                                    │  Claude Model   │
                                    └─────────────────┘
```

## Design Principles

### 1. Principle of Least Privilege
- Each component gets only necessary tool access
- Tool restrictions are **intersected** when combining skills
- Default: deny, explicitly allow

### 2. Modular Composition
- Skills combine into agents
- Agents combine into commands
- Commands combine into workflows
- All components are independently testable

### 3. Scope Hierarchy
```
Enterprise Settings (highest)
    ↓
Project-Level (.claude/settings.json)
    ↓
User-Level (~/.claude/settings.json)
    ↓
Plugin Defaults (lowest)
```

### 4. Dynamic Context Injection
- Shell command substitution for runtime data
- Hook-based injection at lifecycle events
- Variable substitution for session state

### 5. Separation of Concerns
| Component | Responsibility |
|-----------|----------------|
| Hooks | Event interception, prompt injection |
| Agents | Specialized reasoning, isolated context |
| Skills | Reusable instructions, tool restrictions |
| Commands | Workflow orchestration |
| Plugins | Distribution packaging |

## Multi-Agent Orchestration Example

**Code Review Workflow (from PR Review Plugin):**

```
1. PRE-FLIGHT (Haiku)
   └─→ Verify PR isn't closed/draft/already reviewed

2. DISCOVERY (Haiku)
   └─→ Find CLAUDE.md style guidelines

3. SUMMARY (Sonnet)
   └─→ Get overview of changes

4. PARALLEL REVIEWS
   ├─→ Agent 1 (Sonnet): CLAUDE.md compliance
   ├─→ Agent 2 (Sonnet): Code style audit
   ├─→ Agent 3 (Opus): Bug/logic detection
   └─→ Agent 4 (Opus): Security scanning

5. VALIDATION (Subagents)
   └─→ Confirm flagged issues are genuine

6. FILTERING
   └─→ Remove issues with confidence < 80

7. COMMENT PLANNING
   └─→ Review before posting

8. EXECUTION
   └─→ Post inline comments with line links
```

## Configuration Examples

### Project-Level Settings
```json
// .claude/settings.json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [{
          "type": "command",
          "command": "./scripts/inject-project-context.sh"
        }]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Bash(rm:*)",
        "hooks": [{
          "type": "command",
          "command": "./scripts/confirm-delete.sh"
        }]
      }
    ]
  },
  "permissions": {
    "allow": ["Read all files", "Write to src/"],
    "deny": ["Write to .env", "Execute destructive commands"]
  }
}
```

### Agent Specialization Example
```yaml
# agents/type-analyzer.md
---
name: type-analyzer
description: "Analyze type system design quality"
model: opus
color: purple
tools: Read, Grep, Glob, LSP
---

# Type System Design Analyzer

You evaluate type design quality on these criteria:

## Evaluation Dimensions

1. **Encapsulation** (0-100)
   - Are implementation details hidden?
   - Are invariants protected?

2. **Type Safety** (0-100)
   - Are impossible states unrepresentable?
   - Is the type system leveraged for correctness?

3. **Ergonomics** (0-100)
   - Is the API intuitive to use?
   - Are common operations simple?

## Output Format

```json
{
  "overall_score": 85,
  "encapsulation": { "score": 90, "notes": "..." },
  "type_safety": { "score": 80, "notes": "..." },
  "ergonomics": { "score": 85, "notes": "..." },
  "recommendations": ["..."]
}
```
```

## Key Takeaways

1. **Hooks are the primary injection point** - The `additionalContext` field from hook handlers is how custom instructions enter the prompt.

2. **Agents provide isolation** - Subagents have their own context window, preventing main conversation pollution.

3. **Skills are composable** - Multiple skills can be combined, with tool restrictions intersected.

4. **Commands orchestrate workflows** - They combine hooks, agents, and skills into cohesive operations.

5. **Plugins package everything** - Distribution mechanism for sharing complete prompt configurations.

6. **Security is layered** - Tool restrictions at agent, skill, and command levels provide defense in depth.

## Recommendations for Cowork

Based on this analysis, consider implementing:

1. **Hook System** - Event-based prompt injection at key lifecycle points
2. **Agent Definitions** - YAML + Markdown format for specialized subagents
3. **Skill Modules** - Reusable instruction units with tool restrictions
4. **Scope Hierarchy** - Project > User > Default precedence
5. **Dynamic Substitution** - Shell commands and variables in prompts
6. **Plugin Architecture** - Package mechanism for distribution

## References

- Claude Code Documentation: https://code.claude.com/docs/
- GitHub Repository: https://github.com/anthropics/claude-code
- Plugin Examples: `/plugins` directory in repository
