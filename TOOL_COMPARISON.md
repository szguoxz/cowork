# Tool Comparison: Cowork vs Claude Code (CC)

For each tool: CC schema → Cowork schema → Differences → Action.

Rules:
- Keep `integer` types (no need to change to `number`)
- Remove extra params not in CC
- Add missing params from CC
- Align logic and approval levels

---

## 1. Bash — NO CHANGES

Schemas match. `_simulatedSedEdit` is an internal CC UI feature, skip it.

---

## 2. KillShell — APPROVAL FIX

### Schema
Match.

### Approval Level
| CC | Cowork | Action |
|----|--------|--------|
| Auto-approve | `Low` | Change to `None` |

---

## 3. Glob — REMOVE EXTRA PARAM

### CC Schema
```json
{ "pattern": "string (required)", "path": "string" }
```

### Cowork Schema
```json
{ "pattern": "string (required)", "path": "string", "limit": "integer" }
```

### Action
- Remove `limit` from `parameters_schema()`
- Keep the internal 100-file limit in the implementation logic, just don't expose it to LLM

---

## 4. Grep — NO CHANGES

Schemas match (keeping integer types).

---

## 5. Read — NO CHANGES

Schemas match (keeping integer types).

---

## 6. Edit — NO CHANGES

Schemas match exactly.

---

## 7. Write — NO CHANGES

Schemas match exactly.

---

## 8. WebFetch — REMOVE EXTRA PARAMS + ADD FORMAT + APPROVAL FIX

### CC Schema
```json
{ "url": "string, format: uri (required)", "prompt": "string (required)" }
```

### Cowork Schema
```json
{ "url": "string (required)", "prompt": "string (required)", "extract_text": "boolean", "max_length": "integer" }
```

### Action
- Remove `extract_text` from schema (keep internal default `true`)
- Remove `max_length` from schema (keep internal default 50000)
- Add `"format": "uri"` to `url` property
- Change approval from `Low` to `None`
- Update `execute()` — stop reading `extract_text` and `max_length` from params, use hardcoded defaults

---

## 9. WebSearch — APPROVAL FIX

### Schema
Match.

### Approval Level
| CC | Cowork | Action |
|----|--------|--------|
| Auto-approve | `Low` | Change to `None` |

---

## 10. LSP — REMOVE EXTRA PARAM

### CC Schema
```json
{ "operation": "enum (required)", "filePath": "string (required)", "line": "integer (required)", "character": "integer (required)" }
```

### Cowork Schema
```json
{ "operation": "enum (required)", "filePath": "string (required)", "line": "integer (required)", "character": "integer (required)", "query": "string" }
```

### Action
- Remove `query` parameter from schema
- Update `execute()` — remove query parsing logic if any; workspaceSymbol should work without it

---

## 11. NotebookEdit — NO CHANGES

Schemas match.

---

## 12. TodoWrite — ADD MISSING CONSTRAINTS

### CC Schema
```json
{
  "todos": [{
    "content": { "type": "string", "minLength": 1 },
    "status": { "enum": ["pending", "in_progress", "completed"] },
    "activeForm": { "type": "string", "minLength": 1 }
  }]
}
```

### Cowork Schema
Same but missing `minLength: 1` on `content` and `activeForm`.

### Action
- Add `"minLength": 1` to `content` and `activeForm` in schema

---

## 13. Task — ALIGN MODEL ENUM + APPROVAL FIX

### CC Schema
```json
{
  "subagent_type": { "enum": ["Bash", "general-purpose", "statusline-setup", "Explore", "Plan", "claude-code-guide", "code-simplifier"] },
  "model": { "enum": ["sonnet", "opus", "haiku"] },
  "max_turns": { "type": "integer", "exclusiveMinimum": 0 }
}
```

### Cowork Schema
```json
{
  "subagent_type": { "enum": ["Bash", "general-purpose", "Explore", "Plan"] },
  "model": { "enum": ["fast", "balanced", "powerful", "haiku", "sonnet", "opus"] },
  "max_turns": { "type": "integer", "default": 50 }
}
```

