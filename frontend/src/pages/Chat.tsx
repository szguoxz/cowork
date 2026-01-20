import { useState, useRef, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Send, Loader2, Check, X, Terminal, AlertCircle, Sparkles } from 'lucide-react'
import { Button } from '../components/ui/button'
import type { LoopOutput } from '../bindings'

interface ToolInfo {
  id: string
  name: string
  arguments: Record<string, unknown>
  status: 'pending' | 'executing' | 'done' | 'failed'
  output?: string
}

interface Message {
  id: string
  type: 'user' | 'assistant' | 'tool'
  content: string
  tool?: ToolInfo
}

export default function Chat() {
  const [input, setInput] = useState('')
  const [messages, setMessages] = useState<Message[]>([])
  const [isIdle, setIsIdle] = useState(false)
  const [isLoopStarted, setIsLoopStarted] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [hasApiKey, setHasApiKey] = useState<boolean | null>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages])

  // Initialize: set up listener FIRST, then start loop
  useEffect(() => {
    let unlistenFn: (() => void) | null = null

    const init = async () => {
      console.log('Setting up event listener...')

      // 1. Set up event listener FIRST (before starting loop)
      unlistenFn = await listen<LoopOutput>('loop_output', (event) => {
        const output = event.payload
        console.log('Loop output received:', output)

        switch (output.type) {
          case 'ready':
            setIsLoopStarted(true)
            break

          case 'idle':
            setIsIdle(true)
            break

          case 'user_message':
            setIsIdle(false)
            setMessages(prev => [...prev, {
              id: output.id,
              type: 'user',
              content: output.content,
            }])
            break

          case 'assistant_message':
            setMessages(prev => [...prev, {
              id: output.id,
              type: 'assistant',
              content: output.content,
            }])
            break

          case 'tool_start':
            console.log('tool_start:', output.id, output.name)
            setMessages(prev => [...prev, {
              id: output.id,
              type: 'tool',
              content: '',
              tool: {
                id: output.id,
                name: output.name,
                arguments: output.arguments as Record<string, unknown>,
                status: 'executing',
              }
            }])
            break

          case 'tool_pending':
            setMessages(prev => [...prev, {
              id: output.id,
              type: 'tool',
              content: '',
              tool: {
                id: output.id,
                name: output.name,
                arguments: output.arguments as Record<string, unknown>,
                status: 'pending',
              }
            }])
            break

          case 'tool_done':
            console.log('tool_done:', output.id, 'success:', output.success, 'output:', output.output?.substring(0, 100))
            setMessages(prev => {
              const found = prev.some(msg => msg.tool?.id === output.id)
              console.log('tool_done: found matching message:', found)
              return prev.map(msg =>
                msg.tool?.id === output.id
                  ? {
                      ...msg,
                      tool: {
                        ...msg.tool!,
                        status: output.success ? 'done' : 'failed',
                        output: output.output,
                      }
                    }
                  : msg
              )
            })
            break

          case 'error':
            setError(output.message)
            setIsIdle(true)
            break

          case 'stopped':
            setIsLoopStarted(false)
            setIsIdle(false)
            break
        }
      })

      console.log('Event listener set up, checking API key...')

      // 2. Check API key
      try {
        const hasKey = await invoke<boolean>('check_api_key')
        console.log('API key check result:', hasKey)
        setHasApiKey(hasKey)

        // 3. Start the loop (listener is already active)
        if (hasKey) {
          console.log('Starting loop...')
          await invoke('start_loop')
          console.log('start_loop returned successfully')
          // Set started immediately - the Ready event will confirm but this ensures we progress
          setIsLoopStarted(true)
        }
      } catch (err) {
        console.error('Init error:', err)
        setError(String(err))
      }
    }

    init()

    // Cleanup
    return () => {
      if (unlistenFn) unlistenFn()
      invoke('stop_loop').catch(console.error)
    }
  }, [])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!input.trim() || !isIdle) return

    const userMessage = input
    setInput('')
    setError(null)

    try {
      await invoke('send_message', { content: userMessage })
    } catch (err) {
      console.error('Send error:', err)
      setError(String(err))
    }
  }

  const handleApproveTool = async (toolId: string) => {
    try {
      await invoke('approve_tool', { toolId })
      setMessages(prev => prev.map(msg =>
        msg.tool?.id === toolId
          ? { ...msg, tool: { ...msg.tool!, status: 'executing' } }
          : msg
      ))
    } catch (err) {
      setError(String(err))
    }
  }

  const handleRejectTool = async (toolId: string) => {
    try {
      await invoke('reject_tool', { toolId })
      setMessages(prev => prev.map(msg =>
        msg.tool?.id === toolId
          ? { ...msg, tool: { ...msg.tool!, status: 'failed', output: 'Rejected by user' } }
          : msg
      ))
    } catch (err) {
      setError(String(err))
    }
  }

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
          {isLoopStarted ? (isIdle ? 'Ready' : 'Working...') : 'Starting...'}
        </span>
      </header>

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

                {msg.tool.output && (
                  <div className="px-3 py-2 border-t border-border bg-background">
                    <pre className="whitespace-pre-wrap text-xs font-mono max-h-48 overflow-auto">
                      {msg.tool.output}
                    </pre>
                  </div>
                )}
              </div>
            )}
          </div>
        ))}

        {/* Loading indicator when not idle */}
        {isLoopStarted && !isIdle && (
          <div className="flex justify-start">
            <div className="bg-card border border-border rounded-xl px-4 py-3 flex items-center gap-2">
              <Loader2 className="w-4 h-4 animate-spin text-primary" />
              <span className="text-sm text-muted-foreground">Thinking...</span>
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
            placeholder={isIdle ? "Type a message..." : "Waiting..."}
            className="flex-1 rounded-xl border border-border bg-background px-4 py-3 text-foreground placeholder-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            disabled={!isIdle}
          />
          <Button
            type="submit"
            disabled={!isIdle || !input.trim()}
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
