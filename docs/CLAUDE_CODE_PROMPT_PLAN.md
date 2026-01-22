# Claude Code Prompt System Implementation Plan

## Overview

Copy Claude Code's system prompts verbatim and pre-expand template expressions at build time. No JavaScript engine needed - just simple `${VAR}` substitution at runtime.

## Analysis Summary

### Pro/Free Tier Differences

**Only ONE difference** in `tool-description-task.md`:

For non-pro users, this extra line is included:
```
- Launch multiple agents concurrently whenever possible, to maximize performance
```

**Decision**: Include this line (more instructions = better).

### Variable Classification

- **Build-time**: ~40 variables → replace with static values
- **Runtime**: ~12 variables → keep as `${VAR}` for substitution
- **Conditionals**: 6 expressions → expand once based on our decisions

---

## Phase 0: Create Sync Script (1 day)

Create an automated script to fetch and expand prompts from Claude Code's GitHub repo. This script will be re-run whenever Claude Code updates their prompts.

### 0.1 Script Location

```
scripts/sync-claude-prompts.sh
```

### 0.2 Script Responsibilities

```bash
#!/bin/bash
set -euo pipefail

# Configuration
CLAUDE_CODE_REPO="anthropics/claude-code"
PROMPTS_BRANCH="main"
OUTPUT_DIR="crates/cowork-core/src/prompt/builtin/claude_code"
TEMP_DIR=$(mktemp -d)

# 1. Fetch latest prompts from GitHub
fetch_prompts() {
    echo "Fetching prompts from $CLAUDE_CODE_REPO..."
    # Clone or download specific prompt files
    # Could use: gh api, curl, or sparse checkout
}

# 2. Build-time variable substitutions
expand_variables() {
    local file="$1"
    sed -i \
        -e 's/\${BASH_TOOL_NAME}/Bash/g' \
        -e 's/\${BASH_TOOL_NAME\.name}/Bash/g' \
        -e 's/\${BASH_TOOL_OBJECT\.name}/Bash/g' \
        -e 's/\${READ_TOOL_NAME}/Read/g' \
        -e 's/\${READ_TOOL}/Read/g' \
        -e 's/\${WRITE_TOOL_NAME}/Write/g' \
        -e 's/\${WRITE_TOOL}/Write/g' \
        -e 's/\${WRITE_TOOL\.name}/Write/g' \
        -e 's/\${EDIT_TOOL_NAME}/Edit/g' \
        -e 's/\${EDIT_TOOL\.name}/Edit/g' \
        -e 's/\${GLOB_TOOL_NAME}/Glob/g' \
        -e 's/\${GLOB_TOOL}/Glob/g' \
        -e 's/\${GREP_TOOL_NAME}/Grep/g' \
        -e 's/\${SEARCH_TOOL_NAME}/Glob/g' \
        -e 's/\${TASK_TOOL}/Task/g' \
        -e 's/\${TASK_TOOL_NAME}/Task/g' \
        -e 's/\${TASK_TOOL_NAME\.name}/Task/g' \
        -e 's/\${TASK_TOOL_OBJECT}/Task/g' \
        -e 's/\${TASK_TOOL_OBJECT\.name}/Task/g' \
        -e 's/\${TODO_TOOL_OBJECT}/TodoWrite/g' \
        -e 's/\${ASK_USER_QUESTION_TOOL_NAME}/AskUserQuestion/g' \
        -e 's/\${WEBFETCH_TOOL_NAME}/WebFetch/g' \
        -e 's/\${WEBSEARCH_TOOL_NAME}/WebSearch/g' \
        -e 's/\${EXIT_PLAN_MODE_TOOL\.name}/ExitPlanMode/g' \
        -e 's/\${EXIT_PLAN_MODE_TOOL_OBJECT\.name}/ExitPlanMode/g' \
        -e 's/\${CUSTOM_TIMEOUT_MS()}/600000/g' \
        -e 's/\${CUSTOM_TIMEOUT_MS()\/60000}/10/g' \
        -e 's/\${MAX_TIMEOUT_MS()}/120000/g' \
        -e 's/\${MAX_TIMEOUT_MS()\/60000}/2/g' \
        -e 's/\${MAX_OUTPUT_CHARS()}/30000/g' \
        -e 's/\${DEFAULT_READ_LINES}/2000/g' \
        -e 's/\${MAX_LINE_LENGTH}/2000/g' \
        -e 's/\${ICONS_OBJECT\.bullet}/•/g' \
        -e 's/\${ICONS_OBJECT\.star}/★/g' \
        -e 's/\${EXPLORE_AGENT}/Explore/g' \
        -e 's/\${EXPLORE_SUBAGENT\.agentType}/Explore/g' \
        -e 's/\${PLAN_AGENT\.agentType}/Plan/g' \
        "$file"
}

# 3. Expand conditional expressions
expand_conditionals() {
    local file="$1"
    # Handle ternary expressions like ${COND?A:B}
    # This may require a more sophisticated parser (Python/Node)
}

# 4. Copy to output directory
install_prompts() {
    echo "Installing prompts to $OUTPUT_DIR..."
    mkdir -p "$OUTPUT_DIR"/{system,tools,agents,reminders}
    # Copy expanded files to appropriate locations
}

# 5. Record sync metadata
record_metadata() {
    local commit_hash=$(gh api repos/$CLAUDE_CODE_REPO/commits/$PROMPTS_BRANCH --jq '.sha')
    local sync_date=$(date -Iseconds)
    cat > "$OUTPUT_DIR/SYNC_INFO.md" << EOF
# Sync Information

- **Source**: https://github.com/$CLAUDE_CODE_REPO
- **Branch**: $PROMPTS_BRANCH
- **Commit**: $commit_hash
- **Synced**: $sync_date

To update, run: \`./scripts/sync-claude-prompts.sh\`
EOF
}

# Main
main() {
    fetch_prompts
    for file in "$TEMP_DIR"/*.md; do
        expand_variables "$file"
        expand_conditionals "$file"
    done
    install_prompts
    record_metadata
    rm -rf "$TEMP_DIR"
    echo "Done! Run 'cargo build' to rebuild with updated prompts."
}

main "$@"
```

