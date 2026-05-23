// components/StorageBrowser.tsx - 通用云端目录浏览器（支持 Alist/WebDAV/S3 目录树交互）
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
import { storageListDir, loadConfig, saveConfig } from '../services/tauri'
import type { RemoteFileEntry, StorageConfig } from '../services/tauri'
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

interface StorageBrowserProps {
  /** 临时未存盘的云端存储配置（在设置向导预览时传入以实时浏览，未传则默认采用已保存的激活后端） */
  tempConfig?: StorageConfig
}

export default function StorageBrowser({ tempConfig }: StorageBrowserProps) {
  const styles = useStyles()
  const { addToast } = useAppStore()
  const [path, setPath] = useState('/')
  const [entries, setEntries] = useState<RemoteFileEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [backupRoot, setBackupRoot] = useState<string>('')

  // 载入当前激活后端的全局云盘物理备份根路径
  const fetchBackupRoot = useCallback(async () => {
    try {
      const config = await loadConfig()
      let root = ''
      
      // 如果传入了临时配置，优先获取临时配置的根路径以供 UI 渲染
      if (tempConfig) {
        if (tempConfig.type === 'alist') root = tempConfig.backup_root ?? ''
        else if (tempConfig.type === 'webdav') root = tempConfig.backup_root ?? ''
        else if (tempConfig.type === 's3') root = tempConfig.backup_root ?? ''
      } else if (config.storage) {
        // 否则加载已经存盘配置项的根路径
        if (config.storage.type === 'alist') root = config.storage.backup_root ?? ''
        else if (config.storage.type === 'webdav') root = config.storage.backup_root ?? ''
        else if (config.storage.type === 's3') root = config.storage.backup_root ?? ''
      } else if (config.alist) {
        // 向下兼容 fallback
        root = config.alist.backup_root ?? ''
      }
      
      setBackupRoot(root)
    } catch (err) {
      console.error('加载备份根路径配置失败', err)
    }
  }, [tempConfig])

  // 列出云端目录内容
  const loadDir = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      // 传入临时配置或 undefined，由后端工厂智能构建具体分发实例
      const data = await storageListDir(path, tempConfig)
      setEntries(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [path, tempConfig])

  useEffect(() => {
    loadDir()
    fetchBackupRoot()
  }, [path, loadDir, fetchBackupRoot])

  // 一键将当前目录设为该存储后端的云端备份物理根目录，实现动态备份路径锁定
  const handleSetBackupRoot = async () => {
    try {
      const config = await loadConfig()
      
      // 1. 如果是临时配置模式，我们需要提醒用户先保存整个连接表单
      if (tempConfig) {
        addToast(`测试模式下，请先点击下方的“保存设置”以应用该配置`, 'warning')
        return
      }

      // 2. 否则，获取已激活的存储变体并修改
      if (!config.storage) {
        // 向下兼容迁移：如果磁盘中还只是老版的 alist 字段
        if (config.alist) {
          config.alist.backup_root = path
          await saveConfig(config)
          setBackupRoot(path)
          addToast(`云端备份根路径已成功设置为: ${path}`, 'success')
          return
        }
        addToast('请先配置并保存云端存储连接配置', 'error')
        return
      }

      // 根据不同的激活变体注入根路径
      switch (config.storage.type) {
        case 'alist':
          config.storage.backup_root = path
          break;
        case 'webdav':
          config.storage.endpoint = config.storage.endpoint.replace(/\/+$/, '') // 清洗 endpoint 的物理尾部斜杠
          config.storage.backup_root = path
          break;
        case 's3':
          config.storage.backup_root = path
          break;
      }

      await saveConfig(config)
      setBackupRoot(path)
      addToast(`云端备份根路径已成功设置为: ${path}`, 'success')
    } catch (err) {
      addToast(err instanceof Error ? err.message : '设置失败', 'error')
    }
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
            {entries.length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} style={{ textAlign: 'center', padding: '24px 0', color: '#888' }}>
                  该目录下没有任何文件或文件夹 (空文件夹)
                </TableCell>
              </TableRow>
            ) : (
              entries.map((entry) => (
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
              ))
            )}
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
