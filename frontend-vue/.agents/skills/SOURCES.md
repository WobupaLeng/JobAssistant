# 前端技能来源与版本记录

> 本目录 `frontend-vue/.agents/skills/` 下的技能**全部取自上游开源仓库**，不做自写。
> 升级方式：按本表的上游 URL 重新拉取同名目录覆盖；覆盖后在「本次导入」里追加一行记录。

## 上游仓库

| 字段 | 值 |
|---|---|
| 仓库 | [makshc2/vue-3-AI-skills](https://github.com/makshc2/vue-3-AI-skills) |
| 版本 | `release 3.1.0` |
| 提交 | `0d0aa7127adfe73a42399b2900aabf13a9d23bf8`（2026-11-03） |
| 许可 | MIT（`Copyright (c) 2026`）—— 随附全文见 `LICENSE` |

该仓库的 Vue 与 TypeScript 技能本身有两个上游源头，已在各 `SKILL.md` 的 `metadata.sources` 里标注：

- [vuejs-ai/skills](https://github.com/vuejs-ai/skills)（`vue-best-practices`、`vue-debug-guides`）
- [antfu/skills](https://github.com/antfu/skills)（由 `vuejs/docs` 生成的 vue skill）

## 已导入清单

| 目录 | 上游路径 | 文件数 | 用途 |
|---|---|---|---|
| `vue-core` | `skills/vue/vue-core` | 8 | `<script setup lang="ts">`、响应式系统、SFC 结构、组件数据流、组合式函数、性能 |
| `vue-composables` | `skills/vue/vue-composables` | 1 | 组合式函数的设计与抽取规范 |
| `vue-pinia` | `skills/vue/vue-pinia` | 2 | Pinia store 写法与状态管理 |
| `vue-router` | `skills/vue/vue-router` | 1 | 路由与 hash/history 模式 |
| `vue-testing` | `skills/vue/vue-testing` | 1 | 组件测试 |
| `vue-debug` | `skills/vue/vue-debug` | 140 | 故障索引：响应式 / computed / watch / 组件 / 模板 / 过渡 / TS 的逐条坑（**索引式渐进披露**，入口只列症状，按需打开单篇） |
| `vite` | `skills/vite` | 6 | `vite.config.ts`、插件 API、环境变量、构建与 SSR |
| `typescript-core` | `skills/typescript/typescript-core` | 4 | TS 基础、接口 vs 类型、类型收窄 |
| `typescript-vue` | `skills/typescript/typescript-vue` | 4 | Vue 里的 TS：`script setup` 类型、泛型组件、Pinia 与组合式函数类型 |
| `typescript-config` | `skills/typescript/typescript-config` | 4 | `tsconfig` 严格标志、模块解析、声明文件 |
| `typescript-types` | `skills/typescript/typescript-types` | 4 | 映射类型、条件类型、模板字面量类型、工具类型 |
| `typescript-debug` | `skills/typescript/typescript-debug` | 9 | `any` vs `unknown`、可赋值性、空值收窄等报错排查 |
| `typescript-testing` | `skills/typescript/typescript-testing` | 1 | TS 侧测试 |

框架无关、保留自原样（不来自上述仓库）：`apple-design`、`liquid-glass`。

## 刻意未导入

| 目录 | 原因 |
|---|---|
| `vue-axios` | 本项目不经 HTTP，全部走 Tauri `invoke`，`axios` 永不会出现 |
| `vueuse` | 未引入 VueUse 依赖；且其中「Tauri `isTauri()` 不是可靠的运行环境判据」一条值得注意，但我们目前没有 VueUse |
| `vue-architecture` | 目录结构、路由约定、表单校验等内容与本项目 `ARCH-003` 与 `AGENTS.md` 第 2.3 节冲突（本项目的分层与目录边界以项目宪法为准，不能用外部通用约定覆盖） |

## 已移除

| 目录 | 移除日期 | 原因 |
|---|---|---|
| `leptos` | 2026-11-21 | 前端由 Leptos 0.8 WASM 改为 Vue 3，见 `docs/adr/ADR-003-frontend-leptos-to-vue.md` |
| `leptos-component` | 2026-11-21 | 同上 |

## 本次导入

| 日期 | 动作 | 来源提交 |
|---|---|---|
| 2026-11-21 | 删除 `leptos`、`leptos-component`；导入 13 个 Vue / Vite / TypeScript 技能 | `0d0aa71` |
