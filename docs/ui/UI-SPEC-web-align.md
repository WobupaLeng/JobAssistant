# UI 规格：Leptos 界面对齐旧版 `web/`（UI-SPEC-web-align）

> 状态：`UI_READY` 候选稿 · 版本 1 · 2026-11-21
> 作者：Lane G【UI有稿设计】（只读产出，**未修改任何代码**）
> 唯一有稿基准：用户在会话附件里给的 4 张截图（旧版 `web/` 的真实运行画面）。
> 唯一只读参照源：`web/src/App.tsx`(2475 行) / `web/src/api.ts`(528 行) / `web/src/ChatPanel.tsx`(604 行) / `web/src/index.css`。
> 上游宪法：仓库根 `AGENTS.md`、`frontend/AGENTS.md`。

本文件是下一波【前端编码】的施工图。**行号一律为实测值**；凡「截图观感」与「源码事实」冲突处，第 7 节列出裁决项，第 8 节列出待裁决清单。

---

## 0. 阅读顺序与硬约束

1. **第 4 节（testid）与第 3 节（常量）是契约**，逐字照抄，不许改写、不许「顺手统一命名」。
2. 第 6 节的 CSS 落点决定实现批次；**`tokens.css` 一行都不许改**。
3. 所有「PENDING-LANE-F」的命令在 Lane F 落地前**只能置灰或显示占位**，不得造假表单（`frontend/src/features/config/state.rs:3-8` 已立此规）。
4. 前端红线（来自 `frontend/AGENTS.md` 第 4 节）：
   - 禁止硬编码满分 / 门槛，一律取 `get_scoring_spec` 下发的 `score_max` / `score_min_default`；
   - 页面背景必须透明（不透明底色会吃掉 Rust 侧亚克力）；
   - 可交互控件必须带 `data-testid`；
   - **CSS 文件里不允许出现 `//!` 或 `///`**（Rust 注释会让整份样式表被判定非法并**静默丢弃**）。

---

## 1. 信息架构

### 1.1 顶层：仍是「顶栏 + 四页」，不新增第五页

| 层 | 内容 | 现状 | 本规格处置 |
|---|---|---|---|
| 顶栏 | 品牌标「职」+ 构建号 `app-build` + 四个导航页签 `nav-dashboard` / `nav-chat` / `nav-config` / `nav-logs` + 口径 `score-spec` + 大模型状态 + `theme-toggle` | 已实现（`frontend/src/components/shell.rs:152-195`） | **保留不动**；仅在概览页头部追加「自动投递」「敏感库」两个入口按钮（页面内，不进顶栏） |
| 页 1 概览 `nav-dashboard` | 见 1.2 | 已有统计/列表/抓取/期望/试算 | **重排 + 改造为图 2 + 图 3 结构** |
| 页 2 Boss 消息 `nav-chat` | 会话列表 + 对话框 | 已有（`frontend/src/pages/chat.rs`） | 保留；按图外观感做细节微调（见 5.4） |
| 页 3 配置 `nav-config` | 四组 | 已有但**全只读** | 图 4 对齐；Lane F 命令到位前保持「只读诊断」 |
| 页 4 决策记录 `nav-logs` | 决策/运行日志 | 已有（`frontend/src/pages/logs.rs`） | 保留；概览页的「日志框」与它**不同源**，见 1.2.5 |
| 抽屉（非页面） | 敏感库 | **未实现** | 从概览页右上按钮打开，右侧滑出（图 1） |
| 抽屉（非页面） | 岗位详情 | 已实现（`features/jobs/drawer.rs`，右侧滑出 560px） | 保留；卡片点击即打开，与敏感库抽屉**互斥**（同一时刻只开一个） |

> **旧版没有页签**：`web/src/App.tsx` 是单页应用，所有面板同屏。用户要求「顶栏保留现有四页切换」= 保留 Leptos 版已有的 `Nav` 四项（`frontend/src/lib_ui/nav.rs:36-41`），**不是**要求新增页签。

### 1.2 概览页（`nav-dashboard`）纵向分区

从上到下、全宽单列：

1. `legacy-banner`（十分制迁移横幅）— 保持现有条件渲染
2. **页头工具条 `overview-toolbar`**：左侧「自动投递」控制区（`auto-toggle` / `auto-status` / `auto-pause`），右侧「敏感库 {n} 条」按钮（`sensitive-open`，点击开抽屉）
3. **抓取筛选面板 `scrape-filter-panel`（可折叠，图 3）**：折叠条常显，展开体含关键词区 → 三列下拉 → 薪资/经验/学历 → 规模/融资/行业/排除词 → 底部四组；面板尾部保留现有 `scrape-progress` 与 `scrape-feed`
4. **投递列表 `job-list`（图 2）**：页签行 + 批量条 + 三列卡片瀑布流（内含 e2e 隐藏 `<tbody>`，见 7.1）
5. **辅助区（折叠，默认收起）**：`rescore-panel`（全量重算）、`demo-panel`（打分试算）、`expect-panel`（求职期望）、统计胶囊 `overview-*`
6. **日志框 `run-log-box`**：`决策日志 / 运行日志（实时）`，等宽 10px，末尾自动滚底，取末尾 300 行（旧版 `App.tsx:2240-2265`）
7. `job-drawer-panel`（点卡片才出现）+ `sensitive-panel`（点按钮才出现）

> 用户简报里「概览页 = 图 2 + 图 3 + 自动投递控制条 + 日志框」= 上面 2/3/4/6 四项；5 是现有能力，**折叠收纳**但不删（e2e 的 `rescore-*` / `demo-*` 仍在 DOM 里，只是收进 `<details>` 展开区——见 7.3 的 e2e 风险）。

### 1.3 聊天页（`nav-chat`）

沿用现有三栏：会话列表（搜索 + 列表 + 未读角标）/ 对话框（消息流 + 打招呼草稿 + 简历条 + 输入区）。本规格只做「与旧版一致」的文案与信息层级对齐，结构不动（代码已有 309+319+139+43 行，风险集中在别改动）。

### 1.4 配置页（`nav-config`）

四组折叠卡：大模型 / 抓取默认参数 / 打分口径 / 数据目录与库概览。现有 testid 全部保留；Lane F 落地后把 `readonly` 打开成可编辑并补保存。

---

## 2. 逐块对照表

> 列含义：**区块** | **旧版 DOM/类名（`web/` 确切位置）** | **新版落点文件** | **结构要点** | **复用的现有组件** | **需要的 data-testid**
> 「旧版位置」一律 `file:line`，均已实测。`App.tsx` = `web/src/App.tsx`。

### 2.1 概览页 · 页头与自动投递

| 区块 | 旧版 DOM/类名 | 新版落点文件 | 结构要点 | 复用的现有组件 | 需要的 data-testid |
|---|---|---|---|---|---|
| 自动投递开关 | `App.tsx:1230-1248` `<button>` 文案 `自动打招呼 开/关`，开=`bg-emerald-500`、暂停=`bg-amber-500` | `frontend/src/features/auto/mod.rs`（新）+ `frontend/styles/auto.css`（新） | 单按钮切换；开启时按钮变绿并在右侧显示状态胶囊；关闭时只留一个灰按钮 | `Chip` | `auto-toggle`（新） |
| 自动投递状态胶囊 | `App.tsx:1249-1284` 呼吸点 + `{n}s 后发下一条` / `发送中…` / `已暂停` + `· 已发 {n} 条` | 同上 | 仅 `autoOn` 为真时渲染；文案三段优先级：发送中 > 已暂停 > 倒计时 | `StatusDot`（state=`running`/`warn`/`idle`）+ `Chip` | `auto-status`（新） |
| 自动投递暂停 | `App.tsx:1285-1302` `暂停` / `继续` | 同上 | 未开启时不渲染 | `btn btn-sm` | `auto-pause`（新） |
| 本轮计数 | `App.tsx:1241` `｜本轮已发 {autoCount} 条{，连续失败 n}` | 同上 | 挂在 `auto-status` 的 `title` 与内联文本 | — | `auto-count`（新） |
| 敏感库入口 | 无对应（旧版在左下日志区切 `logView`，`App.tsx:180-181`、`:2239`） | `frontend/src/pages/dashboard.rs` | 页头右侧 `btn btn-sm`，文案 `敏感库 {total} 条`；点击 `sensitive_open.set(true)` | `btn` | `sensitive-open`（新） |

### 2.2 概览页 · 抓取筛选面板（图 3）

