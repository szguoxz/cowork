import React, { useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Check, X, AlertTriangle, Terminal, ChevronDown, ChevronUp } from 'lucide-react'

interface ToolCall {
  id: string
  name: string
  arguments: Record<string, unknown>
  status: 'Pending' | 'Approved' | 'Rejected' | 'Executing' | 'Completed' | 'Failed'
  result?: string
}

interface ToolApprovalPanelProps {
  sessionId: string
  pendingTools: ToolCall[]
  isLoopActive: boolean
  onApprove?: () => void
  onReject?: () => void
}

export default function ToolApprovalPanel({
  sessionId,
  pendingTools,
  isLoopActive,
  onApprove,
  onReject,
}: ToolApprovalPanelProps) {
  const [expanded, setExpanded] = React.useState(true)
  const [selectedTools, setSelectedTools] = React.useState<Set<string>>(new Set())

  // Reset selection when pending tools change
  useEffect(() => {
    setSelectedTools(new Set(pendingTools.map((t) => t.id)))
  }, [pendingTools])

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Only handle if we have pending tools
      if (pendingTools.length === 0) return

      // Y to approve all
      if (e.key === 'y' || e.key === 'Y') {
        e.preventDefault()
        handleApproveAll()
      }

      // N to reject all
      if (e.key === 'n' || e.key === 'N') {
        e.preventDefault()
        handleRejectAll()
      }

      // Escape to cancel loop
      if (e.key === 'Escape' && isLoopActive) {
        e.preventDefault()
        handleCancel()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [pendingTools, isLoopActive])

  const handleApproveAll = useCallback(async () => {
    try {
      await invoke('approve_loop_tools', {
        sessionId,
        toolIds: null, // null means approve all
      })
      onApprove?.()
    } catch (err) {
      console.error('Approve error:', err)
    }
  }, [sessionId, onApprove])

  const handleRejectAll = useCallback(async () => {
    try {
      await invoke('reject_loop_tools', {
        sessionId,
        toolIds: null,
      })
      onReject?.()
    } catch (err) {
      console.error('Reject error:', err)
    }
  }, [sessionId, onReject])

  const handleApproveSelected = useCallback(async () => {
    try {
      await invoke('approve_loop_tools', {
        sessionId,
        toolIds: Array.from(selectedTools),
      })
      onApprove?.()
    } catch (err) {
      console.error('Approve selected error:', err)
    }
  }, [sessionId, selectedTools, onApprove])

  const handleCancel = useCallback(async () => {
    try {
      await invoke('stop_loop', { sessionId })
    } catch (err) {
      console.error('Cancel error:', err)
    }
  }, [sessionId])

  const toggleTool = (toolId: string) => {
    setSelectedTools((prev) => {
      const next = new Set(prev)
      if (next.has(toolId)) {
        next.delete(toolId)
      } else {
        next.add(toolId)
      }
      return next
    })
  }

  if (pendingTools.length === 0) {
    return null
  }

  const isDestructive = pendingTools.some((t) =>
    ['write_file', 'edit', 'execute_command', 'delete_file'].includes(t.name)
  )

  return (
    <div
      className={`
        fixed bottom-4 left-1/2 -translate-x-1/2
        w-[600px] max-w-[90vw]
        bg-white dark:bg-gray-800
        rounded-lg shadow-2xl border
        ${isDestructive ? 'border-yellow-500' : 'border-gray-300 dark:border-gray-600'}
        z-50
      `}
    >
      {/* Header */}
      <div
        className={`
          flex items-center justify-between px-4 py-3
          rounded-t-lg cursor-pointer
          ${isDestructive ? 'bg-yellow-50 dark:bg-yellow-900/20' : 'bg-gray-50 dark:bg-gray-700'}
        `}
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-2">
          {isDestructive ? (
            <AlertTriangle className="w-5 h-5 text-yellow-600" />
          ) : (
            <Terminal className="w-5 h-5 text-gray-500" />
          )}
          <span className="font-medium text-gray-900 dark:text-white">
            {pendingTools.length} tool{pendingTools.length !== 1 ? 's' : ''} awaiting approval
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-gray-500">
            Press <kbd className="px-1.5 py-0.5 bg-gray-200 dark:bg-gray-600 rounded">Y</kbd> to approve,{' '}
            <kbd className="px-1.5 py-0.5 bg-gray-200 dark:bg-gray-600 rounded">N</kbd> to reject
          </span>
          {expanded ? <ChevronDown className="w-4 h-4" /> : <ChevronUp className="w-4 h-4" />}
        </div>
      </div>

      {/* Tool list */}
      {expanded && (
        <div className="max-h-64 overflow-y-auto">
          {pendingTools.map((tool) => (
            <div
              key={tool.id}
              className="flex items-start gap-3 px-4 py-3 border-t border-gray-200 dark:border-gray-700"
            >
              <input
                type="checkbox"
                checked={selectedTools.has(tool.id)}
                onChange={() => toggleTool(tool.id)}
                className="mt-1 h-4 w-4 rounded border-gray-300 text-primary-600 focus:ring-primary-500"
              />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="font-mono text-sm font-medium text-gray-900 dark:text-white">
                    {tool.name}
                  </span>
                  {isDestructiveTool(tool.name) && (
                    <span className="px-1.5 py-0.5 text-xs bg-yellow-100 dark:bg-yellow-900 text-yellow-800 dark:text-yellow-200 rounded">
                      destructive
                    </span>
                  )}
                </div>
                <pre className="mt-1 text-xs text-gray-500 dark:text-gray-400 whitespace-pre-wrap break-all">
                  {formatToolArgs(tool.arguments)}
                </pre>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center justify-between px-4 py-3 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-700 rounded-b-lg">
        <button
          onClick={handleCancel}
          className="px-3 py-1.5 text-sm text-gray-600 dark:text-gray-300 hover:text-gray-800 dark:hover:text-white"
        >
          Cancel (Esc)
        </button>
        <div className="flex items-center gap-2">
          <button
            onClick={handleRejectAll}
            className="flex items-center gap-1.5 px-4 py-1.5 text-sm bg-red-500 hover:bg-red-600 text-white rounded-md transition-colors"
          >
            <X className="w-4 h-4" />
            Reject All
          </button>
          <button
            onClick={selectedTools.size === pendingTools.length ? handleApproveAll : handleApproveSelected}
            className="flex items-center gap-1.5 px-4 py-1.5 text-sm bg-green-500 hover:bg-green-600 text-white rounded-md transition-colors"
          >
            <Check className="w-4 h-4" />
            {selectedTools.size === pendingTools.length
              ? 'Approve All'
              : `Approve ${selectedTools.size} Selected`}
          </button>
        </div>
      </div>
    </div>
  )
}

function isDestructiveTool(name: string): boolean {
  return ['write_file', 'edit', 'execute_command', 'delete_file', 'move_file'].includes(name)
}

function formatToolArgs(args: Record<string, unknown>): string {
  return Object.entries(args)
    .map(([key, value]) => {
      const strValue = typeof value === 'string' ? value : JSON.stringify(value)
      // Truncate long values
      const truncated = strValue.length > 100 ? strValue.slice(0, 100) + '...' : strValue
      return `${key}: ${truncated}`
    })
    .join('\n')
}
