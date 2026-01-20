import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  Sparkles,
  CheckCircle2,
  ArrowRight,
  AlertCircle,
  Globe,
  Loader2,
  Zap,
  Brain,
  Server,
  Cpu,
} from 'lucide-react'

interface OnboardingProps {
  onComplete: () => void
}

type Step = 'welcome' | 'provider' | 'apikey' | 'model' | 'testing' | 'complete'

interface ProviderOption {
  id: string
  name: string
  description: string
  envVar: string
  icon: React.ReactNode
}

const PROVIDERS: ProviderOption[] = [
  {
    id: 'anthropic',
    name: 'Anthropic (Claude)',
    description: 'Best for code, writing, and reasoning',
    envVar: 'ANTHROPIC_API_KEY',
    icon: <Sparkles className="w-5 h-5" />,
  },
  {
    id: 'openai',
    name: 'OpenAI (GPT-4)',
    description: 'Versatile and widely supported',
    envVar: 'OPENAI_API_KEY',
    icon: <Globe className="w-5 h-5" />,
  },
  {
    id: 'gemini',
    name: 'Google Gemini',
    description: 'Large context window (1M tokens)',
    envVar: 'GEMINI_API_KEY',
    icon: <Brain className="w-5 h-5" />,
  },
  {
    id: 'groq',
    name: 'Groq',
    description: 'Ultra-fast inference',
    envVar: 'GROQ_API_KEY',
    icon: <Zap className="w-5 h-5" />,
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    description: 'Cost-effective reasoning',
    envVar: 'DEEPSEEK_API_KEY',
    icon: <Brain className="w-5 h-5" />,
  },
  {
    id: 'xai',
    name: 'xAI (Grok)',
    description: 'Latest Grok models',
    envVar: 'XAI_API_KEY',
    icon: <Sparkles className="w-5 h-5" />,
  },
  {
    id: 'together',
    name: 'Together AI',
    description: '200+ open source models',
    envVar: 'TOGETHER_API_KEY',
    icon: <Globe className="w-5 h-5" />,
  },
  {
    id: 'fireworks',
    name: 'Fireworks AI',
    description: 'Fast open source model inference',
    envVar: 'FIREWORKS_API_KEY',
    icon: <Zap className="w-5 h-5" />,
  },
  {
    id: 'zai',
    name: 'Zai (Zhipu AI)',
    description: 'GLM-4 models from China',
    envVar: 'ZAI_API_KEY',
    icon: <Brain className="w-5 h-5" />,
  },
  {
    id: 'nebius',
    name: 'Nebius AI Studio',
    description: '30+ open source models',
    envVar: 'NEBIUS_API_KEY',
    icon: <Globe className="w-5 h-5" />,
  },
  {
    id: 'mimo',
    name: 'MIMO (Xiaomi)',
    description: "Xiaomi's MIMO models",
    envVar: 'MIMO_API_KEY',
    icon: <Zap className="w-5 h-5" />,
  },
  {
    id: 'bigmodel',
    name: 'BigModel.cn',
    description: 'Zhipu AI China platform',
    envVar: 'BIGMODEL_API_KEY',
    icon: <Brain className="w-5 h-5" />,
  },
  {
    id: 'ollama',
    name: 'Ollama (Local)',
    description: 'Run models locally, no API key needed',
    envVar: '',
    icon: <Server className="w-5 h-5" />,
  },
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
  const [step, setStep] = useState<Step>('welcome')
  const [selectedProvider, setSelectedProvider] = useState<string>('anthropic')
  const [apiKey, setApiKey] = useState('')
  const [models, setModels] = useState<ModelInfo[]>([])
  const [selectedModel, setSelectedModel] = useState<string>('')
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [testResult, setTestResult] = useState<ApiTestResult | null>(null)

  const handleProviderSelect = (providerId: string) => {
    setSelectedProvider(providerId)
    setError(null)
  }

  const handleModelSelect = (modelId: string) => {
    setSelectedModel(modelId)
    setError(null)
  }

  const fetchModels = async () => {
    setIsLoading(true)
    setError(null)

    try {
      const fetchedModels = await invoke<ModelInfo[]>('fetch_provider_models', {
        providerType: selectedProvider,
        apiKey: apiKey,
      })
      setModels(fetchedModels)
      // Auto-select first recommended model, or first model
      const recommended = fetchedModels.find((m) => m.recommended)
      setSelectedModel(recommended?.id || fetchedModels[0]?.id || '')
    } catch (err) {
      console.error('Failed to fetch models:', err)
      setModels([])
      setError('Could not fetch models. Please try again.')
    } finally {
      setIsLoading(false)
    }
  }

  const handleNext = async () => {
    setError(null)

    switch (step) {
      case 'welcome':
        setStep('provider')
        break

      case 'provider':
        if (selectedProvider === 'ollama') {
          // Ollama: fetch models then go to model selection
          setApiKey('') // No API key for Ollama
          setStep('model')
          // Fetch Ollama models
          setIsLoading(true)
          try {
            const fetchedModels = await invoke<ModelInfo[]>('fetch_provider_models', {
              providerType: selectedProvider,
              apiKey: '',
            })
            setModels(fetchedModels)
            const recommended = fetchedModels.find((m) => m.recommended)
            setSelectedModel(recommended?.id || fetchedModels[0]?.id || 'llama3.2')
          } catch {
            setModels([])
            setSelectedModel('llama3.2')
          } finally {
            setIsLoading(false)
          }
        } else {
          setStep('apikey')
        }
        break

      case 'apikey':
        if (!apiKey.trim()) {
          setError('Please enter an API key')
          return
        }
        // Move to model selection step
        setStep('model')
        await fetchModels()
        break

      case 'model':
        if (!selectedModel && models.length > 0) {
          setError('Please select a model')
          return
        }
        // Move to testing step
        setStep('testing')
        setIsLoading(true)
        setTestResult(null)

        try {
          // Test the API connection
          const result = await invoke<ApiTestResult>('test_api_connection', {
            providerType: selectedProvider,
            apiKey: apiKey || null,
            model: selectedModel || null,
          })

          setTestResult(result)

          if (result.success) {
            // Save the settings on success
            await invoke('update_settings', {
              settings: {
                provider: {
                  provider_type: selectedProvider,
                  api_key: apiKey || null,
                  model: selectedModel,
                  base_url: null,
                },
                approval: {
                  auto_approve_level: 'low',
                  show_confirmation_dialogs: true,
                },
                ui: {
                  theme: 'system',
                  font_size: 14,
                  show_tool_calls: true,
                },
              },
            })
            await invoke('save_settings')
          }
        } catch (err) {
          setTestResult({
            success: false,
            message: String(err),
          })
        } finally {
          setIsLoading(false)
        }
        break

      case 'testing':
        if (testResult?.success) {
          setStep('complete')
        } else {
          // Go back to model selection
          setStep('model')
          setTestResult(null)
        }
        break

      case 'complete':
        localStorage.setItem('onboarding_complete', 'true')
        onComplete()
        break
    }
  }

  const getStepProgress = () => {
    switch (step) {
      case 'welcome':
        return 16
      case 'provider':
        return 32
      case 'apikey':
        return 48
      case 'model':
        return 64
      case 'testing':
        return 80
      case 'complete':
        return 100
      default:
        return 0
    }
  }

  return (
    <div className="fixed inset-0 bg-gray-900/80 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 rounded-2xl shadow-2xl w-full max-w-lg mx-4 overflow-hidden">
        {/* Progress bar */}
        <div className="h-1 bg-gray-200 dark:bg-gray-700">
          <div
            className="h-full bg-primary-500 transition-all duration-300"
            style={{ width: `${getStepProgress()}%` }}
          />
        </div>

        {/* Content */}
        <div className="p-8">
          {step === 'welcome' && (
            <div className="text-center">
              <div className="w-16 h-16 bg-primary-100 dark:bg-primary-900 rounded-full flex items-center justify-center mx-auto mb-6">
                <Sparkles className="w-8 h-8 text-primary-600" />
              </div>
              <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-4">
                Welcome to Cowork
              </h2>
              <p className="text-gray-600 dark:text-gray-400 mb-8">
                Your AI-powered assistant for software development. Let's get you set up in a few
                quick steps.
              </p>
              <button
                onClick={handleNext}
                className="w-full py-3 px-4 bg-primary-600 hover:bg-primary-700 text-white rounded-lg font-medium flex items-center justify-center gap-2 transition-colors"
              >
                Get Started
                <ArrowRight className="w-5 h-5" />
              </button>
            </div>
          )}

          {step === 'provider' && (
            <div>
              <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-2">
                Choose your AI provider
              </h2>
              <p className="text-gray-600 dark:text-gray-400 mb-6">
                Select which AI service you'd like to use. You can change this later in settings.
              </p>

              <div className="space-y-3 mb-6 max-h-80 overflow-y-auto">
                {PROVIDERS.map((provider) => (
                  <button
                    key={provider.id}
                    onClick={() => handleProviderSelect(provider.id)}
                    className={`w-full p-4 rounded-lg border-2 text-left flex items-start gap-4 transition-colors ${
                      selectedProvider === provider.id
                        ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
                        : 'border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600'
                    }`}
                  >
                    <div
                      className={`p-2 rounded-lg ${
                        selectedProvider === provider.id
                          ? 'bg-primary-100 dark:bg-primary-900 text-primary-600'
                          : 'bg-gray-100 dark:bg-gray-700 text-gray-500'
                      }`}
                    >
                      {provider.icon}
                    </div>
                    <div className="flex-1">
                      <div className="font-medium text-gray-900 dark:text-white">
                        {provider.name}
                      </div>
                      <div className="text-sm text-gray-500 dark:text-gray-400">
                        {provider.description}
                      </div>
                    </div>
                    {selectedProvider === provider.id && (
                      <CheckCircle2 className="w-5 h-5 text-primary-600" />
                    )}
                  </button>
                ))}
              </div>

              <button
                onClick={handleNext}
                className="w-full py-3 px-4 bg-primary-600 hover:bg-primary-700 text-white rounded-lg font-medium flex items-center justify-center gap-2 transition-colors"
              >
                Continue
                <ArrowRight className="w-5 h-5" />
              </button>
            </div>
          )}

          {step === 'apikey' && (
            <div>
              <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-2">
                Enter your API key
              </h2>
              <p className="text-gray-600 dark:text-gray-400 mb-6">
                Your API key is stored locally and never sent anywhere except to the AI provider.
              </p>

              <div className="mb-6">
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                  {PROVIDERS.find((p) => p.id === selectedProvider)?.envVar || 'API Key'}
                </label>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-..."
                  className="w-full px-4 py-3 rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                />
                {error && (
                  <div className="mt-2 flex items-center gap-2 text-red-600 text-sm">
                    <AlertCircle className="w-4 h-4" />
                    {error}
                  </div>
                )}
              </div>

              <p className="text-xs text-gray-500 dark:text-gray-400 mb-6">
                You can also set the {PROVIDERS.find((p) => p.id === selectedProvider)?.envVar}{' '}
                environment variable instead.
              </p>

              <button
                onClick={handleNext}
                disabled={isLoading}
                className="w-full py-3 px-4 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg font-medium flex items-center justify-center gap-2 transition-colors"
              >
                {isLoading ? (
                  <>
                    <Loader2 className="w-5 h-5 animate-spin" />
                    Fetching models...
                  </>
                ) : (
                  <>
                    Continue
                    <ArrowRight className="w-5 h-5" />
                  </>
                )}
              </button>
            </div>
          )}

          {step === 'model' && (
            <div>
              <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-2">
                Select a model
              </h2>
              <p className="text-gray-600 dark:text-gray-400 mb-6">
                Choose the AI model you'd like to use. Recommended models are marked.
              </p>

              {isLoading ? (
                <div className="flex flex-col items-center justify-center py-12">
                  <Loader2 className="w-8 h-8 text-primary-600 animate-spin mb-4" />
                  <p className="text-gray-600 dark:text-gray-400">Fetching available models...</p>
                </div>
              ) : models.length > 0 ? (
                <div className="space-y-2 mb-6 max-h-64 overflow-y-auto">
                  {models.map((model) => (
                    <button
                      key={model.id}
                      onClick={() => handleModelSelect(model.id)}
                      className={`w-full p-3 rounded-lg border-2 text-left flex items-center gap-3 transition-colors ${
                        selectedModel === model.id
                          ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
                          : 'border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600'
                      }`}
                    >
                      <Cpu
                        className={`w-5 h-5 ${
                          selectedModel === model.id ? 'text-primary-600' : 'text-gray-400'
                        }`}
                      />
                      <div className="flex-1 min-w-0">
                        <div className="font-medium text-gray-900 dark:text-white truncate">
                          {model.name || model.id}
                        </div>
                        {model.description && (
                          <div className="text-xs text-gray-500 dark:text-gray-400 truncate">
                            {model.description}
                          </div>
                        )}
                      </div>
                      {model.recommended && (
                        <span className="text-xs bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-300 px-2 py-1 rounded">
                          Recommended
                        </span>
                      )}
                      {selectedModel === model.id && (
                        <CheckCircle2 className="w-5 h-5 text-primary-600 flex-shrink-0" />
                      )}
                    </button>
                  ))}
                </div>
              ) : (
                <div className="text-center py-8 mb-6">
                  <AlertCircle className="w-12 h-12 text-yellow-500 mx-auto mb-4" />
                  <p className="text-gray-600 dark:text-gray-400 mb-4">
                    Could not fetch models from the provider.
                  </p>
                  <input
                    type="text"
                    value={selectedModel}
                    onChange={(e) => setSelectedModel(e.target.value)}
                    placeholder="Enter model name manually"
                    className="w-full px-4 py-3 rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                  />
                </div>
              )}

              {error && (
                <div className="mb-4 flex items-center gap-2 text-red-600 text-sm">
                  <AlertCircle className="w-4 h-4" />
                  {error}
                </div>
              )}

              <button
                onClick={handleNext}
                disabled={isLoading || (!selectedModel && models.length > 0)}
                className="w-full py-3 px-4 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg font-medium flex items-center justify-center gap-2 transition-colors"
              >
                Test Connection
                <ArrowRight className="w-5 h-5" />
              </button>
            </div>
          )}

          {step === 'testing' && (
            <div className="text-center">
              {isLoading ? (
                <>
                  <div className="w-16 h-16 bg-blue-100 dark:bg-blue-900 rounded-full flex items-center justify-center mx-auto mb-6">
                    <Loader2 className="w-8 h-8 text-blue-600 animate-spin" />
                  </div>
                  <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-4">
                    Testing Connection
                  </h2>
                  <p className="text-gray-600 dark:text-gray-400">
                    Connecting to {PROVIDERS.find((p) => p.id === selectedProvider)?.name}...
                  </p>
                </>
              ) : testResult?.success ? (
                <>
                  <div className="w-16 h-16 bg-green-100 dark:bg-green-900 rounded-full flex items-center justify-center mx-auto mb-6">
                    <CheckCircle2 className="w-8 h-8 text-green-600" />
                  </div>
                  <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-4">
                    Connection Successful!
                  </h2>
                  <p className="text-gray-600 dark:text-gray-400 mb-8">
                    Your API key is valid and working with model{' '}
                    <span className="font-medium">{selectedModel}</span>.
                  </p>
                  <button
                    onClick={handleNext}
                    className="w-full py-3 px-4 bg-primary-600 hover:bg-primary-700 text-white rounded-lg font-medium flex items-center justify-center gap-2 transition-colors"
                  >
                    Continue
                    <ArrowRight className="w-5 h-5" />
                  </button>
                </>
              ) : (
                <>
                  <div className="w-16 h-16 bg-red-100 dark:bg-red-900 rounded-full flex items-center justify-center mx-auto mb-6">
                    <AlertCircle className="w-8 h-8 text-red-600" />
                  </div>
                  <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-4">
                    Connection Failed
                  </h2>
                  <p className="text-gray-600 dark:text-gray-400 mb-2">{testResult?.message}</p>
                  <p className="text-sm text-gray-500 dark:text-gray-500 mb-8">
                    Please check your API key and try again.
                  </p>
                  <button
                    onClick={handleNext}
                    className="w-full py-3 px-4 bg-primary-600 hover:bg-primary-700 text-white rounded-lg font-medium flex items-center justify-center gap-2 transition-colors"
                  >
                    Try Again
                    <ArrowRight className="w-5 h-5" />
                  </button>
                </>
              )}
            </div>
          )}

          {step === 'complete' && (
            <div className="text-center">
              <div className="w-16 h-16 bg-green-100 dark:bg-green-900 rounded-full flex items-center justify-center mx-auto mb-6">
                <CheckCircle2 className="w-8 h-8 text-green-600" />
              </div>
              <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-4">
                You're all set!
              </h2>
              <p className="text-gray-600 dark:text-gray-400 mb-8">
                Start by typing a message or try a command like{' '}
                <code className="bg-gray-100 dark:bg-gray-700 px-2 py-1 rounded">/help</code> to see
                what Cowork can do.
              </p>
              <button
                onClick={handleNext}
                className="w-full py-3 px-4 bg-primary-600 hover:bg-primary-700 text-white rounded-lg font-medium transition-colors"
              >
                Start Using Cowork
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
