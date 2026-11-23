# UI-SPEC · Boss 列表与对话框（chat-detach）v3

> **状态：`UI_READY`（已冻结）**
> 真源：本文件。覆盖 `docs/ui/UI-SPEC-legacy-baseline.md` 的 §3.5、§9 Q-2、§5.4 三处（见 §1），其余条款继续有效。
> 可交互原型：`prototypes/ui-v1/index.html`（单文件、`file://` 双击即开）。状态机与断言清单见 `prototypes/ui-v1/states.md` §12。
> 本文件不描述进度与版本号；每轮用户裁决以「§N 追加裁决」小节原地追加，不改写既有结论，除非明确写明废止。

---

## 0. 用户裁决（逐字）

> boss列表默认存在，对话框默认隐藏，点击任意boss下左侧展开聊天，允许主动收起和打开
> 展开的列表+聊天框压缩投递列表，达到窗口的50%

拆成 6 条硬条款：

| 编号 | 条款 | 原型落点 |
| --- | --- | --- |
| R1 | Boss 列表默认常驻（右栏 `.chat-rail` 默认可见） | `.app-shell` 无 `is-chat-open` 时 `aside.chat-rail` 参与布局 |
| R2 | 对话框默认隐藏，不存在「未点 Boss 就弹出」的路径 | `.chat-detach` 初始带 `hidden`，`state.chatOpen = false` |
| R3 | 对话框从右栏左缘向左展开，压缩主列；**不是浮层** | `.chat-detach` 是 `.app-body` 的 flex 子项，`position: static` |
| R4 | 点右栏任意 Boss ⇒ 选中该会话 + 自动展开；原两段式（「对话框」按钮 → 三级）全部删除 | `pickFriend(uid)`；`chat-rail-expand-btn` / `chat-detach-back-btn` / `chat-detach-side` / `chat-detach-l2` 全文 0 命中 |
| R5 | 可主动收起（按钮 / `Esc`）；收起后 Boss 列表恢复全宽、`activeFriend` 保留；再点任意 Boss 立即重开 | `closeChatDetach()` / `chat-detach-close-btn`；`Esc` 只关一层 |
| R6 | 展开时「对话框（含左段 Boss 列表）」宽度 = 窗口宽 × 50%，主列至少保留约 50% | `--detach-w = clamp(600px, 50vw, 80vw)` |

**R6 的口径（v3.1 / v3.2 修正后，最终）**：`.chat-detach` **自身**就是那 50vw 的容器，其内部左段是 Boss 列表（`--rail-w`），右段是聊天区。原 v3.0 的「对话框 + 右栏合计 50vw」口径已废止。

---

## 1. 覆盖与废止

| 被覆盖条目 | 位置 | 处理 |
| --- | --- | --- |
| 三级展开（① 常驻会话列表 → ② 点「对话框」→ ③ 点 Boss 出对话区，三级合计 ≤ 50%） | `UI-SPEC-legacy-baseline.md` §3.5（line 293） | **整条废止**。改为两级：Boss 列表常驻 + 对话框（内含 Boss 列表段）。 |
| Q-2「① → ② → ③ 三级展开 / 50% 上限」裁决 | 同文件 §9 Q-2（line 563） | **展开级数部分废止**；「右栏常驻、不整页跳转」的结论继续有效。 |
| 「chat 整页保留为『展开』视图」 | 同文件 §5.4（line 451） | **废止**。展开不再换页，而是在 `.app-body` 内 flex 收窄主列。 |
| 右栏宽度规则（≥1280 为 300、1200–1279 为 260、根容器 `min-width: 1200`） | 同文件 §2.3（line 112–118） | **保留**，并作为 `.chat-detach-rail` 的宽度来源。 |
| 会话列表底部不设发送框 | 同文件 §3.5 后半句 | **保留**，见 §4.5。 |

本文件不改动 `UI-SPEC-legacy-baseline.md` 与 `UI-REFERENCE-legacy-notes.md`（二者只读），覆盖关系只在这里声明。

---

## 2. 结构与几何

### 2.1 结构树

```
body
└─ .app-shell                       [data-theme][data-shot]
   ├─ header.nav                    顶栏（品牌 / 抓取模式分段 / 敏感库 / token / 配置）
   └─ .app-body                     display:flex; --rail-w:300px（≤1279 → 260px）
      ├─ section.dashboard          主列：筛选条 + 投递列表 + 日志卡（被压缩方）
      ├─ .chat-detach               对话框（默认 hidden；展开时 width = --detach-w）
      │  ├─ .chat-detach-head       h44：标题 / 计数 / 自动投递状态条（展开时搬进来）/ 收起按钮
      │  └─ .chat-detach-body       display:flex; flex-direction:row
      │     ├─ div.chat-detach-rail   左段 = Boss 列表（flex:0 0 var(--rail-w)，border-right）
      │     │  ├─ .rail-head        标题 + 计数 + 刷新
      │     │  ├─ .rail-search      搜索输入
      │     │  └─ .rail-slot → .chat-rail-list → .friend × N
      │     └─ div.chat-detach-chat   右段 = 聊天区（flex:1 1 auto; min-width:0）
      │        ├─ .chat-detach-slot → chat-detach-empty 或 thread
      │        ├─ .rail-body → .chat-detach-thread → .chat-bubble × N
      │        └─ .chat-detach-compose   唯一输入框（input + 发送）
      └─ aside.chat-rail            右栏 Boss 列表（默认常驻；展开时 display:none）
         ├─ .auto-apply-bar         投递状态条（收起态时它在最上）
         ├─ .rail-head
         ├─ .rail-search
         └─ .rail-slot → .chat-rail-list → .friend × N
```

**展开态**：`.app-shell.is-chat-open` 时 `aside.chat-rail { display: none }`，`.chat-detach` 显示；它内部自带一份 Boss 列表（左段），因此「右栏消失」不等于「Boss 列表消失」。

