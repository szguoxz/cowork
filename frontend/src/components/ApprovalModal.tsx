import { useEffect } from 'react'
import { Terminal } from 'lucide-react'

interface ApprovalModalProps {
  id: string
  name: string
  arguments: Record<string, unknown>
  onApprove: (id: string) => void
  onReject: (id: string) => void
  onApproveForSession: (id: string, name: string) => void
  onApproveAll: (id: string) => void
}

function formatArgs(args: Record<string, unknown>): string {
  return Object.entries(args)
    .map(([key, value]) => {
      const strValue = typeof value === 'string' ? value : JSON.stringify(value)
      const truncated = strValue.length > 120 ? strValue.slice(0, 120) + '...' : strValue
      return `${key}: ${truncated}`
    })
    .join('\n')
}

export default function ApprovalModal({ id, name, arguments: args, onApprove, onReject, onApproveForSession, onApproveAll }: ApprovalModalProps) {
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
      <div className="relative w-[480px] max-w-[90vw] bg-card border border-border rounded-xl shadow-2xl">
        {/* Header */}
        <div className="flex items-center gap-2 px-5 py-4 border-b border-border">
          <Terminal className="w-5 h-5 text-warning" />
          <h2 className="font-semibold text-foreground">Tool Approval Required</h2>
        </div>

        {/* Content */}
        <div className="px-5 py-4 space-y-3">
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Tool:</span>
            <span className="font-mono text-sm font-medium text-foreground">{name}</span>
          </div>
          {Object.keys(args).length > 0 && (
            <div>
              <span className="text-sm text-muted-foreground">Arguments:</span>
              <pre className="mt-1 text-xs font-mono text-muted-foreground bg-secondary/50 rounded-lg p-3 max-h-48 overflow-auto whitespace-pre-wrap break-all">
                {formatArgs(args)}
              </pre>
            </div>
          )}
        </div>

        {/* Actions */}
        <div className="grid grid-cols-2 gap-2 px-5 py-4 border-t border-border">
          <button
            onClick={() => onApprove(id)}
            className="px-4 py-2 text-sm font-medium bg-success text-white rounded-lg hover:bg-success/90 transition-colors"
          >
            Approve (Y)
          </button>
          <button
            onClick={() => onReject(id)}
            className="px-4 py-2 text-sm font-medium bg-error text-white rounded-lg hover:bg-error/90 transition-colors"
          >
            Reject (N)
          </button>
          <button
            onClick={() => onApproveForSession(id, name)}
            className="px-4 py-2 text-sm font-medium bg-secondary text-foreground rounded-lg hover:bg-secondary/80 transition-colors"
          >
            Always (A)
          </button>
          <button
            onClick={() => onApproveAll(id)}
            className="px-4 py-2 text-sm font-medium bg-secondary text-foreground rounded-lg hover:bg-secondary/80 transition-colors"
          >
            Approve all
          </button>
        </div>
      </div>
    </div>
  )
}
