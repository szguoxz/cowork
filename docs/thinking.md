# Thinking & Reasoning

Some AI models show their thinking process before giving a response. This feature helps you understand how the AI reasons through problems.

## Supported Models

The following models support thinking/reasoning display:

- **Anthropic Claude** - Extended thinking mode
- **DeepSeek** - Chain-of-thought reasoning

## How It Works

1. When you send a message, the AI may start with a thinking phase
2. A **purple "Thinking..." box** appears while the AI is reasoning
3. The thinking content streams in real-time
4. Once thinking is complete, the actual response appears
5. Thinking is saved with the message for later review

## Visual Indicators

### During Streaming

While the AI is thinking, you'll see:

```
┌─────────────────────────────────────┐
│ 🧠 Thinking...              [▼]    │
├─────────────────────────────────────┤
│ Let me analyze this step by step:  │
│ 1. First, I need to understand...  │
│ 2. Then I should consider...       │
│ ...                                │
└─────────────────────────────────────┘
```

### After Response Complete

For saved messages with thinking:

```
┌─────────────────────────────────────┐
│ 🧠 Thinking          [1234 chars]  │
└─────────────────────────────────────┘
```

Click to expand and view the thinking content.

## Collapsing/Expanding

- Click the **thinking header** to expand or collapse
- Thinking is **collapsed by default** for saved messages
- Thinking is **expanded by default** during streaming

## Benefits

1. **Transparency** - See how the AI approaches problems
2. **Debugging** - Catch misunderstandings early
3. **Learning** - Learn problem-solving techniques
4. **Trust** - Verify the AI's reasoning is sound

## Disabling Thinking Display

Currently, thinking display cannot be disabled in the UI. If you prefer not to see thinking content:

1. Use a provider/model that doesn't support thinking
2. Simply keep the thinking section collapsed

## Technical Details

- Thinking content is stored in messages with `<thinking>` tags
- The frontend parses and displays these separately from the response
- Thinking is included in session saves for later review
