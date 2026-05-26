// App.tsx - 根组件：FluentProvider + 路由
import {
  FluentProvider,
  Spinner,
} from '@fluentui/react-components'
import { HashRouter, Routes, Route, useLocation } from 'react-router-dom'
import { useEffect, useState } from 'react'
import AppShell from './components/AppShell'
import HomePage from './pages/HomePage'
import GameDetailPage from './pages/GameDetailPage'
import GameDbPage from './pages/GameDbPage'
import SettingsPage from './pages/SettingsPage'
import ToastContainer from './components/ToastContainer'
import { loadConfig } from './services/tauri'
import { useAppStore } from './store/appStore'

/** 路由动画包装器：通过 location key 触发淡入+上移动效 */
function AnimatedRoutes() {
  const location = useLocation()
  return (
    <div key={location.pathname} className="page-enter" style={{ height: '100%' }}>
      <Routes location={location}>
        <Route path="/" element={<HomePage />} />
        <Route path="/game/:gameId" element={<GameDetailPage />} />
        <Route path="/database" element={<GameDbPage />} />
        <Route path="/settings" element={<SettingsPage />} />
      </Routes>
    </div>
  )
}

function App() {
  const { fluentTheme, setThemeMode } = useAppStore()
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    loadConfig()
      .then((config) => {
        const mode = config.settings?.theme ?? 'system'
        setThemeMode(mode as 'system' | 'light' | 'dark')
      })
      .catch(() => {
        // 使用默认系统主题
      })
      .finally(() => setLoading(false))
  }, [setThemeMode])

  if (loading) {
    return (
      <FluentProvider theme={fluentTheme}>
        <div style={{ height: '100vh', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <Spinner label="正在加载..." size="huge" />
        </div>
      </FluentProvider>
    )
  }

  return (
    <FluentProvider theme={fluentTheme}>
      <HashRouter>
        <AppShell>
          <AnimatedRoutes />
        </AppShell>
      </HashRouter>
      <ToastContainer />
    </FluentProvider>
  )
}

export default App
