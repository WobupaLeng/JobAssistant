# ARCH-003：前端原地重写为 Vue 3 + Vite + TypeScript

> 状态：`DESIGN_ACCEPTED`（2026-11-22 用户裁决）· 版本 2 · 2026-11-22
> 决策记录：[`ADR-003`](../adr/ADR-003-frontend-leptos-to-vue.md)
> 契约真源：[`frontend-vue/AGENTS.md`](../../frontend-vue/AGENTS.md)（17 条迁移契约 C1–C17）
> 开工前置条件：[`frontend-vue/HANDOFF.md`](../../frontend-vue/HANDOFF.md) 第 1 节

---

## 0. 一句话结论

**`frontend-vue/` 原地重写为 Vue 3 + Vite + TypeScript，`backend-tauri/`（含 `core/`）与 `scraper/` 一行不用改。** 48 条命令的契约、CSP 铁律、11 份 CSS 设计令牌、`data-testid` 锚点全部原样保留。

真正的成本不在「换框架」，而在**把约 11,500 行 Leptos 里非 UI 的那部分重新实现一遍**——其中约 2,400 行与渲染无关的纯逻辑（格式化、筛选、草稿 diff、日志合并）必须逐一对齐，否则会出现「界面一样但行为不一样」的静默回归。

**目录策略已定：原地重写，不新建 `frontend-vue/` 并存。** 代价是回滚只能靠 git，因此开工前必须先落 Leptos 收尾提交。

---

## 1. 现状盘点（全部为实测数字）

### 1.1 代码规模

| 层 | 文件数 | 行数 | 说明 |
|---|---|---|---|
| `frontend-vue/src/**/*.rs` | 61 | 11,551 | 含 `#[test]` 96 个 |
| 其中 `api/` | 2 | 971 | `mod.rs` 380 行（39 手写 + 8 宏展开 = 47 个函数）、`types.rs` 591 行（37 struct + 1 enum） |
| 其中 `features/` | 39 | 8,369 | 最大 `features/scrape/filter.rs` 1,445 行 |
| 其中 `lib_ui/` | 10 | 约 1,000 | 纯函数层 |
| 其中 `components/` | 2 | 550 | `shell.rs` 287 + `widgets.rs` 263 |
| 其中 `pages/` | 5 | 634 | `dashboard.rs` 323 最大 |
| `frontend-vue/styles/*.css` | 11 | 3,197 | 纯 CSS，**零框架依赖，可 1:1 搬走** |

### 1.2 契约面（保持不变）

- **48 条 Tauri 命令**：`backend-tauri/src/lib.rs` 的 `generate_handler!` 注册项（46 条来自 `commands_*.rs` + 2 条 `lib.rs` 内联的 `get_scoring_spec` / `app_info`）。
- **前端已封装 46 / 48**：缺 `passes_threshold`（`commands_db.rs:410`）与 `update_score`（`commands_db.rs:424`）。Vue 版可以补齐，也可以保持现状——**补齐是加分项，不是迁移门槛**。
- **调用约定**：`window.__TAURI_INTERNALS__.invoke(cmd, args)`。
- **两种载荷形状**（最容易在迁移中出错，见第 5 节）：

  1. **包一层 `args`**：`invoke("list_jobs", { args: {...} })`；
  2. **平铺驼峰**：`invoke("get_job", { jobId })` —— Tauri 按**参数名**找键。

  例外：`score_job` 直接发整个 `ScoreArgs`；`auto_greet_next` 包 `args` 而 `auto_greet_pause` 平铺。**照抄，不要统一化。**

- **`null` 必须变 `undefined`**：命令层的 `Option<T>` 字段**没有** `#[serde(default)]`，显式 `null` 会被判成「缺 key」而报 `invalid args 'args' for command 'list_jobs': command list_jobs missing required key args`。现有 Rust 封装逐字段「有值才放键」来规避，**Vue 版必须复刻这个语义**。

### 1.3 运行时行为（迁移时最容易漏掉的部分）

