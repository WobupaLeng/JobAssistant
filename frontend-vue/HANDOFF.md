# `frontend-vue/` 交接文档（HANDOFF）

> **本文件只写「现状 / 坑 / 下一步」**，不写历史记录与用例明细。
> 目录结构、分层约定、17 条迁移契约见 [`AGENTS.md`](AGENTS.md)，跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。

**最后更新**：2026-11-22

---

## 1. 现状

`frontend-vue/` 已进入 **Vue 3 + Vite + TypeScript 原地重写**：ARCH-003 的**阶段 0（脚手架）与阶段 1（契约层 + 纯函数）完成**——`src/api/**` 覆盖全部 48 条命令（`tests/commands-payload.test.ts` 守卫）、`src/lib_ui/**` 纯函数就位、`src/components/AppShell.vue` 外壳可用、`npm run typecheck` 0 错、`npm run build` 通过。

**Leptos 0.8 `.rs` 已部分退役**（用户裁决「先删除」）：`src/pages/*.rs` 五个页面文件、`lib.rs` 的 `mod pages`/`pages::Page` 引用、`features/scrape/filter.rs` 的旧城市 `<select>` 均已删除（git 历史可取回）；`src/pages/` 现在只有 `DashboardPage.vue`。**其余 `.rs`（api/components/features/lib_ui）仍是阶段 2/3 的行为对照**，阶段 4 整体退役。后端 `backend-tauri/` 不动。方案见 [`ARCH-003`](../docs/architecture/ARCH-003-vue-frontend-migration.md)。

**已接线**：概览页 `pages/DashboardPage.vue` 消费 `components/CityPicker.vue`（城市两级联动 + 搜索，373 城），筛选状态在 `features/scrape/filter.ts`（本轮只有 `city` 单字段）；`App.vue` 的 `dashboard` 槽渲染该页，其余三页仍是占位。**未接线**：其余筛选控件（薪资/经验/学历/规模/阶段/职位类型/行业/关键词）、`scrape_start`、本地草稿与防抖——阶段 3。

**测试现状**：`npx vitest run` = **130 passed / 0 failed**（8 文件）。秒/毫秒判据已修为 `1e11`（见 §2.3），原 4 条红全部转绿。

---

## 2. 坑（踩过的，别再踩）

### 2.1 构建与工具链

| 坑 | 怎么踩的 | 正确做法 |
|---|---|---|
| `trunk build` 报 `cargo artifacts not found for target crate` | 设了 `CARGO_TARGET_DIR` 指向工作区外 | trunk 只在项目内找产物，且**不认** `--target-dir`。跑 trunk 前先 `Remove-Item Env:\CARGO_TARGET_DIR` |
| trunk 报 artifacts not found | `frontend-vue/Cargo.toml` 缺 `[[bin]]` | trunk 要的是 **bin** 的 wasm，`[[bin]] path = "src/main.rs"` 不可删 |
| Rust WASM 单测「全过」但其实是静态计数 | `#[test]` 数 ≠ 通过数 | 两者不许混引 |
| `dist/` 里 wasm 有 94.9 MB | 未优化的 debug 产物 | release 档配了 `opt-level="s"` + `lto`，正常 2–4 MB |
| `vitest` 报 `Failed to parse source ... Install @vitejs/plugin-vue` | `vitest.config.ts` 没挂 `@vitejs/plugin-vue`；`vite build` 走的是 `vite.config.ts`（已挂），**构建成功掩盖了测试配置的缺口** | 两份配置各管一摊，改依赖时**两个都要对**。`vitest.config.ts` 已补 `plugins: [vue()]` |
| 组件用例报 `document is not defined` | 全局 `environment: 'node'` | 需要 DOM 的用例在**文件首行**写 `// @vitest-environment jsdom`，不要切全局 |
| 焦点断言恒为 `<body>` | `mount()` 没 `attachTo`，元素不在文档里 | `mount(C, { attachTo: document.body })`，并在断言前 `await new Promise(r => setTimeout(r, 0))` 让 `nextTick` 里的 `focus()` 落地 |
| 测「关闭后清空」时 `Unable to get [data-testid=...]` | 面板 `v-if` 已卸载，DOM 不存在 | 断言重开后的值，不要查已卸载的元素 |
| `npm run build` 打印 `✓ built` 但样式大面积丢失 | `styles/pages.css` 的块注释里用反引号包了 `*/`，注释提前闭合，**其后四份样式表整体失效**，只报一条 `[css-syntax-error] Unexpected "\`"` | 改完 CSS **去产物里数选择器**：`(Select-String dist/assets/*.css -Pattern '\.xxx').Count`，不要只看构建成功 |
| `npm install` 报 `EBUSY ... node_modules/esbuild.exe` 或 SSL 失败 | 残留 `node` 进程持有 exe；直连 npmjs 被代理拦 | 先 `Remove-Item node_modules -Recurse -Force`，再 `npm install --registry https://registry.npmmirror.com` |

### 2.3 契约与文案

