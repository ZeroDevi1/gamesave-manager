// services/tauri.ts - Tauri invoke 命令封装层
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
export { convertFileSrc }

// ==================== Alist ====================

export interface AlistLoginParams {
  url: string
  username: string
  password: string
}

export interface AlistLoginResult {
  token: string
}

export interface AlistFileEntry {
  name: string
  path: string
  size: number
  is_dir: boolean
  modified?: string
}

export function alistLogin(params: AlistLoginParams): Promise<AlistLoginResult> {
  return invoke('alist_login', { ...params })
}

export function alistListDir(
  url: string,
  token: string,
  path: string,
): Promise<AlistFileEntry[]> {
  return invoke('alist_list_dir', { url, token, path })
}

/**
 * 上传本地文件到 Alist 云盘的指定远程目录
 * @param url Alist 服务器基准 URL 地址
 * @param token 已登录用户的授权认证凭证 Token
 * @param localPath 本地待上传的物理文件绝对路径
 * @param remotePath Alist 云端存放该文件的目标绝对路径
 */
export function alistUpload(
  url: string,
  token: string,
  localPath: string,
  remotePath: string,
): Promise<void> {
  return invoke('alist_upload', { url, token, localPath, remotePath })
}

export function alistMkdir(
  url: string,
  token: string,
  path: string,
): Promise<void> {
  return invoke('alist_mkdir', { url, token, path })
}

// ==================== 游戏 ====================

export interface GameConfig {
  id: string
  name: string
  save_paths: string[]
  remote_path: string
  last_backup?: string
  logo_path?: string
  steam_appid?: number
}

export interface SaveFile {
  path: string
  relative_path: string
  size: number
  modified_time: string
}

export function getGames(): Promise<GameConfig[]> {
  return invoke('get_games')
}

/**
 * 向本地配置文件中注册添加一个新的游戏配置项
 * @param name 游戏显示名称（如 "Elden Ring"）
 * @param savePaths 该游戏关联的本地存档物理路径模板列表
 */
export function addGame(name: string, savePaths: string[]): Promise<GameConfig> {
  return invoke('add_game', { name, savePaths })
}

/**
 * 从本地配置文件中永久移除指定的游戏及其所有备份元数据记录
 * @param gameId 待删除的游戏唯一标识符 ID
 */
export function removeGame(gameId: string): Promise<void> {
  return invoke('remove_game', { gameId })
}

/**
 * 实时扫描指定游戏在本地对应的实际存档路径，分析并提取所有存档文件清单
 * @param gameId 需要扫描的游戏唯一标识符 ID
 * @returns 返回扫描出来的物理存档文件及其元信息元组数组
 */
export function scanGameSaves(gameId: string): Promise<SaveFile[]> {
  return invoke('scan_game_saves', { gameId })
}

/**
 * 异步获取指定游戏的高保真 Logo 图标海报路径（支持本地缓存与 SteamGridDB 在线拉取）
 * @param gameId 游戏唯一标识符 ID
 * @param steamAppid 可选的游戏 Steam AppID（加速匹配）
 * @returns 成功则返回 Logo 文件的本地物理绝对路径或 base64，失败或未找到则返回 null
 */
export function getGameLogo(gameId: string, steamAppid?: number): Promise<string | null> {
  return invoke('get_game_logo', { gameId, steamAppid })
}


// ==================== PCGamingWiki ====================

export interface PcgwSearchResult {
  page_name: string
  steam_appid?: number
}

export interface PcgwGameDetail {
  page_name: string
  steam_appid?: number
  windows_save_paths: string[]
  notes?: string
}

export function searchPcgwGames(query: string): Promise<PcgwSearchResult[]> {
  return invoke('search_pcgw_games', { query })
}

/**
 * 根据 PCGamingWiki 游戏页面词条英文唯一名，爬取、解析并提取该游戏的 Windows 存档路径模板和备注
 * @param pageName PCGamingWiki 的维基页面英文标题名（如 "Kingdom Come: Deliverance II"）
 * @returns 解析得到的存档路径详情，包含可能的备注与 Steam AppID
 */
export function fetchPcgwSavePaths(pageName: string): Promise<PcgwGameDetail> {
  return invoke('fetch_pcgw_save_paths', { pageName })
}