| 区块 | 旧版 DOM/类名 | 新版落点文件 | 结构要点 | 复用的现有组件 | 需要的 data-testid |
|---|---|---|---|---|---|
| 面板外壳 + 折叠 | `App.tsx:1415-1421` 按钮 `收起筛选 ▲` / `筛选条件 ▼`；折叠体 `:1493-1909` | `frontend/src/features/scrape/filter.rs`（新文件）+ `frontend/src/features/scrape/panel.rs`（改造宿主） | 折叠条常显：左 `收起筛选 ▲`/`展开筛选 ▼`，中 开始/停止，右 JD 上限/门槛；展开体默认**收起**（`filtersOpen` 初值 false，`App.tsx:196`） | `Card`(test_id=`scrape-filter-panel`) + `Collapsible` 不可直接用（其 header 结构是 `.config-group`），改用 `div.scrape-filter-head` + `Show` | `scrape-filter-panel`、`scrape-filter-toggle`（新） |
| 开始抓取 | `App.tsx:1423-1435`（旧版是「开始/暂停/继续」合一按钮） | `frontend/src/features/scrape/panel.rs:290-297`（已有 `scrape-start-btn`） | **保留现有按钮**（`scrape-start-btn`），文案 `启动中…`/`开始抓取`；旧版的「暂停」语义不迁移（Rust 侧只有 start/stop） | `btn btn-primary` | `scrape-start-btn`（已有，禁改） |
| 停止抓取 | `App.tsx:1436-1438` `btn-danger 停止` | `frontend/src/features/scrape/panel.rs:298-305` | 保留；`disabled = !running` | `btn btn-danger` | `scrape-stop-btn`（已有，禁改） |
| 运行状态 | `App.tsx:1439-1454` `抓取中`/`已暂停` + `空闲 · progress_text` | `frontend/src/features/scrape/panel.rs:252-254` | 保留现有；文案由 `ScrapeProgress.running` 驱动 | `StatusDot` | `scrape-status`（已有，禁改） |
| 有效 JD 上限 | `App.tsx:1457-1468` `有效 JD 上限 [n] 条`（onBlur → `apiClient.setLimit`） | `frontend/src/features/scrape/filter.rs` | `input.input-num` 宽 64px；onBlur 保存；`0 = 不限制` | `Field` | `scrape-filter-max-jd`（新） |
| 分值门槛 | `App.tsx:1469-1488` `分值门槛 ≥ [n] 分（低于不入库）` | 同上 | onBlur 保存；**默认值来自 `score_min_default`**，不写死 | `Field` + 现有 `scrape-min-score` 二选一（见 7.2） | `scrape-filter-min-score`（新） |
| 关键词区标题 | `App.tsx:1507-1508` `关键词（可多个，逐个搜索）` | `frontend/src/features/scrape/filter.rs` | 标题 + `已选 {n}` + 右侧清空 | — | `scrape-filter-kw-label`（新） |
| 关键词 chips（带 ×） | `App.tsx:1510-1524` `chip` + 一个 `×` 按钮 | 同上 | 每个 chip 一个删除按钮；空态提示 `还没有关键词` | `Chip`, `chip.as-btn` | `scrape-filter-kw-chip`、`scrape-filter-kw-del-{kw}`（新） |
| 关键词输入 | `App.tsx:1525-1538` placeholder `输入后回车添加（可空格/逗号分隔多个）` | 同上 | Enter 分割添加；分割符 `split(/[\s,，、;；|/\\]+/)`（`App.tsx:8-12`）→ 复用 `lib_ui::text::split_list`（**注意它不切 `、` 和 `|` 和 `\`，见 7.5**） | `input` | `scrape-filter-kw-input`（新） |
| 读取期望职位 | `App.tsx:1539-1569` chip `读取期望职位` | `frontend/src/features/scrape/expectations.rs`（已有 `expect-load-btn`） | 筛选区放一个「读取期望职位」按钮，复用现有面板的调用 | `chip.as-btn` | `scrape-filter-expect-btn`（新） |
| 期望职位回填 chips | `App.tsx:1573-1589` | 同上 | 展示 Boss 期望职位名，点击并入关键词 | `Chip` | `scrape-filter-expect-{idx}`（新） |
| 推荐关键词 | `App.tsx:1590-1601` + `RECOMMEND_KEYWORDS`(`:114`) 6 项 | `frontend/src/features/scrape/filter.rs` | 常量原样：`Java` / `后端开发` / `全栈工程师` / `Node.js` / `服务端开发` / `Java开发工程师` | `chip.as-btn` | `scrape-filter-recommend-{idx}`（新） |
| 三列下拉 | `App.tsx:1606-1641` 城市 / 职位类型 / 搜索最大翻页次数 | 同上 | `城市` 默认 `101280600`；`职位类型` 首项 `<option value="">不限</option>` 默认 `1901`；`翻页次数` number min1 max20 默认 3 | `Field` + `select.input` | `scrape-filter-city`、`scrape-filter-job-type`、`scrape-filter-max-pages`（新） |
| 薪资档位 | `App.tsx:1644-1718`（`ChipRow`，`App.tsx:69-111`）6 档 + 自定义 | 同上 | 多选 chips；`已选 {n}` + `清空` | `Chip`，新建 `ChipRow` 组件（建议 `frontend/src/components/chip_row.rs`） | `scrape-filter-salary-{code}`、`scrape-filter-salary-clear`（新） |
| 薪资自定义 | `App.tsx` 自定义区 `[下限]K - [上限]K` + `添加` | 同上 | 产出 chip 编码 `sal:{low}-{high}`；`high >= 9000` 显示 `{low}K以上` | — | `scrape-filter-salary-low`、`scrape-filter-salary-high`、`scrape-filter-salary-add`（新） |
| 工作经验 | 同上区 | 同上 | 7 档多选 + `清空` | `ChipRow` | `scrape-filter-exp-{code}`（新） |
| 学历 | 同上区 | 同上 | **7 档**（截图说 6 档，源码是 7 档，以源码为准，见 7.4） | `ChipRow` | `scrape-filter-degree-{code}`（新） |
| 公司规模 | `App.tsx:1724-1739` | 同上 | 6 档 + `清空` | `ChipRow` | `scrape-filter-scale-{code}`（新） |
| 融资阶段 | 同上 | 同上 | **8 档**（截图说 9 档，以源码为准） | `ChipRow` | `scrape-filter-stage-{code}`（新） |
| 行业 | `App.tsx:1742-1787` `行业不限` + 18 档 + `自定义编码回车` | 同上 | 首格是「行业不限」按钮（清空行业）；18 档 chips；末尾 input 回车加自定义编码 | `ChipRow` + `input` | `scrape-filter-industry-any`、`scrape-filter-industry-{code}`、`scrape-filter-industry-custom`（新） |
| 排除关键词 | `App.tsx:1789-1809` textarea | 同上 | 占位 `外包 驻场 兼职 日薪 跨境 外贸 金融 银行 证券 区块链 Web3 985 211 硕士 博士 销售 产品经理 总监`；说明 `空格/逗号/顿号分隔；任意一项留空即表示不按该项排除，改动后自动保存`；`DEFAULT_EXCLUDE`(`:212-213`) = `外包,驻场,人力,兼职,跨境,金融,银行,证券,区块链,Web3,985,211,硕士,博士,销售` | `textarea` | `scrape-filter-exclude-words`（新） |
| HR 活跃度 | `App.tsx:1813-1905` 第一组 | 同上 | `HR 活跃度 → 排除`：不限 / 今日活跃 / 3 天内 / 一周内 | `Chip`（单选组） | `scrape-filter-hr-{1|3|7|any}`（新） |
| 工作制 | 同上 第二组 | 同上 | `不限` / `只投双休` / `大小周` | `Chip` | `scrape-filter-weekend-{any|double|no_bigweek}`（新） |
| JD 学历要求 | 同上 第三组 | 同上 | `JD 岗位要求 → 排除`：学历不限 / 大专 / 本科 / 硕士 / 博士 | `Chip` | `scrape-filter-veto-degree-{any|大专|本科|硕士|博士}`（新） |
| 成立年限 | 同上 第四组 | 同上 | number，占位 `留空 = 不排除` | `input.input-num` | `scrape-filter-founded`（新） |
| 抓取进度 | `App.tsx` 无（旧版是状态文本） | `frontend/src/features/scrape/panel.rs:308-349` | **保留现状**（5 个 Stat + sr-only 汇总），e2e 依赖 | `Stat` + `StatSkeleton` | `scrape-progress` / `scrape-scraped|inserted|duplicates|below|rejected`（已有，禁改） |
| 事件流 | 无（旧版是日志框） | `frontend/src/features/scrape/panel.rs:351-369` | **保留现状**；`accepted` 行 tag 必须是 `✔` | `div.feed` | `scrape-feed`（已有，禁改） |

### 2.3 概览页 · 投递列表（图 2）

| 区块 | 旧版 DOM/类名 | 新版落点文件 | 结构要点 | 复用的现有组件 | 需要的 data-testid |
|---|---|---|---|---|---|
| 页签行 | `App.tsx:1916-1932` 三胶囊 + `共 {total} 条`(`:1933`) + `刷新`(`:1934-1943`) | `frontend/src/features/jobs/list.rs`（改造） | 三页签 + 计数；选中态：未投递绿、已投递**深灰药丸**、不投递红 | 现有 `.tabs/.tab`，选中态换成实心药丸（新建 `jobs.css` 里的 `.tab.is-active` 变体改为 `.jtab.is-*`） | `job-tab-pending`、`job-tab-delivered`、`job-tab-rejected`（**新增**）、`job-tab-count-{slug}`、`job-refresh`（已有，禁改） |
| 排序 | `App.tsx:1944-1966` `加入时间` / `薪资` / `分值↓` | 同上 | 三个文字按钮；当前字段显示 ↓/↑ | `chip.as-btn` | `job-sort-time`、`job-sort-salary`、`job-sort-score`（新） |
| 批量条 | `App.tsx:1970-2002` | 同上 | `全选本页` checkbox + `已选 {n} 条` + `列表共 {total} 条`；n≥2 显示 `不投递（{n}）`/`允许投递（{n}）`，否则右侧提示 `勾选 2 条以上可批量标记；单条直接用行内标签` | `btn`、`checkbox` | `job-select-all`（新）、`job-selected-count`（新）、`job-batch-mark`（新，批量不投递）、`job-batch-allow`（新，批量允许）、`job-list-total`（新） |
| 卡片容器 | `App.tsx:2005-2011` `overflow-y-auto list-fade` + `columnCount: chatOpen?2:3` | 同上 | **CSS 多列瀑布流**（`column-count: 3` / `column-gap: 6px` / 卡片 `break-inside: avoid`）；窄窗（`max-width: 1100px`）降 2 列 | — | `job-grid`（新） |
| 卡片外壳 | `App.tsx:2028-2042` | 同上 | 状态配色：默认白底细边；选中 `ring-1` 主色；已投递 opacity .55 + 标题删除线；被排除 `is-rejected`。进场 `row-in` | — | `job-card-{job_id}`（新） |
| 卡片行 1 | `App.tsx:2045-2105` | 同上 | checkbox + 分数方块（`CARD_SCORE_COLOR`，`App.tsx:22-28`：≥8 绿实心 / ≥6 浅绿 / ≥4 琥珀 / else 红；已投递转灰）+ 标题（`title` 属性 + 省略号）+ 行内 `不投递`/`允许投递` 标签 | 现有 `score-num`（扩展） | `job-check-{job_id}`（新）、`job-score-{job_id}`（新）、`job-title-{job_id}`（新）、`job-allow-{job_id}`（新） |
| 卡片行 2 | `App.tsx:2107-2127` | 同上 | 公司名（空则 `（未知公司）`）+ 薪资（加粗）+ 平台 chip（boss=`Boss直聘`）+ 状态 chip | `Chip` | `job-company-{job_id}`、`job-salary-{job_id}`、`job-platform-{job_id}`、`job-status-{job_id}`（新） |
| 卡片行 3 | `App.tsx:2128-2143` | 同上 | `{location} | {experience} | {degree} | {company_scale}` + 右侧 `MM-DD HH:mm`（`lib_ui::time::fmt_time`） | — | `job-meta-{job_id}`、`job-time-{job_id}`（新） |
| 卡片行 4 | `App.tsx:2145-2176` | 同上 | `单击展开详情 / JD`（被排除时前缀 `已标记不投递 · `）+ `打招呼` 按钮（非 delivered）+ `📍 {company_address 首行}`（`max-width:45%` + 省略号） | `btn btn-sm` | `job-hint-{job_id}`、`job-greet-{job_id}`、`job-addr-{job_id}`（新） |
| 展开体 | `App.tsx:2178-2223` | **不搬**：改为打开现有 `JobDrawer` | 旧版是卡内展开；Leptos 版已有右侧详情抽屉（`frontend/src/features/jobs/drawer.rs`，`drawer.css:34` 562px），信息更全。**裁决见 7.6** | `JobDrawer` | `job-drawer-*`（已有，禁改） |
| e2e 兼容表 | 无（旧版没有表格） | `frontend/src/features/jobs/list.rs` | **必须**在卡片流之后保留一个 `<table class="table sr-only">`（视觉隐藏、`<tbody>` 内有每岗位一行 `<tr>`），否则 `tools/gui_e2e.py:99` 的 `tbody tr` 断言静默归零。写入文档 = 契约 | — | `job-row-{job_id}`、`score-{job_id}`、`job-count`、`job-empty`（已有，禁改） |
| 空态 | `App.tsx:2013-2017` `暂无岗位` | 同上 | 空态卡片居中，文案按页签区分 | `EmptyState` | `job-empty`（已有，禁改） |

### 2.4 概览页 · 日志框与折叠区

| 区块 | 旧版 DOM/类名 | 新版落点文件 | 结构要点 | 复用的现有组件 | 需要的 data-testid |
|---|---|---|---|---|---|
| 日志框 | `App.tsx:2240-2265` `card h-44` + 标题 `决策日志 / 运行日志` + `实时` + 等宽 10px 滚动区，`error` 红 / `warn` 琥珀，末尾滚底 | `frontend/src/features/logs/feed.rs`（新） | 数据源仍需命令（`PENDING-LANE-F`：`list_decisions` 或 `list_run_logs`）。Lane F 未到位时渲染 `运行日志需 list_decisions 命令（Lane F）` 的占位行，**不得假装有数据** | 现有 `.feed`/`.feed-line` 可复用（`scrape.css:19-74`） | `run-log-box`、`run-log-line`（新） |
| 统计胶囊 | 无（旧版顶栏 token 统计，`:1210-1229`） | `frontend/src/pages/dashboard.rs:110-149` | **保留**，收进折叠区「统计与维护」 | `Stat` | `overview-total`/`-listed`/`-rejected`（已有，禁改） |
| 全量重算 | 无 | `frontend/src/features/jobs/rescore.rs` | **保留**，收进折叠区 | — | `rescore-panel`/`-llm-toggle`/`-note`/`-btn`（已有，禁改） |
| 打分试算 | 无 | `frontend/src/features/score/demo.rs` | **保留**，收进折叠区 | — | `demo-*`（已有，禁改） |
| 求职期望 | 无 | `frontend/src/features/scrape/expectations.rs` | **保留**，收进折叠区（筛选区内已有 `读取期望职位` 副本指向同一命令） | — | `expect-*`（已有，不改名） |

### 2.5 敏感库抽屉（图 1）

| 区块 | 旧版 DOM/类名 | 新版落点文件 | 结构要点 | 复用的现有组件 | 需要的 data-testid |
|---|---|---|---|---|---|
| 抽屉外壳 | `App.tsx:2285-2294` `fixed inset-0 z-40 ... bg-black/25` + `card max-h-[86vh] w-[min(1080px,94vw)]`（**居中浮层**） | `frontend/src/features/jobs/sensitive.rs`（新）+ `frontend/styles/drawer.css`（追加 `.sensitive-*`） | 用户要**右侧滑出大抽屉**（约 65% 宽），取 `width: min(1040px, 72vw)`，复用 `.drawer` 的右侧滑出与 `--ease-glass`；点击遮罩/Esc 关闭 | 复用 `drawer.css` 的 `.drawer`/`.drawer-backdrop` 骨架 | `sensitive-panel`（新）、`sensitive-backdrop`（新）、`sensitive-close`（新） |
| 标题行 | `App.tsx:2296-2300` `敏感库 {total} 条` + 副标题 + `关闭` | 同上 | 副标题逐字：`已抓过的岗位（已入库 + 被排除）都会记在这里，下一轮抓取直接跳过；可手动移出` | — | `sensitive-title`（新） |
| 搜索 | `App.tsx:2304-2320` input placeholder `搜索公司 / 岗位 / 排除原因`（Enter 即搜）+ `搜索` + `重置` | 同上 | 回车与按钮等价；重置清关键词且回到「全部」 | `input`、`btn btn-sm` | `sensitive-search`（新）、`sensitive-search-btn`（新）、`sensitive-reset`（新） |
| 类别筛选 | `App.tsx:2325-2340` 三 chip `全部`/`已入库`/`已排除`（`chip-on`） | 同上 | 单选；对应 `SensitiveFilter::All/Stored/Rejected` | `Chip` | `sensitive-filter-all`、`sensitive-filter-stored`、`sensitive-filter-rejected`（新） |
| 全选 | `App.tsx:2345` `全选` / `取消全选` | 同上 | 作用于**当前页全部条目** | `chip.as-btn` | `sensitive-select-all`（新） |
| 移出所选 | `App.tsx:2350-2360` `移出所选（{n}）`，n=0 禁用 | 同上 | `disabled` 由选中数驱动 | `btn btn-sm btn-danger` | `sensitive-remove-selected`（新） |
| 网格 | `App.tsx:2367-2369` `grid auto-rows-min gap-1.5` + `repeat(auto-fill, minmax(300px,1fr))` | 同上 | **三列**（抽屉够宽时 `repeat(3, 1fr)`，窄于 900px 退 2 列、<620px 退 1 列） | — | `sensitive-grid`（新） |
| 卡片 | `App.tsx:2377-2466` | 同上 | 行1：checkbox(`accent-rose-500`) + 平台 chip + 状态 chip（`已入库` 绿 / `已排除` 红）+ 公司名 + 薪资（右对齐加粗）；行2：状态词 + 岗位名（`已排除` 时加删除线）；行3：`已投递信息：{reason 或 已入库（抓取过，下一轮自动跳过）}` / `排除原因：{reason 或 -}`（红字）+ 右下 `HH:mm:ss` + `移出` | `Chip` | `sensitive-item-{id}`（新）、`sensitive-check-{id}`（新）、`sensitive-remove-{id}`（新） |
| 加载/空态 | `App.tsx:2371-2375` `加载中…` / `（暂无记录）` | 同上 | — | `SkeletonRows` / `EmptyState` | `sensitive-empty`（新） |

### 2.6 聊天页

| 区块 | 旧版 DOM/类名 | 新版落点文件 | 结构要点 | 复用的现有组件 | 需要的 data-testid |
|---|---|---|---|---|---|
| 左：会话列表 | `ChatPanel.tsx:319-402` 标题 `Boss 消息` + 计数 + `刷新` + `收起对话框 ▶`/`◀ 对话框` + 搜索 `搜索 HR / 公司 / 消息` + 会话项（头像/昵称/岗位/时间/公司 · 最后消息/未读角标红） | `frontend/src/features/chat/list.rs`（已有） | 已基本对齐；仅确认占位文案与未读角标配色 | 现有 | `chat-search`/`chat-friend-item-{uid}`/`chat-friend-unread-{uid}`（已有） |
| 右：对话框 | `ChatPanel.tsx:404-522` 标题 `对话框` + `{name} · {brand}` + `刷新记录` + `查看岗位` 外链 + 消息流（系统事件用居中胶囊） | `frontend/src/features/chat/thread.rs`（已有） | 已对齐 | 现有 | `chat-feed`/`chat-typing`（已有） |
| 输入区 | `ChatPanel.tsx:524-626` `生成回复` + `发送简历` + 说明 `按提示词 + 岗位 JD + 我的技能生成，可直接改再发送` + textarea 占位 `发消息给 {name}…（Enter 发送，Shift+Enter 换行）` + `发出后该岗位自动进入「已投递」` + `撤回` + `发送` | 同上 + `frontend/src/features/chat/composer.rs` | 保留；文案照旧版 | 现有 | `chat-input`/`chat-send-btn`/`chat-greet-btn`/`chat-resume-btn`/`chat-recall-btn`（已有） |

### 2.7 配置页（图 4）

| 区块 | 旧版 DOM/类名 | 新版落点文件 | 结构要点 | 复用的现有组件 | 需要的 data-testid |
|---|---|---|---|---|---|
| 顶部工具条 | `App.tsx:1303-1305` `配置` / `收起配置` 按钮（旧版是浮层） | `frontend/src/features/config/panel.rs:97-117`（已有 `config-tools`） | 保留；说明文案在 Lane F 落地后从「只读诊断」改为「改动即时保存」 | — | `config-tools`（已有） |
| 组 1 大模型 | `App.tsx:1319-1408` API Key(Base64 脱敏回显) / Base URL / 模型 / 提示词(+`恢复默认`) / 技能 | `frontend/src/features/config/panel.rs:120-177` | 现有 4 个字段 + **新增** `greet_prompt` textarea、`skills` 输入、`score_mode` 单选（rule/llm） | `ConfigSection`(`features/config/form.rs`) | `config-llm-key`/`-base-url`/`-model`/`-prefilter`（已有）、`config-llm-prompt`、`config-llm-prompt-reset`、`config-llm-skills`、`config-llm-mode`（新） |
| 组 2 抓取默认参数 | `loadConfig` 回填 `max_jd` / `exclude_keywords` / `exclude_rules.*`（`App.tsx:704-731`） | `frontend/src/features/config/panel.rs:179-228` | 现有 3 字段 + **新增**排除词、排除规则 8 项（薪资上下限/学历/成立年限/简历年限/HR 活跃/工作制） | 同上 | `config-keywords`/`-limit`/`-min-score`（已有）、`config-exclude-words`、`config-exclude-salary-min`、`config-exclude-salary-max`、`config-exclude-degree`、`config-exclude-founded`、`config-exclude-resume-years`、`config-exclude-hr-days`、`config-exclude-weekend`（新） |
| 组 3 打分口径 | 两制换算说明（`App.tsx` 无独立组） | `frontend/src/features/config/panel.rs:230-268` | 保留满分/默认门槛/五维权重的只读值；五维权重文案来自 `core::domain::scoring::WEIGHTS`（**唯一真源，不得改字**：`技能 40 · 薪资 20 · 经验 15 · 方向 15 · 公司质量 10`） | — | `config-score-max`/`-min`（已有）、`config-score-weights`（新） |
| 组 4 数据目录与库概览 | 无（旧版顶栏 token） | `frontend/src/features/config/panel.rs:270-318` | 现有数据目录/岗位总数 + `by_scale` chips + legacy chip；**新增** token 累计（total/greet/score，`PENDING-LANE-F`） | `Chip` | `config-data-dir`/`config-db-total`（已有）、`config-tokens-total`、`config-tokens-greet`、`config-tokens-score`（新） |

---

## 3. 选项常量表（逐字摘录自 `web/src/api.ts`，不许改值、不许增删）

> 落点建议：`frontend/src/lib_ui/options.rs`（纯常量，`lib_ui` 不依赖任何其它层）。
> 值类型：`pub struct Opt { pub code: &'static str, pub name: &'static str }`。

### 3.1 `CITIES`（`api.ts:426-435`，8 项）

| # | code | name |
|---|---|---|
| 1 | `101280600` | 深圳 |
| 2 | `101020100` | 上海 |
| 3 | `101010100` | 北京 |
| 4 | `101280100` | 广州 |
| 5 | `101210100` | 杭州 |
| 6 | `101270100` | 成都 |
| 7 | `101190400` | 苏州 |
| 8 | `101030100` | 天津 |

### 3.2 `SALARY_OPTIONS`（`api.ts:438-445`，**6 项** + 自定义档）

| # | code | name |
|---|---|---|
| 1 | `sal:0-8` | 8K以下 |
| 2 | `sal:8-12` | 8-12K |
| 3 | `sal:12-16` | 12-16K |
| 4 | `sal:16-20` | 16-20K |
| 5 | `sal:20-30` | 20-30K |
| 6 | `sal:30-9999` | 30K以上 |

自定义档：`sal:{low}-{high}`，渲染时 `high >= 9000` 显示 `{low}K以上`。

### 3.3 `EXPERIENCE_OPTIONS`（`api.ts:447-455`，7 项）

| # | code | name |
|---|---|---|
| 1 | `101` | 在校/应届 |
| 2 | `102` | 经验不限 |
| 3 | `103` | 1年以内 |
| 4 | `104` | 1-3年 |
| 5 | `105` | 3-5年 |
| 6 | `106` | 5-10年 |
| 7 | `107` | 10年以上 |

### 3.4 `DEGREE_OPTIONS`（`api.ts:457-465`，**7 项**）

| # | code | name |
|---|---|---|
| 1 | `202` | 大专 |
| 2 | `203` | 本科 |
| 3 | `204` | 硕士 |
| 4 | `205` | 博士 |
| 5 | `206` | 中专/中技 |
| 6 | `208` | 高中 |
| 7 | `209` | 初中及以下 |

### 3.5 `SCALE_OPTIONS`（`api.ts:467-474`，6 项）

| # | code | name |
|---|---|---|
| 1 | `301` | 0-20人 |
| 2 | `302` | 20-99人 |
| 3 | `303` | 100-499人 |
| 4 | `304` | 500-999人 |
| 5 | `305` | 1000-9999人 |
| 6 | `306` | 10000人以上 |

### 3.6 `STAGE_OPTIONS`（`api.ts:476-485`，**8 项**）

| # | code | name |
|---|---|---|
| 1 | `801` | 未融资 |
| 2 | `802` | 天使轮 |
| 3 | `803` | A轮 |
| 4 | `804` | B轮 |
| 5 | `805` | C轮 |
| 6 | `806` | D轮及以上 |
| 7 | `807` | 已上市 |
| 8 | `808` | 不需要融资 |

### 3.7 `JOB_TYPE_OPTIONS`（`api.ts:487-491`，3 项）

| # | code | name |
|---|---|---|
| 1 | `1901` | 全职 |
| 2 | `1902` | 兼职 |
| 3 | `1903` | 实习 |

### 3.8 `INDUSTRY_OPTIONS`（`api.ts:494-513`，**18 项**）

| # | code | name |
|---|---|---|
| 1 | `100020` | 互联网 |
| 2 | `100021` | 电子商务 |
| 3 | `100022` | 计算机软件 |
| 4 | `100023` | 计算机硬件 |
| 5 | `100024` | 通信/网络设备 |
| 6 | `100025` | 电子/半导体/集成电路 |
| 7 | `100026` | 人工智能 |
| 8 | `100027` | 云计算/大数据 |
| 9 | `100028` | 企业服务 |
| 10 | `100029` | 金融/银行/证券 |
| 11 | `100030` | 游戏 |
| 12 | `100031` | 医疗健康 |
| 13 | `100032` | 教育培训 |
| 14 | `100033` | 汽车/新能源 |
| 15 | `100034` | 智能制造/工业自动化 |
| 16 | `100035` | 物流/仓储 |
| 17 | `100036` | 房地产/建筑 |
| 18 | `100037` | 消费零售 |

> 行业区另有「行业不限」按钮（清空选择）与「自定义编码回车」输入框；二者不是 `INDUSTRY_OPTIONS` 的成员，不得混进常量表。

### 3.9 `WEEKEND_OPTIONS`（`api.ts:143-147`，3 项；字段名是 `value` 不是 `code`）

| # | value | name |
|---|---|---|
| 1 | `any` | 不限 |
| 2 | `double` | 只投双休 |
| 3 | `no_bigweek` | 大小周 |

### 3.10 同文件其它被界面引用的常量（一并冻结）

| 常量 | 位置 | 内容 |
|---|---|---|
| `HR_ACTIVE_OPTIONS` | `api.ts:161-165` | `1` 今日活跃 / `3` 3 天内 / `7` 一周内（字段名 `days`） |
| `DEGREE_VETO_OPTIONS` | `api.ts:167` | `大专` / `本科` / `硕士` / `博士` |
| `PLATFORMS` | `api.ts:418-422` | `boss` Boss直聘 ready / `zhilian` 智联招聘 not-ready / `51job` 51job not-ready |
| `RECOMMEND_KEYWORDS` | `App.tsx:114` | `Java` / `后端开发` / `全栈工程师` / `Node.js` / `服务端开发` / `Java开发工程师` |
| `DEFAULT_EXCLUDE` | `App.tsx:212-213` | `外包,驻场,人力,兼职,跨境,金融,银行,证券,区块链,Web3,985,211,硕士,博士,销售` |
| 排除词 placeholder | `App.tsx:1789-1809` | `外包 驻场 兼职 日薪 跨境 外贸 金融 银行 证券 区块链 Web3 985 211 硕士 博士 销售 产品经理 总监` |

---

## 4. data-testid 命名表

### 4.1 硬约束

- **已有 testid 一律不得改名**：`tools/gui_e2e.py` 的 `REQUIRED_TESTIDS`(16) + `DEMO_FORM_TESTIDS`(5) + 属性写法(5) 是本项目的 E2E 契约。
- 新增 testid 一律 `kebab-case`，`{id}` / `{code}` / `{slug}` 位允许动态拼接。
- 冲突自检结果：**新增 0 条与已有冲突**（见 9.2 自检记录）。

### 4.2 已有 16 项（`tools/gui_e2e.py:69-87`）— 只读清单，禁改

| testid | 归位文件（`tools/gui_e2e.py:274-291`） | 本次是否移动 |
|---|---|---|
| `app-root` | `src/components/shell.rs` | 不动 |
| `score-spec` | `src/components/shell.rs` | 不动 |
| `job-refresh` | `src/features/jobs/list.rs` | 留在同一文件 |
| `scrape-status` | `src/features/scrape/panel.rs` | 留在同一文件 |
| `scrape-login-btn` | `src/features/scrape/panel.rs` | 留在同一文件 |
| `scrape-keywords` | `src/features/scrape/panel.rs` | **留在 panel.rs 本体内**（筛选面板里的 `scrape-filter-kw-input` 是另一个控件） |
| `scrape-min-score` | `src/features/scrape/panel.rs` | 同上 |
| `scrape-start-btn` | `src/features/scrape/panel.rs` | 留在同一文件 |
| `scrape-stop-btn` | `src/features/scrape/panel.rs` | 留在同一文件 |
| `scrape-progress` | `src/features/scrape/panel.rs` | 留在同一文件 |
| `scrape-feed` | `src/features/scrape/panel.rs` | 留在同一文件 |
| `rescore-panel` | `src/features/jobs/rescore.rs` | 不动（收进折叠区，DOM 仍在） |
| `rescore-btn` | `src/features/jobs/rescore.rs` | 同上 |
| `rescore-llm-toggle` | `src/features/jobs/rescore.rs` | 同上 |
| `rescore-note` | `src/features/jobs/rescore.rs` | 同上 |
| `demo-score-btn` | `src/features/score/demo.rs` | 同上 |

属性写法 5 项（`tools/gui_e2e.py:295-301`）：`scrape-scraped` / `scrape-inserted` / `scrape-duplicates` / `scrape-below` / `scrape-rejected`（`Stat` 的 `test_id=`）。
`DEMO_FORM_TESTIDS`：`demo-title` / `demo-title-input` / `demo-salary` / `demo-exp` / `demo-jd` / `demo-result`。

### 4.3 现有非契约 testid（保留原名，可扩充）

`app-build`、`nav-{slug}`、`page-{slug}`、`theme-toggle`、`app-error`、`app-error-dismiss`、`app-notice`、`legacy-banner`、`migrate-btn`、`overview-total|listed|rejected`、`overview-*-skeleton`、`job-count`、`job-tab-{all|active|rejected}`、`job-row-{id}`、`score-{id}`、`job-empty`、`job-drawer-backdrop|panel|title|company|score|score-max|close|chips|reason|kv|link|jd`、`scrape-panel`、`scrape-filter-*`（新增）、`expect-panel|load-btn|empty|item|apply-btn`、`demo-panel|title|llm-toggle|prefilter`、`config-panel|tools|reset-btn|llm-section|scrape-section|score-section|data-section|llm-key|llm-base-url|llm-model|llm-prefilter|keywords|limit|min-score|score-max|score-min|data-dir|db-total`、`chat-panel|refresh-btn|search|empty|friend-item-{uid}|friend-unread-{uid}`、`chat-hint|greet-btn|resume-btn|recall-btn|feed|typing|note|greet-box|greet-text|greet-send|greet-todraft|greet-discard|resume-bar|resume-empty|resume-{id}|resume-close`、`chat-input|chat-send-btn`、`logs-panel|refresh-btn|filter-{slug}|search|note|total|rejected|llm`。

### 4.4 新增 testid（按区块）

**图 3 抓取筛选面板（前缀 `scrape-filter`）**

| testid | 控件 |
|---|---|
| `scrape-filter-panel` | 面板外层 |
| `scrape-filter-toggle` | 收起/展开 |
| `scrape-filter-body` | 展开体容器 |
| `scrape-filter-max-jd` | 有效 JD 上限 |
| `scrape-filter-min-score` | 分值门槛 |
| `scrape-filter-kw-input` | 关键词输入框 |
| `scrape-filter-kw-chip` | 关键词 chip（容器） |
| `scrape-filter-kw-del-{kw}` | 关键词 chip 的 × |
| `scrape-filter-expect-btn` | 读取期望职位 |
| `scrape-filter-expect-{idx}` | 期望职位 chip |
| `scrape-filter-recommend-{idx}` | 推荐关键词 chip |
| `scrape-filter-city` | 城市下拉 |
| `scrape-filter-job-type` | 职位类型下拉 |
| `scrape-filter-max-pages` | 翻页次数 |
| `scrape-filter-salary-{code}` | 薪资档（含 `sal:8-12` 这类含冒号的值需转 `-`，见 7.7） |
| `scrape-filter-salary-low` / `-high` / `-add` / `-clear` | 自定义薪资 |
| `scrape-filter-exp-{code}` / `scrape-filter-exp-clear` | 工作经验 |
| `scrape-filter-degree-{code}` / `scrape-filter-degree-clear` | 学历 |
| `scrape-filter-scale-{code}` / `scrape-filter-scale-clear` | 公司规模 |
| `scrape-filter-stage-{code}` | 融资阶段 |
| `scrape-filter-industry-any` / `scrape-filter-industry-{code}` / `scrape-filter-industry-custom` | 行业 |
| `scrape-filter-exclude-words` | 排除关键词 textarea |
| `scrape-filter-hr-{any|1|3|7}` | HR 活跃度 |
| `scrape-filter-weekend-{any|double|no_bigweek}` | 工作制 |
| `scrape-filter-veto-degree-{any|大专|本科|硕士|博士}` | JD 学历要求 |
| `scrape-filter-founded` | 成立年限 |

**图 2 投递列表**

| testid | 控件 |
|---|---|
| `job-tab-pending` / `job-tab-delivered` / `job-tab-rejected` | 三页签 |
| `job-tab-count-{slug}` | 页签计数 |
| `job-sort-time` / `job-sort-salary` / `job-sort-score` | 排序 |
| `job-select-all` | 全选本页 |
| `job-selected-count` | 已选 n 条 |
| `job-list-total` | 列表共 n 条 |
| `job-batch-mark` | 批量不投递 |
| `job-batch-allow` | 批量允许投递 |
| `job-grid` | 卡片流容器 |
| `job-card-{job_id}` | 卡片 |
| `job-check-{job_id}` | 卡片勾选框 |
| `job-score-{job_id}` / `job-title-{job_id}` | 分数方块 / 标题 |
| `job-company-{job_id}` / `job-salary-{job_id}` / `job-platform-{job_id}` / `job-status-{job_id}` | 行 2 |
| `job-meta-{job_id}` / `job-time-{job_id}` | 行 3 |
| `job-hint-{job_id}` / `job-greet-{job_id}` / `job-addr-{job_id}` | 行 4 |
| `job-allow-{job_id}` | 行内「不投递 / 允许投递」标签 |

**图 1 敏感库**

| testid | 控件 |
|---|---|
| `sensitive-panel` / `sensitive-backdrop` / `sensitive-close` | 抽屉 |
| `sensitive-title` | `敏感库 {n} 条` |
| `sensitive-search` / `sensitive-search-btn` / `sensitive-reset` | 搜索 |
| `sensitive-filter-all` / `sensitive-filter-stored` / `sensitive-filter-rejected` | 类别 |
| `sensitive-select-all` | 全选 |
| `sensitive-remove-selected` | 移出所选 |
| `sensitive-grid` | 网格 |
| `sensitive-item-{id}` / `sensitive-check-{id}` / `sensitive-remove-{id}` | 卡片 |
| `sensitive-empty` | 空态 |
| `sensitive-open` | 概览页入口按钮 |

**自动投递**

| testid | 控件 |
|---|---|
| `auto-toggle` | 开/关 |
| `auto-status` | 状态胶囊 |
| `auto-pause` | 暂停/继续 |
| `auto-count` | 本轮已发 n 条 |

**日志框 / 配置页新增**

| testid | 控件 |
|---|---|
| `run-log-box` / `run-log-line` | 日志框 |
| `config-llm-prompt` / `config-llm-prompt-reset` / `config-llm-skills` / `config-llm-mode` | 大模型组 |
| `config-exclude-words` / `config-exclude-salary-min` / `config-exclude-salary-max` / `config-exclude-degree` / `config-exclude-founded` / `config-exclude-resume-years` / `config-exclude-hr-days` / `config-exclude-weekend` | 抓取默认参数组 |
| `config-score-weights` | 五维权重 |
| `config-tokens-total` / `config-tokens-greet` / `config-tokens-score` | token 统计 |

---

## 5. 命令映射表

> 已注册命令 27 条（`app/src-tauri/src/lib.rs:458-485`）。Lane F 计划新增 11 条，本规格按用户简报命名；**Lane F 若改了名字，以 Lane F 为准并回写本表**。

### 5.1 已有 27 条（存在即调用，无需新增）

| 命令 | 注册/实现在 | 本规格里的调用方 |
|---|---|---|
| `app_info` | `lib.rs:33` | 顶栏构建号、配置页组 4 |
| `get_scoring_spec` | `lib.rs:42` | 顶栏 `score-spec`、门槛默认值、配置页组 3 |
| `list_jobs` | `commands_db.rs:131` | 投递列表（三页签 + 排序） |
| `get_job` | `commands_db.rs:137` | 详情抽屉补齐 JD |
| `db_overview` | `commands_db.rs:152` | 统计胶囊、配置页组 4、敏感库入口条数（暂用 `total`） |
| `migrate_scores` | `commands_db.rs:175` | `legacy-banner` 的迁移按钮 |
| `score_job` | `commands_db.rs:272` | 打分试算（`demo-score-btn`） |
| `passes_threshold` | `commands_db.rs:284` | 暂未接界面（可保留） |
| `update_score` | `commands_db.rs:298` | 暂未接界面（可保留） |
| `rescore_jobs` | `commands_db.rs:399` | `rescore-btn` |
| `llm_status` | `commands_db.rs:511` | 顶栏大模型 chip、配置页组 1 |
| `scrape_start` | `commands_scrape.rs:267` | `scrape-start-btn` |
| `scrape_stop` | `commands_scrape.rs:347` | `scrape-stop-btn` |
| `scrape_progress` | `commands_scrape.rs:361` | `scrape-progress` 轮询 |
| `scrape_drain` | `commands_scrape.rs:380` | 事件流 `scrape-feed` |
| `login_start` | `commands_scrape.rs:423` | `scrape-login-btn` / 敏感库前置登录 |
| `scrape_close` | `commands_scrape.rs:442` | 应用启动/退出编排（已有） |
| `session_open` | `commands_scrape.rs:460` | 切到聊天页（已有） |
| `chat_friends` | `commands_chat.rs:120` | 会话列表 |
| `chat_messages` | `commands_chat.rs:132` | 消息流 |
| `chat_send` | `commands_chat.rs:151` | `chat-send-btn` |
| `chat_resumes` | `commands_chat.rs:176` | 简历条 |
| `chat_send_resume` | `commands_chat.rs:188` | 发送简历 |
| `chat_recall` | `commands_chat.rs:207` | `chat-recall-btn` |
| `greet_draft` | `commands_greet.rs:117` | `chat-greet-btn`、卡片「打招呼」（生成草稿） |
| `greet_one` | `commands_greet.rs:172` | 发送打招呼（`auto-toggle` 的单条执行路径） |
| `list_expectations` | `commands_chat.rs:228` | `expect-load-btn`、`scrape-filter-expect-btn` |

### 5.2 每个新增控件 → 命令

| 控件（testid） | 命令 | 依赖状态 |
|---|---|---|
| `scrape-filter-max-jd` / `scrape-filter-min-score`（onBlur 保存） | `set_scrape_defaults`（Lane F） | `PENDING-LANE-F`；未到位前**只改内存、不落库**，且按钮旁必须显示 `未保存（等命令层）` |
| `scrape-filter-kw-input`（Enter） | 纯前端状态 | — |
| `scrape-filter-expect-btn` | `list_expectations` | 已有 |
| `scrape-filter-*` 全部 chip | 纯前端状态（开始抓取时随 `scrape_start` 一起提交） | — |
| `scrape-start-btn` | `scrape_start`（现有签名：`platform, keywords, maxJd, minScore`） | 已有；**筛选面板的 9 类 chip 目前没有任何命令参数承载**，见 7.8 |
| `auto-toggle` / `auto-pause` / `auto-count` | `auto_greet_start` / `auto_greet_pause` / `auto_greet_next`（Lane F，3 条） | `PENDING-LANE-F`；未到位时 `auto-toggle` 置灰 + `title="等命令层（Lane F）"` |
| `job-tab-*` | `list_jobs`（参数映射见 5.3） | 已有 |
| `job-sort-*` | `list_jobs`（`sort`） | 已有（`salary` 需扩展，见 7.9） |
| `job-select-all` / `job-check-{id}` | 纯前端状态 | — |
| `job-batch-mark` | `jobs_reject`（Lane F，批量） | `PENDING-LANE-F` |
| `job-batch-allow` | `jobs_allow`（Lane F，批量） | `PENDING-LANE-F` |
| `job-allow-{id}`（行内） | `jobs_reject` / `jobs_allow`（单条） | `PENDING-LANE-F` |
| `job-greet-{id}` | `greet_draft` → 用户确认 → `greet_one` | 已有 |
| `job-card-{id}`（点击） | `get_job`（抽屉补 JD） | 已有 |
| `sensitive-open` / `sensitive-title`（条数） | `count_sensitive`（Lane F） | `PENDING-LANE-F`；未到位时用 `db_overview.total` 并标注「粗略」 |
| `sensitive-search` / `sensitive-filter-*` / `sensitive-select-all` | `list_sensitive`（Lane F，`keyword`/`kind`/`limit`） | `PENDING-LANE-F` |
| `sensitive-remove-{id}` / `sensitive-remove-selected` | `delete_sensitive(ids)`（Lane F） | `PENDING-LANE-F` |
| `run-log-box` | `list_decisions`（Lane F） | `PENDING-LANE-F`；未到位显示占位行 |
| `config-llm-*` / `config-exclude-*` / `config-score-*` 可编辑化 | `get_app_config` / `set_app_config`（Lane F，2 条） | `PENDING-LANE-F`；未到位保持 `readonly` |
| `config-tokens-*` | `get_app_config`（同上，字段 `tokens_total` / `tokens_greet` / `tokens_score`） | `PENDING-LANE-F` |
| `config-reset-btn` | 现有 `reload()`（`llm_status`+`db_overview`+`get_scoring_spec`+`app_info` 四并发） | 已有 |

**Lane F 11 条汇总**：`list_sensitive`、`count_sensitive`、`delete_sensitive`、`list_decisions`、`get_app_config`、`set_app_config`、`jobs_reject`、`jobs_allow`、`jobs_mark_delivered`（或与 `jobs_reject` 合并的 `set_delivered`）、`auto_greet_next`、`auto_greet_one`。本规格另**缺失**一条 `set_scrape_defaults`（JD 上限/门槛落库）——若 Lane F 未列入，`scrape-filter-max-jd`/`min-score` 继续走内存态，见第 8 节。

### 5.3 页签 → `list_jobs` 参数映射

| 页签 | testid 后缀 | `ListJobsArgs` |
|---|---|---|
| 未投递 | `pending` | `status: Some("pending")`, `include_rejected: false` |
| 已投递 | `delivered` | `status: Some("delivered")`, `include_rejected: true`（已投递优先展示，不受排除影响） |
| 不投递 | `rejected` | `include_rejected: true`，前端按 `job.rejected != 0` 过滤 |

> `ListJobsArgs` 无 `rejected_only` 字段（`commands_db.rs:99-112`），「不投递」只能就地过滤，且 **`limit` 必须放大**（建议 2000）否则被排除的岗位会被 `include_rejected:false` 之外的原因截断。裁决见 7.1。

### 5.4 聊天页

| 控件 | 命令 |
|---|---|
| `chat-refresh-btn` / `chat-search` | `chat_friends` |
| 会话项点击 | `chat_messages`（+ `session_open("boss", Some("chat"))`） |
| `chat-greet-btn` 生成 | `greet_draft` |
| `chat-greet-send` | `greet_one` |
| `chat-resume-btn` / `chat-resume-{id}` | `chat_resumes` → `chat_send_resume` |
| `chat-recall-btn` | `chat_recall` |
| `chat-send-btn` | `chat_send` |

---

## 6. 样式落点

### 6.1 铁律

- **`frontend/styles/tokens.css` 一行都不许改**（它是 Liquid Glass 令牌的唯一真源，深浅两套数值已调优）。
- 新 CSS 文件必须在 `frontend/index.html` 追加 `<link data-trunk rel="css" href="styles/<file>.css" />`，位置放在消费它的文件之后。
- 所有颜色/圆角/阴影/时长**只能引用 `var(--…)`**，不许写裸色值（浅底深底观感不同，裸值必翻车）。

### 6.2 必须清理的 Rust 注释残留（P0）

| 文件 | 行 | 内容 | 处置 |
|---|---|---|---|
| `frontend/styles/controls.css` | 2-3 | `//!` / `//! Liquid Glass 下控件与卡片的差别只有一层：控件更小、玻璃更薄、按下时下沉。` | **删除这 2 行**，并把这句话翻成 `/* … */` 块注释补在文件第 1 行下面 |
| `frontend/styles/layout.css` | 2-4 | `//!` ×3（`与 tokens.css 的分工…` / `类名与 TS 版 skeleton-web…`） | **同样删除并翻成 `/* … */`** |