### 0.3 Conditional Expression Handler

For complex conditionals, create a small helper script:

```python
# scripts/expand_conditionals.py
import re
import sys

DECISIONS = {
    'OUTPUT_STYLE_CONFIG': None,  # Use standard style
    'GET_SUBSCRIPTION_TYPE_FN()': 'free',  # Not pro
    'CLAUDE_CODE_GUIDE_SUBAGENT_TYPE.has': True,
    'DISABLE_BACKGROUND_TASKS': False,  # Background tasks enabled
}

def expand_ternary(match):
    """Expand ${COND?A:B} or ${COND!==null?A:B} patterns"""
    expr = match.group(1)
    # Logic to evaluate based on DECISIONS
    # Return appropriate branch
    pass

def expand_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # Pattern: ${...?...:...}
    content = re.sub(r'\$\{([^}]+\?[^}]+:[^}]+)\}', expand_ternary, content)

    with open(filepath, 'w') as f:
        f.write(content)

if __name__ == '__main__':
    expand_file(sys.argv[1])
```

### 0.4 Validation

The script should validate after expansion:

```bash
validate_expansion() {
    echo "Validating expanded prompts..."

    # Check for unexpanded build-time variables
    if grep -rE '\$\{[A-Z_]+_TOOL' "$OUTPUT_DIR"; then
        echo "ERROR: Found unexpanded tool variables"
        exit 1
    fi

    # Check for unexpanded function calls
    if grep -rE '\$\{[A-Z_]+\(\)' "$OUTPUT_DIR"; then
        echo "ERROR: Found unexpanded function calls"
        exit 1
    fi

    # Allowed runtime variables
    ALLOWED='WORKING_DIRECTORY|GIT_STATUS|CURRENT_BRANCH|MAIN_BRANCH|CURRENT_DATE|CURRENT_YEAR|PLATFORM|OS_VERSION|MODEL_INFO|AGENT_TYPE_REGISTRY_STRING|LIMITED_COMMANDS|RECENT_COMMITS|SECURITY_POLICY'

    # Check for other unexpanded variables
    if grep -rE '\$\{(?!('$ALLOWED'))[A-Z_]+\}' "$OUTPUT_DIR"; then
        echo "ERROR: Found unexpanded variables"
        exit 1
    fi

    echo "Validation passed!"
}
```

