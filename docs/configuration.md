# Configuration

Configure Cowork from the Settings page or by editing configuration files directly.

## API Keys

You can set API keys in two ways:

### Option 1: Settings Page

1. Open Cowork
2. Go to **Settings** in the sidebar
3. Select your provider
4. Enter your API key
5. Click Save

### Option 2: Environment Variables

Set the appropriate environment variable before launching Cowork:

```bash
# Anthropic (Claude)
export ANTHROPIC_API_KEY=sk-ant-...

# OpenAI (GPT)
export OPENAI_API_KEY=sk-...

# Google (Gemini)
export GEMINI_API_KEY=...

# DeepSeek
export DEEPSEEK_API_KEY=...

# Groq
export GROQ_API_KEY=...

# xAI (Grok)
export XAI_API_KEY=...

# Together
export TOGETHER_API_KEY=...

# Fireworks
export FIREWORKS_API_KEY=...

# Zai (Zhipu)
export ZAI_API_KEY=...

# Nebius
export NEBIUS_API_KEY=...

# MIMO
export MIMO_API_KEY=...

# BigModel
export BIGMODEL_API_KEY=...
```

**Note:** Ollama doesn't require an API key for local models.

## Config File Location

The main configuration file is stored at:

| Platform | Location |
|----------|----------|
| Linux | `~/.config/cowork/config.toml` |
| macOS | `~/Library/Application Support/cowork/config.toml` |
| Windows | `%APPDATA%\cowork\config.toml` |

## Config File Format

The configuration uses TOML format:

```toml
[default_provider]
provider_type = "anthropic"
model = "claude-sonnet-4-20250514"
# api_key = "sk-ant-..." # Optional, can use env var instead

[approval]
auto_approve_level = "low"
show_dialogs = true
```

## Available Settings

### Provider Settings

| Setting | Description | Default |
|---------|-------------|---------|
| `provider_type` | AI provider to use | `anthropic` |
| `model` | Model name | Provider-specific |
| `api_key` | API key (optional if using env var) | None |
| `base_url` | Custom API endpoint | Provider default |

### Approval Settings

| Setting | Description | Default |
|---------|-------------|---------|
| `auto_approve_level` | Automatic approval level | `low` |
| `show_dialogs` | Show confirmation dialogs | `true` |

## Data Directories

| Directory | Purpose |
|-----------|---------|
| `~/.config/cowork/` | Configuration files |
| `~/.config/cowork/sessions/` | Saved chat sessions |
| `~/.config/cowork/skills/` | Installed skills |

## Resetting Configuration

To reset to defaults:

1. Close Cowork
2. Delete the config file: `rm ~/.config/cowork/config.toml`
3. Restart Cowork (it will recreate with defaults)

Or delete the entire config directory to reset everything:

```bash
rm -rf ~/.config/cowork
```

## Troubleshooting

### API Key Not Working

1. Check the key is correct (no extra spaces)
2. Verify the key has the required permissions
3. Try setting via environment variable instead
4. Check the provider's API status page

### Config File Not Saving

1. Check file permissions on the config directory
2. Ensure the directory exists
3. Try running Cowork with elevated permissions (not recommended for regular use)

### Sessions Not Saving

1. Check write permissions on `~/.config/cowork/sessions/`
2. Verify disk space is available
3. Check for file system errors