| 行为 | 现有实现位置 | 迁移风险 |
|---|---|---|
| 页签计数 5 秒轮询，只刷数字不刷列表 | `pages/dashboard.rs:135` `TAB_POLL_MS = 5000` | 用 `setInterval` 忘了在 `onUnmounted` 清，切页后后台空转 |
| 错误条 10 秒自动消失，新错误重置计时 | `components/shell.rs` `ERROR_AUTO_DISMISS_MS` | 同左 |
| 切页副作用：只有非概览页触发 `page_tick` 重拉 | `lib_ui/nav.rs:26-28` + `pages/mod.rs` | **概览页刻意不接**，接错了会丢滚动位置与勾选 |
| 敏感库浮层与岗位详情抽屉互斥 | `features/sensitive/panel.rs:8-10` + 页面层持有 `open` | 两个浮层同时开会叠遮罩 |
| 主题切换写 DOM `data-theme` | `lib_ui/theme.rs:64-67`、`shell.rs` | 需要在首帧就定，避免闪 |
| **WebView 挂起时 `setTimeout` 不触发** | `features/auto/timer.rs:1-8`、`logs/panel.rs:36` | 依赖定时的逻辑不能写成「一定会按时执行」 |

### 1.4 现有 CSS 与设计令牌

11 份样式表，加载顺序即依赖顺序：

```
tokens → layout → controls → jobs → drawer → motion → scrape → pages → sensitive → auto → filter
```

**全部可原样搬迁**。仅两处需要处理：

1. `tokens.css` 里 `--refract-edge`（深 14px / 浅 5px）与 `--refract-turbulence`（0.008 / 0.006）已标「尚未实现」，引用数为 0；Vue 版接第三层时再消费。
2. CSS 里**不允许**出现 `//!` 或 `///`（根 `AGENTS.md` §2.3 红线），搬迁时不得把 Rust 注释带进去。

---

## 2. 目标架构

### 2.1 物理目录（原地重写 `frontend-vue/`）

```
frontend-vue/
  index.html                     Vite 入口（<div id="app">）
  package.json                   vue 3 / vite / typescript / vitest；不放 Tauri 运行时依赖
  vite.config.ts                 ESM only，defineConfig 包一层；客户端变量只用 import.meta.env 的 VITE_ 前缀
  tsconfig.json                  严格模式
  vitest.config.ts
  src/
    main.ts                      createApp + 首帧主题
    App.vue                      外壳插槽装配（不 import 任何页面）
    api/                         唯一允许触碰 @tauri-apps/api 的层
      invoke.ts                  两个调用器（包 args / 平铺）+ prune()
      types.ts                   DTO，无运行时代码
      commands/                  按后端模块分文件：db / scrape / chat / config / auto / sensitive / logs / greet
    lib_ui/                      无 DOM 的纯函数（可单测）
    components/                  AppShell + widgets（Card / Stat / Chip / StatusDot / SkeletonRows / Banner / ErrorBar / Field / Collapsible / EmptyState）
    features/                    auto / chat / config / jobs / logs / score / scrape / sensitive
    pages/                       DashboardPage / ChatPage / ConfigPage / LogsPage
  styles/                        11 份 CSS 直搬 + index.css 按序 @import
  .agents/skills/                15 个技能（见 SOURCES.md）
  AGENTS.md                      模块约定 + 17 条迁移契约
  HANDOFF.md                     现状 / 坑 / 下一步
```

### 2.2 分层与依赖方向

```
pages  →  features  →  components  →  lib_ui
                ↘        ↙
                   api（横切层，任意层可引用）

lib_ui 不得反向引用 api / pages / features / components
features 下不同领域不得互相 import
```

三条**可被机器检查**的规则（`tools/check_layers.py` 已删除，Vue 版用 ESLint 强制）：

| # | 规则 | 检查方式 |
|---|---|---|
| R1 | `components/**` 不得 import `features/**`、`pages/**` | ESLint `no-restricted-imports` |
| R2 | `lib_ui/**` 不得 import `api/**`、`features/**`、`pages/**`、`components/**` | 同上 |
| R3 | `invoke` 只许出现在 `src/api/**` | ESLint `no-restricted-imports`（禁 `@tauri-apps/api/core`） |

**Leptos 版有两处同层横向违例，Vue 版必须消除**：`features/jobs/cards.rs:17` → `features/scrape/options::platform_name`；`features/scrape/panel.rs:23` → `features/auto::AutoBar`。公共部分下沉到 `lib_ui/` 或 `components/`。

### 2.3 与后端的关系（为什么「后端保持不变」成立）

