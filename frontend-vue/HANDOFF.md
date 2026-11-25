# `frontend-vue/` 交接文档（HANDOFF）

> **本文件只写「现状 / 坑 / 下一步」**，不写历史记录与用例明细。
> 目录结构、分层约定、17 条迁移契约见 [`AGENTS.md`](AGENTS.md)，跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。

**最后更新**：2026-11-25

---

## 1. 现状

`frontend-vue/` 已进入 **Vue 3 + Vite + TypeScript 原地重写**：ARCH-003 **阶段 0–4 完成 + 后端功能接线对齐 + 登录接线 + M1 同屏编排合并**——`src/api/**` 覆盖全部 48 条命令（`tests/commands-payload.test.ts` 守卫）、`npm run typecheck` 0 错、`npm run build` 通过、vitest 全绿。

**`App.vue` 分发的当前形态（SPEC-001 / ADR-004，取代此前「四页 + 浮层」的写法）**：概览**常挂**＋**同屏聊天层**（`aside.chat-rail` 与 `.chat-detach` 是 `.app-body` 的直接 flex 子项，点 Boss 就地展开、压缩主列）＋**只有 config / logs 两个浮层窗**。聊天**不是页**，二级导航（概览｜Boss 消息｜配置｜决策记录）已按用户裁决撤除，顶栏合并为单行。

**Leptos 0.8 `.rs` 已全部退役**（阶段 4 完成，用户裁决「先删除」）：`src/` 下 `.rs` 计数 **0**（pages 5 + 第一批 41 + 第二批 15，含 `lib.rs`/`main.rs`/两个 `mod.rs`；git 历史可取回，提交 53d05cb 之前）。注释溯源统一改写为「Leptos 版已退役，见 git 历史」。后端 `backend-tauri/` 不动。方案见 [`ARCH-003`](../docs/architecture/ARCH-003-vue-frontend-migration.md)。

**已接线**：概览页（CityPicker 373 城 + 全筛选控件 + C12 草稿 400ms 防抖 + 抓取三态 + 岗位网格/长按多选/批量 + 敏感库浮层 + 顶栏 token）、聊天页（Boss 列表 + 抽屉 v3 布局 + 自动投递三态队列 + 打招呼草稿）、配置页（三卡 + 掩码 + diff 保存）、日志页（决策/运行双视图 + seq 游标增量）。**分层整改已完成**：被展示组件消费的视图模型全部住 `lib_ui/`（chat-model / auto-model / job-list-model / longpress / scrape-panel-model / sensitive-model / scrape-filter-model），`features/` 只留编排（`features/scrape/filter.ts` 仅剩 loadDraft/saveDraft）；三条越层 grep 归零。

**M5 补遗完成**：`JobDetailDrawer.vue`（kv 十行/JD 四态/chips/单条 reject↔allow 同位单按钮/Esc 摘除）、`PlatformCard.vue`（三平台、boss 登录已接 `login_start`/`login_status`，智联/51job `is-off`）、`AppErrorBar.vue` 改 FIFO 队列（上限 5 + dropped 计数 + 逐条关闭 + aria-live）；视图模型 `lib_ui/{job-detail-model,platform-model,error-queue}.ts`。

**测试现状**：`npx vitest run` 全量全绿（数字以现场实跑为准，本文件不记用例数）。秒/毫秒判据已修为 `1e11`（见 §2.3）。

**窗口标题带（2026-11-25 用户裁决）**：只有窗口最上方 20px 能拖窗口、双击最大化/还原；`.topbar` 的静态拖动区已撤，几何判据落在 `lib_ui/drag-allow.ts`（见 §2.4）。

**后端抓取完成信号已通，前端零改动**（本轮后端 fix(sidecar)）：sidecar 新增 `scrape.finished` 事件，Rust 收到即清 `progress.running` 并写 `finished_reason`；Dashboard 每 tick 轮询 `scrape_progress`（`DashboardPage.vue` refreshProgress），下一 tick 顶条自动从「抓取中」转「空闲」，无需新事件接线、`api/types.ts` 既有 `running`/`finished_reason` 形状不变。真实 E2E CDP 取证通过（`%TEMP%\jt-running\t8\t8-05-ui.json`）。

**几何契约补齐完成**（用户裁决「补齐实现，规格不动」）：原型 v7/v8 响应式几何已搬进 Vue——`tokens.css` 追加 `--ui-scale`（`lib_ui/canvas.ts` 的 `computeUiScale = clamp(1, innerWidth/1920, 1.5)`）+ 5 档 `--fs-{2xs,xs,sm,md,lg}`（11/12/13/15/20px × scale）；`lib_ui/use-canvas-scale.ts` 在 AppShell onMounted/resize（rAF 合并）按 `--ui-scale→--rail-w→--detach-w` 顺序写变量；**全部 174 处 font-size 声明用 var()，产物裸 px 字号归零**（vue 系 + 10 份契约 CSS 共享控件均收口）；三窗固定高乘 scale（config 640 / decisions 620 / sensitive 720），对话框宽 `clamp(600,50vw,80vw)` 不乘 scale，A54 job-greet 方块公式落地；`--detach-w` 单一来源（ChatPage 行内绑定已删，只留 syncCanvas + vue-chat.css 静态兜底）；敏感库居中窗规则从 vue-pages.css 覆写迁回 sensitive.css 本体。**残留非阻塞**：契约 CSS 160 处颜色字面量（vue 系已零，全量收口需另轮新增语义令牌）；非字号定值（.job-check/.score-box 等）原型本就是定值，未乘 scale。

**QA 修复轮完成**（qa-verify 判 QA_FAILED 后用户裁决「全部修」）：① 严重——搜索词与排除词彻底分离，`lib_ui/scrape-filter-model.ts` 新增 `searchKeywords()` 唯一出口，`DashboardPage.onStart` 只发 `keywords`（空则拦下不静默回落），推荐 chip/读期望进 keywords 不进 exclude；② `set_limit` 接线持久化上限/门槛（`onMaxJd`/`onMinScore`，失败走 AppErrorBar 带重试，门槛上限取 `get_scoring_spec` 不硬编码）；③ C9 入库 feed 补齐（`scrape-panel-model.ts` 的 `toFeedLine`/`FEED_ACCEPTED_TAG='✔'`/`FEED_SKIPPED_TAG='跳过'`，`DashboardPage.drainFeed` 调 `scrapeDrain(50)`，LogCard 每行 `log-row`）；④ testid 对齐冻结原型（job-tab/sort/batch-bar/空态四件、sensitive-open-btn、chat-detach-head、chat-rail 去重恰 1 处）；⑤ 51job 显示名对齐原型、注释溯源修正。新增 5 个回归文件 41 用例（qa-{keywords,limits,feed,testids,provenance}）。**低#1-3 后端 alias 已补**（提交 d3cd57d：`jobId`/`useLlm`/`logChanges`）。**已知例外**：`lib_ui/theme.ts` 仍直接触碰 `localStorage`/`matchMedia`/`document`（首帧主题入口，Leptos `lib_ui/theme.rs` 同样如此，C7 遗产）；拆纯函数+副作用会改启动路径，留待后续裁决，不算 §2.5 违例（§2.2 禁的是反向 import，不是 DOM）。

**后端功能接线对齐 + 登录接线完成**（提交链 d3cd57d→0cc998d→b6bebde→603de63）：① 载荷形状按 tauri 2 参数规则修正——`score_job` 收 `ScoreArgs` struct 改 `invokeArgs`（`args` 包裹）、`passes_threshold` 标量门槛键 `min_score`→`minScore`；② `scrape-filter-model.ts` 新增 `BACKEND_READ_KEYS`（14 条驼峰→蛇形，与后端 `KEY_SCRAPE_*` 逐字对齐），`applyBackend` 改蛇形回读（9 个多词键重启后正确回显），写入方向仍驼峰；③ 登录接线——`src/api/events.ts` `listenLoginState` 经 `window.__TAURI_INTERNALS__`（`plugin:event|listen` + `transformCallback`，零 `@tauri-apps/*` 依赖，非 Tauri 降级 no-op）消费冻结事件 `login-state`（payload 蛇形 `{platform,state,logged_in}`）；`lib_ui/login-model.ts`（`mergeLoginState` 未知态不覆盖已知态 + `loginBadge` + 保存/退出按钮派生 + `scanFlagVisible`）；`PlatformCard.vue` 新增 `platform-login-save-btn`/`platform-logout-btn`/`resolve-scan-flag`（waiting 态二维码，`data:image/` 才画）；`DashboardPage.vue` onMounted 订阅/onBeforeUnmount 退订 + visibilitychange/5s tick `login_status` 兜底。新增 login-{events,model,page} 41 用例。**`resolve-scan-flag` 无真源**（冻结规格仅裸名），按「waiting 态平台卡内显示二维码」推断落地，lead 已追认（可逆）。

