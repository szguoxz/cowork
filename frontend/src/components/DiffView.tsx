import { useMemo } from 'react'

interface DiffViewProps {
  oldContent: string
  newContent: string
  filename?: string
  showLineNumbers?: boolean
  mode?: 'unified' | 'split'
}

interface DiffLine {
  type: 'unchanged' | 'added' | 'removed' | 'header'
  content: string
  oldLineNumber?: number
  newLineNumber?: number
}

export default function DiffView({
  oldContent,
  newContent,
  filename,
  showLineNumbers = true,
  mode = 'unified',
}: DiffViewProps) {
  const diffLines = useMemo(() => {
    return computeDiff(oldContent, newContent)
  }, [oldContent, newContent])

  const stats = useMemo(() => {
    const added = diffLines.filter((l) => l.type === 'added').length
    const removed = diffLines.filter((l) => l.type === 'removed').length
    return { added, removed }
  }, [diffLines])

  if (mode === 'split') {
    return (
      <SplitDiffView
        diffLines={diffLines}
        filename={filename}
        showLineNumbers={showLineNumbers}
        stats={stats}
      />
    )
  }

  return (
    <div className="rounded-lg border border-gray-300 dark:border-gray-600 overflow-hidden">
      {/* Header */}
      <div className="bg-gray-100 dark:bg-gray-800 px-4 py-2 flex items-center justify-between border-b border-gray-300 dark:border-gray-600">
        <span className="font-mono text-sm text-gray-700 dark:text-gray-300">
          {filename || 'Diff'}
        </span>
        <div className="flex items-center gap-3 text-xs">
          <span className="text-green-600 dark:text-green-400">+{stats.added}</span>
          <span className="text-red-600 dark:text-red-400">-{stats.removed}</span>
        </div>
      </div>

      {/* Diff content */}
      <div className="overflow-x-auto">
        <table className="w-full text-sm font-mono">
          <tbody>
            {diffLines.map((line, idx) => (
              <tr
                key={idx}
                className={`
                  ${line.type === 'added' ? 'bg-green-50 dark:bg-green-950' : ''}
                  ${line.type === 'removed' ? 'bg-red-50 dark:bg-red-950' : ''}
                  ${line.type === 'header' ? 'bg-blue-50 dark:bg-blue-950' : ''}
                `}
              >
                {showLineNumbers && (
                  <>
                    <td className="px-2 py-0 text-right text-gray-400 select-none w-12 border-r border-gray-200 dark:border-gray-700">
                      {line.oldLineNumber || ''}
                    </td>
                    <td className="px-2 py-0 text-right text-gray-400 select-none w-12 border-r border-gray-200 dark:border-gray-700">
                      {line.newLineNumber || ''}
                    </td>
                  </>
                )}
                <td
                  className={`px-2 py-0 whitespace-pre ${
                    line.type === 'added'
                      ? 'text-green-800 dark:text-green-200'
                      : line.type === 'removed'
                      ? 'text-red-800 dark:text-red-200'
                      : line.type === 'header'
                      ? 'text-blue-800 dark:text-blue-200'
                      : 'text-gray-800 dark:text-gray-200'
                  }`}
                >
                  {line.type === 'added' && '+ '}
                  {line.type === 'removed' && '- '}
                  {line.type === 'unchanged' && '  '}
                  {line.content}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

interface SplitDiffViewProps {
  diffLines: DiffLine[]
  filename?: string
  showLineNumbers: boolean
  stats: { added: number; removed: number }
}

function SplitDiffView({ diffLines, filename, showLineNumbers, stats }: SplitDiffViewProps) {
  // Pair up removed and added lines for side-by-side view
  const pairs = useMemo(() => {
    const result: Array<{ left: DiffLine | null; right: DiffLine | null }> = []
    let removedBuffer: DiffLine[] = []
    let addedBuffer: DiffLine[] = []

    const flushBuffers = () => {
      const maxLen = Math.max(removedBuffer.length, addedBuffer.length)
      for (let i = 0; i < maxLen; i++) {
        result.push({
          left: removedBuffer[i] || null,
          right: addedBuffer[i] || null,
        })
      }
      removedBuffer = []
      addedBuffer = []
    }

    for (const line of diffLines) {
      if (line.type === 'removed') {
        removedBuffer.push(line)
      } else if (line.type === 'added') {
        addedBuffer.push(line)
      } else {
        flushBuffers()
        result.push({ left: line, right: line })
      }
    }
    flushBuffers()

    return result
  }, [diffLines])

  return (
    <div className="rounded-lg border border-gray-300 dark:border-gray-600 overflow-hidden">
      {/* Header */}
      <div className="bg-gray-100 dark:bg-gray-800 px-4 py-2 flex items-center justify-between border-b border-gray-300 dark:border-gray-600">
        <span className="font-mono text-sm text-gray-700 dark:text-gray-300">
          {filename || 'Diff'}
        </span>
        <div className="flex items-center gap-3 text-xs">
          <span className="text-green-600 dark:text-green-400">+{stats.added}</span>
          <span className="text-red-600 dark:text-red-400">-{stats.removed}</span>
        </div>
      </div>

      {/* Split view */}
      <div className="flex">
        {/* Left side (old) */}
        <div className="w-1/2 border-r border-gray-300 dark:border-gray-600 overflow-x-auto">
          <table className="w-full text-sm font-mono">
            <tbody>
              {pairs.map((pair, idx) => (
                <tr
                  key={idx}
                  className={pair.left?.type === 'removed' ? 'bg-red-50 dark:bg-red-950' : ''}
                >
                  {showLineNumbers && (
                    <td className="px-2 py-0 text-right text-gray-400 select-none w-12 border-r border-gray-200 dark:border-gray-700">
                      {pair.left?.oldLineNumber || ''}
                    </td>
                  )}
                  <td className="px-2 py-0 whitespace-pre text-gray-800 dark:text-gray-200">
                    {pair.left?.content || ''}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {/* Right side (new) */}
        <div className="w-1/2 overflow-x-auto">
          <table className="w-full text-sm font-mono">
            <tbody>
              {pairs.map((pair, idx) => (
                <tr
                  key={idx}
                  className={pair.right?.type === 'added' ? 'bg-green-50 dark:bg-green-950' : ''}
                >
                  {showLineNumbers && (
                    <td className="px-2 py-0 text-right text-gray-400 select-none w-12 border-r border-gray-200 dark:border-gray-700">
                      {pair.right?.newLineNumber || ''}
                    </td>
                  )}
                  <td className="px-2 py-0 whitespace-pre text-gray-800 dark:text-gray-200">
                    {pair.right?.content || ''}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  )
}

/**
 * Simple diff algorithm - computes line-based diff
 */
function computeDiff(oldContent: string, newContent: string): DiffLine[] {
  const oldLines = oldContent.split('\n')
  const newLines = newContent.split('\n')

  // Simple LCS-based diff
  const lcs = longestCommonSubsequence(oldLines, newLines)
  const result: DiffLine[] = []

  let oldIdx = 0
  let newIdx = 0
  let oldLineNum = 1
  let newLineNum = 1

  for (const match of lcs) {
    // Add removed lines
    while (oldIdx < match.oldIdx) {
      result.push({
        type: 'removed',
        content: oldLines[oldIdx],
        oldLineNumber: oldLineNum++,
      })
      oldIdx++
    }

    // Add added lines
    while (newIdx < match.newIdx) {
      result.push({
        type: 'added',
        content: newLines[newIdx],
        newLineNumber: newLineNum++,
      })
      newIdx++
    }

    // Add unchanged line
    result.push({
      type: 'unchanged',
      content: oldLines[oldIdx],
      oldLineNumber: oldLineNum++,
      newLineNumber: newLineNum++,
    })
    oldIdx++
    newIdx++
  }

  // Add remaining removed lines
  while (oldIdx < oldLines.length) {
    result.push({
      type: 'removed',
      content: oldLines[oldIdx],
      oldLineNumber: oldLineNum++,
    })
    oldIdx++
  }

  // Add remaining added lines
  while (newIdx < newLines.length) {
    result.push({
      type: 'added',
      content: newLines[newIdx],
      newLineNumber: newLineNum++,
    })
    newIdx++
  }

  return result
}

interface LCSMatch {
  oldIdx: number
  newIdx: number
}

function longestCommonSubsequence(oldLines: string[], newLines: string[]): LCSMatch[] {
  const m = oldLines.length
  const n = newLines.length

  // Build LCS table
  const dp: number[][] = Array(m + 1)
    .fill(null)
    .map(() => Array(n + 1).fill(0))

  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (oldLines[i - 1] === newLines[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1] + 1
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1])
      }
    }
  }

  // Backtrack to find matches
  const matches: LCSMatch[] = []
  let i = m
  let j = n

  while (i > 0 && j > 0) {
    if (oldLines[i - 1] === newLines[j - 1]) {
      matches.unshift({ oldIdx: i - 1, newIdx: j - 1 })
      i--
      j--
    } else if (dp[i - 1][j] > dp[i][j - 1]) {
      i--
    } else {
      j--
    }
  }

  return matches
}
