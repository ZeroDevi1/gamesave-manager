// components/GameCard.tsx - 游戏卡片
import {
  Card,
  CardHeader,
  CardPreview,
  Text,
  Button,
  makeStyles,
  tokens,
} from '@fluentui/react-components'
import { ArrowUpload24Regular, ArrowDownload24Regular } from '@fluentui/react-icons'
import { useNavigate } from 'react-router-dom'
import type { GameConfig } from '../services/tauri'

const useStyles = makeStyles({
  card: {
    width: '240px',
    cursor: 'pointer',
    transition: 'transform 0.2s, box-shadow 0.2s',
    ':hover': {
      transform: 'translateY(-4px)',
      boxShadow: tokens.shadow8,
    },
  },
  preview: {
    height: '140px',
    backgroundColor: tokens.colorNeutralBackground1,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    overflow: 'hidden',
  },
  logo: {
    width: '100%',
    height: '100%',
    objectFit: 'cover',
  },
  placeholder: {
    width: '64px',
    height: '64px',
    borderRadius: '50%',
    backgroundColor: tokens.colorBrandBackground,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: tokens.colorNeutralForegroundOnBrand,
    fontSize: '24px',
    fontWeight: 'bold',
  },
  actions: {
    display: 'flex',
    gap: '4px',
  },
})

interface GameCardProps {
  game: GameConfig
  onBackup?: (gameId: string) => void
  onRestore?: (gameId: string) => void
}

export default function GameCard({ game, onBackup, onRestore }: GameCardProps) {
  const styles = useStyles()
  const navigate = useNavigate()

  return (
    <Card
      className={styles.card}
      onClick={() => navigate(`/game/${game.id}`)}
    >
      <CardPreview className={styles.preview}>
        {game.logo_path ? (
          <img
            src={game.logo_path.startsWith('http') ? game.logo_path : `file://${game.logo_path}`}
            alt={game.name}
            className={styles.logo}
            onError={(e) => {
              (e.target as HTMLImageElement).style.display = 'none'
            }}
          />
        ) : (
          <div className={styles.placeholder}>{game.name.charAt(0)}</div>
        )}
      </CardPreview>
      <CardHeader
        header={
          <Text weight="semibold" size={400}>{game.name}</Text>
        }
        description={
          <Text size={200} style={{ color: tokens.colorNeutralForeground3 }}>
            {game.last_backup
              ? `上次备份: ${new Date(game.last_backup).toLocaleString('zh-CN')}`
              : '尚未备份'}
          </Text>
        }
        action={
          <div className={styles.actions}>
            <Button
              size="small"
              icon={<ArrowUpload24Regular />}
              onClick={(e) => {
                e.stopPropagation()
                onBackup?.(game.id)
              }}
              title="备份"
            />
            <Button
              size="small"
              icon={<ArrowDownload24Regular />}
              onClick={(e) => {
                e.stopPropagation()
                onRestore?.(game.id)
              }}
              title="恢复"
            />
          </div>
        }
      />
    </Card>
  )
}