| 契约 | 真源 | Vue 版对端 | 改后端？ |
|---|---|---|---|
| Tauri 命令签名 | `backend-tauri/src/commands_*.rs` + `lib.rs` 的 `generate_handler!` | `frontend-vue/src/api/**` | **否** |
| sidecar 协议 | `core/sidecar/protocol.rs` | 不接触（前端看不见 sidecar） | **否** |
| 数据库结构 | `core/domain/schema.rs` | 不接触 | **否** |

**只需改 `backend-tauri/tauri.conf.json` 一处**：

```jsonc
"frontendDist": "../frontend-vue/dist",   // 原地重写，路径不变
"devUrl": "http://localhost:5173"        // 从 8080（trunk）改为 5173（Vite 默认）
```

`devUrl` 是**开发态**地址，不影响产物。`tauri.conf.json` 属于根 `AGENTS.md` §4 的**高冲突单点**，改动必须单写者。

**CSP 一字不改**：`script-src 'self' 'wasm-unsafe-eval'` 仍然有效——Vite 产出的外链 JS 天然满足 `'self'`，Tauri 会给它注入 sha256 或 nonce。`'wasm-unsafe-eval'` 在 Vue 版是死代码，**留到退役提交时一起删**，避免中途回滚白屏复发。

### 2.4 进程与渲染模型（不变）

```
Vue 3 (WebView2)  ──invoke()──►  backend-tauri/ 命令层  ──直接调用──►  core/ 领域
                                     │
                                     └──stdin/stdout JSON Lines──►  scraper/ (Python)
```

Vue 版仍然是**纯 CSR 单页**，不引入 SSR / Nuxt。原因：`tauri://localhost` 是静态文件服务，没有 Node 运行时；毛玻璃依赖首帧页面透明（`body { background: transparent }`），SSR 的内联样式与水合会破坏它；现有 CSP 是 `default-src 'self'` 严格模式，hydration script 需额外处理。

---

## 3. 迁移设计：逐模块映射

### 3.1 契约层

| 现有 | Vue 版 | 风险 |
|---|---|---|
| `api/types.rs`（37 struct + 1 enum，591 行） | `api/types.ts`（`interface` / `type`） | 中：`Option<T>` → `?:` 与 `\| null` 的选择要统一 |
| `api/mod.rs`（47 个函数覆盖 46 条命令） | `api/commands/*.ts` 8 个文件 + `invoke.ts` | **高**：包 `args` / 平铺必须逐条从 `commands_*.rs` 抄签名核对 |
| `invoke()` 错误归一 | 官方 `invoke` 抛 `string`，`catch (e)` 直接取 | 低 |

### 3.2 纯函数层（`lib_ui/` 直译，22 个单测一对一复刻）

| 现有 | 单测数 | 迁移方式 |
|---|---|---|
| `logfmt.rs` | 7 | 直译，`#` 注释格式解析 + 高亮切片 |
| `time.rs` | 6 | 直译，**保留 UTC+8 常量与「接收时间戳入参」约定**；`fmt_time` / `fmt_time_secs` 必须兼容秒与毫秒 |
| `text.rs` | 5 | 直译 |
| `session.rs` | 2 | 直译 |
| `clock.rs` | 1 | 直译 |
| `motion.rs` | 1 | 直译，`requestAnimationFrame` + 双帧；4 个类名常量 `drawer-in-play` / `row-in` / `list-fade` / `token-bump` |
| `job.rs`、`theme.rs`、`nav.rs` | 0 | 直译（薪资拆分、标签归一；`localStorage` 键名不变；`slug` / `entersSideEffect`） |

**这一层直译风险最低，优先做**——它是整次迁移的安全网。

### 3.3 组件层

`components/widgets.rs` 的 11 个组件直译为 `components/widgets/*.vue`。**`data-testid` 由调用方传入，组件本身不得写死**（现状如此，保持）。

`components/shell.rs` 拆成三件事：

```vue
<!-- App.vue -->
<AppShell :nav-extra="() => h(SensitiveEntry)">
  <RouterView />            <!-- 页面树 = 原 dispatch 插槽 -->
</AppShell>
```

- **外壳不得 import `features/`**（R1）。敏感库入口住在 `features/sensitive/`，走 `nav-extra` 插槽由 `App.vue` 注入。Leptos 版曾因 `shell.rs` 直接 import `SensitivePanel` 触发分层违例，后来改成 `SensitiveEntry` / `SensitiveOverlay` 两个组件 + 插槽——**Vue 版照此结构**。

