# Prompt System Full Implementation Tasks

Implement a complete Claude Code-style prompt system for Cowork. Follow the design in `docs/PROMPT_SYSTEM_ROADMAP.md` and `docs/PROMPT_SYSTEM_IMPLEMENTATION.md`.

Built-in prompt files are already created in `crates/cowork-core/src/prompt/builtin/`.

---

## Phase 1: Core Types & Parser ✅ COMPLETE

**Goal**: Establish foundational data structures and parsing.

**Files to create**: `src/prompt/types.rs`, `src/prompt/parser.rs`, `src/prompt/substitution.rs`

- [x] Create `ToolSpec` enum with pattern matching
  - `Name(String)` - allow all uses of a tool
  - `Pattern(String)` - parse patterns like `Bash(git:*)`, `Write(src/*:*)`
  - Implement `matches(tool_name: &str, args: &Value) -> bool`
- [x] Create `ToolRestrictions` struct
  - `allowed: Vec<ToolSpec>` and `denied: Vec<ToolSpec>`
  - Implement `is_allowed(tool: &str, args: &Value) -> bool`
  - Implement `intersect(&self, other: &Self) -> Self` for combining restrictions
- [x] Create `Scope` enum for priority hierarchy
  - Enterprise = 0, Project = 1, User = 2, Plugin = 3, Builtin = 4
- [x] Create `ModelPreference` enum (Inherit, Opus, Sonnet, Haiku, Custom)
- [x] Implement YAML frontmatter parser in `parser.rs`
  - Parse `---\nkey: value\n---` at start of markdown files
  - Return `(HashMap<String, Value>, String)` tuple (metadata, content)
- [x] Implement shell command substitution in `substitution.rs`
  - Parse `` !`command` `` syntax in prompts
  - Execute command and replace with output
  - Handle timeouts and errors gracefully
- [x] Add comprehensive unit tests for all types (75 tests)

---

## Phase 2: Hook System ✅ COMPLETE

**Goal**: Event-based prompt injection mechanism.

**Files created**: `src/prompt/hooks.rs`, `src/prompt/hook_executor.rs`

- [x] Define `HookEvent` enum
  - SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop, SubagentStop, PreCompact, Notification
- [x] Define `HookMatcher` for tool-specific hooks
  - Match by tool name, tool pattern, or `*` for all
- [x] Define `HookHandler` enum
  - `Command { command: String, timeout_ms: Option<u64> }`
  - `Prompt { content: String }`
  - `McpTool { server: String, tool: String, args: Value }`
- [x] Define `HookResult` struct
  - `additional_context: Option<String>` - injected into prompt
  - `block: bool` - block the action
  - `block_reason: Option<String>`
  - `modified_args: Option<Value>` - for PreToolUse
- [x] Define `HooksConfig` for loading from hooks.json
- [x] Implement `HookExecutor`
  - `execute(event: HookEvent, config: &HooksConfig, context: &HookContext) -> Vec<Result<HookResult, HookError>>`
  - Execute shell commands with JSON result parsing
  - Inject environment variables (CLAUDE_HOOK_EVENT, CLAUDE_SESSION_ID, CLAUDE_TOOL_NAME, etc.)
- [x] Implement hooks.json file loader (`load_hooks_config`, `load_hooks_from_paths`)
- [x] Integrate hooks into `session/agent_loop.rs` (deferred to Phase 9: Integration)
  - Call SessionStart hooks at session begin
  - Call PreToolUse/PostToolUse around tool execution
  - Call Stop hooks when agent finishes
- [x] Add comprehensive unit tests for hook system (41 new tests, 116 total)

---

## Phase 3: Agent System ✅ COMPLETE

**Goal**: Specialized subagents with custom prompts and tool restrictions.

**Files created**: `src/prompt/agents.rs`

- [x] Define `AgentMetadata` struct
  - name, description, model, color, tools, context, max_turns
- [x] Define `AgentDefinition` struct
  - metadata, system_prompt, source_path, scope
- [x] Define `ContextMode` enum (Fork, Inherit)
- [x] Define `AgentColor` enum for UI display
- [x] Implement agent markdown file parser
  - Parse YAML frontmatter for metadata
  - Extract markdown content as system prompt
  - Validate required fields
- [x] Load built-in agents from `prompt::builtin::agents::*`
  - Explore (Haiku, read-only tools)
  - Plan (Inherit, read-only tools)
  - Bash (Inherit, Bash/KillShell only)
  - general-purpose (Inherit, all tools)
- [x] Create `AgentRegistry` to manage agents
  - `register(agent: AgentDefinition)`
  - `get(name: &str) -> Option<&AgentDefinition>`
  - `list() -> Vec<&AgentDefinition>`
- [x] Implement agent discovery from filesystem
  - Load from `.claude/agents/`, `~/.claude/agents/`
  - Apply scope-based override logic
