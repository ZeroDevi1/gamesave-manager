// pages/GameDetailPage.tsx - 游戏详情：备份历史、存档列表、操作按钮
import { useParams } from 'react-router-dom'
import {
  makeStyles,
  tokens,
  Title1,
  Title2,
  Button,
  Table,
  TableHeader,
  TableRow,
  TableHeaderCell,
  TableBody,
  TableCell,
  Spinner,
  MessageBar,
  MessageBarBody,
  Badge,
} from '@fluentui/react-components'
import {
  ArrowUpload24Regular,
  ArrowDownload24Regular,
  Delete24Regular,
  Clock24Regular,
  Document24Regular,
  CloudArrowDown24Regular,
} from '@fluentui/react-icons'
import { useEffect, useState } from 'react'
import { useGames } from '../hooks/useGames'
import { useBackup } from '../hooks/useBackup'
import { scanGameSaves, convertFileSrc } from '../services/tauri'
import type { SaveFile } from '../services/tauri'
import BackupDialog from '../components/BackupDialog'
import RestoreDialog from '../components/RestoreDialog'

const useStyles = makeStyles({
  root: {
    padding: '20px',
    display: 'flex',
    flexDirection: 'column',
    gap: '16px',
    height: '100%',
    boxSizing: 'border-box',
    overflowY: 'auto',
    scrollbarWidth: 'none',
    '::-webkit-scrollbar': {
      display: 'none',
    },
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: '16px',
  },
  infoPanel: {
    display: 'flex',
    gap: '20px',
    alignItems: 'flex-start',
  },
  logo: {
    width: '120px',
    height: '120px',
    borderRadius: tokens.borderRadiusLarge,
    objectFit: 'cover',
    backgroundColor: tokens.colorNeutralBackground1,
  },
  placeholder: {
    width: '120px',
    height: '120px',
    borderRadius: tokens.borderRadiusLarge,
    backgroundColor: tokens.colorBrandBackground,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: tokens.colorNeutralForegroundOnBrand,
    fontSize: '48px',
    fontWeight: 'bold',
  },
  info: {
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
    flexGrow: 1,
  },
  pathList: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
    fontFamily: 'monospace',
    fontSize: '13px',
    color: tokens.colorNeutralForeground3,
  },
  section: {
    backgroundColor: tokens.colorNeutralBackground1,
    borderRadius: tokens.borderRadiusMedium,
    padding: '16px',
  },
  sectionHeader: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginBottom: '12px',
  },
  fileNameCell: {
    minWidth: '360px',
    wordBreak: 'break-all',
  },
})

