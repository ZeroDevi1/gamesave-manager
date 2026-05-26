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
  tokens,
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
  FolderOpen24Regular,
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
  // 表格行 hover 高亮效果
  tableRow: {
    transition: 'background-color 0.15s ease',
    ':hover': {
      backgroundColor: tokens.colorNeutralBackground1Hover,
    },
  },
  // 斑马纹交替背景
  tableRowEven: {
    backgroundColor: tokens.colorNeutralBackground2,
  },
  // 空状态居中展示
  emptyState: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    padding: '40px 0',
    gap: '12px',
    color: tokens.colorNeutralForeground3,
  },
  emptyIcon: {
    fontSize: '36px',
    opacity: 0.35,
  },
})

interface StorageBrowserProps {
  /** 临时未存盘的云端存储配置（在设置向导预览时传入以实时浏览，未传则默认采用已保存的激活后端） */
  tempConfig?: StorageConfig
  /** 备份根路径变更回调：父组件通过此回调同步维护独立的 backupRoot 状态，打破 tempConfig 模式下的保存死锁 */
  onBackupRootChange?: (newRoot: string) => void
}

export default function StorageBrowser({ tempConfig, onBackupRootChange }: StorageBrowserProps) {
  const styles = useStyles()
  const { addToast } = useAppStore()
  const [entries, setEntries] = useState<RemoteFileEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [backupRoot, setBackupRoot] = useState<string>('')
  // 面包屑导航：name 用于显示，path(fid) 用于后端请求
  const [breadcrumbs, setBreadcrumbs] = useState<{ name: string; path: string }[]>([
    { name: '/', path: '/' },
  ])
  const displayPath = breadcrumbs.map((b) => b.name).join('/').replace(/\/\//g, '/')
  const currentPath = breadcrumbs[breadcrumbs.length - 1]?.path ?? '/'

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
        else if (tempConfig.type === 'netdisk') root = tempConfig.backup_root ?? ''
      } else if (config.storage) {
        // 否则加载已经存盘配置项的根路径
        if (config.storage.type === 'alist') root = config.storage.backup_root ?? ''
        else if (config.storage.type === 'webdav') root = config.storage.backup_root ?? ''
        else if (config.storage.type === 's3') root = config.storage.backup_root ?? ''
        else if (config.storage.type === 'netdisk') root = config.storage.backup_root ?? ''
      } else if (config.alist) {
        // 向下兼容 fallback
        root = config.alist.backup_root ?? ''
      }
      
      setBackupRoot(root)
    } catch (err) {
      console.error('加载备份根路径配置失败', err)
    }
  }, [tempConfig])

  // 序列化配置为字符串，彻底根除因 tempConfig 属性对象引用不断生成导致的 useEffect 死循环自己刷新的 Bug
  const tempConfigStr = tempConfig ? JSON.stringify(tempConfig) : '';

  // 列出云端目录内容
  const loadDir = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const cfg: StorageConfig | undefined = tempConfigStr ? JSON.parse(tempConfigStr) : undefined;
      // 传入临时配置或 undefined，由后端工厂智能构建具体分发实例
      const data = await storageListDir(currentPath, cfg)
      setEntries(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [currentPath, tempConfigStr])

  useEffect(() => {
    loadDir()
    fetchBackupRoot()
  }, [currentPath, loadDir, fetchBackupRoot])

  // 一键将当前目录设为该存储后端的云端备份物理根目录，实现动态备份路径锁定
  const handleSetBackupRoot = async () => {
    try {
      // 临时配置模式：通过回调将 backupRoot 上报给父组件（SettingsPanel），
      // 待用户保存设置后一并持久化，打破 tempConfig 模式下的保存死锁
      if (tempConfig) {
        setBackupRoot(displayPath)
        onBackupRootChange?.(displayPath)
        addToast(`备份根路径已暂存为: ${displayPath}（点击下方"保存设置"后生效）`, 'success')
        return
      }

      const config = await loadConfig()
      
      // 如果配置尚未保存过
      if (!config.storage) {
        if (config.alist) {
          config.alist.backup_root = displayPath
          await saveConfig(config)
          setBackupRoot(displayPath)
          onBackupRootChange?.(displayPath)
          addToast(`云端备份根路径已成功设置为: ${displayPath}`, 'success')
          return
        }
        addToast('请先配置并保存云端存储连接配置', 'error')
        return
      }

      // 根据不同的激活变体注入根路径
      switch (config.storage.type) {
        case 'alist':
        case 'netdisk':
        case 's3':
          config.storage.backup_root = displayPath
          break;
        case 'webdav':
          config.storage.endpoint = config.storage.endpoint.replace(/\/+$/, '')
          config.storage.backup_root = displayPath
          break;
      }

      await saveConfig(config)
      setBackupRoot(displayPath)
      onBackupRootChange?.(displayPath)
      addToast(`云端备份根路径已成功设置为: ${displayPath}`, 'success')
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
          disabled={breadcrumbs.length <= 1}
          onClick={() => setBreadcrumbs((prev) => prev.slice(0, -1))}
        />
        <Input value={displayPath} readOnly style={{ flexGrow: 1 }} />
        <Button
          icon={<ArrowSync24Regular />}
          size="small"
          onClick={loadDir}
          disabled={loading}
          title="刷新目录"
        />
        <Button
          icon={displayPath === backupRoot ? <Checkmark24Regular /> : <Save24Regular />}
          appearance={displayPath === backupRoot ? 'primary' : 'outline'}
          size="small"
          onClick={handleSetBackupRoot}
          disabled={loading}
          title="将当前进入的网盘文件夹设为游戏备份的归档根目录"
          style={displayPath === backupRoot ? { backgroundColor: '#107c41', color: 'white', borderColor: '#107c41' } : {}}
        >
          {displayPath === backupRoot ? '已设为备份根目录' : '设为备份根目录'}
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
                <TableCell colSpan={3}>
                  <div className={styles.emptyState}>
                    <FolderOpen24Regular className={styles.emptyIcon} />
                    <span>该目录下没有任何文件或文件夹</span>
                    <span style={{ fontSize: '13px' }}>尝试浏览其他目录，或在此创建备份后自动生成</span>
                  </div>
                </TableCell>
              </TableRow>
            ) : (
              entries.map((entry) => (
                <TableRow
                  key={entry.path}
                  className={`${styles.tableRow} ${entries.indexOf(entry) % 2 === 1 ? styles.tableRowEven : ''}`}
                  onClick={() => {
                    if (entry.is_dir) {
                      setBreadcrumbs((prev) => [...prev, { name: entry.name, path: entry.path }])
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
