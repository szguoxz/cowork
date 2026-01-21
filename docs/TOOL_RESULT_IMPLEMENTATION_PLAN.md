# Implementation Plan: Tool Result Handling & Subagent Strategy

## Executive Summary

Align cowork with Claude Code's behavior for:
1. Tool result message format (proper `tool_result` content blocks)
2. Subagent delegation strategy (when to use Task vs direct tools)

---

## Gap Analysis

### Current State vs Claude Code

| Aspect | Claude Code | Cowork Current | Gap |
|--------|-------------|----------------|-----|
| Tool result format | `{type: "tool_result", tool_use_id, content}` | Plain text: `[Tool result for {id}]...` | **Major** |
| Tool result role | USER message with content blocks | USER message with string | **Major** |
| Multiple tool results | Single USER message with array | Separate messages | **Medium** |
| Subagent context | Fresh, isolated | Fresh, isolated | OK |
| Subagent nesting | Prevented (no Task in subagents) | Prevented | OK |
| When to use Task | Detailed system prompt guidance | Basic prompts | **Medium** |
| Error results | `is_error: true` flag | Error in string content | **Minor** |

---

## Implementation Plan

### Phase 1: Fix Tool Result Content Block Format

**Goal**: Send tool results as proper `tool_result` content blocks, not plain text.

#### 1.1 Add ContentBlock Types

**File**: `crates/cowork-core/src/provider/mod.rs`

Add content block enum to represent different content types:

```rust
/// Content block types for messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text content
    Text { text: String },
    /// Tool use request from assistant
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result from user
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Updated LlmMessage with content blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    /// Content can be string or array of content blocks
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}
```

#### 1.2 Update ChatMessage Tool Result Creation

**File**: `crates/cowork-core/src/orchestration/session.rs`

Change `tool_result()` to create proper content blocks:

```rust
/// Create a tool result message with proper content block
pub fn tool_result(tool_call_id: &str, result: &str, is_error: bool) -> Self {
    Self {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: String::new(), // Empty - use content_blocks instead
        content_blocks: vec![ContentBlock::ToolResult {
            tool_use_id: tool_call_id.to_string(),
            content: result.to_string(),
            is_error: if is_error { Some(true) } else { None },
        }],
        tool_calls: Vec::new(),
        timestamp: Utc::now(),
    }
}
```

#### 1.3 Update Provider to Handle Content Blocks

**File**: `crates/cowork-core/src/provider/genai_provider.rs`

Update message conversion to emit proper content blocks:

```rust
// When converting messages for API
match &msg.content {
    MessageContent::Text(text) => {
        // Simple text message
    }
    MessageContent::Blocks(blocks) => {
        for block in blocks {
            match block {
                ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                    // Use genai's ToolResponse with proper structure
                }
                // ... other block types
            }
        }
    }
}
```

#### 1.4 Batch Multiple Tool Results

**File**: `crates/cowork-core/src/session/agent_loop.rs`

When LLM requests multiple tools, collect all results into ONE user message:

```rust
async fn execute_tools(&mut self, tool_calls: &[ToolCallInfo]) -> Vec<ContentBlock> {
    let mut results = Vec::new();

    for tool_call in tool_calls {
        let (success, output) = self.execute_single_tool(tool_call).await;
        results.push(ContentBlock::ToolResult {
            tool_use_id: tool_call.id.clone(),
            content: output,
            is_error: if !success { Some(true) } else { None },
        });
    }

    results
}

// Then add single message with all results
self.session.add_tool_results(results);
```

---

### Phase 2: Update Agent Loop Message Flow

**Goal**: Ensure proper message sequence: assistant (tool_use) → user (tool_result).

#### 2.1 Update add_tool_result in ChatSession

**File**: `crates/cowork-core/src/orchestration/session.rs`

```rust
/// Add multiple tool results as a single user message
pub fn add_tool_results(&mut self, results: Vec<ContentBlock>) {
    // Update tool call statuses
    for result in &results {
        if let ContentBlock::ToolResult { tool_use_id, content, is_error } = result {
            for msg in &mut self.messages {
                for tc in &mut msg.tool_calls {
                    if tc.id == *tool_use_id {
                        tc.complete(content.clone());
                        if is_error.unwrap_or(false) {
                            tc.status = ToolCallStatus::Failed;
                        }
                    }
                }
            }
        }
    }

    // Add single user message with all tool results
    self.messages.push(ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: String::new(),
        content_blocks: results,
        tool_calls: Vec::new(),
        timestamp: Utc::now(),
    });
}
```

#### 2.2 Update to_llm_messages Conversion

**File**: `crates/cowork-core/src/orchestration/session.rs`

```rust
pub fn to_llm_messages(&self) -> Vec<LlmMessage> {
    self.messages.iter().map(|m| {
        if !m.content_blocks.is_empty() {
            LlmMessage {
                role: m.role.clone(),
                content: MessageContent::Blocks(m.content_blocks.clone()),
            }
        } else if !m.tool_calls.is_empty() {
            // Assistant message with tool calls
            let mut blocks = vec![];
            if !m.content.is_empty() {
                blocks.push(ContentBlock::Text { text: m.content.clone() });
            }
            for tc in &m.tool_calls {
                blocks.push(ContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
            }
            LlmMessage {
                role: m.role.clone(),
                content: MessageContent::Blocks(blocks),
            }
        } else {
            LlmMessage {
                role: m.role.clone(),
                content: MessageContent::Text(m.content.clone()),
            }
        }
    }).collect()
}
```

