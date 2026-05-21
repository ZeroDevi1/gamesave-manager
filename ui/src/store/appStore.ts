// store/appStore.ts - 全局应用状态（主题、Toast）
import { create } from 'zustand'
import { webLightTheme, webDarkTheme } from '@fluentui/react-components'

export type ThemeMode = 'system' | 'light' | 'dark'

interface Toast {
  id: string
  message: string
  intent: 'success' | 'error' | 'warning' | 'info'
}

interface AppState {
  themeMode: ThemeMode
  fluentTheme: typeof webLightTheme
  toasts: Toast[]
  setThemeMode: (mode: ThemeMode) => void
  addToast: (message: string, intent?: Toast['intent']) => void
  removeToast: (id: string) => void
}

function resolveTheme(mode: ThemeMode) {
  if (mode === 'dark') return webDarkTheme
  if (mode === 'light') return webLightTheme
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
  return prefersDark ? webDarkTheme : webLightTheme
}

export const useAppStore = create<AppState>((set) => ({
  themeMode: 'system',
  fluentTheme: resolveTheme('system'),
  toasts: [],

  setThemeMode: (mode) => {
    set({
      themeMode: mode,
      fluentTheme: resolveTheme(mode),
    })
  },

  addToast: (message, intent = 'info') => {
    const id = `${Date.now()}_${Math.random()}`
    set((state) => ({
      toasts: [...state.toasts, { id, message, intent }],
    }))
    // 自动移除
    setTimeout(() => {
      set((state) => ({
        toasts: state.toasts.filter((t) => t.id !== id),
      }))
    }, 4000)
  },

  removeToast: (id) => {
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    }))
  },
}))
