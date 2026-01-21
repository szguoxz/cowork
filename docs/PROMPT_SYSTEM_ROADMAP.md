# Cowork Prompt System Implementation Roadmap

## Overview

This document outlines the plan to implement a Claude Code-style prompt system in Cowork. The system will enable modular, extensible prompt composition through hooks, agents, skills, and commands.

## Current State

Cowork already has:
- ✅ Tool registry with builder pattern
- ✅ Basic skills system (YAML + prompt)
- ✅ Configuration management (TOML)
- ✅ Session/agent loop
- ✅ System prompt builder (static)
- ✅ Multi-provider support

## Implementation Phases

### Phase 1: Core Types & Parser (1-2 days)

**Goal**: Establish foundational data structures and parsing.

**Files to Create**:
```
crates/cowork-core/src/prompt/
├── mod.rs           # Module exports
├── types.rs         # Core types (ToolSpec, Scope, ModelPreference)
├── parser.rs        # YAML frontmatter parser
└── substitution.rs  # Shell command & variable substitution
```

**Key Types**:
```rust
// Tool restriction with pattern matching
pub struct ToolSpec {
    name: String,
    pattern: Option<String>,  // e.g., "git:*"
}

pub struct ToolRestrictions {
    allowed: Vec<ToolSpec>,
    denied: Vec<ToolSpec>,
}

impl ToolRestrictions {
    pub fn is_allowed(&self, tool: &str, args: &Value) -> bool;
    pub fn intersect(&self, other: &Self) -> Self;
}

// Scope hierarchy
pub enum Scope {
    Enterprise = 0,
    Project = 1,
    User = 2,
    Plugin = 3,
    Builtin = 4,
}
```

**Tasks**:
- [x] Create `prompt` module structure *(COMPLETED - see `crates/cowork-core/src/prompt/`)*
- [x] Create built-in prompt files (system prompt, agents, tools, reminders) *(COMPLETED)*
- [x] Implement template variable substitution (`TemplateVars`) *(COMPLETED)*
- [ ] Implement `ToolSpec` with pattern matching
- [ ] Implement `ToolRestrictions` with intersection logic
- [ ] Implement YAML frontmatter parser
- [ ] Implement shell command substitution (`!`command``)
- [ ] Add unit tests

---

### Phase 2: Hook System (2-3 days)

**Goal**: Event-based prompt injection mechanism.

**Files to Create/Modify**:
```
crates/cowork-core/src/prompt/
├── hooks.rs         # Hook types and configuration
└── hook_executor.rs # Hook execution engine

crates/cowork-core/src/session/
└── agent_loop.rs    # Integrate hook calls
```

**Hook Events**:
```rust
pub enum HookEvent {
    SessionStart,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    Stop,
    PreCompact,
}
```

**Hook Handler Types**:
```rust
pub enum HookHandler {
    Command { command: String, timeout_ms: Option<u64> },
    Prompt { content: String },
    McpTool { server: String, tool: String, args: Value },
}
```

**Hook Result**:
```rust
pub struct HookResult {
    pub additional_context: Option<String>,  // Injected into prompt
    pub block: bool,                         // Block the action
    pub block_reason: Option<String>,
    pub modified_args: Option<Value>,        // For PreToolUse
}
```

**Tasks**:
- [ ] Define hook event types
- [ ] Implement hook handler execution (shell, prompt, mcp)
- [ ] Create `HookExecutor` with timeout support
- [ ] Add environment variable injection for hooks
- [ ] Integrate into agent loop (SessionStart, PreToolUse)
- [ ] Add hooks.json loading
- [ ] Add unit tests

