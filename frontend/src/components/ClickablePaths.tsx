import { open } from '@tauri-apps/plugin-shell'
import { ExternalLink } from 'lucide-react'

interface ClickablePathsProps {
  text: string
}

// Regex to match file paths:
// - Windows: C:\path\to\file.ext or C:/path/to/file.ext
// - Unix: /path/to/file.ext
// - Relative: ./path or ../path
// Must end with a file extension or be in backticks
const FILE_PATH_REGEX = /`([A-Za-z]:[\\\/][^\s`]+|\/[^\s`]+|\.\.?\/[^\s`]+)`|(?<![`\w])([A-Za-z]:[\\\/][^\s<>"|*?]+\.[a-zA-Z0-9]+|\/(?:[\w.-]+\/)*[\w.-]+\.[a-zA-Z0-9]+)(?![`\w])/g

/**
 * Renders text with clickable file paths.
 * File paths in backticks or ending with extensions become clickable.
 */
export default function ClickablePaths({ text }: ClickablePathsProps) {
  const parts: Array<{ type: 'text' | 'path'; content: string }> = []
  let lastIndex = 0

  // Find all file path matches
  const matches = [...text.matchAll(FILE_PATH_REGEX)]

  for (const match of matches) {
    // Add text before this match
    if (match.index !== undefined && match.index > lastIndex) {
      parts.push({ type: 'text', content: text.slice(lastIndex, match.index) })
    }

    // The path is either in group 1 (backtick) or group 2 (bare path)
    const path = match[1] || match[2]
    if (path) {
      // Check if it was in backticks - if so, include them in the display
      const wasBackticked = match[0].startsWith('`')
      parts.push({
        type: 'path',
        content: wasBackticked ? path : match[0]
      })
    }

    lastIndex = (match.index || 0) + match[0].length
  }

  // Add remaining text
  if (lastIndex < text.length) {
    parts.push({ type: 'text', content: text.slice(lastIndex) })
  }

  // If no paths found, just return the text
  if (parts.length === 0) {
    return <>{text}</>
  }

  const handleOpen = async (path: string, e: React.MouseEvent) => {
    e.preventDefault()
    try {
      // Normalize path separators for the current platform
      const normalizedPath = path.replace(/\\/g, '/')
      await open(normalizedPath)
    } catch (err) {
      console.error('Failed to open file:', err)
    }
  }

  return (
    <>
      {parts.map((part, i) => {
        if (part.type === 'text') {
          return <span key={i}>{part.content}</span>
        }

        return (
          <button
            key={i}
            onClick={(e) => handleOpen(part.content, e)}
            className="inline-flex items-center gap-1 text-primary hover:text-primary/80 hover:underline cursor-pointer bg-primary/5 hover:bg-primary/10 px-1 rounded transition-colors"
            title={`Open ${part.content}`}
          >
            <code className="text-sm">{part.content}</code>
            <ExternalLink className="w-3 h-3 inline-block" />
          </button>
        )
      })}
    </>
  )
}