> 这两份文件目前**全部样式可能已被浏览器静默丢弃**（CSS 解析器遇到 `//!` 会判定整份非法）。清理后必须重新截图核对深浅两套主题——若观感「突然变了」，那不是回归，是样式终于生效了。

**现状取证（2026-11-21 06:5x，规格定稿时实测）**：`Select-String -Path frontend/styles/*.css -Pattern '^\s*//[!/]'` 命中 **5 行**，全部在这两份文件：

```
controls.css:2   controls.css:3   layout.css:2   layout.css:3   layout.css:4
```

其余 6 份（tokens / jobs / drawer / motion / scrape / pages）**干净**。注意 `controls.css:1` 已经是合法的 `/* 分域样式：控件 … */`，第 2-3 行的 `//!` 是它下面另起的 Rust 文档注释——清理时**不要连同第 1 行一起删**。

### 6.3 每块落点

| 区块 | 文件 | 动作 | `index.html` |
|---|---|---|---|
| 顶栏 / 外壳 / 卡片 / 栅格 | `styles/layout.css` | 修 `//!`；新增 `.overview-toolbar` | 已有 link |
| 按钮 / 输入 / 页签 / 胶囊 / 状态点 / 骨架 | `styles/controls.css` | 修 `//!`；新增 `.jtab`（实心药丸变体）、`.checkbox`、`.select` | 已有 link |
| 图 2 投递列表卡片流 | `styles/jobs.css` | 追加 `.job-grid`（`column-count`）、`.job-card`、`.job-card.is-delivered`、`.job-card.is-rejected`、`.job-score-box`、`.job-meta`、`.job-addr`；保留现有 `.table*`（e2e 隐藏表用） | 已有 link |
| 图 1 敏感库抽屉 | `styles/drawer.css` | 追加 `.sensitive-*`（外壳、头、工具行、网格、卡片）；复用 `.drawer` 的右侧滑出 | 已有 link |
| 图 3 筛选面板 | **新建** `styles/filter.css` | `.scrape-filter-head`、`.scrape-filter-body`、`.chip-row`、`.chip-row-head`、`.chip.is-on`（浅绿底 + 浅绿描边）、`.filter-grid-3`、`.filter-group` | **追加一行** link（放在 `scrape.css` 之后，`pages.css` 之前） |
| 自动投递控制条 | **新建** `styles/auto.css` | `.auto-bar`、`.auto-toggle.is-on`、`.auto-status`、`.auto-count` | **追加一行** link（放在 `filter.css` 之后） |
| 日志框 | `styles/scrape.css` | 复用 `.feed` / `.feed-line`，追加 `.run-log-box`（高 176px，等宽 10px） | 已有 link |
| 配置页新增字段 | `styles/pages.css` | 追加 `.config-card.is-editable`、`.cfg-inline-chips`（排除规则 chip 组） | 已有 link |
| 过渡动画 | `styles/motion.css` | 复用 `.row-in` / `.list-fade`；新增 `.job-card.is-leaving`（离场：左移 24px + 缩 0.98 + 透明） | 已有 link |
| 聊天页 | `styles/pages.css` | 不动（已对齐） | 已有 link |

