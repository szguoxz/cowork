import { useState } from 'react'
import {
  HelpCircle,
  MessageSquare,
  History,
  Brain,
  Keyboard,
  Settings,
  FolderOpen,
  Sparkles,
  ChevronDown,
  ChevronRight,
  ExternalLink,
} from 'lucide-react'

interface Section {
  id: string
  title: string
  icon: React.ElementType
  content: React.ReactNode
}

export default function Help() {
  const [expandedSections, setExpandedSections] = useState<Set<string>>(
    new Set(['getting-started'])
  )

  const toggleSection = (id: string) => {
    setExpandedSections((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        next.delete(id)
      } else {
        next.add(id)
      }
      return next
    })
  }

  const sections: Section[] = [
    {
      id: 'getting-started',
      title: 'Getting Started',
      icon: Sparkles,
      content: (
        <div className="space-y-4">
          <p>
            Welcome to <strong>Cowork</strong> - your AI-powered coding assistant!
            Cowork helps you with software development tasks using various AI providers.
          </p>

          <h4 className="font-semibold mt-4">Quick Start</h4>
          <ol className="list-decimal list-inside space-y-2 ml-2">
            <li>Configure your API key in <strong>Settings</strong></li>
            <li>Start a conversation in <strong>Chat</strong></li>
            <li>Ask Cowork to help with coding tasks, file operations, or questions</li>
          </ol>

          <h4 className="font-semibold mt-4">Supported Providers</h4>
          <ul className="list-disc list-inside space-y-1 ml-2">
            <li><strong>Anthropic</strong> - Claude models (recommended)</li>
            <li><strong>OpenAI</strong> - GPT-4, GPT-4o</li>
            <li><strong>Google</strong> - Gemini models</li>
            <li><strong>DeepSeek</strong> - DeepSeek models</li>
            <li><strong>Groq</strong> - Fast inference</li>
            <li><strong>xAI</strong> - Grok models</li>
            <li><strong>Together</strong> - Various open models</li>
            <li><strong>Fireworks</strong> - Fast open models</li>
            <li><strong>Ollama</strong> - Local models (no API key needed)</li>
          </ul>
        </div>
      ),
    },
    {
      id: 'chat',
      title: 'Chat Interface',
      icon: MessageSquare,
      content: (
        <div className="space-y-4">
          <p>
            The Chat page is where you interact with your AI assistant.
            Type your message and press Enter or click Send.
          </p>

          <h4 className="font-semibold mt-4">Features</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>
              <strong>Streaming responses</strong> - See the AI's response as it's generated
            </li>
            <li>
              <strong>Tool calls</strong> - The AI can read files, run commands, and more
            </li>
            <li>
              <strong>Tool approval</strong> - Review and approve tool calls before execution
            </li>
            <li>
              <strong>Context indicator</strong> - Shows how much of the context window is used
            </li>
          </ul>

          <h4 className="font-semibold mt-4">Tool Approval</h4>
          <p>
            When the AI wants to use a tool (like reading a file or running a command),
            you'll see an approval prompt. Click the checkmark to approve or X to reject.
          </p>
        </div>
      ),
    },
    {
      id: 'thinking',
      title: 'Thinking & Reasoning',
      icon: Brain,
      content: (
        <div className="space-y-4">
          <p>
            Some AI models (like Claude and DeepSeek) show their thinking process
            before giving a response. This helps you understand how the AI reasons
            through problems.
          </p>

          <h4 className="font-semibold mt-4">How It Works</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>
              <strong>Purple "Thinking..." box</strong> appears while the AI is reasoning
            </li>
            <li>
              <strong>Collapsible</strong> - Click to expand/collapse the thinking content
            </li>
            <li>
              <strong>Saved with messages</strong> - You can view thinking from past messages
            </li>
          </ul>

          <h4 className="font-semibold mt-4">Benefits</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>Understand the AI's reasoning process</li>
            <li>Catch potential misunderstandings early</li>
            <li>Learn from the AI's problem-solving approach</li>
          </ul>
        </div>
      ),
    },
    {
      id: 'sessions',
      title: 'Session History',
      icon: History,
      content: (
        <div className="space-y-4">
          <p>
            Cowork automatically saves your chat sessions so you can resume them later.
          </p>

          <h4 className="font-semibold mt-4">Auto-Save</h4>
          <p>
            Sessions are automatically saved after each message exchange. No manual saving needed!
          </p>

          <h4 className="font-semibold mt-4">Storage Location</h4>
          <p className="font-mono text-sm bg-gray-100 dark:bg-gray-800 p-2 rounded">
            ~/.config/cowork/sessions/
          </p>
          <p className="text-sm text-gray-600 dark:text-gray-400">
            Sessions are stored as JSON files named: <code>YYYY-MM-DD_sessionid.json</code>
          </p>

          <h4 className="font-semibold mt-4">Managing Sessions</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li><strong>History page</strong> - Browse, load, or delete saved sessions</li>
            <li><strong>Open Folder</strong> - Access session files directly in your file manager</li>
            <li><strong>Quick cleanup</strong> - Delete sessions older than 7 or 30 days</li>
            <li><strong>Ask the AI</strong> - "Delete all sessions older than 2 weeks"</li>
          </ul>

          <h4 className="font-semibold mt-4">What's Saved</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>All messages (user and assistant)</li>
            <li>Thinking/reasoning content</li>
            <li>Tool calls and results</li>
            <li>Session metadata (provider, timestamps)</li>
          </ul>
        </div>
      ),
    },
    {
      id: 'keyboard',
      title: 'Keyboard Shortcuts',
      icon: Keyboard,
      content: (
        <div className="space-y-4">
          <p>Use keyboard shortcuts to work faster:</p>

          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 dark:border-gray-700">
                  <th className="text-left py-2 pr-4">Shortcut</th>
                  <th className="text-left py-2">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
                <tr>
                  <td className="py-2 pr-4">
                    <kbd className="px-2 py-1 bg-gray-100 dark:bg-gray-800 rounded text-xs">Ctrl</kbd>
                    {' + '}
                    <kbd className="px-2 py-1 bg-gray-100 dark:bg-gray-800 rounded text-xs">Enter</kbd>
                  </td>
                  <td>Send message</td>
                </tr>
                <tr>
                  <td className="py-2 pr-4">
                    <kbd className="px-2 py-1 bg-gray-100 dark:bg-gray-800 rounded text-xs">Escape</kbd>
                  </td>
                  <td>Cancel active loop</td>
                </tr>
                <tr>
                  <td className="py-2 pr-4">
                    <kbd className="px-2 py-1 bg-gray-100 dark:bg-gray-800 rounded text-xs">Y</kbd>
                  </td>
                  <td>Approve all pending tool calls</td>
                </tr>
                <tr>
                  <td className="py-2 pr-4">
                    <kbd className="px-2 py-1 bg-gray-100 dark:bg-gray-800 rounded text-xs">N</kbd>
                  </td>
                  <td>Reject all pending tool calls</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      ),
    },
    {
      id: 'files',
      title: 'File Browser',
      icon: FolderOpen,
      content: (
        <div className="space-y-4">
          <p>
            The Files page lets you browse and manage files in your workspace.
          </p>

          <h4 className="font-semibold mt-4">Features</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>Browse directory structure</li>
            <li>View file contents</li>
            <li>Navigate with breadcrumbs</li>
          </ul>

          <h4 className="font-semibold mt-4">AI File Operations</h4>
          <p>
            You can also ask the AI to work with files:
          </p>
          <ul className="list-disc list-inside space-y-1 ml-2 text-sm">
            <li>"Read the contents of src/main.rs"</li>
            <li>"Create a new file called utils.ts"</li>
            <li>"Find all TypeScript files in the project"</li>
            <li>"Search for TODO comments in the codebase"</li>
          </ul>
        </div>
      ),
    },
    {
      id: 'settings',
      title: 'Configuration',
      icon: Settings,
      content: (
        <div className="space-y-4">
          <p>
            Configure Cowork from the Settings page.
          </p>

          <h4 className="font-semibold mt-4">API Keys</h4>
          <p>
            You can set API keys in two ways:
          </p>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>
              <strong>Settings page</strong> - Enter your API key directly
            </li>
            <li>
              <strong>Environment variables</strong> - Set <code>ANTHROPIC_API_KEY</code>,
              <code>OPENAI_API_KEY</code>, etc.
            </li>
          </ul>

          <h4 className="font-semibold mt-4">Config File Location</h4>
          <p className="font-mono text-sm bg-gray-100 dark:bg-gray-800 p-2 rounded">
            ~/.config/cowork/config.toml
          </p>

          <h4 className="font-semibold mt-4">Available Settings</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li><strong>Provider</strong> - Which AI service to use</li>
            <li><strong>Model</strong> - Which model from that provider</li>
            <li><strong>API Key</strong> - Your authentication key</li>
          </ul>
        </div>
      ),
    },
  ]

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header className="h-14 border-b border-gray-200 dark:border-gray-700 flex items-center px-4">
        <div className="flex items-center gap-2">
          <HelpCircle className="w-5 h-5 text-primary-600" />
          <h1 className="text-lg font-semibold text-gray-900 dark:text-white">
            Help & Documentation
          </h1>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="max-w-3xl mx-auto space-y-2">
          {sections.map((section) => {
            const Icon = section.icon
            const isExpanded = expandedSections.has(section.id)

            return (
              <div
                key={section.id}
                className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden"
              >
                <button
                  onClick={() => toggleSection(section.id)}
                  className="w-full px-4 py-3 flex items-center gap-3 bg-gray-50 dark:bg-gray-800/50 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors text-left"
                >
                  {isExpanded ? (
                    <ChevronDown className="w-4 h-4 text-gray-500" />
                  ) : (
                    <ChevronRight className="w-4 h-4 text-gray-500" />
                  )}
                  <Icon className="w-5 h-5 text-primary-600" />
                  <span className="font-medium text-gray-900 dark:text-white">
                    {section.title}
                  </span>
                </button>
                {isExpanded && (
                  <div className="px-4 py-4 text-gray-700 dark:text-gray-300 text-sm leading-relaxed">
                    {section.content}
                  </div>
                )}
              </div>
            )
          })}

          {/* Footer with links */}
          <div className="mt-8 pt-4 border-t border-gray-200 dark:border-gray-700">
            <p className="text-sm text-gray-500 dark:text-gray-400 text-center">
              For more information, visit the{' '}
              <a
                href="https://github.com/anthropics/cowork"
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary-600 hover:underline inline-flex items-center gap-1"
              >
                GitHub repository
                <ExternalLink className="w-3 h-3" />
              </a>
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}
