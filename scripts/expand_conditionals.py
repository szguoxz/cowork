#!/usr/bin/env python3
"""
Expand conditional expressions in Claude Code prompts.

This script handles the following patterns:
- ${COND?A:B} - Ternary expressions
- ${COND!==null?A:B} - Null check ternaries
- ${COND===null||...?A:""} - Complex boolean expressions
- ${!IS_TRUTHY_FN(...)&&...?A:""} - Function-based conditionals

Usage:
    python3 expand_conditionals.py <file>
    python3 expand_conditionals.py --all <directory>
"""

import re
import sys
import os
from pathlib import Path
from typing import Optional

# Decision table for conditional expressions
# These are the decisions made for Cowork based on the plan
DECISIONS = {
    # Output style: Use standard style (not custom)
    'OUTPUT_STYLE_CONFIG': None,

    # Subscription type: Not pro (include extra instructions)
    'GET_SUBSCRIPTION_TYPE_FN()': 'free',

    # Claude Code guide subagent types: Has all types
    'CLAUDE_CODE_GUIDE_SUBAGENT_TYPE.has': True,

    # Background tasks: Enabled
    'DISABLE_BACKGROUND_TASKS': False,

    # FALSE() always returns false
    'FALSE()': False,

    # TRUE() always returns true
    'TRUE()': True,
}

# Build-time variable substitutions
BUILD_TIME_VARS = {
    # Tool names
    'BASH_TOOL_NAME': 'Bash',
    'BASH_TOOL_NAME.name': 'Bash',
    'BASH_TOOL_OBJECT.name': 'Bash',
    'READ_TOOL_NAME': 'Read',
    'READ_TOOL': 'Read',
    'WRITE_TOOL_NAME': 'Write',
    'WRITE_TOOL': 'Write',
    'WRITE_TOOL.name': 'Write',
    'EDIT_TOOL_NAME': 'Edit',
    'EDIT_TOOL.name': 'Edit',
    'GLOB_TOOL_NAME': 'Glob',
    'GLOB_TOOL': 'Glob',
    'GREP_TOOL_NAME': 'Grep',
    'SEARCH_TOOL_NAME': 'Glob',
    'TASK_TOOL': 'Task',
    'TASK_TOOL_NAME': 'Task',
    'TASK_TOOL_NAME.name': 'Task',
    'TASK_TOOL_OBJECT': 'Task',
    'TASK_TOOL_OBJECT.name': 'Task',
    'TODO_TOOL_OBJECT': 'TodoWrite',
    'ASK_USER_QUESTION_TOOL_NAME': 'AskUserQuestion',
    'WEBFETCH_TOOL_NAME': 'WebFetch',
    'WEBSEARCH_TOOL_NAME': 'WebSearch',
    'EXIT_PLAN_MODE_TOOL.name': 'ExitPlanMode',
    'EXIT_PLAN_MODE_TOOL_OBJECT.name': 'ExitPlanMode',
    'ENTER_PLAN_MODE_TOOL.name': 'EnterPlanMode',

    # Timeouts and limits
    'CUSTOM_TIMEOUT_MS()': '600000',
    'CUSTOM_TIMEOUT_MS()/60000': '10',
    'MAX_TIMEOUT_MS()': '120000',
    'MAX_TIMEOUT_MS()/60000': '2',
    'MAX_OUTPUT_CHARS()': '30000',
    'DEFAULT_READ_LINES': '2000',
    'MAX_LINE_LENGTH': '2000',

    # Icons
    'ICONS_OBJECT.bullet': '•',
    'ICONS_OBJECT.star': '★',

    # Git messages
    'COMMIT_CO_AUTHORED_BY_CLAUDE_CODE': 'Co-Authored-By: Claude <noreply@anthropic.com>',
    'PR_GENERATED_WITH_CLAUDE_CODE': 'Generated with Claude Code',

    # Agent types
    'EXPLORE_AGENT': 'Explore',
    'EXPLORE_SUBAGENT.agentType': 'Explore',
    'PLAN_AGENT.agentType': 'Plan',
}


def expand_simple_var(match: re.Match) -> str:
    """Expand simple ${VAR} patterns."""
    var_name = match.group(1)
    if var_name in BUILD_TIME_VARS:
        return BUILD_TIME_VARS[var_name]
    # Leave runtime variables as-is
    return match.group(0)