**折叠态**：`.chat-detach` 带 `hidden`（`display:none`），`aside.chat-rail` 恢复常驻并把宽度交还给主列。

### 2.2 宽度与高度公式

```js
// syncDetachWidth()：只在 chatOpen 时执行，resize 时重算
var w = Math.round(Math.min(Math.max(innerWidth * 0.5, 600), innerWidth * 0.8));
document.documentElement.style.setProperty('--detach-w', w + 'px');
```

| 量 | 公式 | 说明 |
| --- | --- | --- |
| 对话框总宽 `--detach-w` | `clamp(600px, 50vw, 80vw)` | 下限 600（v10 裁决 U），上限 80vw 防大屏过宽 |
| 展开时主列宽 | `window − --detach-w` | flex 自动分配，不写死 |
| Boss 列表段（左） | `--rail-w` = 300（≥1280）/ 260（1200–1279） | 与右栏同宽，视觉上「列表没动，只是搬到了对话框里」 |
| 聊天区（右） | `--detach-w − --rail-w` | ≥ 320 为舒适下限 |
| 对话框高 | `.app-body` 高度 − `.app-body` 上下 padding | 不设 `100vh`，避免顶栏挤压 |
| `.chat-detach-head` 高 | 44px | 与 `.rail-head` 同高 |

**硬约束**

1. 下限 600、上限 80vw 由公式保证，窄窗不得出现负宽。
2. 主列（投递列表）至少保留约 50% 窗口宽；1200–1920 区间实际为 50.0%，>1920 时主列更宽（对话框封顶 80vw 之前先按 50% 走）。
3. Boss 条目**不得竖排**：`.chat-detach-rail` 与其祖先链必须 `min-width: 0`，`.friend` 必须 `white-space: nowrap`，否则条目会被逐字换行成竖条。
4. 宽度过渡只由 flex 宽度驱动，**禁止** `transform: translateX`（会破坏「同一路径进出、可中途抓回」）。

### 2.3 四档数值表

| 窗口宽 | `--rail-w` | 对话框总宽 | 占窗口 | Boss 列表段 | 聊天区 | 主列 | 主列占比 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1200 | 260 | 600 | 50.0% | 260 | 340 | 600 | 50.0% |
| 1280 | 300 | 640 | 50.0% | 300 | 340 | 640 | 50.0% |
| 1440 | 300 | 720 | 50.0% | 300 | 420 | 720 | 50.0% |
| 1600 | 300 | 800 | 50.0% | 300 | 500 | 800 | 50.0% |

> 主列实测宽未扣约 12px 的列间隙；断言按「主列 ≥ 0.49 × 窗口宽」取，不按整数等号。
> 1200 档为下限 600 恰好等于 50vw，与其它三档同为 50.0%，不存在「下限触发的例外」。

### 2.4 视觉顺序（DOM 顺序 = 阅读顺序 = Tab 顺序）

`.app-body` 内从左到右：

```
主列（投递列表 / 日志，被压缩）
  ←  .chat-detach
        内部左侧：Boss 列表（.chat-detach-rail）
        内部右侧：聊天区（.chat-detach-chat）
  →  aside.chat-rail（折叠态常驻；展开态隐藏）
```

- **非浮层判据**：`.chat-detach` 的 computed `position` 必须是 `static`；它不得有 `inset` / `z-index` 抬升；主列宽必须随其宽度变化（`getBoundingClientRect().width` 可验）。
- Tab 序自然为「主列 → 对话框（先 Boss 列表，再聊天区） → 右栏」，无需 `tabindex` 干预。

---

## 3. 状态机

`state.chatOpen: false | true` × `state.activeFriend: null | uid`，四个组合都有明确形态：

| `chatOpen` | `activeFriend` | 形态 | 备注 |
| --- | --- | --- | --- |
| `false` | `null` | 右栏常驻，对话框隐藏 | **初始态 / `?mode=default-chat`** |
| `false` | `uid` | 右栏常驻（该条目高亮），对话框隐藏 | 收起后保留选中 |
| `true` | `uid` | 对话框展开，左段列表高亮该条目，右段显示会话 | **`?mode=default-chat-open`** |
| `true` | `null` | 理论不出现（`openChatDetach()` 只在 `pickFriend()` 之后调用；若出现则右段显示空态） | 空态文案见 §5 |

**转换**

| 事件 | 前置 | 结果 |
| --- | --- | --- |
| 点任意 Boss（右栏或对话框左段） | `chatOpen=false` | `activeFriend=uid`；`chatOpen=true`；宽度过渡开始 |
| 点任意 Boss | `chatOpen=true` | 仅切 `activeFriend`，宽度不变 |
| 点 `chat-detach-close-btn` | `chatOpen=true` | `chatOpen=false`；`activeFriend` **保留**；右栏恢复全宽 |
| `Esc` | 光标无上层可关对象（决策记录 / 配置 / 抽屉 / 敏感库都已关） | 同上；有上层时只关上层那**一层** |
| `resize` | `chatOpen=true` | 重算 `--detach-w` |
| 刷新会话列表 | 任意 | 不得改变 `chatOpen` 与 `activeFriend` |

---

## 4. `data-testid` 契约

### 4.1 新增

| testid | 含义 |
| --- | --- |
| `chat-detach-close-btn` | 收起对话框（文案「收起对话框 ›」，`title="Esc"`） |
| `chat-detach-compose-input` | 对话框聊天区底部输入框（textarea，占位「输入消息…」） |
| `chat-detach-send-btn` | 发送按钮（文案「发送」） |
| `chat-detach-thread` | 会话消息容器（内含 `.chat-bubble`） |

### 4.2 沿用