**最终集成产物冒烟通过**（release-smoke @0225c3c）：`tauri build --no-bundle` 独占 jt-release3 exit 0（4m45s），exe 7,241,216 B SHA256 `22F31652BA65795CB0F524FB0EB640FCFE6105F3B4C371F72730095E99103C46`；工作区外 jt-release-smoke3 启动无 os error 5、WebView2 renderer 拉起、CDP 取证仪表盘 28 岗位 + 分值 + 打招呼 + 筛选城市深圳 + 配置掩码 ****1883 不回写 + 敏感库 20；CSP 内嵌 `wasm-unsafe-eval`=0；`login-state` 经真实 `plugin:event|emit` 验证 listen→merge→渲染链路（save/logout 按钮 1ms 渲染），登录 DOM 门控以披露式合成态注入取证（prod `__TAURI_INTERNALS__.invoke` 冻结不可桩）。证据 `%TEMP%\jt-release-smoke3\cdp-{dashboard,filter-city,scan-flag,login-buttons}.png`。**真实扫码/抓取仍受根 §6.2 Chrome 阻塞**。

**M1 同屏编排合并完成**（[`ADR-004`](../docs/adr/ADR-004-legacy-chat-orchestration.md) / [`SPEC-001-legacy-chat-merge`](../docs/specs/SPEC-001-legacy-chat-merge.md) 用户裁决「方案 1」，零后端改动）：
① **编排提升**——`App.vue` 去整页分发，`ChatPage.vue` 改为壳级聊天层（多根 fragment，不再有 `<section class="page page-chat">`），`chatOpen` 上抛给唯一壳节点的 `.is-chat-open`；双层 `.app-shell`（`app-shell-slot` 外套 + `app-root` 内套）收敛为单层。
② **裁决 X 复活**——`vue-dashboard.css` 那组「展开态列数 = 收起 −1」规则此前是死代码（展开时概览已卸载），现在真正生效；展开规则仍写在 ≤1599 断点之后（A52 顺序敏感，`tests/geometry.test.ts` 守）。
③ **greet 就地链**——`DashboardPage.onGreet` 不再 `emit('navigate','chat')`，改 `emit('greet')` → `App.vue` 调聊天层的 `consumeGreetDraft()`：匹配好友 ⇒ 选中 ⇒ 预填 ⇒ `focus()`，不发送不换页（AC7）。
④ **顶栏单行 + 撤导航**——`AppShell` 删页签行与自带 `theme-toggle`，改提供一个 `display: contents` 的 `.tb-embed` 落位盒；平台卡与尾段仍由概览持有数据、经 `<Teleport :disabled="!tailInTopbar" defer>` 搬进那一行（独立 mount 时落位盒不存在 ⇒ 就地渲染，用例口径不变）。`.tb-tail { margin-left: auto }`（A33），配置按钮锚点改 `shell-config-btn`，满分 chip 出顶栏（`score-spec` 唯一落点回到日志卡头部）。
⑤ **Esc 分层**——新单源表 `lib_ui/overlay-stack.ts`（决策记录 → 配置 → 抽屉/敏感库 → 对话框），四个消费方共用；配置与决策记录补上了原先缺失的 Esc 关闭。**敏感库面板自己仍不回应 Esc**（只有 backdrop 与关闭按钮），留给下一轮。
⑥ **几何兜底换宿主**——`--rail-w` / `--detach-w` 的静态兜底最终落在 **`:root`**（与原型 L135/L1085 逐字同宿主）。M1 曾先挪到 `.app-body`，槽位 C 产物冒烟实测发现：自定义属性是继承的，更近祖先的声明会把 `syncCanvas()` 写在 `<html>` 行内的真实值整棵遮掉，展开态对话框只剩 1px 描边——修正提交 `c4913a2` 把兜底改回 `:root`，`tests/geometry.test.ts` 加守卫（兜底必须在 `:root`、`.app-body` 内不得声明 `--detach-w`）。运行期真实值仍只由 `canvas.ts:syncCanvas()` 写。

**M1 槽位 C 产物冒烟通过**（`c4913a2`，独占 `%TEMP%\jt-frontend-m1`）：`tauri build --no-bundle` 产物拷 `%TEMP%\jt-m1-smoke` 工作区外启动，`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` + Node 原生 WebSocket 连 CDP 取证。修 `:root` 兜底后实测：点岗位卡「打招呼」⇒ 壳 `is-chat-open`、右栏 `display:none`、对话框 **720px**（=0.5×1440）、主列 680、网格 2→1 列（裁决 X 生效）、`position:static`；Esc 分层「上层在场仍展开 / 撤上层后收起」均 true。证据 `%TEMP%\jt-m1-smoke\0{1,2,3}-*.png`。**披露**：本机会话列表恒 error 态（`chat_friends` ⇒ sidecar stdin 已关闭，根 §7.2 Chrome 阻塞），真实 `friend-<uid>` 点击路径取不了证，仅打招呼链走通；占位会话 `待打招呼 · 公司` 被 `isSendableFriend` 判不可发送 ⇒ compose 不渲染，按用户裁决①走「生成打招呼」面板、不预填不 focus（SPEC Rev5 AC7，代码零改动）。

**M1 锚点增删清单（QA 重锚用，SPEC-001 §QA 重锚清单的实测结果）**：
- 退役（全站命中 0，`tests/qa-testids.test.ts` 新增 describe 守）：`nav-dashboard` / `nav-chat` / `nav-config` / `nav-logs`、`page-dashboard` / `page-chat` / `page-config` / `page-logs`、`navTestId` / `pageTestId` 两个助手、`.nav-tabs` 页签 class 与 `controls.css` 的两条 `.nav*` 规则。
- 新增：`shell-dashboard`（概览容器，原型 L1418）、`shell-config-btn`（顶栏配置入口）、`topbar-tail-mount`（`.tb-embed` 落位盒）。
- 不变：四个废止 testid 仍 0；`chat-rail` 全站恰好一处且仍在 `src/pages/ChatPage.vue`（`qa-testids.test.ts:84` 的路径断言未改）；`theme-toggle` 全站恰好一处（现在只有 `TopbarTail` 挂）。

**M2 像素对齐完成**（SPEC-001 Rev5，按有稿设计逐条状态表落地；设计只出清单、编码落地，Chief 角色裁决）。含其交接的 A/C/G 行原子编辑 + 本 lane 续接的 B/D/E/F/H 与 ExpectRow 退役，零后端改动：
- **B 四态**：`JobListView` announce 逐字「列表状态 {mode} · 共 N 条 · 已选 N 条」（原型 L2193）；`ChatThread` 空态 glyph 改 40×40 圆盒 + 气泡 SVG（L2422，`.state-glyph.is-round`）；空态标题回归单一真源 `CHAT_EMPTY_TITLE='没有会话'`（L2342，`FriendList` 消费常量不再另写字面量）。
- **D 对话框**：`.chat-detach-chat .bubble` 80% 收口 + `.chat-feed` 12px14px + compose `.textarea` pre-wrap / `.btn` nowrap（L1113/1116/1117/1123，契约宿主 `pages.css` 不动，作用域覆盖）。
- **E 状态条**：`auto-bar` nowrap + flex0 0 auto、label fs-xs/560、running 文字 `--text-1`（状态只由点色表达）、paused 去光晕、acts/counts flex0 0 auto、计数 b 600 + margin-left2px（L521-546）；`.btn-pause` 改吃新令牌 `--btn-pause-*`（原型 L326-328 字面值，深浅两套，悬停改描边非提亮）。
- **F 顶栏**：llm seg title 逐字「大模型打分暂未开放，当前只支持规则打分」；token 读数两份 DOM（`token-total` 落 `<b>`、`token-total-chip` 折叠 chip，文案「累计 N token」，L1400-1406/A32），`.tb-token-chip` 死规则接上；**theme 锚点保持 `theme-toggle`**（SPEC-001 AC10 + 基线 L157 明文，原型 `shell-theme-toggle` 为异名，不改）。
- **H 日志卡/降级**：`log-card` 定高 220/min160 + `.log-card>.card-body` flex column 滚动、`log-rows` 去 180px 二次限高（L943/L1024）；删 `vue-dashboard` 两处 + `vue-chat` 一处 `@media(max-width:900px)` 降级（原型 L1339-1341 硬约束：最小宽 1200，不做落底/单列降级）；`LogCard` 标题「决策日志 / 运行日志」（基线 K9/文案表，斜杠两侧各一空格）。
- **ExpectRow 退役**（Rev3/Rev5）：删 `ExpectRow.vue` + `ChatPage` 引用/`loadExpectations`/`expectations` 状态 + `.expect-row` CSS + `chat-contract` SOURCES 行；`chat-expect-*` 锚点全站 0（`qa-testids` 新增断言守）。期望数据仍由概览筛选面板「读取期望职位」承载，`api/commands/chat.ts` 的 `list_expectations` 封装与契约面不动。
- **红线 8 令牌化**：原型字面色值一律新增语义令牌——`--spec-avatar-top`（头像高光 .5，比 `--spec-top` 亮一档）、`--btn-pause-border/-hover/-bg/-fg`（深浅两套）；`.chat-rail::before` 玻璃描边随族住 `layout.css`（`#000` 遮罩 alpha，vue 系禁字面量）。

