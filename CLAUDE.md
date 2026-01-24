# CLAUDE.md

## Coding Practices

- No duplicating logic.
- Avoid `async_trait` — we use Rust edition 2024 (requires rustc 1.85+), which supports native `async fn` in traits.

## Architecture Notes

- `AgentLoop` is the single execution loop for both CLI sessions and subagents.