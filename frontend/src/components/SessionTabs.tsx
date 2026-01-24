import { Plus, X, MessageSquare, Loader2 } from 'lucide-react'
import type { Session } from '../bindings'

interface SessionTabsProps {
  sessions: Map<string, Session>
  activeId: string | null
  onSelect: (id: string) => void
  onNew: () => void
  onClose: (id: string) => void
}

export default function SessionTabs({ sessions, activeId, onSelect, onNew, onClose }: SessionTabsProps) {
  const sessionList = Array.from(sessions.entries())

  return (
    <div className="flex items-center gap-1 px-2 py-1.5 bg-card/50 border-b border-border overflow-x-auto">
      {sessionList.map(([id, session]) => (
        <div
          key={id}
          className={`
            group flex items-center gap-2 px-3 py-1.5 rounded-lg cursor-pointer
            transition-all duration-200 min-w-0 max-w-[200px]
            ${id === activeId
              ? 'bg-primary/10 text-primary border border-primary/20'
              : 'text-muted-foreground hover:text-foreground hover:bg-secondary/50'
            }
          `}
          onClick={() => onSelect(id)}
        >
          {session.status === '' ? (
            <MessageSquare className="w-3.5 h-3.5 shrink-0" />
          ) : (
            <Loader2 className="w-3.5 h-3.5 shrink-0 animate-spin" />
          )}
          <span className="text-sm truncate">{session.name}</span>
          {session.messages.length > 0 && (
            <span className="text-xs text-muted-foreground/70">
              ({session.messages.length})
            </span>
          )}
          {sessionList.length > 1 && (
            <button
              onClick={(e) => {
                e.stopPropagation()
                onClose(id)
              }}
              className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-error/20 hover:text-error transition-all"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>
      ))}

      <button
        onClick={onNew}
        className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-muted-foreground hover:text-primary hover:bg-primary/10 transition-all border border-transparent hover:border-primary/20"
        title="New Chat"
      >
        <Plus className="w-4 h-4" />
        <span className="text-sm">New Chat</span>
      </button>
    </div>
  )
}
