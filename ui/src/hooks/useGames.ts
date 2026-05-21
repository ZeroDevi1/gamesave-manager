// hooks/useGames.ts - 游戏列表 CRUD
import { useState, useEffect, useCallback } from 'react'
import { getGames, addGame, removeGame } from '../services/tauri'
import type { GameConfig } from '../services/tauri'
import { useAppStore } from '../store/appStore'

export function useGames() {
  const { addToast } = useAppStore()
  const [games, setGames] = useState<GameConfig[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const data = await getGames()
      setGames(data)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setError(msg)
      addToast(msg, 'error')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh()
  }, [refresh])

  const add = useCallback(async (name: string, savePaths: string[]) => {
    const game = await addGame(name, savePaths)
    setGames((prev) => [...prev, game])
    return game
  }, [])

  const remove = useCallback(async (gameId: string) => {
    await removeGame(gameId)
    setGames((prev) => prev.filter((g) => g.id !== gameId))
  }, [])

  return { games, loading, error, refresh, add, remove }
}
