// components/AlistBrowser.tsx - Alist 目录浏览器（设置页内嵌使用）
import {
  Button,
  Input,
  Spinner,
  Table,
  TableHeader,
  TableRow,
  TableHeaderCell,
  TableBody,
  TableCell,
  TableCellLayout,
  makeStyles,
  MessageBar,
  MessageBarBody,
} from '@fluentui/react-components'
import {
  ArrowLeft24Regular,
  Folder24Regular,
  Document24Regular,
  ArrowSync24Regular,
  Checkmark24Regular,
  Save24Regular,
} from '@fluentui/react-icons'
import { useState, useEffect, useCallback } from 'react'
import { useAlist } from '../hooks/useAlist'
import { alistListDir, loadConfig, saveConfig } from '../services/tauri'
import type { AlistFileEntry } from '../services/tauri'
import { useAppStore } from '../store/appStore'

const useStyles = makeStyles({
  root: {
    display: 'flex',
    flexDirection: 'column',
    gap: '12px',
    padding: '16px',
  },
  breadcrumb: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
})

export default function AlistBrowser() {
  const styles = useStyles()
  const { url, token, connected } = useAlist()
  const { addToast } = useAppStore()
  const [path, setPath] = useState('/')
  const [entries, setEntries] = useState<AlistFileEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [backupRoot, setBackupRoot] = useState<string>('')

  // 载入全局配置以拉取云端备份根路径前缀
  const fetchBackupRoot = useCallback(async () => {
    try {
      const config = await loadConfig()
      if (config.alist?.backup_root) {
        setBackupRoot(config.alist.backup_root)
      }
    } catch (err) {
      console.error('加载备份根路径配置失败', err)
    }
  }, [])

  const loadDir = useCallback(async () => {
    if (!connected || !token) return
    setLoading(true)
    setError(null)
    try {
      const data = await alistListDir(url, token, path)
      setEntries(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [connected, url, token, path])

  useEffect(() => {
    if (connected) {
      loadDir()
      fetchBackupRoot()
    }
  }, [connected, loadDir, fetchBackupRoot])

  // 一键将当前目录设为游戏的云端备份根目录，实现动态路由飘移
  const handleSetBackupRoot = async () => {
    try {
      const config = await loadConfig()
      if (!config.alist) {
        addToast('请先配置并保存 Alist 连接配置', 'error')
        return
      }
      config.alist.backup_root = path
      await saveConfig(config)
      setBackupRoot(path)
      addToast(`云端备份根路径已成功设置为: ${path}`, 'success')
    } catch (err) {
      addToast(err instanceof Error ? err.message : '设置失败', 'error')
    }
  }

  if (!connected) {
    return (
      <MessageBar intent="info">
        <MessageBarBody>请先连接 Alist 服务器</MessageBarBody>
      </MessageBar>
    )
  }

  return (
    <div className={styles.root}>
      {backupRoot && (
        <MessageBar intent="success" style={{ padding: '8px 12px', borderRadius: '6px' }}>
          <MessageBarBody>
            当前云端备份根路径锁定为：<strong style={{ color: '#107c41' }}>{backupRoot}</strong>
          </MessageBarBody>
        </MessageBar>
      )}

      <div className={styles.breadcrumb}>
        <Button
          icon={<ArrowLeft24Regular />}
          size="small"
          disabled={path === '/'}
          onClick={() => {
            const parent = path.substring(0, path.lastIndexOf('/')) || '/'
            setPath(parent)
          }}
        />
        <Input value={path} readOnly style={{ flexGrow: 1 }} />
        <Button
          icon={<ArrowSync24Regular />}
          size="small"
          onClick={loadDir}
          disabled={loading}
          title="刷新目录"
        />
        <Button
          icon={path === backupRoot ? <Checkmark24Regular /> : <Save24Regular />}
          appearance={path === backupRoot ? 'primary' : 'outline'}
          size="small"
          onClick={handleSetBackupRoot}
          disabled={loading}
          title="将当前进入的网盘文件夹设为游戏备份的归档根目录"
          style={path === backupRoot ? { backgroundColor: '#107c41', color: 'white', borderColor: '#107c41' } : {}}
        >
          {path === backupRoot ? '已设为备份根目录' : '设为备份根目录'}
        </Button>
      </div>

      {error && (
        <MessageBar intent="error">
          <MessageBarBody>{error}</MessageBarBody>
        </MessageBar>
      )}

      {loading ? (
        <Spinner label="加载中..." />
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHeaderCell>名称</TableHeaderCell>
              <TableHeaderCell>大小</TableHeaderCell>
              <TableHeaderCell>修改时间</TableHeaderCell>
            </TableRow>
          </TableHeader>
          <TableBody>
            {entries.map((entry) => (
              <TableRow
                key={entry.path}
                onClick={() => {
                  if (entry.is_dir) {
                    setPath(entry.path)
                  }
                }}
                style={{ cursor: entry.is_dir ? 'pointer' : 'default' }}
              >
                <TableCell>
                  <TableCellLayout
                    media={
                      entry.is_dir ? (
                        <Folder24Regular />
                      ) : (
                        <Document24Regular />
                      )
                    }
                  >
                    {entry.name}
                  </TableCellLayout>
                </TableCell>
                <TableCell>{entry.is_dir ? '-' : formatBytes(entry.size)}</TableCell>
                <TableCell>{entry.modified || '-'}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  )
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}
