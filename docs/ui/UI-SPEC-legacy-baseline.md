# UI-SPEC · 旧版「AI 求职助手」→ 本项目设计基线

> 状态：`BASELINE_PROPOSED`（§9 的 10 个开放项已由用户逐条裁决并回写本文；剩余未决项见 §10）
> 版本：v2 · 2026-11-22（v2 依据用户裁决回写：删除页签导航与版本徽标、最小宽度 1200px、配置改为浮层、决策记录改为弹窗、对话框三级展开、Boss 列表底部无发送框；见 §10）
> 证据：[`UI-REFERENCE-legacy-notes.md`](UI-REFERENCE-legacy-notes.md)（每条结论的等级 A/B/C/D 在那里）
> 像素参照：[`docs/ui/reference/`](reference/)（11 张裁切图；原图所在临时解包目录已删除，仅裁切图长期可用）
> 只读参考：根 [`AGENTS.md`](../../AGENTS.md)、[`frontend-vue/AGENTS.md`](../../frontend-vue/AGENTS.md)（契约 C1–C17）、[`ARCH-003`](../architecture/ARCH-003-vue-frontend-migration.md)、[`ADR-003`](../adr/ADR-003-frontend-leptos-to-vue.md)、`frontend-vue/styles/*.css`（11 份 3,197 行）、`frontend-vue/.agents/skills/liquid-glass/SKILL.md`、`frontend-vue/.agents/skills/apple-design/SKILL.md`

---

## 0. 一句话结论（三条，先看这个）

1. **布局照旧版、材质用现有：** 信息架构（区块顺序、字段、页签、按钮、文案）以本文 §3 / §4 / §4.1 为**唯一真源**；视觉语言（玻璃、令牌、动效）以 `frontend-vue/styles/` 与两份技能为**唯一真源**。旧版的纯白卡片、实心绿按钮、浅粉危险按钮一律**不进**新界面（§6）。
2. **两栏必须做，右栏宽 300px 是裁决值不是测量值：** 现有 `.app-shell` 是「顶栏 + 单列 `.dashboard`」，旧版是「顶栏 + 主列 + Boss 消息栏」。结论是**保留两栏**——右栏做成 `.dashboard` 的 `position: sticky` **兄弟列**，主列仍是原来的单列卡片流，卡片顺序改为照旧版（控制条 → 列表 → 日志，仓库既有卡片追加在后）。这样 C1–C17 无一条需要作废（§2.3）。截图里右栏真实宽度**未取证**，300px 是本次设计裁决。
3. **旧版截图是「断网空库」时的样子：** 计数互相矛盾、没有卡片、没有批量按钮、日志只有一行。规格把每个区块的**全部状态**补齐，不把截图当成唯一真相；凡是截图没有的，等级一律标 D，并在 §9 列为未取证或需拍板项。

---

## 1. 来源与约束

| 约束 | 原文 / 位置 | 对本规格的含义 |
|---|---|---|
| 单文件分发、无 HTTP 端口 | 根 `AGENTS.md` §1 | 界面里**不出现**「服务未启动 / 端口 / 5173」字样；连通性故障走错误条 |
| 视图层不写业务规则 | 根 `AGENTS.md` §3.1 | 「分值门槛 / 有效 JD 上限 / 是否入库」全部只做**展示与提交**，判定在后端 |
| 迁移契约 C1–C17 | `frontend-vue/AGENTS.md` §4 | 本规格不新增调用约定；只约束**文案、结构、状态** |
| 页面背景透明 | `frontend-vue/AGENTS.md` 红线 3 / C5 | 右栏**不得**有自己的不透明底；它透出的底与主列一致 |
| 禁第二套 UI 库、禁硬编码满分级 | 红线 2 / 6、C10 | 门槛输入框的 `max`/`placeholder` 来自 `get_scoring_spec`，不写死 |
| 玻璃三层分工 | `frontend-vue/AGENTS.md` §8 | 本规格只用**第一层**（`backdrop-filter` + 令牌 + 渐变高光边）；第二/三层不在本次范围 |
| 光学与动效 | `liquid-glass/SKILL.md` §1 / §4；`apple-design/SKILL.md` §1/§3/§4/§12/§14/§15 | 见 §5 的「动效映射」；手势类规则（拖拽/甩动）在本界面**无适用面**，不做 |

---

## 2. 信息架构

### 2.1 分区树（新界面）

```
Window（WebView2，客户区）
└─ .app-outer            外层，唯一不透明度来源 = 窗口亚克力透出的底；padding 10 14 14
   └─ .app-shell         纵向 flex，height:100%
      ├─ header.topbar                    ← Z1 顶栏（旧版：招聘网站 / 打分模式 / 敏感库 / 右组）
      │   （**无** nav.page-tabs：二级导航页签已按用户裁决删除，见 §7-K4 / §9 Q-1）
      └─ main.app-body     横向 flex（新）  ← Z3
         ├─ section.dashboard             ← Z4 主列（单列卡片流，**顺序照旧版**）
         │   ├─ Banner/ErrorBar           ← Z4.1 错误条 / 提示条（贴主列顶部，非全宽）
         │   ├─ card.scrape-card          ← Z4.2 抓取控制条
         │   ├─ card.job-list             ← Z4.3 投递列表
         │   └─ card.log-standalone      ← Z4.5 决策日志 / 运行日志（card-standalone 变体；
         │                                   卡头含「决策记录」按钮 → 打开决策记录弹窗）
         └─ aside.chat-rail  (sticky)     ← Z5 Boss 消息栏（跨页常驻；底部**无**消息发送框）
```

**Z4.4 已删除**（用户裁决）：原「求职期望 / 打分试算」两张折叠卡不再存在——求职期望并入抓取控制条的筛选条件面板（§3.2.1），打分试算移除。

**层级顺序是契约**：Z4.1 → Z4.3 的先后顺序就是旧版视觉顺序；插进新卡片只能追加在 Z4.3 之后、Z4.5 之前。

**最小宽度是契约**：整页 `min-width: 1200px`。**不存在 < 1200px 的降级布局**——窄于 1200px 时保持 1200px 宽度并出现横向滚动条，右栏不折叠、列表不降列（用户裁决）。

### 2.2 与旧版截图的逐块对应

| 旧版区块（截图） | 新界面节点 | 保留项 | 视觉变化 |
|---|---|---|---|
| 原生标题栏「AI 求职助手」 | 窗口标题（`tauri.conf.json`） | 标题文字逐字不变 | 无（属后端配置，不在本规格改动面） |
| 顶栏 | `header.topbar` | 招聘网站分组、平台三卡、打分模式分段、敏感库入口、累计 token | 白底 → `--glass-1` 玻璃；品牌标保留在**最左**（现有资产） |
| 错误条 | `ErrorBar`（`components/`） | 文案来源为后端错误串；自动消失 10s | 贴边 strip → 圆角玻璃条；补「关闭」按钮 |
| 抓取控制条 | `card.scrape-card` | 筛选开关 / 开始 / 停止 / 摘要 / 上限 / 门槛（6 个元素与顺序） | 卡片玻璃；粉色危险按钮改 `--bad` |
| 投递列表 | `card.job-list` | 标题 / 三药丸 / 共 N 条 / 刷新 / 三个排序键 / 批量行 / 空态 | 卡片玻璃；补「查看全部」开关（现有资产） |
| 右栏 Boss 消息 | `aside.chat-rail` | 标题「Boss 消息」+ 计数 + 搜索框 + 错误行 + 引导行 | 改为玻璃栏；宽度见 §3.5（裁决值，非测量值） |
| 日志卡片 | `card.log-standalone` | 标题「决策日志 / 运行日志」+「实时」+ 行格式 `HH:mm:ss 消息` | 玻璃标题带 + 等宽行 |

