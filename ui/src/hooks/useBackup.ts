// hooks/useBackup.ts - 备份操作封装（含本地 + 远程备份）
import { useState, useCallback } from 'react'
import {
  backupFull,
  backupIncremental,
  restoreBackup,
  getBackupHistory,
  listRemoteBackups,
  restoreRemoteBackup,
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

  return {
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
  }
}
