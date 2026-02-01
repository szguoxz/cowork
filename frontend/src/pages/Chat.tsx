import { useState, useRef, useEffect, useCallback } from 'react'
import { Send, Loader2, X, AlertCircle, Sparkles, Square, Paperclip } from 'lucide-react'
import { Button } from '../components/ui/button'
import SessionTabs from '../components/SessionTabs'
import ApprovalModal from '../components/ApprovalModal'
import QuestionModal from '../components/QuestionModal'
import ToolCallMessage from '../components/ToolCallMessage'
import ToolResultMessage from '../components/ToolResultMessage'
import { useSession, ImageData } from '../context/SessionContext'

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
    sendMessageWithImages,
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
  const [pendingImages, setPendingImages] = useState<ImageData[]>([])
  const [isDragging, setIsDragging] = useState(false)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const textInputRef = useRef<HTMLInputElement>(null)

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

  // Auto-focus input on mount and when session becomes active
  useEffect(() => {
    if (isInitialized && hasApiKey && !modal) {
      textInputRef.current?.focus()
    }
  }, [isInitialized, hasApiKey, activeSessionId, modal])

  // Sync session error to local error state
  useEffect(() => {
    if (session?.error) {
      setError(session.error)
    }
  }, [session?.error])

  // Convert File to base64 ImageData
  const fileToImageData = async (file: File): Promise<ImageData> => {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => {
        const result = reader.result as string
        // Remove data URL prefix (e.g., "data:image/png;base64,")
        const base64 = result.split(',')[1]
        resolve({
          data: base64,
          media_type: file.type || 'image/png'
        })
      }
      reader.onerror = reject
      reader.readAsDataURL(file)
    })
  }

  // Handle file selection
  const handleFileSelect = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files
    if (!files) return

    const imageFiles = Array.from(files).filter(f => f.type.startsWith('image/'))
    if (imageFiles.length === 0) return

    try {
      const newImages = await Promise.all(imageFiles.map(fileToImageData))
      setPendingImages(prev => [...prev, ...newImages])
    } catch (err) {
      console.error('Failed to read image:', err)
      setError('Failed to read image file')
    }

    // Reset input so same file can be selected again
    e.target.value = ''
  }

  // Remove a pending image
  const removeImage = (index: number) => {
    setPendingImages(prev => prev.filter((_, i) => i !== index))
  }

  // Drag and drop handlers
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragging(true)
  }

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragging(false)
  }

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragging(false)

    const files = Array.from(e.dataTransfer.files)
    const imageFiles = files.filter(f => f.type.startsWith('image/'))
    if (imageFiles.length === 0) return

    try {
      const newImages = await Promise.all(imageFiles.map(fileToImageData))
      setPendingImages(prev => [...prev, ...newImages])
    } catch (err) {
      console.error('Failed to read dropped image:', err)
      setError('Failed to read dropped image')
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const hasContent = input.trim() || pendingImages.length > 0
    if (!hasContent || modal) return

    const userMessage = input.trim() || 'What is in this image?'
    const images = [...pendingImages]
    setInput('')
    setPendingImages([])
    setError(null)

    try {
      if (images.length > 0) {
        await sendMessageWithImages(userMessage, images)
      } else {
        await sendMessage(userMessage)
      }
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
            {/* Context usage is now appended to message content by core */}
            {session?.provider && (
              <span>{session.provider.type}</span>
            )}
          </div>
        </div>
      )}

      {/* Input */}
      <form
        onSubmit={handleSubmit}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        className={`p-4 border-t border-border bg-card/50 transition-colors ${isDragging ? 'bg-primary/10 border-primary' : ''}`}
      >
        {/* Image Previews */}
        {pendingImages.length > 0 && (
          <div className="flex gap-2 mb-3 flex-wrap">
            {pendingImages.map((img, i) => (
              <div key={i} className="relative group">
                <img
                  src={`data:${img.media_type};base64,${img.data}`}
                  alt={`Attachment ${i + 1}`}
                  className="w-16 h-16 object-cover rounded-lg border border-border"
                />
                <button
                  type="button"
                  onClick={() => removeImage(i)}
                  className="absolute -top-2 -right-2 w-5 h-5 bg-error text-white rounded-full flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  <X className="w-3 h-3" />
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="flex gap-3">
          {/* Hidden file input */}
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            multiple
            onChange={handleFileSelect}
            className="hidden"
          />

          {/* Attachment button */}
          <Button
            type="button"
            variant="ghost"
            size="lg"
            onClick={() => fileInputRef.current?.click()}
            disabled={!!modal}
            className="px-3"
            title="Attach images"
          >
            <Paperclip className="w-5 h-5" />
          </Button>

          <input
            ref={textInputRef}
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={modal ? "Waiting for response..." : isDragging ? "Drop images here..." : pendingImages.length > 0 ? "Add a message about these images..." : "Type a message..."}
            disabled={!!modal}
            className="flex-1 rounded-xl border border-border bg-background px-4 py-3 text-foreground placeholder-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 disabled:opacity-50 disabled:cursor-not-allowed"
          />
          <Button
            type="submit"
            disabled={(!input.trim() && pendingImages.length === 0) || !!modal}
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
          description={modal.description}
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
