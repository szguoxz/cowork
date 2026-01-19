import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  Sparkles,
  CheckCircle2,
  ArrowRight,
  AlertCircle,
  Code,
  FileText,
  Terminal,
  Globe,
  Loader2,
  Zap,
  Brain,
  Server,
} from 'lucide-react'

interface OnboardingProps {
  onComplete: () => void
}

type Step = 'welcome' | 'provider' | 'apikey' | 'testing' | 'profile' | 'complete'

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
    id: 'ollama',
    name: 'Ollama (Local)',
    description: 'Run models locally, no API key needed',
    envVar: '',
    icon: <Server className="w-5 h-5" />,
  },
]

interface ProfileOption {
  id: string
  name: string
  description: string
  icon: React.ReactNode
}

const PROFILES: ProfileOption[] = [
  {
    id: 'developer',
    name: 'Coding Assistant',
    description: 'Full access to all tools for software development',
    icon: <Code className="w-6 h-6" />,
  },
  {
    id: 'writer',
    name: 'Writing Helper',
    description: 'Focus on document editing and content creation',
    icon: <FileText className="w-6 h-6" />,
  },
  {
    id: 'simple',
    name: 'Simple Mode',
    description: 'Hides technical details for everyday tasks',
    icon: <Terminal className="w-6 h-6" />,
  },
]

interface ApiTestResult {
  success: boolean
  message: string
}

export default function Onboarding({ onComplete }: OnboardingProps) {
  const [step, setStep] = useState<Step>('welcome')
  const [selectedProvider, setSelectedProvider] = useState<string>('anthropic')
  const [apiKey, setApiKey] = useState('')
  const [selectedProfile, setSelectedProfile] = useState<string>('developer')
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [testResult, setTestResult] = useState<ApiTestResult | null>(null)

  const handleProviderSelect = (providerId: string) => {
    setSelectedProvider(providerId)
    setError(null)
  }

  const handleNext = async () => {
    setError(null)

    switch (step) {
      case 'welcome':
        setStep('provider')
        break

      case 'provider':
        if (selectedProvider === 'ollama') {
          setStep('profile')
        } else {
          setStep('apikey')
        }
        break

      case 'apikey':
        if (!apiKey.trim()) {
          setError('Please enter an API key')
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
            apiKey: apiKey,
            model: null,
          })

          setTestResult(result)

          if (result.success) {
            // Save the settings on success
            await invoke('update_settings', {
              settings: {
                provider: {
                  provider_type: selectedProvider,
                  api_key: apiKey,
                  model: getDefaultModel(selectedProvider),
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
          setStep('profile')
        } else {
          // Go back to API key entry
          setStep('apikey')
          setTestResult(null)
        }
        break

      case 'profile':
        setIsLoading(true)
        try {
          // Save profile preference (for future use)
          localStorage.setItem('cowork_profile', selectedProfile)
          setStep('complete')
        } catch (err) {
          setError(String(err))
        } finally {
          setIsLoading(false)
        }
        break

      case 'complete':
        localStorage.setItem('onboarding_complete', 'true')
        onComplete()
        break
    }
  }

  const getDefaultModel = (provider: string): string => {
    switch (provider) {
      case 'anthropic':
        return 'claude-sonnet-4-20250514'
      case 'openai':
        return 'gpt-4o'
      case 'gemini':
        return 'gemini-2.0-flash'
      case 'groq':
        return 'llama-3.3-70b-versatile'
      case 'deepseek':
        return 'deepseek-chat'
      case 'xai':
        return 'grok-2'
      case 'ollama':
        return 'llama3.2'
      default:
        return ''
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
      case 'testing':
        return 64
      case 'profile':
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
                {isLoading ? 'Testing...' : 'Test Connection'}
                {!isLoading && <ArrowRight className="w-5 h-5" />}
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
                    Your API key is valid and working.
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

          {step === 'profile' && (
            <div>
              <h2 className="text-xl font-bold text-gray-900 dark:text-white mb-2">
                Choose your experience
              </h2>
              <p className="text-gray-600 dark:text-gray-400 mb-6">
                How would you like to use Cowork? You can change this anytime.
              </p>

              <div className="space-y-3 mb-6">
                {PROFILES.map((profile) => (
                  <button
                    key={profile.id}
                    onClick={() => setSelectedProfile(profile.id)}
                    className={`w-full p-4 rounded-lg border-2 text-left flex items-start gap-4 transition-colors ${
                      selectedProfile === profile.id
                        ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
                        : 'border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600'
                    }`}
                  >
                    <div
                      className={`p-3 rounded-lg ${
                        selectedProfile === profile.id
                          ? 'bg-primary-100 dark:bg-primary-900 text-primary-600'
                          : 'bg-gray-100 dark:bg-gray-700 text-gray-500'
                      }`}
                    >
                      {profile.icon}
                    </div>
                    <div className="flex-1">
                      <div className="font-medium text-gray-900 dark:text-white">
                        {profile.name}
                      </div>
                      <div className="text-sm text-gray-500 dark:text-gray-400">
                        {profile.description}
                      </div>
                    </div>
                    {selectedProfile === profile.id && (
                      <CheckCircle2 className="w-5 h-5 text-primary-600" />
                    )}
                  </button>
                ))}
              </div>

              <button
                onClick={handleNext}
                disabled={isLoading}
                className="w-full py-3 px-4 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded-lg font-medium flex items-center justify-center gap-2 transition-colors"
              >
                {isLoading ? 'Finishing...' : 'Finish Setup'}
                {!isLoading && <ArrowRight className="w-5 h-5" />}
              </button>
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
                Start by typing a message or try a command like <code className="bg-gray-100 dark:bg-gray-700 px-2 py-1 rounded">/help</code> to see what
                Cowork can do.
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