### 2.3 布局结论：两栏怎么做（**本文最重要的一节**）

#### 事实

- 旧版：顶栏 → 「主列 + 右栏」两栏；主列内再纵向堆「控制条 / 列表 / 日志」三张卡。
- 现有 `frontend-vue/styles/layout.css:13-20` 的 `.app-shell` 是 `flex-direction: column`；`frontend-vue/styles/layout.css:78-89` 的 `.dashboard` 是 `flex-direction: column` 的单列卡片流。**没有任何两列规则。**

#### 结论（做）

**保留两栏结构，右栏作为 `.dashboard` 的兄弟第二列，用 `position: sticky` 常驻。**

```text
main.app-body {
  display: flex; flex-direction: row; align-items: stretch;
  flex: 1 1 auto; min-height: 0; gap: var(--rail-gap);   /* --rail-gap: 12px */
  padding-top: 10px;                                      /* 与现有 .dashboard 相同 */
}
section.dashboard { flex: 1 1 auto; min-width: 0; }        /* 现有规则原样保留 */
aside.chat-rail {
  flex: 0 0 var(--rail-w);                                 /* --rail-w: 300px */
  position: sticky; top: 0; align-self: stretch;           /* stretch：吃满 .app-body 的高度 */
  max-height: 100%;                                        /* 与主列可视高一致 */
  display: flex; flex-direction: column; min-height: 0;
  /* 视觉：--glass-1 薄玻璃，与 .topbar 同厚（apple-design §12「材质重量编码层级」：
     右栏是结构分区，用薄一级；主列卡片用 --glass-2） */
}
```

> **为什么是 `align-self: stretch` 而不是 `height: 100%`**：`.app-body` 的高度由 `flex:1` 撑出，`height:100%` 在 `stretch` 之外的一堆嵌套里会与 `max-height` 打架；`stretch`（flex 默认）+ `max-height:100%` 的组合只需要一条规则就拿到「与主列同高、内部滚动、页面不滚」。
>
> **为什么不做 `height: 100vh` 的固定右栏**：截图未取证「右栏是整页等高还是只与列表卡等高」（证据笔记 §4 末条），`stretch` 是两种读法下都成立的最小假设；若日后取证为「只与列表卡等高」，只需把 `aside` 从 `.app-body` 的兄弟改成 `.job-list` 的兄弟，其余规则不动。

#### 为什么不是「改成 grid 三行」或「右栏独立浮层」

| 方案 | 为什么否 |
|---|---|
| A. 整页 `grid-template-columns: 1fr 300px`，左列放全部卡片 | 会毁掉 `.dashboard.is-fill-page` 的「列表撑满」：撑满要求 `.job-list` 是 `.dashboard` 的**直接子项**才能拿到 `flex:1`（`frontend-vue/styles/layout.css:107-110`） |
| B. 右栏 `position: fixed` 浮在右边 | 会盖住主列内容，且必须手工补偿 `padding-right`；与「最小宽度 1200px」的固定布局语义冲突 |
| C. 把列表/日志卡按旧版拆成三张独立卡 | ✅ **部分采纳**：控制条与日志原本就是仓库里的独立卡，只需调整顺序（§3.2），无结构代价 |
| **D. 右栏作为 `.dashboard` 的兄弟列 + sticky（采纳）** | 主列卡片仍是 `.dashboard` 的直接 flex 子项，`is-fill-page` 与 C17 动画类全部不动；右栏在窄窗只需一次 `order` 翻转即可落到主列下方 |

#### 宽度规则（必须按此实现；**最小宽度 1200px，无降级分支**）

| 宽度 | 右栏 | 主列列表 |
|---|---|---|
| ≥ 1280px | 300px，sticky | 撑满、内部滚动（`is-fill-page`） |
| 1200–1279px | 260px，sticky | 撑满、内部滚动 |
| < 1200px | **不降级**：根容器 `min-width: 1200px`，右栏仍是 260px sticky，页面出现横向滚动条 | 撑满、内部滚动 |

> **原「<1100 右栏落底」「<900 折叠成浮球」两档已按用户裁决整条删除**：整页 `min-width: 1200px`，这两个视口在真实运行中不可达，保留它们只会让实现者在不可达分支上做无效降级。
> `button.rail-fab`（`chat-rail-fab`）随之废止，不得出现在实现里。

#### 对 C1–C17 的冲击

**零。** 契约层（`invoke` 形状、CSS 顺序、令牌、`data-testid`）一条都不用改；`AppShell` 只多一个 `aside` 插槽（见 §3.1），而 `frontend-vue/AGENTS.md` §2.4 的「外壳不 import 页面、由 `App.vue` 给插槽」结构正好容纳它。

---

## 3. 分区布局约束

> 所有尺寸单位 px；除特别说明外，这里给的是**内容尺寸**，玻璃盒子的描边/阴影按令牌叠加。
> 命名约定：`.app-body` / `.chat-rail` / `.rail-*` 是**新增类名**，其余全部沿用现有 `styles/` 的类（不得新造同义类）。

### 3.1 Z1 顶栏 `header.topbar`

```text
display: flex; align-items: center; gap: 10px; flex-wrap: wrap;   /* 现有 frontend-vue/styles/layout.css:24-36 */
```

子项顺序（左 → 右），**顺序是契约**：

| # | 子项 | 类名 | 约束 | 来源 |
|---|---|---|---|---|
| 1 | 品牌 | `.brand` + `.brand-mark` | 24×24 圆角方块 + 「AI 求职助手」，14px/650 | 现有资产（截图无此项，按现有保留） |
> **#2 构建标签已按用户裁决删除**：`.build-tag`（`<shell> · v<ver> · <stage>`）与 `vue · v2.0.0 · preview` 一律不进新界面。全文命中必须为 0。

| # | 子项 | 类名 | 约束 | 来源 |
|---|---|---|---|---|
| 1 | 品牌 | `.brand` + `.brand-mark` | 24×24 圆角方块 + 「AI 求职助手」，14px/650 | 现有资产（截图无此项，按现有保留） |
| 2 | 平台分组标签 | `.topbar-label`（新） | 「招聘网站」，12px `--text-3`，前置 1px × 16 竖向 `--hairline` | 旧版 A |
| 3 | 平台卡 ×3 | `.platform-card`（新） | 见 3.1.1 | 旧版 A |
| 4 | 打分模式分段 | `.segmented`（新，结构同 `.tabs`） | 见 3.1.2 | 旧版 A |
| 5 | 敏感库 | `button.btn.btn-sm`（`data-testid="sensitive-open"`） | 文案「敏感库」；点击打开**敏感库浮层**（三列卡片） | 旧版 A + 现有 |
| 6 | 弹性空隙 | `.spacer` | `flex: 1 1 auto` | 现有 |
| 7 | 累计 token | `.rail-stat` 同族 `.token-total`（新） | 「累计 」+ `<b>103.8K</b>` + 「 token」；11.5px；`tabular-nums`。**< 1360px 折叠为单条 chip**（只显总量），为右侧按钮让位 | 旧版 A |
| 8 | 配置 | `button.btn.btn-sm`（`data-testid="config-open"`） | 文案「配置」；点击打开**配置浮层**（§3.6） | 旧版 A + 用户裁决 |
| 9 | 主题切换 | `button.btn.btn-sm.btn-ghost`（`theme-toggle`） | 「☾ 深色」/「☀ 浅色」 | 现有 |

