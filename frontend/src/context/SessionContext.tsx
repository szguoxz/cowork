import { createContext, useContext, useState, useCallback, useEffect, ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { LoopOutput, Session, SessionProvider as SessionProviderType } from '../bindings'
import { createSession, generateSessionId } from '../bindings'

interface SessionContextType {
  sessions: Map<string, Session>
  activeSessionId: string | null
  isInitialized: boolean
  hasApiKey: boolean | null

  // Session management
  setActiveSession: (id: string) => void
  createNewSession: (name?: string, provider?: SessionProviderType) => string
  closeSession: (id: string) => Promise<void>
  updateSessionProvider: (id: string, provider: SessionProviderType) => void

  // Message sending
  sendMessage: (content: string, sessionId?: string) => Promise<void>

  // Tool approval
  approveTool: (toolId: string, sessionId?: string) => Promise<void>
  rejectTool: (toolId: string, sessionId?: string) => Promise<void>

  // Question answering
  answerQuestion: (requestId: string, answers: Record<string, string>, sessionId?: string) => Promise<void>

  // Get active session
  getActiveSession: () => Session | undefined
}

const SessionContext = createContext<SessionContextType | null>(null)

export function useSession() {
  const ctx = useContext(SessionContext)
  if (!ctx) {
    throw new Error('useSession must be used within SessionProvider')
  }
  return ctx
}

interface SessionProviderProps {
  children: ReactNode
}

function summarizeArgs(args: Record<string, unknown>): string {
  const entries = Object.entries(args)
  if (entries.length === 0) return ''
  // Take the first meaningful value
  for (const [, value] of entries) {
    if (typeof value === 'string' && value.length > 0) {
      return value.length > 60 ? value.slice(0, 60) + '...' : value
    }
  }
  return JSON.stringify(args).slice(0, 60)
}

export function SessionProvider({ children }: SessionProviderProps) {
  const [sessions, setSessions] = useState<Map<string, Session>>(new Map())
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
  const [isInitialized, setIsInitialized] = useState(false)
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null)

  // Update a specific session
  const updateSession = useCallback((sessionId: string, updater: (session: Session) => Session) => {
    setSessions(prev => {
      const existing = prev.get(sessionId)
      if (!existing) {
        const newSession = createSession(sessionId)
        const updated = updater(newSession)
        return new Map(prev).set(sessionId, updated)
      }
      const updated = updater(existing)
      return new Map(prev).set(sessionId, updated)
    })
  }, [])

  // Handle session output events
  const handleOutput = useCallback((output: LoopOutput) => {
    const sessionId = output.session_id

    switch (output.type) {
      case 'ready':
        updateSession(sessionId, s => ({ ...s, status: '', isReady: true }))
        break

      case 'idle':
        updateSession(sessionId, s => ({ ...s, status: '', ephemeral: null, updatedAt: new Date() }))
        break

      case 'thinking':
        updateSession(sessionId, s => ({
          ...s,
          status: output.content ? 'Thinking...' : 'Processing...',
          updatedAt: new Date(),
        }))
        break

      case 'user_message':
        updateSession(sessionId, s => ({
          ...s,
          messages: [...s.messages, {
            id: output.id,
            type: 'user' as const,
            content: output.content,
          }],
          updatedAt: new Date(),
        }))
        break

      case 'assistant_message':
        updateSession(sessionId, s => ({
          ...s,
          status: '',
          ephemeral: null,
          messages: [...s.messages, {
            id: output.id,
            type: 'assistant' as const,
            content: output.content,
          }],
          updatedAt: new Date(),
        }))
        break

      case 'tool_start':
        updateSession(sessionId, s => ({
          ...s,
          status: 'Processing...',
          ephemeral: `${output.name}: ${summarizeArgs(output.arguments)}`,
          updatedAt: new Date(),
        }))
        break

      case 'tool_pending':
        updateSession(sessionId, s => ({
          ...s,
          modal: { type: 'approval', id: output.id, name: output.name, arguments: output.arguments },
          updatedAt: new Date(),
        }))
        break

      case 'tool_done':
        updateSession(sessionId, s => ({
          ...s,
          ephemeral: `${output.name}: ${output.success ? 'done' : 'error'}`,
          updatedAt: new Date(),
        }))
        break

      case 'question':
        updateSession(sessionId, s => ({
          ...s,
          modal: { type: 'question', request_id: output.request_id, questions: output.questions },
          updatedAt: new Date(),
        }))
        break

      case 'error':
        updateSession(sessionId, s => ({
          ...s,
          error: output.message,
          status: '',
          updatedAt: new Date(),
        }))
        break

      case 'stopped':
        updateSession(sessionId, s => ({
          ...s,
          isReady: false,
          status: '',
          updatedAt: new Date(),
        }))
        break
    }
  }, [updateSession])

  // Initialize: set up event listener and start loop
  useEffect(() => {
    let unlistenFn: (() => void) | null = null

    const init = async () => {
      // 1. Set up event listener FIRST
      unlistenFn = await listen<LoopOutput>('loop_output', (event) => {
        handleOutput(event.payload)
      })

      // 2. Check API key
      try {
        const hasKey = await invoke<boolean>('check_api_key')
        setHasApiKey(hasKey)

        // 3. Start the loop
        if (hasKey) {
          await invoke('start_loop')

          // Create default session
          const defaultSession = createSession('default', 'Main Session')
          defaultSession.isReady = true
          setSessions(new Map([['default', defaultSession]]))
          setActiveSessionId('default')
        }

        setIsInitialized(true)
      } catch (err) {
        console.error('Init error:', err)
        setHasApiKey(false)
        setIsInitialized(true)
      }
    }

    init()

    return () => {
      if (unlistenFn) unlistenFn()
      invoke('stop_loop').catch(console.error)
    }
  }, [handleOutput])

  // Session management
  const setActiveSession = useCallback((id: string) => {
    if (sessions.has(id)) {
      setActiveSessionId(id)
    }
  }, [sessions])

  const createNewSession = useCallback((name?: string, provider?: SessionProviderType): string => {
    const id = generateSessionId()
    const session = createSession(id, name, provider)
    session.isReady = true
    setSessions(prev => new Map(prev).set(id, session))
    setActiveSessionId(id)
    return id
  }, [])

  const updateSessionProvider = useCallback((id: string, provider: SessionProviderType) => {
    updateSession(id, s => ({ ...s, provider }))
  }, [updateSession])

  const closeSession = useCallback(async (id: string) => {
    try {
      await invoke('stop_loop', { sessionId: id })
    } catch (err) {
      console.error('Failed to stop session:', err)
    }

    setSessions(prev => {
      const next = new Map(prev)
      next.delete(id)
      return next
    })

    if (activeSessionId === id) {
      const remaining = Array.from(sessions.keys()).filter(k => k !== id)
      setActiveSessionId(remaining.length > 0 ? remaining[0] : null)
    }
  }, [activeSessionId, sessions])

  // Message sending
  const sendMessage = useCallback(async (content: string, sessionId?: string) => {
    const targetId = sessionId || activeSessionId
    if (!targetId) throw new Error('No active session')

    updateSession(targetId, s => ({ ...s, error: null }))
    await invoke('send_message', { content, sessionId: targetId })
  }, [activeSessionId, updateSession])

  // Tool approval
  const approveTool = useCallback(async (toolId: string, sessionId?: string) => {
    const targetId = sessionId || activeSessionId
    if (!targetId) throw new Error('No active session')

    await invoke('approve_tool', { toolId, sessionId: targetId })
    updateSession(targetId, s => ({ ...s, modal: null }))
  }, [activeSessionId, updateSession])

  const rejectTool = useCallback(async (toolId: string, sessionId?: string) => {
    const targetId = sessionId || activeSessionId
    if (!targetId) throw new Error('No active session')

    await invoke('reject_tool', { toolId, sessionId: targetId })
    updateSession(targetId, s => ({ ...s, modal: null }))
  }, [activeSessionId, updateSession])

  // Question answering
  const answerQuestion = useCallback(async (requestId: string, answers: Record<string, string>, sessionId?: string) => {
    const targetId = sessionId || activeSessionId
    if (!targetId) throw new Error('No active session')

    await invoke('answer_loop_question', { sessionId: targetId, requestId, answers })
    updateSession(targetId, s => ({ ...s, modal: null }))
  }, [activeSessionId, updateSession])

  const getActiveSession = useCallback(() => {
    return activeSessionId ? sessions.get(activeSessionId) : undefined
  }, [activeSessionId, sessions])

  const value: SessionContextType = {
    sessions,
    activeSessionId,
    isInitialized,
    hasApiKey,
    setActiveSession,
    createNewSession,
    closeSession,
    updateSessionProvider,
    sendMessage,
    approveTool,
    rejectTool,
    answerQuestion,
    getActiveSession,
  }

  return (
    <SessionContext.Provider value={value}>
      {children}
    </SessionContext.Provider>
  )
}
