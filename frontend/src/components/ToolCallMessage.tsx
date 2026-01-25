interface ToolCallMessageProps {
  formatted: string
  elapsedSecs?: number
}

function formatElapsed(secs: number): string {
  if (secs < 0.1) return ''
  if (secs < 60) return ` [${secs.toFixed(1)}s]`
  const mins = Math.floor(secs / 60)
  const remainingSecs = secs % 60
  return ` [${mins}m${remainingSecs.toFixed(0)}s]`
}

/**
 * Renders a tool call in Claude Code style: ● ToolName(args...) [Xs]
 */
export default function ToolCallMessage({ formatted, elapsedSecs }: ToolCallMessageProps) {
  const elapsed = elapsedSecs ? formatElapsed(elapsedSecs) : ''

  return (
    <div className="flex items-start gap-2 py-1">
      <span className="text-foreground font-medium select-none">●</span>
      <span className="font-mono text-sm text-cyan-600 dark:text-cyan-400 break-all">
        {formatted}
      </span>
      {elapsed && (
        <span className="font-mono text-xs text-muted-foreground">{elapsed}</span>
      )}
    </div>
  )
}
