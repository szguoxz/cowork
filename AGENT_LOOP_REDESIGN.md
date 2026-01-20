# Agent Loop Redesign - Multi-Session Architecture

## Overview

Unify CLI and UI app to share the same agent loop logic in `cowork-core`, with support for multiple concurrent sessions.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                           cowork-core                                │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                      SessionManager                          │    │
│  │  ┌─────────────────────────────────────────────────────┐    │    │
│  │  │  HashMap<SessionId, Sender<SessionInput>>           │    │    │
│  │  │                                                      │    │    │
│  │  │  session_1 -> tx1  ──▶ [AgentLoop 1] ──┐            │    │    │
│  │  │  session_2 -> tx2  ──▶ [AgentLoop 2] ──┼──▶ output_tx│    │    │
│  │  │  session_3 -> tx3  ──▶ [AgentLoop 3] ──┘            │    │    │
│  │  └─────────────────────────────────────────────────────┘    │    │
│  │                                                              │    │
│  │  push_message(session_id, msg) -> looks up/creates session  │    │
│  │  output_rx: Receiver<(SessionId, SessionOutput)>            │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
           │                                    │
           │ push_message()                     │ output_rx.recv()
           ▼                                    ▼
┌──────────────────┐                 ┌──────────────────┐
│      CLI         │                 │    Tauri App     │
│  (1 session)     │                 │  (N sessions)    │
│                  │                 │                  │
│  read stdin      │                 │  invoke handler  │
│  push_message()  │                 │  push_message()  │
│  recv output     │                 │  spawn reader    │
│  print to term   │                 │  emit to webview │
└──────────────────┘                 └──────────────────┘
                                              │
                                              ▼
                                     ┌──────────────────┐
                                     │    Frontend      │
                                     │  (React + Tabs)  │
                                     │                  │
                                     │  Session tabs    │
                                     │  Chat per session│
                                     └──────────────────┘
```

## Core Types (cowork-core)

```rust
// === Session Types ===

pub type SessionId = String;

/// Input messages sent TO an agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionInput {
    /// User sends a message
    UserMessage(String),
    /// User approves a tool execution
    ApproveTool(String),
    /// User rejects a tool execution
    RejectTool(String),
    /// User answers a question
    AnswerQuestion { request_id: String, answers: HashMap<String, String> },
    /// Stop the session
    Stop,
}

/// Output messages sent FROM an agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionOutput {
    /// Session is ready
    Ready,
    /// Session is idle, waiting for input
    Idle,
    /// Echo of user message
    UserMessage { id: String, content: String },
    /// Assistant thinking (streaming)
    Thinking { content: String },
    /// Assistant message
    AssistantMessage { id: String, content: String },
    /// Tool execution starting
    ToolStart { id: String, name: String, arguments: serde_json::Value },
    /// Tool needs approval
    ToolPending { id: String, name: String, arguments: serde_json::Value },
    /// Tool execution completed
    ToolDone { id: String, name: String, success: bool, output: String },
    /// Error occurred
    Error { message: String },
    /// Session stopped
    Stopped,
}

/// Configuration for creating a session
pub struct SessionConfig {
    pub workspace_path: PathBuf,
    pub provider: Arc<dyn LlmProvider>,
    pub approval_config: ToolApprovalConfig,
    pub system_prompt: Option<String>,
}
```

## SessionManager (cowork-core)

```rust
pub struct SessionManager {
    /// Map of session ID to input sender
    sessions: Arc<RwLock<HashMap<SessionId, mpsc::Sender<SessionInput>>>>,
    /// Channel for all session outputs (session_id, output)
    output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
    /// Receiver for outputs (given to consumer)
    output_rx: Arc<Mutex<mpsc::Receiver<(SessionId, SessionOutput)>>>,
    /// Default session config factory
    config_factory: Arc<dyn Fn() -> SessionConfig + Send + Sync>,
}

impl SessionManager {
    pub fn new(config_factory: impl Fn() -> SessionConfig + Send + Sync + 'static) -> Self;

    /// Push a message to a session. Creates session if doesn't exist.
    pub async fn push_message(&self, session_id: &str, input: SessionInput) -> Result<()>;

    /// Get the output receiver (clone of Arc)
    pub fn output_receiver(&self) -> Arc<Mutex<mpsc::Receiver<(SessionId, SessionOutput)>>>;

    /// List active sessions
    pub fn list_sessions(&self) -> Vec<SessionId>;

    /// Stop a specific session
    pub async fn stop_session(&self, session_id: &str) -> Result<()>;

    /// Stop all sessions
    pub async fn stop_all(&self) -> Result<()>;
}
```

## AgentLoop (cowork-core)

```rust
/// The unified agent loop - runs in a spawned task
pub struct AgentLoop {
    session_id: SessionId,
    input_rx: mpsc::Receiver<SessionInput>,
    output_tx: mpsc::Sender<(SessionId, SessionOutput)>,
    provider: Arc<dyn LlmProvider>,
    messages: Vec<LlmMessage>,
    system_prompt: String,
    tools: Vec<ToolDefinition>,
    approval_config: ToolApprovalConfig,
    workspace_path: PathBuf,
}

impl AgentLoop {
    pub fn new(...) -> Self;

