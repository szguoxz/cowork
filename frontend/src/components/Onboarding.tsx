import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  Sparkles,
  CheckCircle2,
  ArrowRight,
  AlertCircle,
  Loader2,
  ChevronLeft,
} from 'lucide-react'

interface OnboardingProps {
  onComplete: () => void
}

type Step = 'provider' | 'apikey' | 'model' | 'testing'

interface ProviderOption {
  id: string
  name: string
  envVar: string
}

const PROVIDERS: ProviderOption[] = [
  { id: 'anthropic', name: 'Anthropic (Claude)', envVar: 'ANTHROPIC_API_KEY' },
  { id: 'openai', name: 'OpenAI (GPT-4)', envVar: 'OPENAI_API_KEY' },
  { id: 'deepseek', name: 'DeepSeek', envVar: 'DEEPSEEK_API_KEY' },
  { id: 'gemini', name: 'Google Gemini', envVar: 'GEMINI_API_KEY' },
  { id: 'groq', name: 'Groq', envVar: 'GROQ_API_KEY' },
  { id: 'xai', name: 'xAI (Grok)', envVar: 'XAI_API_KEY' },
  { id: 'together', name: 'Together AI', envVar: 'TOGETHER_API_KEY' },
  { id: 'fireworks', name: 'Fireworks AI', envVar: 'FIREWORKS_API_KEY' },
  { id: 'ollama', name: 'Ollama (Local)', envVar: '' },
  { id: 'nebius', name: 'Nebius AI', envVar: 'NEBIUS_API_KEY' },
  { id: 'zai', name: 'Zhipu AI', envVar: 'ZAI_API_KEY' },
  { id: 'bigmodel', name: 'BigModel.cn', envVar: 'BIGMODEL_API_KEY' },
  { id: 'mimo', name: 'MIMO (Xiaomi)', envVar: 'MIMO_API_KEY' },
]

interface ApiTestResult {
  success: boolean
  message: string
}

interface ModelInfo {
  id: string
  name: string | null
  description: string | null
  recommended: boolean
}