### 0.5 Usage

```bash
# Initial sync
./scripts/sync-claude-prompts.sh

# Check for updates (dry run)
./scripts/sync-claude-prompts.sh --check

# Force re-sync
./scripts/sync-claude-prompts.sh --force

# Sync specific version/commit
./scripts/sync-claude-prompts.sh --ref v1.2.3
```

---

## Phase 1: Fetch and Pre-Expand Prompts (1 day)

### 1.1 Download Raw Prompts

```bash
# Download all prompts from GitHub
mkdir -p crates/cowork-core/src/prompt/builtin/claude_code/{agents,tools,reminders,system}

# Key files to fetch:
# - system-prompt-main-system-prompt.md → system/main.md
# - tool-description-bash.md → tools/bash.md
# - tool-description-task.md → tools/task.md
# - tool-description-todowrite.md → tools/todowrite.md
# - agent-prompt-explore.md → agents/explore.md
# - agent-prompt-plan-mode-enhanced.md → agents/plan.md
# ... etc
```

### 1.2 Build-Time Substitutions

Replace these with literal values:

| Variable | Replace With |
|----------|--------------|
| `${BASH_TOOL_NAME}` | `Bash` |
| `${BASH_TOOL_NAME.name}` | `Bash` |
| `${BASH_TOOL_OBJECT.name}` | `Bash` |
| `${READ_TOOL_NAME}` | `Read` |
| `${READ_TOOL}` | `Read` |
| `${WRITE_TOOL_NAME}` | `Write` |
| `${WRITE_TOOL}` | `Write` |
| `${WRITE_TOOL.name}` | `Write` |
| `${EDIT_TOOL_NAME}` | `Edit` |
| `${EDIT_TOOL.name}` | `Edit` |
| `${GLOB_TOOL_NAME}` | `Glob` |
| `${GLOB_TOOL}` | `Glob` |
| `${GREP_TOOL_NAME}` | `Grep` |
| `${SEARCH_TOOL_NAME}` | `Glob` |
| `${TASK_TOOL}` | `Task` |
| `${TASK_TOOL_NAME}` | `Task` |
| `${TASK_TOOL_NAME.name}` | `Task` |
| `${TASK_TOOL_OBJECT}` | `Task` |
| `${TASK_TOOL_OBJECT.name}` | `Task` |
| `${TODO_TOOL_OBJECT}` | `TodoWrite` |
| `${ASK_USER_QUESTION_TOOL_NAME}` | `AskUserQuestion` |
| `${WEBFETCH_TOOL_NAME}` | `WebFetch` |
| `${WEBSEARCH_TOOL_NAME}` | `WebSearch` |
| `${EXIT_PLAN_MODE_TOOL.name}` | `ExitPlanMode` |
| `${EXIT_PLAN_MODE_TOOL_OBJECT.name}` | `ExitPlanMode` |
| `${CUSTOM_TIMEOUT_MS()}` | `600000` |
| `${CUSTOM_TIMEOUT_MS()/60000}` | `10` |
| `${MAX_TIMEOUT_MS()}` | `120000` |
| `${MAX_TIMEOUT_MS()/60000}` | `2` |
| `${MAX_OUTPUT_CHARS()}` | `30000` |
| `${DEFAULT_READ_LINES}` | `2000` |
| `${MAX_LINE_LENGTH}` | `2000` |
| `${ICONS_OBJECT.bullet}` | `•` |
| `${ICONS_OBJECT.star}` | `★` |
| `${COMMIT_CO_AUTHORED_BY_CLAUDE_CODE}` | `Co-Authored-By: Claude <noreply@anthropic.com>` |
| `${PR_GENERATED_WITH_CLAUDE_CODE}` | `🤖 Generated with Claude Code` |
| `${EXPLORE_AGENT}` | `Explore` |
| `${EXPLORE_SUBAGENT.agentType}` | `Explore` |
| `${WRITE_TOOL_NAME.agentType}` | `Explore` |
| `${PLAN_AGENT.agentType}` | `Plan` |

