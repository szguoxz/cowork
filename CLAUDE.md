# CLAUDE.md

## Coding Practices

- Prefer reusing existing components over duplicating logic.
- No hand-rolled loops when a shared abstraction exists (e.g., use `AgentLoop` for subagents).
- Tool result truncation and context management happen in one place (`AgentLoop`).
- Subagents use the same infrastructure as the main session, configured via `ToolScope`, trust-all approval, disabled hooks, and no persistence.
- Avoid `async_trait` — we use Rust edition 2024 (requires rustc 1.85+), which supports native `async fn` in traits. Use `impl Future` or `async fn` directly in trait definitions instead of the `#[async_trait]` macro.

## Architecture Notes

- `AgentLoop` is the single execution loop for both CLI sessions and subagents.
- `ToolScope` enum controls which tools a subagent can access (Bash, Explore, Plan, GeneralPurpose).
- `SessionConfig` fields `tool_scope`, `enable_hooks`, and `save_session` customize loop behavior for subagents without forking the implementation.
