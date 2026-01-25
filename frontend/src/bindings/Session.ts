/**
 * Session types for multi-session support
 * Simplified: tools are ephemeral, questions/approvals are modals
 */

import type { DiffLine } from './LoopOutput'

export interface SessionProvider {
  type: string  // 'anthropic', 'openai', 'deepseek', etc.
  model: string
}

export interface Message {
  id: string
  type: 'user' | 'assistant' | 'tool_call' | 'tool_result'
  content: string
  // Tool call specific
  toolName?: string
  formatted?: string
  // Tool result specific
  summary?: string
  success?: boolean
  diffPreview?: DiffLine[]
  expanded?: boolean
  // Timing
  elapsedSecs?: number
}

export interface QuestionData {
  question: string
  header: string | null
  options: { label: string; description: string | null }[]
  multi_select: boolean
}

export type Modal =
  | { type: 'approval'; id: string; name: string; arguments: Record<string, unknown> }
  | { type: 'question'; request_id: string; questions: QuestionData[] }

export interface Session {
  id: string
  name: string
  messages: Message[]
  ephemeral: string | null    // Current tool activity line (overwritten each event)
  status: string              // "Processing...", "Thinking...", "" (idle)
  modal: Modal | null         // One pending approval or question
  isReady: boolean
  error: string | null
  provider?: SessionProvider
  createdAt: Date
  updatedAt: Date
  turnStart?: number          // Timestamp when user submitted message (ms since epoch)
}

export function createSession(id: string, name?: string, provider?: SessionProvider): Session {
  const now = new Date()
  return {
    id,
    name: name || `Chat ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`,
    messages: [],
    ephemeral: null,
    status: '',
    modal: null,
    isReady: false,
    error: null,
    provider,
    createdAt: now,
    updatedAt: now,
  }
}

export function generateSessionId(): string {
  return `session-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
}
