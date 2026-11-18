import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Tauri 需要固定端口 + 不清屏，避免 dev 时端口漂移导致壳子连不上
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5183,
    strictPort: true,
    host: '127.0.0.1',
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'chrome110',
  },
  clearScreen: false,
})
