// components/GameCard.tsx - 游戏卡片（放大版 + 图标展示 + 启动游戏）
import {
  Card,
  CardHeader,
  CardPreview,
  Text,
  Button,
  makeStyles,
  tokens,
} from '@fluentui/react-components'
import {
  ArrowUpload24Regular,
  ArrowDownload24Regular,
  Play24Regular,
} from '@fluentui/react-icons'
import { useNavigate } from 'react-router-dom'
import { convertFileSrc } from '../services/tauri'
import type { GameConfig } from '../services/tauri'

const useStyles = makeStyles({
  card: {
    width: '280px',
    cursor: 'pointer',
    transition: 'transform 0.2s, box-shadow 0.2s',
    ':hover': {
      transform: 'translateY(-4px)',
      boxShadow: tokens.shadow16,
    },
  },
  preview: {
    height: '180px',
    backgroundColor: tokens.colorNeutralBackground1,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    overflow: 'hidden',
    position: 'relative',
  },
  logo: {
    width: '100%',
    height: '100%',
    objectFit: 'cover',
  },
  placeholder: {
    width: '72px',
    height: '72px',
    borderRadius: '50%',
    backgroundColor: tokens.colorBrandBackground,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: tokens.colorNeutralForegroundOnBrand,
    fontSize: '28px',
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
  onLaunch?: (steamAppid: number) => void
}

function getLogoUrl(logoPath?: string): string {
  if (!logoPath) return ''
  if (logoPath.startsWith('http')) return logoPath
  // Tauri v2 本地文件路径需使用 convertFileSrc 转换
  return convertFileSrc(logoPath)
}

export default function GameCard({ game, onBackup, onRestore, onLaunch }: GameCardProps) {
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
            src={getLogoUrl(game.logo_path)}
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
            {game.steam_appid != null && (
              <Button
                size="small"
                icon={<Play24Regular />}
                appearance="primary"
                onClick={(e) => {
                  e.stopPropagation()
                  onLaunch?.(game.steam_appid!)
                }}
                title="启动游戏"
              />
            )}
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
