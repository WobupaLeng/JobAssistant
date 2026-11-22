# `frontend-vue/` 模块约定 · WebView 内的界面

> 本文件是本模块**目录结构与分层约定**的唯一真源。跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。
> **只写约束，不写进度**：现状、坑、下一步写 [`HANDOFF.md`](HANDOFF.md)。

**目标**：把 Leptos 0.8 WASM 版**原地重写**为 Vue 3 + Vite + TypeScript（`frontend-vue/` 就是目标目录，**不新建 `frontend-vue/` 并存**）。
方案 [`ARCH-003`](../docs/architecture/ARCH-003-vue-frontend-migration.md)，决策 [`ADR-003`](../docs/adr/ADR-003-frontend-leptos-to-vue.md)。第 4 节的 17 条契约是**必须逐条保住的遗产**，不因框架更换而作废。

---

## 1. 目录结构

```
frontend-vue/
  index.html               Vite 入口。11 份 CSS 的引入顺序是契约（见 C4）
  package.json             vue 3 / vite / typescript；不放任何 Tauri 运行时依赖
  vite.config.ts           ESM only，defineConfig 包一层；客户端变量用 import.meta.env 且只暴露 VITE_ 前缀
  tsconfig.json            严格模式
  src/
    main.ts                挂载点
    App.vue                页面分发插槽（**唯一** import 各页面的地方，C13）
    api/                   契约类型 + invoke 封装（唯一允许触碰 @tauri-apps/api 的层）
      types.ts             DTO，无运行时代码
      invoke.ts            invoke 封装（prune：删 undefined、留 null）
      commands/            按领域拆的命令封装，命令名逐字对抄后端
    lib_ui/                纯函数与 composables（无 DOM 依赖优先，可单测）
    components/            纯展示组件（21 个 .vue）：根级 AppShell / CityPicker / AppErrorBar / FilterPanel / JobCard / JobListView / JobDetailDrawer / LogCard / PlatformCard / ScrapeBar / SensitivePanel / TopbarTail；子目录 chat/（AutoApplyBar / ChatCompose / ChatDetach / ChatThread / ExpectRow / FriendList）、config/（ConfigSection）、logs/（DecisionRow / RunLine）
    features/              只留编排（5 文件）：auto/queue.ts · chat/greet-draft.ts · config/model.ts · logs/model.ts · scrape/filter.ts。视图模型/纯状态机已下沉 lib_ui/（见 §2.5 推论）；jobs/score/sensitive 无独立 feature 目录
    pages/                 dashboard / chat / config / logs
  styles/                  14 份 CSS（11 份契约序 + vue-dashboard/vue-chat/vue-pages 追加在末尾），顺序见 C4
  .agents/skills/          15 个技能，来源见 SOURCES.md
  AGENTS.md  HANDOFF.md
```

**没有 `src/app/`**，不要新建。

Leptos 版**已全部退役**（阶段 4 完成）：`src/` 下 `.rs` 计数 0；历史基线（61 个 `.rs` / 11,551 行）从 git 历史取回（提交 53d05cb 之前）。本文件第 4/5 节引用的 `.rs` 行号均为**历史证据坐标**，不再对应存活文件。

---

## 2. 分层约定

```
pages → features → components → lib_ui
```

1. **`api` 是横切层**：`features` / `components` / `pages` 都可直接引用。**这是既定事实，不是违例**。不要为了「纯净」把 `api` 下沉。
2. **`lib_ui` 不得反向引用** `api` / `pages` / `features` / `components`，只允许内部互引。
   **唯一例外：`import type` 引用 `api/types` 的 DTO 形状**。契约类型的单一真源在 `api/types.ts`，`lib_ui` 的纯函数需要这些形状做标注；复制一份 DTO 进 `lib_ui` 会造出双份真源（serde 静默失效的温床）。**运行时** import `api` 仍禁。
