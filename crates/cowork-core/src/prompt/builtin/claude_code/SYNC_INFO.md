# Claude Code Prompt Sync Information

## About

This directory contains prompts adapted from Claude Code's prompt system.
The prompts have been pre-expanded to replace build-time template variables
with their literal values.

## Sync Details

- **Synced**: 2026-01-22T04:00:06+00:00
- **Script**: `./scripts/sync-claude-prompts.sh`

## Runtime Variables

The following variables are substituted at runtime by `TemplateVars`:

| Variable | Description |
|----------|-------------|
| `${WORKING_DIRECTORY}` | Current working directory |
| `${IS_GIT_REPO}` | Whether the directory is a git repo |
| `${GIT_STATUS}` | Git status output |
| `${CURRENT_BRANCH}` | Current git branch |
| `${MAIN_BRANCH}` | Main/master branch name |
| `${CURRENT_DATE}` | Today's date |
| `${CURRENT_YEAR}` | Current year |
| `${PLATFORM}` | Operating system (linux, macos, windows) |
| `${OS_VERSION}` | OS version string |
| `${MODEL_INFO}` | Model name and ID |
| `${ASSISTANT_NAME}` | Assistant name (e.g., "Cowork") |
| `${RECENT_COMMITS}` | Recent git commit log |
| `${SECURITY_POLICY}` | Security policy content |
| `${SKILLS_XML}` | Available skills as XML |
| `${MCP_SERVER_INSTRUCTIONS}` | MCP server instructions |

## Maintenance

To validate prompts:

```bash
./scripts/sync-claude-prompts.sh --check
```

To rebuild:

```bash
cargo build
cargo test -p cowork-core prompt::builtin::claude_code
```