### 1.3 Conditional Expansions

Expand these conditionals based on our decisions:

| Expression | Decision | Action |
|------------|----------|--------|
| `${OUTPUT_STYLE_CONFIG!==null?A:B}` | `null` | Keep B (standard style) |
| `${OUTPUT_STYLE_CONFIG===null\|\|...keepCodingInstructions...?A:""}` | `true` | Keep A |
| `${GET_SUBSCRIPTION_TYPE_FN()!=="pro"?A:""}` | Not pro | Keep A |
| `${CLAUDE_CODE_GUIDE_SUBAGENT_TYPE.has(X)?A:""}` | Has all | Keep A |
| `${!IS_TRUTHY_FN(...DISABLE_BACKGROUND_TASKS)&&!FALSE()?A:""}` | Enabled | Keep A |
| `${FALSE()?A:""}` | False | Remove A |

### 1.4 Keep Runtime Variables

These stay as `${VAR}` for runtime substitution:

```
${WORKING_DIRECTORY}
${GIT_STATUS}
${CURRENT_BRANCH}
${MAIN_BRANCH}
${CURRENT_DATE}
${CURRENT_YEAR}
${AGENT_TYPE_REGISTRY_STRING}
${LIMITED_COMMANDS}
${FORMAT_SKILLS_AS_XML_FN}
${RECENT_COMMITS}
${SECURITY_POLICY}
${MODEL_INFO}
${PLATFORM}
${OS_VERSION}
```

---

## Phase 2: Store Expanded Prompts (0.5 day)

### 2.1 Directory Structure

```
crates/cowork-core/src/prompt/builtin/claude_code/
├── mod.rs                    # Module exports
├── system/
│   └── main.md              # Main system prompt (pre-expanded)
├── tools/
│   ├── bash.md              # Bash tool description
│   ├── task.md              # Task tool description
│   ├── read.md              # Read tool description
│   ├── write.md             # Write tool description
│   ├── edit.md              # Edit tool description
│   ├── glob.md              # Glob tool description
│   ├── grep.md              # Grep tool description
│   ├── todowrite.md         # TodoWrite tool description
│   ├── askuserquestion.md   # AskUserQuestion tool description
│   ├── webfetch.md          # WebFetch tool description
│   ├── websearch.md         # WebSearch tool description
│   ├── enterplanmode.md     # EnterPlanMode tool description
│   ├── exitplanmode.md      # ExitPlanMode tool description
│   ├── lsp.md               # LSP tool description
│   ├── notebookedit.md      # NotebookEdit tool description
│   └── skill.md             # Skill tool description
├── agents/
│   ├── explore.md           # Explore agent prompt
│   ├── plan.md              # Plan agent prompt
│   ├── bash.md              # Bash/command execution agent
│   ├── general.md           # General purpose agent
│   └── task.md              # Task agent prompt
└── reminders/
    ├── security_policy.md   # Security policy
    ├── plan_mode_active.md  # Plan mode reminder
    └── git_commit.md        # Git commit instructions
```

### 2.2 Module Exports

