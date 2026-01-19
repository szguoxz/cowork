# Getting Started

Welcome to **Cowork** - your AI-powered coding assistant! Cowork helps you with software development tasks using various AI providers.

## Quick Start

1. Configure your API key in **Settings**
2. Start a conversation in **Chat**
3. Ask Cowork to help with coding tasks, file operations, or questions

## Supported Providers

| Provider | Models | Notes |
|----------|--------|-------|
| **Anthropic** | Claude 3.5 Sonnet, Claude 3 Opus | Recommended |
| **OpenAI** | GPT-4, GPT-4o | |
| **Google** | Gemini 1.5 Pro, Gemini 1.5 Flash | |
| **DeepSeek** | DeepSeek Coder, DeepSeek Chat | Shows thinking |
| **Groq** | Llama 3, Mixtral | Fast inference |
| **xAI** | Grok 2 | |
| **Together** | Various open models | |
| **Fireworks** | Fast open models | |
| **Zai (Zhipu)** | GLM-4 | |
| **Nebius** | Various models | |
| **MIMO** | MIMO models | |
| **BigModel** | GLM-4 | |
| **Ollama** | Local models | No API key needed |

## Setting Up Your API Key

### Option 1: Settings Page

1. Open Cowork
2. Go to **Settings** in the sidebar
3. Select your provider
4. Enter your API key
5. Click Save

### Option 2: Environment Variables

Set the appropriate environment variable before launching Cowork:

```bash
# Anthropic
export ANTHROPIC_API_KEY=sk-ant-...

# OpenAI
export OPENAI_API_KEY=sk-...

# Google
export GEMINI_API_KEY=...

# DeepSeek
export DEEPSEEK_API_KEY=...
```

## First Conversation

Once configured, try asking Cowork something like:

- "What files are in the current directory?"
- "Read the contents of package.json"
- "Help me write a function that validates email addresses"
- "Explain what this code does: [paste code]"

## Next Steps

- Learn about the [Chat Interface](./chat.md)
- Understand [Thinking & Reasoning](./thinking.md)
- Explore [Session History](./sessions.md)
