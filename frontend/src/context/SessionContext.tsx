import { createContext, useContext, useState, useCallback, useEffect, useRef, ReactNode } from 'react'
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
  approveToolForSession: (toolId: string, toolName: string, sessionId?: string) => Promise<void>
  approveAllForSession: (toolId: string, sessionId?: string) => Promise<void>

  // Question answering
  answerQuestion: (requestId: string, answers: Record<string, string>, sessionId?: string) => Promise<void>

  // Cancel current turn
  cancelSession: (sessionId?: string) => Promise<void>

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

function truncateStr(s: string, max: number): string {
  if (s.length <= max) return s
  return s.slice(0, max - 3) + '...'
}

/** Format ephemeral display for tool execution (up to 3 lines) */
function formatEphemeral(toolName: string, args: Record<string, unknown>): string {
  const lines: string[] = []

  switch (toolName) {
    case 'Read':
    case 'Glob': {
      const path = (args.file_path as string) || (args.pattern as string) || '?'
      lines.push(`${toolName}: ${truncateStr(path, 60)}`)
      break
    }
    case 'Write': {
      const path = args.file_path as string
      if (path) lines.push(`Write: ${truncateStr(path, 60)}`)
      const content = args.content as string
      if (content) {
        const lineCount = content.split('\n').length
        lines.push(`  ${lineCount} lines`)
      }
      break
    }
    case 'Edit': {
      const path = args.file_path as string
      if (path) lines.push(`Edit: ${truncateStr(path, 60)}`)
      const oldStr = args.old_string as string
      if (oldStr) {
        const preview = oldStr.split('\n')[0] || ''
        lines.push(`  - ${truncateStr(preview, 50)}`)
      }
      const newStr = args.new_string as string
      if (newStr) {
        const preview = newStr.split('\n')[0] || ''
        lines.push(`  + ${truncateStr(preview, 50)}`)
      }
      break
    }
    case 'Grep': {
      const pattern = (args.pattern as string) || '?'
      const path = (args.path as string) || '.'
      lines.push(`Grep: ${truncateStr(pattern, 30)} in ${truncateStr(path, 30)}`)
      break
    }
    case 'Bash': {
      const cmd = args.command as string
      if (cmd) {
        const firstLine = cmd.split('\n')[0] || cmd
        lines.push(`Bash: ${truncateStr(firstLine, 60)}`)
        const lineCount = cmd.split('\n').length
        if (lineCount > 1) {
          lines.push(`  (${lineCount} lines)`)
        }
      }
      break
    }
    case 'Task': {
      const desc = (args.description as string) || '?'
      const agent = (args.subagent_type as string) || '?'
      lines.push(`Task [${agent}]: ${truncateStr(desc, 50)}`)
      break
    }
    default: {
      // Generic: take first string value
      const entries = Object.entries(args)
      for (const [, value] of entries) {
        if (typeof value === 'string' && value.length > 0) {
          lines.push(`${toolName}: ${truncateStr(value, 60)}`)
          break
        }
      }
      if (lines.length === 0) {
        lines.push(`${toolName}: ${truncateStr(JSON.stringify(args), 60)}`)
      }
    }
  }

  return lines.slice(0, 3).join('\n')
}

