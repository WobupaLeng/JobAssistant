# SPEC-001-legacy · 聊天编排同屏合并（chat-merge）

> 状态：**READY_FOR_IMPLEMENTATION**（规格冻结，2026-11-23）
> 修订：**Rev 7**（2026-11-23，偏差②按后端真源更正〔DTO 已下发三键，缺口=核心无累计写入路径〕+ 累计写入范围/时机裁决 ADR-005；Rev 6 theme 锚点定谳=`theme-toggle`〔基线 L157，v3 未覆盖〕，见 UI-SPEC §20；Rev 5 占位会话用户裁决；Rev 4 A6 分配宽口径；Rev 3 DOM 顺序定谳；Rev 2 撤导航；Rev 1 首冻）
> 决策依据：[`ADR-004`](../adr/ADR-004-legacy-chat-orchestration.md)（用户裁决「方案 1」：实现向 SPEC 对齐，整页化否决；§9 追加裁决：撤除二级导航）
> 设计真源：[`UI-SPEC-chat-detach-v3`](../ui/UI-SPEC-chat-detach-v3.md)（冻结，一字不动）+ `prototypes/ui-v1/index.html`；其余条款见 `UI-SPEC-legacy-baseline.md`
> 证据基线：HEAD `a4bb3d5`，工作区干净
> 执行 lane：**前端编码**（`frontend-vue/` 单写者）；后续 有稿设计剩余 A–H → 前端打磨 → QA 重锚
> 并行约束：与后端联调 lane 目录隔离可并行；`frontend-vue/` 内部单线程；不触根 §4 🔒单点（`lib.rs` / `tauri.conf.json` / `Cargo.toml` / `schema.rs` / `protocol.rs` / `sidecar.py`）

---

## Problem Statement

用户要「边看岗位边聊」：Boss 列表默认常驻，点任意 Boss 就地展开对话框、压缩投递列表到约 50%。现实现是聊天整页替换——概览卸载、右栏只存在于聊天页、展开态列数 −1 的 CSS 成死代码、打招呼靠切页，与冻结规格 R1/R3/§1/裁决 X 冲突，已触发熔断。另：Vue 仍保留二级导航页签，而原型「用户裁决 5」已将其移除（用户本次裁决照原型 1:1 撤除）。

## Solution

实现向冻结规格对齐：概览常挂 + Boss 右栏同屏常驻 + 对话框就地展开（`.app-body` flex 子项、`position: static`、宽 `clamp(600px, 50vw, 80vw)`）+ 展开态列数 −1 + 顶栏合并单行 + **二级导航撤除（入口保留）**。组件与几何已就位，改造集中在编排提升；后端零改动、零新增依赖。

## User Stories

1. 作为求职者，我想在概览页随时看到 Boss 右栏，以便不切页就能发现新消息。
2. 作为求职者，我想点任意 Boss 就地展开对话框，以便投递列表仍留在眼前对照。
3. 作为求职者，我想展开时对话框占窗口约 50%、主列保留约 50%，以便两边都看得清。
4. 作为求职者，我想展开时投递卡片自动减一列，以便卡片不被挤扁。
5. 作为求职者，我想点卡片「打招呼」就地展开并预填草稿（不自动发送），以便确认后再发。
6. 作为求职者，我想用按钮或 Esc 收起对话框且保留选中 Boss，以便随时重开。
7. 作为求职者，我想收起后右栏恢复全宽、再点同一位 Boss 立即重开，以便操作连贯。
8. 作为求职者，我想聊天展开/收起时概览轮询与勾选不中断，以便抓取状态持续可见。
9. 作为求职者，我想自动投递状态条全站唯一且随展开搬进对话框头，以便状态不重复不丢失。
10. 作为求职者，我想顶栏单行承载平台卡/抓取模式/敏感库/token/配置/主题，以便与原型一致。
11. 作为求职者，我想窗口缩放时对话框宽度与字号按公式重算，以便四档几何稳定。
12. 作为求职者，我想 Esc 一次只关最上层（决策记录/配置/抽屉/敏感库优先于对话框），以便不误关。
13. 作为求职者，我想刷新会话列表不改变展开态与选中 Boss，以便阅读不被打断。
14. 作为求职者，我想不再看到单独的导航页签行，Boss 消息/配置/决策记录入口就在右栏、顶栏、日志卡头部，以便界面与原型 1:1。
15. 作为 QA，我想废止 testid 命中 0、新锚点按冻结 §4 终表落地、退役锚点有清单，以便零 Mock 验收可重锚。
16. 作为前端编码，我想组件与几何已就位只改编排，以便最小入侵完成合并。
17. 作为架构，我想 C1–C17 契约断言一条不少，以便零静默回归。

