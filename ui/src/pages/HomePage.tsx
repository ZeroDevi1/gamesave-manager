// pages/HomePage.tsx - 首页：游戏卡片网格 + 添加游戏（放大布局 + 隐藏滚动条）
import {
  makeStyles,
  Title1,
  Button,
  Input,
  Dialog,
  DialogSurface,
  DialogTitle,
  DialogBody,
  DialogActions,
  DialogContent,
  Spinner,
  MessageBar,
  MessageBarBody,
} from '@fluentui/react-components'
import { Add24Regular, Search24Regular, Games24Regular, ArrowSync24Regular } from '@fluentui/react-icons'
import { useState } from 'react'
import GameCard from '../components/GameCard'
import BackupDialog from '../components/BackupDialog'
import { useGames } from '../hooks/useGames'
import { useBackup, useChangeDetection } from '../hooks/useBackup'

import { launchGame } from '../services/tauri'
import { useAppStore } from '../store/appStore'

const useStyles = makeStyles({
  root: {
    padding: '20px',
    display: 'flex',
    flexDirection: 'column',
    gap: '16px',
    height: '100%',
    boxSizing: 'border-box',
    // 隐藏原生滚动条，保留滚动能力
    overflowY: 'auto',
    scrollbarWidth: 'none',
    '::-webkit-scrollbar': {
      display: 'none',
    },
    '@media (max-height: 700px)': {
      padding: '12px',
      gap: '10px',
    },
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: '16px',
    flexShrink: 0,
    padding: '14px 18px',
    // Liquid Glass 标题栏面板
    backgroundColor: 'var(--lg-bg-primary)',
    backdropFilter: 'blur(var(--lg-blur-md)) saturate(var(--lg-saturate))',
    WebkitBackdropFilter: 'blur(var(--lg-blur-md)) saturate(var(--lg-saturate))',
    border: '1px solid var(--lg-border-color)',
    borderRadius: 'var(--lg-radius-lg)',
    boxShadow: 'var(--lg-shadow-sm)',
  },
  title: {
    color: 'var(--lg-text-primary)',
  },
  searchBox: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexGrow: 1,
    maxWidth: '400px',
    padding: '4px 10px',
    backgroundColor: 'var(--lg-bg-interactive)',
    border: '1px solid var(--lg-border-color)',
    borderRadius: 'var(--lg-radius-md)',
    ':hover': {
      backgroundColor: 'var(--lg-bg-interactive-hover)',
      border: '1px solid var(--lg-border-hover)',
    },
  },
  grid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))',
    gap: '16px',
    flexShrink: 0,
  },
  empty: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    textAlign: 'center',
    color: 'var(--lg-text-secondary)',
    padding: '80px 0',
    gap: '16px',
  },
  emptyIcon: {
    fontSize: '48px',
    opacity: 0.4,
  },
})

