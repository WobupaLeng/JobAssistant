# 设计稿 B · 四态与交互清单

> 对象：`prototypes/ui-v1/index.html`（单文件，双击即开）。
> 基线：`docs/ui/UI-SPEC-legacy-baseline.md`（§2.3 两栏 / §4.1 逐字文案 / §7 K1–K16 / §9 Q1–Q10）、`docs/ui/UI-REFERENCE-legacy-notes.md`。
> 本期状态：`UI_READY`（有稿 + 四态 + 令牌对齐），可直接进入【6/7. 前端编码】。
> **对话框这一节以 [`docs/ui/UI-SPEC-chat-detach-v3.md`](../../docs/ui/UI-SPEC-chat-detach-v3.md) 为冻结真源，本文件的旧「三级展开」表述已作废**（见 §12）。

---

## 1. 页面骨架（照抄旧版信息架构，顺序不动）

```
顶栏        AI 求职助手｜招聘网站｜Boss直聘●登录｜智联招聘 未接入｜51job 未接入
            ｜大模型打分 规则打分｜敏感库 ……………… 累计 103.8K token｜配置｜主题
            （单行不换行；空间不足时统计组先折叠成 chip、再整体裁让，`.tb-tail` 已回到流内，不再绝对定位重叠）
（二级导航已按用户裁决移除：Boss 消息在右栏、配置在顶栏、决策记录从底部日志卡打开）
原型工具条   列表区 Default Loading Empty Error ｜ 右栏 Default Loading Empty Error ｜ 连接 正常 断连
错误条       （仅在存在错误时出现，整宽）
抓取控制条   筛选条件▾（开面板）｜开始抓取｜开始自动投递｜停止抓取｜空闲 · 已入库 N 条 …｜有效 JD 上限[ ]条｜分值门槛≥[ ]分（低于不入库）
主列表卡     投递列表｜未投递 N｜已投递 N｜不投递 N｜共 N 条｜刷新 …｜加入时间｜薪资｜分值↓
                  ☐ 全选本页　已选 N 条　列表共 N 条　…（右侧：已选 ≥ 2 时出现批量按钮组 不投递（N）/允许投递（N）；单条用岗位卡行内标签）
             岗位卡网格（四态）
筛选条件面板 搜索词(chips+输入+读取期望职位)｜地点与翻页｜薪资｜工作经验｜学历｜公司规模｜融资阶段｜行业｜排除关键词
             （9 个分区，12 段内容一次性全展开、禁止折叠分组；每区独立描边卡，只放筛选与期望条件，不含任何配置项）
日志卡       决策日志 / 运行日志｜实时｜满分 100 / 默认门槛 70｜决策记录（按钮）｜等宽时间戳行
             决策记录已改为**独立模态弹窗**（不再贴在日志卡底部）：9 行 × 时间/来源/结论/理由，全百分制
右栏         Boss 消息 N｜刷新｜搜索 HR / 公司 / 消息｜会话列表（四态，永不切成线程）——**无发送框**
浮层         岗位详情抽屉（点岗位卡）、敏感库浮层（独立浮层，三列卡片网格）、配置弹窗（四组）、
             决策记录弹窗、右侧展开对话框（点 Boss ⇒ 从右栏左缘向左展开，「左 Boss 列表 + 右聊天区」，对话框自身 = 50vw）、
             筛选条件面板
```

区块顺序与旧版逐块对应；**新增的只有「原型工具条」**——它是给你审核用的开关，实现时整体删除。

> **用户裁决后的结构（本版已执行）**：
> ① 列表保持网格卡，不回退单列行式列表；
> ② 原「求职期望」独立折叠卡删除，城市 / 岗位 / 薪资三项并入顶栏 `筛选条件 ▾` 面板；
> ③ 原「打分试算」折叠卡整卡删除（含脚本、`data-testid`、状态）；
> ④ `开始自动投递 / 暂停自动投递` 保留不动；
> ⑤ **页面最小宽度锁定 1200px**（用户硬约束，覆盖此前所有窄窗断点要求）；原「1100 保 260 侧栏」的 1099 断点改动同时作废，所有 `<1200px` 降级分支已删除；
> ⑥ 筛选条件面板重做为**完整筛选器**（12 段一次性全展开，禁止折叠分组）；
> ⑦ 敏感库改为**独立浮层**（三列卡片网格，154 条口径）；
> ⑧ 对话框改为**右侧展开、压缩投递列表**，展开后**对话框自身（内含 Boss 列表）= 窗口宽 50%（v3.1）**（原「520px 居中浮层」、「不超过 50%」、v3.0「对话框 + 右栏 = 50%」三种旧口径均作废，以 §12 与 `docs/ui/UI-SPEC-chat-detach-v3.md` 为准）；
> ⑨ 顶栏版本徽标 `vue · v2.0.0 · preview` 删除；
> ⑩ 二级导航（概览 / Boss 消息 / 配置 / 决策记录 + 满分口径）删除，决策记录改从底部日志卡打开。

> **第三轮用户意见（本版已执行）**：
> ① 筛选面板按分区重排：9 个独立描边分区、标题独立成行（不再与 chip 挤在同一行）、区内 chips 与输入框分行、底部四组改「标签在上、控件在下」并把「成立年限」输入框与三个下拉底部对齐；面板自身可滚动、无横向溢出；
> ② ~~对话框**三级展开**：一级 = 右栏「对话框」按钮；二级 = 右侧展开（最大 50% 窗口宽）+ 主列空态占位 + 常驻会话列表，**二级不放输入框**；三级 = 点会话后铺满主列（约 82% 宽），左侧 200px 会话列表常驻可切，右侧聊天区 + 输入框~~ **【已作废，见 §12】** 现为两级：Boss 列表默认常驻 → 点任意 Boss 即展开「左聊天区 + 右 Boss 列表」，与右栏合计 50vw；
> ③ **移除 Boss 列表底部的「输入消息… + 发送」**（右栏不再自带发送框，输入只发生在对话框聊天区底部）；对应 `chat-rail-send-btn` / `chat-rail-compose-input` 与其 click 分支一并删除，无悬挂引用；
> ④ **决策记录改为独立模态弹窗**（居中 860px、带遮罩、Esc 只关一层）；文案统一**百分制**（`匹配度 60 分 · 低于门槛 70 分`、`规则分 88 · 高于门槛 70 分`），日志行也改为 `满分 100 · 门槛 70`；
> ⑤ **配置按钮接线**：顶栏「配置」打开四组配置弹窗（大模型 / 抓取默认参数 / 打分口径 / 数据目录与库概览），字段与逐字文案对齐 `frontend-vue/src/features/config/panel.rs`，键名与默认值来自 `backend-tauri/src/core/domain/app_config.rs`，**零新增字段**；
> ⑥ 修复 1200px 顶栏重叠：`.tb-tail` 由绝对定位回到流内，统计组改为可裁让位的 flex 项，实测 1600/1440/1280/1200/1150 顶栏恒为单行 46px、控件矩形两两不相交。

---

## 2. 四态：列表区（`job-grid`）

切换方式：页内工具条「列表区 Default   Loading   Empty   Error」，或 URL `?mode=light-empty-error`。

| 态 | 视觉 | 逐字文案 | testid |
| --- | --- | --- | --- |
| **Default** | 岗位卡网格（**≥1600 三列、<1600 两列**，随主列宽度自然排布，不做 1 列回退）；分页签计数、批量行、排序键都可用 | 卡片字段：标题、`35-60K`、公司·城市·经验·学历、匹配度、加入时间、行内标记 | `job-card-{id}` / `job-check-{id}` / `job-reject-{id}` |
| **Loading** | 6 张骨架卡（shimmer），列表头计数同步显示 `共 0 条` | 骨架不出文字（屏幕阅读器读 `列表状态 Loading · 共 0 条 · 已选 0 条`） | `job-grid[data-state=loading]`、`job-state-announce` |
| **Empty** | 120px 高留白 → 图标 → 主文案 → 副文案 → 主/次行动 | 主文案 `暂无岗位`（逐字保留）；副文案 `先展开筛选条件并点击开始抓取`；按钮 `开始抓取`、`展开筛选条件` | `job-state-empty`、`job-empty-hint`、`job-empty-start-btn`、`job-empty-filter-btn` |
| **Error** | 红色错误图标 + 中文标题 + 一句人话 + 等宽原始错误（`title` 里可看全，用于报障）+ `重试` | 标题 `岗位列表加载失败`；说明 `这次没有取到数据，可以重试一次。`（断连时改为 `与后端的连接已中断，恢复后会自动重试。`）；原始串 `failed to invoke command list_jobs: sidecar not ready`；按钮 `重试` | `job-state-error`、`job-state-error-title`、`job-state-error-retry-btn` |

## 3. 四态：右栏（`chat-rail-body`）

切换方式：页内工具条「右栏 Default   Loading   Empty   Error」。

| 态 | 视觉 | 逐字文案 | testid |
| --- | --- | --- | --- |
| **Default** | 会话条目（头像+HR·公司+最近一句+时间+未读红点） | 条目文案为演示假数据（`李女士 · 星瀚科技` 等，不进 §4.1） | `chat-rail`、`chat-rail-body`、`chat-friend-{uid}`、`chat-unread-{uid}` |
| **Loading** | 6 条骨架（头像圆 + 两行条），计数徽标显示 `—` | 屏幕阅读器播报同列表区的 `chat-rail-state-announce`（`.sr-only` + `aria-live`） | `chat-rail-state-announce` |
| **Empty** | 居中提示 + 刷新 | 标题 `没有会话`；说明 `点「刷新」或在 Boss 直聘回一条消息`；按钮 `刷新` | `chat-rail-empty`、`chat-rail-empty-title`、`chat-rail-empty-refresh-btn` |
| **Error** | 带红色左描边的错误块（宽度 100%，长错误串自动换行） | `读取消息失败：` + 等宽原始串 `sidecar replied error frame` + `确认左上角已登录 Boss 直聘` + 按钮 `重试` | `chat-rail-error`、`chat-rail-guide`、`chat-rail-retry-btn` |

> 旧版右栏错误是裸 `Failed to fetch` + `确认左上角已登录 Boss 后点「刷新」。`，标题缺失、原始串与建议混在一行。新版拆成「中文标题 + 原始串（代码字体）+ 下一步建议 + 重试」，原始串仍保留以利排障（规格 §7 K9：错误必须对用户可读，同时不丢技术细节）。

## 4. 错误条的两种形态（`app-error`）

| 形态 | 触发 | 文案 |
| --- | --- | --- |
| 连接失败（有诊断线索） | 工具条「连接 断连」或 `?mode=connbad` | 标题 `连接后端失败` · 原始串 `failed to invoke command list_jobs: …` · 建议 `sidecar 没有就绪，日志里能看到原因。` · 按钮 `重试` |
| 浏览器直开（纯 `file://`） | 用浏览器打开本原型时自动出现 | 标题 `当前是静态原型，没有接后端` · `按钮状态与列表内容均为设计演示数据。` · 按钮 `知道了`（关闭后不再提示） |

两种形态都**不再**直接暴露 `Failed to fetch`；原始串只作为 `<code>` 里的诊断信息存在。

---

## 5. 交互清单（全部可点，均为本地状态）

