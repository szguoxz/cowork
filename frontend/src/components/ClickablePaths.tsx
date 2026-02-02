import { open as openUrl } from '@tauri-apps/plugin-shell'
import { openPath } from '@tauri-apps/plugin-opener'
import { ExternalLink, Globe } from 'lucide-react'

interface ClickablePathsProps {
  text: string
}

// Regex to match file paths:
// - Windows: C:\path\to\file.ext or C:/path/to/file.ext
// - Unix: /path/to/file.ext (optionally with :line_number suffix)
// - Relative: ./path or ../path
// Paths in backticks, or bare paths ending with file extensions
const FILE_PATH_REGEX = /`([A-Za-z]:[\\\/][^\n`]+|\/[^\n`]+|\.\.?\/[^\n`]+)`|([A-Za-z]:[\\\/][^\s<>"|*?`]+\.[a-zA-Z0-9]+(?::\d+)?)|(?:^|[\s(])(\/?(?:[\w.-]+\/)+[\w.-]+\.[a-zA-Z0-9]+(?::\d+)?)/gm

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
    // match[1] = backtick path, match[2] = Windows path, match[3] = Unix path
    const path = match[1] || match[2] || match[3]
    if (path && match.index !== undefined) {
      // For backtick matches, use just the path; for bare paths use full match
      // But strip leading whitespace/parens that might be captured
      const fullMatch = match[0]
      const pathStart = fullMatch.indexOf(path)
      const actualIndex = match.index + pathStart
      parts.push({
        type: 'path',
        content: path,
        index: actualIndex
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
      // For URLs, use shell plugin
      if (target.includes('://')) {
        await openUrl(target)
        return
      }

      // For file paths, strip line number suffix (e.g., :123) and use opener plugin
      const filePath = target.replace(/:\d+$/, '')
      await openPath(filePath)
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