export default function Onboarding({ onComplete }: OnboardingProps) {
  const [step, setStep] = useState<Step>('provider')
  const [selectedProvider, setSelectedProvider] = useState<string>('')
  const [apiKey, setApiKey] = useState('')
  const [models, setModels] = useState<ModelInfo[]>([])
  const [selectedModel, setSelectedModel] = useState<string>('')
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [testResult, setTestResult] = useState<ApiTestResult | null>(null)

  const selectedProviderInfo = PROVIDERS.find((p) => p.id === selectedProvider)

  // Auto-complete after successful test
  useEffect(() => {
    if (testResult?.success) {
      const timer = setTimeout(() => {
        localStorage.setItem('onboarding_complete', 'true')
        onComplete()
      }, 1500)
      return () => clearTimeout(timer)
    }
  }, [testResult, onComplete])

  const handleProviderSelect = (providerId: string) => {
    setSelectedProvider(providerId)
    setError(null)
    // Auto-advance after selection
    setTimeout(() => {
      if (providerId === 'ollama') {
        setApiKey('')
        setStep('model')
        fetchModelsForProvider(providerId, '')
      } else {
        setStep('apikey')
      }
    }, 150)
  }

  const fetchModelsForProvider = async (provider: string, key: string) => {
    setIsLoading(true)
    try {
      const fetchedModels = await invoke<ModelInfo[]>('fetch_provider_models', {
        providerType: provider,
        apiKey: key,
      })
      setModels(fetchedModels)
      const recommended = fetchedModels.find((m) => m.recommended)
      setSelectedModel(recommended?.id || fetchedModels[0]?.id || '')
    } catch {
      setModels([])
      setSelectedModel('')
    } finally {
      setIsLoading(false)
    }
  }

  const handleApiKeySubmit = async () => {
    if (!apiKey.trim()) {
      setError('Please enter an API key')
      return
    }
    setError(null)
    setStep('model')
    await fetchModelsForProvider(selectedProvider, apiKey)
  }

  const handleModelSubmit = async () => {
    if (!selectedModel && models.length > 0) {
      setError('Please select a model')
      return
    }
    setError(null)
    setStep('testing')
    setIsLoading(true)
    setTestResult(null)

    try {
      const result = await invoke<ApiTestResult>('test_api_connection', {
        providerType: selectedProvider,
        apiKey: apiKey || null,
        model: selectedModel || null,
      })
      setTestResult(result)

      if (result.success) {
        await invoke('update_settings', {
          settings: {
            provider: {
              provider_type: selectedProvider,
              api_key: apiKey || null,
              model: selectedModel,
              base_url: null,
            },
            approval: { auto_approve_level: 'low', show_confirmation_dialogs: true },
            ui: { theme: 'system', font_size: 14, show_tool_calls: true },
          },
        })
        await invoke('save_settings')
      }
    } catch (err) {
      setTestResult({ success: false, message: String(err) })
    } finally {
      setIsLoading(false)
    }
  }

  const handleBack = () => {
    setError(null)
    setTestResult(null)
    if (step === 'apikey') setStep('provider')
    else if (step === 'model') setStep(selectedProvider === 'ollama' ? 'provider' : 'apikey')
    else if (step === 'testing') setStep('model')
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !isLoading) {
      if (step === 'apikey') handleApiKeySubmit()
      else if (step === 'model' && selectedModel) handleModelSubmit()
    }
  }

  const progress = step === 'provider' ? 25 : step === 'apikey' ? 50 : step === 'model' ? 75 : 100

  return (
    <div className="fixed inset-0 bg-gray-900/90 backdrop-blur-sm flex items-center justify-center z-50 p-4">
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-2xl w-full max-w-md overflow-hidden">
        {/* Progress bar */}
        <div className="h-1 bg-gray-200 dark:bg-gray-700">
          <div
            className="h-full bg-primary-500 transition-all duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>

        {/* Header */}
        <div className="px-5 pt-4 pb-2 flex items-center gap-3">
          {step !== 'provider' && (
            <button
              onClick={handleBack}
              className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors"
            >
              <ChevronLeft className="w-5 h-5 text-gray-500" />
            </button>
          )}
          <div className="flex items-center gap-2">
            <Sparkles className="w-5 h-5 text-primary-500" />
            <span className="font-semibold text-gray-900 dark:text-white">Cowork Setup</span>
          </div>
        </div>

        {/* Content */}
        <div className="px-5 pb-5" onKeyDown={handleKeyDown}>
          {/* Provider Selection */}
          {step === 'provider' && (
            <div>
              <p className="text-sm text-gray-600 dark:text-gray-400 mb-3">
                Choose your AI provider:
              </p>
              <div className="grid grid-cols-2 gap-2 max-h-[320px] overflow-y-auto pr-1">
                {PROVIDERS.map((provider) => (
                  <button
                    key={provider.id}
                    onClick={() => handleProviderSelect(provider.id)}
                    className={`p-2.5 rounded-lg border text-left text-sm transition-all ${
                      selectedProvider === provider.id
                        ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/30 ring-1 ring-primary-500'
                        : 'border-gray-200 dark:border-gray-700 hover:border-primary-300 dark:hover:border-primary-700'
                    }`}
                  >
                    <span className="font-medium text-gray-900 dark:text-white block truncate">
                      {provider.name}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* API Key Input */}
          {step === 'apikey' && (
            <div>
              <p className="text-sm text-gray-600 dark:text-gray-400 mb-3">
                Enter your {selectedProviderInfo?.name} API key:
              </p>
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={selectedProviderInfo?.envVar || 'API Key'}
                autoFocus
                className="w-full px-3 py-2.5 rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-primary-500 focus:border-transparent"
              />
              {error && (
                <p className="mt-2 text-sm text-red-500 flex items-center gap-1">
                  <AlertCircle className="w-4 h-4" /> {error}
                </p>
              )}
              <p className="mt-2 text-xs text-gray-500">
                Stored locally. Or set {selectedProviderInfo?.envVar} env var.
              </p>
              <button
                onClick={handleApiKeySubmit}
                disabled={isLoading}
                className="w-full mt-4 py-2.5 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg font-medium text-sm flex items-center justify-center gap-2 transition-colors"
              >
                {isLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : 'Continue'}
                {!isLoading && <ArrowRight className="w-4 h-4" />}
              </button>
            </div>
          )}

          {/* Model Selection */}
          {step === 'model' && (
            <div>
              <p className="text-sm text-gray-600 dark:text-gray-400 mb-3">Select a model:</p>
              {isLoading ? (
                <div className="py-8 flex flex-col items-center">
                  <Loader2 className="w-6 h-6 text-primary-500 animate-spin" />
                  <p className="mt-2 text-sm text-gray-500">Loading models...</p>
                </div>
              ) : models.length > 0 ? (
                <div className="space-y-1.5 max-h-[240px] overflow-y-auto pr-1">
                  {models.map((model) => (
                    <button
                      key={model.id}
                      onClick={() => setSelectedModel(model.id)}
                      className={`w-full p-2.5 rounded-lg border text-left text-sm transition-all flex items-center justify-between ${
                        selectedModel === model.id
                          ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/30'
                          : 'border-gray-200 dark:border-gray-700 hover:border-primary-300'
                      }`}
                    >
                      <span className="truncate text-gray-900 dark:text-white">
                        {model.name || model.id}
                      </span>
                      <div className="flex items-center gap-2 flex-shrink-0">
                        {model.recommended && (
                          <span className="text-xs bg-green-100 dark:bg-green-900/50 text-green-700 dark:text-green-400 px-1.5 py-0.5 rounded">
                            rec
                          </span>
                        )}
                        {selectedModel === model.id && (
                          <CheckCircle2 className="w-4 h-4 text-primary-500" />
                        )}
                      </div>
                    </button>
                  ))}
                </div>
              ) : (
                <div className="py-4">
                  <input
                    type="text"
                    value={selectedModel}
                    onChange={(e) => setSelectedModel(e.target.value)}
                    placeholder="Enter model name"
                    className="w-full px-3 py-2.5 rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm"
                  />
                  <p className="mt-2 text-xs text-gray-500">Could not fetch models. Enter manually.</p>
                </div>
              )}
              {error && (
                <p className="mt-2 text-sm text-red-500 flex items-center gap-1">
                  <AlertCircle className="w-4 h-4" /> {error}
                </p>
              )}
              <button
                onClick={handleModelSubmit}
                disabled={isLoading || !selectedModel}
                className="w-full mt-4 py-2.5 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg font-medium text-sm flex items-center justify-center gap-2 transition-colors"
              >
                Test Connection
                <ArrowRight className="w-4 h-4" />
              </button>
            </div>
          )}

          {/* Testing */}
          {step === 'testing' && (
            <div className="py-6 text-center">
              {isLoading ? (
                <>
                  <Loader2 className="w-10 h-10 text-primary-500 animate-spin mx-auto" />
                  <p className="mt-3 text-gray-600 dark:text-gray-400">Testing connection...</p>
                </>
              ) : testResult?.success ? (
                <>
                  <CheckCircle2 className="w-12 h-12 text-green-500 mx-auto" />
                  <p className="mt-3 font-medium text-gray-900 dark:text-white">Connected!</p>
                  <p className="text-sm text-gray-500 mt-1">Starting Cowork...</p>
                </>
              ) : (
                <>
                  <AlertCircle className="w-12 h-12 text-red-500 mx-auto" />
                  <p className="mt-3 font-medium text-gray-900 dark:text-white">Connection Failed</p>
                  <p className="text-sm text-red-500 mt-1">{testResult?.message}</p>
                  <button
                    onClick={handleBack}
                    className="mt-4 px-4 py-2 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg text-sm font-medium transition-colors"
                  >
                    Try Again
                  </button>
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
