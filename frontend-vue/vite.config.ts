import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

/**
 * Vite 配置。
 *
 * `base: './'` 是硬要求：Tauri 打包后 `dist/` 通过 `tauri://localhost/` 这类
 * 自定义协议加载，绝对路径 `/assets/...` 会指到协议根而不是资源目录，表现是
 * 「白屏 + 控制台 404」。
 *
 * 端口固定 8080 且 `strictPort`：`backend-tauri/tauri.conf.json` 的 `devUrl`
 * 就是 `http://localhost:8080`，端口被占时应当直接失败而不是静默换一个端口，
 * 否则 Tauri 窗口会去连一个没人监听的地址。
 *
 * 插件只用 `@vitejs/plugin-vue`，**不引 @tauri-apps/**：前端与 Rust 同进程，
 * 唯一调用约定是 `window.__TAURI_INTERNALS__.invoke`（见 `src/api/invoke.ts`）。
 */
export default defineConfig({
  plugins: [vue()],
  base: './',
  clearScreen: false,
  server: {
    port: 8080,
    strictPort: true,
    // Tauri 的 WebView 从自定义协议访问 dev server，必须允许这类来源
    host: '127.0.0.1',
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // WebView2 是现代 Chromium，但保持与 Tauri 2 默认一致的保守目标
    target: 'chrome105',
    sourcemap: false,
  },
});