## Implementation Decisions

1. **编排提升**：聊天从「页」降为「壳级常驻层」。概览常挂；`aside.chat-rail` 与 `.chat-detach` 成为 `.app-body` 的 flex 子项，与主列同屏（原型 L555–569：`.app-body` flex row，`dashboard flex:1 1 auto` + `transition: flex-basis 240ms`）。对话框 `position: static`、非浮层、无 `inset`/`z-index` 抬升（A15）。
2. **状态机照冻结 §3**：`chatOpen × activeFriend` 四组合；收起保留 `activeFriend`；再点同一位立即重开；刷新列表不改两态。
3. **几何照 §2.2/§16.6**：`--detach-w = clamp(600px, 50vw, 80vw)` 公式原样、单一来源（canvas 已有，不得回退到行内绑定）、不乘 `--ui-scale`；resize 先 `--ui-scale` 再 `--rail-w`/`--detach-w`；宽度过渡只走 flex，**禁 `translateX`**；`prefers-reduced-motion` 压 200ms。
4. **裁决 X 照 §16.9**：两条 `is-chat-open` 列数规则必须写在断点规则**之后**（顺序敏感，A52 唯一易回归点）；展开态切换无 `transition`；单列下限卡片 `320px × --ui-scale`，过窄允许横向滚动不得隐藏卡片。
5. **greet 链照 §16.5**：`openChatDetach()` → `friendUidForJob`（公司前缀匹配，无匹配插临时会话）→ `pickFriend(uid)` → 预填 `chat-detach-compose-input` + `focus()`，**不自动发送、不换页**；消费现有 greet-draft，但不再跨页传递。
6. **自动投递条照 §4.4/§14.2**：全站唯一一段 DOM，展开搬节点不复制（现 `autoBarLocation` 二选一机制保留）；三态文案/点色/`aria-live` 照裁决 N；队列生命周期随聊天层迁移，保 C16（WebView 挂起定时不触发）。
7. **顶栏单行合并 + 二级导航撤除（用户裁决 5，已裁决非开放项）**：顶栏三段——左平台卡 · 中分段控件 `.nav-mid` · 右配置组 `.tb-tail{margin-left:auto}`（原型 L505–513）；顺序 `scrape-mode-segmented` → `sensitive-open` → `.tb-stats`（token）→ `shell-config-btn`（§11.3）；三处折叠规则不变（<1360 折叠 / 1360–1599 隐藏 `.rail-stat` / ≥1900 全展开）。**撤除** 4 项 NAV、`navTestId`/`pageTestId` 与 `nav-<slug>`/`page-<slug>` 锚点、Vue 侧 `nav-tabs` 残留规则；不保留页签、不做动作化改造。**入口保留**：Boss 消息=右栏、配置=顶栏右侧 `shell-config-btn`、决策记录=**底部日志卡头部新增入口**（开 logs 浮层窗，原型 L1413–1415 裁决 5 原文）。config/logs 仍走浮层窗（三窗固定高乘 scale 不变）；`theme-toggle` 全站恰一处。本条与结构改造同一批 DOM，**归前端编码同一次改动做掉**（Chief 明确），有稿设计不再碰 F 顶栏部分。
8. **testid 照 §4**：新增 4 个、沿用表落地、四个废止 testid 保持 0；退役与新增锚点清单随提交信息列明供 QA 重锚（见下节点名清单）。
9. **分层照模块 AGENTS §2**：AppShell 不 import pages/features；聊天编排住 pages/ 或 features/ 由编码 lane 定，被展示组件消费的视图模型住 `lib_ui/`；FriendList 双前缀（`chat-rail` / `chat-detach-rail`）已支持，复用不复制。
10. **契约面零改动**：48 条命令签名/载荷形状/配置键/sidecar 协议/路由命令均不动（冻结 §8/§16.8 自证）；C1–C17 逐条保住；五段文案、时间判据 `1e11`、日志闭区间去重键不动。
11. **零新增**：不引依赖、不新增字号档位（5 档封顶）、CSS 禁 `//!`/`///` 与注释内 `` `*/` ``；契约序 11 份 CSS 不动，Vue 追加 3 份只在末尾。
12. **ADR 溯源**：改造入口（`App.vue` 或聊天层组件）加一行轻量注释引用 `ADR-004`（只一处，不逐行注释），保证代码↔决策可发现。
13. **page_tick 语义保留**：概览恒不重拉（`entersSideEffect` 判据与订阅同源，C16/ARCH-003 §5.6）；config/logs 浮层打开即刷新。导航退役后该判据不得丢失。
14. **开放实现项**（编码 lane 可细化，默认推荐，突破契约面即熔断回架构）：
    - 聊天层载体：默认把 ChatPage 重构为壳级聊天层组件，保留独立 mount 测试口径，不新建第二套组件；
    - 壳节点：`app-shell-slot` 外层与 `app-root` 内层双层 `.app-shell` 默认顺手收敛为单层，`is-chat-open` 挂唯一壳节点；
    - 容器锚点：概览容器锚点照原型 `shell-dashboard`（L1418），其余容器锚点由编码 lane 定并列入提交清单。

