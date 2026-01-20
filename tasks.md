# CLI/UI Shared Logic Refactoring Tasks

## Overview
Extract duplicated logic between CLI (`cowork-cli`) and UI (`cowork-app`) into shared modules in `cowork-core`.

---

## Phase 1: Provider Infrastructure (Critical)

### 1.1 Add `FromStr` for `ProviderType` ✅ COMPLETED
- **File**: `crates/cowork-core/src/provider/genai_provider.rs`
- **Status**: Already implemented in genai_provider.rs with full provider support including aliases (google→Gemini, grok→XAI, zhipu→Zai)
- **Also Implemented**: `fn default_model(&self) -> &'static str` method
- **Updated**: CLI main.rs now uses `provider_str.parse::<ProviderType>()` (simplified from 17 lines to 4 lines)
- **Updated**: UI commands.rs now uses `.parse()` for ProviderType and `.default_model()` for default models
- **Tests Added**: `test_provider_type_from_str()` and `test_provider_type_display_roundtrip()` in provider_tests.rs

### 1.2 Create Provider Factory Module ✅ COMPLETED
- **File**: `crates/cowork-core/src/provider/factory.rs` (new)
- **Status**: Implemented all provider factory functions
- **Functions Added**:
  - `create_provider_from_config(config_manager, provider_type, model_override) -> Result<GenAIProvider>`
  - `create_provider_from_provider_config(config) -> Result<GenAIProvider>` (for UI use)
  - `create_provider_with_settings(provider_type, api_key, model) -> GenAIProvider`
  - `get_api_key(config_manager, provider_type) -> Option<String>`
  - `get_model_tiers(config_manager, provider_type) -> ModelTiers`
  - `has_api_key_configured(config_manager, provider_type) -> bool`
- **Updated**: CLI main.rs now uses shared functions (removed ~80 lines of duplicate code)
- **Updated**: UI chat.rs now uses `create_provider_from_provider_config()` and `create_provider_with_settings()`
- **Tests Added**: 4 tests in factory.rs

### 1.3 Add `FromStr` for `ApprovalLevel` ✅ COMPLETED
- **File**: `crates/cowork-core/src/approval/mod.rs`
- **Status**: Implemented `FromStr` and `Display` traits for ApprovalLevel
- **Updated**: UI commands.rs now uses `.parse::<ApprovalLevel>()` (simplified from 6 match arms to 3 lines)
- **Tests Added**: `test_approval_level_from_str()` and `test_approval_level_display_roundtrip()` in agentic_loop_tests.rs

---

## Phase 2: Tool Registry (Critical)

### 2.1 Create Shared Tool Registry Factory ✅ COMPLETED
- **File**: `crates/cowork-core/src/orchestration/tool_registry.rs` (new)
- **Status**: Implemented ToolRegistryBuilder with full customization support
- **Features**:
  - `ToolRegistryBuilder::new(workspace).with_provider().with_api_key().with_model_tiers().build()`
  - Toggle tool categories: `with_filesystem()`, `with_shell()`, `with_web()`, `with_browser()`, etc.
  - `create_standard_tool_registry(workspace, provider_type, api_key, model_tiers)` convenience function
- **Updated**: CLI main.rs now uses `create_standard_tool_registry()` (removed ~65 lines of duplicate code)
- **Tests Added**: 4 tests in tool_registry.rs

### 2.2 Create Standard Tool Definitions Function ✅ COMPLETED
- **File**: `crates/cowork-core/src/tools/mod.rs`
- **Status**: Implemented `standard_tool_definitions(workspace)` function
- **Updated**: UI chat.rs `default_tools()` now uses shared `standard_tool_definitions()`
- **Benefit**: UI now gets all 25+ tools instead of just 5 hardcoded ones

---

## Phase 3: Message & Context Handling (High)