`chat-detach` / `chat-detach-head` / `chat-detach-title` / `chat-detach-count` / `chat-detach-sub` / `chat-detach-body` / `chat-detach-slot` / `chat-detach-rail` / `chat-detach-rail-body` / `chat-detach-rail-count` / `chat-detach-rail-refresh-btn` / `chat-detach-rail-search-input` / `chat-detach-rail-slot` / `chat-detach-rail-list` / `chat-detach-empty` / `chat-detach-empty-title` / `chat-detach-compose` / `chat-detach-unread-<uid>` / `chat-rail` / `chat-rail-refresh-btn` / `chat-rail-count` / `chat-rail-list` / `chat-rail-search-input` / `chat-rail-empty` / `chat-rail-error` / `chat-rail-retry-btn` / `chat-rail-guide` / `friend-<uid>` / `friend-name-<uid>` / `friend-msg-<uid>` / `friend-time-<uid>` / `resolve-scan-flag` / `decisions-close-btn`。

### 4.3 绝对不出现（全文命中必须为 0）

| testid | 原用途 | 废止原因 |
| --- | --- | --- |
| `chat-rail-expand-btn` | 右栏「对话框」入口按钮 | 两段式触发器已删，点 Boss 直接展开（R4） |
| `chat-detach-back-btn` | 对话框返回上级 | 没有「上级」了，两级扁平 |
| `chat-detach-side` | 三级里的侧栏段 | 三级结构废止 |
| `chat-detach-l2` | 二级容器 | 同上 |

委托分支、CSS、注释里也不得残留这四者的字符串。

### 4.4 自动投递状态条（v5 起）

| testid | 含义 | 位置 |
| --- | --- | --- |
| `auto-apply-bar` | 状态条容器，`data-mode="idle\|running\|paused"` | 收起态：`aside.chat-rail` 首位；展开态：搬进 `.chat-detach-head` |
| `auto-greet-toggle-btn` | 主按钮：开始自动投递 / 暂停 / 继续 | 状态条内 |
| `auto-greet-stop-btn` | 结束（仅 running/paused 露出） | 状态条内 |
| `auto-apply-state` | 状态文案：未开始投递 / 投递中 / 已暂停投递 | 状态条左端 |
| `auto-apply-pending` / `auto-apply-done` | 待投递 / 已投递 计数 | 状态条右端 |

状态条是**全站唯一一段 DOM**，展开时只搬节点不复制；因此 `auto-greet-toggle-btn` 全站只有一个。

### 4.5 底部无发送框

**Boss 列表（右栏 `.chat-rail` 与对话框左段 `.chat-detach-rail`）底部没有发送框**；输入框只存在于对话框聊天区底部的 `.chat-detach-compose`。收起态下全站没有可用的消息输入框（`.chat-detach-compose` 随对话框 `hidden`）。

---

## 5. 逐字文案

| 位置 | 文案 |
| --- | --- |
| 收起按钮 | `收起对话框 ›` |
| 输入框占位 | `输入消息…` |
| 发送按钮 | `发送` |
| 未选中 Boss（右段） | `在左侧 Boss 列表点一位，开始聊天` |
| 未选中 Boss（副文案） | `点左侧 Boss 列表里的任意一位即可查看聊天` |
| 左段搜索占位 | `搜索 HR / 公司 / 消息` |
| 右栏标题 | `BOSS 消息` |

---

## 6. 交互细则

1. **Esc 分层**：决策记录 → 配置 → 抽屉 / 敏感库 → 对话框。一次 `keydown` 只关最上面那一层；上层都没开时才收起对话框。
2. **不抢焦点**：展开 / 收起不得把焦点踢出当前输入区；只有「打招呼」链路会主动 `focus()` 输入框（那是用户显式意图）。
3. **动效**：同一路径进出（宽度过渡 `flex-basis` + `opacity`），可中途抓回；`prefers-reduced-motion` 时压到 200ms；禁止 `translateX`。
4. **滚动**：对话框内部各段独立滚动（`.chat-detach-scroll`），整页不因展开而出现纵向滚动条。
5. **空态 / 错误态**：两段列表各自有 `empty` / `error` / `retry` 锚点，刷新失败不得清空 `activeFriend`。

---

## 7. 断言清单（A1–A17，几何与结构）

| 编号 | 判据 |
| --- | --- |
| A1 | 初始 `state.chatOpen === false` 且 `.chat-detach` 有 `hidden` |
| A2 | 初始右栏 `aside.chat-rail` 可见（无 `display:none`） |
| A3 | 点任意 `friend-<uid>` ⇒ `chatOpen=true` 且 `activeFriend===uid` |
| A4 | 四档 `detachW ≈ 0.5 × innerWidth ±2px` |
| A5 | 收起后 `detachW === 0` 且 `railW === 300`（≥1280） |
| A6 | 主列 `main ≥ 0.49 × innerWidth` |
| A7 | 收起后 `activeFriend` 不为 `null` |
| A8 | 收起后再点**同一位** Boss ⇒ 重新展开 |
| A9 | 四个废止 testid 命中 0 |
| A10 | `Esc` 只关一层（上层开着时对话框不动） |
| A11 | `frontend-vue/src` 不引用四个废止 testid |
| A12 | 外链协议前缀 / 外链标签 / 样式导入指令命中 0 |
| A13 | 单卡 `job-greet-*` 仅 1 处（v10 起） |
| A14 | `.chat-detach-compose` 只在对话框内，右栏无输入框 |
| A15 | `.chat-detach` computed `position` 为 `static`（非浮层） |
| A16 | Boss 条目不竖排（容器链 `min-width: 0` + `.friend` `white-space: nowrap`） |
| A17 | DOM 里 `.chat-detach-rail`（Boss 列表）索引 < `.chat-detach-chat`（聊天区）索引；分隔线只由 `border-right` 提供 |
---

## 8. 与 `frontend-vue/AGENTS.md` C1–C17 的冲击（v3 本体）