export function searchSteamStore(query: string): Promise<{ name: string; id: number }[]> {
  return invoke('search_steam_store_cmd', { query })
}
export function searchSteamStoreRobust(query: string): Promise<[{ name: string; id: number }[], string]> {
  return invoke('search_steam_store_robust_cmd', { query })
}
export function searchPcgwBySteamAppid(appid: number): Promise<PcgwSearchResult[]> {
  return invoke('search_pcgw_by_steam_appid', { appid })
}

// ==================== 游戏数据库 ====================
// ==================== 游戏数据库 ====================

export interface GameDbEntry {
  id: string
  name: string
  aliases: string[]
  save_paths: string[]
  platforms: string[]
  steam_appid?: number
  notes?: string
  source: string
}

export interface GameDatabase {
  entries: GameDbEntry[]
  version: number
}

export function getGameDb(): Promise<GameDatabase> {
  return invoke('get_game_db')
}

export function searchGameDb(query: string): Promise<GameDbEntry[]> {
  return invoke('search_game_db', { query })
}

export function upsertGameDbEntry(entry: GameDbEntry): Promise<GameDbEntry> {
  return invoke('upsert_game_db_entry', { entry })
}

export function removeGameDbEntry(id: string): Promise<void> {
  return invoke('remove_game_db_entry', { id })
}

export function exportGameDb(): Promise<string> {
  return invoke('export_game_db')
}

export function importGameDb(json: string): Promise<boolean> {
  return invoke('import_game_db', { json })
}

/**
 * 基于游戏数据库中的模板条目，为本地直接初始化并创建一个全新的游戏同步备份配置项
 * @param dbId 游戏数据库条目的唯一标识符 ID（如 "elden-ring"）
 */
export function createGameFromDb(dbId: string): Promise<GameConfig> {
  return invoke('create_game_from_db', { dbId })
}
export function refreshGameDbSavePaths(): Promise<[string, number, number][]> {
  return invoke('refresh_game_db_save_paths')
}

// ==================== 备份 ====================

export interface BackupResult {
  success: boolean
  message: string
  files_backed_up: number
  timestamp: string
}

export interface RestoreResult {
  success: boolean
  message: string
}

export interface BackupManifest {
  game_id: string
  backup_type: 'full' | 'incremental'
  timestamp: string
  files: {
    relative_path: string
    size: number
    modified_time: string
    sha256: string
  }[]
  target_path: string
  zip_file?: string
}

export interface RemoteBackupEntry {
  name: string
  path: string
  size: number
  modified?: string
}

/**
 * 列出远程（Alist 网盘）上的备份 ZIP 文件列表
 * @param gameId 游戏唯一标识符 ID
 */
export function listRemoteBackups(gameId: string): Promise<RemoteBackupEntry[]> {
  return invoke('list_remote_backups', { gameId })
}

/**
 * 从远程备份 ZIP 文件恢复存档
 * @param gameId 游戏唯一标识符 ID
 * @param remoteZipPath 远程 ZIP 文件绝对路径
 */
export function restoreRemoteBackup(gameId: string, remoteZipPath: string): Promise<RestoreResult> {
  return invoke('restore_remote_backup', { gameId, remoteZipPath })
}

/**
 * 对指定游戏执行一次完整（全量）的本地存档压缩归档备份
 * @param gameId 游戏唯一标识符 ID
 * @returns 备份结果明细，包含文件计数、耗时及是否成功等
 */
export function backupFull(gameId: string): Promise<BackupResult> {
  return invoke('backup_full', { gameId })
}

/**
 * 对指定游戏执行一次轻量级的增量备份（仅备份自上次备份以来发生内容改变的存档文件）
 * @param gameId 游戏唯一标识符 ID
 * @returns 增量备份结果明细
 */
export function backupIncremental(gameId: string): Promise<BackupResult> {
  return invoke('backup_incremental', { gameId })
}

/**
 * 从指定的备份历史时间戳镜像，恢复对应游戏的本地物理存档
 * @param gameId 游戏唯一标识符 ID
 * @param backupTimestamp 待恢复的目标备份历史记录唯一时间戳字符标识
 */
export function restoreBackup(gameId: string, backupTimestamp: string): Promise<RestoreResult> {
  return invoke('restore_backup', { gameId, backupTimestamp })
}

/**
 * 加载并获取指定游戏的所有历史备份存档记录清单（包含元数据及 SHA256 校验列表）
 * @param gameId 游戏唯一标识符 ID
 */
