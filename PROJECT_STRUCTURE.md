# Cowork - Rust Multi-Agent Desktop Application

## Project Overview

A Cowork/Eigent-like desktop application built with Rust, using Rig for LLM agents and Tauri for the desktop shell.

## Directory Structure

```
cowork/
├── Cargo.toml                      # Workspace definition
├── README.md
├── LICENSE
├── .env.example
│
├── crates/
│   ├── cowork-core/                # Core library - agents, tools, orchestration
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── agent/              # Agent definitions
│   │       │   ├── mod.rs
│   │       │   ├── file_agent.rs       # File operations agent
│   │       │   ├── shell_agent.rs      # Command execution agent
│   │       │   ├── browser_agent.rs    # Web browsing agent
│   │       │   ├── document_agent.rs   # Document creation agent
│   │       │   └── orchestrator.rs     # Multi-agent coordinator
│   │       │
│   │       ├── tools/              # Tool implementations
│   │       │   ├── mod.rs
│   │       │   ├── filesystem/
│   │       │   │   ├── mod.rs
│   │       │   │   ├── read.rs
│   │       │   │   ├── write.rs
│   │       │   │   ├── list.rs
│   │       │   │   ├── move_file.rs
│   │       │   │   ├── delete.rs
│   │       │   │   └── search.rs
│   │       │   ├── shell/
│   │       │   │   ├── mod.rs
│   │       │   │   ├── execute.rs
│   │       │   │   └── sandbox.rs
│   │       │   ├── browser/
│   │       │   │   ├── mod.rs
│   │       │   │   ├── navigate.rs
│   │       │   │   ├── scrape.rs
│   │       │   │   ├── screenshot.rs
│   │       │   │   └── interact.rs
│   │       │   └── document/
│   │       │       ├── mod.rs
│   │       │       ├── markdown.rs
│   │       │       ├── pdf.rs
│   │       │       └── office.rs
│   │       │
│   │       ├── task/               # Task planning and execution
│   │       │   ├── mod.rs
│   │       │   ├── planner.rs          # Break task into steps
│   │       │   ├── executor.rs         # Execute task steps
│   │       │   ├── queue.rs            # Background task queue
│   │       │   └── checkpoint.rs       # Save/restore task state
│   │       │
│   │       ├── approval/           # Human-in-the-loop
│   │       │   ├── mod.rs
│   │       │   ├── policy.rs           # What needs approval
│   │       │   ├── request.rs          # Approval request
│   │       │   └── history.rs          # Audit log
│   │       │
│   │       ├── context/            # Context management
│   │       │   ├── mod.rs
│   │       │   ├── workspace.rs        # Workspace/folder context
│   │       │   ├── memory.rs           # Conversation memory
│   │       │   └── compaction.rs       # Context compaction
│   │       │
│   │       ├── provider/           # LLM provider abstraction
│   │       │   ├── mod.rs
│   │       │   ├── openai.rs
│   │       │   ├── anthropic.rs
│   │       │   ├── ollama.rs
│   │       │   └── config.rs
│   │       │
│   │       └── error.rs            # Error types
│   │
│   ├── cowork-mcp/                 # MCP client implementation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs               # MCP client
│   │       ├── transport/
│   │       │   ├── mod.rs
│   │       │   ├── stdio.rs            # Stdio transport
│   │       │   └── sse.rs              # SSE transport
│   │       ├── protocol/
│   │       │   ├── mod.rs
│   │       │   ├── messages.rs         # JSON-RPC messages
│   │       │   ├── tools.rs            # Tool schemas
│   │       │   ├── resources.rs        # Resource schemas
│   │       │   └── prompts.rs          # Prompt schemas
│   │       ├── server_manager.rs       # Start/stop MCP servers
│   │       └── tool_adapter.rs         # Adapt MCP tools to Rig tools
│   │
│   ├── cowork-sandbox/             # Sandboxing and security
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── policy.rs               # Security policies
│   │       ├── linux/
│   │       │   ├── mod.rs
│   │       │   ├── landlock.rs         # Landlock LSM
│   │       │   └── seccomp.rs          # Seccomp filters
│   │       ├── macos/
│   │       │   ├── mod.rs
│   │       │   └── sandbox.rs          # macOS sandbox-exec
│   │       └── windows/
│   │           ├── mod.rs
│   │           └── appcontainer.rs     # Windows AppContainer
│   │
│   └── cowork-app/                 # Tauri application
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       ├── build.rs
│       ├── icons/
│       └── src/
│           ├── main.rs
│           ├── commands/           # Tauri commands (IPC)
│           │   ├── mod.rs
│           │   ├── task.rs             # Task operations
│           │   ├── agent.rs            # Agent control
│           │   ├── settings.rs         # Settings management
│           │   ├── workspace.rs        # Workspace management
│           │   └── mcp.rs              # MCP server management
│           ├── state.rs            # Application state
│           ├── events.rs           # Event definitions
│           ├── tray.rs             # System tray
│           └── menu.rs             # Application menu
│
├── frontend/                       # Web frontend (Tauri webview)
│   ├── package.json
│   ├── vite.config.ts
│   ├── tailwind.config.js
│   ├── tsconfig.json
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── components/
│       │   ├── chat/
│       │   │   ├── ChatContainer.tsx
│       │   │   ├── MessageList.tsx
│       │   │   ├── MessageInput.tsx
│       │   │   ├── Message.tsx
│       │   │   └── ToolCall.tsx
│       │   ├── workspace/
│       │   │   ├── WorkspaceSelector.tsx
│       │   │   ├── FileTree.tsx
│       │   │   ├── FilePreview.tsx
│       │   │   └── WorkspaceSettings.tsx
│       │   ├── task/
│       │   │   ├── TaskList.tsx
│       │   │   ├── TaskCard.tsx
│       │   │   ├── TaskDetail.tsx
│       │   │   └── TaskProgress.tsx
│       │   ├── approval/
│       │   │   ├── ApprovalModal.tsx
│       │   │   ├── ApprovalQueue.tsx
│       │   │   └── ApprovalHistory.tsx
│       │   ├── settings/
│       │   │   ├── SettingsPanel.tsx
│       │   │   ├── ModelSettings.tsx
│       │   │   ├── McpSettings.tsx
│       │   │   └── SecuritySettings.tsx
│       │   └── common/
│       │       ├── Button.tsx
│       │       ├── Modal.tsx
│       │       ├── Sidebar.tsx
│       │       └── Header.tsx
│       ├── hooks/
│       │   ├── useTask.ts
│       │   ├── useAgent.ts
│       │   ├── useWorkspace.ts
│       │   ├── useApproval.ts
│       │   └── useTauri.ts
│       ├── stores/
│       │   ├── taskStore.ts
│       │   ├── chatStore.ts
│       │   ├── workspaceStore.ts
│       │   └── settingsStore.ts
│       ├── lib/
│       │   ├── tauri.ts            # Tauri IPC wrappers
│       │   ├── events.ts           # Event listeners
│       │   └── utils.ts
│       └── styles/
│           └── globals.css
│
├── config/                         # Configuration files
│   ├── default.toml                # Default configuration
│   ├── mcp-servers.json            # MCP server definitions
│   └── prompts/                    # System prompts
│       ├── file_agent.md
│       ├── shell_agent.md
│       ├── browser_agent.md
│       ├── document_agent.md
│       └── orchestrator.md
│
├── scripts/                        # Build and dev scripts
│   ├── setup.sh                    # Development setup
│   ├── build.sh                    # Production build
│   └── release.sh                  # Create release artifacts
│
└── tests/
    ├── integration/
    │   ├── file_agent_test.rs
    │   ├── shell_agent_test.rs
    │   ├── mcp_test.rs
    │   └── orchestrator_test.rs
    └── e2e/
        ├── playwright.config.ts
        └── specs/
            ├── chat.spec.ts
            ├── task.spec.ts
            └── workspace.spec.ts
```

