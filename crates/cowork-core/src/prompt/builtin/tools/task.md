# Task Tool Description

Launch a new agent to handle complex, multi-step tasks autonomously.

The Task tool launches specialized agents (subprocesses) that autonomously handle complex tasks. Each agent type has specific capabilities and tools available to it.

## Available Agent Types

| Agent | Tools | Purpose |
|-------|-------|---------|
| Bash | Bash | Command execution specialist for git operations, command execution, and other terminal tasks |
| general-purpose | All | General-purpose agent for researching complex questions, searching for code, and executing multi-step tasks |
| Explore | Read-only (Glob, Grep, Read, LSP) | Fast agent for exploring codebases - finding files, searching code, answering codebase questions |
| Plan | Read-only (Glob, Grep, Read, LSP) | Software architect agent for designing implementation plans |

## When Using the Task Tool

Specify a `subagent_type` parameter to select which agent type to use.

### When NOT to Use Task Tool

- If you want to read a specific file path, use the Read or Glob tool instead
- If you are searching for a specific class definition like "class Foo", use the Glob tool instead
- If you are searching for code within a specific file or set of 2-3 files, use the Read tool instead
- Other tasks that are not related to the agent descriptions above

## Usage Notes

- Always include a short description (3-5 words) summarizing what the agent will do
- Launch multiple agents concurrently whenever possible, to maximize performance; use a single message with multiple tool uses
- When the agent is done, it will return a single message back to you. The result returned by the agent is not visible to the user. To show the user the result, send a text message with a concise summary.
- Agents can be resumed using the `resume` parameter by passing the agent ID from a previous invocation
- Provide clear, detailed prompts so the agent can work autonomously
- Each invocation starts fresh - provide all necessary context in the prompt
- Clearly tell the agent whether you expect it to write code or just do research

## Parameters

- `description` (required): A short (3-5 word) description of the task
- `prompt` (required): The task for the agent to perform
- `subagent_type` (required): The type of specialized agent to use
- `model` (optional): Model to use (sonnet, opus, haiku). If not specified, inherits from parent
- `max_turns` (optional): Maximum number of agentic turns before stopping
- `run_in_background` (optional): Set to true to run the agent in the background
- `resume` (optional): Agent ID to resume from a previous invocation