3. **禁止同层横向依赖**：`features` 下不同领域不得互相 import。Leptos 版有两处既有违例（`features/jobs/cards.rs:17` → `features/scrape/options::platform_name`；`features/scrape/panel.rs:23` → `features/auto::AutoBar`），Vue 版**必须消除**——公共部分下沉到 `lib_ui/` 或 `components/`。
4. **页面分发是插槽**：`AppShell` 里**不得 import 任何页面**；由 `App.vue` 作为 dispatch 插槽传入（或 `<router-view>`）。
5. **`components` 不得 import `features`**：展示组件只往 `lib_ui` 与 `api` 走。一个状态机若要被展示组件消费，它就该住 `lib_ui/`，哪怕业务上属于某个 feature——先例是 `lib_ui/city-picker.ts`（城市选择器的开合与键盘规则，消费方是 `components/CityPicker.vue`；放 `features/scrape/` 会造成向上引用）。**`pages` 可以 import `features`**——那是 §2 箭头 `pages → features` 的正向编排（先例：`DashboardPage` → `features/scrape/filter`、`ChatPage` → `features/chat/chat-model`、`ConfigPage` → `features/config/model`）；页面把 features 算好的视图模型传给纯展示组件，组件自己不碰 features。
   **推论（视图模型归属）**：标签常量、派生视图（CardView / 文案 / 禁用规则）、纯状态机只要被展示组件消费，就住 `lib_ui/`；`features/` 只留**编排**（invoke、localStorage 持久化、定时链、跨命令事务）。先例：`lib_ui/city-picker.ts`、`lib_ui/chat-model.ts`、`lib_ui/job-list-model.ts`。`features` 内部互引编排模块允许，但**跨领域**仍禁（§2.3）。

---

## 3. 红线

1. **不碰系统 API、不起子进程**，一切经 `invoke`。
2. **禁止硬编码满分与门槛**（100 / 70 之类），一律由 `get_scoring_spec` 下发。
3. **页面背景必须透明**，否则毛玻璃失效。
4. **可交互控件必须带 `data-testid`**，命名 `<页面/模块>-<功能>-<控件类型>`（`job-refresh`、`scrape-start-btn`）。动态 id 用 `tid_safe()` 把 `: / \` 与空格换成 `-`。
5. **CSS 文件不允许 `//!` 或 `///` 注释**。CSS 解析器会判定整份样式表非法并**静默丢弃**（曾导致「变量生效、组件样式全无」的误诊）。引入预处理器后需重新评估。
   **同族变体：注释正文里也不许出现 `` `*/` ``**——真正的坑不是 `//!` 本身，而是**块注释被提前闭合**。`styles/pages.css` 曾在注释里用反引号包住 `*/`，导致该行之后的四份样式表（`pages` / `sensitive` / `auto` / `filter`）全部失效，而 `vite build` 只报一条 `[css-syntax-error] Unexpected "\`"`、末尾照样打印 `✓ built`。改完 CSS 后**必须在产物里数一遍选择器**（`dist/assets/*.css`），不能只看构建成功。
6. **不引入第二套 UI 库**：玻璃与控件规范由 `styles/` 与 `liquid-glass` 技能确定。
7. **`src/` 里不放调试桩**：浏览器单独调试用的 `__TAURI_INTERNALS__` 桩不得进入正式路径。

---

## 4. 必须保住的契约（Leptos → Vue 逐条对照）

> 重写的验收清单。每条给了 Leptos 版证据位置，改不动就对照读。

**C1 只经 `invoke`，命令名逐字对抄**：调用点全是静态命令名，无动态拼接。命令名以 `lib.rs` 的 `generate_handler!` 为唯一真源（48 条）。

**C2 载荷形状分两类，且有例外，不得「统一化」**

- **包一层 `args`**：`list_jobs` / `scrape_start` / `set_limit` / `set_scrape_defaults` / `set_job_flag` 系 3 条。
- **平铺驼峰**：`get_job("jobId")` / `chat_messages("uid")` / `set_app_config("patch")` / `scrape_drain("limit")` / `list_logs("since")`。
- **例外**：`score_job` 直接发整个 `ScoreArgs`；`auto_greet_next` 包 `args` 而 `auto_greet_pause` 平铺——不一致，但**照抄不改**。

**C3 入参一律驼峰**：`jobId` / `jobIds` / `maxPages` 等。`features/scrape/filter.rs` 有 14 条 `BACKEND_KEYS` 蛇形→驼峰映射。serde 对未知字段**静默不报错**，写错名字不会有任何提示。

**C4 `index.html` 的 CSS 顺序**：14 条 = 11 条契约序 + 3 条 Vue 追加（`vue-dashboard → vue-chat → vue-pages`，只许追加在末尾），契约序精确为
`tokens → layout → controls → jobs → drawer → motion → scrape → pages → sensitive → auto → filter`。
另有 3 条内联 `<style>`：`html{background:transparent}`、`body{margin:0}`、`body:not(:has(#app > *)){overflow:hidden}`——**防首帧闪白**，不要挪进外部文件（挂载点是 `#app`，不是 Leptos 的 `#root`）。

