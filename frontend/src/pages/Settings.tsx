import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Save, RefreshCw } from 'lucide-react'

interface Settings {
  provider: {
    provider_type: string
    api_key: string | null
    model: string
    base_url: string | null
  }
  approval: {
    auto_approve_level: string
    show_confirmation_dialogs: boolean
  }
  ui: {
    theme: string
    font_size: number
    show_tool_calls: boolean
  }
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [message, setMessage] = useState<string | null>(null)

  const loadSettings = async () => {
    setLoading(true)
    try {
      const result = await invoke<Settings>('get_settings')
      setSettings(result)
    } catch (err) {
      setMessage(`Error loading settings: ${err}`)
    } finally {
      setLoading(false)
    }
  }

  const saveSettings = async () => {
    if (!settings) return
    setSaving(true)
    try {
      await invoke('update_settings', { settings })
      await invoke('save_settings')
      setMessage('Settings saved successfully!')
      setTimeout(() => setMessage(null), 3000)
    } catch (err) {
      setMessage(`Error saving settings: ${err}`)
    } finally {
      setSaving(false)
    }
  }

  useEffect(() => {
    loadSettings()
  }, [])

  if (loading || !settings) {
    return (
      <div className="flex items-center justify-center h-full">
        <RefreshCw className="w-8 h-8 animate-spin text-gray-400" />
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header className="h-14 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between px-4">
        <h1 className="text-lg font-semibold text-gray-900 dark:text-white">
          Settings
        </h1>
        <button
          onClick={saveSettings}
          disabled={saving}
          className="
            flex items-center gap-2 px-4 py-2 rounded-lg
            bg-primary-600 text-white hover:bg-primary-700
            disabled:opacity-50 transition-colors
          "
        >
          <Save className="w-4 h-4" />
          Save
        </button>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-6">
        {message && (
          <div className={`
            mb-4 p-3 rounded-lg
            ${message.includes('Error')
              ? 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
              : 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
            }
          `}>
            {message}
          </div>
        )}

        <div className="space-y-8 max-w-2xl">
          {/* Provider Settings */}
          <section>
            <h2 className="text-sm font-semibold text-gray-500 uppercase mb-4">
              LLM Provider
            </h2>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  Provider
                </label>
                <select
                  value={settings.provider.provider_type}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      provider: { ...settings.provider, provider_type: e.target.value },
                    })
                  }
                  className="
                    w-full rounded-lg border border-gray-300 dark:border-gray-600
                    bg-white dark:bg-gray-800 px-3 py-2
                    text-gray-900 dark:text-white
                  "
                >
                  <option value="anthropic">Anthropic (Claude)</option>
                  <option value="openai">OpenAI (GPT)</option>
                  <option value="gemini">Google Gemini</option>
                  <option value="cohere">Cohere</option>
                  <option value="groq">Groq</option>
                  <option value="deepseek">DeepSeek</option>
                  <option value="xai">xAI (Grok)</option>
                  <option value="ollama">Ollama (Local)</option>
                </select>
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  API Key
                </label>
                <input
                  type="password"
                  value={settings.provider.api_key || ''}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      provider: { ...settings.provider, api_key: e.target.value || null },
                    })
                  }
                  className="
                    w-full rounded-lg border border-gray-300 dark:border-gray-600
                    bg-white dark:bg-gray-800 px-3 py-2
                    text-gray-900 dark:text-white
                  "
                  placeholder="Enter your API key"
                />
                <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                  You can also set ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable.
                </p>
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  Model
                </label>
                <input
                  type="text"
                  value={settings.provider.model}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      provider: { ...settings.provider, model: e.target.value },
                    })
                  }
                  className="
                    w-full rounded-lg border border-gray-300 dark:border-gray-600
                    bg-white dark:bg-gray-800 px-3 py-2
                    text-gray-900 dark:text-white
                  "
                />
              </div>
            </div>
          </section>

          {/* Approval Settings */}
          <section>
            <h2 className="text-sm font-semibold text-gray-500 uppercase mb-4">
              Approval Policy
            </h2>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  Auto-approve Level
                </label>
                <select
                  value={settings.approval.auto_approve_level}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      approval: { ...settings.approval, auto_approve_level: e.target.value },
                    })
                  }
                  className="
                    w-full rounded-lg border border-gray-300 dark:border-gray-600
                    bg-white dark:bg-gray-800 px-3 py-2
                    text-gray-900 dark:text-white
                  "
                >
                  <option value="none">None (Ask for everything)</option>
                  <option value="low">Low (Auto-approve reads)</option>
                  <option value="medium">Medium (Auto-approve safe operations)</option>
                  <option value="high">High (Auto-approve most operations)</option>
                </select>
              </div>

              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={settings.approval.show_confirmation_dialogs}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      approval: {
                        ...settings.approval,
                        show_confirmation_dialogs: e.target.checked,
                      },
                    })
                  }
                  className="rounded border-gray-300"
                />
                <span className="text-sm text-gray-700 dark:text-gray-300">
                  Show confirmation dialogs
                </span>
              </label>
            </div>
          </section>

          {/* UI Settings */}
          <section>
            <h2 className="text-sm font-semibold text-gray-500 uppercase mb-4">
              User Interface
            </h2>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  Theme
                </label>
                <select
                  value={settings.ui.theme}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      ui: { ...settings.ui, theme: e.target.value },
                    })
                  }
                  className="
                    w-full rounded-lg border border-gray-300 dark:border-gray-600
                    bg-white dark:bg-gray-800 px-3 py-2
                    text-gray-900 dark:text-white
                  "
                >
                  <option value="system">System</option>
                  <option value="light">Light</option>
                  <option value="dark">Dark</option>
                </select>
              </div>

              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={settings.ui.show_tool_calls}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      ui: { ...settings.ui, show_tool_calls: e.target.checked },
                    })
                  }
                  className="rounded border-gray-300"
                />
                <span className="text-sm text-gray-700 dark:text-gray-300">
                  Show tool calls in chat
                </span>
              </label>
            </div>
          </section>
        </div>
      </div>
    </div>
  )
}
