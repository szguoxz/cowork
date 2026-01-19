import { useEffect, useCallback, useState, useRef } from 'react'
import { listen, UnlistenFn } from '@tauri-apps/api/event'

export interface StreamEvent {
  type: 'start' | 'thinking_delta' | 'text_delta' | 'tool_call_start' | 'tool_call_delta' | 'tool_call_complete' | 'end' | 'error'
  session_id: string
  message_id?: string
  delta?: string
  accumulated?: string
  tool_call_id?: string
  tool_name?: string
  tool_call?: ToolCallInfo
  finish_reason?: string
  error?: string
}

export interface ToolCallInfo {
  id: string
  name: string
  arguments: Record<string, unknown>
  status: 'Pending' | 'Approved' | 'Rejected' | 'Executing' | 'Completed' | 'Failed'
  result?: string
}

export interface StreamingState {
  isStreaming: boolean
  currentThinking: string
  currentText: string
  currentToolCalls: Map<string, ToolCallInfo>
  error: string | null
}

export interface UseStreamingOptions {
  sessionId: string | null
  onThinkingDelta?: (delta: string, accumulated: string) => void
  onTextDelta?: (delta: string, accumulated: string) => void
  onToolCallStart?: (id: string, name: string) => void
  onToolCallComplete?: (toolCall: ToolCallInfo) => void
  onEnd?: (finishReason: string) => void
  onError?: (error: string) => void
}

export function useStreaming({
  sessionId,
  onThinkingDelta,
  onTextDelta,
  onToolCallStart,
  onToolCallComplete,
  onEnd,
  onError,
}: UseStreamingOptions) {
  const [state, setState] = useState<StreamingState>({
    isStreaming: false,
    currentThinking: '',
    currentText: '',
    currentToolCalls: new Map(),
    error: null,
  })

  const unlistenRef = useRef<UnlistenFn | null>(null)

  // Handle stream events
  const handleEvent = useCallback(
    (event: StreamEvent) => {
      switch (event.type) {
        case 'start':
          setState((s) => ({
            ...s,
            isStreaming: true,
            currentThinking: '',
            currentText: '',
            currentToolCalls: new Map(),
            error: null,
          }))
          break

        case 'thinking_delta':
          setState((s) => ({
            ...s,
            currentThinking: event.accumulated || s.currentThinking + (event.delta || ''),
          }))
          onThinkingDelta?.(event.delta || '', event.accumulated || '')
          break

        case 'text_delta':
          setState((s) => ({
            ...s,
            currentText: event.accumulated || s.currentText + (event.delta || ''),
          }))
          onTextDelta?.(event.delta || '', event.accumulated || '')
          break

        case 'tool_call_start':
          if (event.tool_call_id && event.tool_name) {
            setState((s) => {
              const newCalls = new Map(s.currentToolCalls)
              newCalls.set(event.tool_call_id!, {
                id: event.tool_call_id!,
                name: event.tool_name!,
                arguments: {},
                status: 'Pending',
              })
              return { ...s, currentToolCalls: newCalls }
            })
            onToolCallStart?.(event.tool_call_id, event.tool_name)
          }
          break

        case 'tool_call_delta':
          // Tool call arguments are being streamed - update the raw JSON
          break

        case 'tool_call_complete':
          if (event.tool_call) {
            setState((s) => {
              const newCalls = new Map(s.currentToolCalls)
              newCalls.set(event.tool_call!.id, event.tool_call!)
              return { ...s, currentToolCalls: newCalls }
            })
            onToolCallComplete?.(event.tool_call)
          }
          break

        case 'end':
          setState((s) => ({
            ...s,
            isStreaming: false,
          }))
          onEnd?.(event.finish_reason || 'stop')
          break

        case 'error':
          setState((s) => ({
            ...s,
            isStreaming: false,
            error: event.error || 'Unknown error',
          }))
          onError?.(event.error || 'Unknown error')
          break
      }
    },
    [onThinkingDelta, onTextDelta, onToolCallStart, onToolCallComplete, onEnd, onError]
  )

  // Subscribe to stream events
  useEffect(() => {
    if (!sessionId) return

    const eventName = `stream:${sessionId}`

    // Clean up previous listener
    if (unlistenRef.current) {
      unlistenRef.current()
    }

    // Set up new listener
    listen<StreamEvent>(eventName, (event) => {
      handleEvent(event.payload)
    }).then((unlisten) => {
      unlistenRef.current = unlisten
    })

    return () => {
      if (unlistenRef.current) {
        unlistenRef.current()
        unlistenRef.current = null
      }
    }
  }, [sessionId, handleEvent])

  // Reset state
  const reset = useCallback(() => {
    setState({
      isStreaming: false,
      currentThinking: '',
      currentText: '',
      currentToolCalls: new Map(),
      error: null,
    })
  }, [])

  return {
    ...state,
    reset,
  }
}

