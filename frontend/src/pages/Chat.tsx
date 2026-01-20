import { useState, useRef, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Send, Loader2, Check, X, Terminal, AlertCircle, Brain, ChevronDown, ChevronRight, Sparkles } from 'lucide-react'
import ContextIndicator from '../components/ContextIndicator'
import QuestionModal from '../components/QuestionModal'
import ToolResultFormatter from '../components/ToolResultFormatter'
import { Button } from '../components/ui/button'

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

interface QuestionOption {
  label: string
  description: string
}

interface UserQuestion {
  question: string
  header: string
  options: QuestionOption[]
  multi_select: boolean
}

interface PendingQuestion {
  requestId: string
  toolCallId: string
  questions: UserQuestion[]
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
  const [pendingQuestion, setPendingQuestion] = useState<PendingQuestion | null>(null)
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

    const unlistenLoop = listen<{
      type: string
      state?: string
      request_id?: string
      tool_call_id?: string
      tool_name?: string
      questions?: UserQuestion[]
      result?: string
      success?: boolean
      message?: Message
    }>(
      `loop:${sessionId}`,
      (event) => {
        if (event.payload.type === 'state_changed') {
          const state = event.payload.state || ''
          setIsLoopActive(!['idle', 'completed', 'cancelled', 'error'].includes(state))
        }
        // Add new messages directly to state
        if (event.payload.type === 'message_added' && event.payload.message) {
          setMessages((prev) => {
            // Avoid duplicates
            if (prev.some((m) => m.id === event.payload.message!.id)) {
              return prev
            }
            return [...prev, event.payload.message!]
          })
        }
        // Update tool status to Executing
        if (event.payload.type === 'tool_execution_started' && event.payload.tool_call_id) {
          setMessages((prev) =>
            prev.map((msg) => ({
              ...msg,
              tool_calls: msg.tool_calls.map((tc) =>
                tc.id === event.payload.tool_call_id
                  ? { ...tc, status: 'Executing' as const }
                  : tc
              ),
            }))
          )
        }
        // Update tool status and result on completion
        if (event.payload.type === 'tool_execution_completed' && event.payload.tool_call_id) {
          setMessages((prev) =>
            prev.map((msg) => ({
              ...msg,
              tool_calls: msg.tool_calls.map((tc) =>
                tc.id === event.payload.tool_call_id
                  ? {
                      ...tc,
                      status: event.payload.success ? ('Completed' as const) : ('Failed' as const),
                      result: event.payload.result,
                    }
                  : tc
              ),
            }))
          )
        }
        if (event.payload.type === 'loop_completed' || event.payload.type === 'loop_error') {
          setIsLoopActive(false)
          setPendingQuestion(null)
        }
        // Handle question requests from the AI
        if (event.payload.type === 'question_requested' && event.payload.questions) {
          setPendingQuestion({
            requestId: event.payload.request_id || '',
            toolCallId: event.payload.tool_call_id || '',
            questions: event.payload.questions,
          })
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

    const userMessage = input
    setInput('')
    setError(null)
    setIsLoading(true)

    // Add user message to UI immediately for responsiveness
    const userMsg: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      content: userMessage,
      tool_calls: [],
      timestamp: new Date().toISOString(),
    }
    setMessages((prev) => [...prev, userMsg])

    try {
      // Start the agentic loop which handles auto-approval based on config
      // The loop runs in the background and emits events to update the UI
      await invoke('start_loop', {
        sessionId,
        prompt: userMessage,
      })
      // isLoopActive will be set to true by loop events
      // isLoading is cleared since the request to start succeeded
      setIsLoading(false)
    } catch (err) {
      console.error('Send error:', err)
      setError(String(err))
      setIsLoading(false)
      // Refresh to get actual state on error
      await refreshMessages()
    }
  }

  const handleApprove = async (toolCallId: string) => {
    if (!sessionId) return

    try {
      // Update local state to Executing immediately for responsiveness
      setMessages((prev) =>
        prev.map((msg) => ({
          ...msg,
          tool_calls: msg.tool_calls.map((tc) =>
            tc.id === toolCallId ? { ...tc, status: 'Executing' as const } : tc
          ),
        }))
      )

      await invoke('approve_tool_call', { sessionId, toolCallId })

      // Execute the tool
      await invoke<Message | null>('execute_tool', {
        sessionId,
        toolCallId,
      })

      // Refresh messages to get the complete state
      // (Manual approval doesn't have lock contention like the agentic loop)
      await refreshMessages()
    } catch (err) {
      console.error('Approve error:', err)
      setError(String(err))
      await refreshMessages()
    }
  }

  const handleReject = async (toolCallId: string) => {
    if (!sessionId) return

    try {
      // Update local state to Rejected immediately
      setMessages((prev) =>
        prev.map((msg) => ({
          ...msg,
          tool_calls: msg.tool_calls.map((tc) =>
            tc.id === toolCallId ? { ...tc, status: 'Rejected' as const } : tc
          ),
        }))
      )

      await invoke('reject_tool_call', { sessionId, toolCallId })
    } catch (err) {
      console.error('Reject error:', err)
      setError(String(err))
    }
  }

  const handleQuestionAnswered = useCallback(() => {
    setPendingQuestion(null)
  }, [])

  const handleQuestionCancelled = useCallback(() => {
    setPendingQuestion(null)
    setIsLoopActive(false)
  }, [])

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
      <div className="flex flex-col h-full items-center justify-center p-8 bg-background">
        <div className="text-center max-w-md">
          <div className="w-16 h-16 rounded-2xl bg-warning/10 flex items-center justify-center mx-auto mb-4">
            <AlertCircle className="w-8 h-8 text-warning" />
          </div>
          <h2 className="text-xl font-semibold mb-2 text-foreground">API Key Required</h2>
          <p className="text-muted-foreground text-center mb-4">
            Please configure your API key in Settings to start chatting.
          </p>
          <p className="text-sm text-muted-foreground/70">
            Set ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable,
            or configure it in the Settings page.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <header className="h-14 border-b border-border flex items-center justify-between px-4 bg-card/50">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-violet-500 to-purple-600 flex items-center justify-center shadow-glow-sm">
            <Sparkles className="w-4 h-4 text-white" />
          </div>
          <div>
            <h1 className="text-sm font-semibold text-foreground">
              Cowork Assistant
            </h1>
            {sessionId && (
              <span className="text-xs text-muted-foreground">
                {sessionId.slice(0, 8)}...
              </span>
            )}
          </div>
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
        <div className="bg-error/10 border-b border-error/20 px-4 py-2 flex items-center justify-between">
          <span className="text-error text-sm">{error}</span>
          <button onClick={() => setError(null)} className="text-error/70 hover:text-error transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {messages.length === 0 && (
          <div className="text-center mt-16">
            <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-violet-500/20 to-purple-600/20 flex items-center justify-center mx-auto mb-4 border border-primary/20">
              <Sparkles className="w-8 h-8 text-primary" />
            </div>
            <p className="text-lg font-medium text-foreground">Welcome to Cowork!</p>
            <p className="text-sm mt-2 text-muted-foreground">
              Ask me to help with files, run commands, or automate tasks.
            </p>
          </div>
        )}

        {messages
          // Filter out tool result messages (they're shown in the tool call cards)
          .filter((m) => !(m.role === 'user' && m.content.startsWith('[Tool result for ')))
          .map((message) => {
          const { thinking, text } = message.role === 'assistant'
            ? parseThinking(message.content)
            : { thinking: null, text: message.content };
          const isThinkingCollapsed = collapsedThinking.has(message.id);

          return (
          <div key={message.id} className="space-y-2 animate-in">
            {/* Thinking section for assistant messages */}
            {message.role === 'assistant' && thinking && (
              <div className="border border-primary/20 rounded-xl overflow-hidden bg-primary/5">
                <button
                  onClick={() => toggleMessageThinking(message.id)}
                  className="w-full px-3 py-2 flex items-center gap-2 text-left hover:bg-primary/10 transition-colors"
                >
                  {isThinkingCollapsed ? (
                    <ChevronRight className="w-4 h-4 text-primary" />
                  ) : (
                    <ChevronDown className="w-4 h-4 text-primary" />
                  )}
                  <Brain className="w-4 h-4 text-primary" />
                  <span className="text-sm font-medium text-primary">
                    Thinking
                  </span>
                  <span className="text-xs text-primary/60 ml-auto">
                    {thinking.length} chars
                  </span>
                </button>
                {!isThinkingCollapsed && (
                  <div className="px-3 py-2 bg-primary/5 max-h-48 overflow-y-auto border-t border-primary/10">
                    <pre className="whitespace-pre-wrap font-mono text-xs text-primary/80">
                      {thinking}
                    </pre>
                  </div>
                )}
              </div>
            )}

            {/* Message bubble - only show if there's text content or it's a user message */}
            {(text || message.role === 'user') && (
              <div className={`flex ${message.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                <div
                  className={`
                    max-w-[80%] rounded-xl px-4 py-3
                    ${message.role === 'user'
                      ? 'bg-gradient-to-r from-violet-500 to-purple-600 text-white shadow-glow-sm'
                      : 'bg-card border border-border text-foreground'
                    }
                  `}
                >
                  <pre className="whitespace-pre-wrap font-sans text-sm leading-relaxed">
                    {text}
                  </pre>
                </div>
              </div>
            )}

            {/* Tool calls */}
            {message.tool_calls && message.tool_calls.length > 0 && (
              <div className="ml-4 space-y-2">
                {message.tool_calls.map((tc) => (
                  <div
                    key={tc.id}
                    className="border border-border rounded-xl overflow-hidden bg-card"
                  >
                    {/* Tool header */}
                    <div className="bg-secondary/50 px-3 py-2 flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <Terminal className="w-4 h-4 text-muted-foreground" />
                        <span className="font-mono text-sm font-medium text-foreground">{tc.name}</span>
                        <span
                          className={`text-xs px-2 py-0.5 rounded-full font-medium ${
                            tc.status === 'Completed'
                              ? 'bg-success/20 text-success'
                              : tc.status === 'Failed' || tc.status === 'Rejected'
                              ? 'bg-error/20 text-error'
                              : tc.status === 'Pending'
                              ? 'bg-warning/20 text-warning'
                              : 'bg-info/20 text-info'
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
                            className="p-1.5 rounded-lg bg-success text-white hover:bg-success/80 transition-colors"
                            title="Approve"
                          >
                            <Check className="w-4 h-4" />
                          </button>
                          <button
                            onClick={() => handleReject(tc.id)}
                            className="p-1.5 rounded-lg bg-error text-white hover:bg-error/80 transition-colors"
                            title="Reject"
                          >
                            <X className="w-4 h-4" />
                          </button>
                        </div>
                      )}
                    </div>

                    {/* Tool arguments */}
                    <div className="px-3 py-2 bg-background text-xs font-mono">
                      <div className="text-muted-foreground">
                        {formatToolArgs(tc.arguments)}
                      </div>
                    </div>

                    {/* Tool result */}
                    {tc.result && (
                      <div className="px-3 py-2 border-t border-border bg-card">
                        <ToolResultFormatter toolName={tc.name} result={tc.result} />
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
          <div className="space-y-2 animate-in">
            {/* Thinking section - collapsible */}
            {streamingThinking && (
              <div className="border border-primary/20 rounded-xl overflow-hidden bg-primary/5">
                <button
                  onClick={() => setShowThinking(!showThinking)}
                  className="w-full px-3 py-2 flex items-center gap-2 text-left hover:bg-primary/10 transition-colors"
                >
                  {showThinking ? (
                    <ChevronDown className="w-4 h-4 text-primary" />
                  ) : (
                    <ChevronRight className="w-4 h-4 text-primary" />
                  )}
                  <Brain className="w-4 h-4 text-primary thinking-pulse" />
                  <span className="text-sm font-medium text-primary">
                    Thinking...
                  </span>
                  <Loader2 className="w-3 h-3 animate-spin text-primary ml-auto" />
                </button>
                {showThinking && (
                  <div className="px-3 py-2 bg-primary/5 max-h-48 overflow-y-auto border-t border-primary/10">
                    <pre className="whitespace-pre-wrap font-mono text-xs text-primary/80">
                      {streamingThinking}
                    </pre>
                  </div>
                )}
              </div>
            )}

            {/* Streaming text response */}
            {streamingText && (
              <div className="flex justify-start">
                <div className="max-w-[80%] bg-card border border-border rounded-xl px-4 py-3 text-foreground">
                  <pre className="whitespace-pre-wrap font-sans text-sm leading-relaxed">
                    {streamingText}
                  </pre>
                </div>
              </div>
            )}
          </div>
        )}

        {isLoading && !streamingText && !streamingThinking && (
          <div className="flex justify-start animate-in">
            <div className="bg-card border border-border rounded-xl px-4 py-3">
              <Loader2 className="w-5 h-5 animate-spin text-primary" />
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <form onSubmit={handleSubmit} className="p-4 border-t border-border bg-card/50">
        <div className="flex gap-3">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={sessionId ? "Ask anything..." : "Connecting..."}
            className="
              flex-1 rounded-xl border border-border bg-background px-4 py-3
              text-foreground placeholder-muted-foreground
              focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary/50
              transition-all duration-200 hover:border-border-hover
            "
            disabled={isLoading || isLoopActive || !sessionId}
          />
          <Button
            type="submit"
            disabled={isLoading || isLoopActive || !input.trim() || !sessionId}
            variant="gradient"
            size="lg"
            className="px-6"
          >
            <Send className="w-5 h-5" />
          </Button>
        </div>
        <p className="text-xs text-muted-foreground mt-2 text-center">
          Press <kbd className="px-1.5 py-0.5 rounded bg-secondary text-foreground text-xs">Ctrl+Enter</kbd> to send
        </p>
      </form>

      {/* Question Modal */}
      {pendingQuestion && sessionId && (
        <QuestionModal
          sessionId={sessionId}
          requestId={pendingQuestion.requestId}
          questions={pendingQuestion.questions}
          onAnswer={handleQuestionAnswered}
          onCancel={handleQuestionCancelled}
        />
      )}
    </div>
  )
}
