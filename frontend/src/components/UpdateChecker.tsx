import { useState } from 'react'
import { check, Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { RefreshCw, Download, RotateCcw, CheckCircle } from 'lucide-react'
import { Button } from './ui/button'

type UpdateState =
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'up-to-date' }
  | { status: 'available'; update: Update }
  | { status: 'downloading'; progress: number }
  | { status: 'ready' }
  | { status: 'error'; message: string }

export default function UpdateChecker() {
  const [state, setState] = useState<UpdateState>({ status: 'idle' })

  const checkForUpdate = async () => {
    setState({ status: 'checking' })
    try {
      const update = await check()
      if (update) {
        setState({ status: 'available', update })
      } else {
        setState({ status: 'up-to-date' })
      }
    } catch (err) {
      setState({ status: 'error', message: String(err) })
    }
  }

  const downloadAndInstall = async () => {
    if (state.status !== 'available') return
    const { update } = state

    setState({ status: 'downloading', progress: 0 })
    try {
      let totalLen = 0
      let downloaded = 0
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          totalLen = event.data.contentLength ?? 0
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength
          if (totalLen > 0) {
            setState({ status: 'downloading', progress: Math.round((downloaded / totalLen) * 100) })
          }
        } else if (event.event === 'Finished') {
          setState({ status: 'ready' })
        }
      })
      setState({ status: 'ready' })
    } catch (err) {
      setState({ status: 'error', message: String(err) })
    }
  }

  const handleRelaunch = async () => {
    await relaunch()
  }

  return (
    <div className="space-y-3">
      {state.status === 'idle' && (
        <Button onClick={checkForUpdate} variant="outline" className="w-full">
          <RefreshCw className="w-4 h-4 mr-2" />
          Check for Updates
        </Button>
      )}

      {state.status === 'checking' && (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <RefreshCw className="w-4 h-4 animate-spin" />
          Checking for updates...
        </div>
      )}

      {state.status === 'up-to-date' && (
        <div className="flex items-center gap-2 text-sm text-success">
          <CheckCircle className="w-4 h-4" />
          You're on the latest version.
        </div>
      )}

      {state.status === 'available' && (
        <div className="space-y-2">
          <p className="text-sm">
            Version <span className="font-medium text-primary">{state.update.version}</span> is available.
          </p>
          <Button onClick={downloadAndInstall} variant="gradient" className="w-full">
            <Download className="w-4 h-4 mr-2" />
            Download & Install
          </Button>
        </div>
      )}

      {state.status === 'downloading' && (
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Download className="w-4 h-4 animate-pulse" />
            Downloading... {state.progress}%
          </div>
          <div className="w-full h-2 bg-secondary rounded-full overflow-hidden">
            <div
              className="h-full bg-primary transition-all duration-200"
              style={{ width: `${state.progress}%` }}
            />
          </div>
        </div>
      )}

      {state.status === 'ready' && (
        <div className="space-y-2">
          <p className="text-sm text-success">Update installed. Restart to apply.</p>
          <Button onClick={handleRelaunch} variant="gradient" className="w-full">
            <RotateCcw className="w-4 h-4 mr-2" />
            Relaunch Now
          </Button>
        </div>
      )}

      {state.status === 'error' && (
        <div className="space-y-2">
          <p className="text-sm text-error">Error: {state.message}</p>
          <Button onClick={checkForUpdate} variant="outline" size="sm">
            Retry
          </Button>
        </div>
      )}
    </div>
  )
}