| 控件 | testid | 行为 |
| --- | --- | --- |
| 主题切换 | `shell-theme-toggle` | `浅色 ⇄ 深色`，两套令牌独立；`data-theme` 挂在 `<html>` |
| 平台 pill | `platform-boss-card` / `platform-zhilian-card` / `platform-51job-card`、`platform-boss-check`、`platform-boss-status`（绿点）、`platform-boss-login-btn` | 未接入平台整卡灰态、勾选框 `disabled`；Boss 的「登录」原型内 `disabled`，标题注明「登录功能接线中」 |
| 打分方式 | `scrape-mode-llm-seg` / `scrape-mode-rule-seg`（`role=radio`） | 互斥；选中态统一为「玻璃底 + 绿色描边」 |
| 敏感库 | 顶栏打分分段控件（大模型打分 \ |  |
| 配置 | `shell-config-btn` → 弹窗 `config-panel` | **四组配置卡**（可折叠：`config-llm-toggle` / `-scrape-toggle` / `-score-toggle` / `-data-toggle`）：① `大模型`（副标 `密钥只回显尾 4 位`）：`config-api-key`（`type=password`，占位 = 当前掩码；提示 `留空表示不改；当前：****9f3c`）、`config-base-url`、`config-model`（`deepseek-chat`）、`config-llm-prefilter`（50，提示 `（低于此分不调用大模型）`）；② `抓取默认参数`：`config-keywords`（占位 `Java, 后端（逗号分隔）`）、`config-max-jd`（15）、`config-min-score`（70，提示 `（满分由 get_scoring_spec 下发）`）；③ `打分口径`（只读）：100 / 70 / `技能 40 · 薪资 20 · 经验 15 · 方向 15 · 公司质量 10`；④ `数据目录与库概览`（只读）：`config-data-dir`、`config-db-total`。底部 `config-note` + `config-cancel-btn` / `config-save-btn`；`config-close-btn` / `Esc` / 点遮罩关闭。**字段与逐字文案对齐 `frontend-vue/src/features/config/panel.rs`，键名与默认值来自 `backend-tauri/src/core/domain/app_config.rs`，零新增字段** |
| 自动投递 | 状态条 `auto-apply-bar`（计数 `auto-apply-pending` / `auto-apply-done`）；主按钮 `auto-greet-toggle-btn`、结束按钮 `auto-greet-stop-btn` | **裁决 E/F（v5）**：状态条独立成整行（右栏顶端、`.rail-head` 之上；对话框展开时整段搬进 `.chat-detach-head`，同一段 DOM 只搬不复制）。`idle` 一颗按钮「开始自动投递」；点后变两颗：「暂停」（警示色 `.btn-pause`）+「结束」（`.btn-danger`）；再点「暂停」变「继续」（回主色）；「结束」回 `idle` 单按钮态。右侧恒显示 `待投递 N` / `已投递 N`。详见 §12A.5 |
| ~~二级导航~~ | ~~`nav-overview-tab` / `nav-chat-tab` / `nav-config-tab` / `nav-logs-tab`~~ | **已按用户裁决整条删除**：Boss 消息在右栏、配置在顶栏、决策记录从底部日志卡打开 |
| 筛选条件 | `scrape-filter-toggle-btn` → 面板 `filter-panel` | **完整筛选器，12 段一次性全展开**（禁止折叠分组）：① 关键词 chips（可删）+ 输入框回车添加 + `读取期望职位`；② 推荐词条；③ 城市 / 职位类型 / 搜索最大翻页次数；④ 薪资预设 + 自定义区间 + 已选 chip + `清空`；⑤ 工作经验；⑥ 学历；⑦ 公司规模；⑧ 融资阶段；⑨ 行业 + 自定义编码回车；⑩ 排除关键词 textarea；⑪ 底部四列 HR 活跃度 / 工作制 / JD 岗位要求 / 成立年限。面板自身可滚动（实测 scrollH 918 / clientH 682），**面板内不得出现任何配置项**（API Key、模型选择、打分开关、抓取上限、并发、超时、日志级别）——抓取上限与分值门槛仍留在控制条上；`filter-close-btn` / 点外部 / `Esc` 收起 |
| 抓取 | `scrape-start-btn` / `scrape-stop-btn` | **裁决 D（三态）**：`开始抓取` →（点）→「启动中…」禁用 650ms → `抓取中`，主按钮变 `暂停抓取` 并转警示色（`.btn-pause`、`aria-pressed=true`）；`暂停抓取` →（点）→ `继续抓取`（回主色、`aria-pressed=false`）→（点）→ `暂停抓取`；`停止抓取` 回 `空闲`。状态串走 `scrape-brief`：`抓取中 · 已入库 N 条…` / `已暂停 · 已入库 N 条` / `空闲 · 已入库 N 条` |
| 门槛与上限 | `scrape-max-jd-input`、`scrape-min-score-input` | 数值即改即显示（真实写入走 `set_limit`，见 `AGENTS.md` §2 第 2 条） |
| ~~重新打分 / 已入库 N 条~~ | ~~`scrape-rescore-btn` / `scrape-ingested`~~ | **裁决 G 已整块删除（图 1/5）**：入库数只保留在 `scrape-brief` 文案里，`.scrape-tool` 行与其样式一并移除 |
| 投递页签 | `job-tab-pending-tab` / `-delivered-tab` / `-rejected-tab` | 三态统一为「选中=绿底玻璃 / 未选中=透明 + 计数徽标」；`aria-selected` 已随点击同步 |
| 排序 | `job-sort-added-btn` / `job-sort-salary-btn` / `job-sort-score-btn` | 单选，`分值↓` 默认；箭头随点击翻转 |
| 全选/勾选 | `job-select-all-check`、`job-check-{id}` | 批量行计数（`job-selected-count`，初值 `已选 0 条`）实时更新；≥2 条时 `job-batch-actions` 显形，可点 `job-batch-reject-btn` / `job-batch-allow-btn` |
| 行内标记 | `job-reject-{id}` / `job-allow-{id}` / `job-deliver-{id}` | 改标后卡片落到对应页签 |
| 岗位详情 | `job-card-{id}` → `job-detail-drawer` | 抽屉；`job-detail-reject-btn` / `job-detail-deliver-btn` / `job-detail-close-btn` |
| 刷新 | `job-refresh-btn` / `chat-rail-refresh-btn` | 各自先 Loading 后回原态 |
| 会话 | `chat-friend-{uid}` | **点任意 Boss 即展开对话框并选中该会话**（旧行为「切右栏线程视图 + `chat-thread-back-btn` 返回」已作废）。右栏永不显示线程；`.is-active` 标出当前会话 |
| 对话框 | `chat-friend-{uid}` → `chat-detach` | ~~`chat-rail-expand-btn` 触发三级展开~~ **【已作废】**。现为：Boss 列表默认常驻；点任意 Boss ⇒ 对话框从右栏左缘向左展开、主列被压缩，**对话框自身（内部含右段 Boss 列表）= 窗口宽 50vw**；面板内左 = 聊天区（`chat-detach-thread` / `chat-bubble` / `chat-detach-compose-input` / `chat-detach-send-btn`），右 = Boss 列表（`chat-detach-rail`，固定基线宽 = `--rail-w`，300 / 260，不可被挤扁）；`chat-detach-close-btn`（`收起对话框 ›`）与 `Esc` 收起，收起后右栏复原全宽、`activeFriend` 保留，再点任意 Boss 直接重开。详见 §12 |
| 决策记录 | `log-decisions-btn` → `decisions-panel` | **独立模态弹窗**（`position:fixed` 居中 860px + `overlay-scrim`），`decisions-close-btn` / `Esc` / 点遮罩收起；**9 行** `时间 / 来源 / 结论 / 理由`，全百分制。`data-testid=score-spec`（`满分 100 / 默认门槛 70`）仍在日志卡头部，二级导航删除后由此保住 §4.1 的「满分口径」一条 |
| ~~折叠卡~~ | ~~`card-expect-toggle` / `card-trial-toggle`~~ | **已按用户裁决移除**：求职期望并入筛选条件面板，打分试算整卡删除（脚本、`data-testid`、状态不留残留引用） |
| 日志 | `log-live`、`log-rows`、`log-row` | 当前原型把决策日志与运行日志**合流按时间交错**（卡头写 `合流 · 按时间交错`），`实时` 前有呼吸点；旧版日志页的「决策记录 / 运行日志」分段，现由日志卡头部的 `决策记录` 按钮承担（见上一行） |

**键盘可达**（已实测）：`Esc` 关闭抽屉、敏感库浮层、对话框、决策记录弹窗、配置弹窗与筛选条件面板（`document` 级监听）；所有按钮/输入框 `Tab` 可达，焦点环用 `--primary`；关键词输入框与行业自定义编码框支持 `Enter` 添加（空格 / 逗号 / 顿号分隔）；页签与分段控件是 `role=radio tab` 按钮，**方向键切换尚未实现**（见 §11 第 6 条）。

---

## 6. 与旧版对照：改了什么、为什么

| # | 旧版 | 设计稿 B | 依据 |
| --- | --- | --- | --- |
| 1 | 白底扁平，工具栏三色按钮（绿 `开始抓取` / 粉 `停止抓取` / 描边 `筛选条件`）同级并列 | 全部换成玻璃层级：**主操作只有一个视觉重量**（`开始抓取` = 绿色实心渐变）；`停止抓取` 退为 `--bad-soft` 浅底（仍可辨但在抓取中才成为主操作）；`筛选条件`、`刷新`、排序等为 ghost / glass 按钮 | 基线 §2.3 + 本项目 `controls.css` 的 `-primary` / `-danger` / `-ghost` 分级 |
| 2 | 整宽浅红条，唯一内容是裸文案 `Failed to fetch` | 中文标题 + 一句人话 + 原始串（等宽）+ `重试`/关闭；右栏同理 | K9：错误必须对用户可读，同时保留排障线索 |
| 3 | 三药丸页签各自上色（绿/白/白），计数裸跟在文字后 | 统一两态：选中 = 玻璃底 + 绿描边 + 绿色计数徽标；未选中 = 透明 + 灰计数徽标 | 消除「三个颜色各自为政」的层级混乱 |
| 4 | 空列表占满整屏，只有一行居中 `暂无岗位` | 保留 `暂无岗位` 作主文案，补副文案与主行动（`开始抓取`）+ 次行动（`展开筛选条件`）；列表卡不再撑满全屏，工作台其余区块（筛选条件 / 日志）一屏可见 | 「空态要给下一步」，且让页面高度稳定 |
| 5 | 大模型/规则打分分段控件为灰/绿两块 | 统一为玻璃分段 + 绿色描边选中；`大模型未配置` 时以 `llm-missing-chip` 提示（K3 失败态） | 同上 |
| 6 | 右栏错误串在 260px 下会挤成两字一行 | `.rail-error` 100% 宽、`overflow-wrap:anywhere`、标题与原始串分行 | 1100–1279 断点实测 |
| 7 | 顶栏右组 7 项在 1500px 一行放不下 | ≥1900 全显；1360–1899 收成 `累计 103.8K token` chip（`title` 里给全量）；<1360 隐藏统计 | 保持旧版内容不丢，但不许折行 |
| 8 | 排序键、批量提示在列表卡头右侧，靠右对齐 | 位置不变，权重降为 ghost/灰字；`分值↓` 作默认排序 | 布局照旧，只改权重 |
| 9 | 日志是等宽时间戳 + 文案 | 结构不变，时间戳等宽、级别色点；`实时` 前加呼吸点 | 不改字段 |
| 10 | 无日志范围之外的「下一步」引导 | 卡片头统一为「标题 + 计数/副标 + 右端操作」三段式 | 让信息密度可控 |

---

## 7. 断点与视口自检（真实测量，非估算）

| 视口 | 右栏宽 | 右栏位置 | 岗位卡列数（收起态） | 岗位卡列数（展开态，v10 裁决 X） | 顶栏高度 | 横向溢出 |
| --- | --- | --- | --- | --- | --- | --- |
| 1200×800（最小宽度） | 260px | 与主列同一行 | 2 | **1** | 46px | 无（1200 即基线，不出横向条） |
| 1280×820 | 300px | 与主列同一行 | 2 | **1** | 46px | 无 |
| 1440×900 | 300px | 与主列同一行 | 2 | **1** | 46px | 无 |
| 1599×900 | 300px | 与主列同一行 | 2 | **1** | 46px | 无 |
| 1600×900 | 300px | 与主列同一行 | 3 | **2** | 46px | 无 |
| 1920×1080 | 300px | 与主列同一行 | 3 | **2** | 46px | 无 |

| 1150×800（< 最小宽度） | 260px | 与主列同一行 | 2 | **1** | 46px | **有**（根容器锁定 1200px，docSW 1200 > innerW 1150；仍不降级到 0 列） |
- 所有 `<1200px` 的响应式分支已删除：原 `max-width:1099px` 落底块（`--rail-w:100%` / `flex-direction:column` / 右栏 `height:360px` / `.stage{overflow-y:auto}`）、`max-width:900px` 单列、`max-width:760px`、`max-width:1150px` 顶栏隐藏、`max-width:1099px` 日志卡 170px / `.tb-tail` 静态化 / 对话框落底 —— 全部移除。
- 之前「1100 保 260 侧栏」的 1099 断点改动**同时作废**（不存在 1100 这个视口了）。
- 保留的断点只剩：`≥1900` 统计组全展开、`≤1599` 网格 2 列、`≤1279` 右栏 260px、`≤1359` 顶栏统计折叠成 token chip、`1360–1599` 隐藏 `.rail-stat` 与分隔线（v9.1 起网格列数断点由 1279 放宽到 1599），以及 v10 裁决 X 的「展开态再降 1 列」两条（写在断点规则**之后**）。
- 1200 是唯一基线布局：右栏始终与主列同排，网格卡在 1200–1359 为 2 列、≥1360 为 3 列（按主列实际宽度自然排布，不出现 1 列回退）。