> 追加后的 `index.html` link 顺序应为：tokens → layout → controls → jobs → drawer → motion → scrape → **filter** → **auto** → pages。

---

## 7. 截图观感 vs 源码事实（逐项裁决）

| # | 用户转录（截图观感） | `web/` 源码事实 | 裁决 |
|---|---|---|---|
| 7.1 | 图 2 = 三列卡片网格，无表格 | `App.tsx:2005-2011` 用 `columnCount` CSS 多列瀑布流；旧版**没有**表格 | 新界面用 `column-count: 3` 瀑布流（视觉一致），另加一张 `.sr-only` 表格承载 e2e 的 `tbody tr`。**这是本规格唯一的「双结构」，必须两端同步**（见 2.3 最后一行） |
| 7.2 | 「分值门槛 ≥ [7]」是筛选面板顶条 | `App.tsx:1469-1488` 确实在顶条，但它独立于 `scrape-min-score` 存在（旧版只有一个 input） | Leptos 侧 `scrape-min-score` 是 e2e 契约（禁改），因此**保留 `scrape-min-score` 作为唯一编辑控件**，顶条位置只做**镜像显示**（只读文本 `分值门槛 ≥ {n} 分（低于不入库）`），点它把折叠面板展开并聚焦到 `scrape-min-score`。新增的 `scrape-filter-min-score` 撤销，不引入第二个 input |
| 7.3 | 折叠区里的 `rescore-*` / `demo-*` | 旧版没有这两块 | e2e 用 `wait_for` 轮询 testid；若收进 `<details>` 而默认 `closed`，`querySelector` 仍能命中（`<details>` 折叠不会移除 DOM），但 `ElementHandle.click()` 会失败。**裁决：折叠容器用 CSS `max-height`/`display:none` 之外的方式（`aria-expanded` + 高度过渡），保证元素始终可点；或默认展开「统计与维护」折叠区**。见第 8 节风险 |
| 7.4 | 学历 6 档 / 融资 9 档 / 行业 19 档 / 薪资 7 档 | `api.ts` 实测：学历 **7**、融资 **8**、行业 **18**（+「行业不限」按钮）、薪资 **6**（+自定义档） | **以源码为准**（第 3 节常量表）。截图里数错是因为按钮换行 |
| 7.5 | 关键词输入「可空格/逗号」 | `App.tsx:8-12` 的 `SPLIT_KEYWORDS` 还切 `、` `;` `；` `|` `/` `\`；而 Leptos 的 `lib_ui::text::split_list`(`frontend/src/lib_ui/text.rs:7-18`) 只切 `,` `，` `;` `；` 和空白 | **给 `split_list` 补 `、` `|` `/` `\`**（或新增 `split_keywords`），否则用户敲 `Java、Go` 会变成一个关键词。这是唯一必须改 `lib_ui` 的功能缺口 |
| 7.6 | 卡片「单击展开详情 / JD」 | `App.tsx:2178-2223` 卡内展开 | 新界面点卡片打开**右侧详情抽屉**（已有 `JobDrawer`，信息更全且不破坏瀑布流布局）。卡面第 4 行仍显示 `单击展开详情 / JD` 文案 |
| 7.7 | 薪资档位 chip 的 testid | 6 档 code 形如 `sal:0-8`（含冒号） | testid 里的 `{code}` 必须做**字符转义**：`:`→`-`，即 `scrape-filter-salary-sal-8-12`。同理行业/经验/学历 code 都是纯数字，无此问题 |
| 7.8 | 筛选面板 9 类 chip（城市/薪资/经验/学历/规模/阶段/行业/职位类型/翻页） | 旧版 `/api/start` 的 payload 带这些字段（`StartPayload`，`api.ts:169+`）；**Rust 侧 `ScrapeArgs` 只有 platform/keywords/max_jd/min_score**（`commands_scrape.rs:161-168`） | **这是最大的契约缺口**：筛选面板做出来也传不到后端。裁决：面板先按本规格落到前端（状态齐备、可折叠、可勾选），`scrape_start` 继续只发 4 个参数，并在面板脚注显示 `筛选条件暂未下发（等 scrape_start 扩展，Lane F）`。扩展 `ScrapeArgs` 属跨进程契约，需另立任务 |
| 7.9 | 排序「加入时间 / 薪资 / 分值」 | `SortField::parse`(`core/domain/job.rs:311-318`) 已支持 `created` / `salary` / `score`；但前端 `features/jobs/state.rs:48-52` 的 `SortField` 枚举**只有 Score/Created/Updated** | 前端枚举补 `Salary`（`as_str = "salary"`），后端无需改 |
| 7.10 | 图 1 敏感库「右侧滑出 65% 宽」 | `App.tsx:2285-2294` 是**居中浮层** `w-[min(1080px,94vw)]` | 按用户截图观感做**右侧抽屉**（`width: min(1040px, 72vw)`），与岗位详情抽屉同一套 `.drawer` 骨架；两者互斥打开 |
| 7.11 | 状态 chip「待投递」 | `App.tsx:42-52` `STATUS_CHIP`：delivered→已投递、rejected→不投递、否则→待投递 | 照抄三态。注意后端 `Job.status` 只有 `pending`/`delivered`（`core/domain/job.rs:11-14`），「不投递」来自 `rejected != 0` |
| 7.12 | 分数方块「10 / 9 / 8 三色」 | `CARD_SCORE_COLOR`(`App.tsx:22-28`) 阈值 8/6/4，用于**十分制** | 当前是百分制（`scoring.rs: WEIGHTS` 合计 100），阈值必须**按比例换算**：`score >= 0.8*max` 绿实心、`>= 0.6*max` 浅绿、`>= 0.4*max` 琥珀、否则红。**禁止写死 80/60/40**，用 `score_max` 算 |

---

## 8. 迁移顺序（每批 ≤ 2 个文件簇，串行落地）

> 「文件簇」= 一组必须同批改完才自洽的文件。每批结束都要跑 `cargo check --target wasm32-unknown-unknown`（`frontend/` 内）。

| 批 | 文件簇 | 内容 | 自检 |
|---|---|---|---|
| **B0** | ① `styles/controls.css` + `styles/layout.css` | **清理 `//!` 残留**，把 3+2 行 Rust 注释翻成 `/* */` | 深浅双主题各截一张图，确认控件样式**此刻才第一次真正生效**（观感变化不是回归） |
| **B1** | ① `src/lib_ui/options.rs`（新）+ `src/lib_ui/text.rs` ② `src/lib_ui/mod.rs` | 落第 3 节 9 组常量（含 `HR_ACTIVE/dEGREE_VETO/PLATFORMS/RECOMMEND/DEFAULT_EXCLUDE`）；给 `split_list` 补 `、`/`|`/`/`/`\` 分隔符 | `cargo test --target wasm32-unknown-unknown`（`lib_ui` 已有 6 个用例，新增分隔符用例 ≥1） |
| **B2** | ① `src/features/jobs/state.rs` + `src/features/jobs/list.rs` | 页签 All/Active/Rejected → **Pending/Delivered/Rejected**；`SortField` 补 `Salary`；列表改卡片瀑布流 + `.sr-only` `<tbody>` 兼容表 | e2e `tbody tr` 行数仍 >0；`job-tab-rejected` 存在；`job-tab-all`/`job-tab-active` 移除（**确认 `gui_e2e.py` 未引用**） |
| **B3** | ① `src/features/scrape/filter.rs`（新）+ `styles/filter.css`（新） ② `src/features/scrape/panel.rs` + `frontend/index.html` | 筛选面板全量控件（图 3）+ 折叠；`scrape-*` 8 项 testid 原位不动；`index.html` 加 link | `scrape-keywords`/`scrape-min-score`/`scrape-start-btn`/`scrape-stop-btn`/`scrape-progress`/`scrape-feed` 仍在 `panel.rs` |
| **B4** | ① `src/features/jobs/sensitive.rs`（新）+ `styles/drawer.css` ② `src/pages/dashboard.rs` + `styles/auto.css`（新） | 敏感库抽屉（图 1）+ 概览页页头（自动投递条 + 敏感库入口） | Lane F 未到位时全部显示「等命令层」占位，**不得造假数据** |
| **B5** | ① `src/features/config/panel.rs` + `src/features/config/state.rs` ② `styles/pages.css` + `frontend/index.html` | 配置页四组补齐（图 4），`readonly` 依 Lane F 状态切换 | `config-*` 14 项现有 testid 全部保留 |
| **B6** | ① `src/features/logs/feed.rs`（新）+ `styles/scrape.css` ② `src/pages/dashboard.rs` | 日志框接线（`PENDING-LANE-F` + 占位） | `run-log-box` 存在且不伪造内容 |
| **B7** | ① `tools/gui_e2e.py`（**只读方以外的唯一允许改动点，需 lead 批准**） | 把新 testid 加进 `REQUIRED_TESTIDS`（至少 `job-tab-pending/delivered/rejected`、`job-card-*`、`sensitive-panel`、`auto-toggle`、`scrape-filter-panel`），并修 `TESTID_HOMES` 里 `job-refresh` 等路径 | `python tools/gui_e2e.py run --ui-only` 通过；`contract_check()` total 从 22 上升但**旧 22 项全部仍通过** |

> B0 必须最先做：`//!` 未清时，后续所有视觉核对都在「样式全废」的基线上做，会得出错误结论。

