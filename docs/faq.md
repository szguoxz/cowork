# Frequently Asked Questions

## General

### What is Cowork?

Cowork is an AI-powered coding assistant that helps you with software development tasks. It can read and write files, execute commands, and answer questions about your code.

### Is Cowork free?

Cowork itself is open source and free to use. However, you need to provide your own API key for the AI provider (Anthropic, OpenAI, etc.), which may have associated costs.

### Does Cowork work offline?

Only if you use a local model provider like Ollama. Cloud providers (Anthropic, OpenAI, etc.) require an internet connection.

## Privacy & Security

### Is my data sent to the cloud?

Your conversations are sent to the AI provider you configure (Anthropic, OpenAI, etc.) for processing. Cowork itself does not collect or store any data on external servers.

### Where is my data stored?

- **Configuration**: `~/.config/cowork/config.toml`
- **Sessions**: `~/.config/cowork/sessions/`
- All data is stored locally on your computer.

### Is my API key secure?

API keys entered in Settings are stored in the local config file. For better security, use environment variables instead.

## Features

### Why can't I see the AI's thinking?

Thinking/reasoning display is only available with certain models:
- Anthropic Claude (extended thinking)
- DeepSeek (chain-of-thought)

Other providers don't currently expose their reasoning process.

### How do I clear my conversation history?

1. Go to the **History** page
2. Click **Delete All** to remove all sessions
3. Or delete individual sessions with the trash icon

### Can I export my conversations?

Session files are stored as JSON in `~/.config/cowork/sessions/`. You can:
- Copy these files directly
- Open the folder from the History page
- Process them with scripts

### Why is the AI slow to respond?

Response time depends on:
- Your internet connection
- The AI provider's server load
- The model you're using (larger models are slower)
- The length of your conversation (more context = slower)

Try:
- Using a faster provider (Groq, Fireworks)
- Starting a new session to reduce context size
- Using a smaller/faster model

## Troubleshooting

### "API Key Required" error

1. Go to **Settings**
2. Enter your API key for the selected provider
3. Or set the environment variable (e.g., `ANTHROPIC_API_KEY`)

### The app shows a console window on Windows

Make sure you're running a release build, not a debug build. Release builds hide the console window automatically.

### Tool calls are failing

1. Check that you approved the tool call
2. Verify the file/command path is correct
3. Check file permissions
4. Look at the error message in the tool result

### Sessions aren't saving

1. Check that `~/.config/cowork/sessions/` exists and is writable
2. Verify you have disk space available
3. Try manually creating the directory

### The app crashes on startup

1. Try deleting the config file: `rm ~/.config/cowork/config.toml`
2. Check for error messages in the terminal (if running from command line)
3. Report the issue on GitHub with steps to reproduce

## Getting Help

### How do I report a bug?

Open an issue on GitHub with:
- Steps to reproduce
- Expected vs actual behavior
- Your OS and Cowork version
- Any error messages

### How do I request a feature?

Open an issue on GitHub labeled "feature request" describing:
- What you'd like to see
- Why it would be useful
- Any implementation ideas

### Where can I get support?

- **In-app Help**: Click Help in the sidebar
- **GitHub Issues**: For bugs and features
- **GitHub Discussions**: For questions and community help
