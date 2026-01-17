import React, { useState } from 'react'
import {
  X,
  Keyboard,
  Terminal,
  FileText,
  FolderOpen,
  Globe,
  GitBranch,
  Search,
  Edit3,
  ChevronDown,
  ChevronRight,
  HelpCircle,
  Lightbulb,
} from 'lucide-react'

interface HelpPanelProps {
  isOpen: boolean
  onClose: () => void
}

interface HelpSection {
  id: string
  title: string
  icon: React.ReactNode
  content: React.ReactNode
}

export default function HelpPanel({ isOpen, onClose }: HelpPanelProps) {
  const [expandedSection, setExpandedSection] = useState<string | null>('commands')

  if (!isOpen) return null

  const sections: HelpSection[] = [
    {
      id: 'commands',
      title: 'Slash Commands',
      icon: <Terminal className="w-5 h-5" />,
      content: (
        <div className="space-y-4">
          <CommandHelp
            command="/commit"
            description="Stage changes and create a commit with an auto-generated message"
            example="/commit fix the login bug"
          />
          <CommandHelp
            command="/push"
            description="Push commits to the remote repository"
            example="/push origin main"
          />
          <CommandHelp
            command="/pr"
            description="Create a pull request with auto-generated description"
            example="/pr Add user authentication"
          />
          <CommandHelp
            command="/review"
            description="Review staged changes and get feedback"
            example="/review"
          />
          <CommandHelp
            command="/help"
            description="Show available commands"
            example="/help"
          />
        </div>
      ),
    },
    {
      id: 'keyboard',
      title: 'Keyboard Shortcuts',
      icon: <Keyboard className="w-5 h-5" />,
      content: (
        <div className="space-y-3">
          <ShortcutHelp keys={['Y']} description="Approve all pending tool calls" />
          <ShortcutHelp keys={['N']} description="Reject all pending tool calls" />
          <ShortcutHelp keys={['Escape']} description="Cancel current operation" />
          <ShortcutHelp keys={['Ctrl', 'Enter']} description="Send message" />
        </div>
      ),
    },
    {
      id: 'tools',
      title: 'Available Tools',
      icon: <FileText className="w-5 h-5" />,
      content: (
        <div className="space-y-4">
          <ToolHelp
            icon={<FolderOpen className="w-4 h-4" />}
            name="Read/Write Files"
            description="Read, write, and modify files in your project"
          />
          <ToolHelp
            icon={<Edit3 className="w-4 h-4" />}
            name="Edit"
            description="Make surgical edits to files with exact string replacement"
          />
          <ToolHelp
            icon={<Search className="w-4 h-4" />}
            name="Glob & Grep"
            description="Search for files by pattern and content"
          />
          <ToolHelp
            icon={<Terminal className="w-4 h-4" />}
            name="Shell Commands"
            description="Execute shell commands (requires approval)"
          />
          <ToolHelp
            icon={<Globe className="w-4 h-4" />}
            name="Web Fetch"
            description="Retrieve and process web content"
          />
          <ToolHelp
            icon={<GitBranch className="w-4 h-4" />}
            name="Git Operations"
            description="Stage, commit, push, and manage git history"
          />
        </div>
      ),
    },
    {
      id: 'tips',
      title: 'Tips & Best Practices',
      icon: <Lightbulb className="w-5 h-5" />,
      content: (
        <div className="space-y-4 text-sm">
          <TipCard title="Be Specific">
            The more context you provide, the better results you'll get. Include file paths,
            function names, and expected behavior.
          </TipCard>
          <TipCard title="Review Tool Calls">
            Always review tool calls before approving, especially for write operations. Check the
            file paths and content changes.
          </TipCard>
          <TipCard title="Use Slash Commands">
            For common workflows like committing or creating PRs, use slash commands for
            streamlined experiences.
          </TipCard>
          <TipCard title="Add CLAUDE.md">
            Create a CLAUDE.md file in your project root with project-specific instructions and
            context.
          </TipCard>
        </div>
      ),
    },
  ]

  return (
    <div className="fixed inset-0 bg-gray-900/50 backdrop-blur-sm flex items-start justify-end z-50">
      <div className="h-full w-full max-w-md bg-white dark:bg-gray-800 shadow-xl overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-3">
            <HelpCircle className="w-6 h-6 text-primary-600" />
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white">Help</h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
          >
            <X className="w-5 h-5 text-gray-500" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto">
          {sections.map((section) => (
            <div key={section.id} className="border-b border-gray-200 dark:border-gray-700">
              <button
                onClick={() =>
                  setExpandedSection(expandedSection === section.id ? null : section.id)
                }
                className="w-full flex items-center justify-between px-6 py-4 hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors"
              >
                <div className="flex items-center gap-3">
                  <div className="text-primary-600">{section.icon}</div>
                  <span className="font-medium text-gray-900 dark:text-white">
                    {section.title}
                  </span>
                </div>
                {expandedSection === section.id ? (
                  <ChevronDown className="w-5 h-5 text-gray-400" />
                ) : (
                  <ChevronRight className="w-5 h-5 text-gray-400" />
                )}
              </button>
              {expandedSection === section.id && (
                <div className="px-6 pb-4">{section.content}</div>
              )}
            </div>
          ))}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-700/50">
          <p className="text-xs text-gray-500 dark:text-gray-400 text-center">
            Press <kbd className="px-1.5 py-0.5 bg-gray-200 dark:bg-gray-600 rounded text-xs">?</kbd> anytime to open this help panel
          </p>
        </div>
      </div>
    </div>
  )
}

