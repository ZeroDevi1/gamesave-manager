import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    // `mygo dev` 加载 mygo.config.ts 中的 devUrl，端口必须固定
    port: 5173,
    strictPort: true,
    // 开发版应用与打包产物不属于前端源码
    watch: { ignored: ['**/.mygo/**', '**/build/**'] },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
