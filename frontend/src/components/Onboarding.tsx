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

type Step = 'provider' | 'apikey' | 'serpapi' | 'testing'

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

// Providers with native web search capability
const NATIVE_SEARCH_PROVIDERS = ['anthropic', 'openai', 'gemini', 'groq', 'xai', 'perplexity', 'cohere']

interface ApiTestResult {
  success: boolean
  message: string
}

export default function Onboarding({ onComplete }: OnboardingProps) {
  const [step, setStep] = useState<Step>('provider')
  const [selectedProvider, setSelectedProvider] = useState<string>('')
  const [apiKey, setApiKey] = useState('')
  const [serpApiKey, setSerpApiKey] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [testResult, setTestResult] = useState<ApiTestResult | null>(null)

  const needsSerpApi = !NATIVE_SEARCH_PROVIDERS.includes(selectedProvider)

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
      // Ollama doesn't need API key, go straight to testing
      setApiKey('')
      runTest('')
    } else {
      setStep('apikey')
    }
  }

  const handleApiKeySubmit = () => {
    if (!apiKey.trim()) {
      setError('Please enter an API key')
      return
    }
    setError(null)
    if (needsSerpApi) {
      setStep('serpapi')
    } else {
      runTest(apiKey)
    }
  }

  const handleSerpApiSubmit = () => {
    // SerpAPI is optional, proceed to test
    setError(null)
    runTest(apiKey)
  }

  const runTest = async (key: string) => {
    setStep('testing')
    setIsLoading(true)
    setTestResult(null)

    try {
      const result = await invoke<ApiTestResult>('test_api_connection', {
        providerType: selectedProvider,
        apiKey: key || null,
        model: null, // Use provider default
      })
      setTestResult(result)

      if (result.success) {
        // Build settings with optional serpapi config
        const settings: Record<string, unknown> = {
          provider: {
            provider_type: selectedProvider,
            api_key: key || null,
            model: null, // Use provider default
            base_url: null,
          },
          approval: { auto_approve_level: 'low', show_confirmation_dialogs: true },
          ui: { theme: 'system', font_size: 14, show_tool_calls: true },
        }

        // Add web_search config if serpapi key provided
        if (serpApiKey.trim()) {
          settings.web_search = {
            api_key: serpApiKey,
          }
        }

        await invoke('update_settings', { settings })
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
    else if (step === 'serpapi') setStep('apikey')
    else if (step === 'testing') {
      if (selectedProvider === 'ollama') setStep('provider')
      else if (needsSerpApi) setStep('serpapi')
      else setStep('apikey')
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !isLoading) {
      if (step === 'apikey') handleApiKeySubmit()
      else if (step === 'serpapi') handleSerpApiSubmit()
    }
  }

  const progress = step === 'provider' ? 25 : step === 'apikey' ? 50 : step === 'serpapi' ? 70 : 100

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

          {/* SerpAPI Key (optional, for providers without native search) */}
          {step === 'serpapi' && (
            <div>
              <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
                Web Search API (optional):
              </p>
              <p className="text-xs text-gray-400 mb-2">
                {selectedProviderInfo?.name} doesn't have native web search. Add a SerpAPI key to enable web search.
              </p>
              <input
                type="password"
                value={serpApiKey}
                onChange={(e) => setSerpApiKey(e.target.value)}
                placeholder="SERPAPI_API_KEY"
                autoFocus
                className="w-full px-3 py-2 rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              />
              <p className="mt-1.5 text-xs text-gray-400">
                Get a key at serpapi.com (or skip)
              </p>
              <button
                onClick={handleSerpApiSubmit}
                disabled={isLoading}
                className="w-full mt-3 py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-lg font-medium text-sm flex items-center justify-center gap-2 transition-colors"
              >
                {isLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : (serpApiKey.trim() ? 'Continue' : 'Skip')}
                {!isLoading && <ArrowRight className="w-4 h-4" />}
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