export default function HomePage() {
  const styles = useStyles()
  const { games, loading, error, add, refresh } = useGames()
  const { addToast } = useAppStore()
  const change = useChangeDetection()
  const [search, setSearch] = useState('')
  const [addOpen, setAddOpen] = useState(false)
  const [newName, setNewName] = useState('')
  const [newPaths, setNewPaths] = useState('')
  const [backupGameId, setBackupGameId] = useState<string | null>(null)
  const backup = useBackup(backupGameId ?? undefined)

  const filteredGames = games.filter((g) =>
    g.name.toLowerCase().includes(search.toLowerCase()),
  )

  const handleAdd = async () => {
    if (!newName.trim()) return
    const paths = newPaths
      .split('\n')
      .map((p) => p.trim())
      .filter((p) => p.length > 0)
    await add(newName.trim(), paths)
    setAddOpen(false)
    setNewName('')
    setNewPaths('')
  }

  const handleBackup = async (type: 'full' | 'incremental') => {
    if (!backupGameId) return
    if (type === 'full') {
      await backup.fullBackup()
    } else {
      await backup.incrementalBackup()
    }
    await refresh()
    setBackupGameId(null)
  }

  const handleLaunch = async (steamAppid: number) => {
    try {
      await launchGame(steamAppid)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      addToast(msg, 'error')
    }
  }

  const activeGame = games.find((g) => g.id === backupGameId)

  return (
    <div className={styles.root}>
      <div className={styles.header}>
        <Title1 className={styles.title}>游戏存档管理</Title1>
        <div className={styles.searchBox}>
          <Search24Regular style={{ color: 'var(--lg-text-secondary)' }} />
          <Input
            placeholder="搜索游戏..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            style={{
              flexGrow: 1,
              backgroundColor: 'transparent',
              border: 'none',
              outline: 'none',
            }}
          />
        </div>
        <Button
          icon={<ArrowSync24Regular />}
          onClick={() => change.checkChanges()}
          disabled={change.checking || change.backingUpAll}
          appearance="subtle"
        >
          {change.checking ? '检查中...' : '检查变更'}
        </Button>
        {change.changedGames.length > 0 && (
          <Button
            appearance="primary"
            onClick={() => change.backupAll()}
            disabled={change.backingUpAll}
          >
            {change.backingUpAll
              ? '备份中...'
              : `一键备份全部 (${change.changedGames.length})`}
          </Button>
        )}
        <Button
          icon={<Add24Regular />}
          appearance="primary"
          onClick={() => setAddOpen(true)}
        >
          添加游戏
        </Button>

      {change.changedGames.length > 0 && (
        <MessageBar intent="warning" style={{ flexShrink: 0 }}>
          <MessageBarBody>
            {change.changedGames.length} 个游戏共有{' '}
            {change.changedGames.reduce((s, [, c]) => s + c, 0)} 个未备份的存档变更
          </MessageBarBody>
        </MessageBar>
      )}
      </div>

      {error && (
        <MessageBar intent="error">
          <MessageBarBody>{error}</MessageBarBody>
        </MessageBar>
      )}

      {loading ? (
        <Spinner label="加载游戏中..." size="huge" />
      ) : filteredGames.length === 0 ? (
        <div className={styles.empty}>
          <Games24Regular className={styles.emptyIcon} />
          <span>{search ? '未找到匹配的游戏，试试其他关键词' : '还没有添加任何游戏'}</span>
          {!search && (
            <Button icon={<Add24Regular />} appearance="primary" onClick={() => setAddOpen(true)}>
              添加第一个游戏
            </Button>
          )}
        </div>
      ) : (
        <div className={styles.grid}>
          {filteredGames.map((game) => (
            <GameCard
              key={game.id}
              game={game}
              onBackup={(id) => setBackupGameId(id)}
              onRestore={(id) => {
                window.location.href = `#/game/${id}`
              }}
              onLaunch={handleLaunch}
            />
          ))}
        </div>
      )}

      {/* 添加游戏弹窗 */}
      <Dialog open={addOpen} onOpenChange={(_, data) => !data.open && setAddOpen(false)}>
        <DialogSurface
          style={{
            backgroundColor: 'var(--lg-bg-elevated)',
            backdropFilter: 'blur(var(--lg-blur-lg)) saturate(var(--lg-saturate))',
            WebkitBackdropFilter: 'blur(var(--lg-blur-lg)) saturate(var(--lg-saturate))',
            border: '1px solid var(--lg-border-color)',
            borderRadius: 'var(--lg-radius-xl)',
            boxShadow: 'var(--lg-shadow-high)',
          }}
        >
          <DialogBody>
            <DialogTitle style={{ color: 'var(--lg-text-primary)' }}>添加游戏</DialogTitle>
            <DialogContent style={{ color: 'var(--lg-text-primary)' }}>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
                <div>
                  <label>游戏名称</label>
                  <Input
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    placeholder="例如：Elden Ring"
                  />
                </div>
                <div>
                  <label>存档路径（每行一个）</label>
                  <textarea
                    value={newPaths}
                    onChange={(e) => setNewPaths(e.target.value)}
                    placeholder="%APPDATA%/EldenRing/&#10;C:/Users/xxx/Documents/My Games/..."
                    style={{
                      width: '100%',
                      minHeight: '80px',
                      fontFamily: 'monospace',
                      fontSize: '14px',
                    }}
                  />
                </div>
              </div>
            </DialogContent>
            <DialogActions>
              <Button appearance="secondary" onClick={() => setAddOpen(false)}>
                取消
              </Button>
              <Button appearance="primary" onClick={handleAdd}>
                添加
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>

      {/* 备份弹窗 */}
      <BackupDialog
        open={!!backupGameId}
        gameName={activeGame?.name ?? ''}
        onClose={() => setBackupGameId(null)}
        onConfirm={handleBackup}
      />
    </div>
  )
}
