// Auto-generated TypeScript bindings from Rust types
// Regenerate with: cargo test --package cowork-app export_bindings

export type { ChatMessage } from "./ChatMessage";
export type { LoopOutput } from "./LoopOutput";
export type { LoopState } from "./LoopState";
export type { QuestionOption } from "./QuestionOption";
export type { ToolCallInfo } from "./ToolCallInfo";
export type { ToolCallStatus } from "./ToolCallStatus";
export type { UserQuestion } from "./UserQuestion";

// Manual definitions for types with external dependencies
export type { LoopEvent, ContextUsage, ContextBreakdown } from "./LoopEvent";

// Re-import for use in this file
import type { LoopState } from "./LoopState";

// Type guard helpers
export const INACTIVE_LOOP_STATES: readonly LoopState[] = ["idle", "completed", "cancelled", "error"] as const;

export function isLoopActive(state: LoopState): boolean {
  return !INACTIVE_LOOP_STATES.includes(state);
}
