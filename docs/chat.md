# Chat Interface

The Chat page is where you interact with your AI assistant. Type your message and press Enter or click Send.

## Features

### Streaming Responses

Responses stream in real-time as the AI generates them. This makes the experience feel faster and more interactive.

### Tool Calls

The AI can use tools to help you:

| Tool | Description |
|------|-------------|
| `read_file` | Read contents of a file |
| `write_file` | Create or modify a file |
| `list_directory` | List files in a directory |
| `execute_command` | Run shell commands |
| `search_files` | Find files by pattern |

### Tool Approval

When the AI wants to use a tool, you'll see an approval prompt:

```
┌─────────────────────────────────────┐
│ 🔧 execute_command         [Pending]│
│ ✓ Approve    ✗ Reject              │
├─────────────────────────────────────┤
│ command: "npm install"              │
└─────────────────────────────────────┘
```

- Click **✓** to approve and execute
- Click **✗** to reject the tool call
- Press **Y** to approve all pending tools
- Press **N** to reject all pending tools

### Context Indicator

The header shows how much of the AI's context window is used:

```
Context: 15,234 / 200,000 tokens (7.6%)
```

When context gets high, you may want to:
- Start a new session
- Use the compact feature to summarize older messages

## Tips for Effective Conversations

### Be Specific

Instead of: "Fix the bug"

Try: "Fix the TypeError in src/utils.ts line 45 where we're calling `.map()` on undefined"

### Provide Context

- Share relevant file paths
- Describe your project structure
- Mention the programming language/framework

### Break Down Large Tasks

Instead of: "Build me a complete user authentication system"

Try:
1. "Let's design the authentication flow first"
2. "Now create the login endpoint"
3. "Add password hashing"
4. etc.

### Review Tool Calls

Always review what tools the AI wants to use before approving. This is especially important for:
- File writes (check the content)
- Command execution (check the command)
- Destructive operations

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Enter` | Send message |
| `Escape` | Cancel active loop |
| `Y` | Approve all pending tools |
| `N` | Reject all pending tools |
