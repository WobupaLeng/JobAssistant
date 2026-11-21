# `scraper/` 交接文档（HANDOFF）

> **本文件只写「现状 / 坑 / 下一步」**，不写历史记录与用例明细。
> 目录结构、协议、红线见 [`AGENTS.md`](AGENTS.md)，跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。

**最后更新**：2026-11-22

---

## 1. 现状

抓取、登录、聊天、求职期望四条链路**代码已完成**，协议对端（Rust `core/sidecar/`）已通，`python -m compileall -q scraper` 通过。

**唯一未通的是运行态**：从 Tauri 子进程里拉起**真实 Chrome** 仍失败。

### 1.1 【阻塞】应用运行时拉不起 Chrome

**现象**：应用运行中发起抓取，sidecar 的 Chrome 报「正在现有的浏览器会话中打开」并 `exitCode=0`；playwright 收到 CDP 管道关闭 → `TargetClosedError: BrowserType.launch_persistent_context: Target page, context or browser has been closed`。表现为聊天与求职期望全部失败。

**根因**：应用运行时 `msedgewebview2.exe` 会在本机创建 Chromium 单例命名对象，sidecar 启动的 Chrome 探测到后判定「已有会话」直接退出。

**已排除**（都试过，不要再试）：Job Object、环境变量（含 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`）、stdio / `CREATE_NO_WINDOW`、`--user-data-dir` 路径、profile 脏数据、Chrome 版本。

**待定修法**：给 sidecar 的 Chrome 用一个与应用 WebView2 **不再撞名**的 user-data-dir；或分离 `--user-data-dir` 与 `--apps-user-data-dir`。

**旁证**：单独运行 `python scraper/sidecar.py`（不经 Tauri）时 Chrome 能正常拉起——问题只在「作为应用子进程」时出现。

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

### 2.5 反爬

- **随机延时 1.5~9.8 秒不能调小**。调小不会更快，只会触发验证码，而风控页在 DOM 上是空列表——看起来像解析坏了。
- **`AJA_NO_STEALTH` 已彻底移除**（代码零命中），不要重新引入。
- **`headless=False`、`channel="chrome"`、`launch_persistent_context`、不设 viewport/UA/locale、强制直连、不加 `--no-sandbox`** —— 改任一项都会**静默**变成「必被检测」。

### 2.6 profile

- 路径 `<data_dir>/data/browser-profile`，`data_dir` 优先读 `AJA_DATA_DIR`。
- **不能把 `data_dir()` 当 `--user-data-dir`**：里面还有 `data/job_assistant.*.db`，Chrome 会把 `Default/`、`Local State` 洒到应用数据根下；且该目录一旦被别的 Chrome 会话认可，会立刻 `Target page, context or browser has been closed`。
- 正式 profile 在 `data/browser-profile/`（约 220 MB，含 cookies），`.gitignore` 已忽略，**不得** `git add -f`。

---

## 3. 下一步

### 3.1 等外部条件

1. **Chrome 撞名问题**（第 1.1 节）：需要能在正常桌面实际启动应用做对照实验的人。中枢/沙箱环境的 shell 是 Low 完整性，跑不起 GUI。
2. **登录入口接线**：`login.state` 事件链在 Python 侧是完整的（`sidecar.py:263/272/297/303` emit → Rust `process/codec.rs` 解析），但**主进程没有 emit 给 WebView**，前端 `login_status` / `login_save` / `logout` 调用点为 0。这是后端 + 前端两边的活。

### 3.2 可独立开工

1. **`AGENTS.md` 第 8 节列出的三处 skill 参考映射漂移**：`chat.py:9-12` 应为 `:5-14`（线程铁律）与 `:16-24`（监听器铁律）；`core/scrape/restart.rs:28` 路径已变（`core/scrape.rs` 现在是 re-export 外壳）。改 `scraper/.agents/skills/sidecar-scraping/SKILL.md` 即可，纯文档。
2. **`_diagnose_empty_page` 的「卡片选择器一个都没匹配上」分支**（`boss.py:423-424`）：这里已有信息足够的提示，但可以把「页面里有 N 个岗位链接」的 N 也打出来对照，便于判断是选择器漂移还是风控。
3. **`chat.open` 的废弃回执**：目前回 `CHAT_FAILED` + 「已并入 chat.friends」。若确认前端无调用点，可以考虑从 `handle()` 的 `chat.*` 通配里显式列出已支持命令，让 `UNKNOWN_CMD` 兜底真正生效。