## 验收标准（AC，可量化）

| # | 判据 |
|---|---|
| AC1 | R1/A2：初始右栏可见且参与布局，与概览同屏；DashboardPage 永不因聊天展开/收起、浮层开合而卸载，5s 轮询不中断 |
| AC2 | R2/A1：初始 `chatOpen=false` 且 `.chat-detach` 带 `hidden`；不存在「未点 Boss 就弹出」路径 |
| AC3 | R3/A15：`.chat-detach` computed `position: static`，`.app-body` flex 子项，无 `inset`/`z-index` 抬升；主列宽随展开变化（`getBoundingClientRect` 可测） |
| AC4 | R6/A4/A6：四档 1200/1280/1440/1600 `detachW ≈ 0.5×innerWidth ±2px`；主列**分配宽**（`innerWidth − detachW`，即实测 rect + 左右内衬 + 列间隙）`≥0.49×innerWidth`（口径照原型 `_smoke.mjs` L70–76，见 Rev 4 裁决与 UI-SPEC §18）；源码含 `Math.max(innerWidth * 0.5, 600)` 与 80vw 上限；`--detach-w` 单一来源、不乘 scale |
| AC5 | R4/R5/A7/A8：点任意 Boss 展开+选中；收起保留 `activeFriend`；再点同一位立即重开；Esc 只关一层（决策记录/配置/抽屉/敏感库优先） |
| AC6 | 裁决 X/A52：展开列数=收起−1（<1600：2→1；≥1600：3→2）；`is-chat-open` 规则源码位置晚于 ≤1599 断点规则；展开态无 `transition`；单列卡片下限 `320px×--ui-scale` |
| AC7 | §16.5 + §19 用户裁决：greet 链=就地展开+好友匹配，`navigate('chat')` 切页调用点=0；**匹配会话**：预填 compose + focus，不自动发送（A48 适用此情形）；**占位会话**（`待打招呼 · 公司`）：走「生成打招呼」面板，**不预填输入框、不要求 focus**，`isSendableFriend` 语义不变（Rev 5 用户裁决，推翻 Rev 4 裁决 2） |
| AC8 | §4.4/裁决 N：`auto-apply-bar` 全站唯一 DOM，展开搬进对话框头（搬节点不复制）；三态文案/点色/`aria-live` 照 §14.2 |
| AC9 | **用户裁决 5（撤导航）**：NAV 4 项、`navTestId`/`pageTestId`、`nav-<slug>`/`page-<slug>` 锚点、Vue 侧 `nav-tabs` 残留命中 0；三入口保留可用（右栏 `friend-<uid>`、顶栏 `shell-config-btn`、日志卡头部决策记录入口）；config/logs 仍浮层；`page_tick` 语义保留 |
| AC10 | §11.3/顶栏三段：单行 46px；顺序 segmented → sensitive-open → tb-stats → shell-config-btn；`.tb-tail{margin-left:auto}`；`theme-toggle` 全站恰一处；三处折叠规则不变 |
| AC11 | testid：四个废止 testid 命中 0；新增 4 个按 §4.1 落地；锚点增删清单在提交信息列明 |
| AC12 | 契约：C1–C17 一条不少；零 invoke/零配置键/零 sidecar/零路由命令改动；五段文案、时间判据 `1e11`、日志闭区间去重键不变；字号 5 档不新增 |
| AC13 | 门禁：根 §5 槽位 A 全项 + `npm run typecheck` + `npx vitest run` 全绿（实跑，不拿静态计数冒充）；改 CSS 后 `npm run build` 并数 `dist/assets/*.css` 选择器；槽位 C 产物工作区外冒烟 |
| AC14 | 文档：HANDOFF §1 口径修正 + 锚点清单 + ADR-004 溯源注释（入口一处）落盘 |
| AC15 | DOM 顺序唯一标准：`.app-body` 子项 = dashboard → chat-rail → chat-detach（原型 L1731/1734/1773，UI-SPEC §17 定谳）；A17 体内左→右不变；不得以 §2.1 树序翻转 |