> **#8 / #9 永不被压缩**：按钮组 `flex: 0 0 auto`，token 区 `flex-shrink: 1`。**1200px 这一档顶栏任何元素不得重叠**（用户 bug 反馈：token 与「配置 / 深色」重叠）。
> **满分口径 `.score-spec` 已迁出顶栏**：原挂在二级导航里的「满分 {} / 默认门槛 {}」改挂到 `.card.log-standalone` 卡头（`data-testid="score-spec"` 保留，C10 不受影响）。

> **#4 #5 与 #10 #11 语义重叠**：平台卡/打分模式是旧版；满分口径/大模型 chip 是现有。两者同时显示会重复表达「模型是否可用」。处置见 §7-K3（建议：`.topbar` 只留 `Chip` 用于失败态「大模型未配置」，成功后收起）。

#### 3.1.1 平台卡 `.platform-card`

```text
display: inline-flex; align-items: center; gap: 6px;
height: 30px; padding: 0 10px; border-radius: var(--radius-pill);
background: var(--glass-input); border: 1px solid var(--hairline);
```

| 槽位 | 内容 | 状态 |
|---|---|---|
| 复选框 | 原生 `<input type="checkbox">`，14×14，`accent-color: var(--primary)` | Boss 已选；智联/51job 禁用 |
| SVG 勾 | Boss 卡片独有的绿色勾形（截图 A） | 用 `::after` 或内联 SVG，**不引入图标库** |
| 名称 | 「Boss直聘」/「智联招聘」/「51job」 | `--text-1` / 禁用时 `--text-3` |
| 状态点 | 8×8 圆，`--bad`（未登录）/ `--good`（已登录） | 仅 Boss 卡 |
| 后缀 | 无（Boss）/「未接入」12px `--text-3`（另两家） | — |
| 动作 | 「登录」`button.btn.btn-sm.btn-primary`，高 24 | 仅 Boss 卡，未登录时显示 |

- **禁用态**：整卡 `opacity: .55`，`cursor: not-allowed`，复选框 `disabled`（apple-design §16「反馈四类」——状态而非错误）。
- **宽度**：三卡不设固定宽，由内容撑开；`flex-wrap` 允许换行（窄窗时整组落到第二行）。
- **不许**把卡片做成整块可点击（点击目标只有复选框与「登录」按钮），避免误触切换平台。

#### 3.1.2 打分模式分段 `.segmented`

```text
display: inline-flex; padding: 2px; border-radius: var(--radius-sm);
background: var(--glass-input); border: 1px solid var(--hairline);
/* 段按钮复用 .tab 的尺寸：height 24 / padding 0 10 / font 12 */
```

- 两段：「大模型打分」→「规则打分」，**互斥单选**，`role="radiogroup"` + `aria-checked`。
- 选中段：`background: var(--primary-soft); border: 1px solid var(--primary); color: var(--text-1)`，宽度**不随文字变化**（等宽 `min-width: 84px`，避免切换时抖动）。
- 语义为「打分通道选择」，**不是**「是否调用大模型」的即时开关。

### 3.2 Z4 主列 `.dashboard`（单列卡片流，顺序照旧版）

| 序 | 卡片 | 高度策略 | 说明 |
|---|---|---|---|
| 1 | `ErrorBar` / `Banner` | 自适应 | 出现时才占位（现有 `is-fill-page > .app-error { flex: 0 0 auto }`） |
| 2 | `card.scrape-card` | 固定内容高（约 96px 含标题带） | 旧版在最上面；**注意**：现有 `dashboard.rs` 把它排在列表之后（S-5），本次**上移** |
| 3 | `card.job-list` | `flex: 1 1 auto; min-height: 220px` | 撑满（现 `frontend-vue/styles/layout.css:107-110`） |
| 4 | ~~`card`（求职期望）与 `card`（打分试算）~~ | **已删除**（用户裁决） | 求职期望并入筛选条件面板；打分试算移除 |
| 5 | `card.log-standalone` | `flex: 0 0 auto; height: 220px`，内部滚动 | 旧版在最下；`card-standalone` 变体见 §4.2 |

- 卡片间距 `.dashboard { gap: 10px }` 不变。
- `is-fill-page` 仅在 `nav === Dashboard` 时挂上（现有行为，C13 不改）。

### 3.3 Z4.2 抓取控制条 `.scrape-bar`

单行 flex，**顺序是契约**（左 → 右）：

| # | 元素 | 文案（逐字） | 约束 |
|---|---|---|---|
| 1 | 筛选开关 | `筛选条件 ▼` / `收起筛选 ▲` | `.chip.as-btn`，高 24 |
| 2 | 开始 | `开始抓取` / 启动中时 `启动中…` | `.btn.btn-sm.btn-primary`，`disabled = starting \|\| running` |
| 3 | 自动投递 | 见 §3.3.1 | `.btn.btn-sm.btn-ghost`，**仅 ≥1280px 显示** |
| 4 | 停止 | `停止抓取` | `.btn.btn-sm.btn-danger`，`disabled = !running` |
| 5 | 状态摘要 | `空闲 · 已入库 0 条` / `抓取中 · 已入库 N 条 ……` | `.scrape-brief`，`flex: 1 1 120px`，省略号 |
| 6 | 有效 JD 上限 | `有效 JD 上限` [输入] `条` | 输入 52px 宽，`input-num` |
| 7 | 分值门槛 | `分值门槛 ≥` [输入] `分（低于不入库）` | 输入 52px 宽；`max` 与 `placeholder` 来自 `get_scoring_spec` |
| 8 | 尾部工具带 | `已入库 N 条` + 「重新打分」 | `.scrape-tool`，仅 ≥1280px 显示 |

- **与截图的差异**：截图里第 5 项在**左**、第 6/7 项在**右**；本规格改为「顺序编号 1→8 的单一 flex 行 + `#5` 吃掉弹性宽度」。理由：项目现有 `frontend-vue/styles/scrape.css:24-35` 的 `.scrape-brief` 就是为这个顺序写的（注释「进度摘要吃剩下的宽度」），改成截图排布要重写这条规则且会在窄窗产生断行。**文案与元素集合完全照截图，只有「弹性项落在哪个位置」不同**。
- 摘要文案（**逐字，C9 的一部分**）：`{抓取中|空闲} · 已入库 {n} 条 · 已抓 {n} · 低于门槛 {n} · 规则否决 {n}`；`title` = 「抓取进程当前状态」。
- **Tooltip**：`分值门槛 ≥` 输入框 `title="低于该分值的岗位不入库"`；`有效 JD 上限` `title="一次抓取最多落库多少条有效 JD"`。截图里的「分（低于不入库）」是**可见文字**，不是 tooltip —— 两者都要有。

#### 3.3.1 自动投递按钮（`AutoBar`）

- 位置：紧贴「开始抓取」右边（现有用户裁决，见 `features/auto/panel.rs:25` 注释）。
- 文案（照现有）：`开始自动投递` / `暂停自动投递` / `继续自动投递`（以 `AutoBar` 实际字符串为准，本规格不改其文案）。
- 窄窗（<1280px）时随第 3/8 项一起进**溢出菜单** `⋯`（见 §9 Q-6）。

### 3.4 Z4.3 投递列表 `card.job-list`

