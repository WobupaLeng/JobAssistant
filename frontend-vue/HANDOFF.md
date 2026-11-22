# `frontend-vue/` 交接文档（HANDOFF）

> **本文件只写「现状 / 坑 / 下一步」**，不写历史记录与用例明细。
> 目录结构、分层约定、17 条迁移契约见 [`AGENTS.md`](AGENTS.md)，跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。

**最后更新**：2026-11-23

---

## 1. 现状

`frontend-vue/` 已进入 **Vue 3 + Vite + TypeScript 原地重写**：ARCH-003 **阶段 0–4 完成 + 后端功能接线对齐 + 登录接线**——`src/api/**` 覆盖全部 48 条命令（`tests/commands-payload.test.ts` 守卫）、四页全部接线（`App.vue` 分发：概览常挂 + 聊天/配置/日志浮层）、`npm run typecheck` 0 错、`npm run build` 通过、vitest 全绿。

**Leptos 0.8 `.rs` 已全部退役**（阶段 4 完成，用户裁决「先删除」）：`src/` 下 `.rs` 计数 **0**（pages 5 + 第一批 41 + 第二批 15，含 `lib.rs`/`main.rs`/两个 `mod.rs`；git 历史可取回，提交 53d05cb 之前）。注释溯源统一改写为「Leptos 版已退役，见 git 历史」。后端 `backend-tauri/` 不动。方案见 [`ARCH-003`](../docs/architecture/ARCH-003-vue-frontend-migration.md)。

**已接线**：概览页（CityPicker 373 城 + 全筛选控件 + C12 草稿 400ms 防抖 + 抓取三态 + 岗位网格/长按多选/批量 + 敏感库浮层 + 顶栏 token）、聊天页（Boss 列表 + 抽屉 v3 布局 + 自动投递三态队列 + 打招呼草稿）、配置页（三卡 + 掩码 + diff 保存）、日志页（决策/运行双视图 + seq 游标增量）。**分层整改已完成**：被展示组件消费的视图模型全部住 `lib_ui/`（chat-model / auto-model / job-list-model / longpress / scrape-panel-model / sensitive-model / scrape-filter-model），`features/` 只留编排（`features/scrape/filter.ts` 仅剩 loadDraft/saveDraft）；三条越层 grep 归零。

**M5 补遗完成**：`JobDetailDrawer.vue`（kv 十行/JD 四态/chips/单条 reject↔allow 同位单按钮/Esc 摘除）、`PlatformCard.vue`（三平台、boss 登录已接 `login_start`/`login_status`，智联/51job `is-off`）、`AppErrorBar.vue` 改 FIFO 队列（上限 5 + dropped 计数 + 逐条关闭 + aria-live）；视图模型 `lib_ui/{job-detail-model,platform-model,error-queue}.ts`。

**测试现状**：`npx vitest run` 全量全绿（34 文件 565 用例）。秒/毫秒判据已修为 `1e11`（见 §2.3）。

**几何契约补齐完成**（用户裁决「补齐实现，规格不动」）：原型 v7/v8 响应式几何已搬进 Vue——`tokens.css` 追加 `--ui-scale`（`lib_ui/canvas.ts` 的 `computeUiScale = clamp(1, innerWidth/1920, 1.5)`）+ 5 档 `--fs-{2xs,xs,sm,md,lg}`（11/12/13/15/20px × scale）；`lib_ui/use-canvas-scale.ts` 在 AppShell onMounted/resize（rAF 合并）按 `--ui-scale→--rail-w→--detach-w` 顺序写变量；**全部 174 处 font-size 声明用 var()，产物裸 px 字号归零**（vue 系 + 10 份契约 CSS 共享控件均收口）；三窗固定高乘 scale（config 640 / decisions 620 / sensitive 720），对话框宽 `clamp(600,50vw,80vw)` 不乘 scale，A54 job-greet 方块公式落地；`--detach-w` 单一来源（ChatPage 行内绑定已删，只留 syncCanvas + vue-chat.css 静态兜底）；敏感库居中窗规则从 vue-pages.css 覆写迁回 sensitive.css 本体。**残留非阻塞**：契约 CSS 160 处颜色字面量（vue 系已零，全量收口需另轮新增语义令牌）；非字号定值（.job-check/.score-box 等）原型本就是定值，未乘 scale。

