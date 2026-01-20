# Cowork (Rust Claude Code Clone) - Feature Completion Tasks

This file tracks all features that need to be implemented, fixed, or verified to achieve feature parity with Claude Code.

---

## CRITICAL: Missing/Broken Features

### 1. Document Processing ✅ COMPLETED
- [x] **Implement PDF parsing** (`crates/cowork-core/src/tools/document/read_pdf.rs`)
  - Implemented using `pdf-extract` crate
  - Supports page range filtering (e.g., "1-5", "all")
  - Proper error handling for missing files, wrong extensions
  - Unit tests added

- [x] **Implement Office document parsing** (`crates/cowork-core/src/tools/document/read_office.rs`)
  - Word (.docx): Using `dotext` crate for text extraction
  - Excel (.xlsx, .xls): Using `calamine` crate with full cell type support
  - PowerPoint (.pptx): Custom XML parser using `quick-xml` and `zip`
  - Document tools registered in CLI
  - Unit tests added in `tests/document_tests.rs`

### 2. Browser Tools ✅ REGISTERED
- [x] **Register browser tools in CLI** (`crates/cowork-cli/src/main.rs`)
  - BrowserController.create_tools() used to register all browser tools
  - Registered: `browser_navigate`, `browser_click`, `browser_type`, `browser_screenshot`, `browser_get_page_content`
  - Tools added to show_tools() display and system prompt
  - Browser read-only tools (screenshot, get_page_content) auto-approved

### 3. LSP Operations ✅ COMPLETED
- [x] **Implement `prepareCallHierarchy`** (`crates/cowork-core/src/tools/lsp/client.rs`)
- [x] **Implement `incomingCalls`** (`crates/cowork-core/src/tools/lsp/client.rs`)
- [x] **Implement `outgoingCalls`** (`crates/cowork-core/src/tools/lsp/client.rs`)
  - Added CallHierarchyPrepare, CallHierarchyIncomingCalls, CallHierarchyOutgoingCalls support
  - Full LSP protocol implementation with lsp-types crate
  - Proper formatting of call hierarchy items with file locations

### 4. Plan Mode ✅ INTEGRATED
- [x] **Implement plan mode state tracking** in agentic loop
  - `ExitPlanMode` tool with shared PlanModeState
  - Added `EnterPlanMode` tool (`crates/cowork-core/src/tools/planning/enter_plan_mode.rs`)
  - PlanModeState tracks: active, plan_file, allowed_prompts
  - Both tools registered in CLI with shared state
  - Tools documented in show_tools() and system prompt

### 5. AskUserQuestion ✅ COMPLETED
- [x] **Register AskUserQuestion tool in CLI** (`crates/cowork-cli/src/main.rs`)
  - Tool registered in `create_tool_registry()` function
  - Supports 1-4 questions with 2-4 options each
  - Multi-select support, validation, async channels

### 6. Context Management Not Used
- [ ] **Inject CLAUDE.md context into system prompt**
  - Context gathering implemented (`context/mod.rs`) but not used
  - Modify CLI to load and inject 4-tier memory hierarchy
  - Include: Enterprise, Project, Rules, User CLAUDE.md files
  - Add context summarization when token limit approached

---

## HIGH PRIORITY: Robustness Issues

### 7. Error Handling Improvements
- [ ] **Replace 75+ instances of `.unwrap()/.expect()` with proper error handling**
  - `crates/cowork-core/src/config.rs` - config loading
  - `crates/cowork-core/src/mcp_manager.rs` - MCP operations
  - `crates/cowork-core/src/skills/` modules - skill loading
  - Use `?` operator with proper error types
  - Add context to errors with `anyhow` or custom error types

### 8. Web Search Configuration
- [ ] **Add web search API configuration guidance**
  - Currently returns placeholder when no endpoint configured
  - Document how to set up search API
  - Add example config in `config/default.toml`
  - Show helpful error message with setup instructions

### 9. MCP Tool Integration
- [ ] **Auto-register MCP tools with tool registry**
  - MCP servers can be managed but their tools aren't auto-registered
  - Add MCP tool discovery and registration on server start
  - Integrate MCP tool calls into main agentic loop

---

## MEDIUM PRIORITY: Feature Completeness