**M2 锚点增删清单（QA 重锚用）**：
- 退役：`chat-expect-row` / `chat-expect-loading` / `chat-expect-error` / `chat-expect-empty` / `chat-expect-<code>`（ExpectRow 整组）、`.expect-row` class。
- 新增：`chat-rail-empty-title` / `chat-rail-empty-refresh-btn`（及 `chat-detach-rail-*` 对应派生，`chat-contract` REQUIRED 已收）、`token-total-chip`（折叠 chip）。
- 变更：`token-total` 从外层 span 移到读数 `<b>`（锚点名不变、落点变）；`job-empty-title`「暂无岗位」/`job-empty-sub`/`job-state-announce`/`FriendList` 空态「没有会话」/rail 错误 head「读取消息失败：<code>」/llm seg title/`LogCard` 标题 均逐字对齐原型。
- 不变：四个废止 testid 仍 0；`nav-*`/`page-*` 仍 0；`theme-toggle`/`chat-rail`/`auto-apply-bar` 各恰一处；`shell-config-btn`/`shell-dashboard`/`topbar-tail-mount` 沿用 M1。
- **已知偏差（报告项，不在 M2 修）**：`ChatThread` `.chat-head` 三按钮（生成打招呼/选简历/撤回）原型无＝功能 additions；顶栏「模型打分/打招呼」分项 stats 未接线——**更正（M2 后核查）**：config DTO 其实已下发 `tokens_greet`/`tokens_score`（`commands_config.rs` L58-60，`api/types.ts` L290-292 已有类型），但 Rust 核心**无累计写入路径**（三键在 `CONFIG_READ_ONLY_KEYS`、greet/score 逐次 `tokens` 只回传不持久化、jobs 表 `token_used` 列仅声明无写入），读数仅反映旧 Python 库遗留值；分项展示接线＝纯前端活，但是否派后端补累计由 Chief 定，前端不造数；`<1200` body overflow 裁剪 vs 原型横向滚动条（**M3 已修**，见下）。

**M3 视觉收尾完成**（SPEC-001 Rev6 路由①③ + §3.3 #7 动效候选；纯 CSS + 测试守卫，零逻辑改动、零锚点增删）：
- **<1200 横向滚动不裁剪**（Rev6 偏差③，基线 §2.3「最小宽度是契约」）：`vue-dashboard.css` 在 `.app-shell{min-width:1200px}` 旁新增 `body{overflow-x:auto;overflow-y:hidden}`——契约宿主 tokens.css 的 `overflow:hidden` 会把 1200 外壳裁掉，横向出口为 Vue 侧覆盖，纵向仍锁死（各列自己滚），防首帧闪白内联规则只匹配挂载前不受影响。实测（Edge headless + vite preview 8091）：1150×800 scrollWidth 1200 / clientWidth 1150，滚到右缘 `shell-config-btn` right=1058 可达；1200/1440 scrollWidth==clientWidth 无溢出；scrollHeight==clientHeight 无纵向滚动条。
- **入场动效补齐**（原型真值、Vue 此前缺失）：`.rail-error` 补 `animation: rail-in var(--dur) var(--ease-out)`（原型 L1179/L1187 逐字）、`.chat-detach-chat .bubble` 补 `bubble-in`（L1196/L1201 逐字），两组 keyframes 住 `vue-chat.css`；只动 opacity/transform（合成层，不触发 backdrop-filter 每帧重算）；新节点创建即播一次，keyed v-for 保住既有气泡不重播，零 JS 接线。实测真实 rail-error 节点 animationName=rail-in / 0.32s；reduced-motion 由 controls.css 全局封顶生效（0.01ms）。
- **chat-head 三按钮裁量＝零改动**（Rev6 偏差①）：已走标准 `btn btn-sm btn-ghost`（controls.css hover 抬 1px / press scale 0.985 / disabled 0.4 齐备），不新造样式。
- **不接线、待裁决**：C17 其余三类 `list-fade`/`row-in`/`token-bump` 常量保住（契约满足）但原型零命中，接线会发明超出冻结设计的动效，待设计/架构裁决；`.job-card.is-leaving` 同样无原型真值。`transitionend`/`animationend` 无消费方需要（CSS 入场动画节点创建即播，无需事件清理）。
- **门禁**：typecheck 0 错；vitest 34 文件 572 全绿（新增 2 守卫：chat-contract 入场动效 + geometry body 横向出口）；build 产物 76.04 kB，`{` 计数 738 = M2 731 + 新增 7 块，新规则逐条可达；槽位 A cargo 独占 `%TEMP%\jt-m3-polish` exit 0 + compileall 0。证据 `%TEMP%\jt-m3-polish\m3-{1150-right-edge,1440}.png`。
- **顺手发现（未动，仅报告）**：layout.css `@media(max-width:1100px)` 的 `.dashboard.is-fill-page` 降级块是死代码（Vue src 对 `.dashboard`/`is-fill-page` 零命中）；`@media(max-width:1180px)` 收紧内边距块在窄视口仍会触发（媒体查询看视口不看布局宽），是否属「<1200 不降级」口径待架构裁决。

**M3 后续批 · 分项 token 读数接线完成**（Chief 派单，DTO 现成、零新增 invoke、零锚点增删）：
- `TopbarTail.vue` props 增 `tokensGreet/tokensScore`（默认 0）；`.tb-tokens` 改 inline-flex gap 8（原型 L485-490 `.tb-stats` 语义），累计读数包成单项，新增分隔线 `.tb-sep`（1×14 hairline，原型 L487）与两枚 `.rail-stat` 分项「模型打分 / 打招呼」（数字 text-1/650，原型 L486）；1360–1599 隐藏分项与分隔线（原型 L1335-1337）；折叠 chip title 扩为全量三值「累计 N token · 模型打分 N · 打招呼 N」（原型 L1406 逐字格式）。分项 `<b>` 照原型无 testid。
- `DashboardPage.vue` 增两个纯派生 computed 读既有 config 字段（tokens_greet/tokens_score）；新规则住 `vue-dashboard.css`（全 var() 零字面量）；`qa-testids` 增 1 守卫。
- **数据真实性**：Rust 核心无累计写入路径（偏差②更正），读数为旧库遗留值（无 Tauri 预览下为 0），前端不造数；后端补累计排 M4 之后（Chief 顺序）。
- **门禁**：typecheck 0 错；vitest 34 文件 573 全绿；build 76.39 kB，`{` 计数 743 = 738 + 新增 5 块，分项规则与 1360–1599 媒体块可达；槽位 A cargo 独占 `%TEMP%\jt-m3b-polish` exit 0 + compileall 0。实测 1600 分项可见（text-1/650）、1440 分项与分隔线隐藏、1150 chip title 全量三值且横向出口仍生效。证据 `%TEMP%\jt-m3-polish\m3b-1600-stats.png`。