## DOM 顺序定谳（Rev 3，2026-11-23，Chief 指派）

- **争议**：冻结 SPEC §2.1/§2.4 树序（dashboard→detach→rail）vs 原型实际 DOM（dashboard→rail→detach，L1731/1734/1773）。
- **裁决：原型顺序为唯一标准**。§2.1 树为逻辑示意；§2.4 等式对可见子集成立。证据（已核）：rail 与 detach **互斥可见**——展开时 rail 立即 `display:none`（原型 L1132），收起时 `closeChatDetach()` 同步置 `hidden`（L2454–2464），无同屏瞬间；原型 CSS 无 `order`、无 rail/detach 兄弟选择器；A17 只约束对话框体内左→右；`_smoke.mjs` 无 rail-vs-detach 顺序断言。相对顺序在布局/Tab/读屏上不可观察，以可执行原型为准。
- **落地**：M1 `b2b60f3` 已按此顺序，`chat-page.test.ts:137` 断言有效，**零返工**；M2 像素轮不翻改（有稿设计已确认）。澄清按冻结规格追加惯例落 [`UI-SPEC-chat-detach-v3`](../ui/UI-SPEC-chat-detach-v3.md) §17（架构澄清，非用户裁决，不改写既有结论）。

## M2 范围调整（有稿设计结论，Chief 指派，Rev 3 记录）

1. **ExpectRow 删除**：原型右栏结构=状态条→标题行→搜索→会话列表，四档基线实证无期望行；期望读取已由概览筛选面板「读取期望职位」承载。ChatPage 的 ExpectRow 引用与 `loadExpectations` 消费点退役（api 封装保留，契约面不动）；`chat-expect-*` testid 退役，列入 QA 重锚清单。
2. **chat-note / auto-note 保留为瞬态提示**：`v-if` 非空才渲染，默认态与原型像素一致；错误反馈依赖它（K9 错误必须可读），不删除；位置维持右栏末尾、小字样式，M2 收像素。
3. **顶栏三段像素归 M2**：按冻结 §11.3 + 原型 L505–513（单行 46px、顺序、三处折叠、`.tb-tail{margin-left:auto}`）。

## 槽位 C 取证裁决（Rev 4，2026-11-23，前端编码上报、存量架构设计裁定）

### 裁决 1 · A6 测量口径＝分配宽，非实测 rect

- **争议**：1440 档展开态主列实测 rect 680 = 0.472×innerWidth，低于 AC4「≥0.49」。
- **证据**：原型 `_smoke.mjs` L70–76 以 `mainShare = (vw − detachW) / vw` 断言 ≥0.49——**分配口径**；`.app-outer` 左右内衬 14×2（原型 L241）+ 列间隙 12 属主列之外的 chrome：680+28+12=720=0.5×1440，合规。L59 注释「主列 ≥ 47%」为陈旧文字，代码阈值即 0.49。
- **裁决**：A6/AC4 按分配宽 ≥0.49×innerWidth 判（等价：rect+内衬+间隙）；§2.3 表值为理想分配。680@1440 **非缺陷**，不降阈值、不改几何；`c4913a2` 的 `:root` 兜底修复不受影响。口径澄清落 UI-SPEC §18。

### 裁决 2（Rev 4 原裁，已被用户裁决推翻——见下节 Rev 5；证据链留痕备查）

- **争议**：greet 无公司匹配时插的占位「待打招呼 · 公司」被 `isSendableFriend` 判不可发送 ⇒ compose 不渲染 ⇒ §16.5「预填 + focus」落不了地（产物实测 `composeInputPresent=false`）。
- **证据**：原型 `renderDetachChat` L2411–2413 `canCompose = !!f && railMode==='default'`——占位是正常 friend，compose 照常渲染；`greetFromJob` L2515–2519 预填+focus；`_tri.mjs` L265–266 断言预填非空；冻结 A48 同要求。Vue `chat-model.ts:297` 按名字前缀挡 compose，属 Leptos 遗产规则，与在后的用户裁决 T（§16.5）冲突——**规格赢**。
- **裁决**：compose 可见性 = 有 activeFriend && 会话列表 default 态（非 loading/error/empty），照原型 L2412 逐字；`isSendableFriend` 退出可见性判定。预填+focus、不自动发送不变。**发送校验留在动作层**：占位 uid 后端不认识，`chat_send` 与 `greet_one` 同走 ChatSend（`commands_greet.rs:189`），发送会拿回错误，走既有错误提示路径（K9 错误必须可读），不得静默吞掉。
- **真实后端缺口披露（非阻塞）**：占位会话在建立真实会话前**没有可用发送路径**（greet_one 并不比 chat_send 多一条通道）。是否补「建立会话」能力（后端新轨）或修订 §16.5（占位走生成打招呼面板、不预填 compose），属用户另裁，本轮不自决、不改冻结规格。
- **路由建议**：Chief 派前端编码 M1 补遗（canTalk 口径改照原型 + 占位 compose/预填用例 + chat-page/chat-contract 断言同步），门禁绿后再进 M2；零后端改动。