四层，自上而下：

```text
job-list
├─ .job-tabs-row   (h 32)
│  ├─ .job-tabs  [投递列表标题] [未投递 N] [已投递 N] [不投递 N] [共 N 条] [查看全部]
│  └─ .job-sorts [不加前缀] [加入时间] [薪资] [分值 ↓] [刷新]
├─ .job-batch      (h 30，浅底，圆角 --radius-sm)
├─ .job-grid       (flex:1，内部滚动；三列 grid)
└─ table.sr-only   (C8 保活锚点，必须保留)
```

| 元素 | 位置 | 文案（逐字） |
|---|---|---|
| 卡标题 | 工具行最左 | `投递列表`（13px/620） |
| 页签 1 | — | `未投递` + 计数 |
| 页签 2 | — | `已投递` + 计数 |
| 页签 3 | — | `不投递` + 计数 |
| 总数 | 三药丸右侧，12px `--text-3` | `共 {n} 条` |
| 刷新 | 总数右侧 | `刷新` |
| 提示前缀 | — | **删除**「排序」（截图无；三个 pill 自带语义） |
| 排序键 | 右侧 | `加入时间` / `薪资` / `分值`（当前键后缀 ` ↓` 或 ` ↑`，箭头**固定 10px 宽**防抖） |
| 全选 | 批量行最左 | `全选本页` |
| 已选 | — | `已选 {n} 条` |
| 可见总数 | — | `列表共 {n} 条` |
| 批量提示 | 批量行最右（未选 ≥2 时） | `勾选 2 条以上可批量标记；单条直接用行内标签` |
| 批量按钮（≥2） | 批量行最右 | `不投递（{n}）`（`.btn-danger`）、`允许投递（{n}）`（`.btn-ghost`） |
| 空态 | 网格居中 | `暂无岗位` |

- **药丸尺寸**：高 26、`padding: 0 10`、`font-size: 12`、`border-radius: var(--radius-pill)`；选中态 `background: var(--primary-soft); border-color: var(--primary)`，**不写死绿色**（现 `frontend-vue/styles/jobs.css:47-54` 的注释已经是这么定的）。
- **「共 N 条」与页签计数的口径**：页签计数来自 `job_tab_counts`（每 5s 轮询，`TAB_POLL_MS`）；「共 N 条」来自当前列表。**截图里两者矛盾（0 vs 18/14），这是旧版缺陷，不得复刻**——见 §9 Q-4。
- **空态文案取舍**：截图是 `暂无岗位`；仓库现有一长串引导文案。本规格取「`暂无岗位`」为主标题 + 第二行 `--text-3` 引导「先展开筛选条件并点击开始抓取」，见 §9 Q-5。
- **网格**：`.job-grid` 三列（`grid-template-columns: repeat(3, minmax(0,1fr)); gap: 10px`），窄窗依次退到 2 列（<1180）与 1 列（<900）。卡片字段不在本次截图取证范围，沿用现有 `JobCard`（D-5）。

### 3.5 Z5 右栏 Boss 消息 `aside.chat-rail`

```text
chat-rail
├─ .rail-head   (h 44)     [Boss 消息] [计数 chip]
├─ .rail-search (h 32)     搜索框
├─ .rail-body   (flex:1)   会话列表 / 当前线程（内部滚动）
└─ .rail-foot   (h auto)   输入框 + 发送
```

| 元素 | 文案 | 约束 |
|---|---|---|
| 标题 | `Boss 消息` | 13px/620 `--text-1` |
| 计数 | `{n}` | `.chip` 12px，`tabular-nums`；0 时仍显示（截图 A） |
| 搜索 | placeholder `搜索 HR / 公司 / 消息` | 32px 高、`--glass-input`、`--radius-pill` |
| 错误行 | 后端原始错误串（截图 `Failed to fetch`） | `--bad`，12px；**新界面应换成**「读取消息失败：{e}」，见 §9 Q-7 |
| 引导行 | `确认左上角已登录 Boss 应…`（截断：推断为「…聘」） | `--text-3` 12px |
| 空态 | 会话列表空态文案 `没有会话`（C 级，见 §9 Q-7） | — |

- **宽度**：`--rail-w: 300px`（≥1280）/ `260px`（1200–1279）。截图真实宽度 **未取证**（证据笔记 D-1），300 是设计裁决不是测量值。
- **高度**：`position: sticky; top: 0; align-self: stretch; max-height: 100%`；自身 `overflow: hidden`，内部 `.rail-body` 滚动。
- **不许**给右栏设不透明背景（红线 3 / C5）；用 `--glass-1`（比主列卡片 `--glass-2` 薄一级，K16）。
- **三级展开（用户裁决）**：① 常驻会话列表（宽见上）；② 点 `对话框` → 会话列表向左展开为聊天列表，主列被压缩；③ 单击任意 Boss → 再展开出对话区。三级合计不超过窗口宽 50%。**会话列表底部不设消息发送框**，发送框只在对话区底部。
- **不照搬截图的两点**：① 截图右栏第一个元素比主列卡片低约 22px（证据笔记 C-9），新界面**对齐同一起点**（K15）；② 截图右栏底 `#f4f6f9` 与页面底同色，新界面用 `--glass-1` 靠材质层级区分（K16）。

### 3.6 Z4.5 日志卡片 `.card.card-standalone`

- **新增变体 `.card-standalone`**：去掉 `.card-head` / `.card-body` 的左右内边距，改由头部与列表各自控制，使标题带可从卡片左内边距一直铺到右内边距（截图里标题与列表**左对齐到同一条基线**，而现有 `.card-head` 的 `padding: 11px 16px 0` 与 `.card-body` 的 `10px 16px 14px` 会让头部与正文对齐）。
- 标题行：`决策日志 / 运行日志`（13px/620）+ `.log-live`（5px 呼吸点 + `实时`）。
- 列表：等宽 `--font-mono` 11.5px，行格式 `HH:mm:ss` + 空格 + 消息（**HH:mm:ss，C14 的秒级**，截图 A 级已见 `15:19:54`）。
- 高度：220px，内部滚动，新消息到来时贴底（`apple-design` §1：反馈要持续，不能只在结束时跳一下）。

---

## 4. 组件清单

> 逐字文案表见 §4.1。类名沿用 `frontend-vue/styles/` 的既有命名，新增的只有 `.segmented`、`.platform-card`、`.topbar-label`、`.token-total`、`.tb-sep`、`.rail-*`、`.app-body`、`.card-standalone`。