    /// Run the loop until Stop is received
    pub async fn run(mut self);

    /// Handle user message - calls LLM, executes tools, etc.
    async fn handle_user_message(&mut self, content: String);

    /// Process LLM response and tool calls
    async fn process_llm_response(&mut self);

    /// Execute a tool
    async fn execute_tool(&self, name: &str, args: &serde_json::Value) -> Result<String, String>;

    /// Send output to the channel
    fn emit(&self, output: SessionOutput);
}
```

## CLI Integration

```rust
// cowork-cli/src/main.rs

#[tokio::main]
async fn main() {
    let session_manager = SessionManager::new(|| SessionConfig { ... });
    let output_rx = session_manager.output_receiver();

    // Spawn output handler
    let print_handle = tokio::spawn(async move {
        let rx = output_rx.lock().await;
        while let Some((session_id, output)) = rx.recv().await {
            match output {
                SessionOutput::AssistantMessage { content, .. } => {
                    println!("{}: {}", style("Assistant").green(), content);
                }
                SessionOutput::ToolStart { name, .. } => {
                    println!("  [Executing: {}]", name);
                }
                // ... handle other outputs
            }
        }
    });

    // Main input loop
    let session_id = "cli-session";
    loop {
        let input = read_line()?;
        session_manager.push_message(session_id, SessionInput::UserMessage(input)).await?;
    }
}
```

## Tauri Integration

```rust
// cowork-app/src/lib.rs

struct AppState {
    session_manager: SessionManager,
}

#[tauri::command]
async fn push_message(
    session_id: String,
    input: SessionInput,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.session_manager
        .push_message(&session_id, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.session_manager.list_sessions())
}

#[tauri::command]
async fn stop_session(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.session_manager.stop_session(&session_id).await.map_err(|e| e.to_string())
}

// In setup:
fn setup(app: &mut App) {
    let session_manager = SessionManager::new(...);
    let output_rx = session_manager.output_receiver();
    let app_handle = app.handle().clone();

    // Spawn output emitter
    tokio::spawn(async move {
        let rx = output_rx.lock().await;
        while let Some((session_id, output)) = rx.recv().await {
            // Emit to frontend with session_id
            app_handle.emit("session_output", SessionOutputEvent {
                session_id,
                output,
            }).ok();
        }
    });

    app.manage(AppState { session_manager });
}
```

## Frontend Updates (React)

```typescript
// Types
interface Session {
  id: string;
  name: string;
  messages: Message[];
  isIdle: boolean;
}

// State
const [sessions, setSessions] = useState<Map<string, Session>>(new Map());
const [activeSessionId, setActiveSessionId] = useState<string | null>(null);

// Event listener
useEffect(() => {
  const unlisten = listen<{ session_id: string; output: SessionOutput }>(
    'session_output',
    (event) => {
      const { session_id, output } = event.payload;
      setSessions(prev => {
        const session = prev.get(session_id) || createSession(session_id);
        // Update session based on output type
        return new Map(prev).set(session_id, updateSession(session, output));
      });
    }
  );
  return () => { unlisten.then(f => f()); };
}, []);

// UI: Tabs for sessions + chat area
<SessionTabs
  sessions={sessions}
  activeId={activeSessionId}
  onSelect={setActiveSessionId}
  onNew={createNewSession}
  onClose={closeSession}
/>
<ChatArea session={sessions.get(activeSessionId)} />
```

## Implementation Plan

### Phase 1: Core Infrastructure (cowork-core) ✅ COMPLETE
1. ✅ Create `src/session/mod.rs` with types
2. ✅ Create `src/session/manager.rs` with SessionManager
3. ✅ Create `src/session/agent_loop.rs` with unified AgentLoop
4. ✅ Export from `lib.rs`
5. ✅ All 114 tests pass

### Phase 2: CLI Migration ✅ COMPLETE
1. ✅ Removed inline agentic loop from `main.rs`
2. ✅ Used SessionManager with single session
3. ✅ Migrated output handling to SessionOutput processing
4. ✅ Tool approval flow via SessionInput/SessionOutput
5. ✅ Ask user question handling preserved
6. ✅ All 318 tests pass

### Phase 3: Tauri Migration ✅ COMPLETE
1. ✅ Remove `simple_loop.rs`, `agentic_loop.rs`, `chat.rs`, `loop_channel.rs`
2. ✅ Add Tauri commands for session management (`simple_commands.rs` refactored)
3. ✅ Setup output emitter in app initialization (`start_loop` spawns emitter)
4. ✅ Updated `AppState` to use `SessionManager` instead of `LoopInputHandle`
5. ✅ All 331 tests pass
6. Test with virtual display + Playwright (manual testing needed)

### Phase 4: Frontend Updates ✅ COMPLETE
1. ✅ Add session state management (SessionContext.tsx with types, context provider)
2. ✅ Add session tabs UI (SessionTabs.tsx component)
3. ✅ Update Chat component to work with sessions (Chat.tsx uses useSession hook)
4. ✅ Update bindings with session_id support (LoopOutput.ts, Session.ts)
5. ✅ Wrap App with SessionProvider (App.tsx)
6. ✅ Frontend builds successfully, all Rust tests pass

### Phase 5: Cleanup ✅ COMPLETE
1. ✅ Remove old unused code (simple_loop.rs, agentic_loop.rs, chat.rs, loop_channel.rs deleted)
2. ✅ All 318 Rust tests pass
3. ✅ Frontend TypeScript builds successfully
4. Manual integration testing needed (CLI + UI with virtual display)

## Final Status

**All 5 phases are COMPLETE.** The multi-session architecture is fully implemented:

- **cowork-core**: SessionManager + AgentLoop provide unified session handling
- **cowork-cli**: Uses SessionManager for single-session CLI operation
- **cowork-app**: Tauri commands expose session operations to frontend
- **Frontend**: React SessionContext manages multiple sessions with tabs UI

**Files Created:**
- `crates/cowork-core/src/session/` - Session types, manager, and agent loop
- `frontend/src/context/SessionContext.tsx` - React session state management
- `frontend/src/components/SessionTabs.tsx` - Multi-session tab navigation
- `frontend/src/bindings/Session.ts` - TypeScript session types

**Files Deleted:**
- `crates/cowork-app/src/simple_loop.rs`
- `crates/cowork-app/src/agentic_loop.rs`
- `crates/cowork-app/src/chat.rs`
- `crates/cowork-app/src/loop_channel.rs`

**Remaining Work:**
- Manual testing with real LLM provider (CLI and UI)
- Playwright E2E tests with virtual display

## Files to Create/Modify

### Create: ✅ COMPLETE
- ✅ `crates/cowork-core/src/session/mod.rs`
- ✅ `crates/cowork-core/src/session/types.rs`
- ✅ `crates/cowork-core/src/session/manager.rs`
- ✅ `crates/cowork-core/src/session/agent_loop.rs`

### Modify:
- ✅ `crates/cowork-core/src/lib.rs` - export session module
- ✅ `crates/cowork-cli/src/main.rs` - use SessionManager
- ✅ `crates/cowork-app/src/lib.rs` - setup and commands
- ✅ `crates/cowork-app/src/state.rs` - use SessionManager
- ✅ `crates/cowork-app/src/simple_commands.rs` - session-based commands
- ✅ `crates/cowork-app/src/streaming.rs` - use cowork_core::ToolCallInfo
- ✅ `crates/cowork-app/src/session_storage.rs` - use cowork_core::ChatMessage
- ✅ `frontend/src/pages/Chat.tsx` - multi-session support
- ✅ `frontend/src/App.tsx` - SessionProvider wrapper
- ✅ `frontend/src/context/SessionContext.tsx` - session state management (NEW)
- ✅ `frontend/src/components/SessionTabs.tsx` - session tabs UI (NEW)
- ✅ `frontend/src/bindings/LoopOutput.ts` - session_id added to events
- ✅ `frontend/src/bindings/Session.ts` - session types (NEW)

### Delete: ✅ COMPLETE
- ✅ `crates/cowork-app/src/simple_loop.rs`
- ✅ `crates/cowork-app/src/agentic_loop.rs`
- ✅ `crates/cowork-app/src/chat.rs`
- ✅ `crates/cowork-app/src/loop_channel.rs`

## Testing Plan

1. **Unit Tests**: SessionManager, AgentLoop
2. **CLI Test**: Run with OpenAI key, verify multi-turn conversation
3. **UI Test**: Virtual display + Playwright
   - Create session
   - Send message
   - Verify response
   - Create second session
   - Switch between sessions