**Integration Points**:
```rust
// In agent_loop.rs - Session start
let hook_results = hook_executor.execute(HookEvent::SessionStart, &hooks, &context).await;
for result in hook_results {
    if let Some(ctx) = result.additional_context {
        prompt_builder.add_hook_context(ctx);
    }
}

// In agent_loop.rs - Before tool execution
let hook_results = hook_executor.execute(HookEvent::PreToolUse, &hooks, &context).await;
for result in hook_results {
    if result.block {
        return ToolResult::blocked(result.block_reason);
    }
}
```

---

### Phase 3: Agent System (2-3 days)

**Goal**: Specialized subagents with custom prompts and tool restrictions.

**Files to Create**:
```
crates/cowork-core/src/prompt/
├── agents.rs        # Agent types and loading
└── builtin/
    ├── explore.md   # Built-in Explore agent
    ├── plan.md      # Built-in Plan agent
    ├── bash.md      # Built-in Bash agent
    └── general.md   # Built-in general-purpose agent
```

**Agent Definition Format**:
```yaml
---
name: code-architect
description: "Design feature architectures"
model: sonnet
color: blue
tools: Glob, Grep, Read, WebSearch
context: fork
max_turns: 30
---

# Code Architect

You are a senior software architect...
```

**Agent Types**:
```rust
pub struct AgentMetadata {
    pub name: String,
    pub description: String,
    pub model: ModelPreference,
    pub color: AgentColor,
    pub tools: ToolList,
    pub context: ContextMode,
    pub max_turns: Option<usize>,
}

pub struct AgentDefinition {
    pub metadata: AgentMetadata,
    pub system_prompt: String,
    pub source_path: PathBuf,
    pub scope: Scope,
}
```

**Tasks**:
- [ ] Define agent metadata structure
- [ ] Implement agent file parser (YAML + markdown)
- [ ] Create built-in agent definitions
- [ ] Implement agent discovery from filesystem
- [ ] Add model preference handling
- [ ] Implement context isolation (fork mode)
- [ ] Integrate with Task tool for subagent spawning
- [ ] Add unit tests

**Built-in Agents**:
| Agent | Model | Tools | Purpose |
|-------|-------|-------|---------|
| Explore | Haiku | Read-only | Fast codebase searching |
| Plan | Inherit | Read-only | Research and planning |
| Bash | Inherit | Bash, KillShell | Terminal operations |
| general-purpose | Inherit | All | Complex tasks |

---

### Phase 4: Enhanced Skills (1-2 days)

**Goal**: Upgrade existing skills with tool restrictions and auto-invocation.

**Files to Modify**:
```
crates/cowork-core/src/skills/
├── loader.rs        # Add tool restrictions, context mode
└── types.rs         # Enhanced skill metadata
```

**Enhanced Skill Format**:
```yaml
---
name: code-review
description: "Review code for quality and bugs"
disable-model-invocation: false
user-invocable: true
allowed-tools: Read, Grep, Glob, LSP
context: fork
agent: Explore
argument-hint: [files]
---

# Code Review Instructions
...
```

**Tasks**:
- [ ] Add tool restrictions to skill metadata
- [ ] Add context mode (fork/inherit)
- [ ] Add agent association
- [ ] Implement auto-invocation matching
- [ ] Add argument hints for autocomplete
- [ ] Update skill loader for new fields
- [ ] Migrate existing skills to new format
- [ ] Add unit tests

---

### Phase 5: Command System (2-3 days)

**Goal**: User-triggered workflow orchestration.

**Files to Create**:
```
crates/cowork-core/src/prompt/
└── commands.rs      # Command types and loading

.claude/commands/    # Default location
├── commit.md
├── review-pr.md
└── test.md
```

**Command Format**:
```markdown
# Git Commit

**Allowed Tools**: Bash(git:*)
**Description**: Create a git commit

## Context
!`git status`
!`git diff --cached`

## Instructions
1. Review changes
2. Create commit message
3. Execute commit
```

**Tasks**:
- [ ] Define command metadata structure
- [ ] Implement command file parser
- [ ] Create default commands (commit, review-pr)
- [ ] Implement shell substitution in commands
- [ ] Add command discovery from filesystem
- [ ] Add `/command` invocation support
- [ ] Add unit tests

