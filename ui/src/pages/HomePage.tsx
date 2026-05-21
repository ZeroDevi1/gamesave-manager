// pages/HomePage.tsx - 首页：游戏卡片网格 + 添加游戏
import {
  makeStyles,
  tokens,
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
import { Add24Regular, Search24Regular } from '@fluentui/react-icons'
import { useState } from 'react'
import GameCard from '../components/GameCard'
import BackupDialog from '../components/BackupDialog'
import { useGames } from '../hooks/useGames'
import { useBackup } from '../hooks/useBackup'

const useStyles = makeStyles({
  root: {
    padding: '24px',
    display: 'flex',
    flexDirection: 'column',
    gap: '20px',
    minHeight: '100%',
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: '16px',
  },
  searchBox: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexGrow: 1,
    maxWidth: '400px',
  },
  grid: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '20px',
  },
  empty: {
    textAlign: 'center',
    color: tokens.colorNeutralForeground3,
    padding: '60px 0',
  },
})

export default function HomePage() {
  const styles = useStyles()
  const { games, loading, error, add, refresh } = useGames()
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

  const activeGame = games.find((g) => g.id === backupGameId)

  return (
    <div className={styles.root}>
      <div className={styles.header}>
        <Title1>游戏存档管理</Title1>
        <div className={styles.searchBox}>
          <Search24Regular />
          <Input
            placeholder="搜索游戏..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            style={{ flexGrow: 1 }}
          />
        </div>
        <Button
          icon={<Add24Regular />}
          appearance="primary"
          onClick={() => setAddOpen(true)}
        >
          添加游戏
        </Button>
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
          {search ? '未找到匹配的游戏' : '暂无游戏，点击右上角添加'}
        </div>
      ) : (
        <div className={styles.grid}>
          {filteredGames.map((game) => (
            <GameCard
              key={game.id}
              game={game}
              onBackup={(id) => setBackupGameId(id)}
              onRestore={(id) => {
                // 恢复逻辑简化处理：导航到详情页
                window.location.href = `#/game/${id}`
              }}
            />
          ))}
        </div>
      )}

      {/* 添加游戏弹窗 */}
      <Dialog open={addOpen} onOpenChange={(_, data) => !data.open && setAddOpen(false)}>
        <DialogSurface>
          <DialogBody>
            <DialogTitle>添加游戏</DialogTitle>
            <DialogContent>
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
