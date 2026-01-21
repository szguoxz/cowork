# ExitPlanMode Tool Description

Use this tool when you are in plan mode and have finished writing your plan to the plan file and are ready for user approval.

## How This Tool Works

- You should have already written your plan to the plan file specified in the plan mode system message
- This tool does NOT take the plan content as a parameter - it will read the plan from the file you wrote
- This tool simply signals that you're done planning and ready for the user to review and approve
- The user will see the contents of your plan file when they review it

## Requesting Permissions (allowedPrompts)

When calling this tool, you can request prompt-based permissions for bash commands your plan will need. These are semantic descriptions of actions, not literal commands.

**How to use:**
```json
{
  "allowedPrompts": [
    { "tool": "Bash", "prompt": "run tests" },
    { "tool": "Bash", "prompt": "install dependencies" },
    { "tool": "Bash", "prompt": "build the project" }
  ]
}
```

**Guidelines for prompts:**
- Use semantic descriptions that capture the action's purpose, not specific commands
- "run tests" matches: npm test, pytest, go test, bun test, etc.
- "install dependencies" matches: npm install, pip install, cargo build, etc.
- "build the project" matches: npm run build, make, cargo build, etc.
- Keep descriptions concise but descriptive
- Only request permissions you actually need for the plan
- Scope permissions narrowly, like a security-conscious human would
- Never combine multiple actions into one permission - split them into separate, specific permissions
- Prefer "run read-only database queries" over "run database queries"
- Prefer "run tests in the project" over "run code"
- Add constraints like "read-only", "local", "non-destructive" whenever possible

## When to Use This Tool

IMPORTANT: Only use this tool when the task requires planning the implementation steps of a task that requires writing code. For research tasks where you're gathering information, searching files, reading files or in general trying to understand the codebase - do NOT use this tool.

## Before Using This Tool

Ensure your plan is complete and unambiguous:
- If you have unresolved questions about requirements or approach, use AskUserQuestion first (in earlier phases)
- Once your plan is finalized, use THIS tool to request approval

**Important:** Do NOT use AskUserQuestion to ask "Is this plan okay?" or "Should I proceed?" - that's exactly what THIS tool does. ExitPlanMode inherently requests user approval of your plan.

## Parameters

- `allowedPrompts` (optional): Array of prompt-based permissions needed for the plan, each with:
  - `tool` (required): The tool this prompt applies to (currently only "Bash")
  - `prompt` (required): Semantic description of the action (e.g., "run tests", "install dependencies")