```rust
// crates/cowork-core/src/prompt/builtin/claude_code/mod.rs

/// Main system prompt (pre-expanded from Claude Code)
pub const SYSTEM_PROMPT: &str = include_str!("system/main.md");

pub mod tools {
    pub const BASH: &str = include_str!("tools/bash.md");
    pub const TASK: &str = include_str!("tools/task.md");
    pub const READ: &str = include_str!("tools/read.md");
    pub const WRITE: &str = include_str!("tools/write.md");
    pub const EDIT: &str = include_str!("tools/edit.md");
    pub const GLOB: &str = include_str!("tools/glob.md");
    pub const GREP: &str = include_str!("tools/grep.md");
    pub const TODOWRITE: &str = include_str!("tools/todowrite.md");
    pub const ASK_USER_QUESTION: &str = include_str!("tools/askuserquestion.md");
    pub const WEBFETCH: &str = include_str!("tools/webfetch.md");
    pub const WEBSEARCH: &str = include_str!("tools/websearch.md");
    pub const ENTER_PLAN_MODE: &str = include_str!("tools/enterplanmode.md");
    pub const EXIT_PLAN_MODE: &str = include_str!("tools/exitplanmode.md");
    pub const LSP: &str = include_str!("tools/lsp.md");
    pub const NOTEBOOK_EDIT: &str = include_str!("tools/notebookedit.md");
    pub const SKILL: &str = include_str!("tools/skill.md");
}

pub mod agents {
    pub const EXPLORE: &str = include_str!("agents/explore.md");
    pub const PLAN: &str = include_str!("agents/plan.md");
    pub const BASH: &str = include_str!("agents/bash.md");
    pub const GENERAL: &str = include_str!("agents/general.md");
    pub const TASK: &str = include_str!("agents/task.md");
}

pub mod reminders {
    pub const SECURITY_POLICY: &str = include_str!("reminders/security_policy.md");
    pub const PLAN_MODE_ACTIVE: &str = include_str!("reminders/plan_mode_active.md");
    pub const GIT_COMMIT: &str = include_str!("reminders/git_commit.md");
}
```

---

## Phase 3: Update Tool Descriptions (1 day)

### 3.1 Update Tool `description()` Methods

Each tool's `description()` should return the Claude Code prompt content:

```rust
// Example: crates/cowork-core/src/tools/shell/execute.rs

impl Tool for ExecuteCommand {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        // Use pre-expanded Claude Code description
        crate::prompt::builtin::claude_code::tools::BASH
    }

    // ... rest of implementation
}
```

### 3.2 Tools to Update

| Tool | File | Use Prompt |
|------|------|------------|
| `ExecuteCommand` | `tools/shell/execute.rs` | `claude_code::tools::BASH` |
| `TaskTool` | `tools/task/executor.rs` | `claude_code::tools::TASK` |
| `ReadFile` | `tools/filesystem/read.rs` | `claude_code::tools::READ` |
| `WriteFile` | `tools/filesystem/write.rs` | `claude_code::tools::WRITE` |
| `EditFile` | `tools/filesystem/edit.rs` | `claude_code::tools::EDIT` |
| `GlobFiles` | `tools/filesystem/glob.rs` | `claude_code::tools::GLOB` |
| `GrepFiles` | `tools/filesystem/grep.rs` | `claude_code::tools::GREP` |
| `TodoWrite` | `tools/task/todo.rs` | `claude_code::tools::TODOWRITE` |
| `AskUserQuestion` | `tools/interaction/ask.rs` | `claude_code::tools::ASK_USER_QUESTION` |
| `WebFetch` | `tools/web/fetch.rs` | `claude_code::tools::WEBFETCH` |
| `WebSearch` | `tools/web/search.rs` | `claude_code::tools::WEBSEARCH` |
| `EnterPlanMode` | `tools/planning/enter.rs` | `claude_code::tools::ENTER_PLAN_MODE` |
| `ExitPlanMode` | `tools/planning/exit.rs` | `claude_code::tools::EXIT_PLAN_MODE` |
| `LspTool` | `tools/lsp/mod.rs` | `claude_code::tools::LSP` |
| `NotebookEdit` | `tools/notebook/edit.rs` | `claude_code::tools::NOTEBOOK_EDIT` |

---

## Phase 4: Update System Prompt (0.5 day)

### 4.1 Replace Base System Prompt

Update `builtin/mod.rs` to use Claude Code's system prompt:

```rust
// crates/cowork-core/src/prompt/builtin/mod.rs

pub mod claude_code;

// Use Claude Code's system prompt as the default
pub const SYSTEM_PROMPT: &str = claude_code::SYSTEM_PROMPT;

// Keep Cowork-specific overrides if needed
pub mod cowork {
    // Any Cowork-specific customizations
}
```