---

### Phase 6: Prompt Builder (2-3 days)

**Goal**: Layered prompt composition with tool restriction intersection.

**Files to Create**:
```
crates/cowork-core/src/prompt/
├── builder.rs       # PromptBuilder implementation
└── pipeline.rs      # Full assembly pipeline
```

**PromptBuilder API**:
```rust
pub struct PromptBuilder {
    base_prompt: String,
    hook_context: Vec<String>,
    agent: Option<AgentDefinition>,
    skills: Vec<SkillDefinition>,
    command: Option<CommandDefinition>,
    restrictions: ToolRestrictions,
}

impl PromptBuilder {
    pub fn new(base: String) -> Self;
    pub fn with_hook_context(self, ctx: String) -> Self;
    pub fn with_agent(self, agent: AgentDefinition) -> Self;
    pub fn with_skill(self, skill: SkillDefinition) -> Self;
    pub fn with_command(self, cmd: CommandDefinition) -> Self;
    pub fn build(self, tools: &[ToolDefinition]) -> AssembledPrompt;
}
```

**Assembly Order**:
```
1. Base system prompt
2. + Hook-injected context (additionalContext)
3. + Agent system prompt
4. + Skill instructions
5. + Command content
6. → Filter tools by intersected restrictions
```

**Tasks**:
- [ ] Implement PromptBuilder with layered composition
- [ ] Implement tool restriction intersection
- [ ] Create AssembledPrompt output type
- [ ] Implement PromptPipeline for full orchestration
- [ ] Integrate with session/agent_loop
- [ ] Add unit tests

---

### Phase 7: Component Registry (1-2 days)

**Goal**: Unified component discovery and loading with scope priority.

**Files to Create**:
```
crates/cowork-core/src/prompt/
└── registry.rs      # ComponentRegistry implementation
```

**Registry API**:
```rust
pub struct ComponentRegistry {
    agents: HashMap<String, AgentDefinition>,
    skills: HashMap<String, SkillDefinition>,
    commands: HashMap<String, CommandDefinition>,
    hooks: HooksConfig,
}

impl ComponentRegistry {
    pub fn new() -> Self;
    pub fn load_from_paths(&mut self, paths: &ComponentPaths) -> Result<()>;
    pub fn get_agent(&self, name: &str) -> Option<&AgentDefinition>;
    pub fn get_skill(&self, name: &str) -> Option<&SkillDefinition>;
    pub fn get_command(&self, name: &str) -> Option<&CommandDefinition>;
    pub fn auto_invocable_skills(&self) -> impl Iterator<Item = &SkillDefinition>;
}
```

**Discovery Paths**:
```
1. Enterprise: $COWORK_ENTERPRISE_CONFIG/
2. Project:    ./.claude/
3. User:       ~/.claude/
4. Plugins:    ./.claude/plugins/*, ~/.claude/plugins/*
```

**Tasks**:
- [ ] Implement ComponentRegistry
- [ ] Implement filesystem discovery
- [ ] Add scope-based override logic
- [ ] Implement plugin loading
- [ ] Create ComponentPaths configuration
- [ ] Add unit tests

---

### Phase 8: Plugin System (2-3 days)

**Goal**: Package and distribute prompt components.

**Files to Create**:
```
crates/cowork-core/src/prompt/
└── plugins.rs       # Plugin loading and validation
```

**Plugin Structure**:
```
my-plugin/
├── .claude-plugin/
│   └── plugin.json
├── agents/
├── skills/
├── commands/
├── hooks/
│   └── hooks.json
└── hooks-handlers/
```

**Tasks**:
- [ ] Define plugin manifest format
- [ ] Implement plugin discovery
- [ ] Implement plugin validation
- [ ] Add plugin installation command
- [ ] Add plugin list/enable/disable commands
- [ ] Add unit tests

