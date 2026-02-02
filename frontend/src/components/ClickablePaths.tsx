import { open } from '@tauri-apps/plugin-shell'
import { ExternalLink, Globe } from 'lucide-react'

interface ClickablePathsProps {
  text: string
}

// Regex to match file paths:
// - Windows: C:\path\to\file.ext or C:/path/to/file.ext
// - Unix: /path/to/file.ext (optionally with :line_number suffix)
// - Relative: ./path or ../path
// Must end with a file extension or be in backticks
// Paths in backticks can contain any characters; bare paths must have extensions
const FILE_PATH_REGEX = /`([A-Za-z]:[\\\/][^\n`]+|\/[^\n`]+|\.\.?\/[^\n`]+)`|(?<![`\w])([A-Za-z]:[\\\/][^\s<>"|*?]+\.[a-zA-Z0-9]+(?::\d+)?|\/[^\s<>"|*?:]+\.[a-zA-Z0-9]+(?::\d+)?)(?![`\w])/g

// Regex to match URLs (http, https, ftp)
const URL_REGEX = /https?:\/\/[^\s<>)"'\]]+|ftp:\/\/[^\s<>)"'\]]+/g

type PartType = 'text' | 'path' | 'url'

interface Part {
  type: PartType
  content: string
  index: number
}

/**
 * Renders text with clickable file paths and URLs.
 * - File paths in backticks or ending with extensions become clickable
 * - URLs (http://, https://, ftp://) become clickable
 */
export default function ClickablePaths({ text }: ClickablePathsProps) {
  const parts: Part[] = []

  // Find all file path matches
  for (const match of text.matchAll(FILE_PATH_REGEX)) {
    const path = match[1] || match[2]
    if (path && match.index !== undefined) {
      parts.push({
        type: 'path',
        content: match[1] ? path : match[0], // Use full match for bare paths
        index: match.index
      })
    }
  }

  // Find all URL matches
  for (const match of text.matchAll(URL_REGEX)) {
    if (match.index !== undefined) {
      // Clean up trailing punctuation that's likely not part of URL
      let url = match[0]
      while (url.endsWith('.') || url.endsWith(',') || url.endsWith(';') || url.endsWith(':')) {
        url = url.slice(0, -1)
      }
      parts.push({
        type: 'url',
        content: url,
        index: match.index
      })
    }
  }

  // Sort by index
  parts.sort((a, b) => a.index - b.index)

  // Remove overlapping matches (keep the first one)
  const filteredParts: Part[] = []
  let lastEnd = 0
  for (const part of parts) {
    if (part.index >= lastEnd) {
      filteredParts.push(part)
      lastEnd = part.index + part.content.length
    }
  }

  // If no matches found, just return the text
  if (filteredParts.length === 0) {
    return <>{text}</>
  }

  // Build final parts array with text segments
  const result: Array<{ type: PartType; content: string }> = []
  let currentIndex = 0

  for (const part of filteredParts) {
    // Add text before this match
    if (part.index > currentIndex) {
      result.push({ type: 'text', content: text.slice(currentIndex, part.index) })
    }
    result.push({ type: part.type, content: part.content })
    currentIndex = part.index + part.content.length
  }

  // Add remaining text
  if (currentIndex < text.length) {
    result.push({ type: 'text', content: text.slice(currentIndex) })
  }

  const handleOpen = async (target: string, e: React.MouseEvent) => {
    e.preventDefault()
    try {
      // For URLs, open directly
      if (target.includes('://')) {
        await open(target)
        return
      }

      // For file paths, strip line number suffix (e.g., :123) and normalize separators
      let filePath = target.replace(/:\d+$/, '')
      filePath = filePath.replace(/\\/g, '/')
      await open(filePath)
    } catch (err) {
      console.error('Failed to open:', err)
    }
  }

  return (
    <>
      {result.map((part, i) => {
        if (part.type === 'text') {
          return <span key={i}>{part.content}</span>
        }

        const isUrl = part.type === 'url'
        const Icon = isUrl ? Globe : ExternalLink

        return (
          <button
            key={i}
            onClick={(e) => handleOpen(part.content, e)}
            className="inline-flex items-center gap-1 text-primary hover:text-primary/80 hover:underline cursor-pointer bg-primary/5 hover:bg-primary/10 px-1 rounded transition-colors"
            title={isUrl ? `Open ${part.content}` : `Open file: ${part.content}`}
          >
            {isUrl ? (
              <span className="text-sm">{part.content}</span>
            ) : (
              <code className="text-sm">{part.content}</code>
            )}
            <Icon className="w-3 h-3 inline-block flex-shrink-0" />
          </button>
        )
      })}
    </>
  )
}
