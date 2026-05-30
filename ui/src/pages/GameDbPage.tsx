// pages/GameDbPage.tsx - 游戏数据库页面
// 浏览、搜索、编辑内置游戏数据库，支持导入导出和一键创建本地游戏

import {
  makeStyles,
  tokens,
  shorthands,
  Title1,
  Button,
  Input,
  Dialog,
  DialogSurface,
  DialogTitle,
  DialogBody,
  DialogActions,
  DialogContent,
  Table,
  TableHeader,
  TableRow,
  TableHeaderCell,
  TableBody,
  TableCell,
  Badge,
  Spinner,
  MessageBar,
  MessageBarBody,
  Textarea,
  Label,
  Tooltip,
} from '@fluentui/react-components'
import {
  Add24Regular,
  Search24Regular,
  ArrowExport24Regular,
  ArrowImport24Regular,
  Dismiss24Regular,
  Save24Regular,
  Games24Regular,
  Globe24Regular,
  ArrowDownload24Regular,
  FolderOpen24Regular,
  ArrowClockwise24Regular,
  Image24Regular,
} from '@fluentui/react-icons'
import { useState, useEffect, useCallback } from 'react'
import {
  getGameDb,
  searchGameDb,
  upsertGameDbEntry,
  removeGameDbEntry,
  exportGameDb,
  importGameDb,
  createGameFromDb,
  searchPcgwGames,
  fetchPcgwSavePaths,
  searchSteamStore,
  searchSteamStoreRobust,
  searchPcgwBySteamAppid,
  refreshGameDbSavePaths,
  selectAndExtractExeIcon,
  saveCustomLogo,
  getDbGameLogo,
} from '../services/tauri'
import type { GameDbEntry, PcgwSearchResult, PcgwGameDetail } from '../services/tauri'
import { useAppStore } from '../store/appStore'