---

### Phase 9: Integration & Migration (2-3 days)

**Goal**: Integrate new prompt system with existing Cowork components.

**Files to Modify**:
```
crates/cowork-core/src/
├── session/agent_loop.rs    # Use PromptPipeline
├── orchestration/session.rs # Use PromptBuilder
├── tools/task/agent.rs      # Use AgentDefinition
└── config.rs                # Add prompt system config
```

**Tasks**:
- [ ] Replace static system prompt with PromptBuilder
- [ ] Integrate hook execution into agent loop
- [ ] Update Task tool to use AgentDefinition
- [ ] Add prompt config to main Config
- [ ] Update CLI for /command support
- [ ] Update UI for command/skill invocation
- [ ] Migration guide for existing users
- [ ] End-to-end tests

---

## Directory Structure (Final)

```
crates/cowork-core/src/prompt/
├── mod.rs              # Module exports
├── types.rs            # Core types
├── parser.rs           # YAML frontmatter parser
├── substitution.rs     # Shell/variable substitution
├── hooks.rs            # Hook types
├── hook_executor.rs    # Hook execution
├── agents.rs           # Agent system
├── skills.rs           # Enhanced skills (or modify existing)
├── commands.rs         # Command system
├── builder.rs          # PromptBuilder
├── pipeline.rs         # PromptPipeline
├── registry.rs         # ComponentRegistry
├── plugins.rs          # Plugin system
├── config.rs           # Prompt config
└── builtin/            # Built-in components
    ├── explore.md
    ├── plan.md
    ├── bash.md
    └── general.md

project/.claude/        # Project-level components
├── settings.json
├── agents/
├── skills/
├── commands/
├── hooks/
└── plugins/

~/.claude/              # User-level components
├── settings.json
├── agents/
├── skills/
├── commands/
└── plugins/
```

---

## Estimated Timeline

| Phase | Duration | Dependencies |
|-------|----------|--------------|
| Phase 1: Core Types | 1-2 days | None |
| Phase 2: Hook System | 2-3 days | Phase 1 |
| Phase 3: Agent System | 2-3 days | Phase 1 |
| Phase 4: Enhanced Skills | 1-2 days | Phase 1 |
| Phase 5: Command System | 2-3 days | Phase 1 |
| Phase 6: Prompt Builder | 2-3 days | Phases 2-5 |
| Phase 7: Component Registry | 1-2 days | Phases 3-5 |
| Phase 8: Plugin System | 2-3 days | Phase 7 |
| Phase 9: Integration | 2-3 days | All phases |

**Total: ~15-24 days**

---

## Quick Win: Minimal Viable Implementation

For a faster initial implementation, focus on:

1. **Phase 1**: Core types (2 days)
2. **Phase 3**: Agent system (2 days)
3. **Phase 6**: Basic prompt builder (2 days)
4. **Phase 9**: Basic integration (2 days)

**Minimal MVP: ~8 days**

This gives you:
- Agent definitions with YAML + markdown
- Basic prompt composition
- Tool restrictions per agent
- Subagent support via Task tool

Hooks, commands, and plugins can be added later.

---

## Success Criteria

1. **Extensibility**: Users can add agents/skills without code changes
2. **Security**: Tool restrictions properly enforced
3. **Discoverability**: Components auto-loaded from standard paths
4. **Composability**: Multiple skills can combine with correct restrictions
5. **Compatibility**: Existing skills continue to work

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Shell command injection | Sandbox hook execution, validate commands |
| Performance overhead | Lazy loading, caching parsed components |
| Breaking existing skills | Gradual migration, backwards compatibility |
| Complex debugging | Detailed logging, prompt inspection tools |

---

## Next Steps

1. Review this roadmap
2. Decide on MVP vs full implementation
3. Start with Phase 1 (core types)
4. Iterate based on feedback
