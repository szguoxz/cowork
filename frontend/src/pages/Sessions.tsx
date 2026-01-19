import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  History,
  Trash2,
  FolderOpen,
  RefreshCw,
  Calendar,
  MessageSquare,
  HardDrive,
  AlertTriangle,
  Play,
  Clock,
} from 'lucide-react'
import { Button } from '../components/ui/button'

interface SavedSession {
  id: string
  title: string | null
  message_count: number
  provider_type: string
  created_at: string
  updated_at: string
  file_size: number
}

interface SessionsDirectoryInfo {
  path: string
  session_count: number
  total_size: number
}

export default function Sessions() {
  const [sessions, setSessions] = useState<SavedSession[]>([])
  const [directoryInfo, setDirectoryInfo] = useState<SessionsDirectoryInfo | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null)
  const [showDeleteAllConfirm, setShowDeleteAllConfirm] = useState(false)

  const loadSessions = useCallback(async () => {
    setIsLoading(true)
    setError(null)
    try {
      const [sessionList, dirInfo] = await Promise.all([
        invoke<SavedSession[]>('list_saved_sessions'),
        invoke<SessionsDirectoryInfo>('get_sessions_directory_info'),
      ])
      setSessions(sessionList)
      setDirectoryInfo(dirInfo)
    } catch (err) {
      setError(String(err))
    } finally {
      setIsLoading(false)
    }
  }, [])

  useEffect(() => {
    loadSessions()
  }, [loadSessions])

  const handleLoadSession = async (sessionId: string) => {
    try {
      await invoke('load_saved_session', { savedSessionId: sessionId })
      // Navigate to chat page (could use react-router navigate here)
      window.location.href = '/'
    } catch (err) {
      setError(String(err))
    }
  }

  const handleDeleteSession = async (sessionId: string) => {
    try {
      await invoke('delete_saved_session', { savedSessionId: sessionId })
      setDeleteConfirm(null)
      loadSessions()
    } catch (err) {
      setError(String(err))
    }
  }

  const handleDeleteOldSessions = async (days: number) => {
    try {
      const deleted = await invoke<string[]>('delete_old_sessions', { days })
      setError(null)
      loadSessions()
      if (deleted.length > 0) {
        // Show success message briefly
        setError(`Deleted ${deleted.length} session(s) older than ${days} days`)
        setTimeout(() => setError(null), 3000)
      }
    } catch (err) {
      setError(String(err))
    }
  }

  const handleDeleteAllSessions = async () => {
    try {
      const count = await invoke<number>('delete_all_saved_sessions')
      setShowDeleteAllConfirm(false)
      loadSessions()
      if (count > 0) {
        setError(`Deleted ${count} session(s)`)
        setTimeout(() => setError(null), 3000)
      }
    } catch (err) {
      setError(String(err))
    }
  }

  const handleOpenFolder = async () => {
    try {
      await invoke('open_sessions_folder')
    } catch (err) {
      setError(String(err))
    }
  }

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleString()
  }

  const formatFileSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  const formatRelativeTime = (dateStr: string) => {
    const date = new Date(dateStr)
    const now = new Date()
    const diff = now.getTime() - date.getTime()
    const days = Math.floor(diff / (1000 * 60 * 60 * 24))
    const hours = Math.floor(diff / (1000 * 60 * 60))
    const minutes = Math.floor(diff / (1000 * 60))

    if (days > 0) return `${days}d ago`
    if (hours > 0) return `${hours}h ago`
    if (minutes > 0) return `${minutes}m ago`
    return 'just now'
  }

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <header className="h-14 border-b border-border flex items-center justify-between px-4 bg-card/50">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-violet-500 to-purple-600 flex items-center justify-center shadow-glow-sm">
            <History className="w-4 h-4 text-white" />
          </div>
          <h1 className="text-lg font-semibold text-foreground">
            Session History
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <Button
            onClick={loadSessions}
            variant="ghost"
            size="icon"
            title="Refresh"
          >
            <RefreshCw className={`w-4 h-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            onClick={handleOpenFolder}
            variant="ghost"
            size="icon"
            title="Open Sessions Folder"
          >
            <FolderOpen className="w-4 h-4" />
          </Button>
        </div>
      </header>

      {/* Error Banner */}
      {error && (
        <div className="bg-error/10 border-b border-error/20 px-4 py-2 flex items-center gap-2">
          <AlertTriangle className="w-4 h-4 text-error" />
          <span className="text-error text-sm">{error}</span>
        </div>
      )}

      {/* Directory Info */}
      {directoryInfo && (
        <div className="bg-card/50 border-b border-border px-4 py-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4 text-sm text-muted-foreground">
              <div className="flex items-center gap-1.5">
                <MessageSquare className="w-4 h-4" />
                <span>{directoryInfo.session_count} sessions</span>
              </div>
              <div className="flex items-center gap-1.5">
                <HardDrive className="w-4 h-4" />
                <span>{formatFileSize(directoryInfo.total_size)}</span>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Button
                onClick={() => handleDeleteOldSessions(30)}
                variant="outline"
                size="sm"
              >
                Delete &gt;30 days
              </Button>
              <Button
                onClick={() => handleDeleteOldSessions(7)}
                variant="outline"
                size="sm"
              >
                Delete &gt;7 days
              </Button>
              <Button
                onClick={() => setShowDeleteAllConfirm(true)}
                variant="destructive"
                size="sm"
              >
                Delete All
              </Button>
            </div>
          </div>
          <div className="text-xs text-muted-foreground/70 mt-1 truncate font-mono">
            {directoryInfo.path}
          </div>
        </div>
      )}

      {/* Delete All Confirmation */}
      {showDeleteAllConfirm && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50">
          <div className="bg-card border border-border rounded-2xl p-6 max-w-md mx-4 shadow-2xl animate-scale-in">
            <h3 className="text-lg font-semibold mb-2 text-foreground">Delete All Sessions?</h3>
            <p className="text-muted-foreground mb-4">
              This will permanently delete all {directoryInfo?.session_count || 0} saved sessions.
              This action cannot be undone.
            </p>
            <div className="flex justify-end gap-2">
              <Button
                onClick={() => setShowDeleteAllConfirm(false)}
                variant="outline"
              >
                Cancel
              </Button>
              <Button
                onClick={handleDeleteAllSessions}
                variant="destructive"
              >
                Delete All
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Sessions List */}
      <div className="flex-1 overflow-y-auto p-4">
        {isLoading ? (
          <div className="flex items-center justify-center h-32">
            <RefreshCw className="w-6 h-6 animate-spin text-primary" />
          </div>
        ) : sessions.length === 0 ? (
          <div className="text-center mt-16">
            <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-violet-500/20 to-purple-600/20 flex items-center justify-center mx-auto mb-4 border border-primary/20">
              <History className="w-8 h-8 text-primary" />
            </div>
            <p className="text-lg font-medium text-foreground">No saved sessions yet</p>
            <p className="text-sm mt-2 text-muted-foreground">
              Sessions are automatically saved as you chat
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            {sessions.map((session) => (
              <div
                key={session.id}
                className="border border-border rounded-xl hover:border-primary/30 hover:shadow-glow-sm transition-all duration-200 bg-card group"
              >
                <div className="p-4">
                  <div className="flex items-start justify-between">
                    <div className="flex-1 min-w-0">
                      <h3 className="font-medium text-foreground truncate group-hover:text-primary transition-colors">
                        {session.title || 'Untitled Session'}
                      </h3>
                      <div className="flex items-center gap-3 mt-1.5 text-sm text-muted-foreground">
                        <div className="flex items-center gap-1.5">
                          <MessageSquare className="w-3.5 h-3.5" />
                          <span>{session.message_count} messages</span>
                        </div>
                        <div className="flex items-center gap-1.5">
                          <Clock className="w-3.5 h-3.5" />
                          <span>{formatRelativeTime(session.updated_at)}</span>
                        </div>
                        <span className="px-2 py-0.5 rounded-full bg-secondary text-xs font-medium">
                          {session.provider_type}
                        </span>
                      </div>
                      <div className="text-xs text-muted-foreground/70 mt-1.5 flex items-center gap-1">
                        <Calendar className="w-3 h-3" />
                        {formatDate(session.created_at)} | {formatFileSize(session.file_size)}
                      </div>
                    </div>
                    <div className="flex items-center gap-2 ml-4">
                      <Button
                        onClick={() => handleLoadSession(session.id)}
                        variant="default"
                        size="sm"
                        className="shadow-glow-sm"
                      >
                        <Play className="w-4 h-4" />
                        Load
                      </Button>
                      {deleteConfirm === session.id ? (
                        <div className="flex items-center gap-1">
                          <Button
                            onClick={() => handleDeleteSession(session.id)}
                            variant="destructive"
                            size="sm"
                          >
                            Confirm
                          </Button>
                          <Button
                            onClick={() => setDeleteConfirm(null)}
                            variant="outline"
                            size="sm"
                          >
                            Cancel
                          </Button>
                        </div>
                      ) : (
                        <Button
                          onClick={() => setDeleteConfirm(session.id)}
                          variant="ghost"
                          size="icon"
                          className="text-muted-foreground hover:text-error"
                        >
                          <Trash2 className="w-4 h-4" />
                        </Button>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
