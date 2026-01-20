/**
 * Session types for multi-session support
 */

export interface ToolInfo {
  id: string
  name: string
  arguments: Record<string, unknown>
  status: 'pending' | 'executing' | 'done' | 'failed'
  output?: string
}

export interface Message {
  id: string
  type: 'user' | 'assistant' | 'tool'
  content: string
  tool?: ToolInfo
}

export interface SessionProvider {
  type: string  // 'anthropic', 'openai', 'deepseek', etc.
  model: string
}

export interface Session {
  id: string
  name: string
  messages: Message[]
  isIdle: boolean
  isReady: boolean
  error: string | null
  provider?: SessionProvider  // Per-session provider override
  createdAt: Date
  updatedAt: Date
}

export function createSession(id: string, name?: string, provider?: SessionProvider): Session {
  const now = new Date()
  return {
    id,
    name: name || `Chat ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`,
    messages: [],
    isIdle: false,
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