| 条款 | 冲击 |
| --- | --- |
| **C8**（控件全部带 `data-testid`） | **有明确冲击**：新增 4 个（`chat-detach-close-btn` / `-compose-input` / `-send-btn` / `-thread`），删除 4 个（`chat-rail-expand-btn` / `chat-detach-back-btn` / `chat-detach-side` / `chat-detach-l2`）。下游实现必须按 §4 的最终表落地，不得保留旧锚点。 |
| **C17**（动画类名常量） | 零冲击：不新增 keyframes，不加动画类名；宽度过渡走既有 `--dur` / `--ease-glass`，`prefers-reduced-motion` 由既有全局规则降级。 |
| C1–C7 / C9–C16 | 零冲击：零新增 `invoke` 命令、零新增配置键、零路由变化、零 sidecar 协议变化。 |

---

## 9. 截图与人工走查

截图因本机 headless 浏览器在沙箱下崩溃（Edge `network_sandbox.cc:591 拒绝访问 (0x5)` / Chrome `crashpad_client_win.cc:431 OpenProcess 拒绝访问 (0x5)` + `platform_channel.cc:112 FATAL Check failed`，`--no-sandbox` 与独立 profile 均无效）而**放弃**，经用户裁决转为源码级断言 **A53 / A54** 兜底（见 §16.9 与 `_smoke.mjs`）；`shots/shot-18-chat-default-1440x900.png` 与 `shot-19-chat-open-1440x900.png` **不产出**。视觉走查按下面的 7 步人工清单执行（1440×900）：

1. 双击 `prototypes/ui-v1/index.html`（无参数）→ 右栏 Boss 列表常驻，主列与右栏并排，对话框不可见。
2. 打开控制台执行 `window.__PROTO__.chatGeom()` → `open:false, hidden:true, detachW:0, railW:300`。
3. 点右栏第一位 Boss → 对话框从右栏左缘向左展开，左段是 Boss 列表（该条目高亮），右段是聊天区。
4. 再执行 `chatGeom()` → `open:true, detachW:720, chatRailW:300, chatW:420, ratio:0.5, main ≥ 700`。
5. 点「收起对话框 ›」→ 主列恢复全宽，右栏回来且同一条目仍高亮；`chatGeom()` 回到 `detachW:0, railW:300`。
6. 再点同一位 Boss → 直接重新展开（不需要先点任何「对话框」按钮）。
7. 展开状态下按 `Esc` → 收起；先打开「配置」再按 `Esc` → 只关配置，对话框仍在。

---

## 10. v5 追加裁决（投递状态条 / 三态 / 计数；删图 1·2·3·5；图 4 不溢出）

用户逐字：「开始自动投递另起一行， 允许暂停和结束，点击开始后变成两个按钮」「右侧显示待投递数量和已投递数量」「移除图1、2、3、5」「图4 不允许被溢出」。

### 10.1 状态条结构与位置

见 §4.4。DOM 位置：`aside.chat-rail` 之后、`.rail-head` 之前（收起态）；展开时由 `placeAutoBar()` 搬进 `.chat-detach-head` 的 `chat-detach-sub` 之后。**搬节点不复制**。

### 10.2 投递三态

| 态 | `data-mode` | 主按钮文案 | 主按钮类 | `结束` | `aria-pressed` |
| --- | --- | --- | --- | --- | --- |
| 未开始 | `idle` | 开始自动投递 | `btn btn-primary` | 隐藏 | `false` |
| 投递中 | `running` | 暂停 | `btn btn-pause`（警示色） | 显示 | `true` |
| 已暂停 | `paused` | 继续 | `btn btn-primary` | 显示 | `true` |

`.btn-pause` 用 `--warn` 系（深色 `#f0c674` / 浅色 `#92650a`），浅色主题另有覆盖。

### 10.3 计数取数

`auto-apply-pending` = `MOCK_DATA.jobs` 中 `flag==='pending' || !flag` 的条数 + `state.autoPending`；`auto-apply-done` = `flag==='delivered' || flag==='deliver'` 的条数 + `state.autoDone`。数字用 `font-variant-numeric: tabular-nums`，右对齐（`margin-left: auto`）。

### 10.4 抓取三态（裁决 D 延伸）

`scrape-start-btn`：`idle`→「开始抓取」`btn-primary`；点一次先禁用显示「启动中…」650ms 后转 `running`→「暂停抓取」`btn-pause`；再点 `paused`→「继续抓取」`btn-primary`。`scrape-stop-btn` 仅在 running/paused 可点。

### 10.5 删除对象（裁决 G）

`proto-toolbar` 整块 + `.proto*` CSS + `syncProtoToolbar()`、`topbar-label`、`.scrape-tool`（`scrape-rescore-btn` / `scrape-ingested`）、`auto-slot`、旧 `chat-rail-auto-greet-entry-btn` —— 全部命中 0。保留 `?c= / ?l= / ?r=` 自检入口。

### 10.6 图 4 不溢出（配置弹窗）

`.cfg-body { overflow-x: hidden; min-width: 0 }`、`.config-card { flex: 0 0 auto; min-width: 0 }`、`.textarea { box-sizing: border-box; max-width: 100%; overflow-x: hidden }`。

### 10.7 断言（A25–A30 + G1）

A25 主按钮 testid 全局唯一；A26 状态条位置在 `chat-rail` 之后、`.rail-head` 之前；A27 右栏不得再有常驻入口按钮；A28 `.scrape-bar` 内无 auto-slot；A29 计数与 `tabular-nums`；A30 `[data-mode='idle'] … { display: none }`；G1 三件套防溢出。

---

## 11. v6 追加裁决（配置弹窗收敛 / 敏感库挪位 / token 靠近配置）

用户逐字：「移除1、2」「铭感库 放到规则打分的右边。」「token计数放到靠近配置」。

### 11.1 整卡删除

删除 `config-token-section`（`config-token-toggle` / `config-token-usage`）与 `config-advanced-section`（`config-advanced-toggle` 及其子卡 `config-scrape-section` / `config-score-section` / `config-data-section`），共 19 个 testid 从 DOM 消失：`config-token-toggle`、`config-token-usage`、`config-advanced-toggle`、`config-scrape-section`、`config-keywords`、`config-max-jd`、`config-min-score`、`config-llm-prefilter`、`config-score-section`、`config-score-max`、`config-score-min`、`config-data-section`、`config-data-dir`、`config-db-total` 等。

