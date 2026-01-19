import { Outlet, Link, useLocation } from 'react-router-dom'
import { MessageSquare, FolderOpen, Settings, Bot, Server, Puzzle, ChevronLeft, ChevronRight, History, HelpCircle } from 'lucide-react'
import { useState } from 'react'

export default function Layout() {
  const location = useLocation()
  const [collapsed, setCollapsed] = useState(false)

  const navItems = [
    { path: '/', icon: MessageSquare, label: 'Chat' },
    { path: '/sessions', icon: History, label: 'History' },
    { path: '/files', icon: FolderOpen, label: 'Files' },
    { path: '/mcp', icon: Server, label: 'MCP Servers' },
    { path: '/skills', icon: Puzzle, label: 'Skills' },
    { path: '/settings', icon: Settings, label: 'Settings' },
    { path: '/help', icon: HelpCircle, label: 'Help' },
  ]

  return (
    <div className="flex h-screen bg-background">
      {/* Sidebar */}
      <aside className={`
        ${collapsed ? 'w-16' : 'w-56'}
        bg-card border-r border-border flex flex-col
        transition-all duration-300 ease-in-out
      `}>
        {/* Logo */}
        <div className={`
          h-14 border-b border-border flex items-center
          ${collapsed ? 'justify-center px-2' : 'px-4 gap-3'}
        `}>
          <div className="w-9 h-9 rounded-lg bg-primary flex items-center justify-center shrink-0">
            <Bot className="w-5 h-5 text-primary-foreground" />
          </div>
          {!collapsed && (
            <span className="font-semibold text-foreground">Cowork</span>
          )}
        </div>

        {/* Navigation */}
        <nav className="flex-1 p-2 space-y-1">
          {navItems.map((item) => {
            const Icon = item.icon
            const isActive = location.pathname === item.path

            return (
              <Link
                key={item.path}
                to={item.path}
                className={`
                  flex items-center gap-3 px-3 py-2.5 rounded-lg
                  transition-colors duration-200
                  ${isActive
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:text-foreground hover:bg-accent'
                  }
                  ${collapsed ? 'justify-center' : ''}
                `}
                title={collapsed ? item.label : undefined}
              >
                <Icon className="w-5 h-5 shrink-0" />
                {!collapsed && (
                  <span className="text-sm font-medium">{item.label}</span>
                )}
              </Link>
            )
          })}
        </nav>

        {/* Collapse button */}
        <div className="p-2 border-t border-border">
          <button
            onClick={() => setCollapsed(!collapsed)}
            className={`
              flex items-center gap-3 px-3 py-2.5 rounded-lg w-full
              text-muted-foreground hover:text-foreground hover:bg-accent
              transition-colors duration-200
              ${collapsed ? 'justify-center' : ''}
            `}
            title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          >
            {collapsed ? (
              <ChevronRight className="w-5 h-5" />
            ) : (
              <>
                <ChevronLeft className="w-5 h-5" />
                <span className="text-sm font-medium">Collapse</span>
              </>
            )}
          </button>
        </div>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  )
}
