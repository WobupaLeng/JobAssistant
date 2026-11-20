# frontend/ · 模块约定（AI 每次动手前必读）

> 本文件只约束 `frontend/` 这一个 crate。全局工程宪法在仓库根 `AGENTS.md`，
> 设计规格在 `docs/specs/`。两者与本文件冲突时，**以根 `AGENTS.md` 为准**。

## 1. 这是什么

`aja-frontend` 是 AI 求职助手的**界面层**，编译目标 `wasm32-unknown-unknown`，
由 trunk 打包成 `index.html` + `*.wasm`，再由 `app/src-tauri` 的 `tauri build` 内嵌进 exe。

**为什么是 WASM 而不是纯 Rust host 侧界面**（2026-11-21 用户决策链）：
用户要求界面用 Rust 写，同时要保留毛玻璃——毛玻璃由 `window-vibrancy` 作用于
WebView 窗口的合成层。纯 eframe/egui 需要接管主线程消息循环，与 Tauri 冲突。
WASM + WebView 是唯一能同时满足两者的组合。

## 2. 技术栈

| 层 | 选型 | 说明 |
|---|---|---|
| 框架 | Leptos 0.8（`csr` feature） | 细粒度响应式，无 VDOM |
| 语言 | Rust edition 2021 | |
| 构建 | trunk | `cargo build` 不产出可用的 `index.html` |
| 跨进程 | `window.__TAURI_INTERNALS__.invoke` | **不能** `use tauri::`（那是 host 侧） |
| 设计语言 | Liquid Glass | 见第 4 节 |

## 3. 目录分层（单向依赖，禁止反向）

```
src/lib.rs            入口：panic hook + mount_to_body(App)。不含业务
src/app/              应用级状态：bootstrap（appInfo/spec/llm）+ 主题
src/pages/            页面组装。只编排 feature，不写业务规则
src/features/<域>/    业务组件与 hook（jobs / scrape / score / chat / config / logs）
src/components/       设计系统：Card / Stat / Chip / StatusDot / SkeletonRows …
src/lib_ui/           纯函数与纯状态：nav / job / text / time / session / theme
src/api/              Tauri 命令封装 + 契约类型（types.rs 是唯一真源）
styles/               CSS：tokens + 分域样式
```

**依赖方向铁律**：`pages → features → components → lib_ui`；
`api` 只能被 `features` / `pages` / `app` 引用，**不得**反向引用它们。
`lib_ui` 不得引用任何其它层。

## 4. Liquid Glass 设计语言（三层分工）

用户 2026-11-21 选定**方案 C**：

1. **亚克力层** —— `window-vibrancy` 由 Rust 侧施加，负责「透出桌面」。
   **前端不参与**，也看不到它（系统合成层，`readPixels` 读不到）。
2. **CSS 层** —— `styles/tokens.css`。`backdrop-filter` 模糊 + 渐变高光边框 +
   内外阴影 + 材质散射色。这是玻璃感的 80%。
3. **WebGL 层** —— 只渲染**卡片之间**的真折射：抽屉盖列表、气泡盖背景。
   桌面本身折射不了（见第 1 条）。

**深浅双主题各自调优，不共用数值**：深色底上玻璃靠边缘发光立起来，浅色底上靠投影分层。
同一组 alpha 在两种底上观感完全不同，浅色下高光给多了会显脏。

**硬约束**：
- 整页背景必须**透明**，任何不透明底色都会把亚克力吃掉
- 前端**禁止硬编码**满分 100 / 门槛 70，一律由 `get_scoring_spec` 下发
- 所有可交互控件必须有 `data-testid`（`<页面>-<功能>-<控件类型>`）

## 5. 本模块装载的 skill

| skill | 来源 | 管什么 |
|---|---|---|
| `liquid-glass` | [jiuyang5354/skills](https://github.com/jiuyang5354/skills) | 物理折射实现（Snell 定律、squircle 位移、色散、镜面边缘光） |
| `leptos` | [dreamlab-ai/agentbox](https://github.com/dreamlab-ai/agentbox) | Leptos 0.8 实战手册：signals、view!、闭包陷阱、WASM 体积 |
| `leptos-component` | [adelabdelgawad/rust-fullstack-agents](https://github.com/adelabdelgawad/rust-fullstack-agents) | `#[component]` 写法、props 声明、RwSignal/Memo/Effect 选择 |
| `apple-design` | [jiuyang5354/skills](https://github.com/jiuyang5354/skills) | 苹果设计语言：版式、层级、动效曲线、材质 |

**关键事实（来自 `liquid-glass`，与本项目直接相关）**：
`backdrop-filter: url(#svg-filter)` 在 Safari 与 Firefox 上是坏的。
正确做法是「克隆背景进镜头 + 对克隆容器用普通 `filter: url()`」，
而不是对实时 DOM 用 `backdrop-filter`。

## 6. 本地验证（完工前必须真跑）

```powershell
# 编译检查（毫秒级，改一行就跑）
cargo check --target wasm32-unknown-unknown

# 单元测试（纯函数部分）
cargo test --target wasm32-unknown-unknown

# 打包预览
trunk serve --port 5184

# 内嵌进 exe（必须用 tauri build，cargo build 不内嵌前端 → 白屏）
cd ..\app; npx tauri build --no-bundle
```

**构建环境坑**：`cargo install trunk` 在本机编译 `h2` 时 rustc 崩溃
（`STATUS_STACK_BUFFER_OVERRUN`），必须用官方预编译包。