**后端零变更**：这些只是界面控件，`WRITABLE_KEYS` 与命令签名不动；下游实现时若不渲染这些字段，仍必须能读取它们（用默认值兜底）。

### 11.2 保留字段集

`config-llm-section`（`config-api-key` / `-mask` / `config-base-url` / `config-model`）、`config-prompt-section`（`config-prompt-input` / `config-prompt-reset-btn`）、`config-skill-section`（`config-skill-input`）、弹窗骨架（`config-panel` / `config-close-btn` / `config-save-btn` / `config-cancel-btn` / `config-note`）。

### 11.3 顶栏顺序

`scrape-mode-segmented` → **`sensitive-open`** → `.tb-stats`（`token-total` + 模型打分 + 打招呼）→ `shell-config-btn`。三处折叠规则不变（`<1360` 折叠、`1360–1599` 隐藏 `.rail-stat`、`≥1900` 全展开）。

### 11.4 断言（A31–A33 + A21 反转）

A31 索引顺序 `segmented < sensitive-open < tb-stats < shell-config-btn`；A32 `token-total` / `token-total-chip` 各唯一 1 份且落在区间内；A33 `.tb-tail { margin-left: auto }`；A21 **反转**为「19 个已删字段命中 0」。

---

## 12. v7 追加裁决（1920×1080 设计基准 + 超出等比放大）

用户逐字：「窗口默认分辨率1920x1080，调整字体大小，使用标准设计」「超出默认分辨率的等比放大」。

### 12.1 公式与时机

```js
function syncUiScale() {
  var s = Math.min(Math.max(innerWidth / 1920, 1), 1.5);
  s = Math.round(s * 1000) / 1000;
  document.documentElement.style.setProperty('--ui-scale', String(s));
  return s;
}
```

- ≤1920 恒为 1（**不缩小**）；1920→3840 线性放大，封顶 1.5。
- 初始化时调用一次；`resize` 里**先** `syncUiScale()`，**再** `if (state.chatOpen) syncDetachWidth()`。

### 12.2 字号令牌（v8 收敛后的最终 5 档）

| 令牌 | 基准值 | 用途 | 参考文献数 |
| --- | --- | --- | --- |
| `--fs-2xs` | 11px | 说明 / 时间戳 / field-hint / chip / rail-stat | 20 |
| `--fs-xs` | 12px | 次要正文 / 卡片副标题 / 表单标签 / 表格 / 日志行 / 条目摘要 | 31 |
| `--fs-sm` | 13px | **正文基准**：body / 列表正文 / 按钮 / 输入框 / 聊天消息 | 41 |
| `--fs-md` | 15px | 小标题：card-title / rail-title / config-title / 区块标题 | 6 |
| `--fs-lg` | 20px | 页面级标题：.brand / 弹窗大标题 | 2 |

全部定义形如 `--fs-2xs: calc(11px * var(--ui-scale))`；`:root` 注释写明「只允许这 5 个名字，新增第 6 档视为破坏契约」。

| 窗口宽 | `--ui-scale` | 正文实值 |
| --- | --- | --- |
| 1920 | 1 | 13px |
| 2560 | 1.333 | 17.33px |
| 3840 | 1.5 | 19.5px |

### 12.3 比例令牌

`--rail-w`、`--rail-gap`、`--radius-sm/--radius/--radius-lg`、`--blur-0…pop`（明暗两套）、三窗尺寸（决策记录 620 / 配置 640 / 敏感库 720）全部改 `calc(Xpx * var(--ui-scale))`；`--radius-pill: 999px` 不缩放。

### 12.4 不破坏的几何裁决

`--detach-w = clamp(600px, 50vw, 80vw)` 不随 `--ui-scale` 变（50vw 本身已随时窗联动）；三窗固定高度 + 内部滚动不变；三处断点（`≤1279` 右栏 260、`≤1599` 网格 2 列、`<1360` 顶栏折叠）语义不变。

### 12.5 断言（A34–A37）

A34 五档 scale（1200/1440/1920→1、2560→1.333、3840→1.5）且源码含 `Math.min(Math.max(innerWidth / 1920, 1), 1.5)`；A35 三窗高度形状 `min(calc(Xpx * var(--ui-scale)), calc(100vh − k))`（620/640/720）；A36 `--detach-w` 公式原样；A37 硬编码 `font-size: Npx` 命中 0。

---

## 13. v8 追加裁决（字号统一为标准阶梯）

用户逐字：「统一字体大小，按标准设计」。

### 13.1 与 v7 的差异

v7 保留全部手写字号（9 档令牌，10/10.5/11/11.5/12/12.5/13/14/16px），v8 把它们收敛成 5 档标准阶梯（§12.2），并按语义重新归类，**不是机械四舍五入**。

### 13.2 行高与间距

行高三档：正文 1.55、小字 1.5、标题 1.3（例外 `.job-title` 1.4）。只回收两处明显偏挤的间距：`.dl-row` padding `3px 0 → 5px 0`、`.f-textarea` `min-height: 46px → 50px` 且 `padding: 6px 10px → 8px 10px`。

### 13.3 断言（A38–A40）

A38 令牌定义恰好 5 条且基准值依次 11/12/13/15/20px（出现第 6 档即失败）；A39 全站 `var(--fs-*)` 引用集合 ⊆ 这 5 个名字；A40 `--fs-3xs|xxs|base|xl` 残留 0。

### 13.4 下游提示

`frontend-vue/` 实现时**不得新增字号档位**；需要更大/更小的文字时先回来看这 5 档是否真不够，改档位必须同步改本节与 `states.md §12A7.2b`。

---

## 14. v9 追加裁决（删「自动打招呼 关」/ 投递状态描述 / 大模型打分不可选中）

