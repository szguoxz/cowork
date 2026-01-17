import { CheckCircle2, Circle, Loader2, ListTodo } from 'lucide-react'

interface TodoItem {
  content: string
  status: 'pending' | 'in_progress' | 'completed'
  activeForm: string
}

interface TodoPanelProps {
  todos: TodoItem[]
  collapsed?: boolean
  onToggleCollapse?: () => void
}

export default function TodoPanel({ todos, collapsed = false, onToggleCollapse }: TodoPanelProps) {
  if (todos.length === 0) {
    return null
  }

  const completed = todos.filter((t) => t.status === 'completed').length
  const inProgress = todos.filter((t) => t.status === 'in_progress').length
  const pending = todos.filter((t) => t.status === 'pending').length
  const progress = todos.length > 0 ? (completed / todos.length) * 100 : 0

  if (collapsed) {
    return (
      <button
        onClick={onToggleCollapse}
        className="
          fixed right-4 top-20
          flex items-center gap-2 px-3 py-2
          bg-white dark:bg-gray-800
          border border-gray-300 dark:border-gray-600
          rounded-lg shadow-lg
          hover:bg-gray-50 dark:hover:bg-gray-700
          transition-colors z-40
        "
      >
        <ListTodo className="w-4 h-4 text-gray-500" />
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
          {completed}/{todos.length}
        </span>
        <div className="w-16 h-1.5 bg-gray-200 dark:bg-gray-600 rounded-full overflow-hidden">
          <div
            className="h-full bg-green-500 transition-all duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>
      </button>
    )
  }

  return (
    <div
      className="
        fixed right-4 top-20
        w-72
        bg-white dark:bg-gray-800
        border border-gray-300 dark:border-gray-600
        rounded-lg shadow-lg
        z-40
      "
    >
      {/* Header */}
      <div
        className="
          flex items-center justify-between
          px-4 py-3
          border-b border-gray-200 dark:border-gray-700
          cursor-pointer
        "
        onClick={onToggleCollapse}
      >
        <div className="flex items-center gap-2">
          <ListTodo className="w-5 h-5 text-primary-600" />
          <span className="font-medium text-gray-900 dark:text-white">Tasks</span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-gray-500">
            {completed}/{todos.length}
          </span>
          <div className="w-12 h-1.5 bg-gray-200 dark:bg-gray-600 rounded-full overflow-hidden">
            <div
              className="h-full bg-green-500 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      </div>

      {/* Todo list */}
      <div className="max-h-80 overflow-y-auto">
        {todos.map((todo, idx) => (
          <div
            key={idx}
            className={`
              flex items-start gap-3 px-4 py-2.5
              border-b border-gray-100 dark:border-gray-700 last:border-b-0
              ${todo.status === 'in_progress' ? 'bg-primary-50 dark:bg-primary-900/20' : ''}
            `}
          >
            <div className="mt-0.5">
              {todo.status === 'completed' ? (
                <CheckCircle2 className="w-4 h-4 text-green-500" />
              ) : todo.status === 'in_progress' ? (
                <Loader2 className="w-4 h-4 text-primary-500 animate-spin" />
              ) : (
                <Circle className="w-4 h-4 text-gray-300 dark:text-gray-600" />
              )}
            </div>
            <div className="flex-1 min-w-0">
              <p
                className={`
                  text-sm
                  ${
                    todo.status === 'completed'
                      ? 'text-gray-400 dark:text-gray-500 line-through'
                      : todo.status === 'in_progress'
                      ? 'text-primary-700 dark:text-primary-300 font-medium'
                      : 'text-gray-700 dark:text-gray-300'
                  }
                `}
              >
                {todo.status === 'in_progress' ? todo.activeForm : todo.content}
              </p>
            </div>
          </div>
        ))}
      </div>

      {/* Summary */}
      <div className="px-4 py-2 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-700/50 rounded-b-lg">
        <div className="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400">
          <span className="flex items-center gap-1">
            <CheckCircle2 className="w-3 h-3 text-green-500" />
            {completed} done
          </span>
          {inProgress > 0 && (
            <span className="flex items-center gap-1">
              <Loader2 className="w-3 h-3 text-primary-500 animate-spin" />
              {inProgress} running
            </span>
          )}
          {pending > 0 && (
            <span className="flex items-center gap-1">
              <Circle className="w-3 h-3 text-gray-400" />
              {pending} pending
            </span>
          )}
        </div>
      </div>
    </div>
  )
}
