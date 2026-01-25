interface ToolCallMessageProps {
  formatted: string
}

/**
 * Renders a tool call in Claude Code style: ● ToolName(args...)
 */
export default function ToolCallMessage({ formatted }: ToolCallMessageProps) {
  return (
    <div className="flex items-start gap-2 py-1">
      <span className="text-foreground font-medium select-none">●</span>
      <span className="font-mono text-sm text-cyan-600 dark:text-cyan-400 break-all">
        {formatted}
      </span>
    </div>
  )
}