| # | 组件 | 现有来源 | 规格要点 |
|---|---|---|---|
| C-BTN | `Button` | `frontend-vue/styles/controls.css:14-110` `.btn` / `-primary` / `-danger` / `-warn` / `-ghost` / `-sm` | 高 24（sm）/ 28（默认）；`active` 时 `transform: scale(.97)`（apple-design §1）；`disabled` 时 `opacity:.45; cursor:not-allowed` 且**不触发** `active` 缩放 |
| C-CHIP | `Chip` | `frontend-vue/styles/controls.css:340-384` | 高 20；`-primary` / `-accent` / `-warn` / `-bad`；可 `as-btn` |
| C-TAB | `Tab` | `frontend-vue/styles/controls.css:262-317` | 顶栏导航 26 高；列表药丸 26 高、`--radius-pill`；**两者高度相同**、圆角不同 |
| C-SEG | `Segmented`（新） | 结构同 `.tabs` | 单选；等宽段；`aria-checked` |
| C-PLAT | `PlatformCard`（新） | — | 见 §3.1.1；禁用态不透明 `.55` |
| C-FIELD | `Field` | 现有 `components/Field` | 标签 12px `--text-2`；输入 `input` 32 高；`input-num` 52 宽、居中、`tabular-nums` |
| C-CARD | `Card` | `frontend-vue/styles/layout.css:130-198` | 玻璃 `--glass-2` + 渐变高光边；标题 13/620；副标题 11.5 `--text-3` |
| C-CARD-S | `CardStandalone`（新变体） | `.card` 去掉 padding 的 head/body | 见 §3.6 |
| C-LIST | `JobList` | `jobs.css` | 四层结构见 §3.4 |
| C-JOBCARD | `JobCard` | `jobs.css` / `cards.rs` | 字段照现有（D-5 未取证）；分数方块 `tabular-nums` |
| C-BATCH | 批量条 | `frontend-vue/styles/jobs.css:135` | 高 30，`--glass-0` 底 |
| C-EMPTY | `EmptyState` | `frontend-vue/styles/layout.css:225-231` | 主标题 13px `--text-3` + 可选次行 12px；居中；**不画插图** |
| C-ERR | `ErrorBar` | `frontend-vue/styles/controls.css:495` | `--bad-soft` 底 + `--bad` 1px 边 + 左侧 3px 竖条；10s 自动消失；`×` 关闭（`aria-label="关闭"`） |
| C-BANNER | `Banner` | `frontend-vue/styles/controls.css:508` | 提示/迁移横幅；带可选动作按钮 |
| C-STAT | `Stat` | `frontend-vue/styles/controls.css:444` | 标签 11px + 值 13/600；5 个进度胶囊（C9） |
| C-LOGLIVE | `.log-live` | `frontend-vue/styles/pages.css:626-633` | 5px 圆点 + `实时`；动效见 §5.3 |
| C-RAIL | `ChatRail`（新） | — | 见 §3.5；外壳不 import `features/chat`（R1），由 `App.vue` 通过 `aside` 插槽注入 |
| C-TIP | Tooltip | 原生 `title` | **不引入 tooltip 库**；截图里的提示都是长按可读的句子，原生 `title` 足够 |

---

### 4.1 逐字文案表（照抄，不许润色）

> 「来源」列：**S** = 截图里逐字可见（A 级）；**R** = 仓库现有字面量（`frontend-vue/src/**/*.rs`）；**N** = 本规格新拟（无截图依据，等 §9 拍板）。任何一行改动都算改契约，需同步 `data-testid` 清单（C8）。

| 位置 | 文案（逐字） | 来源 | 备注 |
|---|---|---|---|
| 窗口标题 | `AI 求职助手` | S | `tauri.conf.json` 的 `title`，本规格不改 |
| 品牌 | `AI 求职助手` | R | 顶栏 `.brand` |
| 顶栏分组标签 | `招聘网站` | S | |
| 平台 1 名称 | `Boss直聘` | S | 注意「直聘」前**无空格** |
| 平台 1 动作 | `登录` | S | 仅未登录显示 |
| 平台 1 已登录动作 | `退出` | N | §9 Q-3 |
| 平台 2 名称 | `智联招聘` | S | |
| 平台 3 名称 | `51job` | S | 小写 `job` |
| 未接入后缀 | `未接入` | S | 智联 / 51job |
| 打分模式·左段 | `大模型打分` | S | |
| 打分模式·右段 | `规则打分` | S | 截图选中 |
| 敏感库按钮 | `敏感库` | S | |
| 累计 token | `累计 ` + `{103.8K}` + ` token` | S | 数字加粗，空格位置照抄 |
| 满分口径 | `满分 {n} / 默认门槛 {n}` | R | C10 |
| 大模型 chip | `大模型 {model}` | R | |
| 大模型缺省 chip | `大模型未配置` | R | |
| 主题切换 | `☾ 深色` / `☀ 浅色` | R | |
| 错误条 | `连接后端失败：{e}` | N | 替换旧版裸 `Failed to fetch`，§9 Q-7 |
| 错误条关闭 | `关闭` | N | `aria-label` |
| 筛选开关·收起 | `筛选条件 ▼` | S | |
| 筛选开关·展开 | `收起筛选 ▲` | R | |
| 开始 | `开始抓取` | S | |
| 开始·进行中 | `启动中…` | R | 省略号是单字符 `…` |
| 停止 | `停止抓取` | S | |
| 自动投递 | `开始自动投递` / `暂停自动投递` / `继续自动投递` | R | 以 `AutoBar` 实际字面量为准 |
| 摘要·空闲 | `空闲 · 已入库 {n} 条` | S | 「·」两侧各一空格 |
| 摘要·完整 | `{抓取中\|空闲} · 已入库 {n} 条 · 已抓 {n} · 低于门槛 {n} · 规则否决 {n}` | R | C9 |
| 上限标签 | `有效 JD 上限` | S | `JD` 全大写 |
| 上限后缀 | `条` | S | |
| 上限 tooltip | `一次抓取最多落库多少条有效 JD` | N | 原生 `title` |
| 门槛标签 | `分值门槛 ≥` | S | `≥` 前后各一空格 |
| 门槛后缀 | `分（低于不入库）` | S | 全角括号 |
| 门槛 tooltip | `低于该分值的岗位不入库` | N | 原生 `title` |
| 已入库尾巴 | `已入库 {n} 条` | R | |
| 重新打分 | `重新打分` | R | |
| 列表标题 | `投递列表` | S | |
| 页签 1 | `未投递` + `{n}` | S | |
| 页签 2 | `已投递` + `{n}` | S | |
| 页签 3 | `不投递` + `{n}` | S | |
| 页签 4 | `全部` + `{n}` | R | 截图无此页签（当时用「共 N 条」表达），保留现有 |
| 总数 | `共 {n} 条` | S | |
| 刷新 | `刷新` | S | |
| 排序 1 | `加入时间` | S | |
| 排序 2 | `薪资` | S | |
| 排序 3 | `分值` + ` ↓` / ` ↑` | S | 箭头后置 |
| 全选 | `全选本页` | S | |
| 已选 | `已选 {n} 条` | S | |
| 列表可见总数 | `列表共 {n} 条` | S | |
| 批量提示 | `勾选 2 条以上可批量标记；单条直接用行内标签` | S | 全角分号 |
| 批量·不投递 | `不投递（{n}）` | R | 全角括号 |
| 批量·允许 | `允许投递（{n}）` | R | |
| 空态主标题 | `暂无岗位` | S | |
| 空态次行 | `先展开筛选条件并点击开始抓取` | N | §9 Q-5 |
| 抓取进度空态 | `尚无事件。点击「开始抓取」后这里会滚动显示进度。` | R | `feed` |
| 右栏标题 | `Boss 消息` | S | `Boss` 与「消息」间一空格 |
| 右栏搜索占位 | `搜索 HR / 公司 / 消息` | S | 斜杠两侧各一空格 |
| 右栏错误行 | `读取消息失败：{e}` | N | §9 Q-7 |
| 右栏引导行 | `确认左上角已登录 Boss 直聘` | N | 截图截断在 `确认左上角已登录 Boss 应…`，`直聘` 是**推断补全**（§9 Q-7） |
| 右栏会话空态 | `没有会话` | N | §9 Q-7 |
| 日志卡标题 | `决策日志 / 运行日志` | S | 斜杠两侧各一空格 |
| 日志实时标 | `实时` | S | 前置呼吸点 |
| 日志行 | `{HH:mm:ss} {消息}` | S | C14；截图实例 `15:19:54 运行时已就绪 · 数据目录=… 库=…` |
| 日志卡头按钮 | `决策记录` | S | 打开决策记录弹窗（用户裁决） |
| 决策记录弹窗标题 | `决策记录` | S | 弹窗右上 `关闭`；字段 时间 / 环节 / 结论 / 理由 |
| ~~悬浮球~~ | ~~`Boss`~~ | — | **已废止**（最小宽度 1200px，无窄屏降级） |