**QA 修复轮完成**（qa-verify 判 QA_FAILED 后用户裁决「全部修」）：① 严重——搜索词与排除词彻底分离，`lib_ui/scrape-filter-model.ts` 新增 `searchKeywords()` 唯一出口，`DashboardPage.onStart` 只发 `keywords`（空则拦下不静默回落），推荐 chip/读期望进 keywords 不进 exclude；② `set_limit` 接线持久化上限/门槛（`onMaxJd`/`onMinScore`，失败走 AppErrorBar 带重试，门槛上限取 `get_scoring_spec` 不硬编码）；③ C9 入库 feed 补齐（`scrape-panel-model.ts` 的 `toFeedLine`/`FEED_ACCEPTED_TAG='✔'`/`FEED_SKIPPED_TAG='跳过'`，`DashboardPage.drainFeed` 调 `scrapeDrain(50)`，LogCard 每行 `log-row`）；④ testid 对齐冻结原型（job-tab/sort/batch-bar/空态四件、sensitive-open-btn、chat-detach-head、chat-rail 去重恰 1 处）；⑤ 51job 显示名对齐原型、注释溯源修正。新增 5 个回归文件 41 用例（qa-{keywords,limits,feed,testids,provenance}）。**低#1-3 后端 alias 已补**（提交 d3cd57d：`jobId`/`useLlm`/`logChanges`）。**已知例外**：`lib_ui/theme.ts` 仍直接触碰 `localStorage`/`matchMedia`/`document`（首帧主题入口，Leptos `lib_ui/theme.rs` 同样如此，C7 遗产）；拆纯函数+副作用会改启动路径，留待后续裁决，不算 §2.5 违例（§2.2 禁的是反向 import，不是 DOM）。

**后端功能接线对齐 + 登录接线完成**（提交链 d3cd57d→0cc998d→b6bebde→603de63）：① 载荷形状按 tauri 2 参数规则修正——`score_job` 收 `ScoreArgs` struct 改 `invokeArgs`（`args` 包裹）、`passes_threshold` 标量门槛键 `min_score`→`minScore`；② `scrape-filter-model.ts` 新增 `BACKEND_READ_KEYS`（14 条驼峰→蛇形，与后端 `KEY_SCRAPE_*` 逐字对齐），`applyBackend` 改蛇形回读（9 个多词键重启后正确回显），写入方向仍驼峰；③ 登录接线——`src/api/events.ts` `listenLoginState` 经 `window.__TAURI_INTERNALS__`（`plugin:event|listen` + `transformCallback`，零 `@tauri-apps/*` 依赖，非 Tauri 降级 no-op）消费冻结事件 `login-state`（payload 蛇形 `{platform,state,logged_in}`）；`lib_ui/login-model.ts`（`mergeLoginState` 未知态不覆盖已知态 + `loginBadge` + 保存/退出按钮派生 + `scanFlagVisible`）；`PlatformCard.vue` 新增 `platform-login-save-btn`/`platform-logout-btn`/`resolve-scan-flag`（waiting 态二维码，`data:image/` 才画）；`DashboardPage.vue` onMounted 订阅/onBeforeUnmount 退订 + visibilitychange/5s tick `login_status` 兜底。新增 login-{events,model,page} 41 用例。**`resolve-scan-flag` 无真源**（冻结规格仅裸名），按「waiting 态平台卡内显示二维码」推断落地，lead 已追认（可逆）。

**最终集成产物冒烟通过**（release-smoke @0225c3c）：`tauri build --no-bundle` 独占 jt-release3 exit 0（4m45s），exe 7,241,216 B SHA256 `22F31652BA65795CB0F524FB0EB640FCFE6105F3B4C371F72730095E99103C46`；工作区外 jt-release-smoke3 启动无 os error 5、WebView2 renderer 拉起、CDP 取证仪表盘 28 岗位 + 分值 + 打招呼 + 筛选城市深圳 + 配置掩码 ****1883 不回写 + 敏感库 20；CSP 内嵌 `wasm-unsafe-eval`=0；`login-state` 经真实 `plugin:event|emit` 验证 listen→merge→渲染链路（save/logout 按钮 1ms 渲染），登录 DOM 门控以披露式合成态注入取证（prod `__TAURI_INTERNALS__.invoke` 冻结不可桩）。证据 `%TEMP%\jt-release-smoke3\cdp-{dashboard,filter-city,scan-flag,login-buttons}.png`。**真实扫码/抓取仍受根 §6.2 Chrome 阻塞**。

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
| 批量改源码后出现 `from '../../api/types.'`、`.idle. | .running.` 这类怪串 | PowerShell `.Replace()` 把替换串里的单引号吃掉（根 AGENTS §6 明禁） | 批量字面量替换一律 Python `str.replace` 或 `edit` 工具；改完全仓扫 `(?<![\.\w])\.\.(?![\./\w])` 复核 |

