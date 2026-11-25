# `scraper/` 交接文档（HANDOFF）

> **本文件只写「现状 / 坑 / 下一步」**，不写历史记录与用例明细。
> 目录结构、协议、红线见 [`AGENTS.md`](AGENTS.md)，跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。

**最后更新**：2026-11-25

---

## 1. 现状

抓取、登录、聊天、求职期望四条链路**运行态已通**：从 Tauri 子进程拉起真实 Chrome 已修复并实测（提交 `1f5910b`；取证与证据索引在 `%TEMP%\jt-chrome-fix\VERIFICATION.md`，t1/t2/t6/t7 各目录）。

### 1.1 Chrome 撞名阻塞已除（三个预存缺陷，别再按旧结论排查）

旧结论「WebView2 单例命名对象撞名」**是误诊**：Chromium 单例身份 = 标题为字面 user-data-dir 路径的 `Chrome_MessageWindow` 消息窗口，WebView2 UDF 与 sidecar UDF 路径不同，天然不撞。真凶是三个缺陷叠加：

1. **强杀无整树回收**：杀 python 留下的孤儿 Chrome 持单例窗，下次 launch 撞名。修：Rust 侧 Job Object `KILL_ON_JOB_CLOSE`（`backend-tauri/src/core/sidecar/process/job.rs`），spawn 即纳入，shutdown/强杀回收整树。
2. **每命令新线程 vs patchright 线程绑定**：首条命令线程退出后 `_pw/_ctx` 成尸体，第二条命令探活必炸 → 误判失效 → teardown 重启撞仍活着的 Chrome 单例窗 → `TargetClosedError`。修：常驻 `ThreadPoolExecutor(max_workers=1)` 单车道（`_browser_q`），所有浏览器命令含 `login.status` 探活与 EOF teardown 都排队到同一条线程（AGENTS §5）。
3. **launch 失败复用死连接**：`ensure_context` 失败只置 `_ctx=None` 却保留 `_pw`，下次复用已断的 driver 连接。修：launch 异常立即 `_teardown_playwright()` 再抛。

真实小批量 E2E（Java，maxJd=10，门槛 70，已登录真实 profile）：scraped=15 / inserted=7 / below_threshold=5 / rejected=3，新入库行规则五维打分字段齐全，日志无撞名 / 无 `TargetClosedError`；`scrape_close` 整树回收约 1.6s；`login_save` → `saved=true` → `logout` → `saved=false` 标记链通过。

**抓取自然完成信号已发**（本轮 fix(sidecar)）：`_run_scrape` 的全部出口（自然完成 / 关键词为空与未登录早退 / 异常 / 用户停止）都在 finally 发 `scrape.finished`（`{count,stopped,error}`；`stopped` 取 `_stop.is_set()`，`start_scrape` 每轮清 `_stop`，故为 per-run 权威）；emit 包 try/except 防 stdout 已断时 BrokenPipe 掩盖原始异常。Rust 侧消费与竞态守卫见 `backend-tauri/HANDOFF.md`。真实 E2E 自然跑完 +151s 界面自动转「空闲」（证据 `%TEMP%\jt-running\t8\RESULT-t8.json`）。

---

## 2. 坑（踩过的，别再踩）

### 2.1 环境

| 坑 | 表现 | 正确做法 |
|---|---|---|
| Windows stdout 默认 cp936 | 所有中文 JD 变乱码（`{"msg":"������..."}`） | `main()` 里必须 `sys.stdout/sys.stdin.reconfigure(encoding="utf-8", errors="replace")` |
| `print()` 调试 | Rust 判成 `Malformed`，整个协议流错位 | 日志一律走 `emit` 的 `log` 事件，stdout 只写协议 |
| 单行超 1 MB | 整条消息被替换成 error 日志 | 用 `clamp_text` 按 UTF-8 字符边界截断（不能按字节切，会切坏多字节字符） |

### 2.2 线程与生命周期

