// services/api.ts - Go 后端调用封装层
//
// 页面只依赖这里导出的函数与类型；底层调用由 `mygo generate` 生成的 ../mygo.ts 完成。
// 所有调用失败时以错误消息字符串 reject（与 Go 方法返回的 error 文本一致），页面据此展示提示。
import { isCallError, isMyGo, runtime } from 'mygo-runtime'
import {
  AlistService,
  AppService,
  BackupService,
  ConfigService,
  GameDbService,
  GamesService,
  PcgwService,
  StorageService,
} from '../mygo'
import type {
  AppConfig as GoAppConfig,
  StorageConfig as GoStorageConfig,
} from '../mygo'

export type {
  BackupManifest,
  BackupResult,
  GameConfig,
  GameDatabase,
  GameDbEntry,
  PcgwGameDetail,
  PcgwSearchResult,
  RemoteBackupEntry,
  RemoteFileEntry,
  RestoreResult,
  SaveFile,
  ExeIconResult,
  NetdiskTokens,
} from '../mygo'
export type { FileEntry as AlistFileEntry, LoginResult as AlistLoginResult } from '../mygo'
export type { CheckResult as UpdateCheckResult } from '../mygo'
import type { GameConfig } from '../mygo'

/** 把 Go 侧错误转换为其消息字符串后重新抛出，其它异常原样抛出。 */
async function unwrap<T>(p: Promise<T>): Promise<T> {
  try {
    return await p
  } catch (err) {
    throw isCallError(err) ? err.message : err
  }
}

// ==================== 本地封面 ====================

/** 封面缓存目录由 Go 以 logos 协议提供；WebView2 把自定义协议映射到 http://<scheme>.localhost。 */
function logoOrigin(): string {
  return isMyGo() && runtime().platform === 'win32' ? 'http://logos.localhost' : 'logos://localhost'
}

/** 把封面缓存中的本地文件路径转换为页面可加载的 URL。 */
export function localFileSrc(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? ''
  return `${logoOrigin()}/${encodeURIComponent(name)}`
}

// ==================== Alist ====================

export interface AlistLoginParams {
  url: string
  username: string
  password: string
}

export function alistLogin(params: AlistLoginParams) {
  return unwrap(AlistService.login(params.url, params.username, params.password))
}

export function alistListDir(url: string, token: string, path: string) {
  return unwrap(AlistService.listDir(url, token, path))
}

/** 上传本地文件到 Alist 云盘的指定远程路径 */
export function alistUpload(url: string, token: string, localPath: string, remotePath: string) {
  return unwrap(AlistService.upload(url, token, localPath, remotePath))
}

export function alistMkdir(url: string, token: string, path: string) {
  return unwrap(AlistService.mkdir(url, token, path))
}

// ==================== 游戏 ====================

export function getGames() {
  return unwrap(GamesService.list())
}

/** 手动添加游戏：name 为显示名，savePaths 为存档路径（可含环境变量占位符与通配符） */
export function addGame(name: string, savePaths: string[]): Promise<GameConfig> {
  return unwrap(GamesService.add(name, savePaths))
}

/** 删除游戏配置（不删除云端备份） */
export function removeGame(gameId: string) {
  return unwrap(GamesService.remove(gameId))
}

/** 扫描游戏的本地存档文件 */
export function scanGameSaves(gameId: string) {
  return unwrap(GamesService.scanSaves(gameId))
}

/** 获取封面的本地缓存路径（必要时按 Steam AppID 下载），取不到时为 null */
export function getGameLogo(gameId: string, steamAppid?: number) {
  return unwrap(GamesService.getLogo(gameId, steamAppid ?? null))
}

// ==================== PCGamingWiki ====================

export function searchPcgwGames(query: string) {
  return unwrap(PcgwService.search(query))
}

/** 解析 PCGamingWiki 页面（英文标题，如 "Kingdom Come: Deliverance II"）的 Windows 存档路径 */
export function fetchPcgwSavePaths(pageName: string) {
  return unwrap(PcgwService.fetchSavePaths(pageName))
}

export function searchSteamStore(query: string) {
  return unwrap(PcgwService.searchSteamStore(query))
}

/** 多策略搜索 Steam 商店，返回 [结果, 实际命中的查询词] */
export async function searchSteamStoreRobust(query: string): Promise<[{ name: string; id: number }[], string]> {
  const r = await unwrap(PcgwService.searchSteamStoreRobust(query))
  return [r.items, r.query]
}

export function searchPcgwBySteamAppid(appid: number) {
  return unwrap(PcgwService.searchBySteamAppid(appid))
}

// ==================== 游戏数据库 ====================

export function getGameDb() {
  return unwrap(GameDbService.get())
}

export function searchGameDb(query: string) {
  return unwrap(GameDbService.search(query))
}

export function upsertGameDbEntry(entry: Parameters<typeof GameDbService.upsert>[0]) {
  return unwrap(GameDbService.upsert(entry))
}

export function removeGameDbEntry(id: string) {
  return unwrap(GameDbService.remove(id))
}

export function exportGameDb() {
  return unwrap(GameDbService.export())
}

export function importGameDb(json: string) {
  return unwrap(GameDbService.import(json))
}

/** 用游戏数据库条目（如 "elden-ring"）创建本地游戏 */
export function createGameFromDb(dbId: string) {
  return unwrap(GameDbService.createGame(dbId))
}

