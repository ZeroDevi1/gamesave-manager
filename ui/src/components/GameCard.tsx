// components/GameCard.tsx - 游戏卡片（竖版 + 封面展示 + 启动游戏）
import {
  Card,
  CardHeader,
  CardPreview,
  Text,
  Button,
  makeStyles,
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
    width: '200px',
    cursor: 'pointer',
    overflow: 'hidden',
    // 玻璃外观由 .lg-card 工具类提供，避免 Griffel 对 CSS 变量做严格类型校验
  },
  preview: {
    // 统一 2:3 比例封面（200×300）
    height: '300px',
    backgroundColor: 'var(--lg-bg-secondary)',
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
    width: '100%',
    height: '100%',
    // 渐变占位：品牌色渐变融入玻璃氛围
    background: 'linear-gradient(135deg, var(--lg-brand-subtle) 0%, var(--lg-brand) 100%)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: 'var(--lg-text-on-brand)',
    fontSize: '48px',
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
  return convertFileSrc(logoPath)
}

export default function GameCard({ game, onBackup, onRestore, onLaunch }: GameCardProps) {
  const styles = useStyles()
  const navigate = useNavigate()

  return (
    <Card
      className={`${styles.card} lg-card`}
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
          <Text size={200} style={{ color: 'var(--lg-text-secondary)' }}>
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
