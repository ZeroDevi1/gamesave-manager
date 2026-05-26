// hooks/useBackup.ts - 备份操作封装（含本地 + 远程备份 + 全局变更检测）
import { useState, useCallback, useMemo } from 'react'
import {
  backupFull,
  backupIncremental,
  restoreBackup,
  getBackupHistory,
  listRemoteBackups,
  restoreRemoteBackup,
  checkAllGamesForChanges,
  backupAllChangedGames,
} from '../services/tauri'

import type {
  BackupResult,
  RestoreResult,
  BackupManifest,
  RemoteBackupEntry,
} from '../services/tauri'
import { useAppStore } from '../store/appStore'

export function useBackup(gameId?: string) {
  const { addToast } = useAppStore()
  const [backingUp, setBackingUp] = useState(false)
  const [restoring, setRestoring] = useState(false)
  const [history, setHistory] = useState<BackupManifest[]>([])
  const [remoteBackups, setRemoteBackups] = useState<RemoteBackupEntry[]>([])
  const [loadingRemote, setLoadingRemote] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const fetchHistory = useCallback(async () => {
    if (!gameId) return
    setError(null)
    try {
      const data = await getBackupHistory(gameId)
      setHistory(data)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setError(msg)
      addToast(msg, 'error')
    }
  }, [gameId, addToast])

  const fetchRemoteBackups = useCallback(async () => {
    if (!gameId) return
    setLoadingRemote(true)
    setError(null)
    try {
      const data = await listRemoteBackups(gameId)
      setRemoteBackups(data)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setError(msg)
      addToast(msg, 'error')
    } finally {
      setLoadingRemote(false)
    }
  }, [gameId, addToast])

  const fullBackup = useCallback(async () => {
    if (!gameId) return
    setBackingUp(true)
    setError(null)
    try {
      const result: BackupResult = await backupFull(gameId)
      addToast(result.message, 'success')
      await fetchHistory()
      await fetchRemoteBackups()
      return result
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setError(msg)
      addToast(msg, 'error')
      throw err
    } finally {
      setBackingUp(false)
    }
  }, [gameId, fetchHistory, fetchRemoteBackups, addToast])

  const incrementalBackup = useCallback(async () => {
    if (!gameId) return
    setBackingUp(true)
    setError(null)
    try {
      const result: BackupResult = await backupIncremental(gameId)
      addToast(result.message, 'success')
      await fetchHistory()
      await fetchRemoteBackups()
      return result
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setError(msg)
      addToast(msg, 'error')
      throw err
    } finally {
      setBackingUp(false)
    }
  }, [gameId, fetchHistory, fetchRemoteBackups, addToast])

  const restore = useCallback(
    async (backupTimestamp: string) => {
      if (!gameId) return
      setRestoring(true)
      setError(null)
      try {
        const result: RestoreResult = await restoreBackup(gameId, backupTimestamp)
        addToast(result.message, 'success')
        return result
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        setError(msg)
        addToast(msg, 'error')
        throw err
      } finally {
        setRestoring(false)
      }
    },
    [gameId, addToast],
  )

  const restoreRemote = useCallback(
    async (remoteZipPath: string) => {
      if (!gameId) return
      setRestoring(true)
      setError(null)
      try {
        const result: RestoreResult = await restoreRemoteBackup(gameId, remoteZipPath)
        addToast(result.message, 'success')
        return result
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        setError(msg)
        addToast(msg, 'error')
        throw err
      } finally {
        setRestoring(false)
      }
    },
    [gameId, addToast],
  )

  return useMemo(
    () => ({
      backingUp,
      restoring,
      history,
      remoteBackups,
      loadingRemote,
      error,
      fetchHistory,
      fetchRemoteBackups,
      fullBackup,
      incrementalBackup,
      restore,
      restoreRemote,
    }),
    [
      backingUp,
      restoring,
      history,
      remoteBackups,
      loadingRemote,
      error,
      fetchHistory,
      fetchRemoteBackups,
      fullBackup,
      incrementalBackup,
      restore,
      restoreRemote,
    ],
  )
}

/** 全局变更检测与批量备份 Hook（无需 gameId） */
export function useChangeDetection() {
  const { addToast } = useAppStore()
  const [checking, setChecking] = useState(false)
  const [backingUpAll, setBackingUpAll] = useState(false)
  const [changedGames, setChangedGames] = useState<[string, number][]>([])

  /** 检查所有游戏的变更 */
  const checkChanges = useCallback(async () => {
    setChecking(true)
    try {
      const changes = await checkAllGamesForChanges()
      setChangedGames(changes)
      if (changes.length === 0) {
        addToast('所有游戏存档均为最新，无变更', 'success')
      } else {
        const totalFiles = changes.reduce((sum, [, count]) => sum + count, 0)
        addToast(
          `检测到 ${changes.length} 个游戏有 ${totalFiles} 个文件变更`,
          'warning',
        )
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      addToast(`变更检测失败: ${msg}`, 'error')
    } finally {
      setChecking(false)
    }
  }, [addToast])

  /** 一键备份所有有变更的游戏 */
  const backupAll = useCallback(async () => {
    setBackingUpAll(true)
    try {
      const results = await backupAllChangedGames()
      const successCount = results.filter(([, ok]) => ok).length
      const failCount = results.length - successCount
      if (results.length === 0) {
        addToast('没有需要备份的游戏', 'info')
      } else {
        addToast(
          `批量备份完成：${successCount} 成功${failCount > 0 ? `，${failCount} 失败` : ''}`,
          failCount > 0 ? 'warning' : 'success',
        )
      }
      setChangedGames([])
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      addToast(`批量备份失败: ${msg}`, 'error')
    } finally {
      setBackingUpAll(false)
    }
  }, [addToast])

  return useMemo(
    () => ({
      checking,
      backingUpAll,
      changedGames,
      checkChanges,
      backupAll,
    }),
    [checking, backingUpAll, changedGames, checkChanges, backupAll],
  )
}
