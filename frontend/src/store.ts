import { create } from 'zustand'

interface Message {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: Date
}

interface AppState {
  messages: Message[]
  isLoading: boolean
  currentTask: string | null
  addMessage: (message: Message) => void
  clearMessages: () => void
  setLoading: (loading: boolean) => void
  setCurrentTask: (taskId: string | null) => void
}

export const useStore = create<AppState>((set) => ({
  messages: [],
  isLoading: false,
  currentTask: null,

  addMessage: (message) =>
    set((state) => ({
      messages: [...state.messages, message],
    })),

  clearMessages: () => set({ messages: [] }),

  setLoading: (loading) => set({ isLoading: loading }),

  setCurrentTask: (taskId) => set({ currentTask: taskId }),
}))