## 用户裁决 · 占位会话改规格（Rev 5，2026-11-23，Chief 群内传达）

- **用户裁决（推翻 Rev 4 裁决 2）：改规格，不动代码。**`isSendableFriend` 业务语义保持不变；冻结 §16.5 补一句：**占位会话（`待打招呼 · 公司`）走「生成打招呼」面板，不预填输入框、不要求 focus**。落盘于冻结 UI-SPEC §19 追加裁决（按其冻结追加惯例）。
- **代码零改动**：前端编码现状（占位不渲染 compose、`canTalk` 经 `isSendableFriend` 判定）已合规；Rev 4 的「M1 补遗」路由建议**作废**，前端编码无需为此动代码。
- 匹配会话的 greet 链不变（§16.5：预填 compose + focus、不自动发送；A48 预填断言适用匹配情形）。原型 mock 行为不改，本条例外约束下游真实实现。
- 真实后端缺口披露仍成立：占位 uid 后端不认识，`chat_send` 与 `greet_one` 同走 ChatSend（`commands_greet.rs:189`），发送错误如实冒泡（K9）；是否补「建立会话」能力属用户另裁 / 后端新轨。
- **A6 口径确认（Rev 4 裁决 1 维持，Chief 委派架构定口径）**：按**分配宽** `(innerWidth − detachW) / innerWidth ≥ 0.49` 判，等价「实测 rect + 左右内衬 14×2 + 列间隙 12 ≥ 0.49×innerWidth」，亦即 Chief 所列「720 − 内衬 − 间隙」选项；**不降 0.47**（`_smoke.mjs` L59 注释为陈旧文字，代码 L70–76 阈值即 0.49 分配口径）。前端编码几何断言**无需改动**（`c4913a2` 几何正确）；如任何用例拿实测 rect 直比 0.49，改按本口径。UI-SPEC §18 已澄清。

## QA 重锚清单（点名，M4 执行；以各里程碑提交信息增删清单汇总为准）

| 锚点 / 断言 | 现位置 | 处置 |
|---|---|---|
| `nav-dashboard` / `nav-chat` / `nav-config` / `nav-logs` | `lib_ui/nav.ts` `navTestId` + AppShell 页签 | **退役**（裁决 5），命中 0 |
| `page-dashboard` / `page-chat` / `page-config` / `page-logs` | `nav.ts` `pageTestId` + AppShell `.app-body` | **退役**；概览容器锚点照原型 `shell-dashboard`（L1418），其余按编码 lane 提交清单 |
| `nav.test.ts:32` 四串逐字断言 | `tests/nav.test.ts` | 随 NAV 退役重写 |
| `qa-testids.test.ts:84` `chat-rail` 首次命中路径=`src/pages/ChatPage.vue` | `tests/qa-testids.test.ts` | 改指提升后位置（AppShell.vue 或聊天层组件，以实际落点为准） |
| 源码路径字符串断言 | `tests/chat-contract.test.ts`、`tests/geometry-collect.test.ts` | 随 DOM 提升改路径；`tests/geometry.test.ts` 受波及面由编码 lane 核 |
| 结构性断言 | `tests/chat-page.test.ts`、`tests/dashboard-page.test.ts` | 按新编排改写；独立 mount 口径延续 |
| E2E 导航路径 | QA 零 Mock 脚本 | 页签切页作废 → 右栏点 `friend-<uid>` / 顶栏 `shell-config-btn` / 日志卡头部决策记录入口 |
| 四个废止 testid | 全站 | 保持 0；新增 4 个按冻结 §4.1 |
| `chat-expect-*`（ExpectRow） | ChatPage / ExpectRow.vue | **M2 退役**（有稿设计结论①：期望读取由概览筛选面板承载）；api 封装保留，命中 0 |

聚合规则：M1–M3 每个里程碑提交信息逐条列明锚点增删，QA 按汇总清单重锚并零 Mock 验收。

## 改动文件清单（预估，冻结于 HEAD a4bb3d5；编码 lane 细化后以 HANDOFF 为准）