用户逐字：「移除：自动打招呼 关」「自动投递改为描述当前投递状态」「大模型打分不能选中，选中后图1」。

### 14.1 裁决 M：删「自动打招呼 关」

顶栏 `.tb-stats` 里的 `<span class="rail-stat">自动打招呼 <b>关</b></span>` 整条删除；`token-total-chip` 的 `title` 保留三项读数（累计 / 模型打分 / 打招呼）。

### 14.2 裁决 N：状态标签文案

`auto-apply-state` 由恒定「自动投递」改为三态文案 + 点色：

| `data-mode` | 文案 | 点色 |
| --- | --- | --- |
| `idle` | 未开始投递 | 灰（`--text-3`） |
| `running` | 投递中 | 主色（`--primary` + 3px soft 光圈） |
| `paused` | 已暂停投递 | 警示色（`--warn`） |

三个修饰类 `is-idle` / `is-running` / `is-paused` 必须互斥；容器带 `aria-live="polite"`。

### 14.3 裁决 O：大模型打分禁用（图 1 硬回归线）

`scrape-mode-llm-seg` 必须是 `<button class="seg" role="radio" aria-checked="false" aria-disabled="true" disabled title="大模型打分暂未开放，当前只支持规则打分">`：

- CSS：`[disabled] / [aria-disabled='true']` 一律 `opacity: 0.45; cursor: not-allowed; background: transparent; border-color: var(--hairline); color: var(--text-3)`；`aria-checked='true'` / `.is-active` 与之组合时必须透明兜底（图 1 的绿色高亮绝不能出现）。
- JS：`wireRadioGroup` 的 click 处理器 `if (btn.disabled || btn.getAttribute('aria-disabled') === 'true') return;`。

### 14.4 断言（A41–A44）

A41「自动打招呼」命中 0；A42 状态标签三态（`aria-live`、初始文案、三段文案、三色 class、`::before` 点色）；A43 `llm-seg` 标签含 `disabled` + `aria-disabled="true"` + `aria-checked="false"`、不含 `is-active`、有 title，`rule-seg` 为 `aria-checked="true"`；A44 禁用态样式与 JS return 都存在。

---

## 15. v9.1 / v9.2 追加裁决（投递卡片列数断点 / 删除批量标记提示语）

### 15.1 裁决 P（v9.1）：列数断点 = 1600

用户逐字：「小于1600px时，投递卡片缩减为2列」。

```css
.job-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
@media (max-width: 1599px) { .job-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
```

| 窗口宽 | 1200 | 1280 | 1440 | 1599 | 1600 | 1920 |
| --- | --- | --- | --- | --- | --- | --- |
| 列数 | 2 | 2 | 2 | 2 | 3 | 3 |

约束：①全站把 `.job-grid` 降到 2 列的媒体查询**恰好 1 条**且 `max-width: 1599px`；②列数只由 CSS 像素断点决定，不随 `--ui-scale`（2560/3840 仍 3 列）；③≤1279 不得再有重复的 2 列声明。

### 15.2 裁决 Q（v9.2）：删除批量提示语

用户逐字：「移除：勾选 2 条以上可批量标记；单条直接用行内标签」。

- **删除**：`job-batch-hint` 节点、`.batch-hint` 样式、`renderBatch()` 里的 `hint` 相关代码与提示语原文。
- **保留两条途径**：批量条（`job-batch-actions` / `job-batch-reject-btn` / `job-batch-allow-btn`，已选 ≥ 2 时显隐）与单条行内标签（`data-flag`）。显隐规则简化为 `actions.hidden = n < 2`；批量按钮组仍靠 `.batch-actions { margin-left: auto }` 贴右缘。

### 15.3 断言（A45 / A46）

A45：见上表，另断言基础 `.job-grid` 为 `repeat(3, minmax(0, 1fr))`、2 列媒体查询恰好 1 条、规则内不含 `ui-scale`。A46：提示语命中 0、`job-batch-hint` 0、`.batch-hint {` 0、三个批量 testid 仍在、`actions.hidden = n < 2` 存在、`data-flag` 存在、`.batch-actions` 有 `margin-left: auto`。

---

## 16. v10 追加裁决（卡片地址 / 长按多选 / 打招呼跳转 / 对话框下限 600）

用户逐字（多轮收敛，最终形态只认 W2）：

- R：「卡片显示地址 删除不投递和允许投递后的位置」
- S：「默认不出勾选框，长按后被选中，然后可多选」
- T：「打招呼按钮放在卡片上」
- U：「1.把下限降到 600px、2.暂不处理」
- V：「移除卡片中的这个」（= 那颗 `√`）「打招呼 占2行高度，高亮显示」
- W2：「打招呼 在地址那行+分值那行，做成方块」

### 16.1 裁决收敛表

| 裁决 | 首版落点 | 最终状态 |
| --- | --- | --- |
| R | 地址补一行，删「不投递 / 允许投递」两颗行内按钮 | **生效** |
| S | 勾选框默认不渲染，长按 500ms 进入选择模式 | 生效 |
| T | 卡片加「打招呼」，点击展开对话框 | **生效，落点由 W2 决定** |
| V | 删底部那颗 `√`（`data-flag="deliver"`） | 生效（并入 W2 结构） |
| W | 打招呼放底部条、独占 2 行高、主色实底 | **废止**（被 W2 取代） |
| W2 | 打招呼做成方块，跨「地址行 + 分值行」，贴右下角 | **生效（最终）** |
| U | `--detach-w` 下限 640 → 600 | 生效 |

### 16.2 R（修正版）：卡片信息层级与 testid 增删

```
┌ 标题 ................................ 薪资 ┐
│ 公司 · 融资轮次                            │
│ 杭州 · 5-10年 · 分布式 / 高并发            │
│ 杭州市余杭区文一西路 969 号 · 星瀚大厦 12F │ ← 地址行（右侧让出方块宽）
│ [92分]                                     │ ← 分值行（右侧让出方块宽）
└────────────────────────────────────────────┘
```

