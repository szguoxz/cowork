import { useState } from 'react'
import { ChevronDown, ChevronRight } from 'lucide-react'
import type { DiffLine } from '../bindings'
import DiffPreview from './DiffPreview'

interface ToolResultMessageProps {
  summary: string
  diffPreview?: DiffLine[]
  output?: string
  success?: boolean
}

/**
 * Renders a tool result in Claude Code style: ⎿ summary
 * With optional expandable diff preview
 */
export default function ToolResultMessage({
  summary,
  diffPreview,
  output,
  success = true,
}: ToolResultMessageProps) {
  const [expanded, setExpanded] = useState(false)
  const hasExpandableContent = (diffPreview && diffPreview.length > 0) || (output && output.length > 100)

  const summaryColor = success
    ? 'text-muted-foreground'
    : 'text-red-600 dark:text-red-400'

  return (
    <div className="pl-4 py-0.5">
      <div
        className={`flex items-start gap-2 ${hasExpandableContent ? 'cursor-pointer hover:bg-muted/30 rounded' : ''}`}
        onClick={() => hasExpandableContent && setExpanded(!expanded)}
      >
        {hasExpandableContent ? (
          expanded ? (
            <ChevronDown className="w-3 h-3 text-muted-foreground mt-1 flex-shrink-0" />
          ) : (
            <ChevronRight className="w-3 h-3 text-muted-foreground mt-1 flex-shrink-0" />
          )
        ) : (
          <span className="text-muted-foreground select-none">⎿</span>
        )}
        <span className={`text-sm ${summaryColor}`}>{summary}</span>
      </div>

      {expanded && diffPreview && diffPreview.length > 0 && (
        <div className="pl-5 mt-1">
          <DiffPreview lines={diffPreview} />
        </div>
      )}

      {expanded && output && output.length > 100 && !diffPreview?.length && (
        <div className="pl-5 mt-1">
          <pre className="font-mono text-xs text-muted-foreground whitespace-pre-wrap max-h-48 overflow-y-auto">
            {output.slice(0, 2000)}
            {output.length > 2000 && '\n... (truncated)'}
          </pre>
        </div>
      )}
    </div>
  )
}