export function SessionProvider({ children }: SessionProviderProps) {
  const [sessions, setSessions] = useState<Map<string, Session>>(new Map())
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null)
  const [isInitialized, setIsInitialized] = useState(false)
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null)

  // Per-session auto-approve state (doesn't need re-renders)
  const sessionApprovals = useRef<Map<string, { tools: Set<string>; all: boolean }>>(new Map())

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

      case 'assistant_message': {
        // Content already has token usage appended by core (when available)
        updateSession(sessionId, s => ({
          ...s,
          status: '',
          ephemeral: null,
          messages: [...s.messages, {
            id: output.id,
            type: 'assistant' as const,
            content: output.content,
          }],
          contextUsage: output.context_usage,
          updatedAt: new Date(),
        }))
        break
      }

      case 'tool_start':
        updateSession(sessionId, s => ({
          ...s,
          status: 'Processing...',
          ephemeral: formatEphemeral(output.name, output.arguments),
          updatedAt: new Date(),
        }))
        break

      case 'tool_pending':
        updateSession(sessionId, s => ({
          ...s,
          modal: {
            type: 'approval',
            id: output.id,
            name: output.name,
            arguments: output.arguments,
            // If from subagent, route approvals there
            targetSessionId: output.subagent_id,
          },
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

      case 'tool_call':
        // Add tool call as a persistent message with elapsed time
        updateSession(sessionId, s => {
          const elapsedSecs = s.turnStart ? (Date.now() - s.turnStart) / 1000 : 0
          return {
            ...s,
            messages: [...s.messages, {
              id: output.id,
              type: 'tool_call' as const,
              content: '',
              toolName: output.name,
              formatted: output.formatted,
              elapsedSecs,
            }],
            updatedAt: new Date(),
          }
        })
        break

      case 'tool_result':
        // Add tool result as a persistent message with elapsed time
        updateSession(sessionId, s => {
          const elapsedSecs = s.turnStart ? (Date.now() - s.turnStart) / 1000 : 0
          return {
            ...s,
            ephemeral: null,  // Clear ephemeral since we have the result
            messages: [...s.messages, {
              id: `${output.id}-result`,
              type: 'tool_result' as const,
              content: output.output,
              toolName: output.name,
              summary: output.summary,
              success: output.success,
              diffPreview: output.diff_preview || undefined,
              expanded: false,
              elapsedSecs,
            }],
            updatedAt: new Date(),
          }
        })
        break

      case 'question':
        updateSession(sessionId, s => ({
          ...s,
          modal: {
            type: 'question',
            request_id: output.request_id,
            questions: output.questions,
            // If from subagent, route answers there
            targetSessionId: output.subagent_id,
          },
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

      case 'cancelled':
        updateSession(sessionId, s => ({
          ...s,
          status: '',
          ephemeral: null,
          modal: null,
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
        const output = event.payload

        // Auto-approve tools if session has approved them
        if (output.type === 'tool_pending') {
          const approvals = sessionApprovals.current.get(output.session_id)
          if (approvals && (approvals.all || approvals.tools.has(output.name))) {
            invoke('approve_tool', { toolId: output.id, sessionId: output.session_id })
            return
          }
        }

        handleOutput(output)
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

    // Set turnStart for elapsed time tracking
    updateSession(targetId, s => ({ ...s, error: null, turnStart: Date.now() }))
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

  // Approve tool and remember the tool name for auto-approve in this session
  const approveToolForSession = useCallback(async (toolId: string, toolName: string, sessionId?: string) => {
    const targetId = sessionId || activeSessionId
    if (!targetId) throw new Error('No active session')

    // Add tool name to session approvals
    if (!sessionApprovals.current.has(targetId)) {
      sessionApprovals.current.set(targetId, { tools: new Set(), all: false })
    }
    sessionApprovals.current.get(targetId)!.tools.add(toolName)

    await invoke('approve_tool', { toolId, sessionId: targetId })
    updateSession(targetId, s => ({ ...s, modal: null }))
  }, [activeSessionId, updateSession])

  // Approve tool and auto-approve all future tools in this session
  const approveAllForSession = useCallback(async (toolId: string, sessionId?: string) => {
    const targetId = sessionId || activeSessionId
    if (!targetId) throw new Error('No active session')

    // Set approve-all flag for session
    if (!sessionApprovals.current.has(targetId)) {
      sessionApprovals.current.set(targetId, { tools: new Set(), all: true })
    } else {
      sessionApprovals.current.get(targetId)!.all = true
    }

    await invoke('approve_tool', { toolId, sessionId: targetId })
    updateSession(targetId, s => ({ ...s, modal: null }))
  }, [activeSessionId, updateSession])

  // Question answering
  const answerQuestion = useCallback(async (requestId: string, answers: Record<string, string>, sessionId?: string) => {
    const targetId = sessionId || activeSessionId
    if (!targetId) throw new Error('No active session')

    await invoke('answer_question', { sessionId: targetId, requestId, answers })
    updateSession(targetId, s => ({ ...s, modal: null }))
  }, [activeSessionId, updateSession])

  // Cancel current turn
  const cancelSession = useCallback(async (sessionId?: string) => {
    const targetId = sessionId || activeSessionId
    if (!targetId) throw new Error('No active session')

    await invoke('cancel_session', { sessionId: targetId })
    updateSession(targetId, s => ({ ...s, modal: null, status: '', ephemeral: null }))
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
    approveToolForSession,
    approveAllForSession,
    answerQuestion,
    cancelSession,
    getActiveSession,
  }

  return (
    <SessionContext.Provider value={value}>
      {children}
    </SessionContext.Provider>
  )
}