- **跨线程碰浏览器必然炸**：`Cannot switch to a different thread`。patchright 同步 API 绑死创建它的线程。
- **单车道必须常驻**：工作线程退出 = 连接死亡。所有浏览器命令（含探活、EOF teardown）submit 到 `_browser_q`，不许再每条命令起新线程。
- **launch 失败必须清 `_pw`**：`ensure_context` 的 launch 异常分支已做 `_teardown_playwright()`；不清就会复用死 driver 连接，下次探活必炸。
- **`scrape.stop` 只是置标志**，不结束进程。真正的收尾是 Rust 关 stdin → Python 的 `for raw in sys.stdin` 走 EOF → 置 `_stop`；超时才 `kill()`。
- **不关 stdin 就是「命令卡住」**：进程挂在 `for raw in sys.stdin` 等下一行，界面看起来没反应。
- **不能用 `time.sleep`**：最长 9.8 秒的反爬等待期间点停止会毫无反应。所有等待走 `interruptible_sleep`（每 0.2 秒查一次 `_stop`）。
- **抓取进行中发起聊天会排队**，要等那一轮抓取结束。这是刻意取舍。

### 2.3 选择器与解析

- **`hr_active_status` 用老选择器会静默失效**：它**不在** `.job-banner` 的 `.boss-info-attr`（该元素在本站已不存在），而在 `.job-boss-info .boss-active-time`。取不到时一旦用户勾了「HR 活跃度」规则，`exclude_hr_active_days` 会把**全部岗位**判成 `hr_active_unknown` 否决——不是「少抓」，是「全否决」。
- **取到文案必须过正则再回传**：原生文案是「刚刚活跃」这类长串，Rust 侧 `core/domain/parse.rs` 的 `HR_ACTIVE_DAYS` 表按子串匹配。改文案会另 Rust 侧用例 `scraped_variants_of_hr_activity_are_recognized` 变红——它变红就是在提醒抓取侧和解析侧脱节了。
- **卡片里公司名在 `a.boss-info > span.boss-name`，不是 `.company-name`**。
- **卡片选择器 `li` 会匹配到卡片自己**：找经验/学历标签必须限定在 `.tag-list` 里，否则 `inner_text` 拿到整张卡的文本。
- **翻页判定只看卡片数增长**：滚到底页面可能回弹，选择器一直存在，改成「滚动成功」或「等选择器出现」都会无限翻下去。每次滚动后等 2.5 秒是实测稳妥值。
- **薪资数字常被字体加密**（U+E031..U+E03A → 0-9），取不到就留空，不要在 Python 侧臆造。
- **空页有五类**（验证码/风控、登录失效、加载中、筛选无结果、真为空），`_diagnose_empty_page` 会把页面特征翻成人话。新增诊断分支时保持「告诉用户下一步该干什么」的口径。

### 2.4 监听器（`chat.py`）

`page.on("response")` 回调运行在 playwright 自己的调度栈里，**调 `resp.text()` 会炸整个会话**：

```
Response.text: Connection closed while reading from the driver
```

之后所有请求变成 `Event loop is closed` / `Sync API inside the asyncio loop`，只能重启 sidecar。正确写法：回调只把响应对象塞进队列，正文由主流程 `_drain` 读；同一 `(page, key)` 只挂一次监听器（用 `(id(page), key)` 做键，旧回调先 `remove_listener`）；**回调里绝不能抛异常**，会被调度器吞掉并连带弄坏连接。

### 2.4.1 Boss 聊天接口的真实字段（别再按猜的名字写）

点开会话「卡住」三次都是**字段名 / 选择器对不上**，实测数据如下，改动前先照这个核对：

| 接口 | 真实结构 | 踩过的错 |
|---|---|---|
| `getGeekFriendList` | 列表在 **`zpData.result`** | 写成 `zpData.geekFriendList` / `friendList` → 解析 0 行，`_friends_order` 恒空 → `_wait_friends` 白等到 8 秒才走 |
| `historyMsg` | `zpData.messages`，**响应里没有 uid** | 从响应取 `encryptUid`/`uid` → 恒空，`_msgs_raw` 从未写入。uid 必须由调用方经 `_history_handler(uid)` 绑定 |
| `geekEnter` | `zpData` 无 `myId`/`uid`/`userId` | 想从它拿「我是谁」拿不到，只能从 `historyMsg` 反推 |

消息条目的「谁发的」判据：

