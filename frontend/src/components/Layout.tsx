import { Outlet, Link, useLocation, useNavigate } from 'react-router-dom'
import { MessageSquare, Settings, Server, Puzzle, ChevronLeft, ChevronRight, History, HelpCircle, Sparkles, Plus } from 'lucide-react'
import { useState } from 'react'
import { useSession } from '../context/SessionContext'

export default function Layout() {
  const location = useLocation()
  const navigate = useNavigate()
  const { createNewSession } = useSession()
  const [collapsed, setCollapsed] = useState(false)

  const handleNewChat = () => {
    createNewSession()
    navigate('/')
  }

  const navItems = [
    { path: '/', icon: Plus, label: 'New Chat', onClick: handleNewChat },
    { path: '/sessions', icon: History, label: 'History' },
    { path: '/mcp', icon: Server, label: 'MCP Servers' },
    { path: '/skills', icon: Puzzle, label: 'Skills' },
    { path: '/settings', icon: Settings, label: 'Settings' },
    { path: '/help', icon: HelpCircle, label: 'Help' },
  ]

  return (
    <div className="flex h-screen bg-background">
      {/* Sidebar */}
      <aside className={`
        ${collapsed ? 'w-16' : 'w-60'}
        bg-card/50 backdrop-blur-sm border-r border-border flex flex-col
        transition-all duration-300 ease-in-out
      `}>
        {/* Logo */}
        <div className={`
          h-14 border-b border-border flex items-center
          ${collapsed ? 'justify-center px-2' : 'px-4 gap-3'}
        `}>
          <div className="relative w-9 h-9 rounded-xl bg-gradient-to-br from-violet-500 to-purple-600 flex items-center justify-center shrink-0 shadow-glow-sm">
            <Sparkles className="w-5 h-5 text-white" />
          </div>
          {!collapsed && (
            <span className="font-semibold text-foreground tracking-tight">Cowork</span>
          )}
        </div>

        {/* Navigation */}
        <nav className="flex-1 p-2 space-y-1">
          {navItems.map((item) => {
            const Icon = item.icon
            const isActive = location.pathname === item.path && !item.onClick

            // Special "New Chat" button
            if (item.onClick) {
              return (
                <button
                  key={item.path}
                  onClick={item.onClick}
                  className={`
                    flex items-center gap-3 px-3 py-2.5 rounded-lg w-full
                    transition-all duration-200
                    text-muted-foreground hover:text-primary hover:bg-primary/10 hover:border-primary/20 border border-transparent
                    ${collapsed ? 'justify-center' : ''}
                  `}
                  title={collapsed ? item.label : undefined}
                >
                  <Icon className="w-5 h-5 shrink-0" />
                  {!collapsed && (
                    <span className="text-sm font-medium">{item.label}</span>
                  )}
                </button>
              )
            }

            return (
              <Link
                key={item.path}
                to={item.path}
                className={`
                  flex items-center gap-3 px-3 py-2.5 rounded-lg
                  transition-all duration-200
                  ${isActive
                    ? 'bg-primary/10 text-primary border border-primary/20 shadow-glow-sm'
                    : 'text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5'
                  }
                  ${collapsed ? 'justify-center' : ''}
                `}
                title={collapsed ? item.label : undefined}
              >
                <Icon className={`w-5 h-5 shrink-0 ${isActive ? 'text-primary' : ''}`} />
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
              text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5
              transition-all duration-200
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
      <main className="flex-1 overflow-hidden bg-background">
        <Outlet />
      </main>
    </div>
  )
}
