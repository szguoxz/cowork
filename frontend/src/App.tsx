import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import Layout from './components/Layout'
import Chat from './pages/Chat'
import Files from './pages/Files'
import Settings from './pages/Settings'
import Mcp from './pages/Mcp'
import Skills from './pages/Skills'
import Sessions from './pages/Sessions'
import Help from './pages/Help'
import Onboarding from './components/Onboarding'

function App() {
  const [showOnboarding, setShowOnboarding] = useState<boolean | null>(null)

  // Apply dark mode based on system preference
  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')

    const updateTheme = (e: MediaQueryListEvent | MediaQueryList) => {
      if (e.matches) {
        document.documentElement.classList.add('dark')
      } else {
        document.documentElement.classList.remove('dark')
      }
    }

    updateTheme(mediaQuery)
    mediaQuery.addEventListener('change', updateTheme)

    return () => mediaQuery.removeEventListener('change', updateTheme)
  }, [])

  // Check if onboarding should be shown
  useEffect(() => {
    const checkSetup = async () => {
      try {
        // Check if setup has been completed before (via backend)
        const isSetupDone = await invoke<boolean>('is_setup_complete')

        // Also check localStorage for onboarding completion
        const onboardingComplete = localStorage.getItem('onboarding_complete') === 'true'

        // Show onboarding if either:
        // 1. No API key is configured (backend check)
        // 2. Onboarding was never completed (localStorage check)
        setShowOnboarding(!isSetupDone && !onboardingComplete)
      } catch (err) {
        console.error('Failed to check setup status:', err)
        // On error, check localStorage as fallback
        const onboardingComplete = localStorage.getItem('onboarding_complete') === 'true'
        setShowOnboarding(!onboardingComplete)
      }
    }
    checkSetup()
  }, [])

  const handleOnboardingComplete = () => {
    localStorage.setItem('onboarding_complete', 'true')
    setShowOnboarding(false)
  }

  // Show loading state while checking setup status
  if (showOnboarding === null) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-gray-900 flex items-center justify-center">
        <div className="w-8 h-8 border-4 border-primary-600 border-t-transparent rounded-full animate-spin" />
      </div>
    )
  }

  return (
    <BrowserRouter>
      {showOnboarding && <Onboarding onComplete={handleOnboardingComplete} />}
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<Chat />} />
          <Route path="files" element={<Files />} />
          <Route path="sessions" element={<Sessions />} />
          <Route path="mcp" element={<Mcp />} />
          <Route path="skills" element={<Skills />} />
          <Route path="settings" element={<Settings />} />
          <Route path="help" element={<Help />} />
        </Route>
      </Routes>
    </BrowserRouter>
  )
}

export default App