**C5 整页背景透明**：`tokens.css:204` 的 `body` 必须 `background: transparent`，否则毛玻璃失效（全项目 13 处）。

**C6 CSS 禁 `//!` / `///`**：见红线 5。

**C7 深浅双主题，两套令牌数值不共用**：`[data-theme='light']` 选择器 10 处；两套令牌独立定义（`tokens.css:22` 起 `:root`、`:137` 起 light；`theme.rs:64-67` 是切换逻辑）。

**C8 控件全部带 `data-testid`**：Leptos 版 99 个控件（button 69 / input 25 / select 2 / textarea 3）中 98 个有，唯一缺口是 `features/config/form.rs:23` 配置分组折叠标题按钮（外层 `.config-card` 有 `test_id`，按钮没有 `{test_id}-toggle` 派生；对照 `widgets.rs:248` 的 `Collapsible`）——Vue 版应补上。
**`features/jobs/list.rs:272-289` 的隐藏 `<table class="sr-only" aria-hidden="true">` 必须原样保留**——唯一作用是为 `job-row-{id}` / `score-{id}` 保活锚点。

**C9 进度五段文案逐字照抄**

- 可见 Stat 标签：`已抓` / `入库` / `重复` / `低于门槛` / `规则否决`（`panel.rs:436-441`）；
- `sr-only` 摘要：`已抓 {} · 入库 {} · 重复 {} · 低于门槛 {} · 规则否决 {}`（`panel.rs:310-316`，渲于 `:453`）；
- 顶条：`{抓取中|空闲} · 已入库 {} 条 · 已抓 {} · 低于门槛 {} · 规则否决 {}`（`panel.rs:294-301`）；
- 入库 tag：`✔`（`panel.rs:77-78`）。

**C10 满分与门槛只来自 `get_scoring_spec`**：`lib.rs:33-36` 下发，`shell.rs:183-188` 消费；`job.rs:79` 的门槛是**参数**不是常量。

**C11 配置写入四条铁律**

- `min_score` 只走 `set_limit`，**永不进 `set_app_config` 的 patch**；
- **只提交 diff**（后端 `null` 语义是「删除该键」，整份回传等于每次保存清空未设置项）；
- 先 `set_app_config` 后 `set_limit`；`max_jd` 两边不一致即报错；
- 密钥只回显尾 4 位，掩码值不得回写。

证据：`features/config/state.rs:164-191`、`draft.rs:8-14`、`state.rs:193-199`。

**C12 筛选草稿双写**：`localStorage["aja.scrape_filter_draft"]`（`filter.rs:50`）**只作兜底**，`scrape_defaults` 优先（`filter.rs:970-989`）；400ms 防抖，**只发 diff**（`:56` / `:922-950`）。

**C13 页面分发是插槽**：见 §2 第 4 条。

**C14 时区与时间格式**：**UTC+8**、`MM-DD HH:mm`。`fmt_time` / `fmt_time_secs` 必须同时兼容**秒与毫秒**输入（`time.rs:15/32/52/64`）。

**C15 选项表是常量**：`features/scrape/options.rs` 的全部选项表（18 行业 / 6 档薪资 / 哨兵 `9999` / `MAX_PAGES` 3-1-20）照抄；`tid_safe()`（`:223`）。

**C16 定时器语义**：**WebView 挂起时 `setTimeout` 不触发**（`auto/timer.rs:1-8`、`logs/panel.rs:36`）。依赖定时的逻辑必须容忍这个前提，不要写成「一定会按时执行」。

**C17 动画类名常量**：`lib_ui/motion.rs` 的 4 个：`drawer-in-play` / `row-in` / `list-fade` / `token-bump`。⚠️ Leptos 版坑：`class_list().add(&Array)` 抛 `TypeError`，传数组要展开。

---

## 5. 契约面的现状（Leptos 历史基线，已退役；Vue 侧真源见 `src/api/` 与 `tests/commands-payload.test.ts`）