## Crate Dependencies

### cowork-core/Cargo.toml

```toml
[package]
name = "cowork-core"
version = "0.1.0"
edition = "2024"

[dependencies]
# LLM Framework
rig-core = "0.5"

# Async runtime
tokio = { version = "1", features = ["full"] }
futures = "0.3"
async-trait = "0.1"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# File operations
walkdir = "2"
globset = "0.4"
notify = "6"                        # File watching

# Browser automation
chromiumoxide = "0.7"               # Headless Chrome

# Error handling
thiserror = "2"
anyhow = "1"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Utils
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
wiremock = "0.6"
```

### cowork-mcp/Cargo.toml

```toml
[package]
name = "cowork-mcp"
version = "0.1.0"
edition = "2024"

[dependencies]
tokio = { version = "1", features = ["process", "io-util"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
jsonrpc-core = "18"
async-trait = "0.1"
thiserror = "2"
tracing = "0.1"
reqwest = { version = "0.12", features = ["stream"] }   # For SSE
eventsource-stream = "0.2"                               # SSE parsing
```

### cowork-sandbox/Cargo.toml

```toml
[package]
name = "cowork-sandbox"
version = "0.1.0"
edition = "2024"

[dependencies]
thiserror = "2"
tracing = "0.1"

[target.'cfg(target_os = "linux")'.dependencies]
landlock = "0.4"
seccompiler = "0.4"
nix = { version = "0.29", features = ["process"] }

[target.'cfg(target_os = "macos")'.dependencies]
# macOS sandbox via command

[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.58", features = ["Win32_Security"] }
```