- **驼峰参数写错名字不会有任何提示**：serde 对未知字段静默不报错。
- **载荷形状两类且有例外**（`score_job` 整包；`auto_greet_next` 包 `args` 而 `auto_greet_pause` 平铺）——**照抄，不要「统一化」**。
- **`min_score` 只能走 `set_limit`**：塞进 `set_app_config` 会被静默跳过。
- **配置保存只提交 diff**：后端 `null` 语义是「删除该键」，整份回传等于每次保存清空未设置项。
- **进度文案是测试契约**：五段标签与 `·` 分隔逐字照抄，改字就断锚点。
- **`index.html` 的 11 份 CSS 顺序**：乱序会导致层级覆盖错误（`pages` 必须早于 `filter`）。
- **CSS 里写 `//!` 或 `///` 会让整份样式表被静默丢弃**，表现为「变量生效、组件样式全无」。Leptos 版当前 0 违规。
- **时间戳秒/毫秒判据是「`|ts| < 1e11`（10 位及以下）就当秒」**：`lib_ui/time.ts` 与 Leptos 的 `time.rs::to_ms` 同一判据（C14），**改一处必须同改另一处**。取 1e11 后秒覆盖到公元 5138 年、毫秒从 **1973-03-03** 起，`time.time()` 与 `Date.now()` 两个数据源都在各自区间内。**旧判据 1e12 有真缺陷**（2001-09-10 之前的毫秒戳被误乘 1000，例如 2000-02-29 显示成 32129 年），已随本轮修正，`tests/time.test.ts` 全绿。唯一判不了的是 1973 年以前的毫秒值（`|ts| < 1e11` 时与秒不可区分，信息已丢失），不要用「猜日期」的办法补救。

### 2.4 布局与视觉

- **页面背景不透明 = 毛玻璃失效**：`tokens.css` 的 `body` 有 `background: transparent`，不要覆盖它。
- **首帧闪白**：靠 `index.html` 的 3 条内联 `<style>` 压住，不要挪到外部 CSS 文件。

---

## 3. 下一步

### 3.1 开工顺序（阻塞依赖）

1. **【阻塞】落 Leptos 收尾提交**。前提是根 `AGENTS.md` 槽位 A 门禁通过（`cargo check --target wasm32-unknown-unknown`），见根 5.1。
2. **取回 Liquid Glass 规格**：`git show d39bae4:docs/ui/UI-SPEC-liquid-glass-l2-l3.md`（`docs/ui/` 已在工作区删除，目录仍在但为空）。
3. **脚手架**：`npm create vite@latest -- --template vue-ts` 在 `frontend-vue/` 原地初始化，搬 11 份 CSS，`index.css` 按 C4 顺序 `@import`。
4. **契约层**：`api/types.ts` + `api/invocations.ts`（48 条命令，当前 Leptos 版只封装了 46 条，缺 `passes_threshold` / `update_score`）。
5. **外壳 + 概览页** → **其余三页** → **退役 Leptos 文件**。

### 3.2 需要拍板的开放项

| # | 问题 | 影响的文件 |
|---|---|---|
| 1 | **`filter.rs` 1,445 行怎么拆** | 建议 `features/scrape/components/*.vue` + `composables/useScrapeFilter.ts` |
| 2 | **是否同时接 Liquid Glass L2/L3** | 影响 `styles/` 与 WebGL 依赖（Vue 侧需装 canvas 库或手写） |
| 3 | **是否设覆盖率门槛** | 基线是 96 个纯函数断言，不是 40 |
| 4 | **`set_job_flag` / `list_logs` 这类多分支封装是否拆成独立函数** | 建议拆，但命令名字面量不能变 |
| 5 | **`job-row-*` / `score-*` 隐藏表是否保留** | 唯一理由的 `gui_e2e.py` 已不存在。若改用 `job-card-{id}` / `job-score-{id}` 可删 |

### 3.3 可独立开工的小项（与迁移无关）

1. **补最后一个 testid**：`features/config/form.rs:23` 的折叠标题按钮缺 `{test_id}-toggle` 派生。
2. **`index.html:20` 的过期注释**：写「其余三份消费它们」，实际是 11 份。
3. **`tests/fixtures/dashboard_contract.html` 有 `auto-bar`**，代码里不存在（`auto/panel.rs:124` 只有 `auto-toggle`，父节点是 `.scrape-bar`）。
4. **`tests/test_gui_e2e.py:45` 必 `ImportError`**：按路径加载已删除的 `tools/gui_e2e.py`。要么删掉这个测试，要么重写。
5. **两处同层横向依赖**（见 `AGENTS.md` 2.1）：`features/jobs/cards.rs:17`、`features/scrape/panel.rs:23`。
6. **登出入口未接线**：`login_status` / `login_save` / `logout` 调用点为 0。事件链断了——`sidecar.py:263/272/297/303` emit `login.state` → `codec.rs:89` 解析 → **主进程未 emit 给 WebView**。这是后端 + 前端两边的活。
7. **动效未接线**：`transitionend` / `animationend` / `IntersectionObserver` 全 0 命中；`.job-card.is-leaving` 计划过但从未实现（CSS 与 Rust 都 0 命中）。

### 3.4 代码注释里的失效引用

以下注释引用了已删除的 `tools/check_layers.py`，读到顺手删：

`src/lib.rs:40`、`src/components/shell.rs:5`、`:70`、`:296`、`src/lib_ui/logfmt.rs:8`、`src/pages/dashboard.rs:3`、`src/features/auto/state.rs:82`、`src/features/jobs/list.rs:7`、`src/features/jobs/cards.rs:10`、`src/features/scrape/filter.rs:809`、`src/features/scrape/panel.rs:8`、`:77`。