| 项 | 数值 | 说明 |
|---|---|---|
| 后端命令总数 | 48 | 以 `generate_handler!` 为准 |
| 前端已封装 | 46 / 48 | 缺 `passes_threshold`（`commands_db.rs:410`）与 `update_score`（`commands_db.rs:424`） |
| `api/mod.rs` | 39 手写 + 8 宏展开 = 47 函数 | 去重后覆盖 46 条（`list_jobs_default` 与 `list_jobs` 同打 `list_jobs`） |
| `types.rs` | 37 struct + 1 enum | `ScrapeUpdate` 是唯一 enum；`Job` 从 `lib_ui/job.rs` 再导出 |
| `data-testid` 字面总量 | 260 次 | 字面量 72 / `format!`·`concat!` 19 / 动态标识符 7 |

**`login_status` / `login_save` / `logout` 调用点 = 0**（仅 `api/mod.rs:348/:356/:361` 有定义）。事件链断了：`scraper/sidecar.py:263/272/297/303` emit `login.state` → `codec.rs:89` 解析 → **主进程未 emit 给 WebView**。登出入口未接线。

---

## 6. 测试约定

- **全部是纯函数 / 常量断言**：无 DOM 断言、无组件渲染测试；`pages` 名下的用例也只是数据断言（如 `pages/mod.rs:54` 校验 4 个 slug 不重名）。Leptos 版分布：`features` 56 / `lib_ui` 22 / `pages` 18，`api` 与 `components` 为 0。
- **`#[test]` 静态计数 ≠ 通过数**。Rust WASM 无 runner，`cargo test --target wasm32-unknown-unknown` 报 `os error 193`；要真跑用宿主目标 `cargo test --lib`。
- **Vue 版用 `vitest`，只给纯逻辑写单测**（状态机 / 计算 / 表单校验），不给纯展示组件写。
- **`tests/test_gui_e2e.py:45` 当前必 `ImportError`**：它按路径加载 `tools/gui_e2e.py`，而 `tools/` 已删除。

---

## 7. 模块内技能

`frontend-vue/.agents/skills/` 下 **15 个技能目录**，全部取自上游仓库，**不自写**。来源提交、导入清单、刻意未导入项与移除记录见 [`SOURCES.md`](.agents/skills/SOURCES.md)，`LICENSE` 随附同目录。

| 技能 | 用途 |
|---|---|
| `vue-core` | Vue 3.5+ 核心：Composition API、`<script setup lang="ts">`、单一真源 + computed 派生、**绝不直接解构 `reactive()`** |
| `vue-composables` | 组合式函数抽取与复用 |
| `vue-pinia` | 状态管理 |
| `vue-router` | 路由 |
| `vue-testing` | vitest 与组件测试 |
| `vue-debug` | **索引式**排障手册：入口只列症状，按需打开 `reference/*.md` 单篇，**不要通读** |
| `vite` | 构建配置；Vite ≤7 用 `build.rollupOptions`，Vite 8+ 用 `build.rolldownOptions` |
| `typescript-core` / `-vue` / `-config` / `-types` / `-debug` / `-testing` | TS 基础 / 单文件组件类型 / `tsconfig` 严格模式 / 类型设计 / 类型错误排障 / 测试类型 |
| `liquid-glass` | 玻璃三层分工与光学参数（框架无关） |
| `apple-design` | 动效与交互质感（框架无关） |

---

## 8. Liquid Glass 三层分工

| 层 | 手段 | 现状 |
|---|---|---|
| 第一层 CSS 玻璃 | `backdrop-filter: blur()` + 令牌 | ✅ 已完成（20 处 `backdrop-filter`，7 处 `-webkit-`） |
| 第二层 湍流折射 | SVG `feTurbulence` + `feDisplacementMap`，**克隆层 + 普通 `filter: url(#id)`** | 🔨 未做 |
| 第三层 卡片间真折射 | WebGL canvas 盖列表 / 气泡 | 🔨 未做 |

**核心断言**：`backdrop-filter: url(#svg)` 在 Safari / Firefox 上是坏的，Chromium 也会**静默忽略**其中的 `url()`。第二层**必须**把背景克隆进镜头容器，再对**克隆容器**用普通 `filter: url()`。第三层 canvas 必须 `pointer-events: none`。

**占位**：`tokens.css:51-52`（深）与 `:155-156`（浅）定义了 `--refract-edge`（14px / 5px）与 `--refract-turbulence`（0.008 / 0.006），**引用数为 0**；`feTurbulence` / `feDisplacementMap` / WebGL / canvas 的真实代码命中 **= 0**。