实测方式：Chrome DevTools Protocol `Emulation.setDeviceMetricsOverride` + `getBoundingClientRect()` 读数（脚本见 §9）。本机沙箱**已无法启动 headless 浏览器**（§12.7），这几档数值目前由 `_smoke.mjs` 的几何推断复算，实机复核须在 GUI 机器上手工做。

---

## 8. 给前端编码（Vue 3）的接入备注

1. **令牌同源**：`index.html` 里的 `:root` / `[data-theme='light']` 直接抄自 `frontend-vue/styles/tokens.css`，变量名、数值一字不改。接入时**不要**在组件里写颜色字面量，一律用 `var(--glass-2)` / `var(--text-2)` 这类令牌；本原型新增的少量局部变量（如 `--rail-w`）已在文件顶部集中声明。
2. **`data-testid` 已按 `<页面/模块>-<功能>-<控件类型>` 命名**。编码时逐个保留，Vue 侧用 `:data-testid="tid('job', 'retry', 'btn')"` 之类工具函数拼装动态 id；`job-check-{id}` 这类**必须与真实 job id 对齐**。计数一律现场统计（`Select-String -Pattern 'data-testid="' index.html`），本文档不再写死数字——本轮增删控件较多（新增筛选面板、敏感库卡片、决策记录、对话框头部），旧数字已全部过期。
3. **MOCK 与真实数据的分界**：所有假数据集中在 `index.html` 的 `MOCK_DATA` 对象（岗位 8 条、会话 6 条、线程若干）。接入时只保留渲染函数，把 `MOCK_DATA.jobs` 换成 `invoke('list_jobs', {args: ...})` 的返回；`MOCK_DATA` 之外不允许出现任何写死的业务数据。
4. **状态机的四态**：列表 `listMode ∈ default loading empty error`、右栏 `railMode ∈ default loading empty error`，两者**独立**（可能同时出现「列表有数据、右栏报错」）。Vue 侧建议用两个独立 ref + 一个 `computed` 派生可见块，不要合并成一个全局 loading。
5. **原文案来自 §4.1 的必须逐字**：`暂无岗位`、`投递列表`、`未投递`、`已投递`、`不投递`、`共 N 条`、`刷新`、`全选本页`、`已选 N 条`、`列表共 N 条`、`勾选 2 条以上可批量标记；单条直接用行内标签`、`加入时间`、`薪资`、`分值`、`开始抓取`、`停止抓取`、`筛选条件`、`空闲 · 已入库 N 条`、`有效 JD 上限`、`分值门槛 ≥`、`分（低于不入库）`、`决策日志`、`运行日志`、`实时`、`Boss 消息`、`搜索 HR / 公司 / 消息`、`◀对话框`、`敏感库`、`登录`、`未接入`、`大模型打分`、`规则打分`、`招聘网站`、`配置`、各 token 计数标签。**新增文案**（空态副文案、错误态标题与建议、按钮 `重试`、`展开筛选条件`、`开始自动投递`）是与文案表并列的新增项，编码时可直接用，若产品要改文案只改这几处。
6. **C9 进度五段文案**：原型状态条目前是 `空闲 · 已入库 N 条`（旧版实拍形态）。正式实现请按 `frontend-vue/AGENTS.md` C9 补 `已抓 / 入库 / 重复 / 低于门槛 / 规则否决` 五段与 `sr-only` 连排摘要——本原型**故意不改**这一条，避免与既有契约冲突。
7. **C8 隐藏表格**：`features/jobs/list.rs` 的 `sr-only` `<table>` 是 testid 保活锚点，迁移时必须原样保留；本原型用 `job-state-announce` 演示 aria-live 播报的做法可沿用。
8. **不要带走的**：原型工具条（`.proto`）、`?mode=` / `data-shot` 解析、`MOCK_DATA`、`window.__PROTO__` 自检探针、`shots/` 目录。
9. **CSP**：原型全程无内联外的资源引用，也没有 `eval`；迁到 `frontend-vue/` 后仍按 `tauri.conf.json` 的 `script-src` 规则处理（不要加 `'unsafe-inline'`，见根 `AGENTS.md` §3.1）。

---

## 9. 复现自检（可原样再跑）

```powershell
# 1) 自包含校验：应为 0
$c = Get-Content D:\AIProject\JobAssistant\prototypes\ui-v1\index.html -Raw
([regex]::Matches($c, 'https?://')).Count
([regex]::Matches($c, '<link')).Count
([regex]::Matches($c, '(?i)<script[^>]+src' + '=')).Count
([regex]::Matches($c, '@' + 'import')).Count

# 2) 单测级自检：四态 / 主题 / 断点（另存为 probe.mjs，node probe.mjs）
#    用 --headless=new --remote-debugging-port=9335 + Emulation.setDeviceMetricsOverride 读 rect
```

**截图集已整体删除**（用户裁决）。`shots/` 目录不再存在，`prototypes/ui-v1/` 里不再有任何 `.png`。

删除理由：全部截图都停留在 v9 之前的形态，其中默认态那几张还拍着 v10 已删掉的「不投递 / 允许投递 / √」三颗行内按钮，且没有地址行与打招呼方块；本机沙箱又起不了 headless 浏览器（§12.7），补拍不可能。留着的差异信息比信息量更大，只会误导下游 Vue 落地。

**替代手段**：`_smoke.mjs` 的 A1–A54 源码级断言 + `_modes.mjs` / `_tri.mjs` 行为断言；视觉层按 §12A11.5 与各节「人工走查」清单在有 GUI 的机器上复核。旧版真机界面图仍在 `docs/ui/reference/`（11 张裁切图，长期有效）。

---

## 10. 基线 §9 的 Q1–Q10：本原型已替用户做的默认选择

| 编号 | 问题（基线 §9） | 原型默认选择 | 可回退方式 |
| --- | --- | --- | --- |
| Q1 | 是否保留顶栏 token 统计（`累计 103.8K token` 等 5 项） | **保留**：≥1900 全显，1360–1899 折叠成一个 `累计 103.8K token` chip（`title` 里有全量），<1360 隐藏 | 删 `.tb-stats` / `.tb-stats-collapsed` 两块 DOM |
| Q2 | 是否保留「招聘网站」标签与平台 pill 的三角关系 | **保留原样**（Boss 绿底 pill + 红点 + 登录；智联/51job 灰态 `未接入`） | 删对应 DOM 块 |
| Q3 | 页面导航形态（旧版无页签） | **不设二级导航**：Boss 消息在右栏、配置在顶栏、决策记录从底部日志卡打开（用户裁决 5，二级导航已移除） | 如需页签需产品另议 |
| Q4 | 打分方式控件形态 | **分段控件**（互斥、绿色描边选中），保留 `大模型打分 / 规则打分` 原文案 | 改回旧版灰/绿两色块（`controls.css` 的 `.seg`） |
| Q5 | 空列表是否给引导 | **给**：副文案 + `开始抓取` + `展开筛选条件`，`暂无岗位` 仍是主文案 | 删 `job-state-empty` 里的 hint/actions |
| Q6 | 错误呈现形态 | **中文标题 + 人话 + 等宽原始串 + 重试**；原始串保留用于报障 | 只留原始串即回到旧形态 |
| Q7 | 点 Boss 的展开方式 | **点任意 Boss 直接展开，无中间一级**：对话框从右栏左缘向左展开、压缩投递列表，**对话框自身（含左段 Boss 列表）= 50vw**（1200→600（下限，见 v10 裁决）/ 1280→640 / 1440→720 / 1600→800 / 1920→960），主列吃剩下的一半；面板内左 = Boss 列表，右 = 聊天区（底部唯一输入框）；`收起对话框 ›` / `Esc` 收起后右栏复原全宽、选中保留。~~旧三分支（一级按钮 → 二级 640px → 三级铺满 81.9%）已作废~~ | ~~旧方案「整组右栏搬进 520px 居中浮层」已作废~~（用户裁决 3）；详见 §12 与 `docs/ui/UI-SPEC-chat-detach-v3.md` |
| Q8 | 三药丸页签配色 | **统一两态 + 计数徽标**（选中绿底玻璃 / 未选中透明） | 回退三色需自己定三组色值 |
| Q9 | 列表卡片数形态 | **网格卡**（**≥1600 三列、<1600 两列**；聊天对话框展开时再各减一列，见 v10 裁决 X / §12A11；不做 1 列回退，`<1200` 保 1200 基线并出横向滚动条），保留旧版全部字段 | 改回单列行式只需改 `.job-grid` 的 `grid-template-columns`；列数只随窗口断点与展开态变化，不随 `--ui-scale` |
| Q10 | 主题切换位置 | **顶栏右端 ghost 按钮**（`☀ 浅色` / `☾ 深色`），与 `配置` 同组 | 挪到设置页 |

> 其余未在 Q1–Q10 之内、但原型做了取舍的点：`开始自动投递`（旧版顶栏 `自动打招呼 关` 的控件化，属新增，用户裁决保留；v9 起顶栏那个 `自动打招呼 关` 状态项本身已移除，状态改由投递状态条描述）。原「折叠卡（求职期望 / 打分试算）」**已按用户裁决移除或并入**：求职期望并入 `筛选条件 ▾` 面板，打分试算整卡删除。

## 11. 原型未做、明确留给前端编码的空位

| # | 空位 | 说明 |
| --- | --- | --- |
| 1 | 日志分段开关 | 旧版日志卡有「决策日志 / 运行日志」两个来源；本原型只做了合流视图（卡头 `合流 · 按时间交错`）。若要保留分段，请在前端编码时按旧版两段式补 `log-scope-*` 控件，并把合流作为第三个选项。 |
| 2 | 筛选条件面板 | 已按用户裁决做成**真正可展开的完整面板**（`filter-panel`，12 段一次性全展开、自身可滚动）。面板内**只放筛选与期望条件**，不含任何配置项（API Key / 模型选择 / 打分开关 / 抓取上限 / 并发 / 超时 / 日志级别）。C12 的草稿双写逻辑仍待后端契约确定后再接。 |
| 3 | 配置页 / 决策记录页 | 🔁 **第三轮已接线**：配置改为顶栏弹窗（四组字段对齐 `features/config/panel.rs`，不再只是提示）；决策记录改为独立模态弹窗。二者仍是纯原型，保存按钮只关闭弹窗、不做真实 `invoke` 写入。 |
| 4 | 真实数据与写操作 | 一切写操作（开始抓取、标记、发送消息、保存门槛）都只改本地状态，不调 `invoke`，**不得**被当作已接线的行为。 |
| 5 | 五段进度文案 | 状态条只保留旧版实拍的 `空闲 · 已入库 N 条`，C9 五段（已抓/入库/重复/低于门槛/规则否决）留给前端编码按契约补。 |
| 6 | 键盘 `←/→` 切 radio 组 | **本原型未实现**（分段控件、排序组、页签目前只能点）。前端编码若用原生 `<input type="radio">` 自带此行为；若继续用 `role=radio` 按钮，需要自己补方向键+`aria-checked` 同步。 |
| 7 | 1200px 顶栏 | 统计组 `<1359` 已折叠成 chip、`<1150` 起整组裁让（`overflow:hidden`）；若将来要全量可见需改两行或收进弹层。本轮已修 `.tb-tail` 绝对定位导致的重叠：1600/1440/1280/1200/1150 五档实测顶栏单行 46px、控件矩形两两不相交。 |

---

## 12A. 配置弹窗与敏感库（v4：字段集收敛 + 固定尺寸）

> **用户裁决（逐字，2026-11-22）**：`配置只有这些，铭感弹窗大小不要跟随内容变化，固定大小，溢出的使用滚动`（「铭感」= 敏感库）。
> 两句分开生效，**都不删字段**：配置弹窗只收敛**默认视图**，敏感库只改**尺寸策略**。

### 12A.1 裁决 A：配置弹窗默认视图 = 用户截图字段集

默认可见四张卡（全部默认展开）：

| 顺序 | 卡 | 字段 / `data-testid` |
| --- | --- | --- |
| ① | 大模型 | `config-api-key`（密码框，掩码回显 `config-api-key-mask`）/ `config-base-url` / `config-model` |
| ② | 提示词 | `config-prompt-input`（textarea）+ `config-prompt-reset-btn`「恢复默认」 |
| ③ | 技能 | `config-skill-input`（textarea，说明「生成招呼语时优先用这里的技能；留空则用简历画像里的技能。多个用逗号/顿号分隔」） |
| ~~④~~ | ~~token 用量~~ | ~~`config-token-usage`~~ —— **v6 已整卡移除**（用户逐字：「移除1、2」），token 读数改到顶栏「配置」按钮左边（见 §12A5.6） |