const useStyles = makeStyles({
  root: {
    padding: '32px',
    display: 'flex',
    flexDirection: 'column',
    gap: '24px',
    minHeight: '100%',
    backgroundColor: tokens.colorNeutralBackground2,
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
    flexWrap: 'wrap',
  },
  searchBox: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexGrow: 1,
    maxWidth: '400px',
  },
  searchInput: {
    flexGrow: 1,
    transition: 'all 200ms cubic-bezier(0.1, 0.9, 0.2, 1)',
    '&:focus-within': {
      boxShadow: `0 0 0 2px ${tokens.colorBrandBackground2}`,
    }
  },
  actions: {
    display: 'flex',
    gap: '10px',
  },
  tableCard: {
    backgroundColor: tokens.colorNeutralBackground1,
    borderRadius: tokens.borderRadiusXLarge, // Win11 大圆角
    ...shorthands.border('1px', 'solid', tokens.colorNeutralStroke3),
    boxShadow: tokens.shadow8,
    padding: '8px',
    transition: 'box-shadow 250ms ease, transform 250ms ease',
    '&:hover': {
      boxShadow: tokens.shadow16, // Hover 时轻微浮起感
    }
  },
  tableHeader: {
    backgroundColor: tokens.colorNeutralBackground3,
    ...shorthands.borderBottom('1px', 'solid', tokens.colorNeutralStroke3),
  },
  tableHeaderCell: {
    fontWeight: tokens.fontWeightSemibold,
    color: tokens.colorNeutralForeground2,
    paddingTop: '12px',
    paddingBottom: '12px',
  },
  tableRow: {
    transition: 'background-color 150ms ease',
    borderBottom: `1px solid ${tokens.colorNeutralStrokeSubtle}`,
    '&:hover': {
      backgroundColor: tokens.colorNeutralBackground1Hover,
    },
  },
  pathCell: {
    fontFamily: 'monospace',
    fontSize: '13px',
    color: tokens.colorNeutralForeground3,
    maxWidth: '400px',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
  empty: {
    textAlign: 'center',
    color: tokens.colorNeutralForeground3,
    padding: '80px 0',
  },
  formField: {
    display: 'flex',
    flexDirection: 'column',
    gap: '6px',
    marginBottom: '16px',
  },
  dialogContent: {
    display: 'flex',
    flexDirection: 'column',
    gap: '20px',
    minWidth: '520px',
    maxHeight: '70vh',
    overflowY: 'auto',
    scrollbarWidth: 'none',
    '::-webkit-scrollbar': {
      display: 'none',
    },
  },
  importTextarea: {
    minHeight: '220px',
    fontFamily: 'monospace',
    fontSize: '13px',
  },
  // Win11 磨砂高保真搜索列表项
  resultCard: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'flex-start',
    padding: '12px 16px',
    backgroundColor: tokens.colorNeutralBackground1,
    borderRadius: tokens.borderRadiusMedium,
    ...shorthands.border('1px', 'solid', tokens.colorNeutralStroke3),
    cursor: 'pointer',
    transition: 'all 200ms cubic-bezier(0.1, 0.9, 0.2, 1)',
    position: 'relative',
    overflow: 'hidden',
    gap: '4px',
    width: '100%',
    '&:hover': {
      backgroundColor: tokens.colorNeutralBackground1Hover,
      ...shorthands.border('1px', 'solid', tokens.colorBrandStroke1),
      transform: 'translateX(4px)', // 平滑右位移
    },
    // 左边缘的精致小蓝色选中拉伸条
    '&::before': {
      content: '""',
      position: 'absolute',
      left: '0',
      top: '12px',
      bottom: '12px',
      width: '3px',
      borderRadius: tokens.borderRadiusCircular,
      backgroundColor: tokens.colorBrandStroke1,
      opacity: 0,
      transform: 'scaleY(0.4)',
      transition: 'all 200ms ease',
    },
    '&:hover::before': {
      opacity: 1,
      transform: 'scaleY(1)',
    }
  },
  pcgwPathsContainer: {
    backgroundColor: tokens.colorNeutralBackground2,
    padding: '12px 16px',
    borderRadius: tokens.borderRadiusMedium,
    marginTop: '10px',
    fontFamily: 'monospace',
    fontSize: '13px',
    ...shorthands.border('1px', 'solid', tokens.colorNeutralStroke3),
    boxShadow: tokens.shadow2,
  },
  pcgwNotes: {
    marginTop: '10px',
    fontSize: '13px',
    color: tokens.colorNeutralForeground3,
    lineHeight: '1.4',
  },
  // 双栏 Master-Detail 自适应高保真容器
  container: {
    display: 'flex',
    gap: '24px',
    flexDirection: 'row',
    alignItems: 'stretch',
    flexGrow: 1,
  },
  // 左侧主列表栏 (占宽 2/3)
  masterCol: {
    flexGrow: 2,
    flexBasis: '0',
    display: 'flex',
    flexDirection: 'column',
    overflowY: 'auto',
    scrollbarWidth: 'none',
    '::-webkit-scrollbar': {
      display: 'none',
    },
    paddingRight: '4px',
  },
  // 右侧详情玻璃面板 (占宽 1/3)
  detailCol: {
    flexGrow: 1,
    flexBasis: '0',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: tokens.colorNeutralBackground1,
    borderRadius: tokens.borderRadiusXLarge,
    ...shorthands.border('1px', 'solid', tokens.colorNeutralStroke3),
    boxShadow: tokens.shadow16,
    padding: '24px',
    overflowY: 'auto',
    scrollbarWidth: 'none',
    '::-webkit-scrollbar': {
      display: 'none',
    },
    position: 'sticky',
    top: '0',
    alignSelf: 'start',
    backdropFilter: 'blur(20px)', // 精致磨砂模糊背景
    transition: 'all 300ms cubic-bezier(0.1, 0.9, 0.2, 1)',
  },
  detailHeader: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    gap: '16px',
    marginBottom: '24px',
    textAlign: 'center',
  },
  // 经典 2:3 纵横比海报盒子
  posterContainer: {
    width: '180px',
    height: '270px',
    borderRadius: tokens.borderRadiusLarge,
    boxShadow: tokens.shadow28,
    overflow: 'hidden',
    backgroundColor: tokens.colorNeutralBackground3,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    position: 'relative',
    ...shorthands.border('1px', 'solid', tokens.colorNeutralStroke1),
    transition: 'all 300ms ease',
    '&:hover': {
      transform: 'scale(1.03)',
      boxShadow: tokens.shadow64,
    }
  },
  posterImage: {
    width: '100%',
    height: '100%',
    objectFit: 'cover',
  },
  detailTitle: {
    fontWeight: tokens.fontWeightSemibold,
    fontSize: '20px',
    color: tokens.colorNeutralForeground1,
  },
  detailSubtitle: {
    fontSize: '13px',
    color: tokens.colorNeutralForeground3,
    fontFamily: 'monospace',
  },
  detailSection: {
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
    marginBottom: '20px',
  },
  detailSectionTitle: {
    fontWeight: tokens.fontWeightSemibold,
    fontSize: '14px',
    color: tokens.colorNeutralForeground2,
  },
  detailPathCard: {
    backgroundColor: tokens.colorNeutralBackground2,
    ...shorthands.padding('10px', '14px'),
    borderRadius: tokens.borderRadiusMedium,
    fontFamily: 'monospace',
    fontSize: '12px',
    wordBreak: 'break-all',
    color: tokens.colorNeutralForeground3,
    ...shorthands.border('1px', 'solid', tokens.colorNeutralStroke3),
  },
  detailActions: {
    display: 'flex',
    flexDirection: 'column',
    gap: '10px',
    marginTop: 'auto',
  },
  // 列表行选中微光高亮效果
  selectedRow: {
    backgroundColor: tokens.colorBrandBackground2,
    '&:hover': {
      backgroundColor: tokens.colorBrandBackground2Hover,
    }
  },
  // EXE 图标提取实时预览区
  exePreview: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '10px',
    backgroundColor: tokens.colorNeutralBackground2,
    borderRadius: tokens.borderRadiusMedium,
    ...shorthands.border('1px', 'solid', tokens.colorNeutralStroke3),
    marginTop: '8px',
  },
  exePreviewIcon: {
    width: '32px',
    height: '32px',
    objectFit: 'contain',
  }
})

function generateId(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
}

