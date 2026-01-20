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
  }

  const handleProviderNext = () => {
    if (!selectedProvider) {
      setError('Please select a provider')
      return
    }
    if (selectedProvider === 'ollama') {
      setApiKey('')
      setStep('model')
      fetchModelsForProvider(selectedProvider, '')
    } else {
      setStep('apikey')
    }
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
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-2xl w-full max-w-sm overflow-hidden">
        {/* Progress bar */}
        <div className="h-1 bg-gray-200 dark:bg-gray-700">
          <div
            className="h-full bg-blue-500 transition-all duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>

        {/* Header */}
        <div className="px-4 pt-3 pb-1 flex items-center gap-2">
          {step !== 'provider' && (
            <button
              onClick={handleBack}
              className="p-0.5 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors"
            >
              <ChevronLeft className="w-4 h-4 text-gray-500" />
            </button>
          )}
          <div className="flex items-center gap-1.5">
            <Sparkles className="w-4 h-4 text-blue-500" />
            <span className="font-medium text-sm text-gray-900 dark:text-white">Setup</span>
          </div>
        </div>

        {/* Content */}
        <div className="px-4 pb-4" onKeyDown={handleKeyDown}>
          {/* Provider Selection */}
          {step === 'provider' && (
            <div>
              <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
                Select provider:
              </p>
              <div className="grid grid-cols-3 gap-1.5">
                {PROVIDERS.map((provider) => (
                  <button
                    key={provider.id}
                    onClick={() => handleProviderSelect(provider.id)}
                    className={`px-2 py-1.5 rounded border text-xs transition-all truncate ${
                      selectedProvider === provider.id
                        ? 'border-blue-500 bg-blue-500 text-white font-medium'
                        : 'border-gray-200 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:border-blue-400 hover:bg-gray-50 dark:hover:bg-gray-700'
                    }`}
                  >
                    {provider.name.replace(' (Claude)', '').replace(' (GPT-4)', '').replace(' (Grok)', '').replace(' (Local)', '').replace(' AI', '').replace('.cn', '')}
                  </button>
                ))}
              </div>
              {error && (
                <p className="mt-2 text-xs text-red-500 flex items-center gap-1">
                  <AlertCircle className="w-3 h-3" /> {error}
                </p>
              )}
              <button
                onClick={handleProviderNext}
                disabled={!selectedProvider}
                className="w-full mt-3 py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-40 disabled:cursor-not-allowed text-white rounded-lg font-medium text-sm flex items-center justify-center gap-2 transition-colors"
              >
                Next
                <ArrowRight className="w-4 h-4" />
              </button>
            </div>
          )}

          {/* API Key Input */}
          {step === 'apikey' && (
            <div>
              <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
                {selectedProviderInfo?.name} API key:
              </p>
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={selectedProviderInfo?.envVar || 'API Key'}
                autoFocus
                className="w-full px-3 py-2 rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              />
              {error && (
                <p className="mt-1.5 text-xs text-red-500 flex items-center gap-1">
                  <AlertCircle className="w-3 h-3" /> {error}
                </p>
              )}
              <p className="mt-1.5 text-xs text-gray-400">
                Or set {selectedProviderInfo?.envVar} env var
              </p>
              <button
                onClick={handleApiKeySubmit}
                disabled={isLoading}
                className="w-full mt-3 py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-lg font-medium text-sm flex items-center justify-center gap-2 transition-colors"
              >
                {isLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : 'Continue'}
                {!isLoading && <ArrowRight className="w-4 h-4" />}
              </button>
            </div>
          )}

          {/* Model Selection */}
          {step === 'model' && (
            <div>
              <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">Select model:</p>
              {isLoading ? (
                <div className="py-6 flex flex-col items-center">
                  <Loader2 className="w-5 h-5 text-blue-500 animate-spin" />
                  <p className="mt-2 text-xs text-gray-500">Loading...</p>
                </div>
              ) : models.length > 0 ? (
                <div className="space-y-1 max-h-[200px] overflow-y-auto">
                  {models.map((model) => (
                    <button
                      key={model.id}
                      onClick={() => setSelectedModel(model.id)}
                      className={`w-full px-2.5 py-1.5 rounded border text-left text-xs transition-all flex items-center justify-between ${
                        selectedModel === model.id
                          ? 'border-blue-500 bg-blue-500 text-white'
                          : 'border-gray-200 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:border-blue-400'
                      }`}
                    >
                      <span className="truncate">{model.name || model.id}</span>
                      <div className="flex items-center gap-1.5 flex-shrink-0">
                        {model.recommended && (
                          <span className={`text-[10px] px-1 py-0.5 rounded ${
                            selectedModel === model.id
                              ? 'bg-white/20 text-white'
                              : 'bg-green-100 dark:bg-green-900/50 text-green-700 dark:text-green-400'
                          }`}>
                            rec
                          </span>
                        )}
                        {selectedModel === model.id && (
                          <CheckCircle2 className="w-3.5 h-3.5" />
                        )}
                      </div>
                    </button>
                  ))}
                </div>
              ) : (
                <div className="py-2">
                  <input
                    type="text"
                    value={selectedModel}
                    onChange={(e) => setSelectedModel(e.target.value)}
                    placeholder="Enter model name"
                    className="w-full px-3 py-2 rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm"
                  />
                  <p className="mt-1.5 text-xs text-gray-400">Enter model name manually</p>
                </div>
              )}
              {error && (
                <p className="mt-1.5 text-xs text-red-500 flex items-center gap-1">
                  <AlertCircle className="w-3 h-3" /> {error}
                </p>
              )}
              <button
                onClick={handleModelSubmit}
                disabled={isLoading || !selectedModel}
                className="w-full mt-3 py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-40 disabled:cursor-not-allowed text-white rounded-lg font-medium text-sm flex items-center justify-center gap-2 transition-colors"
              >
                Test Connection
                <ArrowRight className="w-4 h-4" />
              </button>
            </div>
          )}

          {/* Testing */}
          {step === 'testing' && (
            <div className="py-4 text-center">
              {isLoading ? (
                <>
                  <Loader2 className="w-8 h-8 text-blue-500 animate-spin mx-auto" />
                  <p className="mt-2 text-sm text-gray-500">Testing...</p>
                </>
              ) : testResult?.success ? (
                <>
                  <CheckCircle2 className="w-10 h-10 text-green-500 mx-auto" />
                  <p className="mt-2 font-medium text-gray-900 dark:text-white">Connected!</p>
                  <p className="text-xs text-gray-500 mt-0.5">Starting...</p>
                </>
              ) : (
                <>
                  <AlertCircle className="w-10 h-10 text-red-500 mx-auto" />
                  <p className="mt-2 font-medium text-sm text-gray-900 dark:text-white">Failed</p>
                  <p className="text-xs text-red-500 mt-1 max-w-[250px] mx-auto">{testResult?.message}</p>
                  <button
                    onClick={handleBack}
                    className="mt-3 px-3 py-1.5 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded text-sm transition-colors"
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