**v6 起配置弹窗只有三张卡**（大模型 / 提示词 / 技能）+ 底部说明与「取消 / 保存」。

~~**契约字段一个不删，全部收进「高级」卡**：`config-advanced-section`（`aria-expanded="false"`，默认收起，虚线边框区分）内含
`config-scrape-section`（`config-keywords` / `config-max-jd` / `config-min-score` / `config-llm-prefilter`）、
`config-score-section`（`config-score-max` / `config-score-min` / 五维权重）、
`config-data-section`（`config-data-dir` / `config-db-total`）。~~

- **v6 作废**：上面「收进高级卡保契约」的做法已被用户裁决推翻 —— 「移除1、2」把 token 卡与整张「高级」卡（含其三个子卡）**真的从界面删掉**。
  后端命令不变（`set_limit` / `get_scoring_spec` / 数据目录查询照旧），只是不再有界面入口；字段定义备份在 `docs/ui/UI-SPEC-chat-detach-v3.md` §10 v6，需要时从规格取回。
- **v6 保留的 testid**：`config-api-key` / `config-api-key-mask` / `config-base-url` / `config-model` / `config-panel` / `config-close-btn` / `config-save-btn` / `config-cancel-btn` / `config-note` / `config-llm-section` / `config-llm-toggle` / `config-prompt-input` / `config-prompt-reset-btn` / `config-skill-input`（零改名）。
- **v6 必须不在 DOM 的 testid**：`config-token-section`、`config-token-toggle`、`config-token-usage`、`config-advanced-section`、`config-advanced-toggle`、`config-scrape-section`、`config-scrape-toggle`、`config-keywords`、`config-max-jd`、`config-min-score`、`config-llm-prefilter`、`config-score-section`、`config-score-toggle`、`config-score-max`、`config-score-min`、`config-data-section`、`config-data-toggle`、`config-data-dir`、`config-db-total`。
- **`config-keywords` 与新的「技能」textarea 不是同一个东西**：前者是**抓取关键词**（后端 `keywords`），后者是**打招呼优先技能**。语义不同，故**不复用节点**，各用各的 testid。
- Base URL / 模型保持**占位文案**策略（`DeepSeek 官方 · Base URL` / `deepseek-chat`），用户截图里的真实接口地址**不搬进原型**（外链门禁必须为 0）。
- 卡头折叠复用既有 `toggleConfigSection()`；v6 后委托分支只剩 `config-llm-toggle` / `config-prompt-toggle` / `config-skill-toggle` 三个 case（其余随卡片一并删除，不留死分支）。

### 12A.2 裁决 B：两个弹窗固定尺寸，溢出只在内层滚

| 选择器 | 旧 | 新 |
| --- | --- | --- |
| `.config-modal` | `width: min(760px, 100vw-64px)` + `max-height: min(80vh, 720px)` | 同宽 + **`height: min(640px, calc(100vh - 96px))`**，删除 `max-height` 口径 |
| `.cfg-body` | `flex:1 1 auto; min-height:0; overflow-y:auto` | 不变（核对生效）—— 长 textarea 在这块里滚，**不把弹窗顶高** |
| `.sensitive.floater` | `max-height: min(720px, calc(100vh - 56px))` | **`height: min(720px, calc(100vh - 56px))`** + `max-height: none`（只覆盖本浮层，**不动基类 `.floater`**，`job-detail-drawer` 等不受影响） |
| `.sen-head` / `.sen-toolbar` | 默认 `flex: 0 1 auto` | 加 **`flex: 0 0 auto`**，固定头与工具条不被压缩 |
| `.sen-grid` | `flex:1 1 auto; min-height:0; overflow-y:auto` | 不变（核对生效）—— 卡片多也只在这块里滚 |

**通用断言**：弹窗高度在「内容多 / 内容少」两种状态下**不变**；只有内层滚动容器（`.cfg-body` / `.sen-grid`）发生滚动。窗口高度不足时靠 `calc(100vh - …)` 的下限收缩，仍不回到内容自适应。

### 12A.3 断言 A18–A24（`_smoke.mjs` 已固化，全部是纯文本静态检查）

| 编号 | 断言 |
| --- | --- |
| **A18** | `.config-modal` 规则里有 `height: min(640px…`，且不再只有 `max-height` 一种口径 |
| **A19** | `.sensitive.floater` 有固定 `height: min(720px…` 且 `max-height: none` |
| **A20** | `.cfg-body` / `.sen-grid` 同时具备 `flex: 1 1 auto` + `min-height: 0` + `overflow-y: auto` |
| **A21** | **v6 反转**：19 个被移除的字段在全文命中 0（`config-token-*` / `config-advanced-*` / `config-keywords` / `config-max-jd` / `config-min-score` / `config-llm-prefilter` / `config-score-*` / `config-data-*` / `config-db-total`） |
| **A22** | 保留的 11 个既有 config testid 逐个 `includes` 命中（`config-api-key` / `-mask` / `config-base-url` / `config-model` / `config-panel` / `config-close-btn` / `config-save-btn` / `config-cancel-btn` / `config-note` / `config-llm-section` / `config-llm-toggle`） |
| **A23** | 3 个新增 testid 存在（`config-prompt-input` / `config-prompt-reset-btn` / `config-skill-input`） |
| **A24** | 原型外链命中 0，且不含 DeepSeek 官方接口地址字面量（Base URL 只留占位文案） |

**反向验证（已实测）**：分别注入 5 个变异各跑一次 `node prototypes/ui-v1/_smoke.mjs`，全部 `exit=1` 并报出对应编号，恢复后回到 OK —— 断言不是空转。

### 12A.4 人工走查（本机无 headless，必须目视）

1. 开 `index.html?mode=default-config`（或点顶栏「配置」）：弹窗 640px 高、宽度 760px。
2. 默认只看到 大模型 / 提示词 / 技能 三张卡（v6 起 token 卡与「高级」卡已不存在）；弹窗仍是 640px 高，**不因内容变少而收缩**。
3. 底部只有一行说明 + 「取消 / 保存」；`.cfg-body` 内无任何抓取参数 / 打分口径 / 数据目录字段。
4. 在「提示词」textarea 里连打 30 行：**弹窗高度仍不变**，滚动条只出现在 `.cfg-body`。
5. 打开敏感库（入口在顶栏打分分段控件右侧）：720px 高固定；把三列卡片滚到底，头部（标题 + 工具条）**不移动、不被压缩**。
6. 全程外链协议前缀 / 外链标签 / 样式导入指令 / 外部脚本标签 命中 0。

---

## 12. Boss 列表与对话框（chat-detach v3）

> **冻结真源：[`docs/ui/UI-SPEC-chat-detach-v3.md`](../../docs/ui/UI-SPEC-chat-detach-v3.md)**（状态 `UI_READY`）。
> 本节是原型的可执行说明；两者冲突时以 v3 规格为准。v3 **废止** `docs/ui/UI-SPEC-legacy-baseline.md` §3.5 的三级展开条目、§9 Q-2 的「三级展开 / 50% 上限」表述与 §5.4 的「`chat` 整页保留为展开视图」，其余条款继续有效。

**用户裁决（逐字）**：`boss列表默认存在，对话框默认隐藏，点击任意boss下左侧展开聊天，允许主动收起和打开` / `展开的列表+聊天框压缩投递列表，达到窗口的50%`。

### 12.1 状态机（两维，互相独立）

```
chatOpen      ∈ { false, true }      初始 false —— 对话框默认隐藏
activeFriend  ∈ { null, uid }        初始 null —— 收起时不清空

关闭 ──点 chat-friend-{uid}──▶ 打开（activeFriend = uid；未读清零）
打开 ──点 chat-friend-{uid}──▶ 打开（只切会话，不重播进场）
打开 ──点 chat-detach-close-btn / Esc──▶ 关闭（Boss 列表复原全宽，activeFriend 保留）
关闭 ──点同一 uid──▶ 打开（保留的选中不会被清掉）
```

| `chatOpen` | `activeFriend` | 形态 |
| --- | --- | --- |
| `false` | `null` | 默认态：Boss 列表常驻（无高亮），主列全宽 |
| `false` | `uid` | 收起态：Boss 列表常驻 + 该条目 `.is-active`，主列全宽 |
| `true` | `uid` | 展开态：主列压到 ≈50%，对话框整体占 50%（内部「左 Boss 列表 + 右聊天区（线程 + 唯一输入框）」） |
| `true` | `null` | 仅异常路径（参见 §12.6 第 5 条）：聊天区显示 `选择一位 Boss` 空态，**不渲染输入框** |

### 12.2 宽度公式与四档数值

```
--rail-w   = 300px (≥1280)   260px (1200–1279)
--detach-w = clamp(600px, 50vw, 80vw)      ← .chat-detach 自身宽（内部已含左段 Boss 列表）
聊天区宽   = --detach-w − --rail-w         （1440 档 = 720 − 300 = 420 ≥ 320）
主列剩余   = 100vw − --detach-w ≈ 50vw
```

> **v3.2 修正（2026-11-22）**：用户裁决「boss列表在左侧，聊天框在右侧」—— 对话框体内左段 = Boss 列表（`--rail-w`）、右段 = 聊天区（余量）；DOM 顺序随之翻转，Tab 序天然对齐视觉，分隔线只留 `.chat-detach-rail` 的 `border-right`。testid 一个没改名。
>
> **v3.1 修正**：旧公式 `50vw − --rail-w` 已废止（那口径下聊天框只有 340~500px，用户实测判为「太小」）。`syncDetachWidth()` 里 `half - railW` **禁止出现**，验收门禁 `Select-String 'half - railW' → 0`。

| 视口宽 | 对话框整体 | 占窗口 | 其中 Boss 列表段 | 其中聊天区 | 主列 | 占窗口 |
| --- | --- | --- | --- | --- | --- | --- |
| 1200 | 600（下限触发） | 50.0% | 260 | 340 | 600 | 50.0% |
| 1280 | 640 | 50.0% | 300 | 340 | 640 | 50.0% |
| 1440 | 720 | 50.0% | 300 | 420 | 720 | 50.0% |
| 1600 | 800 | 50.0% | 300 | 500 | 800 | 50.0% |

> 数值为公式推算（未扣 `.app-body` 约 12px 列间隙）。**v10 起下限 640 → 600**（用户裁决，1200 档从 53.3% / 主列 46.7% 拉回各 50.0%，扣间隙后约 49.5%）。本机无浏览器权限，**必须**在有 GUI 的机器上用 `__PROTO__.chatGeom()` 复核后再当实测值引用。

**条目不得竖排换行（v3.1 新增硬约束）**：`.chat-detach-rail` 固定 `flex: 0 0 var(--rail-w)` + `width: var(--rail-w)`；容器链 `.chat-detach-rail / .rail-body-slot / .rail-body / .chat-rail-list / .friend / .friend-main` 全部 `min-width: 0`；`.friend` 加 `white-space: nowrap`，`friend-name` / `friend-msg` 单行省略。否则 300px 容器里条目会被压成约 60px 竖条、逐字换行（用户截图实测现象：「简/历/通/过/初/筛」）。

### 12.3 结构、顺序与动效

- `.app-body` 三兄弟：`section.dashboard`｜`aside.chat-rail`｜`div.chat-detach[hidden]`。
- 视觉顺序（从左到右）：**主列（投递列表 / 日志，被压缩）← `.chat-detach`（内部：左 Boss 列表 + 右聊天区）→ `aside.chat-rail`**。展开时右栏隐藏、内容搬进 `.chat-detach-rail`；对话框右缘贴窗口右缘，宽度 0 → `--detach-w` 增长，读作「从右栏左缘向左展开」。
- 它**在流内**（`position: static`），出现即令主列变窄 —— 不是浮在列表上的遮蔽层。
- 动效：`flex-basis` + `width` 同一条过渡（`var(--dur)` / `var(--ease-glass)`），同一路径进出、可中途抓回；不做 `translateX`；`prefers-reduced-motion` 沿用 §15 全局降级（200ms、只留透明度）。

### 12.4 逐字文案