### Action
- Change `model` enum to `["sonnet", "opus", "haiku"]` — remove fast/balanced/powerful aliases
- Update `execute()` — remove model alias resolution logic (fast→haiku, balanced→sonnet, powerful→opus)
- Remove `"default": 50` from `max_turns` (CC doesn't specify a default in schema)
- Add `"exclusiveMinimum": 0` to `max_turns`
- Skip extra subagent_types (statusline-setup, claude-code-guide, code-simplifier are CC plugins)
- Change approval from `Low` to `None`

---

## 14. TaskOutput — ADD MISSING CONSTRAINTS

### CC Schema
```json
{ "task_id": "string (required)", "block": "boolean, default: true", "timeout": "integer, default: 30000, min: 0, max: 600000" }
```

### Cowork Schema
```json
{ "task_id": "string (required)", "block": "boolean, default: true", "timeout": "integer, default: 30000" }
```

### Action
- Add `"minimum": 0, "maximum": 600000` to `timeout`

---

## 15. AskUserQuestion — FIX REQUIRED FIELDS

### CC Schema
- `options.items.required`: `["label"]`
- `header`: no maxLength in schema

### Cowork Schema
- `options.items.required`: `["label", "description"]`
- `header`: has `maxLength: 12`

### Action
- Change `options.items.required` to `["label"]` (description is optional)
- Remove `"maxLength": 12` from `header`

---

## 16. EnterPlanMode — REMOVE PARAMS + ALIGN LOGIC

### CC Schema
```json
{ "type": "object", "properties": {} }
```

### Cowork Schema
```json
{ "task_description": "string", "plan_file": "string" }
```

### Action
- Remove `task_description` and `plan_file` from schema (CC has no params)
- Update `execute()`:
  - Generate plan file path internally: `~/.claude/plans/<random-name>.md` (use `dirs::home_dir()` + random adjective-noun pattern)
  - Store generated path in `PlanModeState.plan_file`
  - Remove param parsing for task_description/plan_file
  - CC tells the LLM "write your plan to {plan_file}" in the tool result, include that in response

---

## 17. ExitPlanMode — NO CHANGES

`pushToRemote` and remote session params are CC-specific (Claude.ai integration). Skip them intentionally.

---

## 18. Skill — NO CHANGES

Schemas match exactly.

---

## Approval Level Summary

| Tool | Current | Target | Change? |
|------|---------|--------|---------|
| Bash | Medium | Medium | — |
| KillShell | Low | None | YES |
| Glob | None | None | — |
| Grep | None | None | — |
| Read | None | None | — |
| Edit | High | High | — |
| Write | Low | Low | — |
| WebFetch | Low | None | YES |
| WebSearch | Low | None | YES |
| LSP | None | None | — |
| NotebookEdit | Medium | Medium | — |
| TodoWrite | None | None | — |
| Task | Low | None | YES |
| TaskOutput | None | None | — |
| AskUserQuestion | None | None | — |
| EnterPlanMode | Low | Low | — |
| ExitPlanMode | None | None | — |
| Skill | (missing) | None | ADD |

---

## Execution Checklist

Work through each tool that needs changes, one at a time:

- [ ] **Glob** — remove `limit` from schema
- [ ] **WebFetch** — remove `extract_text`, `max_length`; add `format: uri`; approval→None
- [ ] **LSP** — remove `query` from schema
- [ ] **TodoWrite** — add `minLength: 1` to content/activeForm
- [ ] **Task** — model enum `[sonnet,opus,haiku]`; max_turns constraints; approval→None
- [ ] **TaskOutput** — add min/max to timeout
- [ ] **AskUserQuestion** — fix options.required; remove header maxLength
- [ ] **EnterPlanMode** — remove params; generate plan file internally
- [ ] **KillShell** — approval→None
- [ ] **WebSearch** — approval→None
- [ ] **Skill** — add explicit `approval_level()` returning None
- [ ] Build and test