---

## 9. 自检记录（本规格自身的可核对项）

### 9.1 对照表行数

命令化统计（`Get-Content` + 逐行匹配 `^\| ` 且排除表头/分隔行，`### 2.x` 分节）：

- 2.1 概览页页头与自动投递：**5 行**（`UI-SPEC-web-align.md:48-52`）
- 2.2 抓取筛选面板：**27 行**（`UI-SPEC-web-align.md:85-111`）
- 2.3 投递列表：**12 行**（`UI-SPEC-web-align.md:117-128`）
- 2.4 日志框与折叠区：**5 行**
- 2.5 敏感库抽屉：**9 行**
- 2.6 聊天页：**3 行**
- 2.7 配置页：**5 行**
- **合计 66 行**

> 口径说明：一个 Markdown 表格行 = 一个「区块」。`frontend/src/pages/dashboard.rs` 折叠区内的独立卡片（统计胶囊 / RescorePanel / ScoreDemo / Expectations）在 2.4 里合并为 4 行 + 日志框 1 行，不在别处重复计数。

### 9.2 testid 冲突自检

新增 testid 逐项与 4.2（16 项）、4.3（现有非契约项）比对：**0 冲突**。特别核对：

| 易冲突项 | 结论 |
|---|---|
| `job-tab-{slug}` 现为 all/active/rejected，新增 pending/delivered/rejected | 与 `job-tab-rejected` **同名同义**（沿用）；`job-tab-all`/`job-tab-active` 退役，`gui_e2e.py` 未引用两者（`REQUIRED_TESTIDS` 无 `job-tab-*`），安全 |
| `scrape-keywords` / `scrape-min-score` vs `scrape-filter-kw-input` / `scrape-filter-min-score` | 前缀 `scrape-` 相同但**完整串不同**，不冲突；且按 7.2 裁决 `scrape-filter-min-score` 不落地 |
| `job-status-{id}` 与 `job-tab-*` | 不同命名段，不冲突 |
| `sensitive-*` 全新前缀 | 与现有 `logs-*` / `config-*` 无交集 |