| 位置 | 文案 |
| --- | --- |
| 右栏头 | `Boss 消息` + 计数 chip + `刷新` |
| 右栏搜索 | 占位 `搜索 HR / 公司 / 消息` |
| 右栏空态 | 标题 `没有会话`；说明 `点「刷新」或在 Boss 直聘回一条消息`；按钮 `刷新` |
| 对话框头 | `Boss 消息` + 计数 chip + `{HR} · {公司}` / 未选中 `在左侧 Boss 列表点一位，开始聊天` |
| 收起按钮 | `收起对话框 ›`（`title="Esc"`） |
| 聊天区空态 | 标题 `选择一位 Boss`；说明 `点左侧 Boss 列表里的任意一位即可查看聊天` |
| 输入区 | 占位 `输入消息…`；按钮 `发送` |
| 左段头 | `Boss 列表` + 计数 chip + `刷新` |

### 12.5 `data-testid` 表

| testid | 角色 | 备注 |
| --- | --- | --- |
| `chat-rail` / `chat-rail-body` / `chat-rail-body-slot` / `chat-rail-count` | 右栏骨架 | 默认可见 |
| `chat-rail-refresh-btn` / `chat-rail-search-input` / `chat-rail-state-announce` | 右栏操作 | — |
| `chat-rail-empty` / `chat-rail-empty-title` / `chat-rail-empty-refresh-btn` / `chat-rail-error` / `chat-rail-guide` / `chat-rail-retry-btn` | 右栏四态 | — |
| `chat-friend-{uid}` | **Boss 条目 = 展开触发器** | 右栏与对话框左段同名同 uid（同一时刻只有一处可见） |
| `chat-unread-{uid}` | 未读徽标 | — |
| `chat-detach` | 对话框根 | `role="dialog"`，默认 `hidden` |
| `chat-detach-head` / `chat-detach-title` / `chat-detach-count` / `chat-detach-sub` | 头部 | — |
| **`chat-detach-close-btn`** | 收起 | 文案 `收起对话框 ›`，`title="Esc"` |
| `chat-detach-body` / `chat-detach-slot` / `chat-detach-rail-body` / `chat-detach-state-announce` | 右聊天区 | — |
| `chat-detach-thread` / `chat-bubble` | 线程 | — |
| `chat-detach-compose` / **`chat-detach-compose-input`** / **`chat-detach-send-btn`** | 唯一输入区 | 未选中 / 加载 / 错误态时 `chat-detach-compose` 为 `hidden` |
| `chat-detach-empty` / `chat-detach-empty-title` | 聊天区空态 | — |
| `chat-detach-rail` / `chat-detach-rail-list` / `chat-detach-rail-count` / `chat-detach-rail-refresh-btn` / `chat-detach-rail-search-input` | **左段** Boss 列表 | 宽 = `--rail-w`；DOM 在 `.chat-detach-chat` 之前，分隔线用 `border-right` |
| `chat-detach-error` / `chat-detach-guide` / `chat-detach-retry-btn` / `chat-detach-empty-refresh-btn` | 左段四态 | — |

**绝对不出现**（`index.html` 全文命中必须 0）：`chat-rail-expand-btn`、`chat-detach-back-btn`、`chat-detach-side`、`chat-detach-l2`；同时无 `is-detached` / `is-detached-full` / `--detach-w-full` 残留。

**Boss 列表底部无发送框**：`chat-rail` 与 `chat-detach-rail` 内不得出现 `textarea` 或 `*-send-btn`；输入只发生在 `chat-detach-compose`。

### 12.6 断言清单


| --- | --- | --- |
| A1 | 默认态 `chat-detach` 带 `hidden`、`chatOpen === false` —— **默认不可见**，无自动弹出路径 | `_smoke.mjs` A1 |
| A2 | 默认态右栏可见且全宽：300（≥1280）/ 260（1200–1279） | — |
| A3 | 点**任意** Boss（6 条逐个试）⇒ 立刻展开且 `activeFriend` 为该 uid | — |
| A4 | 展开几何 **`detachW ≈ 0.5 × innerWidth`（±2px）**：1200→640（下限）/ 1280→640 / 1440→720 / 1600→800；且 `chatRailW = 300 / 260`、`chatW = detachW − chatRailW ≥ 320` | — |
| A5 | 非浮层：`getComputedStyle(chat-detach).position === 'static'`，且主列宽度展开后明显变小 | — |
| A6 | 收起后 **50% 归还主列**：右栏恢复 300 / 260，主列 ≥ 0.97 × 窗口宽 | — |
| A7 | 收起不清选中：`activeFriend` 保留，右栏该条目 `.is-active` | — |
| A8 | **再点同一 Boss 可重开**（连点两次均展开） | — |
| A9 | **`Esc` 只关一层**：决策记录 / 配置 / 浮层打开时只关它，对话框仍展开；再按才收起对话框 | — |
| A10 | 底部无发送框（§12.5 末两条） | — |
| A11 | 四个作废 testid 在 `index.html` 与 `frontend-vue/` 全文命中 0 | — |
| A12 | 自包含：外链计数 0、体积 > 100KB（命令见 §9） | — |
| A13 | `?mode=default-chat` = 默认态；`?mode=default-chat-open-f1` = 已展开且 f1 选中 | — |
| A14 | 既有 mode 不失效（`light-empty-error` / `*-filters` / `*-sensitive` / `*-decisions` / `*-config` / `dark-rails-empty-chat-open-f2` 等） | — |
| A15 | 输入发送后气泡进 `chat-detach-thread`，左段计数同步、不重播进场 | — |
| **A16** | **条目不得竖排换行**：展开态 `.chat-detach-rail` 宽恒为 300 / 260；`.friend` 的 `scrollWidth ≤ clientWidth + 1`（单行省略生效）；`.chat-detach-chat` ≥ 320px 且 `.chat-bubble` 在 420px 宽下不逐字换行 | — |
| **A17** | **左右顺序不得回归**：`index.html` 中 `class="chat-detach-rail"` 的索引 **<** `class="chat-detach-chat"` 的索引；`.chat-detach-rail` 有 `border-right`，`.chat-detach-chat .rail-body-slot` 无 `border-left`（只一条分隔线）；Tab 序 = 列表搜索框 → 条目 → 聊天输入框 → 发送 | — |

### 12.7 截图与复核

- **本机未生成 `shots/shot-18-chat-default-1440x900.png` / `shots/shot-19-chat-open-1440x900.png`**：沙箱下 Chrome / Edge 的 `--headless=new` 在 `crashpad_client_win.cc:431 OpenProcess: 拒绝访问 (0x5)` → `platform_channel.cc:112 FATAL Check failed: . 拒绝访问 (0x5)` 处自毁（Edge 另报 `network_sandbox.cc:591 拒绝访问 (0x5)`）；换 `$env:TEMP` 下独立 `--user-data-dir`、加 `--no-sandbox --disable-features=Crashpad` 均无效，`--screenshot` 也报 `Multiple targets are not supported in headless mode`。**该路径已彻底放弃，不要再试**（用户裁决「暂不处理」）。**已做的替代校验**：内联脚本 `node --check` 通过；`_smoke.mjs` 用 DOM 桩跑通初始化并断言 A1–A6/A16 的公式与 CSS 约束，`_modes.mjs` 断言 9 个 `?mode=` 直达组合；v10 之后的视觉承诺由 §12A11.5 的 A53 / A54 源码级断言兜底。
- **人工走查步骤**：见 v3 规格 §8（1440 默认态截图 → `__PROTO__.chatGeom()` 断言 `detachW === 720 / chatRailW === 300 / chatW === 420 / main ≥ 700` → 点 Boss 截图 → 收起断言 `detachW === 0 且 railW === 300` → 再点重开 → `Esc` 分层 → 四档宽度复核 → 逐个 mode 直达）。
- **`shots/` 目录已整体删除**（用户裁决，理由见 §9）：历史截图全部停留在 v9 之前形态，与当前实现冲突（默认态还拍着 v10 已删的三颗行内按钮），留作对照反而误导。
## 12A5. 投递状态条与三窗（v5：裁决 D / E / F / G）

> 取代关系：本节新增，**不改**前面 §12（chat-detach v3）的任何条款；§12A（v4 配置弹窗字段集与固定尺寸）继续有效，本节只补 v5 的四处裁决。

### 12A5.1 裁决 D：抓取主按钮三态（scrape-start-btn）

| 态 | 文案 | 类名 | aria-pressed | scrape-stop-btn | scrape-brief 状态串 |
| --- | --- | --- | --- | --- | --- |
| idle | 开始抓取 | `btn btn-primary` | false | disabled | 空闲 · 已入库 N 条 |
| （点击瞬间） | 启动中… | `btn btn-primary`（disabled 650ms） | — | — | — |
| running | 暂停抓取 | `btn btn-pause`（警示色，回主色前不叠 `btn-primary`） | true | 可用 | 抓取中 · 已入库 N 条… |
| paused | 继续抓取 | `btn btn-primary`（回主色） | false | 可用 | 已暂停 · 已入库 N 条 |