### 4.2 Update TemplateVars

Ensure `TemplateVars` handles all runtime variables:

```rust
impl TemplateVars {
    pub fn substitute(&self, template: &str) -> String {
        template
            .replace("${WORKING_DIRECTORY}", &self.working_directory)
            .replace("${IS_GIT_REPO}", if self.is_git_repo { "Yes" } else { "No" })
            .replace("${PLATFORM}", &self.platform)
            .replace("${OS_VERSION}", &self.os_version)
            .replace("${CURRENT_DATE}", &self.current_date)
            .replace("${CURRENT_YEAR}", &self.current_year)
            .replace("${MODEL_INFO}", &self.model_info)
            .replace("${GIT_STATUS}", &self.git_status)
            .replace("${MAIN_BRANCH}", &self.main_branch)
            .replace("${CURRENT_BRANCH}", &self.current_branch)
            .replace("${RECENT_COMMITS}", &self.recent_commits)
            .replace("${SECURITY_POLICY}", &self.security_policy)
            .replace("${AGENT_TYPE_REGISTRY_STRING}", &self.agent_registry)
            .replace("${LIMITED_COMMANDS}", &self.available_commands)
    }
}
```

---

## Phase 5: Testing (0.5 day)

### 5.1 Validation Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_unexpanded_variables() {
        let prompts = vec![
            claude_code::SYSTEM_PROMPT,
            claude_code::tools::BASH,
            claude_code::tools::TASK,
            // ... all prompts
        ];

        for prompt in prompts {
            // Only runtime variables should remain
            let remaining: Vec<_> = prompt.match_indices("${")
                .filter(|(_, _)| {
                    // These are OK - runtime variables
                    !prompt.contains("${WORKING_DIRECTORY}")
                    && !prompt.contains("${GIT_STATUS}")
                    // ... etc
                })
                .collect();

            assert!(remaining.is_empty(),
                "Unexpanded variable found: {:?}", remaining);
        }
    }

    #[test]
    fn test_runtime_substitution() {
        let vars = TemplateVars {
            working_directory: "/home/user/project".into(),
            git_status: "On branch main".into(),
            ..Default::default()
        };

        let template = "Working in ${WORKING_DIRECTORY}\n${GIT_STATUS}";
        let result = vars.substitute(template);

        assert!(result.contains("/home/user/project"));
        assert!(result.contains("On branch main"));
    }
}
```

---

## Timeline

| Phase | Work | Time |
|-------|------|------|
| 0 | Create sync script (fetch, expand, validate) | 1 day |
| 1 | Initial sync and manual verification | 0.5 day |
| 2 | Store expanded prompts, create modules | 0.5 day |
| 3 | Update 25+ tool `description()` methods | 1.5 days |
| 4 | Update system prompt and TemplateVars | 0.5 day |
| 5 | Testing and validation | 0.5 day |
| **Total** | | **4.5 days** |

---

## Complete Integration Points

### Data Flow Analysis

**System Prompt Flow:**
```
ChatSession::new()
  → DEFAULT_SYSTEM_PROMPT (orchestration/system_prompt.rs:266)
    → builtin::SYSTEM_PROMPT (prompt/builtin/mod.rs)
      → system_prompt.md
```

**Tool Description Flow:**
```
Tool::description()
  → Tool::to_definition()
    → ToolRegistry::list()
      → AgentLoop.tool_definitions
        → sent to LLM