interface CommandHelpProps {
  command: string
  description: string
  example?: string
}

function CommandHelp({ command, description, example }: CommandHelpProps) {
  return (
    <div className="bg-gray-50 dark:bg-gray-700/50 rounded-lg p-3">
      <code className="text-primary-600 font-medium">{command}</code>
      <p className="text-sm text-gray-600 dark:text-gray-400 mt-1">{description}</p>
      {example && (
        <p className="text-xs text-gray-500 dark:text-gray-500 mt-2">
          Example: <code className="text-gray-700 dark:text-gray-300">{example}</code>
        </p>
      )}
    </div>
  )
}

interface ShortcutHelpProps {
  keys: string[]
  description: string
}

function ShortcutHelp({ keys, description }: ShortcutHelpProps) {
  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-1">
        {keys.map((key, idx) => (
          <React.Fragment key={key}>
            {idx > 0 && <span className="text-gray-400">+</span>}
            <kbd className="px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm font-mono text-gray-700 dark:text-gray-300 border border-gray-200 dark:border-gray-600">
              {key}
            </kbd>
          </React.Fragment>
        ))}
      </div>
      <span className="text-sm text-gray-600 dark:text-gray-400">{description}</span>
    </div>
  )
}

interface ToolHelpProps {
  icon: React.ReactNode
  name: string
  description: string
}

function ToolHelp({ icon, name, description }: ToolHelpProps) {
  return (
    <div className="flex items-start gap-3">
      <div className="p-2 bg-gray-100 dark:bg-gray-700 rounded-lg text-gray-500">{icon}</div>
      <div>
        <div className="font-medium text-gray-900 dark:text-white text-sm">{name}</div>
        <p className="text-sm text-gray-500 dark:text-gray-400">{description}</p>
      </div>
    </div>
  )
}

interface TipCardProps {
  title: string
  children: React.ReactNode
}

function TipCard({ title, children }: TipCardProps) {
  return (
    <div className="bg-primary-50 dark:bg-primary-900/20 rounded-lg p-3 border-l-4 border-primary-500">
      <div className="font-medium text-primary-900 dark:text-primary-100 text-sm mb-1">
        {title}
      </div>
      <p className="text-primary-700 dark:text-primary-300 text-sm">{children}</p>
    </div>
  )
}