| 文件 | 改动 |
|---|---|
| `frontend-vue/src/App.vue` | 分发重写：概览常挂 + 聊天层常驻，`chatOpen` 提升为壳级状态；整页分发退役 |
| `frontend-vue/src/pages/ChatPage.vue` | 重构为对话框本体承载（壳级层），右栏 DOM 上移 |
| `frontend-vue/src/pages/DashboardPage.vue` | `onGreet`（L596–601）改就地展开+预填；顶栏/平台卡归位配合 |
| `frontend-vue/src/components/AppShell.vue` | `.app-body` flex 三子项；顶栏单行三段合并；页签与 `showNav` 退役 |
| `frontend-vue/src/components/LogCard.vue` | 日志卡头部新增「决策记录」入口（开 logs 浮层窗，裁决 5） |
| `frontend-vue/src/components/chat/*.vue` | 复用为主：ChatDetach / FriendList / AutoApplyBar / ChatCompose / ChatThread / ExpectRow |
| `frontend-vue/src/lib_ui/nav.ts` | 4 项 NAV 与 `navTestId`/`pageTestId` 退役；`entersSideEffect`/`page_tick` 判据保留（概览恒 false） |
| `frontend-vue/src/lib_ui/canvas.ts`、`use-canvas-scale.ts` | 几何公式已有，核对顺序与单一来源 |
| `frontend-vue/src/features/chat/greet-draft.ts`、`features/auto/queue.ts` | 生命周期随层迁移 |
| `frontend-vue/styles/vue-chat.css`、`vue-dashboard.css`、`vue-pages.css` | 右栏常驻、`is-chat-open` 复活（顺序敏感）、顶栏合并、Vue 侧 `nav-tabs` 残留清理；契约序不动 |
| `frontend-vue/tests/` | 必改 6 文件：`chat-page` / `chat-contract` / `qa-testids` / `geometry-collect` / `nav` / `dashboard-page`（`geometry.test.ts` 波及面核）；契约断言一条不少 |
| `frontend-vue/HANDOFF.md` | 完工后同步现状（含 §1「聊天浮层」口径修正 + 锚点清单） |

**零写入**：`backend-tauri/`、`scraper/`、`docs/ui/`、`prototypes/`（其自身 L356 `nav-tabs` 遗留规则属历史痕迹，**不动**；清理对象是 Vue 侧副本）、`data/`、根 `AGENTS.md`（本次为前端模块内改造，预期不动根 §0 跨模块看板）。

## Testing Decisions

- **好测试口径**：断言外部行为与契约（状态机/几何公式/testid/文案/载荷形状），不断言实现细节；沿用现有 vitest 纯逻辑 + 页面编排 mount 口径，不新增测试框架、不新增依赖。
- **基线**：现有全量用例为基线（HANDOFF 口径 34 文件 565 用例，以现场实跑为准）；结构性断言允许随编排改写，C1–C17 契约断言、五段文案、时间判据、载荷形状守卫**一条不少**。
- **新增/改写覆盖**：`chatOpen × activeFriend` 状态机、收起保留选中、greet 就地链（展开+选中+预填+不发送）、auto-bar 唯一性与搬移、resize 公式顺序、Esc 分层、概览轮询不中断、撤导航后三入口可用。
- **几何/结构取证**：源码断言 + 产物取证（A1–A17/A52 相关子集）；截图链路由 Chief 补（沙箱内 headless 受限，冻结 §9 口径）；槽位 C 产物工作区外冒烟。
- **prior art**：QA 修复轮 qa-* 回归文件、login-{events,model,page} 用例、`tests/commands-payload.test.ts`、原型 `_smoke.mjs` A 系列断言。

## Out of Scope（Non-goals）

SPEC 与原型修订；后端/scraper/三条契约改动；A–H 剩余像素差距（有稿设计 M2，F 顶栏已归 M1）；Liquid Glass L2/L3；动效接线（transitionend/animationend，前端打磨候选）；契约 CSS 160 处颜色字面量收口；真实扫码/抓取端到端（受根 §7.2 Chrome 阻塞，另轨）；`data/*.db` 任何写入。

## 里程碑与顺序（硬约束，以 Chief 群内顺序为准）

