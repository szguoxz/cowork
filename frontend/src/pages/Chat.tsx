import { useState, useRef, useEffect } from 'react'
import { Send, Loader2, Check, X, Terminal, AlertCircle, Sparkles } from 'lucide-react'
import { Button } from '../components/ui/button'
import SessionTabs from '../components/SessionTabs'
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
    getActiveSession,
  } = useSession()

  const [input, setInput] = useState('')
  const [error, setError] = useState<string | null>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)

  const session = getActiveSession()
  const messages = session?.messages || []
  const isIdle = session?.isIdle ?? false
  const isReady = session?.isReady ?? false
  const isThinking = session?.isThinking ?? false
  const thinkingContent = session?.thinkingContent

  // Scroll to bottom on new messages or thinking content
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages, thinkingContent])

  // Sync session error to local error state
  useEffect(() => {
    if (session?.error) {
      setError(session.error)
    }
  }, [session?.error])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!input.trim()) return

    const userMessage = input
    setInput('')
    setError(null)

    try {
      // Message will be queued if not idle, processed when ready
      await sendMessage(userMessage)
    } catch (err) {
      console.error('Send error:', err)
      setError(String(err))
    }
  }

  const handleApproveTool = async (toolId: string) => {
    try {
      await approveTool(toolId)
    } catch (err) {
      setError(String(err))
    }
  }

  const handleRejectTool = async (toolId: string) => {
    try {
      await rejectTool(toolId)
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
      {/* Header */}
      <header className="h-14 border-b border-border flex items-center px-4 bg-card/50">
        <Sparkles className="w-5 h-5 text-primary mr-2" />
        <h1 className="font-semibold">Cowork</h1>
        <span className="ml-auto text-xs text-muted-foreground">
          {isReady ? (isIdle ? 'Ready' : 'Working...') : 'Starting...'}
        </span>
      </header>

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
        {messages.length === 0 && (
          <div className="text-center mt-16">
            <Sparkles className="w-12 h-12 text-primary mx-auto mb-4 opacity-50" />
            <p className="text-lg font-medium">Welcome to Cowork!</p>
            <p className="text-sm text-muted-foreground mt-2">
              Ask me anything to get started.
            </p>
          </div>
        )}

        {messages.map((msg) => (
          <div key={msg.id} className="space-y-2">
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

            {msg.type === 'tool' && msg.tool && (
              <div className="border border-border rounded-xl overflow-hidden bg-card ml-4">
                <div className="bg-secondary/50 px-3 py-2 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Terminal className="w-4 h-4" />
                    <span className="font-mono text-sm">{msg.tool.name}</span>
                    <span className={`text-xs px-2 py-0.5 rounded-full ${
                      msg.tool.status === 'done' ? 'bg-success/20 text-success' :
                      msg.tool.status === 'failed' ? 'bg-error/20 text-error' :
                      msg.tool.status === 'pending' ? 'bg-warning/20 text-warning' :
                      'bg-info/20 text-info'
                    }`}>
                      {msg.tool.status}
                    </span>
                  </div>

                  {msg.tool.status === 'pending' && (
                    <div className="flex gap-2">
                      <button
                        onClick={() => handleApproveTool(msg.tool!.id)}
                        className="p-1 rounded bg-success text-white hover:bg-success/80"
                      >
                        <Check className="w-4 h-4" />
                      </button>
                      <button
                        onClick={() => handleRejectTool(msg.tool!.id)}
                        className="p-1 rounded bg-error text-white hover:bg-error/80"
                      >
                        <X className="w-4 h-4" />
                      </button>
                    </div>
                  )}
                </div>

                <div className="px-3 py-2 text-xs font-mono text-muted-foreground">
                  {JSON.stringify(msg.tool.arguments, null, 2)}
                </div>

    {/* Tool output hidden from frontend */}
              </div>
            )}
          </div>
        ))}

        {/* Thinking indicator - show actual content if available, otherwise just spinner */}
        {isReady && !isIdle && isThinking && (
          <div className="flex justify-start">
            {thinkingContent && thinkingContent !== "Thinking..." ? (
              <div className="max-w-[80%] bg-card border border-border rounded-xl px-4 py-3">
                <div className="flex items-center gap-2 mb-2">
                  <Loader2 className="w-4 h-4 animate-spin text-primary" />
                  <span className="text-sm font-medium text-muted-foreground">Thinking</span>
                </div>
                <pre className="whitespace-pre-wrap font-sans text-sm text-muted-foreground/80 max-h-64 overflow-auto">
                  {thinkingContent}
                </pre>
              </div>
            ) : (
              <div className="bg-card border border-border rounded-xl px-4 py-3 flex items-center gap-2">
                <Loader2 className="w-4 h-4 animate-spin text-primary" />
                <span className="text-sm text-muted-foreground">Thinking...</span>
              </div>
            )}
          </div>
        )}

        {/* Loading indicator when processing but not thinking */}
        {isReady && !isIdle && !isThinking && (
          <div className="flex justify-start">
            <div className="bg-card border border-border rounded-xl px-4 py-3 flex items-center gap-2">
              <Loader2 className="w-4 h-4 animate-spin text-primary" />
              <span className="text-sm text-muted-foreground">Working...</span>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input - always enabled to allow queueing messages */}
      <form onSubmit={handleSubmit} className="p-4 border-t border-border bg-card/50">
        <div className="flex gap-3">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={isIdle ? "Type a message..." : "Type to queue message..."}
            className="flex-1 rounded-xl border border-border bg-background px-4 py-3 text-foreground placeholder-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
          <Button
            type="submit"
            disabled={!input.trim()}
            variant="gradient"
            size="lg"
          >
            <Send className="w-5 h-5" />
          </Button>
        </div>
      </form>
    </div>
  )
}
