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

export interface Session {
  id: string
  name: string
  messages: Message[]
  isIdle: boolean
  isReady: boolean
  error: string | null
  createdAt: Date
  updatedAt: Date
}

export function createSession(id: string, name?: string): Session {
  const now = new Date()
  return {
    id,
    name: name || `Session ${id}`,
    messages: [],
    isIdle: false,
    isReady: false,
    error: null,
    createdAt: now,
    updatedAt: now,
  }
}

export function generateSessionId(): string {
  return `session-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`
}