- **删除**（单条行内按钮，共三颗）：`job-reject-*`（不投递）、`job-allow-*`（允许投递）、`job-deliver-*`（√）。
- **新增**：`job-addr-<tid>`（地址，`title` 存全文，视觉单行省略）、`job-greet-<tid>`（打招呼方块）。
- **保留**：`job-batch-reject-btn` / `job-batch-allow-btn`（批量条）、`job-detail-deliver-btn` / `job-detail-reject-btn`（详情抽屉）、`job-card-<tid>` / `job-check-<tid>` / `score-<tid>`。
- **契约 testid 净减少 3 个**，下游不得再引用。
- 委托保留 `flag === 'reject' || flag === 'deliver'` 判定（详情抽屉仍走它）。

### 16.3 S：选择模式

| 触发 | 行为 |
| --- | --- |
| `pointerdown` 在 `.job-card`（跳过 `button / input / [data-greet]`） | 记 `LP.id/x/y`，500ms 后 `enterSelecting(id)` |
| `pointermove` 位移 > 8px 或 `pointerup / pointerleave / pointercancel` | `lpCancel()` |
| 选择模式内单击卡片 | `toggleCardCheck(id)` |
| 选择模式内点空白 | `exitSelecting()` + `renderList()` |
| 已选数归零 | 自动 `exitSelecting()` |

`.job-check-slot` 默认 `display: none`，`.job-grid.is-selecting` 时 `display: flex`；批量按钮显隐为「已选 ≥ 2」。

### 16.4 W2：打招呼方块

```css
.job-info { display: grid; grid-template-columns: minmax(0, 1fr) auto; grid-template-rows: auto auto; column-gap: 8px; row-gap: 6px; align-items: center; margin-top: auto; }
.job-info .job-addr { grid-column: 1; grid-row: 1; min-width: 0; }
.job-info .score-box { grid-column: 1; grid-row: 2; justify-self: start; }
.job-greet {
  grid-column: 2; grid-row: 1 / span 2; align-self: stretch;
  height: calc(var(--fs-sm) * 1.55 * 2 + 12px);
  min-width: calc(var(--fs-sm) * 4.6);
  aspect-ratio: 1 / 1;
  display: inline-flex; align-items: center; justify-content: center;
  padding: 0 8px; border: 0; border-radius: var(--radius-sm);
  background: var(--primary); color: #fff;
  font-size: var(--fs-sm); font-weight: 620; letter-spacing: 0.02em; white-space: nowrap;
}
```

- 高度 = 两行正文（`--fs-sm × 1.55 × 2`）+ 12px；`--fs-sm` 已含 `--ui-scale`，**不再单独乘 scale**。
- **可读性优先于正方形**：`min-width: calc(var(--fs-sm) * 4.6)` 保证「打招呼」三字横排，`aspect-ratio` 仅在不压缩文字时生效；禁止 `writing-mode` 竖排。
- `border: 0`，hover 只 `filter: brightness(1.08)`，不新增配色 token。
- `.job-foot` 容器整体删除，W 的 `grid-template-columns: auto 1fr` 不得残留。
- 分值是信息不是操作：`.score-box` 不加边框/底色强化。

### 16.5 T：打招呼链路

`greetFromJob(jobId)`：`openChatDetach()` → `friendUidForJob(job)`（按公司前缀匹配，无匹配则插 `uid='g'+job.id` 的临时会话）→ `pickFriend(uid)` → 写 `state.greetDraft` 并填进 `chat-detach-compose-input` + `focus()` → **不自动发送**。点击委托里 `[data-greet]` 必须在 `switch (tid)` 之前的早期分支判定，且 `closest` 包 `try`。

### 16.6 U：对话框宽度下限 600

```js
var w = Math.round(Math.min(Math.max(innerWidth * 0.5, 600), innerWidth * 0.8));
```

| 窗口宽 | 对话框 | 占窗口 | Boss 列表段 | 聊天区 | 主列 | 主列占比 |
| --- | --- | --- | --- | --- | --- | --- |
| 1200 | 600 | 50.0% | 260 | 340 | 600 | 50.0% |
| 1280 | 640 | 50.0% | 300 | 340 | 640 | 50.0% |
| 1440 | 720 | 50.0% | 300 | 420 | 720 | 50.0% |
| 1600 | 800 | 50.0% | 300 | 500 | 800 | 50.0% |

§2.2 / §2.3 里所有 `640px` 下限表述由本节 **600px** 取代（`.config-modal` 的 640px 是弹窗高度，与本节无关）。

### 16.7 断言接力（A47–A51）

| 编号 | 判据 |
| --- | --- |
| A47 | 8 条 mock 岗位 `addr` 非空；`.job-addr` 单行省略三件套；卡片默认不含可见 `job-check-` |
| A48 | 卡片渲染 `job-greet-*` 与 `data-greet`；`greetFromJob()` 含 `openChatDetach()` + `pickFriend(uid)` + `state.greetDraft` + 预填文案 |
| A49 | 源码含 `Math.max(innerWidth * 0.5, 600)`；1200 档 detach 与主列占比都落 49.5%–50.5% |
| A50 | `jobCardHtml()` 内 `job-reject-` / `job-allow-` / `job-deliver-` 各 **0** 次、`job-greet-` **1** 次；三个保留入口仍在；委托 deliver/reject 判定仍在 |
| A51 | `.job-info` 首列 `minmax(0, 1fr)`、次列 `auto`；`.job-greet` 含 `grid-row: 1 / span 2`、`grid-column: 2`、`var(--primary)`、`aspect-ratio: 1 / 1`、`min-width: calc(var(--fs-sm) * 4.6)`、两行正文高度；不带 `btn-sm`；不得残留 `.job-foot { grid-template-columns: auto 1fr }` |