export function getBackupHistory(gameId: string): Promise<BackupManifest[]> {
  return invoke('get_backup_history', { gameId })
}

/** 检查所有游戏是否有未备份的存档变更，返回 (游戏ID, 变更文件数) 列表 */
export function checkAllGamesForChanges(): Promise<[string, number][]> {
  return invoke('check_all_games_for_changes')
}

/** 一键增量备份所有有变更的游戏，返回 (游戏ID, 成功, 消息) 列表 */
export function backupAllChangedGames(): Promise<[string, boolean, string][]> {
  return invoke('backup_all_changed_games')
}

// ==================== 配置 ====================

export interface AlistConfig {
  base_url: string
  username: string
  token?: string
  password?: string
  provider: string
  backup_root?: string
}

export type StorageConfig =
  | {
      type: 'netdisk'
      driver: string // 选取的云网盘物理驱动类型（如 baiduyun_go、onedrive_go 等）
      token: string // 访问令牌 (Access Token)
      refresh_token?: string // 刷新令牌 (Refresh Token，主要用于突破 Access Token 时效限制，非必填以向下兼容旧版)
      backup_root?: string
    }
  | {
      type: 'alist'
      base_url: string
      username: string
      token?: string
      password?: string
      provider: string
      backup_root?: string
    }
  | {
      type: 'webdav'
      endpoint: string
      username: string
      password: string
      backup_root?: string
    }
  | {
      type: 's3'
      endpoint: string
      bucket: string
      access_key_id: string
      secret_access_key: string
      region?: string
      backup_root?: string
    }

export interface Settings {
  theme: string
  steamgriddb_api_key?: string
}

export interface AppConfig {
  storage?: StorageConfig
  games: GameConfig[]
  settings: Settings
  alist?: AlistConfig
}

export interface RemoteFileEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
  modified?: string
}

export function loadConfig(): Promise<AppConfig> {
  return invoke('load_config')
}

export function saveConfig(config: AppConfig): Promise<boolean> {
  return invoke('save_config', { config })
}

// ==================== 统一存储交互命令 ====================

/**
 * 一键测试存储后端的连通性（支持临时未存盘参数测试）
 */
export function storageTestConnection(config: StorageConfig): Promise<boolean> {
  return invoke('storage_test_connection', { config })
}

/**
 * 通用云端物理目录列表浏览
 */
export function storageListDir(
  path: string,
  config?: StorageConfig,
): Promise<RemoteFileEntry[]> {
  return invoke('storage_list_dir', { path, config })
}

/**
 * 轮询并刷新全部已配置网盘的 Access & Refresh Token 凭证并自动持久化写盘
 */
export function storageRefreshAllTokens(): Promise<boolean> {
  return invoke('storage_refresh_all_tokens')
}

// ==================== 自定义封面与物理图标提取 ====================

export interface ExeIconResult {
  base64: string | null
  path: string
}

export function selectAndExtractExeIcon(): Promise<ExeIconResult | null> {
  return invoke('select_and_extract_exe_icon')
}

export function saveCustomLogo(gameId: string, logoBase64: string): Promise<void> {
  return invoke('save_custom_logo', { gameId, logoBase64 })
}

export function getDbGameLogo(gameId: string): Promise<string | null> {
  return invoke('get_db_game_logo', { gameId })
}
// ==================== 游戏启动 ====================

/**
 * 通过 Steam AppID 启动游戏
 * @param steamAppid 游戏的 Steam AppID
 */
export function launchGame(steamAppid: number): Promise<void> {
  return invoke('launch_game', { steamAppid })
}

// ==================== 夸克 TV 扫码登录 ====================

/** 获取夸克 TV 登录二维码（返回 base64 PNG 图片数据和 query_token） */
export function quarkTvGetQrCode(): Promise<[string, string]> {
  return invoke('quark_tv_get_qr_code')
}

/** 轮询夸克 TV 授权状态（返回 code 或 null） */
export function quarkTvPollQr(queryToken: string): Promise<string | null> {
  return invoke('quark_tv_poll_qr', { queryToken })
}

/** 用授权码交换 AccessToken 和 RefreshToken */
export function quarkTvExchange(code: string): Promise<[string, string]> {
  return invoke('quark_tv_exchange', { code })
}
