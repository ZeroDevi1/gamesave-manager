# GameSave Manager

一个基于 [MyGo](https://mygo.egoist.dev/) 的游戏存档备份管理工具，帮助玩家快速备份、恢复和管理本地游戏存档。

> Tauri（Rust）版本已归档在 `archive/tauri` 分支与 `v1.1.0-tauri` 标签。

## 技术栈

- **桌面框架**: [MyGo](https://mygo.egoist.dev/)（Go 后端 + 系统 WebView 前端，Windows 上为 WebView2）
- **后端**: Go，经 `mygo.Bind` 暴露服务，TypeScript 客户端由 `mygo generate` 生成到 `src/mygo.ts`
- **前端**: React 19 + TypeScript + Vite
- **UI 组件**: [Fluent UI React](https://react.fluentui.dev/)
- **状态管理**: Zustand
- **路由**: React Router

## 功能特性

- 扫描并识别本地已安装的游戏及其存档路径
- 一键备份游戏存档为压缩包（ZIP 格式），支持增量备份
- 从备份快速恢复存档
- 存档版本管理与历史记录
- 云端存储：直连网盘（百度 / 夸克 / 经 OpenList 网关的其它网盘）、Alist/OpenList、WebDAV
- 存档完整性校验（SHA256）
- Windows 原生支持，自动提取游戏图标

## 快速开始

### 环境要求

- [Go](https://go.dev/) >= 1.27
- [Bun](https://bun.sh/)
- Windows 10/11（当前主要支持平台；Windows 10 需安装 WebView2 Runtime）

### 安装依赖

```bash
bun install
```

### 开发运行

```bash
# 启动 Vite dev server 与开发版应用，修改 Go 代码后自动重新编译并重启
bun run dev
```

### 构建发行版

```bash
# 输出到 build/windows-amd64/：可执行文件与 NSIS 安装包
bun run build
```

## 数据位置

配置、备份清单与封面缓存位于 `%LOCALAPPDATA%\com.gamesave.manager`，与 Tauri 版相同，升级后无需迁移。

## 项目结构

```
gamesave-manager/
├── main.go             # 应用入口：绑定服务、主窗口、封面协议
├── services.go         # 暴露给前端的 Go 服务
├── oplist.go           # api.oplist.org 网盘授权小窗
├── internal/           # 后端实现
│   ├── alist/          # Alist / OpenList 客户端
│   ├── backup/         # 全量/增量备份、还原、启动前同步
│   ├── config/         # 配置模型与读写
│   ├── game/           # 游戏数据库、PCGamingWiki、封面、图标、进程监控
│   ├── storage/        # 云端存储后端（网盘、Alist、WebDAV、S3 占位）
│   └── ...
├── src/                # React 前端（src/mygo.ts 为生成的客户端）
├── resources/          # 应用图标
├── mygo.config.ts      # MyGo 配置
└── package.json        # 前端依赖与脚本
```

## 开源协议

[MIT](LICENSE)