export default function GameDetailPage() {
  const styles = useStyles()
  const { gameId } = useParams<{ gameId: string }>()
  const { games, remove, refresh: refreshGames } = useGames()
  const backup = useBackup(gameId)
  const [saves, setSaves] = useState<SaveFile[]>([])
  const [savesLoading, setSavesLoading] = useState(false)
  const [backupOpen, setBackupOpen] = useState(false)
  const [restoreOpen, setRestoreOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const game = games.find((g) => g.id === gameId)

  useEffect(() => {
    if (!gameId) return
    setSavesLoading(true)
    scanGameSaves(gameId)
      .then(setSaves)
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setSavesLoading(false))
  }, [gameId])

  useEffect(() => {
    if (gameId) {
      backup.fetchHistory()
      backup.fetchRemoteBackups()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [gameId])

  const handleBackup = async (type: 'full' | 'incremental') => {
    try {
      if (type === 'full') {
        await backup.fullBackup()
      } else {
        await backup.incrementalBackup()
      }
      await refreshGames()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBackupOpen(false)
    }
  }

  const handleRestoreLocal = async (timestamp: string) => {
    try {
      await backup.restore(timestamp)
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      throw err
    }
  }

  const handleRestoreRemote = async (remoteZipPath: string) => {
    try {
      await backup.restoreRemote(remoteZipPath)
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      throw err
    }
  }

  if (!game) {
    return (
      <div className={styles.root}>
        <MessageBar intent="warning">
          <MessageBarBody>未找到该游戏</MessageBarBody>
        </MessageBar>
      </div>
    )
  }

  return (
    <div className={styles.root}>
      {error && (
        <MessageBar intent="error">
          <MessageBarBody>{error}</MessageBarBody>
        </MessageBar>
      )}

      {/* 游戏信息面板 */}
      <div className={styles.infoPanel}>
        {game.logo_path ? (
          <img
            src={game.logo_path.startsWith('http') ? game.logo_path : convertFileSrc(game.logo_path)}
            alt={game.name}
            className={styles.logo}
            draggable={false}
            onError={(e) => {
              (e.target as HTMLImageElement).style.display = 'none'
            }}
          />
        ) : (
          <div className={styles.placeholder}>{game.name.charAt(0)}</div>
        )}
        <div className={styles.info}>
          <Title1>{game.name}</Title1>
          <div className={styles.pathList}>
            <div style={{ fontWeight: 600, color: tokens.colorNeutralForeground1 }}>存档路径：</div>
            {game.save_paths.map((p, i) => (
              <div key={i}>{p}</div>
            ))}
          </div>
          <div style={{ display: 'flex', gap: '8px', marginTop: '8px' }}>
            <Button
              icon={<ArrowUpload24Regular />}
              appearance="primary"
              onClick={() => setBackupOpen(true)}
              disabled={backup.backingUp}
            >
              备份
            </Button>
            <Button
              icon={<ArrowDownload24Regular />}
              onClick={() => setRestoreOpen(true)}
              disabled={backup.restoring}
            >
              恢复
            </Button>
            <Button
              icon={<Delete24Regular />}
              appearance="subtle"
              onClick={async () => {
                if (confirm('确定要删除这个游戏配置吗？存档文件不会被删除。')) {
                  await remove(game.id)
                  window.location.href = '#/'
                }
              }}
            >
              删除
            </Button>
          </div>
        </div>
      </div>

      {/* 本地备份历史（第一位） */}
      <div className={styles.section}>
        <div className={styles.sectionHeader}>
          <Title2>本地备份历史</Title2>
          <Button size="small" onClick={() => backup.fetchHistory()}>
            刷新
          </Button>
        </div>
        {backup.history.length === 0 ? (
          <div style={{ color: tokens.colorNeutralForeground3 }}>暂无备份记录</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHeaderCell>时间</TableHeaderCell>
                <TableHeaderCell>类型</TableHeaderCell>
                <TableHeaderCell>文件数</TableHeaderCell>
                <TableHeaderCell>操作</TableHeaderCell>
              </TableRow>
            </TableHeader>
            <TableBody>
              {backup.history.map((h) => (
                <TableRow key={h.timestamp}>
                  <TableCell>
                    <Clock24Regular style={{ marginRight: '8px', verticalAlign: 'middle' }} />
                    {new Date(h.timestamp).toLocaleString('zh-CN')}
                  </TableCell>
                  <TableCell>
                    <Badge
                      appearance="filled"
                      color={h.backup_type === 'full' ? 'brand' : 'informative'}
                    >
                      {h.backup_type === 'full' ? '全量' : '增量'}
                    </Badge>
                  </TableCell>
                  <TableCell>{h.files.length}</TableCell>
                  <TableCell>
                    <Button
                      size="small"
                      icon={<ArrowDownload24Regular />}
                      onClick={() => handleRestoreLocal(h.timestamp)}
                      disabled={backup.restoring}
                    >
                      恢复
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </div>

      {/* 远程备份（Alist 网盘）（第二位） */}
      <div className={styles.section}>
        <div className={styles.sectionHeader}>
          <Title2>远程备份（Alist 网盘）</Title2>
          <Button size="small" onClick={() => backup.fetchRemoteBackups()}>
            刷新
          </Button>
        </div>
        {backup.loadingRemote ? (
          <Spinner label="加载中..." />
        ) : backup.remoteBackups.length === 0 ? (
          <div style={{ color: tokens.colorNeutralForeground3 }}>暂无远程备份</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHeaderCell>文件名</TableHeaderCell>
                <TableHeaderCell>大小</TableHeaderCell>
                <TableHeaderCell>修改时间</TableHeaderCell>
                <TableHeaderCell>操作</TableHeaderCell>
              </TableRow>
            </TableHeader>
            <TableBody>
              {backup.remoteBackups.map((rb) => (
                <TableRow key={rb.path}>
                  <TableCell className={styles.fileNameCell} title={rb.name}>
                    {rb.name}
                  </TableCell>
                  <TableCell>{formatBytes(rb.size)}</TableCell>
                  <TableCell>
                    {rb.modified ? new Date(rb.modified).toLocaleString('zh-CN') : '-'}
                  </TableCell>
                  <TableCell>
                    <Button
                      size="small"
                      icon={<CloudArrowDown24Regular />}
                      onClick={() => handleRestoreRemote(rb.path)}
                      disabled={backup.restoring}
                    >
                      还原
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </div>

      {/* 本地存档文件（移到最下方） */}
      <div className={styles.section}>
        <div className={styles.sectionHeader}>
          <Title2>本地存档文件</Title2>
          <Button size="small" onClick={() => scanGameSaves(game.id).then(setSaves)}>
            刷新
          </Button>
        </div>
        {savesLoading ? (
          <Spinner label="扫描中..." />
        ) : saves.length === 0 ? (
          <div style={{ color: tokens.colorNeutralForeground3 }}>未找到存档文件</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHeaderCell>文件</TableHeaderCell>
                <TableHeaderCell>大小</TableHeaderCell>
                <TableHeaderCell>修改时间</TableHeaderCell>
              </TableRow>
            </TableHeader>
            <TableBody>
              {saves.map((file) => (
                <TableRow key={file.path}>
                  <TableCell>
                    <Document24Regular style={{ marginRight: '8px', verticalAlign: 'middle' }} />
                    {file.relative_path}
                  </TableCell>
                  <TableCell>{formatBytes(file.size)}</TableCell>
                  <TableCell>{new Date(file.modified_time).toLocaleString('zh-CN')}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </div>

      <BackupDialog
        open={backupOpen}
        gameName={game.name}
        onClose={() => setBackupOpen(false)}
        onConfirm={handleBackup}
      />

      <RestoreDialog
        open={restoreOpen}
        gameName={game.name}
        localHistory={backup.history}
        remoteBackups={backup.remoteBackups}
        onClose={() => setRestoreOpen(false)}
        onRestoreLocal={handleRestoreLocal}
        onRestoreRemote={handleRestoreRemote}
      />
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