- 循环：idle →(点)→ running →(点)→ paused →(点)→ running →…；scrape-stop-btn 任意时刻 → idle。
- 警示色令牌：.btn-pause { border-color: rgba(240,198,100,.46); background: var(--warn-soft,rgba(240,198,100,.16)); color: #f0c674 }，浅色主题覆盖 [data-theme='light'] .btn-pause { color:#92650a; background: rgba(176,124,0,.12); border-color: rgba(146,101,10,.4) }。

### 12A5.2 裁决 E/F（v9 裁决 N 修订）：自动投递状态条（`auto-apply-bar`）

- **位置**：整行宽度，默认在右栏 `aside.chat-rail` 内、`.rail-head`（Boss 消息标题行）**之上**，`margin: 8px 10px 2px`。Boss 消息标题行不再放投递按钮。
- **展开时搬家**：对话框展开（.chat-rail 被 display:none）时，placeAutoBar() 把**同一段 DOM** 用 insertBefore 搬进 .chat-detach-head（chat-detach-sub 之后），收起时搬回右栏首位。**只搬节点、不复制**，data-testid 全站唯一。JS 里对应的唯一性守卫是 data-initial-label="start" / data-initial-label="stop" / data-default-mode="idle" 三个属性各出现 1 次（_smoke.mjs A25 断言）。
- **三态**（状态机 state.autoApply）：

| `data-mode` | 按钮数 | 状态标签文案（`auto-apply-state`） | 状态点 | 主按钮文案 | 主按钮类名 | 结束按钮 |
| --- | --- | --- | --- | --- | --- | --- |
| `idle` | 1 | **未开始投递** | 灰 `is-idle`（`var(--text-3)`） | 开始自动投递 | `btn btn-sm btn-primary` | 隐藏（`hidden` + `[data-mode='idle'] … {display:none}`） |
| `running` | 2 | **投递中** | 主色绿 `is-running`（`var(--primary)` + `--primary-soft` 光晕） | 暂停 | `btn btn-sm btn-pause`（警示色） | 露出：结束 `btn btn-sm btn-danger` |
| `paused` | 2 | **已暂停投递** | 警示琥珀 `is-paused`（`var(--warn, #d29922)`） | 继续 | `btn btn-sm btn-primary`（回主色） | 露出：结束 `btn btn-sm btn-danger` |

- 转移：`idle` →(点主按钮)→ `running` →(点)→ `paused` →(点)→ `running` →…；`running`/`paused` 任意时刻 →(点「结束」)→ `idle`。**idle 永不出现「结束」**。
- **状态标签**：`auto-apply-state` 挂 `aria-live="polite"`，文案随 `data-mode` 切换（见上表），并同步互斥地挂 `is-idle` / `is-running` / `is-paused` 三色 class（状态点 `::before`）。点色令牌：`is-idle` 灰 `var(--text-3)`、`is-running` 主色绿 `var(--primary)`（外圈 `--primary-soft` 光晕）、`is-paused` 警示琥珀 `var(--warn, #d29922)`。**禁止**出现「自动投递」这类不表达状态的恒定文案（v9 裁决 N）。
- **计数**：状态条右缘（`.auto-apply-counts { margin-left: auto }`）恒显示两枚：待投递 `<b>N</b>`（`auto-apply-pending`）与 已投递 `<b>N</b>`（`auto-apply-done`），数字用 `font-variant-numeric: tabular-nums`。原型期取数来源 = `MOCK_DATA.jobs`：`flag === 'pending'` 或无 `flag` 计入待投递，`flag === 'delivered' || flag === 'deliver'` 计入已投递，再加本次会话增量 `state.autoPending` / `state.autoDone`；接真实数据时只替换 `autoCounts()` 一处。

### 12A5.3 裁决 G：删图 1/2/3/5，图 4 不许溢出

| 图 | 内容 | 处置 | 复验命令命中数 |
| --- | --- | --- | --- |
| 图 1 | 重新打分 + `已入库 N 条` | `.scrape-tool` 整段 DOM + 样式 + `renderList()` 写入一起删；入库数只在 `.scrape-brief` | `scrape-rescore-btn` = 0、`scrape-ingested` = 0 |
| 图 2 | 顶栏 `招聘网站` 标签 | `.topbar-label` 元素 + 两条 CSS 删 | `topbar-label` = 0 |
| 图 3 | 原型自检条 | `proto-toolbar` 整块 + `.proto*` CSS + `syncProtoToolbar()` + `wireRadioGroup` 调用 + `__PROTO__` 导出一起删；**保留** `?c=` / `?l=` / `?r=` URL 自检入口 | `proto-toolbar` / `proto-list-*` = 0 |
| 图 5 | 与图 1 同行的按钮组 | 随图 1 消失 | 同上 |
| 图 4 | 配置弹窗 | 版式冻结，不许溢出：.cfg-body { overflow-y:auto; overflow-x:hidden; min-width:0 }、.config-card { flex: 0 0 auto; min-width:0 }、.textarea { box-sizing:border-box; max-width:100%; overflow-x:hidden } | _smoke.mjs 三窗行追加 cfg-body{no-x-scroll:ok} config-card{flex-shrink:0:ok} |

### 12A5.4 断言（_smoke.mjs / _modes.mjs / _tri.mjs 三个脚本，全部本地可跑）

- **A25–A30（静态唯一性与位置）**：`data-initial-label="start"` / `"stop"` / `data-default-mode="idle"` 各 1 处；状态条在 `chat-rail` 之后、`.rail-head` 之前；`chat-rail-auto-greet-entry-btn` 不得出现；`.scrape-bar` 内无投递按钮；计数用 `tabular-nums`；`[data-mode='idle'] [data-testid='auto-greet-stop-btn'] { display: none }` 存在。
- **G（溢出防护）**：`.cfg-body` 必须 `overflow-x: hidden`；`.config-card` 必须 `flex: 0 0 auto`；`.textarea` 必须 `overflow-x: hidden`。
- **_modes.mjs 行为三态**（`_states.mjs` 提供最小 DOM 桩，点同一颗按钮逐态核对）：抓取 `idle→running→paused→running`、投递 `idle→running(暂停+结束)→paused(继续+结束)→idle`，逐态核对文案 / 类名 / `aria-pressed` / 状态串。
- **_tri.mjs**：同款三态走查（独立脚本，便于单跑），并断言**投递状态标签三态**（`未开始投递` / `投递中` / `已暂停投递` 与点色 class 互斥）与**大模型打分点击无效**（见 12A5.6）。

### 12A5.5 人工走查（本机无 headless，必须目视）

1. 打开 `prototypes/ui-v1/index.html?mode=default-chat`：右栏顶部应是一条「● 未开始投递  [开始自动投递] …… 待投递 N　已投递 N」，**读得出当前状态**；Boss 消息标题行**不带**投递按钮。
2. 点「开始自动投递」：标签变「● 投递中」（主色绿点）＋ [暂停][结束]；点「暂停」→ 标签「● 已暂停投递」（琥珀点）＋ [继续][结束]；点「结束」→ 回「● 未开始投递」＋ 单按钮 [开始自动投递]，`结束` 消失。
3. 点任意 Boss 展开对话框：状态条应**整体出现在对话框头部**（开始自动投递 与 收起对话框 › 同一行），右栏消失；收起后状态条回右栏顶部，态与计数不变。
4. 点「开始抓取」：按钮变「启动中…」（短暂禁用）→ 暂停抓取 琥珀色；再点 → 继续抓取 主色；scrape-brief 依次为 抓取中… / 已暂停… / 空闲…。
5. 顶栏不应有 招聘网站；列表工具行不应有 重新打分 / 已入库 N 条；页面不应有原型自检条。
6. `?mode=default-config` 打开配置弹窗：固定高度，**v6 起只剩三张卡**（大模型 / 提示词 / 技能），卡片不被压扁，只能纵向滚动，正文没有横向滚动条。
7. **大模型打分**（顶栏分段控件左侧那颗）：必须是灰化禁用态（约 45% 不透明、`cursor: not-allowed`、hover 不反色），鼠标点击 / Tab 聚焦后回车 / 任何 URL 参数都**不得**让它变成绿色选中态；右侧「规则打分」始终保持绿色选中。悬停应出现提示「大模型打分暂未开放，当前只支持规则打分」。
8. 顶栏右端不应出现「自动打招呼 关」这枚统计 chip（v9 裁决 M 已删除）。

### 12A5.6 裁决 O（v9）：大模型打分不可选中（图 1 回归防线）

| 项 | 规格 |
| --- | --- |
| DOM | `<button class="seg" role="radio" aria-checked="false" aria-disabled="true" disabled title="大模型打分暂未开放，当前只支持规则打分" data-testid="scrape-mode-llm-seg">大模型打分</button>` |
| 永远不可选中 | 点击 / 键盘回车 / `?mode=` 类 URL 参数三条路径都不得把 `aria-checked` 置 `true`，也不得挂 `is-active` |
| 对端 | `scrape-mode-rule-seg` 恒为 `aria-checked="true"` 且保持绿色高亮 |
| 事件层兜底 | `wireRadioGroup(root, sel, onPick)` 的 click 处理器在「btn 或 root 判空」的 return 之后紧跟「btn.disabled 或 aria-disabled 为 true」的 return（源码见 `_smoke.mjs` A44） |
| 事件层兜底 | `wireRadioGroup` 的 click 处理器有两道 return：先判 btn 与 root 为空，再判 btn 禁用（源码形态与断言见 `_smoke.mjs` A44） |
| 断言 | `_smoke.mjs` **A43**（标签含 `disabled` + `aria-disabled="true"` + `aria-checked="false"`、不含 `is-active`、有 title、rule 段 `aria-checked="true"`、该 testid 全文 ≤2 处）、**A44**（禁用态四条 CSS + `wireRadioGroup` 的 return 守卫）；`_tri.mjs` 用 DOM 桩 `fire(llmSeg)` 后断言 `aria-checked` 仍 `false`、无 `is-active`，且 `ruleSeg` 仍 `true` + `is-active` |
| 反向验证 | 已实测 5 个变异全部被捕获：恢复「自动打招呼」→A41；标签退回「自动投递」→A42；去掉 `disabled`→A43；静态挂 `is-active`→A43；删 `wireRadioGroup` 的 return→A44 |

**注意**：源码从未给 `.segmented` 绑 `wireRadioGroup`（该控件当前是静态演示），因此 `_tri.mjs` 打印里会明确写「分段控件委托捕获 0 个（源码未绑 .segmented，禁用由 DOM `disabled` 兜底）」——这不是漏绑定，禁用语义由 DOM 属性 + CSS 双重保证。

---

## 12A6. 顶栏与配置弹窗收敛（v6：裁决 H / I / J）

> 用户逐字：「移除1、2」「铭感库 放到规则打分的右边。」「token计数放到靠近配置」（「铭感库」= 敏感库）。

### 12A6.1 裁决 H：配置弹窗删掉两张卡

| 删 | 原 testid | 处理 |
| --- | --- | --- |
| 图 1「token 用量」整卡 | `config-token-section` / `config-token-toggle` / `config-token-usage` | 整卡删除（读数改到顶栏，见 12A6.3） |
| 图 2「高级」整卡（默认收起） | `config-advanced-section` / `config-advanced-toggle` | 整卡删除，连带三个子卡 |
| ↳ 抓取默认参数 | `config-scrape-section` / `-toggle` / `config-keywords` / `config-max-jd` / `config-min-score` / `config-llm-prefilter` | 全部删除 |
| ↳ 打分口径 | `config-score-section` / `-toggle` / `config-score-max` / `config-score-min` | 全部删除 |
| ↳ 数据目录与库概览 | `config-data-section` / `-toggle` / `config-data-dir` / `config-db-total` | 全部删除 |

- **真的删**，不是折叠：19 个字段全文命中 0。
- **后端不变**：`set_limit` / `get_scoring_spec` / 数据目录查询等命令照旧存在，只是没有界面入口；字段定义备份在 `docs/ui/UI-SPEC-chat-detach-v3.md` §10 v6。
- **弹窗尺寸不变**：`.config-modal` 仍是 `height: min(640px, calc(100vh - 96px))`，内容变少**不收缩**。
- 委托分支同步收窄：`toggleConfigSection()` 只剩 `config-llm-toggle` / `config-prompt-toggle` / `config-skill-toggle`（不留死 case）。

### 12A6.2 裁决 I：敏感库入口挪到「规则打分」右侧

- `sensitive-open` 从「token 统计之后」移到 `.segmented`（`大模型打分   规则打分`，`scrape-mode-segmented`）**紧右侧**，同处 `.nav-mid` 分组。
- testid 不变 `sensitive-open`；`.nav button[data-testid='sensitive-open']` 的 `height/padding/font-size` 规则照旧生效，视觉重量仍是 `btn-sm`。
- 语义理由：打分模式 ↔ 打分结果记录（敏感库），同一组相邻。

### 12A6.3 裁决 J：token 计数靠近「配置」

- `.tb-stats`（累计 token / 模型打分 / 打招呼 / 自动打招呼）与窄窗折叠 chip `token-total-chip` **整组搬进** `.tb-tail`，位于 `shell-config-btn` **紧左边**。
- `.tb-tail` 加 `margin-left: auto` 顶到右端；折叠规则（`<1360` 折叠成 chip、`≥1900` 展开）原样随组搬迁，**没有第二处读同一数据**。
- 读数唯一性：`data-testid="token-total"` **1 份**、`data-testid="token-total-chip"` **1 份**（chip 是窄窗折叠态，同一组 DOM，不重复读）。
- 「自动打招呼 关」原本只在 `.tb-stats` 里，随组一起搬，未新增第二份。

### 12A6.4 断言增量（`_smoke.mjs` A31–A33）

| 编号 | 断言 |
| --- | --- |
| **A21（v6 反转）** | 19 个被删字段全文命中 0 |
| **A31** | 字符串索引 `scrape-mode-segmented` < `sensitive-open` < `class="tb-stats"` < `shell-config-btn` |
| **A32** | `token-total` 与 `token-total-chip` 都落在 `tb-stats` → `shell-config-btn` 区间内 |
| **A33** | `token-total` 唯一 1 份、`token-total-chip` 唯一 1 份、`.tb-tail` 有 `margin-left: auto` |

当前实测输出：`SMOKE OK · … · v6: A31–A33 seg@67902 < sensitive@68298 < stats@68524 < config@69191`。

### 12A6.5 人工走查

1. 1440×900 打开 `index.html`：顶栏从左到右应为 —— 平台卡 · `大模型打分   规则打分` `敏感库` ……（空白）…… `累计 103.8K token · 模型打分 · 打招呼 · 自动打招呼` `配置` `☀ 浅色`。
2. 缩到 1280 宽：token 统计折叠成一枚 chip，位置仍在「配置」左边，不与平台卡重叠、顶栏不换行。
3. 点「配置」：弹窗 640px 高，只有 大模型 / 提示词 / 技能 三张卡 + 底部说明与取消·保存；没有任何「高级」箭头、没有 token 卡。
4. 点「敏感库」：浮层正常打开（入口在打分分段控件右侧，不再在顶栏中段）。
5. 全文搜索 `config-keywords` / `config-max-jd` / `config-min-score` / `config-data-dir` / `config-db-total` / `config-token-usage`：均为 0。

---

## 12A7. 1920×1080 设计基准与等比缩放（v7：裁决 K）

> 用户逐字：「窗口默认分辨率1920x1080，调整字体大小，使用标准设计」「超出默认分辨率的等比放大」

### 12A7.1 基准与公式

- **设计基准**：1920×1080、系统显示缩放 100%。所有 `--fs-*` 括号里的像素值都是这个基准下的值。
- **单一缩放变量**：`:root { --ui-scale: 1; }`，由 `syncUiScale()` 在**启动**与**每次 resize** 时写入 `<html>` 行内 style。
- **公式**：`--ui-scale = clamp(1, innerWidth / 1920, 1.5)`，取值四舍五入到 3 位小数。

| 窗口宽 | 1200 | 1280 | 1440 | 1600 | 1920 | 2560 | 3840 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `--ui-scale` | 1 | 1 | 1 | 1 | 1 | **1.333** | **1.5**（2.0 被 clamp 封顶） |

- **边界必须写明**：`≤1920 一律 1`，即 1280 / 1440 / 1600 都是 **1**，**只放大不缩小**；`≥1920` 线性放大；`≥2880` 封顶 1.5，避免 4K 上字号被撑到 24px 以上。

### 12A7.2 字号令牌映射表（1920 基准值 × `--ui-scale`）

> **v8（裁决 L「统一字体大小，按标准设计」）：旧 9 档已废止。** v7 曾保留全部手写值（10 / 10.5 / 11 / 11.5 / 12 / 12.5 / 13 / 14 / 16），相邻档只差 0.5–1px、肉眼无从分辨，反而显得大小不统一；v8 收敛为标准阶梯 **5 档**，全站**只允许这 5 个名字**。下面这张旧表仅作历史对照，**以本节末的 5 档表为准**。

| ~~旧令牌~~ | ~~基准值~~ | ~~处置（v8）~~ |
| --- | --- | --- |
| ~~`--fs-3xs` 10px~~ / ~~`--fs-2xs` 10.5px~~ / ~~`--fs-xxs` 11px~~ | 10 / 10.5 / 11px | 全部并入 **`--fs-2xs` = 11px** |
| ~~`--fs-xs` 11.5px~~ | 11.5px | 并入 **`--fs-xs` = 12px**（辅助类）或 **`--fs-sm` = 13px**（正文类） |
| ~~`--fs-sm` 12px~~ / ~~`--fs-base` 13px~~ | 12 / 13px | 全部并入 **`--fs-sm` = 13px** |
| ~~`--fs-md` 12.5px~~ | 12.5px | 辅助类 → `--fs-xs`；标题类 → **`--fs-md` = 15px** |
| ~~`--fs-lg` 14px~~ / ~~`--fs-xl` 16px~~ | 14 / 16px | 并入 **`--fs-lg` = 20px**（页面级标题）；`--fs-xl` 直接删除 |

### 12A7.2b 标准 5 档（v8 生效口径）

| 令牌 | 基准值 | 语义 | 典型使用处 | 引用数 |
| --- | --- | --- | --- | --- |
| `--fs-2xs` | **11px** | 说明 / 时间戳 / 微注 | `.field-hint` / `.f-hint` / `.fg-label-sub` / `.friend-time` / `.sen-time` / `.tab-count` / `.rail-cap` / 角标（`.score-box i`、`.collapse-caret`、未读点、气泡时间）/ `.rail-stat` / `.filter-note` / `.filter-note` / `.sen-reason` | 20 |
| `--fs-xs` | **12px** | 次要正文 | `.card-sub` / `.card-sub-inline` / `.job-meta` / `.friend-msg` / `.chat-detach-sub` / `.dl-row` / `.cfg-field > .label` / `.log-row` / `.sen-sub` / `.sen-count` / `.sen-selinfo` / `.filter-sec-title` / `.f-count` / `.f-all` / `.f-subhead` / `.f-cell-label` / `.batch-hint` / tab / `.testid:sensitive-open` / 输入框 | 31 |
| `--fs-sm` | **13px** | 正文基准 | `body` / `.job-title` / `.job-salary` / `.friend-name` / `.btn` / `.btn-sm` / `.input` / `.textarea` / `.bubble`（聊天消息）/ `.cfg-value` / `.state-sub` / `.empty` / `.sen-job` / `.filter-label` / `.drawer-body` / `.app-error-text` | 41 |
| `--fs-md` | **15px** | 小标题 | `.card-title` / `.rail-title` / `.config-title` / `.f-title` / `.state-title` / `.score-box b`（打分数字） | 6 |
| `--fs-lg` | **20px** | 页面级标题 | `.brand`（品牌名）/ `.sen-head-title`（弹窗大标题「敏感库」） | 2 |

配套行高：**正文 1.55**、**小字 1.5**、**标题 1.3**；`.job-title` 因为要挤两行，单列 1.4。间距微调只动了两处明显被压扁的：`.dl-row` 由 `padding: 3px 0` 回收为 `5px 0`（13px 字号下原间距太挤）、`.f-textarea` 由 `min-height: 46px / padding 6px 10px` 调整为 `50px / 8px 10px`。

**保留的硬编码字号：0 处** —— 连 `html`/`body` 基准也写成 `font-size: var(--fs-sm)`，令牌定义自身是 `calc(Xpx * var(--ui-scale))`，不计入硬编码。

### 12A7.3 哪些比例类令牌参与缩放

| 令牌 / 属性 | 基准写法 | 缩放后 |
| --- | --- | --- |
| `--rail-w`（右栏宽） | 300px | `calc(300px * var(--ui-scale))`；1280 档 `calc(260px * var(--ui-scale))` |
| `--rail-gap` | 12px | `calc(12px * var(--ui-scale))` |
| `--radius-sm` / `--radius` / `--radius-lg` | 10 / 16 / 22px | 全部 `calc(… * var(--ui-scale))`；`--radius-pill: 999px` 不缩放（本来就是胶囊） |
| `--blur-0…3` / `--blur-pop`（明暗两套主题都改） | 18/24/18/12/30px | 全部 `calc(… * var(--ui-scale))` |
| `.decisions-modal` | `height: min(620px, 100vh−96px)` | `min(calc(620px * scale), calc(100vh − 96px))` |
| `.config-modal` | `height: min(640px, 100vh−96px)` / `width: min(760px, 100vw−64px)` | 同法乘 scale |
| `.sensitive.floater` | `height: min(720px, 100vh−56px)` / `width: min(1120px, 100vw−40px)` | 同法乘 scale |
  drawer（旧抽屉，未在设计集内）   `width: min(460px, 100vw−40px)`   同法乘 scale  

### 12A7.4 与既有裁决的兼容（**不得破坏的三条**）

1. **`.chat-detach` = 50vw 不变**：`syncDetachWidth()` 仍是 `Math.min(Math.max(innerWidth * 0.5, 640), innerWidth * 0.8)`，`--detach-w` 默认仍是 `0px`。缩放只影响「640 下限」这个**常量**的解释，不改变百分比关系 —— 1920 下依然是「对话框整体 960 + 主列 960」。实测：`A36` 断言公式原样。
2. **三窗固定尺寸 + 内部滚动不变**：三窗外框从 `min(640px, …)` 变成 `min(calc(640px * scale), …)`，**仍然是固定高度**（不是 `max-height`），在 scale=1 与 scale=1.5 下都不随内容塌陷；滚动仍只发生在 `.cfg-body` / `.sen-grid` / `.dl-rows`。实测：`A35` 断言三窗形状。
3. **断点语义保留（v9.1 更新）**：`≥1900`（统计组全展开）、`≤1599`（网格 2 列）、`≤1279`（右栏 260）、`1360–1599`（隐藏 `.rail-stat` 与分隔线）。网格列数断点从 `1279` 改为 `1599` 是**用户裁决**，其余未改

### 12A7.5 断言（`_smoke.mjs` A34–A37）

| 编号 | 断言 |
| --- | --- |
| **A34** | `--ui-scale` 五档计算值 `1200→1 / 1440→1 / 1920→1 / 2560→1.333 / 3840→1.5`；源码公式必须是 `Math.min(Math.max(innerWidth / 1920, 1), 1.5)`；`--ui-scale` 全文引用 ≥3 处 |
| **A35** | 三窗（`.decisions-modal` 620 / `.config-modal` 640 / `.sensitive.floater` 720）高度均为 `min(calc(Xpx * var(--ui-scale)), calc(100vh − k))` 形状，scale=1 与 1.5 下仍固定 |
| **A36** | `.chat-detach` 仍是 `Math.min(Math.max(innerWidth * 0.5, 640), innerWidth * 0.8)`；`--detach-w` 默认 `0px` |
| **A37** | 硬编码 `font-size: Npx` 全文命中 **0** |

实测输出：

```
SMOKE OK · … · v7: A34–A36 scale 1200→1 / 1440→1 / 1920→1 / 2560→1.333 / 3840→1.5 · 三窗固定 · detach 50vw · 硬编码字号 0 处
```

### 12A7.6 人工走查

1. 1920×1080、100% 显示：这是基准画面，正文 13px、次要 11.5px，`--ui-scale` 应为 `1`（控制台 `getComputedStyle(document.documentElement).getPropertyValue('--ui-scale')`）。
2. 拖到 1440 宽：**字号不缩小**（scale 仍为 1），只是列变窄、右栏可能落到 260。
3. 拖到 2560 宽：字号与圆角、弹窗尺寸、右栏宽一起放大 ~33%；三窗仍是固定高度、只有内部滚动条出现。
4. 拖到 3840 宽：放大封顶在 1.5（字号最大 19.5px），不会继续膨胀。
5. 任何档位下点任意 Boss 展开对话框：对话框整体仍 = 窗口宽 50%，主列仍 ≥ 约 47%。

### 12A7.7 断言增量（v8：A38–A40）

| 编号 | 断言 | 脚本 |
| --- | --- | --- |
| **A38** | 字号令牌定义恰好 **5 条**：`calc(11px…)` / `12px` / `13px` / `15px` / `20px`，出现第 6 档即失败 | `_smoke.mjs` |
| **A39** | 全站 `var(--fs-*)` 引用集合 ⊆ 这 5 个名字（枚举后逐个比对，未定义的引用即失败） | `_smoke.mjs` |
| **A40** | `--fs-3xs` / `--fs-xxs` / `--fs-base` / `--fs-xl` 残留命中 **0**（定义与引用都算） | `_smoke.mjs` |

实测输出：

```
SMOKE OK · … · v8: A38–A40 tiers 2xs=11px / xs=12px / sm=13px / md=15px / lg=20px · refs 5 个 · 废止档位残留 0
```

### 12A7.8 人工走查（v8 字号统一）

1. 1920×1080 下目视整页：只应看到三种「明显不同」的文字大小 —— 11px 微注、12px 辅助、13px 正文，再加 15px 小标题与 20px 品牌名。
2. 顶栏在 **1280** 与 **1920** 两档都必须是**单行**（不得因字号变大折成两行）；若折行，按裁决 L 第 4 条把次要文本降一档，**不许加宽断点**。
3. 投递状态条（待投递 / 已投递计数）、Boss 列表条目、聊天消息三处读起来不应出现「半号字」错位感。
4. 拖到 2560 / 3840：5 档整体等比放大到 ×1.333 / ×1.5，档位数量不变、相对关系不变。
5. 三窗（决策记录 / 配置 / 敏感库）仍是固定高度 + 内部滚动，字号变大后不出现横向溢出。

## 12A8. v9 裁决 M / N / O（状态语义化与打分模式禁用）

> 用户逐字：「移除：自动打招呼 关」「自动投递改为描述当前投递状态」「大模型打分不能选中，选中后图1」。详见冻结规格 `docs/ui/UI-SPEC-chat-detach-v3.md` §14。

| 裁决 | 落地 | 断言 |
| --- | --- | --- |
| **M** 删除「自动打招呼 关」 | 右栏 `.rail-stat` 该条删除；`token-total-chip` 的 title 去掉该段 | A41（全文命中 0） |
| **N** 状态条描述状态 | `auto-apply-state` 三态文案 未开始投递 / 投递中 / 已暂停投递 + `is-idle`/`is-running`/`is-paused` 三色点 + `aria-live="polite"` | A42 + `_tri.mjs` T1 |
| **O** 大模型打分不可选中 | `scrape-mode-llm-seg` 加 `disabled` + `aria-disabled="true"` + `aria-checked="false"` + title；禁用态 CSS 四规则；`wireRadioGroup` 的 click 守卫 return | A43 / A44 + `_tri.mjs` T2 |

**走查**：`?mode=default-chat` 打开，确认顶栏分段控件左侧「大模型打分」灰化、悬停出现提示、点击/回车都不变色；右栏状态条初始显示「● 未开始投递」，点「开始自动投递」→「● 投递中」+[暂停][结束]，点「暂停」→「● 已暂停投递」+[继续][结束]，点「结束」回单按钮；顶栏右端不再有「自动打招呼 关」。

## 12A9. v9.1 / v9.2（投递卡片列数断点 / 删批量提示语）

> 用户逐字：「小于1600px时，投递卡片缩减为2列」；「移除：勾选 2 条以上可批量标记；单条直接用行内标签」。冻结规格见 `docs/ui/UI-SPEC-chat-detach-v3.md` §15。

### 12A9.1 投递卡片列数断点 = 1600

| 窗口宽（CSS px） | `.job-grid` 列数 | 命中规则 |
| --- | --- | --- |
| < 1200 | 2（保持 1200 基线并出现横向滚动条，不降级到 1 列） | `max-width: 1599px` |
| 1200 – 1599 | 2 | `max-width: 1599px` |
| ≥ 1600 | 3 | 基础 `repeat(3, minmax(0, 1fr))` |

- 全站把 `.job-grid` 降到 2 列的媒体查询**只有一条**（原 `max-width: 1279px` 那条已合并删除，避免重复声明）。
- **列数只随窗口 CSS 像素断点变化**，不随 `--ui-scale`：2560 / 3840 下仍是 3 列；`.job-grid` 规则内不得出现 `var(--ui-scale)`。
- 断言 **A45**（`_smoke.mjs`）：基础 `repeat(3, minmax(0, 1fr))`、2 列规则唯一且断点为 `1599px` 无 `min-width`、列数推断 `1200→2 / 1440→2 / 1599→2 / 1600→3 / 1920→3`、`.job-grid` 不含 `ui-scale`。

### 12A9.2 删除批量标记提示语（功能保留）

| 删除 | 保留 |
| --- | --- |
| `job-batch-hint` 节点 + `.batch-hint` 样式 + `renderBatch()` 里对它的取值与 `hint.hidden` 赋值 | `job-batch-actions`（已选 ≥ 2 才出现，`actions.hidden = n < 2`）、`job-batch-reject-btn`「不投递（N）」、`job-batch-allow-btn`「允许投递（N）」、单条 `data-flag` 行内标签入口 |

- 提示语消失后 `.batch-actions` 靠自身 `margin-left: auto` 独占右缘，不存在两个右浮动元素对撞。
- 断言 **A46**（`_smoke.mjs`）：`勾选 2 条以上` / `job-batch-hint` / `.batch-hint` 命中 0；三个批量 testid 都在；`actions.hidden = n < 2`；`data-flag` 在；`.batch-actions` 带 `margin-left: auto`。
- 反向验证已实测：加回 hint 节点 → A46 捕获；`data-flag` 改名 → A46 捕获；断点改回 1279 → A45 捕获。

## 12A10. 投递卡片（v10）

> 真源：`docs/ui/UI-SPEC-chat-detach-v3.md` §16。本节只记录状态与断言，重复裁决原文会把两处口径拖歪。

### 12A10.1 卡片信息层级（最终形态，W2）

```
┌ 高级前端工程师 ........................... 25-35K ┐
│ 星瀚科技 · B 轮                                    │
│ 杭州 · 5-10年 · 分布式 / 高并发                    │
│ 杭州市余杭区文一西路 969 号 · 星瀚大厦 12F ┌──────┐│
│ [92分]                                     │打招呼││
└────────────────────────────────────────────┴──────┘
```

- 前三行：标题 + 薪资 / 公司 · 轮次 / 城市 · 经验 · 标签，保持原样。
- 第 4–5 行合并成一个 `.job-info` 网格（`grid-template-columns: minmax(0, 1fr) auto`，`column-gap: 8px`）：左列第 1 行 `.job-addr`（单行省略，`title` 存全文）、左列第 2 行 `.score-box`；右列 `.job-greet` 跨两行（`grid-row: 1 / span 2`）。
- `.job-greet` 高度 = `calc(var(--fs-sm) * 1.55 * 2 + 12px)`（两行正文），`min-width: calc(var(--fs-sm) * 4.6)` 保证「打招呼」三字横排，`aspect-ratio: 1 / 1` 只在文字不被压缩时生效 —— **可读性优先于正方形**，禁止竖排。
- 高亮：`background: var(--primary)` 实底 + 白字 + `border: 0` + `border-radius: var(--radius-sm)`，hover 仅 `filter: brightness(1.08)`，`focus-visible` 有 2px 主色环。

### 12A10.2 已移除的单条入口

| 已移除 | 原 testid | 替代途径 |
| --- | --- | --- |
| 不投递 | `job-reject-<jobId>` | 详情抽屉 `job-detail-reject-btn`、批量条 `job-batch-reject-btn` |
| 允许投递 | `job-allow-<jobId>` | 详情抽屉、批量条 `job-batch-allow-btn` |
| 已投递标记（√） | `job-deliver-<jobId>` | 详情抽屉 `job-detail-deliver-btn` |

卡片现在**只有一颗按钮**：`job-greet-<jobId>`（打招呼）。批量入口的显隐规则不变（已选 ≥ 2 才出现，提示语已按 v9.2 删除）。

### 12A10.3 选择模式状态机

| 态 | 触发 | 表现 | 退出 |
| --- | --- | --- | --- |
| `selecting: false` | 默认 | 勾选框 `.job-check-slot` 不显示；单击卡片打开详情抽屉 | 长按任意卡片 500ms → `true` |
| `selecting: true` | `pointerdown` 持续 500ms 且位移 ≤ 8px | `.job-grid.is-selecting`，勾选框出现；单击卡片切换勾选 | 已选归零 / 点空白 / 批量动作完成 → `false` |

- 长按取消：`pointermove` 位移 > 8px、`pointerup`、`pointerleave`、`pointercancel` 任一发生即 `lpCancel()`。
- 长按与打招呼互不干扰：`pointerdown` / `click` 委托都跳过 `button`、`input`、`[data-greet]`。

### 12A10.4 断言（A47–A51）

| 编号 | 判据 |
| --- | --- |
| A47 | 8 条 mock 岗位 `addr` 非空；`.job-addr` 有 `overflow: hidden` + `text-overflow: ellipsis` + `white-space: nowrap`；勾选框槽默认 `display: none` |
| A48 | 卡片渲染 `job-greet-*` 且带 `data-greet`；`greetFromJob()` 依次 `openChatDetach()` → `pickFriend(uid)` → 写 `state.greetDraft` → 预填输入框（**不自动发送**）；委托里 `[data-greet]` 先于 `switch (tid)` 判定 |
| A49 | `syncDetachWidth()` 源码含 `Math.max(innerWidth * 0.5, 600)`；1200 档对话框与主列占比都落在 49.5%–50.5% |
| A50 | `jobCardHtml()` 函数体内 `job-reject-` / `job-allow-` / `job-deliver-` 各 0 次、`job-greet-` 恰 1 次；`job-batch-reject-btn` / `job-batch-allow-btn` / `job-detail-deliver-btn` 仍在；`flag === 'reject'` 或 `flag === 'deliver'` 判定仍在 |
| A51 | `.job-info` 首列 `minmax(0, 1fr)`、次列 `auto`；`.job-greet` 含 `grid-row: 1 / span 2`、`grid-column: 2`、`var(--primary)`、`aspect-ratio: 1 / 1`、`min-width: calc(var(--fs-sm) * 4.6)`、`height: calc(var(--fs-sm) * 1.55 * 2 + 12px)`；不带 `btn-sm`；不得残留 `.job-foot { grid-template-columns: auto 1fr }` |

反向验证（已实测）：加回 √ → A50 `FAIL`；把 `.job-greet` 挪回底部横排（改回 W 口径）→ A51 `FAIL`；下限改回 640 → A49 `FAIL`。

### 12A10.5 人工走查（6 步）

1. 打开原型，卡片第 4 行是地址、第 5 行是分值，右侧一个主色方块写着「打招呼」；卡片上**没有** √ / 不投递 / 允许投递。
2. 鼠标悬停方块 → 变亮；`Tab` 走到方块 → 出现 2px 主色焦点环。
3. 点方块 → 右侧对话框展开、对应 Boss 被选中，输入框已预填招呼语且**未发送**。
4. 普通单击卡片任意空白 → 打开详情抽屉（不进入选择模式）。
5. 长按卡片 500ms → 出现勾选框；再点第二张 → 批量条出现两颗按钮（不投递 / 允许投递）。
6. 取消全部勾选 → 勾选框消失、退回普通态。

## 12A11. 展开态列数联动（v10 裁决 X）

> 真源：`docs/ui/UI-SPEC-chat-detach-v3.md` §16.9。用户原话：「投递列表在聊天框展开时自动减少1列」。

### 12A11.1 规则表

| 窗口宽 | 收起态（Boss 列表常驻） | 展开态（`.app-shell.is-chat-open`） |
| --- | --- | --- |
| < 1600px | 2 列 | **1 列** |
| ≥ 1600px | 3 列 | **2 列** |

```css
.job-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
@media (max-width: 1599px) { .job-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
/* 必须在上面两条之后：顺序反了就被 ≤1599 的 2 列吃掉 */
.app-shell.is-chat-open .job-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
@media (max-width: 1599px) {
  .app-shell.is-chat-open .job-grid { grid-template-columns: minmax(0, 1fr); }
  .app-shell.is-chat-open .job-card { min-width: calc(320px * var(--ui-scale)); }
}
```

约束：
- 列数只由「窗口断点 + 展开态」决定，**不随 `--ui-scale`** 变化（2560 / 3840 仍是收起 3 / 展开 2）。
- 展开/收起切换**不加过渡动画**（`.job-grid` 规则内不得出现 `transition`），避免卡片重排闪动。
- 单列时卡片最小宽度 `320px × --ui-scale`；窗口过窄允许出横向滚动条，**不得**退到 0 列或隐藏卡片。
- `< 1200px` 仍按 1200 基线走，展开态同样按上表 −1。

### 12A11.2 断言（A52）

| 判据 |
| --- |
| 收起态基础 `.job-grid` 为 `repeat(3, minmax(0, 1fr))` |
| `@media (max-width: 1599px)` 收起态为 `repeat(2` |
| `.app-shell.is-chat-open .job-grid` 为 `repeat(2, minmax(0, 1fr))`（≥1600 展开态） |
| `is-chat-open` 的单列声明出现在 ≤1599 断点内，且整块 `indexOf` 位置**晚于**断点规则（顺序敏感） |
| 展开态规则内不含 `transition` |

推断表（由 `_smoke.mjs` 打印）：`1200 收起2/展开1 · 1440 收起2/展开1 · 1599 收起2/展开1 · 1600 收起3/展开2 · 1920 收起3/展开2`。

反向验证（已实测）：把 `is-chat-open` 规则整块挪到断点规则之前 → `SMOKE FAIL · A52 展开态规则写在断点规则之前（会被 ≤1599 的 2 列覆盖，顺序敏感）`。

### 12A11.3 行为侧

`_tri.mjs` 在 DOM 桩里取不到计算样式，因此行为侧只断言两件事：`openChatDetach()` 后 `.app-shell` 带 `is-chat-open`，`closeChatDetach()` 后该 class 消失；列数归属由上面的静态同源断言（A52）兜底，人工走查见 12A11.4。

### 12A11.4 人工走查（5 步）

1. **1200 宽**（最小档）打开原型（收起态）→ 投递列表 2 列。
2. 点任意 Boss → **投递列表 1 列**、**对话框整体 600px = 窗口 50%**（左 Boss 列表 260 + 右聊天区 340），卡片可用宽约 562px，无横向滚动条。
3. **1440 宽**重复 2 → 仍 1 列，对话框 720px = 50%（300 + 420），卡片可用宽约 682px。
4. 点「收起对话框 ›」→ 立刻回到 2 列、对话框宽度归 0、右栏回来，无重排抖动。
5. 把窗口拉到 **1600 / 1920** 再重复 → 收起 3 列 / 展开 **2 列**（每卡 ≥ 320px，1600 档约 376px）。

### 12A11.5 源码级兜底（A53 / A54，替代截图）

截图因本机 headless 浏览器在沙箱下崩溃而放弃（用户裁决「暂不处理」），两个视觉承诺改为静态断言：

- **A53**：`.job-info` 为 `minmax(0, 1fr) auto`；`.job-addr` `grid-row: 1`、`.score-box` `grid-row: 2`、`.job-greet` `grid-row: 1 / span 2`；`.job-greet` 含 `height: calc(var(--fs-sm) * 1.55 * 2 + 12px)`、`aspect-ratio: 1 / 1`、`min-width: calc(var(--fs-sm) * 4.6)`、`background: var(--primary)`、`border: 0`、白字；不含 `btn-sm`、不含 `transition`。
- **A54**：≤1599 展开态存在 `.job-card { min-width: calc(320px * var(--ui-scale)) }`；`.job-grid` 与展开态 `.job-grid` 都不得用 `overflow-x: hidden` 掩盖溢出；打印推断表并断言 `1200 展开1列≈562px`、`1440 展开1列≈682px`、`1600 展开2列≈376px` 均 ≥ 320px。
- 反向验证（已实测）：`.job-greet` 的 `grid-row: 1 / span 2` 改成 `auto` → A53 FAIL；把 `.job-card` 的 `min-width` 改成 `min-height` → A54 FAIL；还原后 `SMOKE OK`。
