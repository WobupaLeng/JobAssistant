import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vitest/config';

/**
 * Vitest 覆盖两层：
 *
 * 1. **纯逻辑**（`src/lib_ui/**`、`src/api/**` 的载荷形状、`features/**` 的状态机）——
 *    跑在 node 环境，不需要 DOM；
 * 2. **展示组件**（`src/components/**`）——只有个别用例需要，用**文件级**
 *    `// @vitest-environment jsdom` 单独切环境（见 `tests/city-picker.test.ts`），
 *    不做全局切换：jsdom 会让纯函数用例慢几倍，也没有必要。
 *
 * **必须挂 `@vitejs/plugin-vue`**：没有它，`.vue` 文件会被当成 JS 解析，
 * 报 `Failed to parse source for import analysis because the content contains
 * invalid JS syntax. Install @vitejs/plugin-vue`。`vite build` 走的是
 * `vite.config.ts`（已挂插件），vitest 读的是**本文件**，两份配置各管一摊。
 */
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    reporters: ['default'],
  },
});
