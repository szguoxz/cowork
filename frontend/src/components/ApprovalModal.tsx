import { useEffect } from 'react'
import { Terminal } from 'lucide-react'

interface ApprovalModalProps {
  id: string
  name: string
  arguments: Record<string, unknown>
  description?: string
  onApprove: (id: string) => void
  onReject: (id: string) => void
  onApproveForSession: (id: string, name: string) => void
  onApproveAll: (id: string) => void
}

function formatArgs(args: Record<string, unknown>): string {
  return Object.entries(args)
    .map(([key, value]) => {
      const strValue = typeof value === 'string' ? value : JSON.stringify(value, null, 2)
      // Show more content - truncate at 500 chars
      const truncated = strValue.length > 500 ? strValue.slice(0, 500) + '...' : strValue
      return `${key}: ${truncated}`
    })
    .join('\n')
}

export default function ApprovalModal({ id, name, arguments: args, description, onApprove, onReject, onApproveForSession, onApproveAll }: ApprovalModalProps) {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'y' || e.key === 'Y' || e.key === 'Enter') {
        e.preventDefault()
        onApprove(id)
      }
      if (e.key === 'n' || e.key === 'N' || e.key === 'Escape') {
        e.preventDefault()
        onReject(id)
      }
      if (e.key === 'a' || e.key === 'A') {
        e.preventDefault()
        onApproveForSession(id, name)
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [id, name, onApprove, onReject, onApproveForSession])

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/50" />

      {/* Modal */}
      <div className="relative w-[600px] max-w-[95vw] max-h-[90vh] bg-card border border-border rounded-xl shadow-2xl flex flex-col">
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-3 border-b border-border">
          <Terminal className="w-4 h-4 text-warning" />
          <h2 className="font-semibold text-sm text-foreground">Tool Approval</h2>
          <span className="font-mono text-sm font-medium text-primary ml-auto">{name}</span>
        </div>

        {/* Content */}
        <div className="px-4 py-3 flex-1 overflow-auto min-h-0 space-y-3">
          {description && (
            <p className="text-sm text-foreground">{description}</p>
          )}
          {Object.keys(args).length > 0 && (
            <pre className="text-xs font-mono text-foreground bg-secondary/50 rounded-lg p-3 max-h-[50vh] overflow-auto whitespace-pre-wrap break-words">
              {formatArgs(args)}
            </pre>
          )}
        </div>

        {/* Actions */}
        <div className="grid grid-cols-4 gap-2 px-4 py-3 border-t border-border">
          <button
            onClick={() => onApprove(id)}
            className="px-3 py-1.5 text-xs font-medium bg-success text-white rounded-lg hover:bg-success/90 transition-colors"
          >
            Allow (Y)
          </button>
          <button
            onClick={() => onReject(id)}
            className="px-3 py-1.5 text-xs font-medium bg-error text-white rounded-lg hover:bg-error/90 transition-colors"
          >
            Deny (N)
          </button>
          <button
            onClick={() => onApproveForSession(id, name)}
            className="px-3 py-1.5 text-xs font-medium bg-secondary text-foreground rounded-lg hover:bg-secondary/80 transition-colors truncate"
            title={`Auto-approve all future "${name}" calls this session`}
          >
            Always (A)
          </button>
          <button
            onClick={() => onApproveAll(id)}
            className="px-3 py-1.5 text-xs font-medium bg-secondary text-foreground rounded-lg hover:bg-secondary/80 transition-colors"
            title="Auto-approve all tools for the rest of this session"
          >
            All tools
          </button>
        </div>
      </div>
    </div>
  )
}