- [x] Update `tools/task/executor.rs` to use AgentDefinition (deferred to Phase 9: Integration)
  - Load agent by subagent_type parameter via `get_agent_from_registry()`
  - Apply agent's tool restrictions
  - Use agent's system prompt via `get_system_prompt_dynamic()`
  - Respect agent's model preference via `get_agent_model_preference()`
- [x] Add unit tests for agent system (30 new tests, 146 total prompt tests)

---

## Phase 4: Enhanced Skills System ✅ COMPLETE

**Goal**: Upgrade existing skills with tool restrictions and auto-invocation.

**Files modified**: `src/skills/loader.rs`

- [x] Extend `SkillFrontmatter` (SkillMetadata) with new fields
  - `allowed_tools: ToolList` (already existed)
  - `denied_tools: ToolList` - explicitly deny tools
  - `context: Option<String>` - ContextMode (fork/inherit)
  - `agent: Option<String>` - run skill in specific agent
  - `disable_model_invocation: bool` - prevent auto-invocation
  - `auto_triggers: Vec<String>` - patterns for auto-invocation
  - `argument_hint: Vec<String>` - CLI autocomplete hints
- [x] Update skill YAML parser to handle new fields
- [x] Implement skill auto-invocation matching
  - `matches_auto_trigger()` matches user input against skill triggers
  - Respects `disable_model_invocation` flag
- [x] Implement skill tool restrictions
  - `tool_restrictions()` returns ToolRestrictions based on allowed/denied lists
  - Integration with agent restrictions via PromptBuilder
- [x] Backward compatibility maintained
  - All new fields have defaults
  - Existing skills without new fields work correctly
- [x] Add argument hints support
- [x] Add unit tests for enhanced skills (10 new tests, 19 total)

---

## Phase 5: Command System ✅ COMPLETE

**Goal**: User-triggered workflow orchestration.

**Files created**: `src/prompt/commands.rs`, `src/prompt/builtin/commands/*.md`

- [x] Define `CommandMetadata` struct
  - name, description, allowed_tools, denied_tools, argument_hint
- [x] Define `CommandDefinition` struct
  - metadata, content (with shell substitutions), source_path, scope
- [x] Implement command markdown parser
  - Parse metadata from YAML frontmatter
  - Extract command content
  - Support $ARGUMENTS and ${ARGUMENTS} substitution
- [x] Create default built-in commands
  - `/commit` - create git commit with AI-generated message
  - `/pr` - create pull request on GitHub
  - `/review-pr` - review a pull request
- [x] Implement command discovery from filesystem
  - Load from `.claude/commands/`, `~/.claude/commands/`
  - Scope-based override logic (Project > User > Builtin)
- [x] Create `CommandRegistry`
  - `register(cmd: CommandDefinition)`
  - `get(name: &str) -> Option<&CommandDefinition>`
  - `list() -> Vec<&CommandDefinition>`
  - `with_builtins()` - load built-in commands
  - `discover()` - discover from standard locations
  - `parse_invocation()` - parse /command input
  - `execute()` - expand command with arguments
- [x] Implement `/command` invocation parsing
  - Parse `/command` from user input
  - Extract command name and arguments
  - Substitute arguments in command content
- [x] Integrate into CLI (deferred to Phase 9: Integration)
- [x] Add unit tests for command system (38 new tests, 185 total prompt tests)

---

## Phase 6: Prompt Builder ✅ COMPLETE

**Goal**: Layered prompt composition with tool restriction intersection.

**Files created**: `src/prompt/builder.rs`, `src/prompt/pipeline.rs`

- [x] Create `PromptBuilder` struct
  - base_prompt: String
  - hook_contexts: Vec<String>
  - agent: Option<AgentDefinition>
  - skills: Vec<SkillDefinition>
  - command: Option<CommandDefinition>
  - additional_restrictions: Vec<ToolRestrictions>
  - template_vars: Option<TemplateVars>
- [x] Implement builder methods
  - `new(base: String) -> Self`
  - `empty() -> Self`
  - `with_base_prompt(self, prompt: String) -> Self`
  - `with_hook_context(self, ctx: String) -> Self`
  - `with_hook_contexts(self, ctxs: impl IntoIterator) -> Self`
  - `with_agent(self, agent: AgentDefinition) -> Self`
  - `with_skill(self, skill: SkillDefinition) -> Self`
  - `with_skills(self, skills: impl IntoIterator) -> Self`
  - `with_command(self, cmd: CommandDefinition, args: String) -> Self`
  - `with_environment(self, vars: &TemplateVars) -> Self`
  - `with_restrictions(self, restrictions: ToolRestrictions) -> Self`
  - `build(self) -> AssembledPrompt`