### 3.4 页面与 feature

| 页面 | 对应 feature | 关键交互 |
|---|---|---|
| `DashboardPage.vue` | `jobs/`、`score/`、`scrape/`、`auto/`、`sensitive/` | 页签计数 5 秒轮询、筛选键变化重拉、详情抽屉与敏感库互斥 |
| `ChatPage.vue` | `chat/` | 会话切换、发送、简历、撤回 |
| `ConfigPage.vue` | `config/`（panel / draft / state） | **保存分两条命令**：门槛/上限走 `set_limit`，其余走 `set_app_config`（见 5.4） |
| `LogsPage.vue` | `logs/` | **日志游标是闭区间**，按 `(ts, level, message)` 多重集去重（见 5.5） |

### 3.5 最大单点：`features/scrape/filter.rs`（1,445 行）

必须先拆再译，否则会得到一个 1,400 行的 `.vue`：

| 拆出 | 内容 |
|---|---|
| `FilterBar.vue` | 关键词行、薪资区间、城市/经验/学历多选 |
| `SalaryEditor.vue` | 自定义薪资档位增删 |
| `ExpectationPanel.vue` | 求职期望（`scrape/expectations.rs`） |
| `useFilterState.ts` | 全部筛选逻辑与派生值（无 DOM，可单测） |
| `useFilterOptions.ts` | 选项枚举加载与归一（`scrape/options.rs`）+ `tid_safe()` |

> 这是本方案里**唯一建议「翻译时顺手重构」**的地方。其余模块一律 1:1 直译，不要借迁移之名重写。

---

## 4. 上线路径

### 阶段 0：前置与脚手架（0.5 天）

- **【阻塞】先落 Leptos 收尾提交**——原地重写会覆盖工作区里 50 项未提交改动，回滚只能靠 git。
- `npm create vite@latest -- --template vue-ts` 原地初始化，搬 11 份 CSS，`index.css` 按序 `@import`。
- `App.vue` 渲染空壳 + 首帧主题脚本。
- **验收**：`tauri.conf.json` 的 `frontendDist` 不改，`tauri build --no-bundle` 后产物放**工作区外**启动，能看到顶栏与空壳（不是白屏）。

### 阶段 1：契约 + 纯函数（2 天）

- `api/types.ts` + `api/commands/*.ts` + `invoke.ts`
- `lib_ui/` 9 个模块直译 + Vitest 覆盖现有 22 个用例
- **验收**：`vue-tsc --noEmit` 0 错；`vitest run` 全绿；ESLint 三条分层规则通过。

### 阶段 2：外壳 + 概览页（3 天）

- `AppShell.vue` + 11 个 widgets + `boot` 状态
- `DashboardPage.vue` + `jobs` / `score` / `sensitive`
- **验收**：在 CLI 产物上手工核对顶栏、概览卡片、抽屉、错误条。

### 阶段 3：其余三页（3 天）

- `ChatPage` / `ConfigPage` / `LogsPage`
- `filter.rs` 按 3.5 拆成 5 个文件
- **验收**：四页均可点；配置页能读能存（写数据库副本，不碰真实库）。

### 阶段 4：退役（0.5 天）

- 删 Leptos 残留（`Cargo.toml` / `src/**/*.rs` / `trunk` 配置）
- 删 CSP 里不再需要的 `'wasm-unsafe-eval'`
- 更新根 `AGENTS.md` §5 门禁命令、`frontend-vue/AGENTS.md`、`frontend-vue/HANDOFF.md`
- **验收**：全套门禁 A/B/C + 启动冒烟。

合计 **约 9 个工作日**（单人，不含并行）。

### 回滚

原地重写没有「改一行配置回滚」这条路。唯一安全网是阶段 0 那个 Leptos 收尾提交：`git checkout <该提交> -- frontend-vue/` 即可整体还原。

---

## 5. 迁移中的六个静默失效点（本节最重要）

> 这六条都是「编译通过、界面能点、但结果错了」的类型。Leptos 版是踩过坑之后写下的，Vue 版直译不仔细会**原样复发**。

### 5.1 载荷形状

`invoke("list_jobs", { args: {...} })` 与 `invoke("list_jobs", {...})` 在 TypeScript 里**都能通过类型检查**，但后者运行时可能报 `command list_jobs missing required key args`。**逐条命令必须从 `commands_*.rs` 的形参名反查**，不许凭记忆。