### 3.1 Add Message Type Conversions ✅ COMPLETED
- **File**: `crates/cowork-core/src/context/mod.rs`
- **Status**: Implemented conversion methods on MessageRole and Message
- **Functions Added**:
  - `MessageRole::parse(s: &str) -> MessageRole` - Parse string role to enum
  - `MessageRole::as_str(&self) -> &'static str` - Convert enum to string
  - `MessageRole::Display` and `FromStr` traits
  - `Message::new(role, content)` - Create message with current timestamp
  - `Message::with_timestamp(role, content, timestamp)` - Create with specific timestamp
  - `Message::from_str_role(role, content, timestamp)` - Create from string role
  - `Message::role_str(&self) -> &'static str` - Get role as string
  - `messages_from_ui(messages, accessor)` - Generic converter for UI messages
- **Updated**: UI agentic_loop.rs uses `Message::from_str_role()` and `role.as_str()`
- **Updated**: UI commands.rs uses `Message::from_str_role()` and `role.as_str()`
- **Updated**: UI chat.rs uses `Message::from_str_role()`
- **Exports Added**: `Message`, `MessageRole`, `messages_from_ui` exported from lib.rs
- **Tests Added**: 10 tests in agentic_loop_tests.rs (message_conversion_tests module)

### 3.2 Extract Question Parsing Logic ✅ COMPLETED
- **File**: `crates/cowork-core/src/tools/interaction/ask_question.rs`
- **Status**: Implemented shared question parsing functions
- **Types Already Existed**: `Question`, `QuestionOption` (added serde camelCase rename)
- **Functions Added**:
  - `parse_questions(args) -> Result<Vec<Question>, String>` - Strict parser with validation
  - `parse_questions_lenient(args) -> Result<Vec<Question>, String>` - Forgiving parser
  - `validate_questions(questions) -> Result<(), String>` - Validation helper
  - `format_answer_response(answers) -> Value` - Format answers JSON
  - `format_answer_response_with_id(id, answers) -> Value` - Format with request ID
- **Exports Added**: All types and functions exported from `tools::interaction` module
- **Tests Added**: 10 tests in agentic_loop_tests.rs (question_parsing_tests module)

---

## Phase 4: Formatting Utilities (High)

### 4.1 Create Shared Formatting Module ✅ COMPLETED
- **File**: `crates/cowork-core/src/orchestration/formatting.rs` (new)
- **Status**: Implemented all formatting functions with comprehensive tests
- **Functions Implemented**:
  - `format_tool_result()` - Routes to appropriate formatter
  - `format_directory_result()` - Directory listing format
  - `format_glob_result()` - File search results
  - `format_grep_result()` - Code search matches
  - `format_file_content()` - File content preview
  - `format_command_result()` - Shell output format
  - `format_status_result()` - Success/error messages
  - `format_generic_json()` - Auto-detect and format
  - `format_size()` - Bytes to human-readable
  - `truncate_result()` - Safe string truncation
- **Updated**: CLI main.rs now uses shared formatting functions (~213 lines removed)
- **Exports Added**: All formatting functions exported from `orchestration` and `lib.rs`
- **Tests Added**: 13 tests in formatting.rs

---

## Phase 5: Configuration Helpers (Medium)

### 5.1 Add API Key Validation to ConfigManager ✅ COMPLETED (Previously)
- **File**: `crates/cowork-core/src/config.rs`
- **Status**: Methods already exist in ConfigManager
- **Existing Methods**:
  - `has_api_key(&self) -> bool` - Check default provider
  - `has_api_key_for(&self, provider_name: &str) -> bool` - Check specific provider
  - `get_api_key(&self) -> Option<String>` - Get default provider key
  - `get_api_key_for(&self, provider_name: &str) -> Option<String>` - Get specific provider key

### 5.2 Centralize Default Constants ✅ COMPLETED
- **File**: `crates/cowork-core/src/config.rs`
- **Status**: Added `defaults` module with common constants
- **Constants Added**:
  - `COMMAND_TIMEOUT_SECS: u64 = 30`
  - `MAX_AGENTIC_ITERATIONS: usize = 100`
  - `DEFAULT_APPROVAL_LEVEL: &str = "low"`
  - `HISTORY_FILE_NAME: &str = "history.txt"`
  - `DEFAULT_MAX_TOKENS: u32 = 4096`
  - `DEFAULT_TEMPERATURE: f32 = 0.7`
  - `DEFAULT_PROVIDER: &str = "anthropic"`
  - `SESSION_DIR_NAME: &str = ".cowork"`
  - `MAX_CONTEXT_SIZE: usize = 100_000`
  - `BROWSER_TIMEOUT_SECS: u64 = 30`
  - `DEFAULT_SEARCH_RESULTS: usize = 50`
