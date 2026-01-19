# Session History

Cowork automatically saves your chat sessions so you can resume them later.

## Auto-Save

Sessions are automatically saved after each message exchange. No manual saving needed!

## Storage Location

Sessions are stored as JSON files in your config directory:

| Platform | Location |
|----------|----------|
| Linux | `~/.config/cowork/sessions/` |
| macOS | `~/Library/Application Support/cowork/sessions/` |
| Windows | `%APPDATA%\cowork\sessions\` |

### File Naming

Files are named: `YYYY-MM-DD_sessionid.json`

For example: `2024-01-15_abc12345.json`

## Managing Sessions

### From the History Page

1. Click **History** in the sidebar
2. Browse saved sessions
3. Click the **Play** button to load a session
4. Click the **Trash** button to delete a session

### Quick Cleanup

The History page provides quick cleanup buttons:

- **Delete >30 days** - Remove sessions older than 30 days
- **Delete >7 days** - Remove sessions older than 7 days
- **Delete All** - Remove all saved sessions (with confirmation)

### Using the File Manager

Click **Open Folder** in the History page to open the sessions directory in your file manager. From there you can:

- View session files directly
- Delete files manually
- Back up sessions by copying the folder

### Ask the AI

You can ask Cowork to manage sessions for you:

- "Delete all sessions older than 2 weeks"
- "How much space are my sessions using?"
- "Clean up old cowork sessions"

## What's Saved

Each session file contains:

- **Messages** - All user and assistant messages
- **Thinking** - AI reasoning/thinking content (if available)
- **Tool calls** - Tools used and their results
- **Metadata** - Provider, model, timestamps

## Privacy Note

Session files are stored locally on your computer. They are never uploaded to any server. You have full control over your data.

## Restoring a Session

1. Go to the **History** page
2. Find the session you want to restore
3. Click the **Play** button
4. The session loads and you can continue the conversation
