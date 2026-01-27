import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Database, AlertTriangle, Trash2, Minimize2 } from 'lucide-react'
import type { ContextUsage } from '../bindings/LoopOutput'

// Check if we're running in Tauri
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

interface ContextIndicatorProps {
  sessionId: string | null
  contextUsage?: ContextUsage  // Now passed from session context
  onClear?: () => void
}

// Mock data for browser testing
const mockUsage: ContextUsage = {
  used_tokens: 45000,
  limit_tokens: 200000,
  used_percentage: 0.225,
  remaining_tokens: 155000,
  should_compact: false,
  breakdown: {
    system_tokens: 5000,
    conversation_tokens: 35000,
    tool_tokens: 3000,
    memory_tokens: 2000,
    input_tokens: 40000,
    output_tokens: 5000,
  },
}

export default function ContextIndicator({ sessionId, contextUsage, onClear }: ContextIndicatorProps) {
  const usage = contextUsage || (isTauri ? null : mockUsage)
  const [isExpanded, setIsExpanded] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleClear = async () => {
    if (!isTauri) {
      // Mock clear in browser - just call onClear
      onClear?.()
      return
    }

    if (!sessionId) return

    try {
      await invoke('clear_session', { sessionId })
      onClear?.()
    } catch (err) {
      console.error('Failed to clear:', err)
      setError(String(err))
    }
  }

  if (!usage) {
    return null
  }

  // Determine color based on usage
  const getProgressColor = () => {
    if (usage.used_percentage >= 0.9) return 'bg-red-500'
    if (usage.used_percentage >= 0.75) return 'bg-yellow-500'
    if (usage.used_percentage >= 0.5) return 'bg-blue-500'
    return 'bg-green-500'
  }

  const formatTokens = (tokens: number) => {
    if (tokens >= 1000000) return `${(tokens / 1000000).toFixed(1)}M`
    if (tokens >= 1000) return `${(tokens / 1000).toFixed(1)}K`
    return tokens.toString()
  }

  return (
    <div className="relative">
      {/* Compact indicator button */}
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className={`
          flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm
          transition-colors duration-200
          ${usage.should_compact
            ? 'bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-200 border border-yellow-300 dark:border-yellow-700'
            : 'bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 border border-gray-200 dark:border-gray-700'
          }
          hover:bg-gray-200 dark:hover:bg-gray-700
        `}
        title="Context usage"
      >
        {usage.should_compact ? (
          <AlertTriangle className="w-4 h-4" />
        ) : (
          <Database className="w-4 h-4" />
        )}

        {/* Mini progress bar */}
        <div className="w-16 h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
          <div
            className={`h-full ${getProgressColor()} transition-all duration-300`}
            style={{ width: `${Math.min(usage.used_percentage * 100, 100)}%` }}
          />
        </div>

        <span className="font-mono text-xs">
          {(usage.used_percentage * 100).toFixed(0)}%
        </span>
      </button>

      {/* Expanded panel */}
      {isExpanded && (
        <div className="absolute right-0 top-full mt-2 w-72 bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700 z-50">
          <div className="p-4">
            <div className="flex items-center justify-between mb-3">
              <h3 className="font-semibold text-gray-900 dark:text-white">Context Usage</h3>
              <button
                onClick={() => setIsExpanded(false)}
                className="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200"
              >
                <Minimize2 className="w-4 h-4" />
              </button>
            </div>

            {/* Main progress bar */}
            <div className="mb-4">
              <div className="flex justify-between text-xs text-gray-500 dark:text-gray-400 mb-1">
                <span>{formatTokens(usage.used_tokens)} used</span>
                <span>{formatTokens(usage.remaining_tokens)} remaining</span>
              </div>
              <div className="h-3 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
                <div
                  className={`h-full ${getProgressColor()} transition-all duration-300`}
                  style={{ width: `${Math.min(usage.used_percentage * 100, 100)}%` }}
                />
              </div>
              <div className="text-center text-xs text-gray-500 dark:text-gray-400 mt-1">
                {formatTokens(usage.limit_tokens)} total capacity
              </div>
            </div>

            {/* Breakdown */}
            <div className="space-y-2 mb-4">
              <h4 className="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide">
                Breakdown
              </h4>
              <div className="grid grid-cols-2 gap-2 text-xs">
                <div className="flex justify-between">
                  <span className="text-gray-600 dark:text-gray-400">System:</span>
                  <span className="font-mono text-gray-900 dark:text-white">
                    {formatTokens(usage.breakdown.system_tokens)}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-600 dark:text-gray-400">Memory:</span>
                  <span className="font-mono text-gray-900 dark:text-white">
                    {formatTokens(usage.breakdown.memory_tokens)}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-600 dark:text-gray-400">Conversation:</span>
                  <span className="font-mono text-gray-900 dark:text-white">
                    {formatTokens(usage.breakdown.conversation_tokens)}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-600 dark:text-gray-400">Tools:</span>
                  <span className="font-mono text-gray-900 dark:text-white">
                    {formatTokens(usage.breakdown.tool_tokens)}
                  </span>
                </div>
              </div>
            </div>

            {/* Warning message */}
            {usage.should_compact && (
              <div className="mb-4 p-2 bg-yellow-50 dark:bg-yellow-900/20 rounded border border-yellow-200 dark:border-yellow-800">
                <p className="text-xs text-yellow-800 dark:text-yellow-200">
                  Context is getting full. Older messages will be summarized automatically.
                </p>
              </div>
            )}

            {/* Actions */}
            <button
              onClick={handleClear}
              className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-sm
                bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300
                hover:bg-red-200 dark:hover:bg-red-900/50
                transition-colors duration-200"
              title="Clear conversation"
            >
              <Trash2 className="w-4 h-4" />
              Clear History
            </button>

            {/* Error message */}
            {error && (
              <p className="mt-2 text-xs text-red-600 dark:text-red-400">{error}</p>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
