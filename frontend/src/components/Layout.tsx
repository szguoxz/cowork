import { Outlet, Link, useLocation } from 'react-router-dom'
import { MessageSquare, FolderOpen, Settings, Bot } from 'lucide-react'

export default function Layout() {
  const location = useLocation()

  const navItems = [
    { path: '/', icon: MessageSquare, label: 'Chat' },
    { path: '/files', icon: FolderOpen, label: 'Files' },
    { path: '/settings', icon: Settings, label: 'Settings' },
  ]

  return (
    <div className="flex h-screen">
      {/* Sidebar */}
      <aside className="w-16 bg-gray-900 flex flex-col items-center py-4 gap-2">
        <div className="w-10 h-10 rounded-lg bg-primary-600 flex items-center justify-center mb-4">
          <Bot className="w-6 h-6 text-white" />
        </div>

        {navItems.map((item) => {
          const Icon = item.icon
          const isActive = location.pathname === item.path

          return (
            <Link
              key={item.path}
              to={item.path}
              className={`
                w-10 h-10 rounded-lg flex items-center justify-center
                transition-colors duration-200
                ${isActive
                  ? 'bg-gray-700 text-white'
                  : 'text-gray-400 hover:text-white hover:bg-gray-800'
                }
              `}
              title={item.label}
            >
              <Icon className="w-5 h-5" />
            </Link>
          )
        })}
      </aside>

      {/* Main content */}
      <main className="flex-1 bg-gray-50 dark:bg-gray-900 overflow-hidden">
        <Outlet />
      </main>
    </div>
  )
}