- [x] Create `AssembledPrompt` struct
  - system_prompt: String
  - tool_restrictions: ToolRestrictions
  - model: ModelPreference
  - max_turns: Option<usize>
  - metadata: AssemblyMetadata
- [x] Create `AssemblyMetadata` struct for tracking assembly info
  - has_agent, agent_name, has_command, command_name
  - skill_count, hook_context_count
- [x] Create `SkillDefinition` struct for builder-friendly skill representation
  - Implements `From<&DynamicSkill>` for easy conversion
- [x] Implement prompt assembly order
  1. Base system prompt
  2. + Hook-injected context
  3. + Agent system prompt
  4. + Skill instructions
  5. + Command content (with argument substitution)
  6. → Template variable substitution
  7. → Filter tools by intersected restrictions
- [x] Implement tool restriction intersection
  - Combine restrictions from agent, skills, command, and additional
  - Most restrictive wins (intersection, not union)
  - Uses existing `ToolRestrictions::intersect()` method
- [x] Create `PromptPipeline` for full orchestration
  - `PipelineConfig` for configuration
  - `HookResults` for aggregating hook execution results
  - `ProcessedInput` for processed user input
  - `InputType` enum (Regular, Command, Skill)
  - Coordinate hooks, agents, skills, commands
  - `run_session_start_hooks()`, `run_user_prompt_hooks()`
  - `run_pre_tool_hooks()`, `run_post_tool_hooks()`, `run_stop_hooks()`
  - `build_command_prompt()`, `build_agent_prompt()`, `build_skill_prompt()`
  - `process_input()` - main entry point for processing user input
- [x] Add unit tests for prompt builder (42 new tests, 227 total prompt tests)

---

## Phase 7: Component Registry ✅ COMPLETE

**Goal**: Unified component discovery and loading with scope priority.

**Files created**: `src/prompt/registry.rs`

- [x] Create `ComponentPaths` struct
  - enterprise_path: Option<PathBuf>
  - project_path: PathBuf (`.claude/`)
  - user_path: PathBuf (`~/.claude/`)
  - plugin_paths: Vec<PathBuf>
  - Helper methods: `for_project()`, `user_only()`, `iter_by_priority()`
- [x] Create `ComponentRegistry` struct
  - agents: HashMap<String, AgentDefinition>
  - skills: HashMap<String, DynamicSkill>
  - commands: HashMap<String, CommandDefinition>
  - hooks: HooksConfig
- [x] Implement `ComponentRegistry` methods
  - `new() -> Self`
  - `with_builtins() -> Self` - load built-in agents and commands
  - `load_from_paths(&mut self, paths: &ComponentPaths) -> Result<LoadResult>`
  - `get_agent(&self, name: &str) -> Option<&AgentDefinition>`
  - `get_skill(&self, name: &str) -> Option<&DynamicSkill>`
  - `get_command(&self, name: &str) -> Option<&CommandDefinition>`
  - `get_hooks(&self) -> &HooksConfig`
  - `list_agents()`, `list_skills()`, `list_commands()`
  - `agent_names()`, `skill_names()`, `command_names()`
  - `register_agent()`, `register_skill()`, `register_command()`, `merge_hooks()`
  - `auto_invocable_skills()` - skills that can be auto-invoked
  - `user_invocable_skills()` - skills for /command completion
  - `find_matching_skills()` - find skills by auto-trigger patterns
  - `to_agent_registry()`, `to_command_registry()` - conversion helpers
- [x] Implement filesystem discovery
  - Scan directories for agent/skill/command files
  - Load from `{path}/agents/*.md`, `{path}/skills/*/SKILL.md`, `{path}/commands/*.md`
  - Load hooks from `{path}/hooks/hooks.json`
  - Parse and validate each component
  - Handle parse errors gracefully (log and skip)
- [x] Implement scope-based override logic
  - Priority order: Builtin < Plugin < User < Project < Enterprise
  - Components with same name: use higher priority
- [ ] Add hot-reload capability (optional, deferred)
  - Watch for file changes
  - Reload affected components
- [x] Add unit tests for registry (31 new tests, 289 total prompt tests)

---

## Phase 8: Plugin System ✅ COMPLETE

**Goal**: Package and distribute prompt components.

**Files created**: `src/prompt/plugins.rs`

- [x] Define plugin manifest format (`plugin.json`)
  ```json
  {
    "name": "my-plugin",
    "version": "1.0.0",
    "description": "Plugin description",
    "author": "Author Name",
    "agents": ["agents/*.md"],
    "skills": ["skills/*/SKILL.md"],
    "commands": ["commands/*.md"],
    "hooks": "hooks/hooks.json"
  }
  ```
- [x] Create `PluginManifest` struct
  - name, version, description, author, agents, skills, commands, hooks
  - enabled, homepage, license, min_cowork_version, keywords
  - `parse()`, `load()`, `validate()` methods