export default function GameDbPage() {
  const styles = useStyles()
  const { addToast } = useAppStore()
  const [entries, setEntries] = useState<GameDbEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [error, setError] = useState<string | null>(null)

  // 双栏 Master-Detail 选中项与海报 Base64
  const [selectedEntryId, setSelectedEntryId] = useState<string | null>(null)
  const [selectedEntryLogo, setSelectedEntryLogo] = useState<string | null>(null)

  // 弹窗状态
  const [editOpen, setEditOpen] = useState(false)
  const [editEntry, setEditEntry] = useState<Partial<GameDbEntry>>({})
  const [isEditing, setIsEditing] = useState(false)

  // 弹窗内的 Exe 图标提取与 AppID 检索
  const [exeIconBase64, setExeIconBase64] = useState<string | null>(null)
  const [exePath, setExePath] = useState<string>('')
  const [autoAppIdLoading, setAutoAppIdLoading] = useState(false)

  const [importOpen, setImportOpen] = useState(false)
  const [importJson, setImportJson] = useState('')

  const [exportOpen, setExportOpen] = useState(false)
  const [exportJson, setExportJson] = useState('')


  // PCGamingWiki 搜索状态（统一搜索 + AppID 桥接）
  const [pcgwOpen, setPcgwOpen] = useState(false)
  const [pcgwQuery, setPcgwQuery] = useState('')
  const [pcgwResults, setPcgwResults] = useState<PcgwSearchResult[]>([])
  const [pcgwLoading, setPcgwLoading] = useState(false)
  const [pcgwSelected, setPcgwSelected] = useState<PcgwGameDetail | null>(null)
  // PCGW 批量刷新状态
  const [pcgwRefreshLoading, setPcgwRefreshLoading] = useState(false)

  const fetchDb = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const db = await getGameDb()
      setEntries(db.entries)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchDb()
  }, [fetchDb])

  // 当 entries 发生变化时，默认高亮选中列表中的第一个游戏
  useEffect(() => {
    if (entries.length > 0) {
      if (!selectedEntryId || !entries.some((e) => e.id === selectedEntryId)) {
        setSelectedEntryId(entries[0].id)
      }
    } else {
      setSelectedEntryId(null)
    }
  }, [entries, selectedEntryId])

  // 当选中的游戏改变时，异步加载本地缓存的自定义 Logo base64
  useEffect(() => {
    if (selectedEntryId) {
      getDbGameLogo(selectedEntryId)
        .then((logo) => {
          setSelectedEntryLogo(logo)
        })
        .catch((err) => {
          console.error('加载本地 Logo 失败:', err)
          setSelectedEntryLogo(null)
        })
    } else {
      setSelectedEntryLogo(null)
    }
  }, [selectedEntryId])

  const handleSearch = useCallback(
    async (q: string) => {
      setSearch(q)
      if (!q.trim()) {
        fetchDb()
        return
      }
      setLoading(true)
      try {
        const results = await searchGameDb(q.trim())
        setEntries(results)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      } finally {
        setLoading(false)
      }
    },
    [fetchDb],
  )

  const handleAdd = () => {
    setEditEntry({
      id: '',
      name: '',
      aliases: [],
      save_paths: [],
      platforms: ['windows'],
      source: 'user',
    })
    setIsEditing(false)
    setExePath('')
    setExeIconBase64(null)
    setEditOpen(true)
  }

  const handleEdit = async (entry: GameDbEntry) => {
    setEditEntry({ ...entry })
    setIsEditing(true)
    setExePath('')
    
    // 编辑时尝试读取已有的物理图标做回显
    try {
      const existingLogo = await getDbGameLogo(entry.id)
      setExeIconBase64(existingLogo)
    } catch {
      setExeIconBase64(null)
    }
    
    setEditOpen(true)
  }

  // 点击选择本地 EXE 并提取高清图标
  const handleSelectExe = async () => {
    try {
      const res = await selectAndExtractExeIcon()
      if (res) {
        setExePath(res.path)
        if (res.base64) {
          setExeIconBase64(res.base64)
        }
        
        // 智能提取可执行文件名，如 "eldenring.exe"
        const parts = res.path.split(/[\\/]/)
        const exeName = parts[parts.length - 1]
        
        if (exeName) {
          const currentAliases = editEntry.aliases || []
          if (!currentAliases.some((a) => a.toLowerCase() === exeName.toLowerCase())) {
            const nextAliases = [...currentAliases, exeName]
            updateEditField('aliases', nextAliases)
            addToast(`已自动提取并追加可执行文件别名: ${exeName}`, 'info')
          }
        }
      }
    } catch (err) {
      addToast(err instanceof Error ? err.message : '提取可执行文件图标失败', 'error')
    }
  }

  // 🔍 自动获取 Steam AppID 
  const handleAutoGetAppId = async () => {
    if (!editEntry.name?.trim()) {
      addToast('请先输入游戏名称以搜索 AppID', 'warning')
      return
    }
    setAutoAppIdLoading(true)
    try {
      const results = await searchSteamStore(editEntry.name.trim())
      if (results.length > 0) {
        const first = results[0]
        updateEditField('steam_appid', first.id)
        addToast(`成功获取并填充 Steam AppID: ${first.id} (${first.name})`, 'success')
      } else {
        addToast('未在 Steam 商店中搜索到匹配的游戏，请手动输入', 'warning')
      }
    } catch (err) {
      addToast(err instanceof Error ? err.message : '获取 Steam AppID 失败', 'error')
    } finally {
      setAutoAppIdLoading(false)
    }
  }

  const handleSave = async () => {
    if (!editEntry.name?.trim()) {
      addToast('游戏名称不能为空', 'warning')
      return
    }
    const finalId = editEntry.id || generateId(editEntry.name!)
    const entry: GameDbEntry = {
      id: finalId,
      name: editEntry.name!.trim(),
      aliases: (editEntry.aliases || [])
        .map((a) => a.trim())
        .filter((a) => a.length > 0),
      save_paths: (editEntry.save_paths || [])
        .map((p) => p.trim())
        .filter((p) => p.length > 0),
      platforms: editEntry.platforms || ['windows'],
      steam_appid: editEntry.steam_appid,
      notes: editEntry.notes?.trim() || undefined,
      source: editEntry.source || 'user',
    }
    try {
      await upsertGameDbEntry(entry)
      
      // 保存所提取出的物理 EXE 图标
      if (exeIconBase64) {
        await saveCustomLogo(finalId, exeIconBase64)
      }
      
      addToast(isEditing ? '更新成功' : '添加成功', 'success')
      setEditOpen(false)
      fetchDb()
      setSelectedEntryId(finalId) // 保存后自动让其高亮选中
    } catch (err) {
      addToast(err instanceof Error ? err.message : '保存失败', 'error')
    }
  }

  const handleRemove = async (id: string) => {
    if (!confirm('确定要删除这个条目吗？')) return
    try {
      await removeGameDbEntry(id)
      addToast('删除成功', 'success')
      fetchDb()
    } catch (err) {
      addToast(err instanceof Error ? err.message : '删除失败', 'error')
    }
  }

  const handleCreateGame = async (dbId: string) => {
    try {
      await createGameFromDb(dbId)
      addToast('已添加到本地游戏列表', 'success')
    } catch (err) {
      addToast(err instanceof Error ? err.message : '创建失败', 'error')
    }
  }

  const handleExport = async () => {
    try {
      const json = await exportGameDb()
      setExportJson(json)
      setExportOpen(true)
    } catch (err) {
      addToast(err instanceof Error ? err.message : '导出失败', 'error')
    }
  }

  const handleImport = async () => {
    if (!importJson.trim()) {
      addToast('请输入 JSON 内容', 'warning')
      return
    }
    try {
      await importGameDb(importJson.trim())
      addToast('导入成功', 'success')
      setImportOpen(false)
      setImportJson('')
      fetchDb()
    } catch (err) {
      addToast(err instanceof Error ? err.message : '导入失败', 'error')
    }
  }


  // 统一搜索：多策略 Steam → AppID 桥接 PCGW → 合并展示结果
  const handlePcgwSearch = async () => {
    if (!pcgwQuery.trim()) return
    setPcgwLoading(true)
    setPcgwResults([])
    setPcgwSelected(null)
    try {
      // 1. 多策略 Steam 鲁棒搜索
      const [steamItems, usedQuery] = await searchSteamStoreRobust(pcgwQuery.trim())
      
      if (steamItems.length === 0) {
        // 回退：直接用名称搜索 PCGW
        const results = await searchPcgwGames(pcgwQuery.trim())
        setPcgwResults(results)
        if (results.length === 0) {
          addToast('Steam 和 PCGamingWiki 均未找到匹配游戏', 'warning')
        }
        return
      }

      // 2. 并发查询每个 Steam AppID 对应的 PCGW 页面名
      const merged: PcgwSearchResult[] = []
      const pcgwPromises = steamItems.slice(0, 8).map(async (item) => {
        try {
          const pcgwMatches = await searchPcgwBySteamAppid(item.id)
          if (pcgwMatches.length > 0) {
            for (const match of pcgwMatches) {
              merged.push({
                page_name: match.page_name,
                steam_appid: match.steam_appid || item.id,
              })
            }
          } else {
            // PCGW 未直接匹配 AppID，尝试用 Steam 商品名搜索 PCGW Cargo API
            // 以获取真正的 PCGW 页面名（避免 DLC 名称直接作为页面名导致 missingtitle）
            try {
              const fallbackResults = await searchPcgwGames(item.name)
              if (fallbackResults.length > 0) {
                merged.push({
                  page_name: fallbackResults[0].page_name,
                  steam_appid: item.id,
                })
              } else {
                merged.push({
                  page_name: item.name,
                  steam_appid: item.id,
                })
              }
            } catch {
              merged.push({
                page_name: item.name,
                steam_appid: item.id,
              })
            }
          }
        } catch {
          merged.push({
            page_name: item.name,
            steam_appid: item.id,
          })
        }
      })

      await Promise.all(pcgwPromises)

      // 去重（按 page_name）
      const seen = new Set<string>()
      const uniqueResults = merged.filter((r) => {
        const key = r.page_name.toLowerCase()
        if (seen.has(key)) return false
        seen.add(key)
        return true
      })

      setPcgwResults(uniqueResults)
      
      if (usedQuery !== pcgwQuery.trim()) {
        addToast(`Steam 搜索自动缩短为 "${usedQuery}" 并找到 ${uniqueResults.length} 条结果`, 'info')
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      addToast(`搜索失败: ${msg}`, 'error')
    } finally {
      setPcgwLoading(false)
    }
  }

  // 点击结果项：获取 PCGW 存档路径详情
  const handlePcgwSelect = async (pageName: string) => {
    setPcgwLoading(true)
    try {
      const detail = await fetchPcgwSavePaths(pageName)
      setPcgwSelected(detail)
    } catch (err) {
      const msg = typeof err === 'string' ? err : (err instanceof Error ? err.message : String(err))
      addToast(`获取存档路径失败: ${msg}`, 'error')
    } finally {
      setPcgwLoading(false)
    }
  }


  const handlePcgwImport = async () => {
    if (!pcgwSelected) return
    // 若 PCGW 未含 AppID，尝试通过 Steam 搜索自动补全
    let steamAppid = pcgwSelected.steam_appid
    if (steamAppid == null) {
      try {
        const [items] = await searchSteamStoreRobust(pcgwSelected.page_name)
        if (items.length > 0) {
          steamAppid = items[0].id
          addToast(`已自动补全 Steam AppID: ${steamAppid}`, 'info')
        }
      } catch {
        // 忽略补全失败，继续导入
      }
    }
    const entry: GameDbEntry = {
      id: generateId(pcgwSelected.page_name),
      name: pcgwSelected.page_name,
      aliases: [],
      save_paths: pcgwSelected.windows_save_paths,
      platforms: ['windows'],
      steam_appid: steamAppid,
      notes: pcgwSelected.notes,
      source: 'user',
    }
    try {
      await upsertGameDbEntry(entry)
      addToast('从 PCGamingWiki 导入成功', 'success')
      setPcgwOpen(false)
      fetchDb()
    } catch (err) {
      addToast(err instanceof Error ? err.message : '导入失败', 'error')
    }
  }

  // 从 PCGamingWiki 批量刷新所有条目的存档路径
  const handleRefreshAll = async () => {
    setPcgwRefreshLoading(true)
    try {
      const summary = await refreshGameDbSavePaths()
      if (summary.length === 0) {
        addToast('刷新完成：所有条目均为最新，无变更', 'success')
      } else {
        const total = summary.reduce((acc, [, count]) => acc + count, 0)
        addToast(
          `刷新完成：${summary.length} 个条目新增 ${total} 条路径`,
          'success',
        )
        fetchDb()
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      addToast(`批量刷新失败: ${msg}`, 'error')
    } finally {
      setPcgwRefreshLoading(false)
    }
  }

  const updateEditField = (field: string, value: any) => {
    setEditEntry((prev) => ({ ...prev, [field]: value }))
  }

  // 计算当前高亮选中的游戏条目
  const selectedEntry = entries.find((e) => e.id === selectedEntryId)

  return (
    <div className={styles.root}>
      <div className={styles.header}>
        <Title1>游戏数据库</Title1>
        <div className={styles.searchBox}>
          <Search24Regular />
          <Input
            placeholder="搜索游戏名称或 exe..."
            value={search}
            onChange={(e) => handleSearch(e.target.value)}
            className={styles.searchInput}
          />
        </div>
        <div className={styles.actions}>
          <Button
            icon={<Globe24Regular />}
            onClick={() => {
              setPcgwQuery('')
              setPcgwResults([])
              setPcgwSelected(null)
              setPcgwOpen(true)
            }}
          >
            从 PCGamingWiki
          </Button>
          <Button
            icon={<ArrowImport24Regular />}
            onClick={() => {
              setImportJson('')
              setImportOpen(true)
            }}
          >
            导入
          </Button>
          <Button icon={<ArrowExport24Regular />} onClick={handleExport}>
            导出
          </Button>
          <Button
            icon={<ArrowClockwise24Regular />}
            onClick={handleRefreshAll}
            disabled={pcgwRefreshLoading}
          >
            {pcgwRefreshLoading ? '刷新中...' : '刷新全部'}
          </Button>
          <Button
            icon={<Add24Regular />}
            appearance="primary"
            onClick={handleAdd}
          >
            添加条目
          </Button>
        </div>
      </div>

      {error && (
        <MessageBar intent="error">
          <MessageBarBody>{error}</MessageBarBody>
        </MessageBar>
      )}

      {loading ? (
        <Spinner label="加载中..." size="huge" />
      ) : entries.length === 0 ? (
        <div className={styles.empty}>
          {search ? '未找到匹配的游戏' : '数据库为空，点击右上角添加'}
        </div>
      ) : (
        <div className={styles.container}>
          {/* 左栏：Master 列表 */}
          <div className={styles.masterCol}>
            <div className={styles.tableCard}>
              <Table>
                <TableHeader className={styles.tableHeader}>
                  <TableRow>
                    <TableHeaderCell className={styles.tableHeaderCell}>游戏</TableHeaderCell>
                    <TableHeaderCell className={styles.tableHeaderCell}>别名 / exe</TableHeaderCell>
                    <TableHeaderCell className={styles.tableHeaderCell} style={{ width: '80px' }}>操作</TableHeaderCell>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {entries.map((entry) => {
                    const isSelected = entry.id === selectedEntryId
                    return (
                      <TableRow 
                        key={entry.id} 
                        className={`${styles.tableRow} ${isSelected ? styles.selectedRow : ''}`}
                        onClick={() => setSelectedEntryId(entry.id)}
                        style={{ cursor: 'pointer' }}
                      >
                        <TableCell>
                          <div style={{ fontWeight: 600 }}>{entry.name}</div>
                          {entry.notes && (
                            <div
                              style={{
                                fontSize: '12px',
                                color: tokens.colorNeutralForeground3,
                                textOverflow: 'ellipsis',
                                overflow: 'hidden',
                                whiteSpace: 'nowrap',
                                maxWidth: '300px'
                              }}
                            >
                              {entry.notes}
                            </div>
                          )}
                        </TableCell>
                        <TableCell>
                          <div
                            style={{
                              fontFamily: 'monospace',
                              fontSize: '13px',
                              color: tokens.colorNeutralForeground3,
                              textOverflow: 'ellipsis',
                              overflow: 'hidden',
                              whiteSpace: 'nowrap',
                              maxWidth: '200px'
                            }}
                          >
                            {entry.aliases.join(', ') || '-'}
                          </div>
                          {entry.steam_appid && (
                            <div
                              style={{
                                fontSize: '12px',
                                color: tokens.colorBrandForeground1,
                              }}
                            >
                              Steam ID: {entry.steam_appid}
                            </div>
                          )}
                        </TableCell>
                        <TableCell>
                          <div style={{ display: 'flex', gap: '2px' }} onClick={(e) => e.stopPropagation()}>
                            <Tooltip content="添加到本地游戏" relationship="label" positioning="above">
                              <Button
                                size="small"
                                appearance="subtle"
                                icon={<Games24Regular />}
                                onClick={() => handleCreateGame(entry.id)}
                              />
                            </Tooltip>
                            <Tooltip content="编辑" relationship="label" positioning="above">
                              <Button
                                size="small"
                                appearance="subtle"
                                icon={<Save24Regular />}
                                onClick={() => handleEdit(entry)}
                              />
                            </Tooltip>
                          </div>
                        </TableCell>
                      </TableRow>
                    )
                  })}
                </TableBody>
              </Table>
            </div>
          </div>

          {/* 右栏：磨砂高保真 Detail 详情面板 */}
          <div className={styles.detailCol}>
            {selectedEntry ? (
              <>
                <div className={styles.detailHeader}>
                  <div className={styles.posterContainer}>
                    {selectedEntry.steam_appid ? (
                      <img
                        className={styles.posterImage}
                        src={`https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${selectedEntry.steam_appid}/library_600x900.jpg`}
                        alt={selectedEntry.name}
                        onError={(e) => {
                          // 如果 Steam 海报直链加载失败，则退避到本地提取的 exe 图标 base64，或者 Fluent 占位
                          const img = e.currentTarget
                          if (selectedEntryLogo) {
                            img.src = selectedEntryLogo
                          } else {
                            // 隐藏原图并展示 noPoster
                            img.style.display = 'none'
                            const parent = img.parentElement
                            if (parent) {
                              const placeholder = parent.querySelector('.no-poster-placeholder')
                              if (placeholder) {
                                (placeholder as HTMLElement).style.display = 'flex'
                              }
                            }
                          }
                        }}
                      />
                    ) : selectedEntryLogo ? (
                      <img
                        className={styles.posterImage}
                        src={selectedEntryLogo}
                        alt={selectedEntry.name}
                      />
                    ) : null}

                    {/* 兜底 Fluent 磨砂占位界面 */}
                    <div
                      className="no-poster-placeholder"
                      style={{
                        display: (!selectedEntry.steam_appid && !selectedEntryLogo) ? 'flex' : 'none',
                        flexDirection: 'column',
                        alignItems: 'center',
                        justifyContent: 'center',
                        width: '100%',
                        height: '100%',
                        gap: '8px',
                        color: tokens.colorNeutralForeground4,
                      }}
                    >
                      <Image24Regular style={{ fontSize: '48px', width: '48px', height: '48px' }} />
                      <span style={{ fontSize: '12px' }}>无海报</span>
                    </div>
                  </div>
                  <div>
                    <div className={styles.detailTitle}>{selectedEntry.name}</div>
                    <div className={styles.detailSubtitle}>ID: {selectedEntry.id}</div>
                  </div>
                </div>

                <div className={styles.detailSection}>
                  <div className={styles.detailSectionTitle}>存档路径模板</div>
                  {selectedEntry.save_paths.length === 0 ? (
                    <div style={{ color: tokens.colorNeutralForeground4, fontSize: '13px' }}>未配置路径</div>
                  ) : (
                    selectedEntry.save_paths.map((path: string, idx: number) => (
                      <div key={idx} className={styles.detailPathCard}>
                        {path}
                      </div>
                    ))
                  )}
                </div>

                {selectedEntry.aliases.length > 0 && (
                  <div className={styles.detailSection}>
                    <div className={styles.detailSectionTitle}>别名与可执行文件</div>
                    <div style={{ display: 'flex', gap: '6px', flexWrap: 'wrap' }}>
                      {selectedEntry.aliases.map((alias: string, idx: number) => (
                        <Badge key={idx} appearance="outline">
                          {alias}
                        </Badge>
                      ))}
                    </div>
                  </div>
                )}

                <div className={styles.detailSection}>
                  <div className={styles.detailSectionTitle}>基础属性</div>
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                    <Badge appearance="filled" color={selectedEntry.source === 'builtin' ? 'informative' : 'success'}>
                      {selectedEntry.source === 'builtin' ? '内置游戏' : '自定义游戏'}
                    </Badge>
                    {selectedEntry.steam_appid && (
                      <Badge appearance="tint" color="brand">
                        Steam ID: {selectedEntry.steam_appid}
                      </Badge>
                    )}
                  </div>
                </div>

                {selectedEntry.notes && (
                  <div className={styles.detailSection}>
                    <div className={styles.detailSectionTitle}>备注说明</div>
                    <div style={{ fontSize: '13px', color: tokens.colorNeutralForeground3, lineHeight: '1.5' }}>
                      {selectedEntry.notes}
                    </div>
                  </div>
                )}

                <div className={styles.detailActions}>
                  <Button
                    appearance="primary"
                    icon={<Games24Regular />}
                    onClick={() => handleCreateGame(selectedEntry.id)}
                  >
                    一键加入我的游戏
                  </Button>
                  <div style={{ display: 'flex', gap: '10px' }}>
                    <Button
                      style={{ flexGrow: 1 }}
                      icon={<Save24Regular />}
                      onClick={() => handleEdit(selectedEntry)}
                    >
                      编辑游戏
                    </Button>
                    <Button
                      style={{ flexGrow: 1 }}
                      icon={<Dismiss24Regular />}
                      disabled={selectedEntry.source === 'builtin'}
                      onClick={() => handleRemove(selectedEntry.id)}
                    >
                      删除条目
                    </Button>
                  </div>
                </div>
              </>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100%', color: tokens.colorNeutralForeground4 }}>
                <Games24Regular style={{ fontSize: '48px', width: '48px', height: '48px', marginBottom: '12px' }} />
                <span>请选择左侧游戏查看详情</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* 添加/编辑弹窗 */}
      <Dialog
        open={editOpen}
        onOpenChange={(_, data) => !data.open && setEditOpen(false)}
      >
        <DialogSurface>
          <DialogBody>
            <DialogTitle>
              {isEditing ? '编辑条目' : '添加条目'}
            </DialogTitle>
            <DialogContent>
              <div className={styles.dialogContent}>
                <div className={styles.formField}>
                  <Label required>游戏名称</Label>
                  <Input
                    value={editEntry.name || ''}
                    onChange={(e) => updateEditField('name', e.target.value)}
                    placeholder="例如：Elden Ring"
                  />
                </div>

                <div className={styles.formField}>
                  <Label>别名 / 常见 exe（逗号分隔）</Label>
                  <Input
                    value={(editEntry.aliases || []).join(', ')}
                    onChange={(e) =>
                      updateEditField(
                        'aliases',
                        e.target.value
                          .split(',')
                          .map((s) => s.trim())
                          .filter((s) => s),
                      )
                    }
                    placeholder="eldenring.exe, ER.exe"
                  />
                </div>

                <div className={styles.formField}>
                  <Label required>存档路径模板（每行一个）</Label>
                  <Textarea
                    value={(editEntry.save_paths || []).join('\n')}
                    onChange={(e) =>
                      updateEditField(
                        'save_paths',
                        e.target.value
                          .split('\n')
                          .map((s) => s.trim())
                          .filter((s) => s),
                      )
                    }
                    placeholder={
                      '%APPDATA%/EldenRing\n%USERPROFILE%/Documents/My Games/...'
                    }
                  />
                </div>

                <div className={styles.formField}>
                  <Label>Steam AppID</Label>
                  <div style={{ display: 'flex', gap: '8px' }}>
                    <Input
                      type="number"
                      value={editEntry.steam_appid?.toString() || ''}
                      onChange={(e) => {
                        const v = parseInt(e.target.value, 10)
                        updateEditField(
                          'steam_appid',
                          isNaN(v) ? undefined : v,
                        )
                      }}
                      placeholder="1245620"
                      style={{ flexGrow: 1 }}
                    />
                    <Button
                      icon={<ArrowClockwise24Regular />}
                      disabled={autoAppIdLoading}
                      onClick={handleAutoGetAppId}
                    >
                      {autoAppIdLoading ? '获取中...' : '🔍 自动获取'}
                    </Button>
                  </div>
                </div>

                <div className={styles.formField}>
                  <Label>关联本地可执行文件（提取高清原装图标）</Label>
                  <div style={{ display: 'flex', gap: '8px' }}>
                    <Input
                      value={exePath}
                      readOnly
                      placeholder="未关联 exe"
                      style={{ flexGrow: 1 }}
                    />
                    <Button
                      icon={<FolderOpen24Regular />}
                      onClick={handleSelectExe}
                    >
                      选择 EXE
                    </Button>
                  </div>
                  {exeIconBase64 && (
                    <div className={styles.exePreview}>
                      <img
                        className={styles.exePreviewIcon}
                        src={exeIconBase64}
                        alt="EXE icon preview"
                      />
                      <div style={{ display: 'flex', flexDirection: 'column' }}>
                        <span style={{ fontSize: '12px', fontWeight: 600, color: tokens.colorNeutralForeground2 }}>已提取高清原装图标</span>
                        <span style={{ fontSize: '11px', color: tokens.colorNeutralForeground4 }}>保存后该图标将被永久缓存在数据库中</span>
                      </div>
                      <Button
                        size="small"
                        appearance="subtle"
                        icon={<Dismiss24Regular />}
                        style={{ marginLeft: 'auto' }}
                        onClick={() => {
                          setExePath('')
                          setExeIconBase64(null)
                        }}
                      />
                    </div>
                  )}
                </div>

                <div className={styles.formField}>
                  <Label>备注</Label>
                  <Input
                    value={editEntry.notes || ''}
                    onChange={(e) => updateEditField('notes', e.target.value)}
                    placeholder="路径说明、注意事项等"
                  />
                </div>
              </div>
            </DialogContent>
            <DialogActions>
              <Button
                appearance="secondary"
                onClick={() => setEditOpen(false)}
              >
                取消
              </Button>
              <Button appearance="primary" onClick={handleSave}>
                保存
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>

      {/* 导入弹窗 */}
      <Dialog
        open={importOpen}
        onOpenChange={(_, data) => !data.open && setImportOpen(false)}
      >
        <DialogSurface>
          <DialogBody>
            <DialogTitle>导入数据库</DialogTitle>
            <DialogContent>
              <div className={styles.formField}>
                <Label>粘贴 JSON</Label>
                <Textarea
                  className={styles.importTextarea}
                  value={importJson}
                  onChange={(e) => setImportJson(e.target.value)}
                  placeholder={'{\n  "entries": [...],\n  "version": 1\n}'}
                />
              </div>
            </DialogContent>
            <DialogActions>
              <Button
                appearance="secondary"
                onClick={() => setImportOpen(false)}
              >
                取消
              </Button>
              <Button appearance="primary" onClick={handleImport}>
                导入
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>

      {/* PCGamingWiki 搜索弹窗 */}
      <Dialog
        open={pcgwOpen}
        onOpenChange={(_, data) => !data.open && setPcgwOpen(false)}
      >
        <DialogSurface>
          <DialogBody>
            <DialogTitle>从 PCGamingWiki 搜索</DialogTitle>
            <DialogContent>
              <div className={styles.dialogContent}>
                <div className={styles.formField}>
                  <Label>游戏名称</Label>
                  <Input
                    value={pcgwQuery}
                    onChange={(e) => setPcgwQuery(e.target.value)}
                    placeholder="中文或英文均可，例如：艾尔登法环 / Elden Ring"
                    onKeyDown={(e) => e.key === 'Enter' && handlePcgwSearch()}
                  />
                  <div style={{ display: 'flex', gap: '8px', marginTop: '8px' }}>
                    <Button
                      appearance="primary"
                      onClick={handlePcgwSearch}
                      disabled={pcgwLoading}
                      style={{ flexGrow: 1 }}
                    >
                      {pcgwLoading ? '搜索中...' : '搜索（支持中文/英文）'}
                    </Button>
                  </div>

                </div>

                {pcgwLoading && !pcgwSelected && (
                  <Spinner label="搜索中..." />
                )}

                {pcgwResults.length > 0 && !pcgwSelected && (
                  <div>
                    <Label>搜索结果（点击查看存档路径）</Label>
                    <div
                      style={{
                        display: 'flex',
                        flexDirection: 'column',
                        gap: '8px',
                        marginTop: '8px',
                      }}
                    >
                      {pcgwResults.map((r) => (
                        <div
                          key={r.page_name}
                          className={styles.resultCard}
                          onClick={() => handlePcgwSelect(r.page_name)}
                        >
                          <div style={{ fontWeight: 600 }}>{r.page_name}</div>
                          <div
                            style={{
                              fontSize: '12px',
                              color: tokens.colorNeutralForeground3,
                            }}
                          >
                            {r.steam_appid ? `Steam AppID: ${r.steam_appid}` : '暂无 AppID'}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {pcgwSelected && (
                  <div>
                    <div
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                      }}
                    >
                      <Label>存档路径（Windows）</Label>
                      <Button
                        size="small"
                        onClick={() => setPcgwSelected(null)}
                      >
                        返回结果
                      </Button>
                    </div>
                    <div className={styles.pcgwPathsContainer}>
                      {pcgwSelected.windows_save_paths.length === 0 ? (
                        <div>未找到 Windows 存档路径</div>
                      ) : (
                        pcgwSelected.windows_save_paths.map((p, i) => (
                          <div key={i}>{p}</div>
                        ))
                      )}
                    </div>
                    {pcgwSelected.notes && (
                      <div className={styles.pcgwNotes}>
                        备注: {pcgwSelected.notes}
                      </div>
                    )}
                  </div>
                )}
              </div>
            </DialogContent>
            <DialogActions>
              <Button
                appearance="secondary"
                onClick={() => setPcgwOpen(false)}
              >
                取消
              </Button>
              <Button
                appearance="primary"
                icon={<ArrowDownload24Regular />}
                onClick={handlePcgwImport}
                disabled={!pcgwSelected || pcgwSelected.windows_save_paths.length === 0}
              >
                导入到数据库
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>

      {/* 导出弹窗 */}
      <Dialog
        open={exportOpen}
        onOpenChange={(_, data) => !data.open && setExportOpen(false)}
      >
        <DialogSurface>
          <DialogBody>
            <DialogTitle>导出数据库</DialogTitle>
            <DialogContent>
              <div className={styles.formField}>
                <Label>复制下方 JSON 即可分发</Label>
                <Textarea
                  className={styles.importTextarea}
                  value={exportJson}
                  readOnly
                  onFocus={(e) => e.target.select()}
                />
              </div>
            </DialogContent>
            <DialogActions>
              <Button
                appearance="secondary"
                onClick={() => setExportOpen(false)}
              >
                关闭
              </Button>
              <Button
                appearance="primary"
                onClick={() => {
                  navigator.clipboard.writeText(exportJson)
                  addToast('已复制到剪贴板', 'success')
                }}
              >
                复制
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </div>
  )
}
