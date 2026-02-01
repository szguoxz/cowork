import { useState, useEffect, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Settings as SettingsIcon, Save, RefreshCw, Sparkles, ArrowUpCircle, FolderOpen, FileCode } from 'lucide-react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../components/ui/card'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Select } from '../components/ui/select'
import UpdateChecker from '../components/UpdateChecker'

interface Settings {
  provider: {
    provider_type: string
    api_key: string | null
    model: string | null
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

interface ModelInfo {
  id: string
  name: string
  description: string
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)
  const [configPath, setConfigPath] = useState<string | null>(null)
  const [availableModels, setAvailableModels] = useState<ModelInfo[]>([])

  const fetchModels = useCallback(async (providerType: string) => {
    try {
      const models = await invoke<ModelInfo[]>('fetch_provider_models', { providerType })
      setAvailableModels(models)
    } catch (err) {
      console.error('Failed to fetch models:', err)
      setAvailableModels([])
    }
  }, [])

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
      setMessage({
        type: 'success',
        text: 'Settings saved! Note: Active sessions will continue using their original settings. Start a new session to use the updated configuration.'
      })
      setTimeout(() => setMessage(null), 8000)
    } catch (err) {
      setMessage({ type: 'error', text: `Error saving settings: ${err}` })
    } finally {
      setSaving(false)
    }
  }

  const loadConfigPath = async () => {
    try {
      const path = await invoke<string>('get_config_path')
      setConfigPath(path)
    } catch (err) {
      console.error('Failed to load config path:', err)
    }
  }

  const openConfigFolder = async () => {
    try {
      await invoke('open_config_folder')
    } catch (err) {
      setMessage({ type: 'error', text: `Failed to open config folder: ${err}` })
    }
  }

  useEffect(() => {
    loadSettings()
    loadConfigPath()
  }, [])

  // Fetch models when settings load or provider changes
  useEffect(() => {
    if (settings?.provider.provider_type) {
      fetchModels(settings.provider.provider_type)
    }
  }, [settings?.provider.provider_type, fetchModels])

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
                  onChange={async (e) => {
                    const newProvider = e.target.value
                    // Fetch saved config and models for the new provider
                    const [savedConfig, models] = await Promise.all([
                      invoke<{ api_key: string | null; model: string | null; base_url: string | null }>('get_provider_config', { providerType: newProvider }),
                      invoke<ModelInfo[]>('fetch_provider_models', { providerType: newProvider }),
                    ])
                    setAvailableModels(models)
                    setSettings({
                      ...settings,
                      provider: {
                        provider_type: newProvider,
                        // Use saved API key if available, otherwise null
                        api_key: savedConfig.api_key,
                        // Use saved model if available, otherwise first model (balanced)
                        model: savedConfig.model || (models.length > 0 ? models[0].id : null),
                        // Use saved base_url if available
                        base_url: savedConfig.base_url,
                      },
                    })
                  }}
                >
                  <option value="anthropic">Anthropic (Claude)</option>
                  <option value="openai">OpenAI (GPT)</option>
                  <option value="gemini">Google Gemini</option>
                  <option value="cohere">Cohere</option>
                  <option value="groq">Groq</option>
                  <option value="deepseek">DeepSeek</option>
                  <option value="xai">xAI (Grok)</option>
                  <option value="perplexity">Perplexity</option>
                  <option value="together">Together AI</option>
                  <option value="fireworks">Fireworks AI</option>
                  <option value="nebius">Nebius AI</option>
                  <option value="ollama">Ollama (Local)</option>
                </Select>
              </div>

              <div>
                <label className="text-sm font-medium mb-1.5 block text-foreground">Model</label>
                <Select
                  value={settings.provider.model || ''}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      provider: { ...settings.provider, model: e.target.value || null },
                    })
                  }
                >
                  {availableModels.map((model) => (
                    <option key={model.id} value={model.id}>
                      {model.name} - {model.description}
                    </option>
                  ))}
                </Select>
                <p className="mt-1.5 text-xs text-muted-foreground">
                  Select the model tier for this provider.
                </p>
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
              </div>

              <div>
                <label className="text-sm font-medium mb-1.5 block text-foreground">Proxy URL (Optional)</label>
                <Input
                  type="text"
                  value={settings.provider.base_url || ''}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      provider: { ...settings.provider, base_url: e.target.value || null },
                    })
                  }
                  placeholder="Leave empty for default API endpoint"
                />
                <p className="mt-1.5 text-xs text-muted-foreground">
                  Override the default API endpoint with a proxy URL.
                </p>
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

          {/* Updates */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <ArrowUpCircle className="w-5 h-5 text-primary" />
                Updates
              </CardTitle>
              <CardDescription>
                Check for new versions of Cowork
              </CardDescription>
            </CardHeader>
            <CardContent>
              <UpdateChecker />
            </CardContent>
          </Card>

          {/* Advanced Configuration */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <FileCode className="w-5 h-5 text-primary" />
                Advanced Configuration
              </CardTitle>
              <CardDescription>
                Configure MCP servers, skills, and more via config file
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <label className="text-sm font-medium mb-1.5 block text-foreground">Config File</label>
                <div className="flex items-center gap-2">
                  <code className="flex-1 px-3 py-2 text-sm bg-secondary rounded-lg text-muted-foreground overflow-x-auto">
                    {configPath || 'Loading...'}
                  </code>
                  <Button variant="outline" size="sm" onClick={openConfigFolder}>
                    <FolderOpen className="w-4 h-4" />
                  </Button>
                </div>
              </div>

              <div className="text-sm text-muted-foreground space-y-2">
                <p>Edit the config file to configure:</p>
                <ul className="list-disc list-inside space-y-1 ml-2">
                  <li><strong>MCP Servers</strong> - Add tools via Model Context Protocol</li>
                  <li><strong>Skills</strong> - Custom prompt templates in <code className="text-xs bg-secondary px-1 rounded">~/.claude/skills/</code></li>
                  <li><strong>Agents</strong> - Custom subagent definitions</li>
                  <li><strong>Web Search</strong> - Configure fallback search providers</li>
                </ul>
                <p className="mt-3">
                  The config file contains sample configuration with comments.
                </p>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}
