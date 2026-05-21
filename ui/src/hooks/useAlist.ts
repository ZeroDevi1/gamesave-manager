// hooks/useAlist.ts - Alist 连接状态管理
import { useState, useEffect, useCallback } from 'react'
import { loadConfig, saveConfig, alistLogin } from '../services/tauri'
import { useAppStore } from '../store/appStore'

export interface AlistState {
  connected: boolean
  url: string
  username: string
  token?: string
  provider: string
}

export function useAlist() {
  const { addToast } = useAppStore()
  const [state, setState] = useState<AlistState>({
    connected: false,
    url: '',
    username: '',
    provider: 'alist',
  })
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // 从配置加载 Alist 状态
  useEffect(() => {
    loadConfig().then((config) => {
      if (config.alist) {
        setState({
          connected: !!config.alist.token,
          url: config.alist.base_url,
          username: config.alist.username,
          token: config.alist.token,
          provider: config.alist.provider,
        })
      }
    })
  }, [])

  const login = useCallback(
    async (url: string, username: string, password: string) => {
      setLoading(true)
      setError(null)
      try {
        const result = await alistLogin({ url, username, password })

        // 保存配置
        const config = await loadConfig()
        config.alist = {
          base_url: url,
          username,
          token: result.token,
          provider: 'alist',
        }
        await saveConfig(config)

        setState({
          connected: true,
          url,
          username,
          token: result.token,
          provider: 'alist',
        })

        addToast('Alist 登录成功', 'success')
        return result
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        setError(msg)
        addToast(`登录失败: ${msg}`, 'error')
        throw err
      } finally {
        setLoading(false)
      }
    },
    [addToast],
  )

  const disconnect = useCallback(async () => {
    const config = await loadConfig()
    if (config.alist) {
      config.alist.token = undefined
      await saveConfig(config)
    }
    setState((prev) => ({ ...prev, connected: false, token: undefined }))
    addToast('已断开 Alist 连接', 'info')
  }, [addToast])

  return { ...state, loading, error, login, disconnect }
}
