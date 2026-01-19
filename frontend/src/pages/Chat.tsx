import { useState, useRef, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Send, Loader2, Check, X, Terminal, AlertCircle, Brain, ChevronDown, ChevronRight } from 'lucide-react'
import ContextIndicator from '../components/ContextIndicator'

interface ToolCall {
  id: string
  name: string
  arguments: Record<string, unknown>
  status: 'Pending' | 'Approved' | 'Rejected' | 'Executing' | 'Completed' | 'Failed'
  result?: string
}

interface Message {
  id: string
  role: 'user' | 'assistant'
  content: string
  tool_calls: ToolCall[]
  timestamp: string
}

interface SessionInfo {
  id: string
  message_count: number
  created_at: string
}

export default function Chat() {
  const [input, setInput] = useState('')
  const [messages, setMessages] = useState<Message[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null)
  const [isLoopActive, setIsLoopActive] = useState(false)
  const [streamingText, setStreamingText] = useState('')
  const [streamingThinking, setStreamingThinking] = useState('')
  const [showThinking, setShowThinking] = useState(true)
  const messagesEndRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages, streamingText, streamingThinking])

  // Check API key and create session on mount
  useEffect(() => {
    const init = async () => {
      try {
        const hasKey = await invoke<boolean>('check_api_key')
        setHasApiKey(hasKey)

        if (hasKey) {
          const session = await invoke<SessionInfo>('create_session')
          setSessionId(session.id)
        }
      } catch (err) {
        console.error('Init error:', err)
        setError(String(err))
      }
    }
    init()
  }, [])

  // Listen for stream events
  useEffect(() => {
    if (!sessionId) return

    const unlistenStream = listen<{ type: string; delta?: string; accumulated?: string }>(
      `stream:${sessionId}`,
      (event) => {
        if (event.payload.type === 'start') {
          setStreamingText('')
          setStreamingThinking('')
        } else if (event.payload.type === 'thinking_delta') {
          setStreamingThinking(event.payload.accumulated || '')
        } else if (event.payload.type === 'text_delta') {
          setStreamingText(event.payload.accumulated || '')
        } else if (event.payload.type === 'end') {
          setStreamingText('')
          setStreamingThinking('')
        }
      }
    )

    const unlistenLoop = listen<{ type: string; state?: string }>(
      `loop:${sessionId}`,
      (event) => {
        if (event.payload.type === 'state_changed') {
          const state = event.payload.state || ''
          setIsLoopActive(!['idle', 'completed', 'cancelled', 'error'].includes(state))
        }
        if (event.payload.type === 'loop_completed' || event.payload.type === 'loop_error') {
          setIsLoopActive(false)
          refreshMessages()
        }
      }
    )

    return () => {
      unlistenStream.then((fn) => fn())
      unlistenLoop.then((fn) => fn())
    }
  }, [sessionId])

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+Enter or Cmd+Enter to send
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault()
        if (input.trim() && !isLoading && sessionId) {
          handleSubmit(new Event('submit') as unknown as React.FormEvent)
        }
      }

      // Escape to cancel loop
      if (e.key === 'Escape' && isLoopActive && sessionId) {
        e.preventDefault()
        handleStopLoop()
      }

      // Y to approve all pending tools
      if ((e.key === 'y' || e.key === 'Y') && hasPendingTools()) {
        e.preventDefault()
        handleApproveAll()
      }

      // N to reject all pending tools
      if ((e.key === 'n' || e.key === 'N') && hasPendingTools()) {
        e.preventDefault()
        handleRejectAll()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [input, isLoading, sessionId, isLoopActive, messages])

  const refreshMessages = useCallback(async () => {
    if (!sessionId) return
    try {
      const allMessages = await invoke<Message[]>('get_session_messages', { sessionId })
      setMessages(allMessages)
    } catch (err) {
      console.error('Refresh error:', err)
    }
  }, [sessionId])

  const hasPendingTools = useCallback(() => {
    return messages.some((m) => m.tool_calls?.some((tc) => tc.status === 'Pending'))
  }, [messages])

  const handleStopLoop = async () => {
    if (!sessionId) return
    try {
      await invoke('stop_loop', { sessionId })
      setIsLoopActive(false)
    } catch (err) {
      console.error('Stop loop error:', err)
    }
  }

  const handleApproveAll = async () => {
    if (!sessionId) return
    const pendingCalls = messages
      .flatMap((m) => m.tool_calls || [])
      .filter((tc) => tc.status === 'Pending')

    for (const tc of pendingCalls) {
      await handleApprove(tc.id)
    }
  }

  const handleRejectAll = async () => {
    if (!sessionId) return
    const pendingCalls = messages
      .flatMap((m) => m.tool_calls || [])
      .filter((tc) => tc.status === 'Pending')

    for (const tc of pendingCalls) {
      await handleReject(tc.id)
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!input.trim() || isLoading || !sessionId) return

    setError(null)
    setIsLoading(true)

    try {
      await invoke<Message>('send_message', {
        sessionId,
        content: input,
      })

      // Fetch updated messages
      const allMessages = await invoke<Message[]>('get_session_messages', {
        sessionId,
      })
      setMessages(allMessages)
      setInput('')
    } catch (err) {
      console.error('Send error:', err)
      setError(String(err))
    } finally {
      setIsLoading(false)
    }
  }

  const handleApprove = async (toolCallId: string) => {
    if (!sessionId) return

    try {
      await invoke('approve_tool_call', { sessionId, toolCallId })

      // Execute the tool
      await invoke<Message | null>('execute_tool', {
        sessionId,
        toolCallId,
      })

      // Refresh messages
      const allMessages = await invoke<Message[]>('get_session_messages', {
        sessionId,
      })
      setMessages(allMessages)
    } catch (err) {
      console.error('Approve error:', err)
      setError(String(err))
    }
  }

  const handleReject = async (toolCallId: string) => {
    if (!sessionId) return

    try {
      await invoke('reject_tool_call', { sessionId, toolCallId })

      // Refresh messages
      const allMessages = await invoke<Message[]>('get_session_messages', {
        sessionId,
      })
      setMessages(allMessages)
    } catch (err) {
      console.error('Reject error:', err)
      setError(String(err))
    }
  }

  const formatToolArgs = (args: Record<string, unknown>) => {
    return Object.entries(args)
      .map(([key, value]) => `${key}: ${JSON.stringify(value)}`)
      .join(', ')
  }

  // Parse thinking content from message
  const parseThinking = (content: string): { thinking: string | null; text: string } => {
    const thinkingMatch = content.match(/<thinking>\n?([\s\S]*?)\n?<\/thinking>\n*/);
    if (thinkingMatch) {
      return {
        thinking: thinkingMatch[1].trim(),
        text: content.replace(thinkingMatch[0], '').trim()
      };
    }
    return { thinking: null, text: content };
  };

  // State for collapsed thinking in messages
  const [collapsedThinking, setCollapsedThinking] = useState<Set<string>>(new Set());

  const toggleMessageThinking = (messageId: string) => {
    setCollapsedThinking(prev => {
      const next = new Set(prev);
      if (next.has(messageId)) {
        next.delete(messageId);
      } else {
        next.add(messageId);
      }
      return next;
    });
  };

  if (hasApiKey === false) {
    return (
      <div className="flex flex-col h-full items-center justify-center p-8">
        <AlertCircle className="w-16 h-16 text-yellow-500 mb-4" />
        <h2 className="text-xl font-semibold mb-2">API Key Required</h2>
        <p className="text-gray-600 dark:text-gray-400 text-center mb-4">
          Please configure your API key in Settings to start chatting.
        </p>
        <p className="text-sm text-gray-500 dark:text-gray-500">
          Set ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable,
          or configure it in the Settings page.
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header className="h-14 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between px-4">
        <div className="flex items-center">
          <h1 className="text-lg font-semibold text-gray-900 dark:text-white">
            Cowork Assistant
          </h1>
          {sessionId && (
            <span className="ml-2 text-xs text-gray-500 dark:text-gray-400">
              Session: {sessionId.slice(0, 8)}...
            </span>
          )}
        </div>

        {/* Context Indicator */}
        <ContextIndicator
          sessionId={sessionId}
          onCompact={refreshMessages}
          onClear={refreshMessages}
        />
      </header>

      {/* Error Banner */}
      {error && (
        <div className="bg-red-100 dark:bg-red-900/50 border-b border-red-200 dark:border-red-800 px-4 py-2 flex items-center justify-between">
          <span className="text-red-800 dark:text-red-200 text-sm">{error}</span>
          <button onClick={() => setError(null)} className="text-red-600 hover:text-red-800">
            <X className="w-4 h-4" />
          </button>
        </div>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {messages.length === 0 && (
          <div className="text-center text-gray-500 mt-8">
            <p className="text-lg">Welcome to Cowork!</p>
            <p className="text-sm mt-2">
              Ask me to help with files, run commands, or automate tasks.
            </p>
          </div>
        )}

        {messages.map((message) => {
          const { thinking, text } = message.role === 'assistant'
            ? parseThinking(message.content)
            : { thinking: null, text: message.content };
          const isThinkingCollapsed = collapsedThinking.has(message.id);

          return (
          <div key={message.id} className="space-y-2">
            {/* Thinking section for assistant messages */}
            {message.role === 'assistant' && thinking && (
              <div className="border border-purple-200 dark:border-purple-800 rounded-lg overflow-hidden">
                <button
                  onClick={() => toggleMessageThinking(message.id)}
                  className="w-full bg-purple-50 dark:bg-purple-900/30 px-3 py-2 flex items-center gap-2 text-left hover:bg-purple-100 dark:hover:bg-purple-900/50 transition-colors"
                >
                  {isThinkingCollapsed ? (
                    <ChevronRight className="w-4 h-4 text-purple-500" />
                  ) : (
                    <ChevronDown className="w-4 h-4 text-purple-500" />
                  )}
                  <Brain className="w-4 h-4 text-purple-500" />
                  <span className="text-sm font-medium text-purple-700 dark:text-purple-300">
                    Thinking
                  </span>
                  <span className="text-xs text-purple-500 dark:text-purple-400 ml-auto">
                    {thinking.length} chars
                  </span>
                </button>
                {!isThinkingCollapsed && (
                  <div className="px-3 py-2 bg-purple-50/50 dark:bg-purple-900/20 max-h-48 overflow-y-auto">
                    <pre className="whitespace-pre-wrap font-mono text-xs text-purple-600 dark:text-purple-400">
                      {thinking}
                    </pre>
                  </div>
                )}
              </div>
            )}

            {/* Message bubble */}
            <div className={`flex ${message.role === 'user' ? 'justify-end' : 'justify-start'}`}>
              <div
                className={`
                  max-w-[80%] rounded-lg px-4 py-2
                  ${message.role === 'user'
                    ? 'bg-primary-600 text-white'
                    : 'bg-gray-200 dark:bg-gray-700 text-gray-900 dark:text-white'
                  }
                `}
              >
                <pre className="whitespace-pre-wrap font-sans text-sm">
                  {text || '(thinking...)'}
                </pre>
              </div>
            </div>

            {/* Tool calls */}
            {message.tool_calls && message.tool_calls.length > 0 && (
              <div className="ml-4 space-y-2">
                {message.tool_calls.map((tc) => (
                  <div
                    key={tc.id}
                    className="border border-gray-300 dark:border-gray-600 rounded-lg overflow-hidden"
                  >
                    {/* Tool header */}
                    <div className="bg-gray-100 dark:bg-gray-800 px-3 py-2 flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Terminal className="w-4 h-4 text-gray-500" />
                        <span className="font-mono text-sm font-medium">{tc.name}</span>
                        <span
                          className={`text-xs px-2 py-0.5 rounded ${
                            tc.status === 'Completed'
                              ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
                              : tc.status === 'Failed' || tc.status === 'Rejected'
                              ? 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
                              : tc.status === 'Pending'
                              ? 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200'
                              : 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200'
                          }`}
                        >
                          {tc.status}
                        </span>
                      </div>

                      {/* Approval buttons */}
                      {tc.status === 'Pending' && (
                        <div className="flex gap-2">
                          <button
                            onClick={() => handleApprove(tc.id)}
                            className="p-1 rounded bg-green-500 text-white hover:bg-green-600 transition-colors"
                            title="Approve"
                          >
                            <Check className="w-4 h-4" />
                          </button>
                          <button
                            onClick={() => handleReject(tc.id)}
                            className="p-1 rounded bg-red-500 text-white hover:bg-red-600 transition-colors"
                            title="Reject"
                          >
                            <X className="w-4 h-4" />
                          </button>
                        </div>
                      )}
                    </div>

                    {/* Tool arguments */}
                    <div className="px-3 py-2 bg-gray-50 dark:bg-gray-900 text-xs font-mono">
                      <div className="text-gray-600 dark:text-gray-400">
                        {formatToolArgs(tc.arguments)}
                      </div>
                    </div>

                    {/* Tool result */}
                    {tc.result && (
                      <div className="px-3 py-2 border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800">
                        <pre className="text-xs font-mono text-gray-700 dark:text-gray-300 whitespace-pre-wrap max-h-40 overflow-y-auto">
                          {tc.result}
                        </pre>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        );
        })}

        {/* Streaming thinking content */}
        {(streamingThinking || streamingText) && (
          <div className="space-y-2">
            {/* Thinking section - collapsible */}
            {streamingThinking && (
              <div className="border border-purple-200 dark:border-purple-800 rounded-lg overflow-hidden">
                <button
                  onClick={() => setShowThinking(!showThinking)}
                  className="w-full bg-purple-50 dark:bg-purple-900/30 px-3 py-2 flex items-center gap-2 text-left hover:bg-purple-100 dark:hover:bg-purple-900/50 transition-colors"
                >
                  {showThinking ? (
                    <ChevronDown className="w-4 h-4 text-purple-500" />
                  ) : (
                    <ChevronRight className="w-4 h-4 text-purple-500" />
                  )}
                  <Brain className="w-4 h-4 text-purple-500" />
                  <span className="text-sm font-medium text-purple-700 dark:text-purple-300">
                    Thinking...
                  </span>
                  <Loader2 className="w-3 h-3 animate-spin text-purple-500 ml-auto" />
                </button>
                {showThinking && (
                  <div className="px-3 py-2 bg-purple-50/50 dark:bg-purple-900/20 max-h-48 overflow-y-auto">
                    <pre className="whitespace-pre-wrap font-mono text-xs text-purple-600 dark:text-purple-400">
                      {streamingThinking}
                    </pre>
                  </div>
                )}
              </div>
            )}

            {/* Streaming text response */}
            {streamingText && (
              <div className="flex justify-start">
                <div className="max-w-[80%] bg-gray-200 dark:bg-gray-700 rounded-lg px-4 py-2 text-gray-900 dark:text-white">
                  <pre className="whitespace-pre-wrap font-sans text-sm">
                    {streamingText}
                  </pre>
                </div>
              </div>
            )}
          </div>
        )}

        {isLoading && !streamingText && !streamingThinking && (
          <div className="flex justify-start">
            <div className="bg-gray-200 dark:bg-gray-700 rounded-lg px-4 py-2">
              <Loader2 className="w-5 h-5 animate-spin text-gray-500" />
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <form onSubmit={handleSubmit} className="p-4 border-t border-gray-200 dark:border-gray-700">
        <div className="flex gap-2">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={sessionId ? "Ask anything..." : "Connecting..."}
            className="
              flex-1 rounded-lg border border-gray-300 dark:border-gray-600
              bg-white dark:bg-gray-800 px-4 py-2
              text-gray-900 dark:text-white
              placeholder-gray-500 dark:placeholder-gray-400
              focus:outline-none focus:ring-2 focus:ring-primary-500
            "
            disabled={isLoading || !sessionId}
          />
          <button
            type="submit"
            disabled={isLoading || !input.trim() || !sessionId}
            className="
              px-4 py-2 rounded-lg bg-primary-600 text-white
              hover:bg-primary-700 disabled:opacity-50
              disabled:cursor-not-allowed transition-colors
            "
          >
            <Send className="w-5 h-5" />
          </button>
        </div>
      </form>
    </div>
  )
}