### 10. Test Coverage
- [ ] **Add tests for browser tools**
- [ ] **Add tests for document tools** (once implemented)
- [ ] **Add tests for planning mode flow**
- [ ] **Add integration tests for tool approval flow**
- [ ] **Add tests for multi-turn agentic loops** (with mock LLM)
- [ ] **Add tests for MCP integration**
- [ ] **Add tests for context management**

### 11. Browser Tool Fallbacks
- [ ] **Improve fallback messages when browser feature disabled**
  - Currently some tools have simulation-only fallbacks
  - Clearly inform user that browser feature is required
  - Provide instructions to enable browser feature

### 12. Skills/Tools Unification
- [ ] **Document relationship between Skills and Tools**
  - Skills = slash commands (user invoked)
  - Tools = LLM callable functions
  - Some overlap is confusing
  - Either unify or document clearly

---

## LOW PRIORITY: Polish

### 13. Structured Tool Results
- [ ] **Consider structured tool result passing** instead of string-based
  - Current: Tool results passed as strings in agentic loop
  - Could improve with typed result objects

### 14. Feature Flag Consistency
- [ ] **Audit feature flags for completeness**
  - `browser` feature enabled by default but tools not registered
  - `lsp` feature enabled but some operations unimplemented
  - Either disable incomplete features or finish implementations

### 15. Documentation
- [ ] **Add inline documentation for public APIs**
- [ ] **Update PROJECT_STRUCTURE.md with current state**
- [ ] **Add CONTRIBUTING.md with development setup**

---

## Verification Checklist

After implementing, verify each feature works end-to-end:

### Filesystem Tools
- [ ] `read_file` - Read file with offset/limit
- [ ] `write_file` - Create and overwrite files
- [ ] `edit` - Surgical string replacement
- [ ] `glob` - Pattern matching
- [ ] `grep` - Regex content search
- [ ] `list_directory` - Directory listing
- [ ] `search_files` - File search
- [ ] `delete_file` - File deletion
- [ ] `move_file` - File moving

### Shell Tools
- [ ] `execute_command` - Run shell commands
- [ ] `kill_shell` - Kill background processes
- [ ] Command blocklist works (blocks dangerous commands)

### Web Tools
- [ ] `web_fetch` - Fetch URL content
- [ ] `web_search` - Search with configured API

### Browser Tools ✅ REGISTERED
- [x] `browser_navigate` - Navigate to URL
- [x] `browser_click` - Click elements
- [x] `browser_type` - Type text
- [x] `browser_screenshot` - Take screenshots
- [x] `browser_get_page_content` - Get page content

### LSP Tools ✅ IMPLEMENTED
- [x] `goToDefinition` - Find definition
- [x] `findReferences` - Find all references
- [x] `hover` - Get hover info
- [x] `documentSymbol` - Get document symbols
- [x] `workspaceSymbol` - Search workspace symbols
- [x] `goToImplementation` - Find implementations
- [x] `prepareCallHierarchy` - Get call hierarchy
- [x] `incomingCalls` - Find callers
- [x] `outgoingCalls` - Find callees

### Document Tools ✅ IMPLEMENTED
- [x] `read_pdf` - Extract PDF text (using pdf-extract)
- [x] `read_office` - Extract Office doc text (docx, xlsx, pptx)

### Task Tools
- [ ] `todo_write` - Track todos
- [ ] `task` - Launch subagents
- [ ] `task_output` - Get agent output

### Planning Tools ✅ INTEGRATED
- [x] `enter_plan_mode` - Enter planning mode
- [x] `exit_plan_mode` - Exit with approval

### Interaction Tools ✅ REGISTERED
- [x] `ask_user_question` - Interactive questions (registered in CLI)

### Context Management
- [ ] CLAUDE.md files loaded and injected
- [ ] Context summarization works
- [ ] Token counting accurate

### MCP Integration
- [ ] MCP server start/stop
- [ ] MCP tool discovery
- [ ] MCP tool execution in agentic loop

---

## Running Tests

```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test filesystem_tests
cargo test --test shell_tests
cargo test --test agentic_loop_tests

# Run with all features
cargo test --all-features

# Run with verbose output
cargo test -- --nocapture
```

## Build Commands

```bash
# Development build
cargo build

# Release build
cargo build --release

# Build with all features
cargo build --all-features

# Run CLI
cargo run --bin cowork -- --help
cargo run --bin cowork -- "your prompt here"
```
