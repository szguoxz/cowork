# Grep Tool Description

A powerful search tool built on ripgrep.

## Usage

- ALWAYS use Grep for search tasks. NEVER invoke `grep` or `rg` as a Bash command. The Grep tool has been optimized for correct permissions and access.
- Supports full regex syntax (e.g., "log.*Error", "function\\s+\\w+")
- Filter files with glob parameter (e.g., "*.js", "**/*.tsx") or type parameter (e.g., "js", "py", "rust")
- Output modes: "content" shows matching lines, "files_with_matches" shows only file paths (default), "count" shows match counts
- Use Task tool for open-ended searches requiring multiple rounds
- Pattern syntax: Uses ripgrep (not grep) - literal braces need escaping (use `interface\\{\\}` to find `interface{}` in Go code)
- Multiline matching: By default patterns match within single lines only. For cross-line patterns like `struct \\{[\\s\\S]*?field`, use `multiline: true`

## Parameters

- `pattern` (required): The regular expression pattern to search for in file contents
- `path` (optional): File or directory to search in (rg PATH). Defaults to current working directory.
- `glob` (optional): Glob pattern to filter files (e.g. "*.js", "*.{ts,tsx}") - maps to rg --glob
- `type` (optional): File type to search (rg --type). Common types: js, py, rust, go, java, etc.
- `output_mode` (optional): "content", "files_with_matches" (default), or "count"
- `-i` (optional): Case insensitive search
- `-n` (optional): Show line numbers in output (default true for content mode)
- `-A` (optional): Lines to show after each match (requires output_mode: "content")
- `-B` (optional): Lines to show before each match (requires output_mode: "content")
- `-C` (optional): Lines to show before and after each match (requires output_mode: "content")
- `multiline` (optional): Enable multiline mode (default false)
- `head_limit` (optional): Limit output to first N entries (default 0 = unlimited)
- `offset` (optional): Skip first N entries before applying head_limit (default 0)
