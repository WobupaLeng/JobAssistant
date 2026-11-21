# ADR-003：前端由 Leptos 0.8 WASM 改为 Vue 3 + Vite + TypeScript

> 状态：**Accepted**（2026-11-22 用户裁决）
> 日期：2026-11-21 提案 / 2026-11-22 批准
> 决策者：用户
> 相关：[`ARCH-003`](ARCH-003-vue-frontend-migration.md)（实施方案）、[`frontend/AGENTS.md`](../../frontend/AGENTS.md)（迁移契约）、[`frontend/HANDOFF.md`](../../frontend/HANDOFF.md)（开工前置条件）

---

## 1. 背景

`frontend/` 于 2026-11-21 从 TS/React 改为 Leptos 0.8 CSR（WASM），目标是「界面用 Rust 写，同时保留毛玻璃」。该版本已达成部分目标：四页（概览 / 消息 / 配置 / 日志）全部正式实现、11 份 CSS 设计令牌与深浅双主题可用、启动白屏已修复。

但已实测出三类结构性成本：

1. **构建链长且脆**：`cargo check --target wasm32` → `trunk build` → `npx tauri build --no-bundle` 三段，各有环境坑（`CARGO_TARGET_DIR` 不能指向工作区外、`D:\` 建目录报 `os error 5`、共享 target 目录会重放陈旧二进制）。
2. **工具链反馈慢**：`trunk build` 单次 51 秒起步，`tauri build --no-bundle` 2 分 21 秒；没有热部署（`devUrl` 指向 trunk 的 8080，但 `beforeDevCommand` 为空，`tauri dev` 不会自动拉起前端）。
3. **生态成熟度差距**：Vue/Vite 的 DebugTools、HMR、组件库生态显著优于 Leptos 0.8（0.8 版本 API 仍在变动，如 `create_memo` 已标记 deprecated）。

## 2. 决策

**将 `frontend/` 原地重写为 Vue 3 + Vite + TypeScript；`app/`（含 `core/`）与 `scraper/` 完全不动。**

| 项 | 裁决 |
|---|---|
| 目录策略 | **原地重写 `frontend/`**，不新建 `frontend-vue/` 并存 |
| 后端改动 | 只需改 `app/src-tauri/tauri.conf.json` 的 `frontendDist` 与 `devUrl` 两处路径 |
| 命令签名 / sidecar 协议 / 数据库结构 | **均不改** |
| 11 份 CSS 与全部 `data-testid` | **原样保留**（见 `frontend/AGENTS.md` C4 / C8） |

**否决「并存」方案的理由**：并存期两套前端都要维护，`frontendDist` 需要来回切换，且无法保证 17 条迁移契约（`frontend/AGENTS.md` §4）在切换点上被逐条验证。原地重写的代价是回滚只能靠 git——因此**开工前必须先落一个 Leptos 收尾提交**（`frontend/HANDOFF.md` §1）。

## 3. 后果

### 3.1 正面

| 项 | 变化 |
|---|---|
| 构建 | 三段 → 两段：`vite build`（数秒）→ `tauri build` |
| 反馈速度 | 有标准 HMR（`vite dev` + `tauri dev` 同时跑，改 `.vue`/`.ts` 亚秒级热更新） |
| 类型检查 | `vue-tsc --noEmit`，比 `cargo check --target wasm32` 快一个量级 |
| 单测 | `vitest run`，毫秒级；不再需要「宿主目标跑 `cargo test --lib`」这种绕法 |
| 工具链 | 单一语言（TS）+ 标准前端工具链；`.vue` 单文件组件对设计稿对齐更直观 |
| 玻璃效果 | CSS 直搬，零改动 |

### 3.2 负面（必须明示的代价）

| 项 | 代价 |
|---|---|
| **约 11,500 行 Rust 重写** | 其中约 2,400 行是与渲染无关的纯逻辑（格式化、筛选、草稿 diff、日志合并），**必须逐一对齐**，否则出现「界面一样但行为不一样」的静默回归 |
| **96 个既有单测要重新实现** | Rust `#[test]` → Vitest，断言必须一条不少（分布见 `frontend/AGENTS.md` §6） |
| **运行时类型校验能力下降** | Rust 靠 `serde` 在反序列化时校验字段；TS 类型编译期即擦除。后端字段改名后前端只会静默拿到 `undefined`。缓解：首屏关键数据加运行时守卫 |
| **载荷形状两类且有例外** | `args` 包一层 / 平铺驼峰，编译期无法发现抄错（见 `frontend/AGENTS.md` C2） |
| **失去 WASM 的强类型跨端校验** | 前端不再是「同一个 Rust 生态」，契约同步退化为人工纪律 |
| **工具链回到 Node 依赖** | 需要 `npm install`，`node_modules` 须进 `.gitignore` |
| **`'wasm-unsafe-eval'` 变冗余** | CSP 里这条在 Vue 版是死代码，但**不能立刻删**——中途回滚到 Leptos 版会白屏复发。留到退役提交时一起删 |

### 3.3 对既有工程约束的影响

| `AGENTS.md` 条款 | 影响 |
|---|---|
| §0 状态看板 | 阶段名保持 `FRONTEND_REBUILD_IN_PROGRESS` 直到 Vue 版验收通过 |
| §2.3 前端目录与红线 | 已整节重写为 Vue 版，见 [`frontend/AGENTS.md`](../../frontend/AGENTS.md) |
| §3.4 CSP 铁律 | `script-src` 显式声明**仍然必须**；`'wasm-unsafe-eval'` 标注为「Leptos 遗留，暂不删」 |
| §5 槽位 A | `trunk build` → `vite build`；`cargo check --target wasm32` → `vue-tsc --noEmit` |
| §5 槽位 B | `cargo test --lib`（宿主目标）→ `vitest run` |
| §5 槽位 C | `tools/` 门禁脚本已随旧版实现删除，不再需要 |

## 4. 实施与回滚

- 实施路径见 [`ARCH-003`](ARCH-003-vue-frontend-migration.md) 第 4 节。
- **回滚**：原地重写没有「改一行配置回滚」这条路。唯一的安全网是开工前那个 Leptos 收尾提交——`git checkout <该提交> -- frontend/` 即可整体还原。