/** 从 PCGamingWiki 补充数据库存档路径，返回 [游戏名, 新增数, 总数] 列表 */
export async function refreshGameDbSavePaths(): Promise<[string, number, number][]> {
  const list = await unwrap(GameDbService.refreshSavePaths())
  return list.map((u) => [u.name, u.added, u.total_paths])
}

// ==================== 备份 ====================

/** 列出云端全量备份 ZIP */
export function listRemoteBackups(gameId: string) {
  return unwrap(BackupService.listRemote(gameId))
}

/** 从云端 ZIP 还原存档 */
export function restoreRemoteBackup(gameId: string, remoteZipPath: string) {
  return unwrap(BackupService.restoreRemote(gameId, remoteZipPath))
}

/** 全量备份 */
export function backupFull(gameId: string) {
  return unwrap(BackupService.full(gameId))
}

/** 增量备份（只上传有变化的文件） */
export function backupIncremental(gameId: string) {
  return unwrap(BackupService.incremental(gameId))
}

/** 按本地清单时间戳还原全量备份 */
export function restoreBackup(gameId: string, backupTimestamp: string) {
  return unwrap(BackupService.restore(gameId, backupTimestamp))
}

/** 本地备份清单 */
export function getBackupHistory(gameId: string) {
  return unwrap(BackupService.history(gameId))
}

/** 检查所有游戏是否有未备份的存档变更，返回 (游戏ID, 变更文件数) 列表 */
export async function checkAllGamesForChanges(): Promise<[string, number][]> {
  const list = await unwrap(BackupService.checkAllForChanges())
  return list.map((c) => [c.game_id, c.changed_files])
}

/** 一键增量备份所有有变更的游戏，返回 (游戏ID, 成功, 消息) 列表 */
export async function backupAllChangedGames(): Promise<[string, boolean, string][]> {
  const list = await unwrap(BackupService.backupAllChanged())
  return list.map((r) => [r.game_id, r.success, r.message])
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

/** 页面使用的存储配置：以 type 区分的联合类型（Go 侧为同一 JSON 形态的扁平结构） */
export type StorageConfig =
  | {
      type: 'netdisk'
      driver: string // 网盘驱动（如 baiduyun_go、onedrive_go、quark）
      token: string // 访问令牌（夸克为 Cookie）
      refresh_token?: string // 刷新令牌，用于突破 Access Token 时效
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

/** 页面可见的设置字段；auto_backup 等未列出的字段由 Go 侧在缺失时取默认值 */
export interface Settings {
  theme: string
  steamgriddb_api_key?: string
  backup_retention_limit?: number
}

export interface AppConfig {
  storage?: StorageConfig
  games: GameConfig[]
  settings: Settings
  alist?: AlistConfig
}

export async function loadConfig(): Promise<AppConfig> {
  // Go 侧的 StorageConfig 是扁平结构，字段集合与页面的联合类型一致
  return (await unwrap(ConfigService.load())) as unknown as AppConfig
}

export function saveConfig(config: AppConfig) {
  return unwrap(ConfigService.save(config as unknown as GoAppConfig))
}

// ==================== 统一存储 ====================

/** 测试存储连通性（可用尚未保存的临时配置） */
export function storageTestConnection(config: StorageConfig) {
  return unwrap(StorageService.testConnection(config as GoStorageConfig))
}

/** 列出云端目录；不传 config 时使用已保存的配置 */
export function storageListDir(path: string, config?: StorageConfig) {
  return unwrap(StorageService.listDir(path, (config as GoStorageConfig | undefined) ?? null))
}

/** 刷新已配置网盘的 Access/Refresh Token 并写回配置 */
export function storageRefreshAllTokens() {
  return unwrap(StorageService.refreshAllTokens())
}

/** 打开 api.oplist.org 授权小窗并自动捕获令牌；用户关闭窗口时为 null */
export function oplistAuth(driver: string) {
  return unwrap(StorageService.oplistAuth(driver))
}

// ==================== 应用更新 ====================

export function getAppVersion() {
  return unwrap(AppService.version())
}

export function checkForUpdates() {
  return unwrap(AppService.checkForUpdates())
}

export function openReleasePage(url: string) {
  return unwrap(AppService.openReleasePage(url))
}

// ==================== 自定义封面与图标提取 ====================

export function selectAndExtractExeIcon() {
  return unwrap(GamesService.selectAndExtractExeIcon())
}

export function saveCustomLogo(gameId: string, logoBase64: string) {
  return unwrap(GamesService.saveCustomLogo(gameId, logoBase64))
}

export function getDbGameLogo(gameId: string) {
  return unwrap(GamesService.getDbGameLogo(gameId))
}

// ==================== 游戏启动 ====================

/** 通过 Steam AppID 启动游戏 */
export function launchGame(steamAppid: number) {
  return unwrap(GamesService.launch(steamAppid))
}

// ==================== 夸克 TV 扫码登录 ====================

/** 获取夸克 TV 登录二维码，返回 [PNG base64, query_token] */
export async function quarkTvGetQrCode(): Promise<[string, string]> {
  const qr = await unwrap(StorageService.quarkTVGetQRCode())
  return [qr.qr_data, qr.query_token]
}

/** 轮询夸克 TV 授权状态（返回 code 或 null） */
export function quarkTvPollQr(queryToken: string) {
  return unwrap(StorageService.quarkTVPollCode(queryToken))
}

/** 用授权码交换令牌，返回 [AccessToken, RefreshToken] */
export async function quarkTvExchange(code: string): Promise<[string, string]> {
  const t = await unwrap(StorageService.quarkTVExchange(code))
  return [t.access_token, t.refresh_token]
}
