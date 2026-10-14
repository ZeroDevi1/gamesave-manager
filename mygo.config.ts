import { defineConfig } from "mygo-cli";
import pkg from "./package.json" with { type: "json" };

export default defineConfig({
  name: "gamesave-manager",
  // 沿用 Tauri 版的标识符：数据目录（%LOCALAPPDATA%\com.gamesave.manager）由 Go 侧按它定位，改动会让旧配置与备份清单失联
  identifier: "com.gamesave.manager",
  version: pkg.version,
  // `mygo dev` 运行 devCommand 并加载 devUrl；`mygo build` 运行 buildCommand 并把 frontendDist 嵌入可执行文件
  devUrl: "http://localhost:5173",
  devCommand: "bun run dev:web",
  buildCommand: "bun run build:web",
  frontendDist: "dist",
  bindings: "src/mygo.ts",
  out: "build",
});
