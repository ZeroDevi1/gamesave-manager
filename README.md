# GameSave Manager

一个基于 Tauri 的跨平台游戏存档备份管理工具，帮助玩家快速备份、恢复和管理本地游戏存档。

## 技术栈

- **桌面框架**: [Tauri 2.x](https://tauri.app/)（Rust 后端 + Web 前端）
- **前端**: React 19 + TypeScript + Vite
- **UI 组件**: [Fluent UI React](https://react.fluentui.dev/)
- **状态管理**: Zustand
- **路由**: React Router
- **后端**: Rust（Tokio 异步运行时）

## 功能特性

- 扫描并识别本地已安装的游戏及其存档路径
- 一键备份游戏存档为压缩包（ZIP 格式）
- 从备份快速恢复存档
- 存档版本管理与历史记录
- Alist 网盘集成，支持远程备份与同步
- 存档完整性校验（SHA2 / MD5）
- Windows 原生支持，自动提取游戏图标

## 快速开始

### 环境要求

- [Rust](https://rustup.rs/) >= 1.77.2
- [Node.js](https://nodejs.org/) >= 18
- Windows 10/11（当前主要支持平台）

### 安装依赖

```bash
# 安装前端依赖
cd ui && npm install

# Rust 依赖由 cargo 自动管理
```

### 开发运行

```bash
# 启动 Tauri 开发模式（同时运行前端 dev server 与 Rust 后端）
npm run dev
```

### 构建发行版

```bash
# 构建 Windows MSI 安装包
npm run build
```

## 项目结构

```
gamesave-manager/
├── core/               # Tauri Rust 后端
│   ├── src/            # Rust 源码
│   ├── icons/          # 应用图标
│   └── tauri.conf.json # Tauri 配置
├── ui/                 # React 前端
│   ├── src/            # 前端源码
│   ├── public/         # 静态资源
│   └── index.html      # 入口 HTML
├── Cargo.toml          # Rust Workspace 配置
└── package.json        # 根目录脚本
```

## 开源协议

[MIT](LICENSE)