### 2.3 契约与文案

- **驼峰参数写错名字不会有任何提示**：serde 对未知字段静默不报错。
- **载荷形状按 tauri 2 参数规则**：struct 参数（`score_job`/`scrape_start`/`set_limit`/`rescore_jobs` 等）需 `args` 包裹（`invokeArgs`）；标量参数平铺、键名取参数名 lowerCamelCase（`passes_threshold` 门槛键 `minScore`、`auto_greet_pause` 平铺）；`ScoreArgs` 内层字段蛇形（`rename_all`）。**照抄后端真源签名，不要「统一化」**。
- **`min_score` 只能走 `set_limit`**：塞进 `set_app_config` 会被静默跳过。
- **配置保存只提交 diff**：后端 `null` 语义是「删除该键」，整份回传等于每次保存清空未设置项。
- **进度文案是测试契约**：五段标签与 `·` 分隔逐字照抄，改字就断锚点。
- **`styles/index.css` 的 14 份 CSS 顺序**：11 份契约序乱序会导致层级覆盖错误（`pages` 必须早于 `filter`）；3 份 Vue 追加（vue-dashboard/vue-chat/vue-pages）只许在末尾。
- **CSS 里写 `//!` 或 `///` 会让整份样式表被静默丢弃**，表现为「变量生效、组件样式全无」。Leptos 版当前 0 违规。
- **时间戳秒/毫秒判据是「`|ts| < 1e11`（10 位及以下）就当秒」**：`lib_ui/time.ts` 与 Leptos 的 `time.rs::to_ms` 同一判据（C14），**改一处必须同改另一处**。取 1e11 后秒覆盖到公元 5138 年、毫秒从 **1973-03-03** 起，`time.time()` 与 `Date.now()` 两个数据源都在各自区间内。**旧判据 1e12 有真缺陷**（2001-09-10 之前的毫秒戳被误乘 1000，例如 2000-02-29 显示成 32129 年），已随本轮修正，`tests/time.test.ts` 全绿。唯一判不了的是 1973 年以前的毫秒值（`|ts| < 1e11` 时与秒不可区分，信息已丢失），不要用「猜日期」的办法补救。

### 2.4 布局与视觉

- **页面背景不透明 = 毛玻璃失效**：`tokens.css` 的 `body` 有 `background: transparent`，不要覆盖它。
- **首帧闪白**：靠 `index.html` 的 3 条内联 `<style>` 压住，不要挪到外部 CSS 文件。

---

## 3. 下一步

### 3.1 开工顺序（阶段 0–4 已完成，HEAD 5f7cddc；以下为收尾顺序）

1. **QA 只读验收**：✅ 已完成（qa-verify 判 QA_FAILED → 用户裁决全部修 → 修复轮落地，474 用例全绿，见 §1）。
2. **几何契约补齐**：✅ 已完成（用户裁决「补齐实现」，见 §1）——`--ui-scale` + 5 档 `--fs-*` 落地，174 处 font-size 全 var()、产物裸 px 归零，三窗 scale + 敏感库 720 居中窗 + A54，`--detach-w` 单一来源。残留：契约 CSS 160 处颜色字面量全量收口（非阻塞，另轮）。
3. **槽位 C 产物冒烟**（根 AGENTS §5.3 / ARCH-003 §6 R5，用户已授权 release lane）：先删 CSP `'wasm-unsafe-eval'`（ARCH-003:248 阶段 4 遗漏，WASM 已不存在），再 `cd backend-tauri && npx tauri build --no-bundle`（独占 `CARGO_TARGET_DIR=%TEMP%\jt-release`），产物拷到**工作区外**启动，确认 Vue 真渲染（CSP 白屏唯一实测点）。
4. **登出入口接线**（§3.3 #6，后端+前端两边）：主进程把 `login.state` emit 给 WebView；前端 PlatformCard 已接 `login_start`/`login_status`，缺 `login_save`/`logout` 与事件订阅。
5. **Liquid Glass L2/L3**（根 §7.2 #3，待用户裁决是否开工）：规格 `git show d39bae4:docs/ui/UI-SPEC-liquid-glass-l2-l3.md`。
6. 后端未决项（根 §7.2 #1/#4/#5）与 QA 低#1-3 alias 缺口，单独路由 legacy/backend lane。

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

原清单指向的 Leptos `.rs` 已全部退役（阶段 4），清单作废。Vue 侧悬空 `.rs` 溯源已在 M5 轮统一改写为「Leptos 版已退役，见 git 历史」；后端路径溯源保留带 `backend-tauri/` 前缀的全路径。