- 顶层 `from` / `to` 是**成员对象** `{uid,name,avatar,...}`，不是数字；`body` 里没有 `fromId`/`toId`。
- **`received` 恒为 `true`（全部 11 条实测），它是送达/已读状态，与发件人无关。** 旧代码按 `received` 判方向，导致用户看到左右颠倒。
- 唯一可靠判据：`from.uid == 自己的 uid` → `me`。自己的 uid 由 `_note_me(msgs, peer_uid)` 反推 —— `peer_uid` 是调用方给的对端，发件人不是对端时收件人就是自己。
- `type in (3,4,7)` 是简历卡片 / 系统行，`sender='system'`，不要当正文解析。

**会话列表的送达态同理**：`normalize_friend()` 给的是 `last_from_me`（方向，判据同上面 `from.uid`）与 `last_status`（`1` 已送达 / `2` 已读 / `0` 或缺失 = 不显示）。`last_status` **不要拿来判方向**，Boss 用 `0` 表示「最后一条是对方发的」。这两个字段曾整条链路丢过：sidecar 给了、Rust `Friend` 没声明、`chat_friends` 表没列，serde 与 SQL 各自静默丢弃，界面上就永远没有 `[送达]`/`[已读]`。

### 2.4.2 会话列表 DOM 与接口是两套数据，不能混用

- 接口给 **100 条**，DOM 虚拟滚动只渲染 **40 行**。
- DOM 行**拿不到 uid**，旧代码用 `dom:{i}` 造假 uid → 界面上能点，点了必然报「找不到这条会话」。**已删掉这条兜底**：拿不到接口就抛错，不再造数据。
- 行内类名以实测为准：名字在 **`.name-text`**（不是 `.name` / `.title`），最近一条在 `.last-msg-text`，时间在 `.time`。旧选择器全落空 → `name` 恒为 `''` → 只能靠公司名子串猜行。改选择器必须重跑一次真实抓取核对。

### 2.4.3 等响应的「提前返回」必须按 key 分别计

`_wait_messages` 曾把 `geekEnter` 的响应也算进「已处理」计数，而点开会话时 **`geekEnter` 先回、`historyMsg` 约 1.5 秒后才回**，于是函数在 0.08 秒就判定「没有消息」，用户看到空会话。修法：提前返回只看 `KEY_HISTORY`，`KEY_ENTER` 只负责补 myId、不参与判定。

同理，`_wait_friends` / `_wait_messages` 在「响应已处理但解析不出任何数据」时立即返回而不是耗满超时——那是字段名变了，多等只是白等。

### 2.4.4 「从未聊过的岗位」建会话：原先不存在，现已由 `ensure()` 补上

全库没有任何一条路径能「在没打过招呼的岗位上创建会话」。两处入口都撞在同一个洞上：

1. **前端占位会话**：`chat-model.ts` 的 `pendingFriend()` 生成 uid `pending:<jobId>`（历史形态 `g<jobId>`），这是**前端自己编的**，Boss 服务器上不存在。点开时 `chat.py:652 messages()` / `:704 send()` 都先 `self._friends_raw.get(uid)` → 取不到 → 重拉 `friends()` 仍然取不到（占位 uid 永远不会出现在远端列表）→ 抛 `ChatError("在 Boss 会话列表里找不到这条会话…")`。**重拉是白做**。
2. **自动打招呼**：`commands_auto.rs:217 open_chat()` 走 `Command::ChatOpen{query: 公司名}`，而 `sidecar.py:611` 的 `chat.open` 分支**已经是一个直接抛异常的空壳**（`raise ValueError("chat.open 已并入 chat.friends")`）。故 `open_chat` 对未聊岗位**必然失败** → `record_failure()` → 满 3 次自动暂停。它实际只在「已聊过、且公司名能在 `friends()` 列表里匹配到」时能用，而那种会话本来就不需要打招呼。

#### 已落地（2026-11-25，四个未知项全部实测取证）

路径：`chat.ChatClient.ensure(ctx, job_id)` → 打开岗位详情页 → 点「立即沟通」→ 页面**自己**跳到聊天页 → **直接读跳转 URL 的 `id` 参数**，那就是新会话的 uid。