def evaluate_condition(cond: str) -> bool:
    """Evaluate a conditional expression based on DECISIONS."""
    cond = cond.strip()

    # FALSE()
    if cond == 'FALSE()' or cond == 'FALSE':
        return False

    # TRUE()
    if cond == 'TRUE()' or cond == 'TRUE':
        return True

    # !==null checks (variable is not null)
    if '!==null' in cond:
        var_name = cond.replace('!==null', '').strip()
        val = DECISIONS.get(var_name, DECISIONS.get(var_name.replace('()', ''), None))
        return val is not None

    # ===null checks (variable is null)
    if '===null' in cond:
        var_name = cond.replace('===null', '').strip()
        val = DECISIONS.get(var_name, DECISIONS.get(var_name.replace('()', ''), None))
        return val is None

    # !=="pro" check
    if '!=="pro"' in cond:
        return DECISIONS.get('GET_SUBSCRIPTION_TYPE_FN()', 'free') != 'pro'

    # Function calls with .has
    if '.has' in cond:
        return DECISIONS.get('CLAUDE_CODE_GUIDE_SUBAGENT_TYPE.has', True)

    # Negation
    if cond.startswith('!'):
        inner = cond[1:]
        return not evaluate_condition(inner)

    # IS_TRUTHY_FN
    if 'IS_TRUTHY_FN' in cond:
        # Extract the argument
        match = re.search(r'IS_TRUTHY_FN\s*\(\s*[^)]*DISABLE_BACKGROUND_TASKS[^)]*\)', cond)
        if match:
            return not DECISIONS.get('DISABLE_BACKGROUND_TASKS', False)
        return True

    # && (AND) operator
    if '&&' in cond:
        parts = cond.split('&&')
        return all(evaluate_condition(p) for p in parts)

    # || (OR) operator
    if '||' in cond:
        parts = cond.split('||')
        return any(evaluate_condition(p) for p in parts)

    # Default: check DECISIONS
    if cond in DECISIONS:
        return bool(DECISIONS[cond])

    # Unknown condition - default to True to include content
    return True


def expand_ternary(match: re.Match) -> str:
    """Expand ${COND?A:B} ternary patterns."""
    full_expr = match.group(1)

    # Find the ? and : positions (accounting for nested content)
    q_pos = full_expr.find('?')
    if q_pos == -1:
        return match.group(0)  # Not a ternary, return as-is

    condition = full_expr[:q_pos].strip()
    rest = full_expr[q_pos + 1:]

    # Find the colon, but be careful about nested content
    # Simple approach: find last colon
    colon_pos = rest.rfind(':')
    if colon_pos == -1:
        return match.group(0)  # Malformed, return as-is

    true_branch = rest[:colon_pos]
    false_branch = rest[colon_pos + 1:]

    # Handle empty branches (often shown as "")
    true_branch = true_branch.strip().strip('"')
    false_branch = false_branch.strip().strip('"')

    # Evaluate and return appropriate branch
    if evaluate_condition(condition):
        return true_branch
    else:
        return false_branch


def expand_content(content: str) -> str:
    """Expand all template expressions in content."""
    # First, expand ternary expressions ${COND?A:B}
    # This pattern matches ${...?...:...}
    ternary_pattern = r'\$\{([^{}]+\?[^{}]+:[^{}]*)\}'
    content = re.sub(ternary_pattern, expand_ternary, content)

    # Then expand simple variables ${VAR}
    simple_pattern = r'\$\{([A-Z_][A-Z0-9_]*(?:\.[a-zA-Z_]+)?(?:\(\))?(?:/\d+)?)\}'
    content = re.sub(simple_pattern, expand_simple_var, content)

    return content


def process_file(filepath: Path) -> bool:
    """Process a single file and expand all conditionals."""
    try:
        content = filepath.read_text(encoding='utf-8')
        expanded = expand_content(content)

        if content != expanded:
            filepath.write_text(expanded, encoding='utf-8')
            print(f"Expanded: {filepath}")
            return True
        else:
            print(f"No changes: {filepath}")
            return False

    except Exception as e:
        print(f"Error processing {filepath}: {e}", file=sys.stderr)
        return False


def process_directory(dirpath: Path) -> int:
    """Process all .md files in a directory recursively."""
    count = 0
    for filepath in dirpath.rglob('*.md'):
        if process_file(filepath):
            count += 1
    return count


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    if sys.argv[1] == '--help' or sys.argv[1] == '-h':
        print(__doc__)
        sys.exit(0)

    if sys.argv[1] == '--all':
        if len(sys.argv) < 3:
            print("Error: --all requires a directory argument", file=sys.stderr)
            sys.exit(1)
        dirpath = Path(sys.argv[2])
        if not dirpath.is_dir():
            print(f"Error: {dirpath} is not a directory", file=sys.stderr)
            sys.exit(1)
        count = process_directory(dirpath)
        print(f"Expanded {count} file(s)")
    else:
        filepath = Path(sys.argv[1])
        if not filepath.is_file():
            print(f"Error: {filepath} is not a file", file=sys.stderr)
            sys.exit(1)
        process_file(filepath)


if __name__ == '__main__':
    main()