### cowork-app/Cargo.toml

```toml
[package]
name = "cowork-app"
version = "0.1.0"
edition = "2024"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
cowork-core = { path = "../cowork-core" }
cowork-mcp = { path = "../cowork-mcp" }
cowork-sandbox = { path = "../cowork-sandbox" }

tauri = { version = "2", features = ["tray-icon", "devtools"] }
tauri-plugin-shell = "2"
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"
tauri-plugin-notification = "2"

tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

## Data Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              User Input                                  │
│                     "Organize my downloads folder"                       │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           Tauri Command                                  │
│                      commands/task.rs::create_task                       │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         Task Planner                                     │
│                    task/planner.rs::plan_task                            │
│                                                                         │
│  1. Analyze request                                                     │
│  2. Select appropriate agent(s)                                         │
│  3. Break into steps                                                    │
│  4. Identify approval requirements                                      │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         Orchestrator                                     │
│                   agent/orchestrator.rs::execute                         │
│                                                                         │
│  For each step:                                                         │
│    1. Check if approval needed → emit event, wait                       │
│    2. Dispatch to appropriate agent                                     │
│    3. Collect result                                                    │
│    4. Update context                                                    │
│    5. Checkpoint state                                                  │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
            ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
            │ File Agent  │ │Shell Agent  │ │Browser Agent│
            │             │ │             │ │             │
            │ Uses tools: │ │ Uses tools: │ │ Uses tools: │
            │ - ReadFile  │ │ - Execute   │ │ - Navigate  │
            │ - WriteFile │ │ - Sandbox   │ │ - Scrape    │
            │ - ListDir   │ │             │ │ - Screenshot│
            │ - MoveFile  │ │             │ │             │
            └─────────────┘ └─────────────┘ └─────────────┘
                    │               │               │
                    └───────────────┼───────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          Tool Execution                                  │
│                      (Sandboxed if configured)                           │
│                                                                         │
│  - File operations within workspace boundary                            │
│  - Shell commands with seccomp/landlock                                 │
│  - Network requests with allowlist                                      │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           Result                                         │
│                                                                         │
│  - Tool outputs collected                                               │
│  - Context updated                                                      │
│  - Events emitted to frontend                                           │
│  - Task marked complete                                                 │
└─────────────────────────────────────────────────────────────────────────┘
```

## Key Interfaces

### Agent Trait

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    /// Agent identifier
    fn id(&self) -> &str;

    /// Agent capabilities description
    fn description(&self) -> &str;

    /// Tools this agent can use
    fn tools(&self) -> &[Arc<dyn Tool>];

    /// Execute a task step
    async fn execute(&self, step: &TaskStep, ctx: &mut Context) -> Result<StepResult>;

    /// Check if agent can handle this type of task
    fn can_handle(&self, task_type: &TaskType) -> bool;
}
```

### Tool Trait (Rig-compatible)

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name for LLM
    fn name(&self) -> &str;

    /// Tool description for LLM
    fn description(&self) -> &str;

    /// JSON schema for parameters
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput>;

    /// Whether this tool requires approval
    fn requires_approval(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}

pub enum ApprovalLevel {
    None,
    Notify,      // Inform user but don't wait
    Confirm,     // Wait for confirmation
    Strict,      // Require explicit approval with details
}
```

### Task Types

```rust
pub struct Task {
    pub id: Uuid,
    pub description: String,
    pub status: TaskStatus,
    pub steps: Vec<TaskStep>,
    pub context: Context,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub struct TaskStep {
    pub id: Uuid,
    pub description: String,
    pub agent_id: String,
    pub tool_calls: Vec<ToolCall>,
    pub status: StepStatus,
    pub result: Option<StepResult>,
}

pub enum TaskStatus {
    Pending,
    Planning,
    Running,
    AwaitingApproval { step_id: Uuid },
    Completed,
    Failed { error: String },
    Cancelled,
}
```

### MCP Integration

```rust
pub struct McpServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

pub struct McpClient {
    transport: Box<dyn Transport>,
    capabilities: ServerCapabilities,
}

impl McpClient {
    pub async fn connect(server: &McpServer) -> Result<Self>;
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>>;
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value>;
    pub async fn list_resources(&self) -> Result<Vec<Resource>>;
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContent>;
}
```

## Configuration

### config/default.toml