**Liquid Glass L2 完成**（SPEC-003 裁定 1–7，上游规格 `d39bae4` 冻结；根 §7.2 #3 第二层落地）：
- **L2-0 闸门 PASS**：真 WebView2（Chrome/157）探针实证 `filter:url()` 湍流像素差 >0、`feImage data:` 生效、webgl2 可用——最高风险项确证，无需 `data-l2` 降级。探针在 `%TEMP%\jt-glass-l23\`（红线 7 禁入 src）。
- **L2-1 基础**：`index.html` 加 svg defs（`lg-turb-dark` 0.008/3/scale10、`lg-turb-light` 0.006/2/scale5，seed 同 11）；`styles/vue-glass.css` 第 15 份追加 `index.css` @import 链末尾；`lib_ui/glass.ts` ThemeParams 与 `tokens.css` 折射令牌同源（vitest 断言）。
- **L2-2 抽屉/敏感库 P0**（裁定 6）：`GlassClone.vue` backdrop 模式 + `use-glass-clone.ts`——面板背后窗内内容非交互副本（剥全部 id/data-testid、剥既有克隆层防递归、排除叠层、aria-hidden/inert/pointer-events:none），`computeGlassOffset` 反平移 rect+scroll，按面板 rect 预裁剪 +20px 出血，内容变更节流 200ms 重克隆、滚动只更新反平移。
- **L2-3 卡片 P1**（裁定 7）：material 模式克隆卡自身玻璃 tint（card-local 不反平移，链 1 滤镜）+ `use-glass-budget.ts` IntersectionObserver 限 ≤12 可见卡施滤镜；`use-glass-support.ts` 运行时探针不支持则 `<html data-l2="off">` 整层降级。
- **映射/红线**：Leptos `glass.rs`→`glass.ts`、`shell.rs`→`AppShell.vue`（L3 `glass_canvas.rs`→`GlassCanvas.vue` 见下段）；`.card` 补 `overflow:hidden`（规格铁律 3，`layout.css` 只加这一条）；零新依赖（禁 three/pixi/wgpu/html2canvas），颜色全运行时读 token、TS/CSS 零字面量；jsdom 无 IO/RO 加能力守卫（真实 WebView2 照常）。
- **门禁**：typecheck 0；vitest 35 文件 589 全绿；build CSS 77.61 kB `{` 758（glass 规则全可达、`.card overflow:hidden` present）、JS 241.76 kB（L2 增量远低于 A9 40KB 预算）；C4 @import 15、vue 系 CSS 4 份零字面量守卫绿。**真窗口视觉验收（规格 5.2/5.3 A2–A8）与槽位 C 冒烟按 Chief 裁定搁置至统一验收**。

**Liquid Glass L3 完成**（SPEC-003 裁定 3/4/5，上游规格 §3；根 §7.2 #3 第三层落地，L2/L3 收口）：
- **GlassCanvas 抽屉透镜**：`components/GlassCanvas.vue`（canvas `data-testid="glass-l3-canvas"`，reduced-motion/reduced-transparency 时 `v-if` 根本不创建＝A8）+ `lib_ui/use-glass-l3.ts` 裸 WebGL2 渲染器（零新依赖）：每帧读 ≤64 张可见 `.job-card` 几何/底色（运行时 computed style）实例化画进 FBO，再画覆盖抽屉矩形的透镜，片元就地算 squircle SDF + Snell 法线偏移 UV 采样 FBO，混 `--glass-3` tint + 边缘高光；`glass.ts` 落 L3 光学纯函数（`snellDeflection`/`squircleSDF`/`bevelSlope`/`clampDpr` + `GLASS_L3` 常量，vitest 单测）。挂 `AppShell.vue` `.app-body` 之后（裁定 5：`position:fixed inset:0 z-index:40 pointer-events:none`，DOM 序盖过同层遮罩、低于抽屉面板 41）。
- **预算降级**（规格 §3.4）：抽屉关/被上层叠层（config/decisions）或敏感库盖住 → 停 rAF 清屏；静止 500ms 停 rAF 保留最终 draw；visibility hidden 停；dpr≤2；实例≤64；webgl2 创建/着色器编译失败 → `<html data-l3="off">`（第二层照常）。
- **映射偏差（记此，类裁定 7 适配非契约冲突）**：① Vue 透明底（C5）→ canvas 不画规格 §3.3 step3 的 body 不透明 quad（会挡亚克力），FBO/默认帧透明清屏、透镜只绘折射卡片其余全透明；② 进度＝抽屉真实 computed opacity（Vue 抽屉是 24px 微移 + 淡入，opacity 0→1 即动画真值），替代规格 §3.6 字面公式 `1-r.right/innerWidth`（预设整屏滑入对本抽屉恒≈0），原则照 §3.6 每帧读真实几何不猜动画。敏感库栈 50/51 在 canvas 上、与抽屉互斥 → 敏感库开时透镜 inactive（裁定 5 v1＝抽屉透镜）。
- **L3 施工探针 PASS**（真 WebView2 WebGL 2.0，复用冒烟产物 + CDP，不 build Tauri）：从 `use-glass-l3.ts` 逐字提取 4 段 shipped GLSL 编译/链接全过、FBO complete、透镜绘 50256/60000 像素、有折射(edge14) vs 无折射(edge0) 像素差 815/max31 >0 证 Snell UV 偏移生效、glError 0。探针 `%TEMP%\jt-glass-l23\probe-l3.mjs`（红线 7 禁入 src），冒烟实例用后已关。
- **门禁**：typecheck 0；vitest 35 文件 606 全绿；build CSS 77.99 kB `{` 766（L2 758+8，canvas 规则全可达）、JS 252.89 kB；**A9 实测**：pre-Glass 基线 237.61 kB（worktree@dcd5e66~1 独立 build）→ L2+L3 总增量 15.28 kB ≤ 40KB ✓；C4 @import 仍 15（无新样式表）、vue 系 CSS 4 份零字面量、`glass-l3-canvas` 全站恰 1 处守卫绿。**真窗口视觉验收（规格 5.2/5.3 A2–A8）与槽位 C 冒烟按 Chief 裁定搁置至统一验收**。

> **⚠️ 以上 L2/L3 段落已作废（2026-11-25，用户裁决整层移除，提交 `b5a9062`）。** 保留作历史：删掉的正是 L2 克隆层（`GlassClone.vue` / `use-glass-clone.ts` / `glass.ts` / `vue-glass.css`）与 L3 透镜（`GlassCanvas.vue` / `use-glass-l3.ts` / `glass.test.ts`），以及 `index.html` 里的 svg defs、`AppShell` 的挂载点。**只留第一层 `backdrop-filter` 玻璃。**
>
> 移除后的契约变化（已同步进 `AGENTS.md`）：C4 回到 **14 条**、红线 8 的 `VUE_CSS` 回到 **3 份**、前端测试文件数由 39 降为 38（少的是 `glass.test.ts`，**不是漏跑**）。§2.1「敏感库面板被压到视口外」那条坑与「已修」表里的 L3 自激循环、L2 克隆层节流，都因代码删除而**不再适用**——不是结论错了，是病灶没了。

**单休小批完成**（SPEC-004 §三，独立提交不混 Glass；后端三档 `009c643` 已先合，两端字面量集对齐）：`lib_ui/options.ts` `WEEKEND_OPTIONS` 3→4 档（新增 `single`「单休」＝单休+大小周+双休只排除无休；`no_bigweek` 矛盾 hint 校正为「找大小周+双休；排除单休/无休（未标注按双休含）」；`double` hint 与可见名维持）；`api/types.ts` `ScrapeDefaultsArgs.workWeekend` `unknown`→`'any'|'double'|'no_bigweek'|'single'` union（回读 `scrape_defaults` 仍 `Record<string,unknown>` 不动）；`FilterPanel.vue`/`scrape-filter-model.ts` 选项表驱动零改动。门禁：typecheck 0；vitest 35 文件 608 全绿（字面量集断言＋`single` 经 decode/applyBackend 两条路校验用例）；build JS 253.00 kB（+0.11，`value:"single"` 与新 hint 可达、旧矛盾 hint 0 残留）、CSS 不变。

---

**聊天两处行为修正（2026-11-25 用户 m00145 / m00393）**：① **建会话不再挡住界面**——点还没打过招呼的岗位，`pick()` 不再 `await chat_open`（真机要开浏览器点「立即沟通」再等页面自己跳聊天页，实测 5–15 秒、上限 ~80 秒），只把占位行留在列表、挂「正在打开 Boss 会话…」就返回，用户可以继续点别的会话；真 uid 到了再回填那一行，人还停在上面就顺势进入并拉消息。同一行在飞的 Promise 记在 `ChatPage.vue` 的 `booting` 表里，连点 / 切走再切回只发一次 `chat_open`；失败时占位行留着、提示写在 `note`，再点即重试。② **`local_only` 行**——`chat_open` 给的是聊天页地址里的**加密 uid**，与 `chat_friends` 的**数字 uid** 不是一套（实库已证），只按 uid 合并会让刚建的会话被下一次刷新整条吃掉，故换 uid 时打 `local_only` 并记 `job_id`，`mergeFriends` 仅在远程按岗位 id / 公司名收录到同一条时让位。用例 `tests/chat-session-boot.test.ts`、`tests/chat-model.test.ts`。

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
| CSS 块注释的**收尾那一行**里出现反引号包住的代码字面量 | 反引号之后的文本里带 `*/`，注释提前闭合，整份样式表被静默判定非法（`geometry.test.ts:141` 就是为此而写）。`.app-body` 那批新注释第一版就踩了：`` `.nav-mid` `` 和 `*/` 落在同一行 | 行尾注释行不要出现反引号；改完跑 `npx vitest run` 里那条 CSS 注释用例 |
| 概览常挂后右栏被挤掉一列 / 顶栏 teleport 落位盒变成布局盒 | `.app-body` 由 column 改 row 后，`config` / `logs` 的包裹 `<section>` 还留在 flex 流里占一格；`.tb-embed` 若不给 `display: contents` 就多做一层容器 | 浮层窗包裹层 `position: fixed; inset: 0; pointer-events: none`（弹窗本体 `pointer-events: auto`）；落位盒必须 `display: contents` |
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
- **会话副行的 `[送达]`/`[已读]` 靠两个独立字段，缺一个就永远只有裸文本**：`last_from_me`（方向）为真、且 `last_status` 为 `1`/`2` 才出词（`chat-model.ts` 的 `friendMsgTone`）。`last_status` **不是方向**，Boss 用 `0` 表示「对方发的」。发消息后由 `markFriendSent` 就地写 `last_status = 1`，**不许伪造 `2`（已读）**——已读只能等下一次真拉 `chat_friends`。发完也不要立刻重拉列表：那条命令要 reload 整个聊天页（秒级），会把「发出去立刻有反馈」的体感毁掉。

### 2.4 布局与视觉

- **页面背景不透明 = 毛玻璃失效**：`tokens.css` 的 `body` 有 `background: transparent`，不要覆盖它。
- **首帧闪白**：靠 `index.html` 的 3 条内联 `<style>` 压住，不要挪到外部 CSS 文件。
- **敏感库面板被压到视口外**：`vue-glass.css:8` 的 `.glass-l2 { position: relative }` 后加载，同特异性会覆盖 `sensitive.css` 的 `position: fixed`，面板落到 body 底部（实测 `y=774`，超出 900px 视口）。已用 `styles/sensitive.css` 的 `.sensitive-panel.glass-l2 { position: fixed; }` 钉回（提交 `6b1ae9c`）。改动 `.glass-l2` 或面板定位时必须复测。
- **CDP 实测方法**：用 headless Chrome `--remote-debugging-port=9333 --no-sandbox` 打开 `http://127.0.0.1:5173/`，经 `/json/list` 取页面 ws，用 `Runtime.evaluate` 读 `getBoundingClientRect` / `getComputedStyle`。注意每次加载都是新页面，重复点击会切换开关，判断前先确认初始状态。
- **无 Tauri 后端时的表现**：所有 `invoke` 报 `__TAURI_INTERNALS__ 不可用`，列表与右栏均为错误态，只能验证结构，不能验证 Boss 列表数据与日志栏内容。
- **右栏错误「叠字」未证实**：测得关闭对话框时只有 1 个 `.rail-error`（`FriendList.vue:133`），`h=131` 是内容撑开的结果。截图叠字疑似 `rail-in` 入场动画截帧，未复现，暂不改标记。
- **`FriendList` 会渲染两份**：页面右栏与对话框 `#rail` 槽各挂一份（`ChatPage.vue` 的 `prefix="chat-rail"` 与 `chat-detach-rail`，后者 `v-if="ui.chatOpen"`）。改错误/空态时两处都要生效。
- **顶栏右组未溢出**：1440 视口下 `.tb-tail.right=1389`，`.topbar` 内容宽 1388，恰好放下。不要因此改顶栏宽度。
- **窗口拖动 / 双击最大化只认最上方 20px**（用户裁决 2026-11-25）：`header.topbar` **不得**再挂静态 `data-tauri-drag-region`（那会让整条 46px 都能拖），改由 `lib_ui/drag-allow.ts` 在捕获阶段按 `ev.clientY < TITLE_STRIP_HEIGHT(20)` 给**命中元素**补属性；上一次的标记必须在下一次 pointerdown 时摘掉，否则该节点永久变成拖动区（曾经的「整窗都能拖」根因）。双击最大化由 Tauri 自带 `drag.js` 处理，**不要**自己再发 `window_toggle_maximize`，否则一次双击翻两次。回归：`tests/title-strip-drag.test.ts`。
- **日志与决策记录的信息密度**（用户裁决 2026-11-25）：运行日志**正序**（第一行在最上面、新行追加到底部，`features/logs/model.ts::mergeRunRows` 按 `ts asc || seq asc` 排，**不要改回倒序**）；首屏停在第一行，只有用户自己滚到底之后才跟随新行，翻到中间时出现「回到最新」。级别只给**标签**上色（INFO 走 `--accent` / WARN 走 `--warn-*` / ERROR 走 `--bad-*`，正文一律 `--text-1`），整行染色会让满屏日志变成色块。时间列必须 `max-content + white-space: nowrap`：定宽列装不下 `MM-DD HH:mm:ss` 会把日期与时分折成两行（曾被误判成「没有时间戳」）。决策记录行多显示两样库里本就有的数据：`platform`（`platformName`，空串给 `—`）与带年份的 `timeTitle`（tooltip）。滚动条一律 `overflow-y: scroll` + `--scroll-thumb/--scroll-track`（overlay 滚动条不悬停时几乎不可见，曾被误判成「没有滚动条」）。回归：`tests/logs-cursor.test.ts` / `tests/logs-model.test.ts` / `tests/decisions-row.test.ts` / `tests/decisions-css.test.ts`。
- **实心主按钮 / 品牌标不挂绿色外发光**（用户裁决 2026-11-25）：`.btn-primary` 只留 `inset 0 -1px 0` 内嵌暗线，悬停也只有 `.btn:hover` 的 1px 抬升；`.brand-mark` 同样只留内嵌暗线。绿色 box-shadow（原 `--btn-primary-shadow` / `--btn-primary-shadow-hover`，两枚令牌已删）叠在玻璃底上会散成一圈绿色光晕，被当成「玻璃折射高光」。登录（`PlatformCard`）、打招呼（`JobCard`）、开始抓取（`ScrapeBar`）都是 `.btn-primary`，一处改全改。复查口径：CDP 悬停前后比 `boxShadow` / `backdrop-filter` / `::before`·`::after`，现在只有 `transform` 变化。
- **概览页日志卡要有内容、且贴底跟随**（用户裁决 2026-11-25）：日志卡原先只在「已登录 + 抓取运行中」才轮询，空闲态永远是空框，用户看到的是「滚下去没有操作信息，但决策记录里的运行日志能看到」。现在 `DashboardPage.vue::primeLogs` 在登录态一到就补一次 `list_logs` 快照（**只补一次**，`logPrimed` 守门；`list_logs` 只读 Rust 进程内 `LogRing`，不碰 sidecar、不导航浏览器），实时追加仍由 `startLogPolling` 的运行态闸门管——**空闲态依然一条定时 IPC 都不发**。`LogCard.vue` 的 `.log-rows` 加了贴底跟随（`stickBottom`，8px 容差，用户手动上翻后停止跟随）：卡高只放得下约 8 行，不跟随的话看到的永远是最早的几行。回归：`tests/dashboard-expect-passive.test.ts`（原「挂载时不轮询」用例改成「只补一次快照，空闲不轮询」）。
- **日志弹窗：整块一个滚动区 + 筛选条吸顶 + 页签选中态**（用户裁决 2026-11-25）：
  - `.tab.is-active` 的**本体**样式原先根本不存在（只有 `.tab.is-active .tab-num` 给计数换色），选中与未选中几乎看不出区别——用户报「选中的 tab 没有高亮」。现 `controls.css` 给 `.tab.is-active` 实心主色药丸（`--primary-soft` 底 + `inset 0 0 0 1px var(--primary)` 环），另加 `.tab:hover:not(.is-active)` 浅玻璃。**不要再只给 `.tab-num` 上色。**
  - 运行日志与决策日志的筛选条已**移进滚动容器内部**：`.run-log-box` / `.dl-rows` 的第一个子节点，类名 `.log-tools.is-sticky`（`position: sticky; top: 0`，底色必须是不透明的 `--popup-surface`，否则滚过去的内容会从筛选条底下透出来）。整块内容只有一个滚动条，工具栏也不会被挤出可视区。
  - 内容一律不截断：`.dl-row .dl-who` 去掉了 `nowrap + ellipsis`、`.dl-row .dl-why` 去掉了两行 clamp（用户口径「所有信息都写进滚屏，不要有遗漏」），行高随内容长。
  - 筛选条件必须看得见：`logs-run-filter-state` / `logs-decision-filter-state`（主色 `--primary-text`）显示「筛选 级别 错误 / 关键字「xxx」」；`watch([levels, runKeyword])` 在条件变化时把列表滚回顶部。**排查提示**：级别筛选「行数不变」不等于没生效——当选中的那一档占满了全部日志时，行数本来就不变，先看筛选状态文字与「N / M 条」。
- **未核实项**：日志栏与 `section.page-dashboard` 是否缺 `.dashboard` 类导致 `.app-body` 行布局错位（未运行态确认）；`controls.css` 的 `.btn-danger` 浅色主题覆盖（8 处调用，定义存在，覆盖未核）；`ScrapeBar` 的 `scrape-stop-btn` 初始 disabled、`chat-detach-close-btn` 文案（C5/C9，未核）；`.dashboard` padding-top 对齐（A2，未核）。

### 2.5 Boss 消息卡顿（一档 + 二档 4 + 三档 6 已落地；仅剩三档第 7 条待产品拍板）

> 现象：**点某个 Boss 会话约 1 秒后卡一下**；**点 Boss 列表旁刷新按钮**同样卡。F5 整页刷新**不卡**。
> 已完成部分见下方「已修」，**未完成部分见方案三档**，按档位接手即可。

**已修（勿重复排查）**

| 热点 | 病灶 | 处置 |
|---|---|---|
| L3 透镜自激循环 | `use-glass-l3.ts` 的 `moBody` 监听 `document.body` 的 `style`，帧末 `setOpacity` 写入又触发自己 → rAF 永不停止；`start()` 无条件重置 `lastSig`/`idleSince`，500ms 静止降频永久失效 | MutationObserver 只留 `attributeFilter:['class']`；`setOpacity` 用模块级 `appliedOpacity` 字符串比较；`frame()` 末尾 `scheduleNext(wait)` 按 `GLASS_L3.frameMinMs`(33ms) 限帧；`onScroll` 加 `activeDrawer() === null` 门控。实测渲染进程 CPU 371s/5s → 2.2s/5s |
| L2 克隆层深度复制 | `use-glass-clone.ts` 的 `contentMo` 监听 `{childList, subtree, characterData}` + 200ms 节流，任何文本变化都整屏 `cloneNode(true)` | `RECLONE_THROTTLE_MS` 200 → 1500，observe 去掉 `characterData` |
| 已排除 | `main.ts` 卡片高光（仅 `pointermove` 读 rect）、`use-canvas-scale.ts`（仅 resize 时 rAF）、`main.ts` resize 的 140ms 落定加类 | 非热点，勿再查 |

**根因（两条链路，同一病灶：远程数据整体替换 → 节点全量重建）**

- 链路 A「点会话」：`ChatPage.vue:242-245`
  ```ts
  const reply = await chatMessages(uid);   // ① IPC → Rust → sidecar → 真实 Chrome 抓取
  if (ui.value.activeFriendsUid !== uid) return;
  messages.value = reply.items;            // ② 整个数组一次性替换
  ```
  `chatMessages`（`api/commands/chat.ts:34`，命令 `chat_messages`）**不是读缓存**，会命令真实浏览器抓该会话；「一秒后」= 抓取回来的那一刻，不是渲染慢。
- 链路 B「点刷新」：`ChatPage.vue:219` `friends.value = [...locals, ...reply.items];`，40 个 `.friend` 行同时重建。
- 重建成本三处叠加（`components/chat/ChatThread.vue:114-126` + `styles/vue-chat.css:728-741`）：每个气泡重建都**重播 `bubble-in`**；`scale(0.985)` 让每气泡**强制独立合成层**（40 条 = 40 层）；气泡无 `contain`，一条变化牵动**整列表重排**。
- 已于 `.chat-detach-chat .chat-feed` 加 `content-visibility: auto` + `contain-intrinsic-size: auto 48px`：只省**视口外的布局与绘制**，**省不了重建与动画重播**，故仍卡。

**方案一档 · 纯 CSS（推荐先做，零逻辑风险）**

1. 入场动画改为按需触发，替换数据不重播：
   ```css
   .chat-detach-chat .bubble { /* 去掉 animation */ }
   .chat-detach-chat .chat-feed.is-entering .bubble { animation: bubble-in var(--dur) var(--ease-out); }
   ```
   `.is-entering` 只在**切换会话首次渲染**时临时挂上，`nextTick` 后 `classList.remove`；**不要**绑响应式状态（每帧变更会触发重算）。
2. 从 `@keyframes bubble-in` 拿掉 `scale(0.985)`（视觉几乎不可见，却让每行独立合成层）：
   ```css
   @keyframes bubble-in { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: none; } }
   ```
3. 给行加渲染隔离：`.chat-detach-chat .bubble-row, .friend { contain: layout paint; }`
   **注意**：`.friend`（`vue-chat.css:348`）已有 `overflow: hidden` + `flex: 0 0 auto`（`:358-360` 有注释说明防行被压扁），**这两行不要动**。

**方案二档 · 渲染结构**

4. 稳定 key：现在 key 是 `msgTestId(m) ?? \`local-${i}\``，远程消息有 `msg_id` 时稳定，**本地乐观气泡用索引**，刷新后索引错位会整列表重 diff。给乐观气泡生成稳定临时 id 随消息对象保存（`ChatPage.vue` 的 `messages.value = [...]` 乐观插入处，`:328` / `:354` 附近），key 改用该 id。
5. 虚拟化（可选）：仅当单会话常 100+ 条才值得，引入依赖收益/风险比不划算，**先做一档 + 第 4 条**。

**方案三档 · 数据流（须产品先拍板，不可单方面改）**

6. `ChatPage.vue:231-253` 现在是「先本地（仅 `messages.length === 0` 时填）→ 再等远程整体替换」，切换会话时 `:263` 先 `messages.value = []`，本地数据要等一轮 IPC。应改为：立刻用 `chatMessagesLocal(uid)` 渲染（去掉 `messages.length === 0` 前置）→ 远程结果按 `msg_id` **合并**（保留乐观气泡，只追加/更新）→ 不再整体替换。
7. 点击会话时不要强制走浏览器抓取：先只读本地库立刻出内容，抓取放后台或推迟到「展开聊天框」（`ChatPage.vue:772` 已有 `messages.value.length === 0` 条件可参考）。**会改变用户看到的数据新鲜度，须产品确认。**

**验收标准**

| 项 | 标准 |
|---|---|
| 点 40 条消息的会话 | 可感知卡顿消失，或降到 < 50ms |
| 点刷新（40 个 Boss） | 同上 |
| 气泡动画 | 切换会话仍见一次淡入；数据刷新**不重播** |
| 门禁 | `vue-tsc --noEmit` exit 0；`vitest run` 全绿（基线 39 files / 657 tests） |
| 回归 | 抽屉开合、Esc 分层、长按多选、右栏提示不受影响 |

**2026-11-25 二轮已落地 · 一档 + 二档 4 + 三档 6（未提交）**

- 一档第 1 条：`.bubble` 不再常驻 `animation`，改挂 `.chat-detach-chat.is-entering .bubble`；`ChatThread.vue` 用 `onBeforeUpdate` 在**首次重建的那一帧**直接给根节点加/摘 class（不进响应式状态，符合本条「不要绑响应式」的约束）。
- 一档第 2 条：`@keyframes bubble-in` 去掉 `scale(0.985)`（每气泡一个强制合成层的元凶）。
- 一档第 3 条：`.chat-detach-chat .bubble-row, .friend { contain: layout paint; }`；`.friend` 的 `overflow: hidden` / `flex: 0 0 auto` **未动**。
- 二档第 4 条：`ChatMessage` 增可选 `local_id`（前端内部字段，后端不认识也不回传）；`chat-model.seedLocalMsgIds(messages, previous)` 给乐观气泡补会话内稳定 id 并按「发送方+时刻+正文」在上一版列表里回填；key 改 `msgKey(m,i)`、testid 改 `msgTestIdOrLocal(m,i)`，`chat-msg-<id>` 锚点不变。
- 三档第 6 条：`loadMessages` 不再等一轮 IPC——本地缓存立即渲染；远程结果走 `chat-model.mergeMessages`：按 `msg_id` 对齐、**内容没变就复用原对象引用**（气泡不重建）、远程没有的乐观气泡留在尾部；抓取失败时**不再清屏**（有本地内容只补提示）。
- 链路 B：`loadFriends` 不再 `[...locals, ...reply.items]`，改 `chat-model.mergeFriends`：顺序口径同 `sortFriends`（占位置顶），未变的行返回入参同一对象引用，远程已有的 uid 不再保留占位副本。
- 门禁：`vue-tsc --noEmit` exit 0；`vitest` 39 files / **661 passed**；`vite build` exit 0。新增模型层回归 4 条（`chat-model.test.ts`：seedLocalMsgIds 幂等、msgKey/testid、mergeMessages 引用复用、mergeFriends 行级复用），`chat-contract.test.ts` 的 CSS 锚点改为断言「`.bubble` 无常驻 animation / `.is-entering .bubble` 才播 / 关键帧无 scale」。

**2026-11-25 三轮已落地 · 导航失败改人话（用户 m02288，未提交）**

- 症状：右栏红条整段贴 Playwright 原文（`Error: Page.goto: net::ERR_ABORTED at …` + 多行 `Call log`），把对话框撑掉半块，用户以为程序卡死。**其实主线程没被挡住**：`chat_*` 是 Tauri 同步命令，跑在 Tauri 自己的命令线程上阻塞等回执（`commands_chat.rs` 的 `CHAT_TIMEOUT` 90 秒），前端渲染不受影响。
- `ChatPage.vue` 新增 `navAbortOf(text, fallback)` / `isRetryableNote(text)`：认得 `ERR_ABORTED` / `ERR_CONNECTION` / `ERR_TIMED_OUT` / `ERR_NETWORK_CHANGED` → 翻成「Boss 页面没打开，重试即可」；认得浏览器断开 → 翻成「先点右上角「扫码登录」再点「刷新」」；**认不出的错误照原样显示**（不编话）。`loadFriends` / `loadMessages` / `openSessionFor` 的 catch 都过这一层。
- `noteTone` 把这类「重试即可」的提示归到 `bad`（顶部红条），灰色说明条仍只走 `bottomNote`。
- `styles/vue-chat.css` 的 `.detach-note` 加 `max-height: calc(var(--fs-sm) * 6); overflow-y: auto; overflow-wrap: anywhere;`——错误再长也不许把对话框撑高到看不见消息。
- 后端侧配套（`_goto` 收口、`ERR_ABORTED` 判落地、`KEY_FRIENDS` 只记账不解析）见 `scraper/HANDOFF.md` §2.5。
- 回归：新增 `scraper/tests/test_chat_goto.py`（6 条纯离线断言）。
- 门禁：`vue-tsc --noEmit` exit 0；`vitest` 39 files / **682 passed**；`vite build` exit 0；`cargo test` 451 passed / 0 failed；`compileall` exit 0。

**2026-11-25 四轮已落地 · uid 失效按岗位重建（用户 m02734 / m02802）**

- 症状：还没打过招呼的岗位点开 ⇒ 红条「没拿到 Boss 会话列表：可能未登录，或 Boss 接口结构已变化」；发送同样报这句。**不是没登录**，是 `messages()` / `send()` 在列表里查不到 uid 时去重拉整个 `friends()`——未聊岗位永远不在这个列表里，白拉一次才报错。
- 用户拍板的方案（m02802）：**uid 失效就按岗位重建**，不再去 `friends()` 里找。`ChatPage.vue` 新增 `rebindDeadSession(friend, text)`：错误文案命中「在 Boss 会话列表里找不到这条会话」且能拿到岗位 id 时，调 `chatOpen(jobId)` 走「立即沟通」拿服务端新 uid，把该行原 uid 原地换掉（`savePendingFriends` 落盘），再用新 uid 重拉一次消息。`loadMessages` 与 `send` 的 catch 都接这一层。
- 岗位 id 从哪儿来：`friend.job_id ?? (isPendingFriend(friend) ? pendingJobId(friend) : '')`。`job_id` 是这轮新增的 `Friend` 字段，**由 Rust 层按公司名从 `job_queue` 回填**（Boss 列表本身不带岗位 id，见 `scraper/HANDOFF.md` §2.4.4）。拿不到岗位 id 就退回原来的报错，不假装成功。
- **重建只做一次，不递归**：`loadMessages` 的 catch 里重拉成功后直接 return，不再回头走 `rebindDeadSession`；`send` 同理。否则一条永远建不起来的会话会无限重建。
- `navAbortOf` 增第三类：命中「在 Boss 会话列表里找不到这条会话」也翻成可重试的人话（`RETRY_READ` / `RETRY_SEND`），不贴后端原文。
- 顺带修掉两个真缺陷：
  - `pick()` 的占位分支原本 `return pick(realUid)`，重建后 uid 变了但 `pick` 里 `switched` 判据用的是入参，会漏拉消息；改成直接 `await loadMessages(realUid !== '' ? realUid : uid)`。
  - `watch(() => ui.value.chatOpen, …)` 在 `pick()` 之后立刻又调一次 `loadMessages(旧 uid)`，导致 `chat_messages` 发两次、`chat_open` 重建两次。新增两道守卫：`if (loadingMessages.value) return;` 与 `if (feedUid.value !== friend?.uid) return;`。为此引入 `feedUid`（「屏上这屏消息属于哪条会话」），与 `ui.value.activeFriendsUid`（选中）分开记——重建会中途换 uid，两者不能等同。
- 回归：`chat-page.test.ts` 新增「点开的会话在 Boss 上找不到 ⇒ 按岗位重建会话并重新拉消息」，断言 `chat_open` 只发一次、载荷是 `{ jobId: 'job-u1' }`、`chat_messages` 依次用旧 uid 和新 uid 各拉一次、界面不含后端原文。
- 门禁：`vue-tsc --noEmit` exit 0；`vitest` 38 files / **652 passed**；`vite build` exit 0；`cargo test` 451 passed / 0 failed；`compileall` exit 0。

**2026-11-25 五轮已落地 · 聊天三条命令键名纠偏（用户截图：建立会话失败 `missing required key jobId`，未跑门禁）**

- 症状：点开未聊过的岗位 / 会话 uid 失效时，红条贴 `建立会话失败：invalid args 'jobId' for command 'chat_open': command chat_open missing required key jobId`；Boss 页另有一条自家 toast「该boss不在好友列表，无法沟通」（**不是本仓库产出**，属 Boss 服务端拒绝建会话，与键名无关）。
- 根因：`#[tauri::command]` 的**平铺参数默认按 camelCase 取键**（tauri-macros 2.7.6 `src/lib.rs:63-64` 明写 `"camelCase"` (default)），而 `api/commands/chat.ts` 三条按旧注释发了蛇形。`chat_open.job_id` / `chat_send_resume.resume_id` 是必填 ⇒ 硬失败；`chat_send.job_id` 是 `Option<String>` ⇒ 缺键静默 `None`，「聊过就算投过」从未生效且无报错。
- 修法（用户拍板 A 案：对齐 Tauri 默认与既有 `get_job` / `update_score`）：`api/commands/chat.ts` 三条键名改 `jobId` / `resumeId`，注释改成真口径；`tests/commands-payload.test.ts` 三条断言与 describe 说明同步；`tests/chat-page.test.ts` 的重建断言改 `{ jobId: 'job-u1' }`；`backend-tauri/AGENTS.md` §3.3 重写为「结构体参数蛇形 + 平铺参数驼峰」两条规则，并记下 `Option` 缺键静默失效这一坑。
- 未验证：按用户要求本轮**不执行任何命令**（`vue-tsc` / `vitest` / `cargo test` / `tauri build` 均未跑），真机点一次「立即沟通」的 E2E 留待下一轮。

**仍待产品拍板**：三档第 7 条（点击会话只读本地库、抓取延后台）未做——它直接决定「读到的消息有多新」。右栏 `ChatError: 在 Boss 会话列表里找不到这条会话` 的后端病灶已除（见下），三档第 7 条不再是它的前提。二档第 5 条（虚拟化）按原判断不做。

**上一轮已落地（`7f47cba`）**

- 链路 B 不再是「刷新即 40 行全重建」的表面症状：会话列表已**落库** `chat_friends.last_ts`，`ChatPage.vue` 刷新先读本地、只把远程**更新**的行覆盖进内存（`chat_store.rs` 的 `WHERE excluded.last_ts IS NULL OR excluded.last_ts > COALESCE(chat_friends.last_ts, 0)`）。
- 链路 A 的「一点就命令浏览器」未改（三档第 6 条已于二轮补上，见上）。
- 同一提交另修：`.drawer` 基线 `opacity:0` 无复位导致「抽屉只有背景没内容」、抽屉关闭动画结束后再卸载、拖动 `inFlight` 不清账、L3 自激循环与克隆层节流（见「已修」表）。
- 门禁自报：`cargo test` 450 passed；`vue-tsc` exit 0；`vitest` 39 files / 657 passed。

**右栏错误文案 `读取消息失败：ChatError: 在 Boss 会话列表里找不到这条会话…`**：这不是前端 bug，是 `scraper/chat.py` 在实时页面上找不到该会话行时抛的 `ChatError`，经 Rust 透传到 `FriendList` 的 rail-error。**根因已于 2026-11-25 定位并修复，见 `scraper/HANDOFF.md` §2.4.1–2.4.3**：

- 接口列表字段名写错（`zpData.geekFriendList` ≠ 真实的 `zpData.result`）→ 解析 0 行 → 退回 DOM 兜底造假 uid `dom:{i}` → 点它必然报这句。
- 会话行名字选择器 `.name`/`.title` 对不上真实的 `.name-text` → `name` 恒空，只能靠公司名子串猜行。
- `_wait_messages` 把 `geekEnter` 计入提前返回 → 0.08 秒就判「没有消息」，`historyMsg` 其实 1.5 秒后到。

三处修完后实测：`friends()` 100 条 / 1.5 秒；`messages()` 0.33–0.37 秒出全部消息，`me`/`hr` 方向正确。**前端这条文案对应的后端病灶已除，三档第 7 条不再是根治此错的前提**（它仍是一个可选的「点击不阻塞」优化，新鲜度取舍须产品拍板）。

**实施注意**

- 一档纯 CSS 安全；二/三档动 `messages.value` 赋值顺序时，严禁改副作用依赖数组、Store 状态流转、IPC 参数（根 AGENTS 前端微调铁律）。
- `apply_patch` 偶发 `ReplaceFileW EIO (Win32 1175)`：先验证文件未被锁再重试一次。
- `vitest` 需完整文件访问（沙箱禁 spawn → esbuild `EPERM`）。
- **playwright-skill 未安装**，无法做浏览器自动化验证；验证手段是 `vue-tsc` + `vitest` + 手工复测。
- 改 CSS 前读本目录 `AGENTS.md`：分层顺序 tokens→layout→controls→jobs→drawer→motion→scrape→pages→sensitive→auto→filter；`vue-*` 禁颜色字面量；任何 CSS 文件不得含 `//!` 或 `///`（整份样式表被静默丢弃）。
- **聊天的两套 uid 不通用**：`chat_open` 返回的 uid 取自聊天页地址 `?id=`，是加密串（如 `c83b66ecd36a48070X1-3ti-FFBZ`）；`chat_friends` 每条的 `uid` 是纯数字（实库 145 行全是数字，另有 `dom:0…` 垃圾行）。按 uid 做「新会话 ↔ 好友列表」匹配永远失败，判同一条只能靠岗位 id / 公司名（`mergeFriends` 的 `confirmedBy`）。`scraper/chat.py` 里「地址里的 id 就是 friends() 的 uid」那句注释是错的。

---

## 3. 下一步

### 3.1 开工顺序（阶段 0–4 已完成，HEAD 5f7cddc；以下为收尾顺序）

1. **QA 只读验收**：✅ 已完成（qa-verify 判 QA_FAILED → 用户裁决全部修 → 修复轮落地，474 用例全绿，见 §1）。
2. **几何契约补齐**：✅ 已完成（用户裁决「补齐实现」，见 §1）——`--ui-scale` + 5 档 `--fs-*` 落地，174 处 font-size 全 var()、产物裸 px 归零，三窗 scale + 敏感库 720 居中窗 + A54，`--detach-w` 单一来源。残留：契约 CSS 160 处颜色字面量全量收口（非阻塞，另轮）。
3. **槽位 C 产物冒烟**（根 AGENTS §5.3 / ARCH-003 §6 R5，用户已授权 release lane）：先删 CSP `'wasm-unsafe-eval'`（ARCH-003:248 阶段 4 遗漏，WASM 已不存在），再 `cd backend-tauri && npx tauri build --no-bundle`（独占 `CARGO_TARGET_DIR=%TEMP%\jt-release`），产物拷到**工作区外**启动，确认 Vue 真渲染（CSP 白屏唯一实测点）。
4. **登出入口接线**：✅ 已完成（`login-state` emit + 前端订阅 + 保存/退出按钮，见 §1）。
5. **M1 同屏编排合并**：✅ 已完成（ADR-004 / SPEC-001，见 §1 的锚点增删清单）。**M2 像素对齐** ✅（`9a60d3f`）、**M3 视觉收尾** ✅（见 §1）。下一手：**QA** 按清单重锚 + 零 Mock 验收；分项 token 展示接线 ✅（M3 后续批，见 §1）；后端补累计写入排 M4 之后（Chief 顺序）。
6. ~~**Liquid Glass L2/L3**~~ **已整层移除**（2026-11-25，用户裁决，提交 `b5a9062`）。原先「已完成但真窗口视觉验收搁置」的账一笔勾销——没有代码就没有待验收项。只留第一层 `backdrop-filter` 玻璃，见 `AGENTS.md` §8。**单休小批已完成**（SPEC-004 §三：`options.ts` 加 `single` 档＋校 `no_bigweek` 矛盾 hint、`types.ts` `workWeekend` union 四值、字面量集断言＋`single` 经 decode/applyBackend 选项表驱动校验用例；`FilterPanel.vue`/`scrape-filter-model.ts` 选项表驱动零改动；后端三档 `009c643` 已先合，两端字面量集对齐，无静默无效窗口）。
7. 后端未决项（根 §7.2 #1/#4/#5）与 QA 低#1-3 alias 缺口，单独路由 legacy/backend lane。

### 3.2 需要拍板的开放项

| # | 问题 | 影响的文件 |
|---|---|---|
| 1 | **`filter.rs` 1,445 行怎么拆** | 建议 `features/scrape/components/*.vue` + `composables/useScrapeFilter.ts` |
| 2 | ~~是否同时接 Liquid Glass L2/L3~~ ✅ 已了结：实现过又**整层移除**（见 §1 作废说明）。此项不再需要拍板 | — |
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
7. **动效接线现状**（M3 更新）：`drawer-in-play` 已接（JobDetailDrawer）；`rail-in` / `bubble-in` M3 已接（CSS 驱动、节点创建即播，无需 `transitionend`/`animationend` 消费方）；C17 其余三类 `list-fade`/`row-in`/`token-bump` 常量保住、原型零真值，接线待裁决；`.job-card.is-leaving` 仍未实现（无原型真值）。

### 3.4 代码注释里的失效引用

原清单指向的 Leptos `.rs` 已全部退役（阶段 4），清单作废。Vue 侧悬空 `.rs` 溯源已在 M5 轮统一改写为「Leptos 版已退役，见 git 历史」；后端路径溯源保留带 `backend-tauri/` 前缀的全路径。