## 5. 交互与状态

### 5.1 状态矩阵

| 维度 | 取值 | 视觉表达 | 数据来源 |
|---|---|---|---|
| 平台登录态 | 未登录 / 已登录 / 未接入 | 状态点 `--bad` / `--good` / 无点 + `opacity .55`；「登录」按钮仅未登录时出现 | `login/status`（当前**未接线**，见 §5.4） |
| 抓取态 | 空闲 / 启动中 / 抓取中 / 已结束 | 开始按钮禁用逻辑；摘要首词 `空闲`/`抓取中`；`.scrape-brief` 颜色不变（靠文字） | `scrape_status` 事件 |
| 分页页签 | 未投递 / 已投递 / 不投递 / 全部 | 药丸选中态；切页签**清空勾选** | `list_jobs` + 本地 |
| 排序 | 加入时间 / 薪资 / 分值 × 升/降 | 当前键后缀 `↑`/`↓`；箭头位固定 10px | 本地 |
| 批量勾选 | 0 / 1 / ≥2 | ≥2 时提示文案**替换**为两个按钮 | 本地 |
| 连通性 | 正常 / 后端报错 | `ErrorBar` 出现且 10s 自动消失；右栏独立错误行 | `invoke` 抛错 |

### 5.2 关键交互规则

1. **勾选与页签**：切换页签、改变排序、刷新都会重拉列表；**页签切换必须清空勾选**（现有 `reset_checked` 语义），排序与刷新**保留**勾选（只重排不换集合）。
2. **批量 vs 行内**：`已选 ≥ 2` 才出现批量按钮；单条操作用卡片行内标签（`不投递` / `允许投递` / `√`），**两套入口不得同时出现**（截图提示文字本身就是这条规则）。
3. **全选本页**：只作用于当前可见页，不是全库；`aria-checked="mixed"` 部分选中时用混合态。
4. **门槛与上限**：输入框 `change` 触底后即时提交；门槛走 `set_limit`（**永不进 `set_app_config`**，C11）；失败时输入框**回滚**到上一次成功值并在 `ErrorBar` 说明原因。
5. **页签计数**：每 5s 只刷三个数字，不刷列表（避免丢勾选）；失败静默，不弹错。
6. **右栏与主列解耦**：右栏的消息发送/撤回/简历不触发主列刷新；主列勾选不随右栏刷新丢失。
7. **浮层互斥**：敏感库浮层与岗位详情抽屉同一时刻只开一个（现有规则，ARCH-003 §1.3）。
8. **键盘**：所有按钮可 Tab 到达，焦点环 `outline: 2px solid var(--primary); outline-offset: 2px`；页签组支持 ←/→ 切换。

### 5.3 动效映射（apple-design / liquid-glass 的落地）

| 交互 | 规则 | 依据 |
|---|---|---|
| 按钮按下 | 高亮在 `pointerdown` 生效，`scale(.97)`，100ms | apple-design §1 |
| 浮层进入/退出 | 沿**同一路径**进出；从触发元素的方向展开（`transform-origin` 对齐触发按钮） | apple-design §7 |
| 抽屉 | `damping .8 / response .3`；**可中途抓回**，从当前值开始 | apple-design §3/§4 |
| 列表刷新 | 整列 `list-fade`（C17），不做逐项位移 | C17 |
| 新行插入 | `row-in`（C17） | C17 |
| token 数字变化 | `token-bump`（C17） | C17 |
| 「实时」指示点 | 1.6s 呼吸（透明度 .35→1），**不做位移** | §14「避免慢速循环振荡」 |
| 玻璃进入 | 同时动画 `backdrop-filter` 的模糊量与 `scale`，读作「材质到位」而不是淡入 | apple-design §12 |
| 减少动效 | `prefers-reduced-motion` 全部退化为 ≤200ms 透明度过渡 | skill §14 |
| 减少透明 | `prefers-reduced-transparency`：玻璃退为 `--glass-3` 或近实色，去 `backdrop-filter` | liquid-glass §4 |

> **不做**：拖拽排序、甩动手势、弹簧位移——本界面没有可拖拽对象，引入这些只会增加不可控的动画面。轮询驱动的数值变化用 CSS 过渡，不用弹簧。

### 5.4 已知断点（必须写进实现清单，不是设计遗留）

| 断点 | 现象 | 本规格的态度 |
|---|---|---|
| `login_status` / `login_save` / `logout` 调用点为 0，`login.state` 事件未转发给 WebView | 「登录」按钮无处可点 | 平台卡的「登录」按钮先按 **disabled + `title="登录功能接线中"`** 渲染，接线后一行开关打开；**不得**用假的登录成功态占位 |
| 旧版是 HTTP 后端，本项目是同进程 `invoke` | 不会出现 `Failed to fetch` | 错误条文案改为「连接后端失败：{e}」；右栏错误行同理（§9 Q-7） |
| 旧版右栏消息在**所有页面**（C-1） | 本项目 `chat` 是独立页 | 两栏方案下右栏天然常驻；`chat` 整页保留为「展开」视图（§9 Q-2） |

---

## 6. 设计令牌映射（旧版观感 → 现有 Liquid Glass）

**规则：旧版的任何具体颜色值都不进代码；一律映射到 `styles/tokens.css` 的令牌。** 下表是「旧版看到什么 → 用哪个令牌」。

| 旧版元素 | 旧版取值（B 级） | 新界面令牌 | 备注 |
|---|---|---|---|
| 页面底 | `#f4f6f9` | 无（透明，窗口亚克力透上来） | 红线 3 / C5 |
| 卡片底 | `#ffffff` | `--glass-2` + `.card::before` 渐变高光边 | 必须有 ```backdrop-filter``` |
| 顶栏 / 右栏 | `#ffffff` / `#f4f6f9` | `--glass-1`（比卡片薄一级） | apple-design §12「材质重量编码层级」 |
| 主按钮 | `#69b984` | `--primary` / `--primary-strong` 渐变 | `.btn-primary` |
| 危险按钮 | `#e99fae`（浅粉） | `--bad` | 粉色饱和度不足，危险语义不清；**改** |
| 错误条 | `#fbf1f2` + 深红字 | `--bad-soft` 底 + `--bad` 文字 | |
| 文本主色 | `#1f2937` | `--text-1` | |
| 次级文字 | `#9ca3af` | `--text-2` / `--text-3` | |
| 描边 | `#e4e8f0` | `--hairline` / `--hairline-strong` | |
| 数字（计数、token） | 普通字体 | `font-variant-numeric: tabular-nums` | 数字跳动时不抖 |
| 圆角 | 约 8 / 16 / pill | `--radius-sm`(10) / `--radius`(16) / `--radius-pill` | |
| 间距 | 卡片间 0–14 | `.dashboard { gap: 10px }`（现有） | |

**判定口径（逐条回答「照旧 vs 采用」）：**

