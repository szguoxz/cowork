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
    <div className="flex flex-col h-full">
      {/* Header */}
      <header className="h-14 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between px-4">
        <div className="flex items-center gap-2">
          <History className="w-5 h-5 text-primary-600" />
          <h1 className="text-lg font-semibold text-gray-900 dark:text-white">
            Session History
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={loadSessions}
            className="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
            title="Refresh"
          >
            <RefreshCw className={`w-4 h-4 ${isLoading ? 'animate-spin' : ''}`} />
          </button>
          <button
            onClick={handleOpenFolder}
            className="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
            title="Open Sessions Folder"
          >
            <FolderOpen className="w-4 h-4" />
          </button>
        </div>
      </header>

      {/* Error Banner */}
      {error && (
        <div className="bg-red-100 dark:bg-red-900/50 border-b border-red-200 dark:border-red-800 px-4 py-2 flex items-center gap-2">
          <AlertTriangle className="w-4 h-4 text-red-500" />
          <span className="text-red-800 dark:text-red-200 text-sm">{error}</span>
        </div>
      )}

      {/* Directory Info */}
      {directoryInfo && (
        <div className="bg-gray-50 dark:bg-gray-800/50 border-b border-gray-200 dark:border-gray-700 px-4 py-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4 text-sm text-gray-600 dark:text-gray-400">
              <div className="flex items-center gap-1">
                <MessageSquare className="w-4 h-4" />
                <span>{directoryInfo.session_count} sessions</span>
              </div>
              <div className="flex items-center gap-1">
                <HardDrive className="w-4 h-4" />
                <span>{formatFileSize(directoryInfo.total_size)}</span>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => handleDeleteOldSessions(30)}
                className="text-xs px-2 py-1 rounded bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors"
              >
                Delete &gt;30 days
              </button>
              <button
                onClick={() => handleDeleteOldSessions(7)}
                className="text-xs px-2 py-1 rounded bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors"
              >
                Delete &gt;7 days
              </button>
              <button
                onClick={() => setShowDeleteAllConfirm(true)}
                className="text-xs px-2 py-1 rounded bg-red-100 text-red-700 dark:bg-red-900/50 dark:text-red-300 hover:bg-red-200 dark:hover:bg-red-900 transition-colors"
              >
                Delete All
              </button>
            </div>
          </div>
          <div className="text-xs text-gray-500 dark:text-gray-500 mt-1 truncate">
            {directoryInfo.path}
          </div>
        </div>
      )}

      {/* Delete All Confirmation */}
      {showDeleteAllConfirm && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-gray-800 rounded-lg p-6 max-w-md mx-4 shadow-xl">
            <h3 className="text-lg font-semibold mb-2">Delete All Sessions?</h3>
            <p className="text-gray-600 dark:text-gray-400 mb-4">
              This will permanently delete all {directoryInfo?.session_count || 0} saved sessions.
              This action cannot be undone.
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setShowDeleteAllConfirm(false)}
                className="px-4 py-2 rounded-lg bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600"
              >
                Cancel
              </button>
              <button
                onClick={handleDeleteAllSessions}
                className="px-4 py-2 rounded-lg bg-red-500 text-white hover:bg-red-600"
              >
                Delete All
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Sessions List */}
      <div className="flex-1 overflow-y-auto p-4">
        {isLoading ? (
          <div className="flex items-center justify-center h-32">
            <RefreshCw className="w-6 h-6 animate-spin text-gray-400" />
          </div>
        ) : sessions.length === 0 ? (
          <div className="text-center text-gray-500 dark:text-gray-400 mt-8">
            <History className="w-12 h-12 mx-auto mb-2 opacity-50" />
            <p>No saved sessions yet</p>
            <p className="text-sm mt-1">
              Sessions are automatically saved as you chat
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {sessions.map((session) => (
              <div
                key={session.id}
                className="border border-gray-200 dark:border-gray-700 rounded-lg hover:border-primary-300 dark:hover:border-primary-700 transition-colors"
              >
                <div className="p-4">
                  <div className="flex items-start justify-between">
                    <div className="flex-1 min-w-0">
                      <h3 className="font-medium text-gray-900 dark:text-white truncate">
                        {session.title || 'Untitled Session'}
                      </h3>
                      <div className="flex items-center gap-3 mt-1 text-sm text-gray-500 dark:text-gray-400">
                        <div className="flex items-center gap-1">
                          <MessageSquare className="w-3 h-3" />
                          <span>{session.message_count} messages</span>
                        </div>
                        <div className="flex items-center gap-1">
                          <Clock className="w-3 h-3" />
                          <span>{formatRelativeTime(session.updated_at)}</span>
                        </div>
                        <span className="px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-xs">
                          {session.provider_type}
                        </span>
                      </div>
                      <div className="text-xs text-gray-400 dark:text-gray-500 mt-1">
                        <Calendar className="w-3 h-3 inline mr-1" />
                        {formatDate(session.created_at)} | {formatFileSize(session.file_size)}
                      </div>
                    </div>
                    <div className="flex items-center gap-2 ml-4">
                      <button
                        onClick={() => handleLoadSession(session.id)}
                        className="p-2 rounded-lg bg-primary-100 text-primary-700 dark:bg-primary-900/50 dark:text-primary-300 hover:bg-primary-200 dark:hover:bg-primary-900 transition-colors"
                        title="Load Session"
                      >
                        <Play className="w-4 h-4" />
                      </button>
                      {deleteConfirm === session.id ? (
                        <div className="flex items-center gap-1">
                          <button
                            onClick={() => handleDeleteSession(session.id)}
                            className="px-2 py-1 rounded bg-red-500 text-white text-xs hover:bg-red-600"
                          >
                            Confirm
                          </button>
                          <button
                            onClick={() => setDeleteConfirm(null)}
                            className="px-2 py-1 rounded bg-gray-200 dark:bg-gray-700 text-xs hover:bg-gray-300 dark:hover:bg-gray-600"
                          >
                            Cancel
                          </button>
                        </div>
                      ) : (
                        <button
                          onClick={() => setDeleteConfirm(session.id)}
                          className="p-2 rounded-lg text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30 transition-colors"
                          title="Delete Session"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
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
