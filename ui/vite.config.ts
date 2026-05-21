import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  // 防止 Vite 在开发模式下清理 Tauri 的屏幕输出
  clearScreen: false,
  server: {
    // Tauri 要求固定端口
    port: 5173,
    strictPort: true,
    // 允许 Tauri 的本地开发主机
    host: 'localhost',
    watch: {
      ignored: ['**/core/**'],
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
