// Manual TypeScript definition for LoopEvent
// This type has dependencies on cowork-core types that can't be auto-generated
// Keep in sync with: crates/cowork-app/src/agentic_loop.rs LoopEvent enum

import type { ChatMessage } from "./ChatMessage";
import type { LoopState } from "./LoopState";
import type { ToolCallInfo } from "./ToolCallInfo";
import type { UserQuestion } from "./UserQuestion";

/**
 * Context token breakdown
 */
export interface ContextBreakdown {
  system_tokens: number;
  conversation_tokens: number;
  tool_tokens: number;
  memory_tokens: number;
}

/**
 * Context usage information
 */
export interface ContextUsage {
  used_tokens: number;
  limit_tokens: number;
  used_percentage: number;
  remaining_tokens: number;
  should_compact: boolean;
  breakdown: ContextBreakdown;
}

/**
 * Events emitted by the agentic loop to the frontend
 * Tagged union with "type" discriminator
 */
export type LoopEvent =
  | { type: "state_changed"; session_id: string; state: LoopState }
  | { type: "text_delta"; session_id: string; delta: string }
  | { type: "message_added"; session_id: string; message: ChatMessage }
  | { type: "tool_approval_needed"; session_id: string; tool_calls: ToolCallInfo[] }
  | { type: "tool_execution_started"; session_id: string; tool_call_id: string; tool_name: string }
  | { type: "tool_execution_completed"; session_id: string; tool_call_id: string; result: string; success: boolean }
  | { type: "question_requested"; session_id: string; request_id: string; tool_call_id: string; questions: UserQuestion[] }
  | { type: "loop_completed"; session_id: string }
  | { type: "loop_error"; session_id: string; error: string }
  | { type: "context_usage"; session_id: string; usage: ContextUsage }
  | { type: "auto_compact_started"; session_id: string; tokens_before: number }
  | { type: "auto_compact_completed"; session_id: string; tokens_before: number; tokens_after: number; messages_removed: number };