### 9.3 常量表核对

第 3 节 9 组常量逐项对照 `web/src/api.ts`：

| 常量 | 源码行 | 源码条数 | 本表条数 | 结果 |
|---|---|---|---|---|
| `CITIES` | 426-435 | 8 | 8 | ✅ |
| `SALARY_OPTIONS` | 438-445 | 6 | 6 | ✅ |
| `EXPERIENCE_OPTIONS` | 447-455 | 7 | 7 | ✅ |
| `DEGREE_OPTIONS` | 457-465 | 7 | 7 | ✅ |
| `SCALE_OPTIONS` | 467-474 | 6 | 6 | ✅ |
| `STAGE_OPTIONS` | 476-485 | 8 | 8 | ✅ |
| `JOB_TYPE_OPTIONS` | 487-491 | 3 | 3 | ✅ |
| `INDUSTRY_OPTIONS` | 494-513 | 18 | 18 | ✅ |
| `WEEKEND_OPTIONS` | 143-147 | 3 | 3 | ✅ |
| **合计** | — | **66** | **66** | ✅ |

### 9.4 命令映射核对

命令化核对（`app/src-tauri/src/lib.rs:458` 起的 `invoke_handler` 块，逐行断言）：

- **已注册命令实测 27 条**，与第 5.1 节表格逐条一致（`app_info` … `list_expectations`），无一遗漏、无一虚构。
- **Lane F 的 11 条在 `app/src-tauri/src/**.rs` 中全部 `absent`**（`get_app_config` / `set_app_config` / `list_sensitive` / `count_sensitive` / `delete_sensitive` / `list_decisions` / `jobs_reject` / `jobs_allow` / `auto_greet_next` 逐名 grep 无命中）——所以本规格把它们全部标成 `PENDING-LANE-F`，**没有任何一处按「已存在」写**。
- 本规格额外识别 1 条缺失命令 `set_scrape_defaults`（JD 上限 / 分值门槛落库），全仓同样 `absent`。

