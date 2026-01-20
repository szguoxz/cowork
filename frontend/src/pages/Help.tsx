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
            Welcome to <strong className="text-foreground">Cowork</strong> - your AI-powered coding assistant!
            Cowork helps you with software development tasks using various AI providers.
          </p>

          <h4 className="font-semibold mt-4 text-foreground">Quick Start</h4>
          <ol className="list-decimal list-inside space-y-2 ml-2">
            <li>Configure your API key in <strong className="text-foreground">Settings</strong></li>
            <li>Click <strong className="text-foreground">New Chat</strong> to start a conversation</li>
            <li>Ask Cowork to help with coding tasks, file operations, or questions</li>
          </ol>

          <h4 className="font-semibold mt-4 text-foreground">Supported Providers</h4>
          <ul className="list-disc list-inside space-y-1 ml-2">
            <li><strong className="text-foreground">Anthropic</strong> - Claude models (recommended)</li>
            <li><strong className="text-foreground">OpenAI</strong> - GPT-4, GPT-4o</li>
            <li><strong className="text-foreground">Google</strong> - Gemini models</li>
            <li><strong className="text-foreground">DeepSeek</strong> - DeepSeek models</li>
            <li><strong className="text-foreground">Groq</strong> - Fast inference</li>
            <li><strong className="text-foreground">xAI</strong> - Grok models</li>
            <li><strong className="text-foreground">Together</strong> - Various open models</li>
            <li><strong className="text-foreground">Fireworks</strong> - Fast open models</li>
            <li><strong className="text-foreground">Ollama</strong> - Local models (no API key needed)</li>
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

          <h4 className="font-semibold mt-4 text-foreground">Features</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>
              <strong className="text-foreground">Streaming responses</strong> - See the AI's response as it's generated
            </li>
            <li>
              <strong className="text-foreground">Tool calls</strong> - The AI can read files, run commands, and more
            </li>
            <li>
              <strong className="text-foreground">Tool approval</strong> - Review and approve tool calls before execution
            </li>
            <li>
              <strong className="text-foreground">Context indicator</strong> - Shows how much of the context window is used
            </li>
          </ul>

          <h4 className="font-semibold mt-4 text-foreground">Tool Approval</h4>
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

          <h4 className="font-semibold mt-4 text-foreground">How It Works</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>
              <strong className="text-primary">Purple "Thinking..." box</strong> appears while the AI is reasoning
            </li>
            <li>
              <strong className="text-foreground">Collapsible</strong> - Click to expand/collapse the thinking content
            </li>
            <li>
              <strong className="text-foreground">Saved with messages</strong> - You can view thinking from past messages
            </li>
          </ul>

          <h4 className="font-semibold mt-4 text-foreground">Benefits</h4>
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
      title: 'Sessions & History',
      icon: History,
      content: (
        <div className="space-y-4">
          <p>
            Cowork supports multiple concurrent sessions and automatically saves them when closed.
          </p>

          <h4 className="font-semibold mt-4 text-foreground">Multiple Sessions</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li><strong className="text-foreground">Session tabs</strong> - Work with multiple conversations at once</li>
            <li><strong className="text-foreground">New Chat</strong> - Click to create a new session</li>
            <li><strong className="text-foreground">Close tab</strong> - Sessions are saved automatically when closed</li>
          </ul>

          <h4 className="font-semibold mt-4 text-foreground">Auto-Save</h4>
          <p>
            Sessions are automatically saved when you close them or exit the app.
            Empty sessions (with no messages) are not saved.
          </p>

          <h4 className="font-semibold mt-4 text-foreground">Storage Location</h4>
          <p className="font-mono text-sm bg-secondary p-2 rounded-lg text-foreground">
            ~/.local/share/cowork/sessions/
          </p>
          <p className="text-sm text-muted-foreground">
            Sessions are stored as JSON files named: <code className="bg-secondary px-1 rounded">YYYY-MM-DD_sessionid.json</code>
          </p>

          <h4 className="font-semibold mt-4 text-foreground">Managing Sessions</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li><strong className="text-foreground">History page</strong> - Browse, load, or delete saved sessions</li>
            <li><strong className="text-foreground">Open Folder</strong> - Access session files directly in your file manager</li>
            <li><strong className="text-foreground">Quick cleanup</strong> - Delete sessions older than 7 or 30 days</li>
          </ul>

          <h4 className="font-semibold mt-4 text-foreground">What's Saved</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>All messages (user and assistant)</li>
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
                <tr className="border-b border-border">
                  <th className="text-left py-2 pr-4 text-foreground">Shortcut</th>
                  <th className="text-left py-2 text-foreground">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                <tr>
                  <td className="py-2 pr-4">
                    <kbd className="px-2 py-1 bg-secondary rounded text-xs text-foreground">Ctrl</kbd>
                    {' + '}
                    <kbd className="px-2 py-1 bg-secondary rounded text-xs text-foreground">Enter</kbd>
                  </td>
                  <td>Send message</td>
                </tr>
                <tr>
                  <td className="py-2 pr-4">
                    <kbd className="px-2 py-1 bg-secondary rounded text-xs text-foreground">Escape</kbd>
                  </td>
                  <td>Cancel active loop</td>
                </tr>
                <tr>
                  <td className="py-2 pr-4">
                    <kbd className="px-2 py-1 bg-secondary rounded text-xs text-foreground">Y</kbd>
                  </td>
                  <td>Approve all pending tool calls</td>
                </tr>
                <tr>
                  <td className="py-2 pr-4">
                    <kbd className="px-2 py-1 bg-secondary rounded text-xs text-foreground">N</kbd>
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

          <h4 className="font-semibold mt-4 text-foreground">Features</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>Browse directory structure</li>
            <li>View file contents</li>
            <li>Navigate with breadcrumbs</li>
          </ul>

          <h4 className="font-semibold mt-4 text-foreground">AI File Operations</h4>
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

          <h4 className="font-semibold mt-4 text-foreground">API Keys</h4>
          <p>
            You can set API keys in two ways:
          </p>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li>
              <strong className="text-foreground">Settings page</strong> - Enter your API key directly
            </li>
            <li>
              <strong className="text-foreground">Environment variables</strong> - Set <code className="bg-secondary px-1 rounded">ANTHROPIC_API_KEY</code>,
              <code className="bg-secondary px-1 rounded">OPENAI_API_KEY</code>, etc.
            </li>
          </ul>

          <h4 className="font-semibold mt-4 text-foreground">Config File Location</h4>
          <p className="font-mono text-sm bg-secondary p-2 rounded-lg text-foreground">
            ~/.config/cowork/config.toml
          </p>

          <h4 className="font-semibold mt-4 text-foreground">Available Settings</h4>
          <ul className="list-disc list-inside space-y-2 ml-2">
            <li><strong className="text-foreground">Provider</strong> - Which AI service to use</li>
            <li><strong className="text-foreground">Model</strong> - Which model from that provider</li>
            <li><strong className="text-foreground">API Key</strong> - Your authentication key</li>
          </ul>
        </div>
      ),
    },
  ]

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <header className="h-14 border-b border-border flex items-center px-4 bg-card/50">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-violet-500 to-purple-600 flex items-center justify-center shadow-glow-sm">
            <HelpCircle className="w-4 h-4 text-white" />
          </div>
          <h1 className="text-lg font-semibold text-foreground">
            Help & Documentation
          </h1>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="max-w-3xl mx-auto space-y-3">
          {sections.map((section) => {
            const Icon = section.icon
            const isExpanded = expandedSections.has(section.id)

            return (
              <div
                key={section.id}
                className="border border-border rounded-xl overflow-hidden bg-card"
              >
                <button
                  onClick={() => toggleSection(section.id)}
                  className="w-full px-4 py-3 flex items-center gap-3 bg-card hover:bg-black/5 dark:hover:bg-white/5 transition-colors text-left"
                >
                  {isExpanded ? (
                    <ChevronDown className="w-4 h-4 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="w-4 h-4 text-muted-foreground" />
                  )}
                  <Icon className={`w-5 h-5 ${isExpanded ? 'text-primary' : 'text-muted-foreground'}`} />
                  <span className={`font-medium ${isExpanded ? 'text-foreground' : 'text-muted-foreground'}`}>
                    {section.title}
                  </span>
                </button>
                {isExpanded && (
                  <div className="px-4 py-4 text-muted-foreground text-sm leading-relaxed border-t border-border">
                    {section.content}
                  </div>
                )}
              </div>
            )
          })}

          {/* Footer with links */}
          <div className="mt-8 pt-4 border-t border-border">
            <p className="text-sm text-muted-foreground text-center">
              For more information, visit the{' '}
              <a
                href="https://github.com/anthropics/cowork"
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:text-primary/80 inline-flex items-center gap-1 transition-colors"
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