- **必须照旧（信息架构层）**：区块集合与顺序；每个区块的字段集合；页签三枚与语义；按钮集合与顺序；批量规则（≥2）；排序键三项；右栏作为常驻第二栏；日志行格式；**全部文案逐字**（§5 表）。
- **采用现有 Liquid Glass（视觉层）**：所有底色、模糊、描边、阴影、圆角、动效、字号阶梯、深浅双主题；按钮/chip/tab 的材质；错误条的软化处理。
- **两边都不照搬的**：截图的颜色值（只用令牌）；截图的 0 间隙卡片堆叠（用 `gap:10px`）；截图的分页矛盾计数（§9 Q-4）。

---

## 7. 冲突清单与建议（旧版 ↔ 现有 CSS 体系 ↔ 技能）

| # | 冲突 | 冲突双方 | 建议 | 理由 |
|---|---|---|---|---|
| K1 | 主列卡片数量与顺序 | 旧版 3 块（控制条/列表/日志）；现有 6 块（列表/日志/抓取/期望/试算/抽屉） | **主列 3 块**（控制条 → 列表 → 日志）；「求职期望」并入筛选条件面板、「打分试算」**移除**（用户裁决） | 截图是 IA 真源；求职期望是筛选条件的一部分，不该单独占卡 |
| K2 | 控制条内序 | 截图：摘要左、上限右；现有 CSS：摘要吃弹性宽度在中间偏右 | **保持现有 flex 顺序**，只对齐元素集合与文案 | 改排布要重写 `.scrape-bar` 与其 1180px 断点，收益只是像素位置 |
| K3 | 顶栏信息重叠 | 「打分模式分段」vs「大模型 chip」vs「满分口径」 | Chip 仅在**未配置**时出现；满分口径移到日志卡头（`data-testid="score-spec"`） | 三者讲的是同一件事，顶栏放不下且窄窗必换行 |
| K4 | 顶栏导航页签 | 截图无；现有四页签 | ~~保留四页签~~ → **已按用户裁决整条删除**：不设 `.page-tabs`、不设 `概览 / Boss 消息 / 配置 / 决策记录` 页签、不设「满分 100 / 默认门槛 70」徽标。配置入口改为顶栏 `配置` 按钮（浮层），日志入口为底部日志卡，决策记录为日志卡头的弹窗 | 用户明确裁决「移除」；旧单页工作台即唯一主视图 |
| K5 | 「刷新」按钮归属 | 截图在工具行最右（排序之后）；现有在页签行 | 放工具行最右端，`.btn.btn-sm.btn-ghost` | 跟截图；在排序组之后再追加一个不需要改排序逻辑 |
| K6 | 「排序」前缀文字 | 截图无；现有有 `.job-sort-label` | **删除**该字面量 | 三个 pill 语义自明；省 22px 宽度 |
| K7 | 计数口径矛盾 | 截图 0 vs 18/14 | 「共 N 条」= 当前页签总数（与页签数字同源） | 矛盾来自旧版两套数据源；新界面一律以 `job_tab_counts` 为页签数、以列表响应为分页数 |
| K8 | 危险色 | 截图浅粉；令牌 `--bad: #f85149` | 用令牌 | 浅粉在浅色主题下与 `--bad-soft` 对比不足 |
| K9 | 日志卡与日志页 | 截图「决策日志 / 运行日志」合一；现有一页两视图 | 卡内**合并成一条流**（按时间交错）；卡头右侧提供 `决策记录` 按钮，点击打开**决策记录弹窗**（时间 / 环节 / 结论 / 理由） | 截图是一行标题，不是页签；合并后首页就能看到两类记录；决策记录改为弹窗是用户裁决 |
| K10 | 右栏是否常驻 | 截图常驻；现有 chat 是页 | **常驻**（sticky 第二列），chat 页作展开视图 | 截图 C-1 级推断（证据笔记 §2.1）；两栏是本规格核心裁决 |
| K11 | 空态文案 | 截图「暂无岗位」；现有长文案 | 主「暂无岗位」+ 次行引导 | 截图负责主标题、现有负责可操作性 |
| K12 | 第二/三层玻璃 | 技能主张真折射；项目只完成第一层 | **本次只用第一层** | ARCH-003 §7 第 2 条已裁决「先不接」，本规格不推翻 |
| K13 | 手势类动效 | apple-design 大篇幅讲拖拽/甩动 | 不适用 | 无可拖拽对象（§5.3 脚注） |
| K14 | 字体 | 截图系统中文；令牌 `--font-sans` 含 Microsoft YaHei | 用令牌 | 一致 |
| K15 | 右栏上边距 | 截图右栏第一个元素在 y≈198（搜索框顶 238），主列卡片从 y=176 起 —— 两者错位约 22px（证据笔记 C-9） | **不照旧版的错位**：右栏与主列**同一起点**（`.app-body { padding-top: 10px }`），内部用 `.rail-head { margin-top: 12px }` 补呼吸感 | 22px 的错位在新体系里会读作「右栏没对齐」，是缺陷不是设计 |
| K16 | 右栏底色 | 截图右栏采样 `#f4f6f9`，与页面底**同色**（证据笔记 §1.6 勘误） | 右栏用 `--glass-1` 玻璃，比主列卡片薄一级 | 旧版右栏靠「同色无卡片」区分，新体系靠「材质层级」区分；不引入新的分区底色 |

---

## 8. 与后端路由的映射

### 8.1 旧版 33 条 HTTP 路由 → 新界面归属

> 旧版路由用于确认**信息架构确实存在这些能力**，不是新界面的调用方式。新界面一律走 Tauri `invoke`（C1–C3）。「新命令」一列只填能确认存在的对应命令名；标注 `未确认` 的表示需要由前端编码阶段从 `backend-tauri/src/commands_*.rs` 反查。