---

## 10. 需要用户裁决的点（REMAINING）

1. **【高】筛选面板 9 类条件传不到后端**：`scrape_start` 只有 `platform/keywords/max_jd/min_score`（`commands_scrape.rs:161-168`），而 `web/` 的 `/api/start` 有完整 `StartPayload`。是「先做 UI、条件暂不生效」还是「先扩 `ScrapeArgs` 再做 UI」？扩协议属跨进程单点（`protocol.rs` / `sidecar.py`），按 `AGENTS.md` 2.5 需单写者串行。
2. **【高】`job-tab-all` / `job-tab-active` 退役**：现有三页签是「全部/未排除/已排除」，新三页签是「未投递/已投递/不投递」。旧两页签删掉后，用户再也看不到「全部岗位」视图——是否保留第四个「全部」页签（会违背「三段药丸」的截图布局）？
3. **【中】敏感库抽屉宽度**：截图观感 65%，旧版源码是居中 1080px。规格暂定 `min(1040px, 72vw)` 右侧抽屉，请确认。
4. **【中】折叠区与 e2e 的冲突**（7.3）：`rescore-*` / `demo-*` 若默认折叠，`gui_e2e.py` 的点击可能失败。建议默认展开「统计与维护」折叠区，或授权改 `gui_e2e.py`（B7）。
5. **【中】`set_scrape_defaults` 是否存在**：JD 上限 / 分值门槛的落库需要一条命令；Lane F 的 11 条里是否包含它？未包含则这两项只能内存态。
6. **【低】`lib_ui::text::split_list` 扩大分隔符**是否允许（7.5）？它是 `lib_ui` 的公开纯函数，被 `dashboard.rs` 与 `expectations.rs` 调用，改动影响 2 处（都朝更宽松方向，风险低）。
7. **【低】日志框数据源**：旧版是 `/api/logs?since=`（运行日志流），Lane F 只计划了 `list_decisions`（决策日志）。两者语义不同，是否新增一条 `list_run_logs`？