- **Exports Added**: `defaults` module exported from lib.rs

---

## Phase 6: Cleanup (Low)

### 6.1 Remove Duplicated Code from CLI ✅ COMPLETED
Removed/updated from `crates/cowork-cli/src/main.rs`:
- [x] `parse_provider_type()` - Now uses `ProviderType::from_str()` (Phase 1.1)
- [x] `create_provider_from_config()` - Now uses shared factory (Phase 1.2)
- [x] `get_api_key()` - Now uses shared function (Phase 1.2)
- [x] `get_model_tiers()` - Now uses shared function (Phase 1.2)
- [x] `has_api_key_configured()` - Now uses shared function (Phase 1.2)
- [x] `create_tool_registry()` - Now uses `create_standard_tool_registry()` (Phase 2.1)
- [x] Format functions - Now uses shared formatting module (Phase 4.1)

### 6.2 Remove Duplicated Code from UI ✅ COMPLETED (Previously)
Updated in `crates/cowork-app/src/`:
- [x] `chat.rs`: Uses `create_provider_from_provider_config()` (Phase 1.2)
- [x] `chat.rs`: Uses `create_provider_with_settings()` (Phase 1.2)
- [x] `chat.rs`: Uses `standard_tool_definitions()` (Phase 2.2)
- [x] `agentic_loop.rs`: Uses shared Message types (Phase 3.1)
- [x] `agentic_loop.rs`: Uses shared question parsing (Phase 3.2)
- [x] `commands.rs`: Uses `ApprovalLevel::from_str()` (Phase 1.3)
- [x] `commands.rs`: Uses `ProviderType::from_str()` (Phase 1.1)

### 6.3 Update Exports ✅ COMPLETED
- **File**: `crates/cowork-core/src/lib.rs`
- All shared modules exported:
  - Provider factory functions
  - Tool registry functions and builder
  - Formatting functions
  - Message types and conversions
  - Question parsing utilities
  - Default constants

---

## Testing Checklist

After each phase:
- [x] `cargo check --workspace` passes
- [x] `cargo test -p cowork-core` passes (306 tests, 0 failures)
- [x] `cargo run -p cowork-cli -- --help` works
- [x] `cargo run -p cowork-cli -- tools` shows all 26 tools
- [x] UI app builds and basic chat works

---

## Estimated Impact

| Phase | Files Changed | Lines Removed | Lines Added | Priority | Status |
|-------|---------------|---------------|-------------|----------|--------|
| 1     | 5             | ~100          | ~150        | Critical | ✅ DONE |
| 2     | 4             | ~150          | ~100        | Critical | ✅ DONE |
| 3     | 4             | ~80           | ~60         | High     | ✅ DONE |
| 4     | 2             | ~220          | ~250        | High     | ✅ DONE |
| 5     | 3             | ~30           | ~40         | Medium   | ✅ DONE |
| 6     | 3             | ~50           | ~10         | Low      | ✅ DONE |

**Total**: ~630 lines of duplication removed, replaced with ~610 lines of shared code (net reduction + better maintainability)

## Summary

All phases have been completed! The refactoring has:
1. Unified provider type handling with `FromStr` and `Display` traits
2. Created a shared provider factory for consistent provider creation
3. Added `FromStr` for `ApprovalLevel` to simplify parsing
4. Created a `ToolRegistryBuilder` for flexible tool registry construction
5. Added shared tool definitions via `standard_tool_definitions()`
6. Added message type conversions and shared question parsing
7. Created a comprehensive formatting module for tool result display
8. Centralized default constants in a `defaults` module
9. Updated all exports in lib.rs for easy access