1. **M0 架构落文档**：本 Rev 2 + ADR-004 §9 补录，提交。
2. **M1 前端编码**：结构合并 + 顶栏单行合并（F 顶栏归本批）+ 二级导航撤除 → 门禁全绿 → 提交（角色身份，`--only` 精确路径，前后 `git status --porcelain` / `git show --stat HEAD` 核对，锚点增删列明）。
3. **M2 前端编码**（用户裁决改派；有稿设计只出差距清单与设计复核，不写实现）：剩余 A–H 像素项 + F 文案/testid/禁用 CSS（F 顶栏已由 M1 做掉，不再碰）——已收口 `9a60d3f`。
4. **M3 前端打磨**：视觉收尾（动效接线等，先出《微调改动范围》再动手）。
5. **M4 QA**：按汇总重锚清单重锚 + 零 Mock 验收。

M1 期间 `frontend-vue/` 单写者；M2/M3 同样单写者串行，严禁 UI 设计与前端编码/打磨并行。门禁（M1 完工前真实执行）：根 §5 槽位 A 全项（独占 `CARGO_TARGET_DIR=%TEMP%\jt-<lane>`，显式 `$env:TEMP`；后端零改动亦跑，防陈旧产物重放）+ typecheck + vitest 全绿；改 CSS 后 build 数选择器；交付前槽位 C `npx tauri build --no-bundle` + 工作区外产物冒烟。

Chief 当前全局顺序（2026-11-23 派单）：M3 前端打磨（视觉收尾 + 分项 token 展示，DTO 现成不等后端）→ M4 QA 重锚 → 后端累计写入（ADR-005，M4 之后开工、可与 M4 并行）→ 发布。后端累计写入**不属本 SPEC 交付面**（Non-goals「后端改动」仍成立），此处仅路由记录；其范围/时机/验收以 [ADR-005](../adr/ADR-005-legacy-token-accumulation.md) 为唯一真源。

## 回滚方案

仅 `frontend-vue/` 改动：`git revert` M1 提交即整体还原，**回滚范围含二级导航撤除**（nav.ts / 锚点 / tabs 残留与结构改造同一提交）；无数据迁移、无后端联动、无配置键变化。revert 后页签与整页化一并恢复（偏差回到熔断前状态），须重新裁决，不得静默恢复。M2/M3 各自 revert 各自提交。

## 风险与缓解

| # | 风险 | 缓解 |
|---|---|---|
| 1 | 自动投递队列随层迁移漏停/漏起 | `startAutoQueue` 在层 onMounted/onBeforeUnmount 成对；C16 挂起语义用例保住 |
| 2 | greet-draft 改同屏消费后时序变化 | 预填+focus 在 `pickFriend` 之后；用例断言不自动发送 |
| 3 | `is-chat-open` 规则顺序回归（A52） | 源码断言规则位置晚于断点规则 |
| 4 | 概览常挂后轮询/勾选/滚动保持 | 5s tick 不中断用例；概览恒不接 `page_tick` 判据不变 |
| 5 | testid 重锚遗漏 | 提交信息列明增删锚点清单，QA 按点名清单 + 冻结 §4 终表核 |
| 6 | 顶栏合并撞 `theme-toggle` 唯一性 | 合并后全站恰一处，用例守 |
| 7 | 独立 mount 测试口径破坏 | 重构后聊天层组件仍可独立 mount，口径延续 |
| 8 | 双层 `.app-shell` 收敛影响既有选择器 | 收敛后全量 grep `.app-shell` 选择器与用例锚点复核 |
| 9 | 撤导航后决策记录入口丢失 | 日志卡头部新增入口为 AC9 硬判据；浮层开合用例覆盖 |
| 10 | 误改冻结原型 | `prototypes/` 零写入（含 L356 历史残留）；清理只针对 Vue 侧副本 |

## Further Notes

