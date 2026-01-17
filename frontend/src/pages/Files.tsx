import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { File, Folder, ChevronRight, RefreshCw } from 'lucide-react'

interface FileEntry {
  name: string
  path: string
  is_dir: boolean
  size: number | null
  modified: string | null
}

export default function Files() {
  const [entries, setEntries] = useState<FileEntry[]>([])
  const [currentPath, setCurrentPath] = useState('.')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadFiles = async (path: string) => {
    setLoading(true)
    setError(null)
    try {
      const result = await invoke<FileEntry[]>('list_files', { path })
      setEntries(result.sort((a, b) => {
        // Directories first
        if (a.is_dir && !b.is_dir) return -1
        if (!a.is_dir && b.is_dir) return 1
        return a.name.localeCompare(b.name)
      }))
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadFiles(currentPath)
  }, [currentPath])

  const handleEntryClick = (entry: FileEntry) => {
    if (entry.is_dir) {
      setCurrentPath(entry.path)
    }
  }

  const formatSize = (size: number | null) => {
    if (size === null) return '-'
    if (size < 1024) return `${size} B`
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
    return `${(size / (1024 * 1024)).toFixed(1)} MB`
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header className="h-14 border-b border-gray-200 dark:border-gray-700 flex items-center px-4 gap-4">
        <h1 className="text-lg font-semibold text-gray-900 dark:text-white">
          Files
        </h1>
        <div className="flex-1 flex items-center gap-1 text-sm text-gray-500">
          {currentPath.split('/').map((part, i, arr) => (
            <span key={i} className="flex items-center">
              {i > 0 && <ChevronRight className="w-4 h-4 mx-1" />}
              <span className="hover:text-primary-600 cursor-pointer">
                {part || 'root'}
              </span>
            </span>
          ))}
        </div>
        <button
          onClick={() => loadFiles(currentPath)}
          className="p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg"
          disabled={loading}
        >
          <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </header>

      {/* File list */}
      <div className="flex-1 overflow-y-auto">
        {error && (
          <div className="p-4 text-red-600 dark:text-red-400">
            Error: {error}
          </div>
        )}

        {!error && entries.length === 0 && !loading && (
          <div className="p-4 text-gray-500 text-center">
            No files found
          </div>
        )}

        <table className="w-full">
          <thead className="bg-gray-50 dark:bg-gray-800 sticky top-0">
            <tr className="text-left text-xs text-gray-500 uppercase">
              <th className="px-4 py-2">Name</th>
              <th className="px-4 py-2 w-24">Size</th>
              <th className="px-4 py-2 w-40">Modified</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-100 dark:divide-gray-800">
            {entries.map((entry) => (
              <tr
                key={entry.path}
                onClick={() => handleEntryClick(entry)}
                className="
                  hover:bg-gray-50 dark:hover:bg-gray-800 cursor-pointer
                  text-gray-900 dark:text-white
                "
              >
                <td className="px-4 py-2 flex items-center gap-2">
                  {entry.is_dir ? (
                    <Folder className="w-4 h-4 text-primary-500" />
                  ) : (
                    <File className="w-4 h-4 text-gray-400" />
                  )}
                  {entry.name}
                </td>
                <td className="px-4 py-2 text-sm text-gray-500">
                  {entry.is_dir ? '-' : formatSize(entry.size)}
                </td>
                <td className="px-4 py-2 text-sm text-gray-500">
                  {entry.modified || '-'}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
