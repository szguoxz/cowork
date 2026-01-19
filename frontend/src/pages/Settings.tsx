import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Settings as SettingsIcon, Save, RefreshCw, Sparkles } from 'lucide-react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../components/ui/card'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Select } from '../components/ui/select'

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
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)

  const loadSettings = async () => {
    setLoading(true)
    try {
      const result = await invoke<Settings>('get_settings')
      setSettings(result)
    } catch (err) {
      setMessage({ type: 'error', text: `Error loading settings: ${err}` })
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
      setMessage({ type: 'success', text: 'Settings saved successfully!' })
      setTimeout(() => setMessage(null), 3000)
    } catch (err) {
      setMessage({ type: 'error', text: `Error saving settings: ${err}` })
    } finally {
      setSaving(false)
    }
  }

  useEffect(() => {
    loadSettings()
  }, [])

  if (loading || !settings) {
    return (
      <div className="flex items-center justify-center h-full bg-background">
        <RefreshCw className="w-8 h-8 animate-spin text-primary" />
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <header className="h-14 border-b border-border flex items-center justify-between px-6 bg-card/50">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-violet-500 to-purple-600 flex items-center justify-center shadow-glow-sm">
            <SettingsIcon className="w-4 h-4 text-white" />
          </div>
          <h1 className="text-lg font-semibold text-foreground">Settings</h1>
        </div>
        <Button onClick={saveSettings} disabled={saving} variant="gradient">
          {saving ? (
            <RefreshCw className="w-4 h-4 animate-spin" />
          ) : (
            <Save className="w-4 h-4" />
          )}
          Save Changes
        </Button>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-2xl mx-auto space-y-6">
          {/* Message */}
          {message && (
            <div className={`
              p-4 rounded-xl animate-in
              ${message.type === 'error'
                ? 'bg-error/10 text-error border border-error/20'
                : 'bg-success/10 text-success border border-success/20'
              }
            `}>
              {message.text}
            </div>
          )}

          {/* Provider Settings */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Sparkles className="w-5 h-5 text-primary" />
                LLM Provider
              </CardTitle>
              <CardDescription>
                Configure your AI provider and model settings
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <label className="text-sm font-medium mb-1.5 block text-foreground">Provider</label>
                <Select
                  value={settings.provider.provider_type}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      provider: { ...settings.provider, provider_type: e.target.value },
                    })
                  }
                >
                  <option value="anthropic">Anthropic (Claude)</option>
                  <option value="openai">OpenAI (GPT)</option>
                  <option value="gemini">Google Gemini</option>
                  <option value="cohere">Cohere</option>
                  <option value="groq">Groq</option>
                  <option value="deepseek">DeepSeek</option>
                  <option value="xai">xAI (Grok)</option>
                  <option value="ollama">Ollama (Local)</option>
                </Select>
              </div>

              <div>
                <label className="text-sm font-medium mb-1.5 block text-foreground">API Key</label>
                <Input
                  type="password"
                  value={settings.provider.api_key || ''}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      provider: { ...settings.provider, api_key: e.target.value || null },
                    })
                  }
                  placeholder="Enter your API key"
                />
                <p className="mt-1.5 text-xs text-muted-foreground">
                  You can also set ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable.
                </p>
              </div>

              <div>
                <label className="text-sm font-medium mb-1.5 block text-foreground">Model</label>
                <Input
                  type="text"
                  value={settings.provider.model}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      provider: { ...settings.provider, model: e.target.value },
                    })
                  }
                  placeholder="e.g., claude-sonnet-4-20250514"
                />
              </div>
            </CardContent>
          </Card>

          {/* Approval Settings */}
          <Card>
            <CardHeader>
              <CardTitle>Approval Policy</CardTitle>
              <CardDescription>
                Control how tool calls are approved
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <label className="text-sm font-medium mb-1.5 block text-foreground">Auto-approve Level</label>
                <Select
                  value={settings.approval.auto_approve_level}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      approval: { ...settings.approval, auto_approve_level: e.target.value },
                    })
                  }
                >
                  <option value="none">None (Ask for everything)</option>
                  <option value="low">Low (Auto-approve reads)</option>
                  <option value="medium">Medium (Auto-approve safe operations)</option>
                  <option value="high">High (Auto-approve most operations)</option>
                </Select>
              </div>

              <label className="flex items-center gap-3 cursor-pointer group">
                <div className="relative">
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
                    className="w-5 h-5 rounded-md border-border bg-secondary checked:bg-primary checked:border-primary transition-colors"
                  />
                </div>
                <span className="text-sm text-foreground group-hover:text-primary transition-colors">
                  Show confirmation dialogs
                </span>
              </label>
            </CardContent>
          </Card>

          {/* UI Settings */}
          <Card>
            <CardHeader>
              <CardTitle>User Interface</CardTitle>
              <CardDescription>
                Customize the appearance
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <label className="text-sm font-medium mb-1.5 block text-foreground">Theme</label>
                <Select
                  value={settings.ui.theme}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      ui: { ...settings.ui, theme: e.target.value },
                    })
                  }
                >
                  <option value="system">System</option>
                  <option value="light">Light</option>
                  <option value="dark">Dark</option>
                </Select>
              </div>

              <label className="flex items-center gap-3 cursor-pointer group">
                <div className="relative">
                  <input
                    type="checkbox"
                    checked={settings.ui.show_tool_calls}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        ui: { ...settings.ui, show_tool_calls: e.target.checked },
                      })
                    }
                    className="w-5 h-5 rounded-md border-border bg-secondary checked:bg-primary checked:border-primary transition-colors"
                  />
                </div>
                <span className="text-sm text-foreground group-hover:text-primary transition-colors">
                  Show tool calls in chat
                </span>
              </label>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}