// Hook for loop events
export interface LoopEvent {
  type:
    | 'state_changed'
    | 'text_delta'
    | 'message_added'
    | 'tool_approval_needed'
    | 'tool_execution_started'
    | 'tool_execution_completed'
    | 'loop_completed'
    | 'loop_error'
  session_id: string
  state?: string
  delta?: string
  message?: Message
  tool_calls?: ToolCallInfo[]
  tool_call_id?: string
  tool_name?: string
  result?: string
  success?: boolean
  error?: string
}

export interface Message {
  id: string
  role: 'user' | 'assistant'
  content: string
  tool_calls: ToolCallInfo[]
  timestamp: string
}

export interface LoopState {
  isActive: boolean
  state: string
  pendingApprovals: ToolCallInfo[]
  error: string | null
}

export interface UseLoopOptions {
  sessionId: string | null
  onStateChanged?: (state: string) => void
  onMessageAdded?: (message: Message) => void
  onToolApprovalNeeded?: (toolCalls: ToolCallInfo[]) => void
  onToolExecutionCompleted?: (toolCallId: string, result: string, success: boolean) => void
  onLoopCompleted?: () => void
  onLoopError?: (error: string) => void
}

export function useLoop({
  sessionId,
  onStateChanged,
  onMessageAdded,
  onToolApprovalNeeded,
  onToolExecutionCompleted,
  onLoopCompleted,
  onLoopError,
}: UseLoopOptions) {
  const [state, setState] = useState<LoopState>({
    isActive: false,
    state: 'idle',
    pendingApprovals: [],
    error: null,
  })

  const unlistenRef = useRef<UnlistenFn | null>(null)

  // Handle loop events
  const handleEvent = useCallback(
    (event: LoopEvent) => {
      switch (event.type) {
        case 'state_changed':
          setState((s) => ({
            ...s,
            isActive: event.state !== 'idle' && event.state !== 'completed' && event.state !== 'cancelled',
            state: event.state || s.state,
          }))
          onStateChanged?.(event.state || '')
          break

        case 'message_added':
          if (event.message) {
            onMessageAdded?.(event.message)
          }
          break

        case 'tool_approval_needed':
          if (event.tool_calls) {
            setState((s) => ({
              ...s,
              pendingApprovals: event.tool_calls || [],
            }))
            onToolApprovalNeeded?.(event.tool_calls)
          }
          break

        case 'tool_execution_completed':
          if (event.tool_call_id) {
            setState((s) => ({
              ...s,
              pendingApprovals: s.pendingApprovals.filter((tc) => tc.id !== event.tool_call_id),
            }))
            onToolExecutionCompleted?.(event.tool_call_id, event.result || '', event.success || false)
          }
          break

        case 'loop_completed':
          setState((s) => ({
            ...s,
            isActive: false,
            state: 'completed',
            pendingApprovals: [],
          }))
          onLoopCompleted?.()
          break

        case 'loop_error':
          setState((s) => ({
            ...s,
            isActive: false,
            state: 'error',
            error: event.error || 'Unknown error',
          }))
          onLoopError?.(event.error || 'Unknown error')
          break
      }
    },
    [onStateChanged, onMessageAdded, onToolApprovalNeeded, onToolExecutionCompleted, onLoopCompleted, onLoopError]
  )

  // Subscribe to loop events
  useEffect(() => {
    if (!sessionId) return

    const eventName = `loop:${sessionId}`

    // Clean up previous listener
    if (unlistenRef.current) {
      unlistenRef.current()
    }

    // Set up new listener
    listen<LoopEvent>(eventName, (event) => {
      handleEvent(event.payload)
    }).then((unlisten) => {
      unlistenRef.current = unlisten
    })

    return () => {
      if (unlistenRef.current) {
        unlistenRef.current()
        unlistenRef.current = null
      }
    }
  }, [sessionId, handleEvent])

  // Reset state
  const reset = useCallback(() => {
    setState({
      isActive: false,
      state: 'idle',
      pendingApprovals: [],
      error: null,
    })
  }, [])

  return {
    ...state,
    reset,
  }
}