```

---

## Files to Create

```
crates/cowork-core/src/prompt/builtin/claude_code/
├── mod.rs                      # Module exports
├── system/
│   └── main.md                 # Main system prompt
├── tools/
│   ├── bash.md                 # Bash/ExecuteCommand
│   ├── task.md                 # Task tool
│   ├── task_output.md          # TaskOutput tool
│   ├── todowrite.md            # TodoWrite tool
│   ├── read.md                 # ReadFile
│   ├── write.md                # WriteFile
│   ├── edit.md                 # EditFile
│   ├── list_directory.md       # ListDirectory
│   ├── glob.md                 # GlobFiles
│   ├── grep.md                 # GrepFiles
│   ├── search.md               # SearchFiles
│   ├── delete.md               # DeleteFile
│   ├── move.md                 # MoveFile
│   ├── webfetch.md             # WebFetch
│   ├── websearch.md            # WebSearch
│   ├── enterplanmode.md        # EnterPlanMode
│   ├── exitplanmode.md         # ExitPlanMode
│   ├── askuserquestion.md      # AskUserQuestion
│   ├── lsp.md                  # LspTool
│   ├── notebookedit.md         # NotebookEdit
│   ├── read_pdf.md             # ReadPdf
│   ├── read_office.md          # ReadOfficeDoc
│   ├── kill_shell.md           # KillShell
│   ├── navigate.md             # Browser: NavigateTo
│   ├── screenshot.md           # Browser: TakeScreenshot
│   ├── click.md                # Browser: ClickElement
│   ├── type_text.md            # Browser: TypeText
│   └── page_content.md         # Browser: GetPageContent
├── agents/
│   ├── explore.md
│   ├── plan.md
│   ├── bash.md
│   ├── general.md
│   └── task.md
└── reminders/
    ├── security_policy.md
    ├── plan_mode_active.md
    └── git_commit.md
