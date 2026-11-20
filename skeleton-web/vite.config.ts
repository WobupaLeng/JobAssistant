import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// 骨架前端：只产出 dist 供 Tauri 内嵌。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5184, strictPort: true },
  build: {
    // 体积是 V1 硬指标，明确关闭 sourcemap 并用较小目标
    sourcemap: false,
    target: 'chrome105',
    minify: 'esbuild',
  },
});
