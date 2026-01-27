import { useState, useRef, useEffect, useCallback } from 'react'
import { Send, Loader2, X, AlertCircle, Sparkles, Square } from 'lucide-react'
import { Button } from '../components/ui/button'
import SessionTabs from '../components/SessionTabs'
import ApprovalModal from '../components/ApprovalModal'
import QuestionModal from '../components/QuestionModal'
import ToolCallMessage from '../components/ToolCallMessage'
import ToolResultMessage from '../components/ToolResultMessage'
import { useSession } from '../context/SessionContext'

export default function Chat() {
  const {
    sessions,
    activeSessionId,
    isInitialized,
    hasApiKey,
    setActiveSession,
    createNewSession,
    closeSession,
    sendMessage,
    approveTool,
    rejectTool,
    approveToolForSession,
    approveAllForSession,
    answerQuestion,
    cancelSession,
    getActiveSession,
  } = useSession()

  const [input, setInput] = useState('')
  const [error, setError] = useState<string | null>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)

  const session = getActiveSession()
  const messages = session?.messages || []
  const ephemeral = session?.ephemeral
  const status = session?.status || ''
  const modal = session?.modal || null
  const isReady = session?.isReady ?? false

  // Scroll to bottom on new messages or ephemeral changes
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages, ephemeral])

  // Sync session error to local error state
  useEffect(() => {
    if (session?.error) {
      setError(session.error)
    }
  }, [session?.error])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!input.trim() || modal) return

    const userMessage = input
    setInput('')
    setError(null)

    try {
      await sendMessage(userMessage)
    } catch (err) {
      console.error('Send error:', err)
      setError(String(err))
    }
  }

  // Get targetSessionId from modal for subagent routing
  const targetSessionId = modal?.targetSessionId

  const handleApprove = async (toolId: string) => {
    try {
      await approveTool(toolId, targetSessionId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleReject = async (toolId: string) => {
    try {
      await rejectTool(toolId, targetSessionId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleApproveForSession = async (toolId: string, toolName: string) => {
    try {
      await approveToolForSession(toolId, toolName, targetSessionId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleApproveAll = async (toolId: string) => {
    try {
      await approveAllForSession(toolId, targetSessionId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleAnswer = async (requestId: string, answers: Record<string, string>) => {
    try {
      await answerQuestion(requestId, answers, targetSessionId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleCancel = useCallback(async () => {
    try {
      await cancelSession()
    } catch (err) {
      console.error('Cancel error:', err)
    }
  }, [cancelSession])

  // ESC key handler to cancel processing
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && status) {
        handleCancel()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [status, handleCancel])

  // Loading state
  if (!isInitialized) {
    return (
      <div className="flex flex-col h-full items-center justify-center p-8 bg-background">
        <Loader2 className="w-8 h-8 animate-spin text-primary mb-4" />
        <p className="text-muted-foreground">Initializing...</p>
      </div>
    )
  }

  // No API key
  if (hasApiKey === false) {
    return (
      <div className="flex flex-col h-full items-center justify-center p-8 bg-background">
        <div className="text-center max-w-md">
          <AlertCircle className="w-16 h-16 text-warning mx-auto mb-4" />
          <h2 className="text-xl font-semibold mb-2">API Key Required</h2>
          <p className="text-muted-foreground">
            Please configure your API key in Settings.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Session Tabs */}
      <SessionTabs
        sessions={sessions}
        activeId={activeSessionId}
        onSelect={setActiveSession}
        onNew={() => createNewSession()}
        onClose={closeSession}
      />

      {/* Error Banner */}
      {error && (
        <div className="bg-error/10 border-b border-error/20 px-4 py-2 flex items-center justify-between">
          <span className="text-error text-sm">{error}</span>
          <button onClick={() => setError(null)}>
            <X className="w-4 h-4 text-error" />
          </button>
        </div>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {messages.length === 0 && !ephemeral && (
          <div className="text-center mt-16">
            <Sparkles className="w-12 h-12 text-primary mx-auto mb-4 opacity-50" />
            <p className="text-lg font-medium">Welcome to Cowork!</p>
            <p className="text-sm text-muted-foreground mt-2">
              Ask me anything to get started.
            </p>
          </div>
        )}

        {messages.map((msg) => (
          <div key={msg.id}>
            {msg.type === 'user' && (
              <div className="flex justify-end">
                <div className="max-w-[80%] rounded-xl px-4 py-3 bg-primary text-primary-foreground">
                  <pre className="whitespace-pre-wrap font-sans text-sm">{msg.content}</pre>
                </div>
              </div>
            )}

            {msg.type === 'assistant' && (
              <div className="flex justify-start">
                <div className="max-w-[80%] rounded-xl px-4 py-3 bg-card border border-border">
                  {/* Add ● prefix for assistant messages */}
                  <div className="flex items-start gap-2">
                    <span className="text-foreground font-medium select-none">●</span>
                    <pre className="whitespace-pre-wrap font-sans text-sm flex-1">{msg.content}</pre>
                  </div>
                </div>
              </div>
            )}

            {msg.type === 'tool_call' && msg.formatted && (
              <div className="flex justify-start">
                <div className="max-w-[80%]">
                  <ToolCallMessage formatted={msg.formatted} elapsedSecs={msg.elapsedSecs} />
                </div>
              </div>
            )}

            {msg.type === 'tool_result' && msg.summary && (
              <div className="flex justify-start">
                <div className="max-w-[80%]">
                  <ToolResultMessage
                    summary={msg.summary}
                    diffPreview={msg.diffPreview}
                    output={msg.content}
                    success={msg.success ?? true}
                    elapsedSecs={msg.elapsedSecs}
                  />
                </div>
              </div>
            )}
          </div>
        ))}

        {/* Ephemeral tool activity (up to 3 lines) */}
        {ephemeral && (
          <div className="text-sm text-muted-foreground/70 font-mono pl-2 space-y-0.5">
            {ephemeral.split('\n').slice(0, 3).map((line, i) => (
              <div key={i} className="truncate">{line}</div>
            ))}
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Status Bar */}
      {(status || isReady) && (
        <div className="h-8 border-t border-border flex items-center px-4 text-xs text-muted-foreground bg-card/30">
          {status ? (
            <div className="flex items-center gap-2">
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
              <span>{status}</span>
              <button
                onClick={handleCancel}
                className="ml-2 p-1 rounded hover:bg-muted/50 text-muted-foreground hover:text-foreground transition-colors"
                title="Cancel (ESC)"
              >
                <Square className="w-3 h-3 fill-current" />
              </button>
            </div>
          ) : (
            <span>Ready</span>
          )}
          <div className="ml-auto flex items-center gap-3">
            {/* Context Usage: input/output/total (percentage%) */}
            {session?.contextUsage && (
              <span className="font-mono" title="Input / Output / Total context tokens">
                {Math.round(session.contextUsage.breakdown.input_tokens / 1000)}k/
                {Math.round(session.contextUsage.breakdown.output_tokens / 1000)}k/
                {Math.round(session.contextUsage.limit_tokens / 1000)}k
                ({Math.round(session.contextUsage.used_percentage * 100)}%)
              </span>
            )}
            {session?.provider && (
              <span>{session.provider.type}</span>
            )}
          </div>
        </div>
      )}

      {/* Input */}
      <form onSubmit={handleSubmit} className="p-4 border-t border-border bg-card/50">
        <div className="flex gap-3">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={modal ? "Waiting for response..." : "Type a message..."}
            disabled={!!modal}
            className="flex-1 rounded-xl border border-border bg-background px-4 py-3 text-foreground placeholder-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 disabled:opacity-50 disabled:cursor-not-allowed"
          />
          <Button
            type="submit"
            disabled={!input.trim() || !!modal}
            variant="gradient"
            size="lg"
          >
            <Send className="w-5 h-5" />
          </Button>
        </div>
      </form>

      {/* Modal Overlay */}
      {modal?.type === 'approval' && (
        <ApprovalModal
          id={modal.id}
          name={modal.name}
          arguments={modal.arguments}
          onApprove={handleApprove}
          onReject={handleReject}
          onApproveForSession={handleApproveForSession}
          onApproveAll={handleApproveAll}
        />
      )}

      {modal?.type === 'question' && (
        <QuestionModal
          requestId={modal.request_id}
          questions={modal.questions}
          onAnswer={handleAnswer}
        />
      )}
    </div>
  )
}