| 当时的未知项 | 实测结论 |
|---|---|
| 详情页 URL 怎么拼 | 只由 jobId 决定：`https://www.zhipin.com/job_detail/<去掉 boss: 前缀>.html`（实测 goto 0.96s） |
| 「立即沟通」选择器 | `a.btn-startchat`（`<A class="btn btn-startchat" href="javascript:;">`）。外层 `div.btn-container` 文本是「感兴趣 立即沟通」、`div.btn-startchat-wrap` 文本是「立即沟通」，只有这个 `a` 唯一可点，用 `:has-text()` 会连外层一起命中 |
| 新 uid 从哪儿取 | **跳转 URL**：`/web/geek/chat?id=<uid>&jobId=<岗位>&securityId=…`。该值与页面随即发出的 `getBossData?bossId=` / `historyMsg?bossId=` 完全一致。`geekEnter` 响应里没有 uid（§2.4.1），不用它 |
| 建好后 `historyMsg` 是否立即可用 | 可以。实测 `ensure()` 4.09s → `messages(uid)` 1.77s 读到该会话消息 |

**用户明确否掉的写法（m01981）**：「不要什么取差集 这是写屎山代码，直接用立即沟通之后的岗位」——不要前后拉两次 `friends()` 求差集找新增行，也不必给占位行造 uid。点完就地读 URL 就够。

**踩坑**：`goto(wait_until="domcontentloaded")` 时按钮还没被前端框架挂上，此时直接 `click` 会等满超时。必须 `page.wait_for_selector(SEL_START_CHAT, timeout=15_000)` 之后再点。**职位已关闭**的岗位页（正文出现「职位已关闭」）本来就没有这个按钮，这不是脚本故障，报错文案要区分开。

配套改动：`protocol.rs` 的 `ChatOpen{query}` → `ChatOpen{job_id}`；`sidecar.py` 的 `chat.open` 空壳改成真调 `ensure()`；新增命令 `chat_open`（`commands_chat.rs`，回 `{uid}`）；`commands_auto.rs` 的 `open_chat()` 改为按 `job.job_id` 走（旧版按公司名匹配，对未聊岗位从未成功过，是死路径）；前端 `chatOpen(jobId)` + `ChatPage.vue` 的 `pick()` 在选中占位行时先建会话、把占位行原地换成真实 uid 再继续拉消息。

**占位行不必等 `getGeekFriendList` 刷新**：刚建的会话可能还没进 `_friends_raw`，所以 `messages()` / `send()` 在列表里找不到该 uid 且页面**已经停在聊天页**时，不报错、不再点一次，直接挂 `historyMsg` 监听等它回来（`_open_conversation` 为此改成返回 `bool`）。

**别再这样做**：给占位会话调 `friends()` 重拉、或在 DOM 行上造假 uid（§2.4.2 已删的 `dom:{i}`）。

#### 会话列表不带岗位 id ⇒ 由 Rust 层按公司名回填（2026-11-25 二轮）

`getGeekFriendList` 的条目里**没有 `jobId`**（§2.4.1 字段表），所以「这条会话对应哪个岗位」在 sidecar 侧无从得知。缺了它，`ensure()` 的入参就凑不出来，任何一条会话失效后都无法自动重建。

做法：`protocol.rs` 的 `Friend` 增 `#[serde(default)] job_id: Option<String>`（sidecar 永远给 `None`）；`commands_chat.rs` 的 `chat_friends` / `chat_friends_local` 在 upsert 之后调 `fill_job_ids()`，按 `friend.company` 查 `job_queue` 补上。**配套的读库方法必须在 `store_ports.rs` 的 `JobAdminStore` 里声明**——写进 `JobStore` 会在 `impl JobAdminStore` 里编译不过（`no method named 'find_job_id_by_company' found`）。

SQL 口径：`SELECT job_id FROM job_queue WHERE company = ?1 ORDER BY CASE WHEN status='delivered' THEN 0 ELSE 1 END, job_id LIMIT 1` —— 同一家公司多个岗位时优先已投递的那条，取不到就 `None`。查询走公司名缓存去重，读库失败只记一条 warn 并整批停手，不逐行刷日志。

**这不是设计洁癖**：没有它，前端拿到的每条会话都没有岗位身份，「按岗位重建会话」只能对占位行生效（占位行的 jobId 编在 uid 里）。

### 2.4.5 发消息前必须真的打开那条会话（`send()` 不许无视 `_open_conversation` 的返回值）