反向验证（已实测）：加回 `√` → A50 FAIL；把 `.job-greet` 挪回底部横排 → A51 FAIL；下限改回 640 → A49 FAIL。行为侧 `_tri.mjs` 覆盖打招呼链路（展开 + 选中 + 预填 + 不发送）。

### 16.8 与 `frontend-vue/AGENTS.md` C1–C17 的冲击

- **C8**：**主动删除 3 个契约 testid**（`job-reject-*` / `job-allow-*` / `job-deliver-*`），新增 2 个（`job-addr-*` / `job-greet-*`）。下游必须同步删掉三颗行内按钮、新增地址与方块，且 `job-greet-*` 必须带 `data-greet="<jobId>"`。**单条三按钮已全部移除，卡片只保留打招呼；标记已投递走详情抽屉，批量走批量条。**
- **C17**：零冲击 —— 长按 500ms 是 JS 定时器不是 CSS 动画；`.job-greet` hover 只改 `filter`；选择模式只切 `.is-selecting` 类名。
- **C1–C7 / C9–C16**：零冲击（无 `invoke`、无配置键、无路由、无 sidecar 协议变化；`addr` 只是 mock 字段）。
- **下游落地提示**：`job-info` 网格与 `.job-greet` 尺寸公式照抄，不要改 flex + 固定 px；`--fs-sm` 已含 `--ui-scale`，再乘一次会双倍放大。

### 16.9 裁决 X：展开态列数 = 收起态列数 − 1

用户逐字：「投递列表在聊天框展开时自动减少1列」。

| 窗口宽 | 收起态 | 展开态（`.app-shell.is-chat-open`） |
| --- | --- | --- |
| < 1600px | 2 列 | **1 列** |
| ≥ 1600px | 3 列 | **2 列** |

```css
.job-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
@media (max-width: 1599px) { .job-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
/* ⚠ 顺序敏感：本块必须写在上面两条之后，否则被 ≤1599 的 2 列覆盖 */
.app-shell.is-chat-open .job-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
@media (max-width: 1599px) {
  .app-shell.is-chat-open .job-grid { grid-template-columns: minmax(0, 1fr); }
  .app-shell.is-chat-open .job-card { min-width: calc(320px * var(--ui-scale)); }
}
```

- **特异性/顺序警告**：两条 `is-chat-open` 规则与断点规则特异性相同，写在前者就被后者覆盖 —— 这是本节唯一容易回归的点。
- 列数只由「窗口断点 + 展开态」决定，**不随 `--ui-scale`** 变化（2560 / 3840 仍为收起 3 / 展开 2）。
- 切换不加过渡动画（`.job-grid` 规则内不得出现 `transition`）。
- 单列下限：卡片最小宽度 `320px × --ui-scale`；过窄允许横向滚动条，**不得**退到 0 列或隐藏卡片。
- `< 1200px` 仍按 1200 基线，展开态同样 −1。

**A52**：收起态 `repeat(3, minmax(0, 1fr))`；`@media (max-width: 1599px)` 收起态 `repeat(2`；`is-chat-open` 为 `repeat(2, minmax(0, 1fr))` 且其源码位置晚于断点规则；`is-chat-open` 的单列声明在 ≤1599 断点内；展开态无 `transition`。打印推断表 `1200 收起2/展开1 · 1440 收起2/展开1 · 1599 收起2/展开1 · 1600 收起3/展开2 · 1920 收起3/展开2`。
反向验证（已实测）：把 `is-chat-open` 整块挪到断点规则之前 → `SMOKE FAIL · A52 展开态规则写在断点规则之前`。

**C1–C17 冲击**：零冲击（纯 CSS 列数规则，无新 testid、无新动画类名、无 invoke/配置变更）。

### 16.10 像素来源口径

长期唯一可对照的像素参照是 `docs/ui/reference/` 下的 11 张裁切图（legacy-00 … legacy-10）。原图所在的临时解包目录已删除，本规格与 `states.md` 不再引用任何已删除路径。

---

## 17. 追加澄清：右栏与对话框的 DOM 顺序定谳（2026-11-23，存量架构设计裁决 · Chief 指派）

> 性质：架构澄清，**非用户裁决**；不改写任何既有结论与 R/A 条款，只定谳 §2.1/§2.4 的读法，消除「同一顺序两份标准」。

**唯一标准：`.app-body` 子项 DOM 顺序 = `section.dashboard` → `aside.chat-rail` → `div.chat-detach`，照原型 HTML（L1731 / L1734 / L1773）。**

1. §2.1 结构树是**逻辑示意**（按角色列：被压缩方 → 展开体 → 常驻右栏），不是字面 DOM 序列。§2.4「DOM 顺序 = 阅读顺序 = Tab 顺序」对**可见子集**成立：收起态 = 主列 → 右栏；展开态 = 主列 → 对话框（体内左段 → 右段）。§2.4 所述「主列 → 对话框 → 右栏」的全可见 Tab 序在 R1/R2 下永不出现。
2. 证据（已核）：右栏与对话框**互斥可见**——展开时 `.is-chat-open .chat-rail { display:none }` 立即生效（L1132）；收起时 `closeChatDetach()` 同步置 `el.hidden = true`（L2454–2464），不存在两者同屏的瞬间。原型 CSS 无 `order` 属性、无 rail/detach 兄弟选择器依赖；A17 只约束**对话框体内**左段先于右段，两种排法均满足；`_smoke.mjs` 无 rail-vs-detach 顺序断言。因此相对 DOM 顺序在布局 / Tab / 读屏上**不可观察**，以可执行原型为准。
3. 下游：`frontend-vue` 照此顺序落地（M1 提交 `b2b60f3` 已按原型顺序，`chat-page.test.ts` 顺序断言有效，**无需返工**）；后续像素轮不得以 §2.1 树序为由翻转 DOM。[`SPEC-001-legacy-chat-merge`](../specs/SPEC-001-legacy-chat-merge.md) Rev 3 同步记录。
