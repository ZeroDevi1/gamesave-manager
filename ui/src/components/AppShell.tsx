// components/AppShell.tsx - 侧边栏 + 主内容区布局
// 遵循 WinUI 3 NavigationView 标准规范重构，具备顶折叠、优雅指示条与流畅过渡动效
import {
  makeStyles,
  shorthands,
  tokens,
  Button,
  Tooltip,
  mergeClasses,
} from '@fluentui/react-components'
import {
  bundleIcon,
  Settings24Regular,
  Settings24Filled,
  Games24Regular,
  Games24Filled,
  Database24Regular,
  Database24Filled,
  PanelLeftContract24Regular,
  PanelLeftExpand24Regular,
} from '@fluentui/react-icons'
import { useNavigate, useLocation } from 'react-router-dom'
import { useState, useCallback } from 'react'
import type { ReactNode } from 'react'

// ==================== Fluent Design v9 极简精致样式系统 ====================
const useStyles = makeStyles({
  root: {
    display: 'flex',
    height: '100vh',
    width: '100vw',
    overflow: 'hidden',
  },
  // 侧边栏容器：支持宽度平滑动效与磨砂微透明感
  sidebar: {
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: tokens.colorNeutralBackground3,
    ...shorthands.borderRight('1px', 'solid', tokens.colorNeutralStroke3),
    paddingTop: '8px',
    paddingBottom: '8px',
    paddingLeft: '0',
    paddingRight: '0',
    flexShrink: 0,
    // 采用 Fluent 标准动力学缓动曲线以带来丝滑的展开折叠体验
    transition: 'width 250ms cubic-bezier(0.1, 0.9, 0.2, 1)',
    width: '56px',
    boxSizing: 'border-box',
  },
  sidebarExpanded: {
    width: '240px',
  },
  // WinUI 3 经典的置顶 Header 区域（折叠按钮 + 标题并排）
  sidebarHeader: {
    display: 'flex',
    alignItems: 'center',
    height: '48px',
    paddingLeft: '10px',
    paddingRight: '10px',
    gap: '12px',
    flexShrink: 0,
    marginBottom: '8px',
    overflow: 'hidden',
  },
  appTitle: {
    fontSize: tokens.fontSizeBase300,
    fontWeight: tokens.fontWeightSemibold,
    color: tokens.colorNeutralForeground1,
    whiteSpace: 'nowrap',
    textOverflow: 'ellipsis',
    overflow: 'hidden',
    userSelect: 'none',
    opacity: 0,
    animationName: {
      from: { opacity: 0, transform: 'translateX(-8px)' },
      to: { opacity: 1, transform: 'translateX(0)' },
    },
    animationDuration: '250ms',
    animationFillMode: 'forwards',
    animationTimingFunction: 'cubic-bezier(0.1, 0.9, 0.2, 1)',
  },
  // 导航列表：左右预留 10px 间隙，与置顶折叠按钮保持绝对垂直对齐
  navList: {
    display: 'flex',
    flexDirection: 'column',
    width: '100%',
    gap: '4px',
    flexGrow: 1,
    flexShrink: 1,
    overflow: 'hidden',
    paddingLeft: '10px',
    paddingRight: '10px',
    boxSizing: 'border-box',
  },
  // 自动沉底垫片
  spacer: {
    flexGrow: 1,
    flexShrink: 1,
    minHeight: '16px',
  },
  // 细分割线
  divider: {
    height: '1px',
    backgroundColor: tokens.colorNeutralStroke3,
    marginTop: '6px',
    marginBottom: '6px',
    marginLeft: '12px',
    marginRight: '12px',
  },
  // 导航项按钮基础样式：提供 200ms 圆润 hover 过渡
  navBtn: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    height: '36px',
    width: '100%',
    minWidth: '36px',
    padding: '0',
    borderRadius: tokens.borderRadiusMedium,
    position: 'relative',
    overflow: 'hidden',
    color: tokens.colorNeutralForeground2,
    transition: 'all 200ms ease',
    '&:hover': {
      backgroundColor: tokens.colorNeutralBackground3Hover,
      color: tokens.colorNeutralForeground2Hover,
    },
  },
  navBtnExpanded: {
    justifyContent: 'flex-start',
    paddingLeft: '6px', // 24px图标居中于36px容器时，左边距为(36-24)/2=6px。此项设定确保折叠与展开中线完全重合，像素级无抖动！
    paddingRight: '12px',
    gap: '12px',
  },
  // 选中态按钮：背景变浅，颜色变主题色，并激活精致小指示条的竖向拉伸动画
  navBtnSelected: {
    backgroundColor: tokens.colorNeutralBackground1,
    color: tokens.colorBrandForeground1,
    fontWeight: tokens.fontWeightSemibold,
    '&:hover': {
      backgroundColor: tokens.colorNeutralBackground1Hover,
    },
    // 精致的 3D 圆角选中指示器，提供竖向微动效以增加愉悦感
    '&::before': {
      content: '""',
      position: 'absolute',
      left: '0',
      top: '10px', // 配合 36px 高度的按钮，上下各保留 10px，生成 16px 高的精致微竖线条
      bottom: '10px',
      width: '3px',
      borderRadius: tokens.borderRadiusCircular,
      backgroundColor: tokens.colorBrandStroke1,
      transformOrigin: 'center',
      animationName: {
        from: { transform: 'scaleY(0.3)', opacity: 0 },
        to: { transform: 'scaleY(1)', opacity: 1 },
      },
      animationDuration: '220ms',
      animationTimingFunction: 'cubic-bezier(0.1, 0.9, 0.2, 1)',
    },
  },
  navLabel: {
    fontSize: tokens.fontSizeBase200,
    whiteSpace: 'nowrap',
  },
  // 折叠控制按钮样式
  toggleBtn: {
    width: '36px',
    height: '36px',
    minWidth: '36px',
    padding: '0',
    flexShrink: 0,
    borderRadius: tokens.borderRadiusMedium,
    color: tokens.colorNeutralForeground2,
    '&:hover': {
      backgroundColor: tokens.colorNeutralBackground3Hover,
      color: tokens.colorNeutralForeground2Hover,
    },
  },
  // 右侧主内容区域
  content: {
    flexGrow: 1,
    overflow: 'auto',
    scrollbarWidth: 'none',
    '::-webkit-scrollbar': {
      display: 'none',
    },
    backgroundColor: tokens.colorNeutralBackground2,
  },
})