---

### Phase 3: Enhance System Prompt for Task Delegation

**Goal**: Guide the LLM on when to use Task (subagent) vs direct tools.

#### 3.1 Update Main Agent System Prompt

**File**: `crates/cowork-core/src/orchestration/system_prompt.rs` (or equivalent)

Add Claude Code's guidance on Task tool usage:

```rust
const TASK_TOOL_GUIDANCE: &str = r#"
## Task Tool Usage (Subagents)

The Task tool launches specialized subagents with isolated context. Use it strategically:

### When to Use Task Tool:
- **Open-ended exploration**: "How does authentication work?" → Task(Explore)
- **Multiple rounds of searching**: Finding patterns across many files → Task(Explore)
- **Complex multi-step operations**: Tasks requiring both exploration and modification → Task(GeneralPurpose)
- **Verbose output expected**: Running tests, fetching docs → isolates output from main context

### When NOT to Use Task Tool:
- Reading a specific file path → use Read directly
- Searching for a specific class like "class Foo" → use Glob directly
- Searching within 2-3 known files → use Read directly
- Running a simple command → use Bash directly

### Subagent Types:
| Type | Use When | Model |
|------|----------|-------|
| Explore | Codebase search, finding files, understanding structure | Fast (Haiku) |
| Plan | Designing implementation approach | Balanced |
| Bash | Git operations, command execution | Fast |
| GeneralPurpose | Complex tasks needing exploration + action | Balanced |

### Key Principle:
Use direct tools for **targeted, single operations**.
Use Task for **open-ended exploration or multi-step workflows**.
"#;
```

#### 3.2 Integrate into System Prompt Builder

```rust
impl SystemPrompt {
    pub fn build(&self) -> String {
        let mut prompt = String::new();

        // ... existing sections ...

        // Add Task tool guidance if Task tool is available
        if self.has_task_tool {
            prompt.push_str(TASK_TOOL_GUIDANCE);
        }

        prompt
    }
}
```

---

### Phase 4: Verify Subagent Result Handling

**Goal**: Ensure subagent returns summary, not full transcript.

#### 4.1 Current State (Already Correct)

The executor already returns only the final result:

```rust
// executor.rs - execute_agent_loop returns final_result string only
Ok(final_result)  // Just the final LLM response, not full history
```

#### 4.2 Add Result Size Limiting (Optional Enhancement)

**File**: `crates/cowork-core/src/tools/task/executor.rs`

```rust
const MAX_RESULT_SIZE: usize = 10000;

// After getting final_result
let truncated_result = if final_result.len() > MAX_RESULT_SIZE {
    format!(
        "{}...\n\n[Result truncated - {} chars total]",
        &final_result[..MAX_RESULT_SIZE],
        final_result.len()
    )
} else {
    final_result
};
```

---

## Files to Modify

| File | Changes |
|------|---------|
| `provider/mod.rs` | Add `ContentBlock` enum, update `LlmMessage` |
| `orchestration/session.rs` | Add `content_blocks` field, update `tool_result()`, add `add_tool_results()` |
| `session/agent_loop.rs` | Batch tool results, use new `add_tool_results()` |
| `provider/genai_provider.rs` | Handle `ContentBlock` in message conversion |
| `orchestration/system_prompt.rs` | Add Task tool usage guidance |

---

## Testing Plan

### Unit Tests
1. Test `ContentBlock` serialization/deserialization
2. Test `tool_result()` creates proper content blocks
3. Test `add_tool_results()` batches multiple results
4. Test `to_llm_messages()` converts correctly

### Integration Tests
1. Single tool call flow: Read → result
2. Parallel tool calls: Read + Grep → single user message with 2 results
3. Error handling: Tool failure → `is_error: true`
4. Subagent delegation: Task(Explore) returns summary only
5. Full conversation with multiple turns

### Manual Testing
1. Run cowork CLI, trigger tool calls, inspect API request payload
2. Verify message format matches Anthropic API spec
3. Test subagent isolation - large file reads shouldn't bloat main context

---

## Implementation Order

1. **Phase 1.1-1.2**: Add ContentBlock types (foundation)
2. **Phase 2.1-2.2**: Update ChatSession message handling
3. **Phase 1.3-1.4**: Update provider and agent loop
4. **Phase 3**: Enhance system prompts
5. **Phase 4**: Verify/enhance subagent results
6. **Testing**: Run all test suites

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Breaking existing message flow | Add content_blocks as optional field, maintain backward compat |
| Provider-specific differences | Abstract via MessageContent enum |
| Large tool results | Already have truncation, add size limits |
| Subagent infinite loops | Already prevented - no Task in subagent tools |

---

## Success Criteria

1. Tool results sent as `{type: "tool_result", tool_use_id, content}` blocks
2. Multiple tool results in single USER message
3. Error results have `is_error: true`
4. System prompt guides Task vs direct tool usage
5. All existing tests pass
6. New tests for content block handling pass
