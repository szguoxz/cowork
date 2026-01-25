import type { DiffLine } from '../bindings'

interface DiffPreviewProps {
  lines: DiffLine[]
  maxLines?: number
}

export default function DiffPreview({ lines, maxLines = 10 }: DiffPreviewProps) {
  const displayLines = lines.slice(0, maxLines)

  return (
    <div className="font-mono text-xs mt-1 space-y-0.5">
      {displayLines.map((line, idx) => {
        const lineNum = line.line_number ? String(line.line_number).padStart(4, ' ') : '    '
        const marker = line.line_type === 'added' ? '+' : line.line_type === 'removed' ? '-' : ' '

        let colorClass = 'text-muted-foreground'
        if (line.line_type === 'added') {
          colorClass = 'text-green-600 dark:text-green-400'
        } else if (line.line_type === 'removed') {
          colorClass = 'text-red-600 dark:text-red-400'
        }

        return (
          <div key={idx} className={`${colorClass} truncate`}>
            <span className="text-muted-foreground/50">{lineNum} </span>
            <span className={colorClass}>{marker} </span>
            <span>{line.content}</span>
          </div>
        )
      })}
      {lines.length > maxLines && (
        <div className="text-muted-foreground/50 italic">
          ... {lines.length - maxLines} more lines
        </div>
      )}
    </div>
  )
}