// 统一采用 bundleIcon 获取 Regular/Filled 高保真过渡效果
const GamesIcon = bundleIcon(Games24Filled, Games24Regular)
const DbIcon = bundleIcon(Database24Filled, Database24Regular)
const SettingsIcon = bundleIcon(Settings24Filled, Settings24Regular)

interface NavItemDef {
  value: string
  label: string
  icon: ReturnType<typeof bundleIcon>
}

// 经过合并优化的顶级菜单项，去除原本错乱多余的“首页”与“游戏”两个指向相同的项
const TOP_ITEMS: NavItemDef[] = [
  { value: '/', label: '我的游戏', icon: GamesIcon },
  { value: '/database', label: '游戏数据库', icon: DbIcon },
]

// 底部设置菜单项
const BOTTOM_ITEMS: NavItemDef[] = [
  { value: '/settings', label: '设置', icon: SettingsIcon },
]

interface AppShellProps {
  children: ReactNode
}

/**
 * 侧栏导航按钮子组件
 */
function NavButton({
  item,
  selected,
  expanded,
  onClick,
}: {
  item: NavItemDef
  selected: boolean
  expanded: boolean
  onClick: () => void
}) {
  const styles = useStyles()
  const Icon = item.icon

  const btn = (
    <Button
      appearance="subtle"
      icon={<Icon />}
      onClick={onClick}
      className={mergeClasses(
        styles.navBtn,
        expanded && styles.navBtnExpanded,
        selected && styles.navBtnSelected,
      )}
      title={expanded ? undefined : item.label}
    >
      {expanded && <span className={styles.navLabel}>{item.label}</span>}
    </Button>
  )

  // 折叠时显示精致的 Tooltip，展开时直接返回按钮本身以提高交互效率
  if (expanded) return btn
  return <Tooltip content={item.label} relationship="label" positioning="after">{btn}</Tooltip>
}

/**
 * 全局 AppShell 主骨架布局组件
 */
export default function AppShell({ children }: AppShellProps) {
  const styles = useStyles()
  const navigate = useNavigate()
  const location = useLocation()
  const [expanded, setExpanded] = useState(false)

  // 切换折叠与展开状态
  const toggleExpanded = useCallback(() => {
    setExpanded((prev) => !prev)
  }, [])

  // 修复高亮路由逻辑：如果路径是游戏详情页 `/game/xxx`，则“我的游戏”菜单项依然高亮选中
  const isSelected = (value: string) => {
    if (value === '/') return location.pathname === '/' || location.pathname.startsWith('/game/')
    if (value === '/database') return location.pathname === '/database'
    if (value === '/settings') return location.pathname === '/settings'
    return location.pathname === value
  }

  // 简洁的导航处理函数，消除了离奇的重映射，并直接使用底层 value 路由
  const handleNav = (value: string) => {
    navigate(value)
  }

  const sidebarClasses = [styles.sidebar]
  if (expanded) {
    sidebarClasses.push(styles.sidebarExpanded)
  }

  return (
    <div className={styles.root}>
      {/* 侧边栏容器 */}
      <div className={sidebarClasses.join(' ')}>
        {/* WinUI 3 置顶 Header 栏 */}
        <div className={styles.sidebarHeader}>
          <Button
            appearance="transparent"
            icon={
              expanded ? (
                <PanelLeftContract24Regular />
              ) : (
                <PanelLeftExpand24Regular />
              )
            }
            onClick={toggleExpanded}
            className={styles.toggleBtn}
            title={expanded ? '收起侧边栏' : '展开侧边栏'}
          />
          {expanded && <span className={styles.appTitle}>GameSave Manager</span>}
        </div>

        {/* 导航按钮列表区域 */}
        <div className={styles.navList}>
          {TOP_ITEMS.map((item) => (
            <NavButton
              key={item.value}
              item={item}
              selected={isSelected(item.value)}
              expanded={expanded}
              onClick={() => handleNav(item.value)}
            />
          ))}

          {/* 自动沉底，将设置与折叠菜单顶至底部 */}
          <div className={styles.spacer} />
          
          <div className={styles.divider} />

          {BOTTOM_ITEMS.map((item) => (
            <NavButton
              key={item.value}
              item={item}
              selected={isSelected(item.value)}
              expanded={expanded}
              onClick={() => handleNav(item.value)}
            />
          ))}
        </div>
      </div>

      {/* 右侧主体页面内容 */}
      <div className={styles.content}>{children}</div>
    </div>
  )
}