用户报障（m00417）：点发送弹

```
发送失败：TimeoutError: Locator.wait_for: Timeout 15000ms exceeded.
Call log:
  - waiting for locator("#chat-input").first to be visible
```

**根因不是 `#chat-input` 选择器失效**，而是 `send()` 把 `_open_conversation()` 的返回值丢了。会话列表在 DOM 里是虚拟滚动的（§2.4.2：DOM 约 40 行 / 接口 100 条），目标会话排到后面时那一行**根本没渲染**，`_open_conversation` 找不到就 `return False`、什么也不点，页面仍停在列表页；接着等 `#chat-input` 只能耗满 15 秒，最终弹给用户的是一句裸 Playwright 超时。

现在 `chat.py::send()` 的三道闸：

1. 点中了行 ⇒ 直接发。
2. 没点中但 `#chat-input` 已可见（`_input_ready()`）= 会话本来就开着（`ensure()` 刚建的那条）⇒ 不打断页面，直接发。
3. 没点中且没有输入框 ⇒ 按 uid 直接打开 `CHAT_URL?id=<uid>`（地址格式同 `ensure()` 从跳转 URL 里读出来的那条），不依赖会话行在不在。

打开后仍没有输入框 ⇒ 抛 `ChatError("Boss 聊天页没有出现输入框（这条会话可能打不开，或页面结构已变），消息没有发出")`，**不再漏裸 `TimeoutError`**；同时日志留一条 warn——人已经在聊天页却没有 `#chat-input`，那才是选择器真的变了，重试没有意义。

已补离线用例 `tests/test_chat_send_open.py`（覆盖上述四条分支：行缺失按 uid 打开 / 行存在不导航 / 会话已开着不导航 / 打不开只报中文错）。本轮按用户新规**未执行**任何编译与测试，等客户自验。

### 2.5 导航失败与提示文案（用户 m02288）

- **`page.goto` 的原始异常不许进界面**。Playwright 的 `Error: Page.goto: net::ERR_ABORTED at …` 后面还跟着多行 `Call log: - navigating to …`，前端直接贴出来会撑掉半块对话框，用户以为程序卡死。所有导航统一走 `ChatClient._goto(page, url, what)`：失败只保留首行，并翻成「打不开 + 下一步怎么办」的人话。
- **`ERR_ABORTED` 多半是页面自己跳走，不是真失败**。点「立即沟通」后 Boss 会自己跳到聊天页，此时 `goto` 抛 ABORTED 但目标已经达成。`_goto` 的判据是 `_goto_landed(page, url)`——**必须真探一次 `page.query_selector("body")`**：浏览器被关掉时 `page.url` 不抛错、仍返回上一次的地址，光看地址会把「浏览器没了」咽掉。
- **连接断了要单独说**。`Target page, context or browser has been closed` 一律翻成「先点右上角「扫码登录」把浏览器拉起来，再点「刷新」」，不能只说「失败」。
- **`chat_page()` 挂的 `KEY_FRIENDS` 只记账、不解析**（`_catch(page, KEY_FRIENDS, None)`）。`friends()` 会在 `reload` 之后重新挂真正解析正文的监听，这里若也入队，reload 后旧文档的响应只能读出 `net::ERR_ABORTED`，白灌一条「读取 getGeekFriendList 响应失败」。`_catch` 的 `handler=None` 仍会把 key 记进 `_drained`，等待方据此提前返回（§2.4.3）。
- **离线回归在 `scraper/tests/test_chat_goto.py`**，纯替身不碰浏览器。仓库没有 pytest，跑法：
  ```powershell
  python -c "import sys; sys.path.insert(0,'scraper/tests'); import test_chat_goto as t; names=[n for n in dir(t) if n.startswith('test_')]; [getattr(t,n)() for n in names]; print('passed:', len(names))"
  ```

### 2.5.1 聊天要留「操作日志」（用户 m00505）