- 原型真值坐标（已核）：裁决 5 原文 L1413–1415；顶栏三段 L505–513；`.app-body` flex + dashboard `transition: flex-basis 240ms` L555–569；概览容器 `shell-dashboard` L1418。右栏/对话框真值 L1039–1136（有稿设计已提供前端编码）。
- 四个废止 testid 在 `src/` 命中 0（已核，保持）；`autoBarLocation` 二选一搬移机制已在 ChatPage 落地，提升后保留。
- 本 SPEC 与 ADR-004 同日冻结；实施细化不得突破契约面与 AC，突破即熔断回存量架构设计修订。
- 架构侧门禁快照（Rev 5，HEAD `c4913a2` + M2 在途工作树）：架构 lane 实跑槽位 A 全绿——cargo test 392 过 0 败（1 ignored=已知 sidecar_smoke）、typecheck 0 错、compileall 过。vitest 当轮 567/568，唯一失败为 `geometry.test.ts:137` hex 守卫命中 M2 在途 `.chat-rail::before` mask `#000`（非 HEAD 缺陷、非本文档所致）。**已由前端编码接手处置（M2 改派后）**：整条 `::before` 规则移进 `styles/layout.css`（与 `.card::before` 同宿主，原型 L1055–1057 数值逐字保留）；`.avatar` 顶边内高光 `rgba(255,255,255,.5)`（原型 L1158）新增语义令牌 `--spec-avatar-top` 落 `tokens.css`，vue 系改 `var()`。修后 typecheck 0 错、vitest 34 文件 568 全绿、build 通过（前端编码实跑）。
- **theme 锚点定谳（Rev 6，前端编码提请）**：保持 `theme-toggle`——基线 L157 明文且 v3 未覆盖，v3 全文无主题锚点异名；原型 `shell-theme-toggle`（L1408/states.md L96）为 mock 内部命名，不构成下游契约。config 照 v3 §11.3 `shell-config-btn`（覆盖基线 `config-open`）。M2 现状合规，零返工；UI-SPEC §20 已澄清。
- **M2 已知偏差（前端编码报告项，路由意见供 Chief 派单）**：① ChatThread `.chat-head` 三按钮原型无——属合法功能 additions（C 契约保行为，原型是 mock 不穷举真实功能），**保留**，C8 testid 已在；视觉融合归 M3 打磨裁量。② 顶栏「模型打分/打招呼」分项 stats 未接线——**Rev 7 事实更正**（前端编码核查 `76685f1`，架构复核采纳）：config DTO **已下发** `tokens_total/tokens_greet/tokens_score`（commands_config.rs L58–60/L126–128；前端 types.ts L290–292 类型已在），「后端未下发」不成立；三键属 `CONFIG_READ_ONLY_KEYS`（store_ports.rs L216–223，**只读派生键、非可写键**，set_app_config 静默跳过且有守卫测试 commands_config.rs L385）。真实缺口＝Rust 核心**无累计写入路径**：greet/score 逐次 tokens 只回传不落库，`jobs.token_used` 列仅声明（schema.rs L134）无写入，读数只反映旧 Python 库遗留值。**路由（Chief 已派单）**：分项展示＝纯前端，归 M3 前端打磨（不等后端）；后端累计写入＝ADR-005（M4 之后开工、可与 M4 并行，🔒 六单点零触碰，前端零改动）；**前端不得造数**，后端未接前读数为遗留值/0 属如实展示。③ `<1200` body overflow 裁剪 vs 原型横向滚动条——真偏差（基线 §2.3 根 min-width:1200 经 v3 §1 保留，窄窗应横向滚动不裁剪），建议 M3 修，AC 补「<1200 横向滚动条、无裁剪」。
- M2 收口基线：`9a60d3f`（15 路径 +436/−230，ExpectRow.vue 删除），门禁全绿（typecheck 0、vitest 34 文件 570、build 75.71 kB 选择器可达、cargo exit 0、compileall 0，前端编码实跑；架构 lane 槽位 A 复核同绿）。
- **AC5 缺口跟踪**：敏感库面板不回应 Esc（冻结 §6.1 Esc 分层含敏感库）——Chief 已派 M4 QA 重锚轮一并处理；AC5 不降标，M4 闭合前该子项记未过。
- **偏差②更正与累计写入裁决（Rev 7，Chief 派单、后端编码勘探提请四问）**：裁决全文见 [ADR-005](../adr/ADR-005-legacy-token-accumulation.md)——累计点置 `LlmClient::complete` 边界（domain `TokenMeter` 装饰器，4 接线点按 kind 构造），重试/降级/空内容全计（parse_completion 对空内容仍回 Ok 带真 tokens，llm.rs L173–183；Unparsable 系 domain 后置构造，scoring_llm.rs L368）；`ConfigStore` 加 `bump_tokens`（实现方恰 2 处：admin.rs L449 + commands_greet.rs L238 测试桩）；单事务三键 SQL 侧原子自增（Δtotal=Δgreet+Δscore）；`rescore.rs`/`app_config.rs`/domain 服务/infra llm 零改动；`jobs.token_used` 本批不动（无消费方）；reply.tokens 语义与打分决策顺序/降级语义不变；真实计费口径不另立里程碑。可写范围恰 8 文件（ADR-005 Implementation Plan），🔒 六单点零触碰，前端零改动。
- 架构侧门禁快照（Rev 7，HEAD `c0183ca` + M3 分项展示在途工作树 4 文件）：架构 lane 实跑槽位 A 全绿——cargo test 392 过 0 败（1 ignored=已知 sidecar_smoke）、typecheck 0 错、compileall 过。vitest 归 M3 lane 实跑（前端打磨报告其批次 572 全绿，非本 lane 数字）。