```toml
[workspace]
# Default workspace directory (can be overridden per session)
default_path = "~/Documents/Cowork"
# File types to ignore
ignore_patterns = [".git", "node_modules", ".DS_Store", "*.pyc"]

[providers.openai]
enabled = true
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4o"

[providers.anthropic]
enabled = true
api_key_env = "ANTHROPIC_API_KEY"
default_model = "claude-sonnet-4-20250514"

[providers.ollama]
enabled = true
base_url = "http://localhost:11434"
default_model = "llama3.2"

[agents.file]
enabled = true
model = "default"  # Use default provider's model
max_file_size = "10MB"

[agents.shell]
enabled = true
model = "default"
allowed_commands = ["ls", "cat", "grep", "find", "wc", "head", "tail"]
blocked_commands = ["rm -rf", "sudo", "chmod", "chown"]

[agents.browser]
enabled = true
model = "default"
headless = true
timeout_seconds = 30

[approval]
# Actions that always require approval
always_approve = ["delete", "move", "shell_execute"]
# Actions that never require approval (within workspace)
never_approve = ["read", "list"]
# Default for everything else
default = "confirm"

[security]
# Enable sandboxing (Linux: landlock, macOS: sandbox-exec)
sandbox_enabled = true
# Network access for agents
network_allowlist = ["api.openai.com", "api.anthropic.com"]
# Maximum task execution time
max_task_duration_seconds = 300

[ui]
theme = "system"  # system, light, dark
show_tool_calls = true
show_thinking = false
```

## Event System

### Tauri Events (Backend → Frontend)

```rust
// Events emitted from Rust to frontend
pub enum AppEvent {
    // Task events
    TaskCreated { task: Task },
    TaskUpdated { task: Task },
    TaskCompleted { task_id: Uuid, result: TaskResult },
    TaskFailed { task_id: Uuid, error: String },

    // Step events
    StepStarted { task_id: Uuid, step: TaskStep },
    StepCompleted { task_id: Uuid, step_id: Uuid, result: StepResult },

    // Approval events
    ApprovalRequired { request: ApprovalRequest },

    // Agent events
    AgentThinking { agent_id: String, thought: String },
    ToolCallStarted { tool_name: String, params: Value },
    ToolCallCompleted { tool_name: String, result: Value },

    // Workspace events
    FileChanged { path: PathBuf, change_type: ChangeType },
}
```

### Tauri Commands (Frontend → Backend)

```rust
#[tauri::command]
async fn create_task(description: String, workspace: PathBuf) -> Result<Task>;

#[tauri::command]
async fn cancel_task(task_id: Uuid) -> Result<()>;

#[tauri::command]
async fn approve_request(request_id: Uuid, approved: bool) -> Result<()>;

#[tauri::command]
async fn list_tasks(status: Option<TaskStatus>) -> Result<Vec<Task>>;

#[tauri::command]
async fn get_workspace_files(path: PathBuf) -> Result<Vec<FileInfo>>;

#[tauri::command]
async fn update_settings(settings: Settings) -> Result<()>;

#[tauri::command]
async fn list_mcp_servers() -> Result<Vec<McpServerInfo>>;

#[tauri::command]
async fn start_mcp_server(name: String) -> Result<()>;

#[tauri::command]
async fn stop_mcp_server(name: String) -> Result<()>;
```

## Security Considerations

1. **Workspace Isolation**
   - Agents can only access files within designated workspace
   - Symlinks outside workspace are blocked
   - Path traversal attempts are rejected

2. **Command Sandboxing**
   - Shell commands run in sandboxed environment
   - Landlock (Linux) / sandbox-exec (macOS) for filesystem isolation
   - Seccomp for syscall filtering

3. **Network Control**
   - Allowlist for permitted domains
   - No arbitrary network access from tools

4. **Approval System**
   - Destructive operations require explicit approval
   - Audit log of all actions taken
   - Ability to revert changes

5. **Prompt Injection Defense**
   - Structured output parsing
   - Tool output sanitization
   - Separate system/user contexts

## Development Phases

### Phase 1: Core Foundation (Week 1-2)
- [ ] Set up workspace structure
- [ ] Implement basic Tool trait
- [ ] Create FileAgent with read/write/list tools
- [ ] Basic Tauri shell with chat UI

### Phase 2: Agent System (Week 3-4)
- [ ] Implement ShellAgent with sandboxing
- [ ] Add task planning and orchestration
- [ ] Human-in-the-loop approval system
- [ ] Task queue and background execution

### Phase 3: MCP Integration (Week 5)
- [ ] MCP client implementation
- [ ] Stdio and SSE transports
- [ ] Tool adapter for Rig compatibility
- [ ] MCP server management UI

### Phase 4: Browser Agent (Week 6)
- [ ] Chromiumoxide integration
- [ ] Navigation and scraping tools
- [ ] Screenshot capture
- [ ] Form interaction

### Phase 5: Polish & Security (Week 7-8)
- [ ] Full sandboxing implementation
- [ ] Comprehensive error handling
- [ ] Settings UI
- [ ] Documentation
- [ ] Release builds for all platforms
