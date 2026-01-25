import { useState, useRef, useEffect } from 'react'
import { Send, Loader2, X, AlertCircle, Sparkles } from 'lucide-react'
import { Button } from '../components/ui/button'
import SessionTabs from '../components/SessionTabs'
import ApprovalModal from '../components/ApprovalModal'
import QuestionModal from '../components/QuestionModal'
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

  const handleApprove = async (toolId: string) => {
    try {
      await approveTool(toolId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleReject = async (toolId: string) => {
    try {
      await rejectTool(toolId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleApproveForSession = async (toolId: string, toolName: string) => {
    try {
      await approveToolForSession(toolId, toolName)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleApproveAll = async (toolId: string) => {
    try {
      await approveAllForSession(toolId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleAnswer = async (requestId: string, answers: Record<string, string>) => {
    try {
      await answerQuestion(requestId, answers)
    } catch (err) {
      setError(String(err))
    }
  }

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
                  <pre className="whitespace-pre-wrap font-sans text-sm">{msg.content}</pre>
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
            </div>
          ) : (
            <span>Ready</span>
          )}
          {session?.provider && (
            <span className="ml-auto">{session.provider.type}</span>
          )}
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