- **每一次真导航都写一行**：`_goto()` 成功即 `self.log("info", f"{what}：{url}")`。用户报「切会话时浏览器刷了两次，第一次是 /web/geek/chat、第二次带 bossid」时，靠这一行才能分清哪次是本程序发的、哪次是 Boss 页面自己跳的——以前 `_goto` 只在失败时说话，日志里一片空白。
- **每个用户动作一对行**：`messages()` 开头「打开会话：<HR（公司）>」、结尾「会话消息 N 条」；`send()` 开头「发送消息给 …」、结尾「已发送消息给 …」；`friends()` 成功「会话列表已刷新：N 条」。会话怎么称呼见 `_friend_label()`（姓名→公司名→uid），**别在日志里只写 uid**，数字串看不出是哪位 Boss。
- **`scraper/tests/test_chat_send_open.py` 在 HEAD 就是红的**（`FakeLocator.first` 是方法、`send()` 按属性用），不是本轮引入，也不许拿它当门禁。别照它改 `send()`。

### 2.6 反爬

- **随机延时 1.5~9.8 秒不能调小**。调小不会更快，只会触发验证码，而风控页在 DOM 上是空列表——看起来像解析坏了。
- **`AJA_NO_STEALTH` 已彻底移除**（代码零命中），不要重新引入。
- **`headless=False`、`channel="chrome"`、`launch_persistent_context`、不设 viewport/UA/locale、强制直连、不加 `--no-sandbox`** —— 改任一项都会**静默**变成「必被检测」。

### 2.7 profile

- 路径 `<data_dir>/data/browser-profile`，`data_dir` 优先读 `AJA_DATA_DIR`。
- **不能把 `data_dir()` 当 `--user-data-dir`**：里面还有 `data/job_assistant.*.db`，Chrome 会把 `Default/`、`Local State` 洒到应用数据根下；且该目录一旦被别的 Chrome 会话认可，会立刻 `Target page, context or browser has been closed`。
- 正式 profile 在 `data/browser-profile/`（约 220 MB，含 cookies），`.gitignore` 已忽略，**不得** `git add -f`。

---

## 3. 下一步

### 3.1 等产品级验收

1. **新设备扫码登录全流程**：本轮 E2E 用的是已登录 profile；新设备 / 登录失效态的扫码 → 保存 → 抓取全流程未实测。

### 3.2 可独立开工

1. **`AGENTS.md` 第 8 节列出的三处 skill 参考映射漂移**：`chat.py:9-12` 应为 `:5-14`（线程铁律）与 `:16-24`（监听器铁律）；`core/scrape/restart.rs:28` 路径已变（`core/scrape.rs` 现在是 re-export 外壳）。改 `scraper/.agents/skills/sidecar-scraping/SKILL.md` 即可，纯文档。
2. **`_diagnose_empty_page` 的「卡片选择器一个都没匹配上」分支**（`boss.py:423-424`）：这里已有信息足够的提示，但可以把「页面里有 N 个岗位链接」的 N 也打出来对照，便于判断是选择器漂移还是风控。
3. **`chat.open` 的语义已换**（原「按公司名找会话」→ 现「按 jobId 建会话」，见 §2.4.4）。`handle()` 的 `chat.*` 通配仍是通配，显式列出已支持命令、让 `UNKNOWN_CMD` 兜底真正生效这件事还没做。

---

## 4. 写代码的硬规矩（用户明令，m00585）

**没有证据表明有用的兜底，一律不要写。**

起因：为「省一次 reload」加了「先挂监听等推送 3 秒」的快路径，探针实测证明没有推送就白等 3 秒，纯增开销；`_wait_items` 每轮对 40 行做 7 个 `querySelector` + `innerText`（强制布局），返回值却从未被使用，只是节流用的空转。两者都被要求删除。

落地口径：

1. **不许写「等等看」**。等某个信号出现必须有明确触发条件与上限，收到即走。
2. **不许写返回值没人用的探测**。节流用 `page.wait_for_timeout(...)`，不要拿「读一遍 DOM」当节拍器。
3. **兜底必须有实测证据**。`chat.py` 的 `friends()` 原来注释称「reload 才能真正触发 getGeekFriendList；单次点击可能拿不到」——探针实测：不 reload 只空转 8 秒也能收到 2 次；点击列表项 0 次；reload 本身只要 0.7 秒。**那条注释的前提不成立**，别再据此加等待。
4. **拿不到数据就报错，不要造假数据**。`dom:{i}` 假 uid 就是反例：让界面看起来有内容，点下去必然失败，比直接报错更难排查。