| # | 旧版路由 | 旧版用途（从路由名推出） | 新界面落点 | 新命令 |
|---|---|---|---|---|
| 1 | `/api/health` | 存活探针 | 无独立 UI；失败 → `ErrorBar` | `app_info`（未确认，按 §9 Q-7 文案） |
| 2 | `/api/start` | 开始抓取 | 控制条 #2 开始 | `scrape_start` |
| 3 | `/api/stop` | 停止抓取 | 控制条 #4 停止 | `scrape_stop`（未确认命名） |
| 4 | `/api/pause` | 暂停 | 控制条 #3 自动投递 | `auto_greet_pause`（未确认映射） |
| 5 | `/api/resume` | 继续 | 同上 | `auto_greet_resume`（未确认） |
| 6 | `/api/status` | 抓取状态 | 控制条 #5 摘要 | `scrape_status` 事件 + `get_scrape_status`（未确认） |
| 7 | `/api/limit` | 上限/门槛 | 控制条 #6 #7 | `set_limit` |
| 8 | `/api/expectations` | 求职期望 | 主列 #4 求职期望卡 | `get_expectations` / `set_expectations`（未确认） |
| 9 | `/api/platforms` | 平台列表 | 顶栏平台三卡 | `list_platforms`（未确认） |
| 10 | `/api/login/start` | 发起扫码登录 | 平台卡「登录」 | `login_start`（**已定义未接线**） |
| 11 | `/api/login/status` | 登录态轮询 | 平台卡状态点 | `login_status`（未接线） |
| 12 | `/api/login/save` | 保存登录态 | 登录完成回调 | `login_save`（未接线） |
| 13 | `/api/login/logout` | 退出登录 | 平台卡（已登录态需补「退出」入口，§9 Q-3） | `logout`（未接线） |
| 14 | `/api/jobs` | 列表 | 列表卡全部 | `list_jobs`（包 `args`） |
| 15 | `/api/jobs/reject` | 标不投递 | 行内「不投递」/ 批量 | `set_job_flag`（包 `args`） |
| 16 | `/api/jobs/deliver` | 标已投递 | 行内「√」 | `set_job_flag` |
| 17 | `/api/jobs/allow` | 放回待投递 | 行内「允许投递」 | `set_job_flag` |
| 18 | `/api/sensitive` | 敏感库列表 | 敏感库浮层 | `list_sensitive`（未确认） |
| 19 | `/api/sensitive/clear` | 清空 | 浮层「清空」 | `clear_sensitive`（未确认） |
| 20 | `/api/sensitive/delete` | 移出条目 | 浮层「移出所选」 | `delete_sensitive`（未确认） |
| 21 | `/api/logs` | 运行日志 | 日志卡 | `list_logs`（闭区间游标，§9 Q-8） |
| 22 | `/api/chat/friends` | 会话列表 | 右栏 `.rail-body` | `chat_friends` |
| 23 | `/api/chat/messages` | 消息 | 右栏线程 | `chat_messages`（平铺 `uid`） |
| 24 | `/api/chat/send` | 发送 | 右栏输入 | `chat_send` |
| 25 | `/api/auto-greet/next` | 下一个自动打招呼 | 控制条 #3 | `auto_greet_next`（包 `args`） |
| 26 | `/api/auto-greet/one` | 对单条打招呼 | 卡片行内动作 | `auto_greet_one`（未确认） |
| 27 | `/api/greet` | 生成话术 | 打招呼草稿卡 | `greet`（未确认） |
| 28 | `/api/chat/resumes` | 简历列表 | 右栏「发简历」 | `chat_resumes` |
| 29 | `/api/chat/resume` | 发送简历 | 同上 | `chat_resume` |
| 30 | `/api/chat/recall` | 撤回 | 气泡菜单 | `chat_recall` |
| 31 | `/api/chat/open` | 打开会话 | 搜索框选中 | `session_open`（未确认映射） |
| 32 | `/api/config` | 配置读写 | 配置页 | `get_app_config` / `set_app_config` |
| 33 | `/api/rescore` | 重新打分 | 控制条 #8 | `rescrape` / `rescore`（未确认） |

### 8.2 旧库表 → 新界面

| 旧表 | 新界面依赖 | 来源 |
|---|---|---|
| `job_queue` | 列表卡三页签、计数、排序 | `list_jobs` / `job_tab_counts` |
| `sensitive_jobs` | 敏感库浮层、抓取去重 | `list_sensitive` 等 |

---

## 9. 待拍板（10 项，拍完即可冻结）

> Q-1 ~ Q-8 是「旧版有、现有没有」或「截图未取证、需要用户选口径」；Q-9 / Q-10 是本次复核新增。

| # | 问题 | 建议默认 | 影响面 |
|---|---|---|---|
| Q-1 | 顶栏**有没有**页面导航（概览/消息/配置/日志）？截图里没有，但旧版一定得有进配置页的入口，只是可能在被截断的右缘之后 | ~~保留四页签~~ → **用户裁决：删除二级导航**。配置入口 = 顶栏 `配置` 按钮（浮层，§3.6）；日志 = 底部日志卡；决策记录 = 日志卡头弹窗 | 顶栏高度不变（省 34px） |
| Q-2 | Boss 消息：右栏常驻 + 独立页，两处并存；还是只留右栏？ | **用户裁决：只留右栏常驻**，按三级展开——① 常驻会话列表（300px）→ ② 点「对话框」向左展开聊天列表 → ③ 单击某位 Boss 再展开对话区。三级合计不超过窗口宽 50%。**会话列表底部不设消息发送框**，发送框只在对话区 | 路由与状态同步 |
| Q-3 | 已登录的 Boss 卡要不要「退出登录」入口？ | 要，放在卡的 `⋯` 里，不占位 | 需接线 `logout` |
| Q-4 | 页签计数与「共 N 条」口径 | 统一为「当前页签总数」 | 影响 `job_tab_counts` 语义 |
| Q-5 | 空态只有「暂无岗位」还是加引导次行 | 加一行 `--text-3` 引导 | 文案 |
| Q-6 | 窄窗（<1280）时「自动投递 / 重新打分」进 `⋯` 溢出菜单还是换行 | **前提已变**：整页 `min-width: 1200px`，不存在 < 1200px 的降级布局；1200–1360 区间允许换行（不做新组件） | 1200 档控制条高度 |
| Q-7 | 错误文案：沿用后端的 `Failed to fetch` 原样，还是包装成中文 | 包装：`读取消息失败：{e}` / `连接后端失败：{e}` | 错误条与右栏 |
| Q-8 | 日志卡内「决策 + 运行」合并成一条流还是两条 | 合并（K9） | 日志卡数据源 |
| Q-9 | 右栏是「与整页等高」还是「只与列表卡等高、日志卡横贯全宽」？截图在 y=1190 被窗口底缘截断，两种读法都成立（证据笔记 §4 末条） | 暂按「与整页等高」（`align-self: stretch`）；若拍板为「只与列表卡等高」，只移动 `aside` 的父节点 | 决定日志卡是否横贯全宽 |
| Q-10 | 「分值门槛」输入框的 `max` 与「满分」口径来自 `get_scoring_spec`；截图里门槛填 7、上限填 0，**0 是否等于「不限制」**无法从截图确认 | 按后端语义原样显示，前端不做特殊解释 | 输入框边界提示 |

---

## 11. 验收（交给前端编码阶段）

1. **结构**：`.app-body` 两列；`aside.chat-rail` 全程 300px 且 sticky（**任何宽度都不落到主列下方**，因为最小宽度 1200px）；`section.dashboard` 仍是 `flex-direction: column` 且 `is-fill-page` 生效（列表撑满、页面不滚）。
2. **文案**：§5 逐字表里的每一个字符串在 DOM 里都能 `getByText` 找到；`data-testid` 与 `frontend-vue/AGENTS.md` C8 的命名对齐（新增控件同样要带）。
3. **令牌**：`grep` 不到任何截图里的色值（`#69b984` / `#e99fae` / `#fbf1f2` / `#f4f6f9`）；`body` 背景透明；CSS 里无 `//!` / `///`。
4. **状态**：§5.1 六行矩阵每行都有可演示状态；勾选/页签/排序三条规则按 §5.2 落实。
5. **降级**：`prefers-reduced-motion` 与 `prefers-reduced-transparency` 两套媒体查询生效。
6. **宽度**：整页 `min-width: 1200px`；在 1280 / 1440 / 1600 三档各截一张对照 `docs/ui/reference/legacy-00-window-top.png`，偏差只允许出现在 §6 列出的视觉语言项；另在 1150 宽下验证「根容器 ≥ 1200、出现横向滚动条、布局不塌」。
7. **顶栏不重叠**：1200 与 1280 两档，顶栏所有元素 `getBoundingClientRect` 两两求交必须为空集（用户报过 token 与「配置 / 深色」重叠）。
8. **已删元素**：`v2.0.0` / `preview` / `.page-tabs` / `.build-tag` 全文命中为 0；`.score-spec` 只出现在日志卡头。
9. **浮层与弹窗**：敏感库、配置、决策记录三处均为独立浮层（遮罩 + 可关闭）；Boss 会话列表底部不含发送框。