### 5.2 `undefined` vs `null`

现有 Rust 封装逐字段「有值才放键」。Vue 版用 `prune()` 统一处理，且**不许改成 `JSON.stringify` 后处理**（会丢 `undefined` 键，但会保留 `null`）。

### 5.3 类型契约的运行时校验

Rust 版靠 `serde` 在反序列化时校验字段。TS 类型**编译期检查、运行时不做任何事**——后端字段改名后，Vue 只会静默拿到 `undefined`。

**建议**：对**首屏三件事**（`AppInfo` / `ScoringSpec` / `LlmStatus`）加轻量运行时守卫（手写 `isAppInfo(v): v is AppInfo`），失败时写入 `boot.error` 走现有错误条。其余 34 个 struct 不加守卫，靠手工验收兜底。

### 5.4 配置写入分两条命令

`min_score` **不在**后端 `WRITABLE_KEYS` 里，塞进 `set_app_config` 会被**静默跳过**（表现为「保存成功但没生效」）。现有 `features/config/draft.rs` 用单测 `min_score_never_enters_the_patch_it_only_goes_to_set_limit` 兜住这条——**这个用例必须 1:1 搬到 Vitest**，它是这条规则唯一的自动化护栏。

另外两条约束也要保留：

- **只提交 diff**：后端对 `null` 的语义是「删除该键」，整份回传等于每次保存清空未设置项；
- **密钥草稿恒空串**，只回显 `api_key_hint`（尾 4 位掩码），拒绝提交 `****` 开头的值。

### 5.5 日志游标是闭区间

`LogRing::since` 是**闭区间**（`ts_ms >= since`），边界那一毫秒会重送一条。前端必须按 `(ts, level, message)` 多重集去重。Vue 版若改成开放区间或改去重键，会**永久丢失同毫秒的多条日志且不可恢复**。

### 5.6 概览页不接 `page_tick`

`Nav::Dashboard.enters_side_effect() == false`，且页面是否订阅路由变化必须与之严格一致（现有单测 `side_effect_pages_are_exactly_the_tick_pages` 守这条）。Vue 版的 `meta.sideEffect` 与「是否订阅」两处判据必须来自**同一个常量**。

---

## 6. 风险清单

| # | 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|---|
| R1 | 48 条命令的 `args` 形状抄错 | **高** | 运行时报错或静默丢参 | 逐条从 `commands_*.rs` 反查；为每条封装写载荷快照单测 |
| R2 | 96 个既有 `#[test]` 的断言在迁移中丢失 | 中 | 静默回归 | 按文件建对照表，一条一条打勾 |
| R3 | 概览页 `page_tick` 语义接错 | 中 | 切换回来丢勾选 / 不刷新 | 5.6 的单测 1:1 搬 |
| R4 | 日志闭区间改错 | 低 | 丢日志且不可恢复 | 5.5 的去重键保持不变 |
| R5 | 首屏白屏（CSP / 资源路径） | 中 | 完全不可用 | 阶段 0 就让 `tauri build` 内嵌产物并在**工作区外**启动验证——工作区内的 exe 一律跑不起来（`os error 5`） |
| R6 | 玻璃效果失效 | 中 | 视觉全毁 | CSS 直搬；`body { background: transparent }` 不得丢；首帧主题脚本要在 `main.ts` 之前 |
| R7 | 与后端未提交改动冲突 | 中 | 提交混杂 | 阶段 0 那个收尾提交就是为了消掉这个风险 |
| R8 | 两处同层横向违例被继承 | 中 | 分层退化成一张网 | 2.2 的 R1 规则用 ESLint 卡住 |

---

## 7. 尚未裁决的三项

前两项在迁移期间保持现状即可，不阻塞开工。

1. **`filter.rs` 拆不拆**：本方案建议拆（3.5）。若不拆，迁移后的 `.vue` 会是 1,400 行单文件。
2. **是否同时接 Liquid Glass 第二/第三层**：建议**先不接**，等 Vue 版四页验完再单独做。第二层的 `filter: url(#svg-filter)` 生效性仍需真机验证（规格原文从提交 `d39bae4` 的 `docs/ui/UI-SPEC-liquid-glass-l2-l3.md` 取回）。
3. **单元测试覆盖口径**：「现有 96 个断言一条不少」是最低要求，是否额外要求对新代码设覆盖率门槛？
