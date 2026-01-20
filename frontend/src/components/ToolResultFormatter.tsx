import { File, Folder, FileText, AlertCircle, CheckCircle2, Search, Code } from 'lucide-react'

interface ToolResultFormatterProps {
  toolName: string
  result: string
}

interface DirectoryEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
}

interface DirectoryResult {
  count: number
  entries: DirectoryEntry[]
}

interface GrepMatch {
  path: string
  line_number?: number
  line?: string
  count?: number
}

interface GrepResult {
  matches: GrepMatch[]
  total_matches?: number
}

interface GlobResult {
  files: string[]
  count: number
}

// Format file size to human readable
function formatSize(bytes: number): string {
  if (bytes === 0) return '-'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${units[i]}`
}

// Try to parse JSON, return null if not valid
function tryParseJson<T>(str: string): T | null {
  try {
    return JSON.parse(str) as T
  } catch {
    return null
  }
}

// Format directory listing
function DirectoryListing({ data }: { data: DirectoryResult }) {
  const sortedEntries = [...data.entries].sort((a, b) => {
    // Directories first, then alphabetically
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
    return a.name.localeCompare(b.name)
  })

  return (
    <div className="space-y-1">
      <div className="text-xs text-muted-foreground mb-2">
        {data.count} items
      </div>
      <div className="space-y-0.5">
        {sortedEntries.map((entry, i) => (
          <div
            key={i}
            className="flex items-center gap-2 text-sm py-0.5 hover:bg-secondary/50 rounded px-1 -mx-1"
          >
            {entry.is_dir ? (
              <Folder className="w-4 h-4 text-blue-500 flex-shrink-0" />
            ) : (
              <File className="w-4 h-4 text-muted-foreground flex-shrink-0" />
            )}
            <span className={`flex-1 truncate ${entry.is_dir ? 'text-blue-500 font-medium' : 'text-foreground'}`}>
              {entry.name}
              {entry.is_dir && '/'}
            </span>
            {!entry.is_dir && (
              <span className="text-xs text-muted-foreground tabular-nums">
                {formatSize(entry.size)}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}

// Format grep/search results
function GrepResults({ data }: { data: GrepResult }) {
  return (
    <div className="space-y-1">
      {data.total_matches !== undefined && (
        <div className="text-xs text-muted-foreground mb-2">
          {data.total_matches} matches in {data.matches.length} files
        </div>
      )}
      <div className="space-y-1">
        {data.matches.slice(0, 50).map((match, i) => (
          <div key={i} className="text-sm">
            <div className="flex items-center gap-2">
              <Search className="w-3 h-3 text-muted-foreground flex-shrink-0" />
              <span className="text-blue-500 truncate">{match.path}</span>
              {match.line_number && (
                <span className="text-xs text-muted-foreground">:{match.line_number}</span>
              )}
              {match.count !== undefined && (
                <span className="text-xs bg-secondary px-1.5 rounded">{match.count} matches</span>
              )}
            </div>
            {match.line && (
              <pre className="text-xs text-muted-foreground ml-5 mt-0.5 truncate">
                {match.line.trim()}
              </pre>
            )}
          </div>
        ))}
        {data.matches.length > 50 && (
          <div className="text-xs text-muted-foreground mt-2">
            ... and {data.matches.length - 50} more files
          </div>
        )}
      </div>
    </div>
  )
}

// Format glob results (file list)
function GlobResults({ data }: { data: GlobResult }) {
  return (
    <div className="space-y-1">
      <div className="text-xs text-muted-foreground mb-2">
        {data.count} files found
      </div>
      <div className="space-y-0.5">
        {data.files.slice(0, 50).map((file, i) => (
          <div key={i} className="flex items-center gap-2 text-sm">
            <FileText className="w-4 h-4 text-muted-foreground flex-shrink-0" />
            <span className="text-foreground truncate">{file}</span>
          </div>
        ))}
        {data.files.length > 50 && (
          <div className="text-xs text-muted-foreground mt-2">
            ... and {data.files.length - 50} more files
          </div>
        )}
      </div>
    </div>
  )
}

// Format file content
function FileContent({ content, path }: { content: string; path?: string }) {
  const extension = path?.split('.').pop()?.toLowerCase() || ''
  const isCode = ['rs', 'ts', 'tsx', 'js', 'jsx', 'py', 'go', 'java', 'c', 'cpp', 'h', 'hpp', 'json', 'toml', 'yaml', 'yml', 'md', 'sh', 'bash', 'css', 'scss', 'html', 'xml'].includes(extension)

  return (
    <div className="space-y-1">
      {path && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
          <Code className="w-3 h-3" />
          <span>{path}</span>
        </div>
      )}
      <pre className={`text-xs whitespace-pre-wrap ${isCode ? 'font-mono' : ''} text-foreground max-h-60 overflow-y-auto`}>
        {content}
      </pre>
    </div>
  )
}

// Format command output
function CommandOutput({ result }: { result: string }) {
  // Try to parse as JSON with stdout/stderr
  const parsed = tryParseJson<{ stdout?: string; stderr?: string; exit_code?: number; success?: boolean }>(result)

  if (parsed && (parsed.stdout !== undefined || parsed.stderr !== undefined)) {
    return (
      <div className="space-y-2">
        {parsed.exit_code !== undefined && (
          <div className="flex items-center gap-2 text-xs">
            {parsed.exit_code === 0 || parsed.success ? (
              <CheckCircle2 className="w-3 h-3 text-success" />
            ) : (
              <AlertCircle className="w-3 h-3 text-error" />
            )}
            <span className={parsed.exit_code === 0 ? 'text-success' : 'text-error'}>
              Exit code: {parsed.exit_code}
            </span>
          </div>
        )}
        {parsed.stdout && (
          <pre className="text-xs font-mono whitespace-pre-wrap text-foreground max-h-40 overflow-y-auto">
            {parsed.stdout}
          </pre>
        )}
        {parsed.stderr && (
          <pre className="text-xs font-mono whitespace-pre-wrap text-error/80 max-h-40 overflow-y-auto">
            {parsed.stderr}
          </pre>
        )}
      </div>
    )
  }

  // Plain text output
  return (
    <pre className="text-xs font-mono whitespace-pre-wrap text-foreground max-h-40 overflow-y-auto">
      {result}
    </pre>
  )
}

// Format success/error messages
function StatusMessage({ result }: { result: string }) {
  const parsed = tryParseJson<{ success?: boolean; message?: string; error?: string }>(result)

  if (parsed) {
    const isSuccess = parsed.success === true
    const message = parsed.message || parsed.error || JSON.stringify(parsed)

    return (
      <div className="flex items-start gap-2">
        {isSuccess ? (
          <CheckCircle2 className="w-4 h-4 text-success flex-shrink-0 mt-0.5" />
        ) : parsed.error ? (
          <AlertCircle className="w-4 h-4 text-error flex-shrink-0 mt-0.5" />
        ) : null}
        <span className={`text-sm ${parsed.error ? 'text-error' : 'text-foreground'}`}>
          {message}
        </span>
      </div>
    )
  }

  return <span className="text-sm text-foreground">{result}</span>
}

export default function ToolResultFormatter({ toolName, result }: ToolResultFormatterProps) {
  // Handle empty or very short results
  if (!result || result.trim() === '') {
    return <span className="text-xs text-muted-foreground italic">No output</span>
  }

  // Try to format based on tool name
  switch (toolName) {
    case 'list_directory': {
      const data = tryParseJson<DirectoryResult>(result)
      if (data?.entries) {
        return <DirectoryListing data={data} />
      }
      break
    }

    case 'grep':
    case 'search_code':
    case 'ripgrep': {
      const data = tryParseJson<GrepResult>(result)
      if (data?.matches) {
        return <GrepResults data={data} />
      }
      break
    }

    case 'glob':
    case 'find_files': {
      const data = tryParseJson<GlobResult>(result)
      if (data?.files) {
        return <GlobResults data={data} />
      }
      break
    }

    case 'read_file':
    case 'read_pdf':
    case 'read_office_doc': {
      // Check if it's JSON with content field
      const parsed = tryParseJson<{ content?: string; path?: string }>(result)
      if (parsed?.content) {
        return <FileContent content={parsed.content} path={parsed.path} />
      }
      // Otherwise treat as plain file content
      return <FileContent content={result} />
    }

    case 'execute_command':
    case 'shell':
    case 'bash':
    case 'run_command': {
      return <CommandOutput result={result} />
    }

    case 'write_file':
    case 'edit_file':
    case 'create_file':
    case 'delete_file': {
      return <StatusMessage result={result} />
    }

    default:
      break
  }

  // For unknown tools, try to detect the format
  const parsed = tryParseJson<Record<string, unknown>>(result)

  if (parsed) {
    // Check for common patterns
    if ('entries' in parsed && Array.isArray(parsed.entries)) {
      return <DirectoryListing data={parsed as unknown as DirectoryResult} />
    }
    if ('matches' in parsed && Array.isArray(parsed.matches)) {
      return <GrepResults data={parsed as unknown as GrepResult} />
    }
    if ('files' in parsed && Array.isArray(parsed.files)) {
      return <GlobResults data={parsed as unknown as GlobResult} />
    }
    if ('success' in parsed || 'error' in parsed || 'message' in parsed) {
      return <StatusMessage result={result} />
    }
    if ('stdout' in parsed || 'stderr' in parsed) {
      return <CommandOutput result={result} />
    }
  }

  // Default: show as preformatted text (truncated if too long)
  const maxLength = 2000
  const truncated = result.length > maxLength
  const displayResult = truncated ? result.slice(0, maxLength) + '...' : result

  return (
    <div>
      <pre className="text-xs font-mono text-muted-foreground whitespace-pre-wrap max-h-40 overflow-y-auto">
        {displayResult}
      </pre>
      {truncated && (
        <div className="text-xs text-muted-foreground mt-1">
          (truncated, {result.length.toLocaleString()} total characters)
        </div>
      )}
    </div>
  )
}