- [x] Create `Plugin` struct
  - manifest, base_path, agents, skills, commands, hooks
  - `load()`, `load_components()`, `load_agents()`, `load_skills()`, `load_commands()`, `load_hooks()`
  - `name()`, `version()`, `description()`, `is_enabled()`, `component_count()`
- [x] Implement `PluginRegistry` for managing plugins
  - `new()`, `discover()`, `load_plugin()`, `get()`, `list()`, `names()`, `count()`
  - `contains()`, `is_disabled()`, `disabled_reason()`, `enable()`, `disable()`, `unload()`
  - `all_agents()`, `all_skills()`, `all_commands()`, `merged_hooks()`
- [x] Implement plugin discovery
  - Scan `.claude/plugins/` and `~/.claude/plugins/`
  - Load plugin.json from each directory
  - Handle missing manifests, disabled plugins, conflicts
- [x] Implement plugin loading
  - Parse manifest with JSON
  - Load all referenced components via glob patterns
  - Log warnings for failed component loads
- [x] Implement plugin validation
  - Check required fields (name, version)
  - Validate plugin name format
  - Check for conflicts with existing components
- [x] Integrate with ComponentRegistry
  - `load_plugins()` method to load from plugin directories
  - Plugins loaded during `load_from_paths()` if plugin_paths present
  - `plugins()`, `plugins_mut()`, `get_plugin()`, `list_plugins()`, `plugin_count()`
  - Components from plugins registered with Plugin scope
- [x] Add CLI commands for plugin management (deferred to Phase 9: Integration)
  - `cowork plugin list` - list installed plugins
  - `cowork plugin info <name>` - show plugin details
  - `cowork plugin enable/disable <name>`
- [x] Add unit tests for plugin system (42 new tests, 300 total prompt tests)

---

## Phase 9: Integration & Migration ✅ COMPLETE

**Goal**: Integrate new prompt system with existing Cowork components.

**Files to modify**: Multiple existing files

- [x] Update `orchestration/system_prompt.rs`
  - Already has PromptBuilder integration
  - Uses `builtin::SYSTEM_PROMPT` as base
  - Template variable substitution implemented
- [x] Update `session/types.rs`
  - Added PromptSystemConfig to SessionConfig
  - Added ComponentRegistry support to SessionConfig
- [x] Update `session/agent_loop.rs`
  - Integrated hook execution at lifecycle points
  - SessionStart hooks on session begin
  - UserPromptSubmit hooks on user message
  - PreToolUse/PostToolUse hooks around tool execution
  - Hook blocking support implemented
- [x] Update `tools/task/executor.rs`
  - Load agent definitions from registry via `get_agent_from_registry()`
  - Dynamic system prompts via `get_system_prompt_dynamic()`
  - Model preferences via `get_agent_model_preference()`
  - Registry support in AgentExecutionConfig
- [x] Update `config.rs`
  - Add PromptSystemConfig with hooks, plugins, paths
  - Support custom component paths
- [x] Update CLI for new features
  - Slash commands already handled via SkillRegistry
  - Added `cowork plugin list/info/enable/disable` commands
  - Added `cowork components agents/commands/skills/all` commands
- [x] Create migration guide for existing users
  - Created `docs/MIGRATION_GUIDE.md`
  - Documented all new features
  - No breaking changes (fully backward compatible)
- [x] Add end-to-end integration tests
  - Test full prompt assembly (24 tests in prompt_system_tests.rs)
  - Test hook execution
  - Test agent spawning with restrictions
  - Test command invocation

---

## Final Verification Checklist

- [x] `cargo build` succeeds with no errors
- [x] `cargo test` passes all tests (including new tests) - 313 prompt tests pass
- [x] `cargo clippy` has no warnings
- [x] Cowork starts successfully with new prompt system - CLI builds and runs
- [x] Built-in agents work (Explore, Plan, Bash, general-purpose) - tested in registry tests
- [x] Custom agents can be loaded from `.claude/agents/` - tested in registry tests
- [x] Skills work with tool restrictions - tested in skills tests
- [x] Commands can be invoked with `/command` - slash commands work via SkillRegistry
- [x] Hooks execute at correct lifecycle points - tested in hook tests
- [x] Plugins can be installed and loaded - tested in plugin tests
- [x] Tool restrictions are properly enforced - tested in types and builder tests
- [x] Template variables are substituted correctly - tested in builder tests
- [x] Backward compatibility with existing skills - tested in skills tests

---

## Reference Files

- Design: `docs/CLAUDE_CODE_PROMPT_SYSTEM.md`
- Implementation details: `docs/PROMPT_SYSTEM_IMPLEMENTATION.md`
- Roadmap: `docs/PROMPT_SYSTEM_ROADMAP.md`
- Built-in prompts: `crates/cowork-core/src/prompt/builtin/`