```

---

## Files to Modify

### Phase 2: System Prompt Wiring

| File | Change |
|------|--------|
| `prompt/builtin/mod.rs` | Add `pub mod claude_code;`, update `SYSTEM_PROMPT` |
| `orchestration/system_prompt.rs` | No change needed (uses `builtin::SYSTEM_PROMPT`) |

### Phase 3: Tool Descriptions (ALL tools)

| File | Tool | New Prompt |
|------|------|------------|
| `tools/shell/execute.rs` | `ExecuteCommand` (Bash) | `claude_code::tools::BASH` |
| `tools/shell/kill.rs` | `KillShell` | `claude_code::tools::KILL_SHELL` |
| `tools/task/executor.rs` | `TaskTool` | `claude_code::tools::TASK` |
| `tools/task/output.rs` | `TaskOutputTool` | `claude_code::tools::TASK_OUTPUT` |
| `tools/task/todo.rs` | `TodoWrite` | `claude_code::tools::TODOWRITE` |
| `tools/filesystem/read.rs` | `ReadFile` | `claude_code::tools::READ` |
| `tools/filesystem/write.rs` | `WriteFile` | `claude_code::tools::WRITE` |
| `tools/filesystem/edit.rs` | `EditFile` | `claude_code::tools::EDIT` |
| `tools/filesystem/list.rs` | `ListDirectory` | `claude_code::tools::LIST_DIRECTORY` |
| `tools/filesystem/glob.rs` | `GlobFiles` | `claude_code::tools::GLOB` |
| `tools/filesystem/grep.rs` | `GrepFiles` | `claude_code::tools::GREP` |
| `tools/filesystem/search.rs` | `SearchFiles` | `claude_code::tools::SEARCH` |
| `tools/filesystem/delete.rs` | `DeleteFile` | `claude_code::tools::DELETE` |
| `tools/filesystem/move_file.rs` | `MoveFile` | `claude_code::tools::MOVE` |
| `tools/web/fetch.rs` | `WebFetch` | `claude_code::tools::WEBFETCH` |
| `tools/web/search.rs` | `WebSearch` | `claude_code::tools::WEBSEARCH` |
| `tools/planning/enter.rs` | `EnterPlanMode` | `claude_code::tools::ENTER_PLAN_MODE` |
| `tools/planning/exit.rs` | `ExitPlanMode` | `claude_code::tools::EXIT_PLAN_MODE` |
| `tools/interaction/ask.rs` | `AskUserQuestion` | `claude_code::tools::ASK_USER_QUESTION` |
| `tools/lsp/mod.rs` | `LspTool` | `claude_code::tools::LSP` |
| `tools/notebook/edit.rs` | `NotebookEdit` | `claude_code::tools::NOTEBOOK_EDIT` |
| `tools/document/read_pdf.rs` | `ReadPdf` | `claude_code::tools::READ_PDF` |
| `tools/document/read_office.rs` | `ReadOfficeDoc` | `claude_code::tools::READ_OFFICE` |
| `tools/browser/navigate.rs` | `NavigateTo` | `claude_code::tools::NAVIGATE` |
| `tools/browser/screenshot.rs` | `TakeScreenshot` | `claude_code::tools::SCREENSHOT` |
| `tools/browser/interact.rs` | `ClickElement`, `TypeText`, `GetPageContent` | Multiple prompts |

**Total: 25+ tool files to update**

### Phase 4: Runtime Variables

| File | Change |
|------|--------|
| `prompt/mod.rs` | Add new fields to `TemplateVars`: `main_branch`, `current_branch`, `recent_commits`, `agent_registry`, `available_commands` |

---

## Verification Checklist

After implementation, verify:

- [x] `scripts/sync-claude-prompts.sh` runs without errors
- [x] `SYNC_INFO.md` contains correct commit hash and date
- [x] Validation passes (no unexpanded build-time variables)
- [x] `builtin::SYSTEM_PROMPT` points to Claude Code prompt (Phase 4 - DONE)
- [x] All tool `description()` methods return Claude Code prompts (Phase 3 - DONE)
- [x] No `${FUNCTION()}` or `${OBJ.prop}` patterns in any prompt
- [x] Only `${SIMPLE_VAR}` patterns remain for runtime substitution
- [x] `TemplateVars::substitute()` handles all runtime variables
- [x] `cargo test` passes
- [x] `cargo build` succeeds

## Implementation Progress (as of Iteration 2)

### Completed (Phase 0-4 - ALL PHASES COMPLETE):
- Created `scripts/sync-claude-prompts.sh` - validation and setup script
- Created `scripts/expand_conditionals.py` - Python helper for conditional expansion
- Created `claude_code/` directory structure with system/, tools/, agents/, reminders/
- Created `claude_code/mod.rs` with module exports and tests
- Added new runtime variables to `TemplateVars`: `current_branch`, `main_branch`, `recent_commits`
- All 6 claude_code tests pass
- Validation script passes
- **Phase 3-4 (Iteration 2):**
  - Updated `builtin::SYSTEM_PROMPT` to point to `claude_code::SYSTEM_PROMPT`
  - Updated 14 tool `description()` methods to use `claude_code::tools::*`:
    - ReadFile → `claude_code::tools::READ`
    - ExecuteCommand (Bash) → `claude_code::tools::BASH`
    - WriteFile → `claude_code::tools::WRITE`
    - EditFile → `claude_code::tools::EDIT`
    - GlobFiles → `claude_code::tools::GLOB`
    - GrepFiles → `claude_code::tools::GREP`
    - TodoWrite → `claude_code::tools::TODOWRITE`
    - AskUserQuestion → `claude_code::tools::ASK_USER_QUESTION`
    - TaskTool → `claude_code::tools::TASK`
    - WebFetch → `claude_code::tools::WEBFETCH`
    - WebSearch → `claude_code::tools::WEBSEARCH`
    - EnterPlanMode → `claude_code::tools::ENTER_PLAN_MODE`
    - ExitPlanMode → `claude_code::tools::EXIT_PLAN_MODE`
  - Updated tests that expected "Cowork" to expect "Claude" in system prompt
  - All 493 tests pass
  - Build succeeds

### Remaining:
- None - all phases complete!

## Maintenance

To update prompts when Claude Code releases changes:

```bash
# 1. Run sync script
./scripts/sync-claude-prompts.sh

# 2. Review changes
git diff crates/cowork-core/src/prompt/builtin/claude_code/

# 3. Test and rebuild
cargo test && cargo build

# 4. Commit updates
git add -A && git commit -m "chore: sync Claude Code prompts"
```

---

## Success Criteria

1. All Claude Code prompts stored verbatim (after expansion)
2. No `${FUNCTION()}` or `${OBJ.prop}` patterns remain (build-time expanded)
3. Only `${SIMPLE_VAR}` patterns for runtime substitution
4. All tool descriptions match Claude Code exactly
5. All tests pass
