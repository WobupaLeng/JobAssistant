# `scraper/` 交接文档（HANDOFF）

> **本文件只写「现状 / 坑 / 下一步」**，不写历史记录与用例明细。
> 目录结构、协议、红线见 [`AGENTS.md`](AGENTS.md)，跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。

**最后更新**：2026-11-23

---

## 1. 现状

抓取、登录、聊天、求职期望四条链路**运行态已通**：从 Tauri 子进程拉起真实 Chrome 已修复并实测（提交 `1f5910b`；取证与证据索引在 `%TEMP%\jt-chrome-fix\VERIFICATION.md`，t1/t2/t6/t7 各目录）。

### 1.1 Chrome 撞名阻塞已除（三个预存缺陷，别再按旧结论排查）

旧结论「WebView2 单例命名对象撞名」**是误诊**：Chromium 单例身份 = 标题为字面 user-data-dir 路径的 `Chrome_MessageWindow` 消息窗口，WebView2 UDF 与 sidecar UDF 路径不同，天然不撞。真凶是三个缺陷叠加：

1. **强杀无整树回收**：杀 python 留下的孤儿 Chrome 持单例窗，下次 launch 撞名。修：Rust 侧 Job Object `KILL_ON_JOB_CLOSE`（`backend-tauri/src/core/sidecar/process/job.rs`），spawn 即纳入，shutdown/强杀回收整树。
2. **每命令新线程 vs patchright 线程绑定**：首条命令线程退出后 `_pw/_ctx` 成尸体，第二条命令探活必炸 → 误判失效 → teardown 重启撞仍活着的 Chrome 单例窗 → `TargetClosedError`。修：常驻 `ThreadPoolExecutor(max_workers=1)` 单车道（`_browser_q`），所有浏览器命令含 `login.status` 探活与 EOF teardown 都排队到同一条线程（AGENTS §5）。
3. **launch 失败复用死连接**：`ensure_context` 失败只置 `_ctx=None` 却保留 `_pw`，下次复用已断的 driver 连接。修：launch 异常立即 `_teardown_playwright()` 再抛。

真实小批量 E2E（Java，maxJd=10，门槛 70，已登录真实 profile）：scraped=15 / inserted=7 / below_threshold=5 / rejected=3，新入库行规则五维打分字段齐全，日志无撞名 / 无 `TargetClosedError`；`scrape_close` 整树回收约 1.6s；`login_save` → `saved=true` → `logout` → `saved=false` 标记链通过。

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

### 3.1 等产品级验收

1. **新设备扫码登录全流程**：本轮 E2E 用的是已登录 profile；新设备 / 登录失效态的扫码 → 保存 → 抓取全流程未实测。
2. **抓取自然完成信号**：`_run_scrape` 结束只发 log（`sidecar.py:258`），Rust `progress.running` 只在 stop/Exited 清，界面停在「抓取中」直到用户手动停止。需协议完成事件（动 `protocol.rs` 高冲突单点）+ 前端消费。

### 3.2 可独立开工

1. **`AGENTS.md` 第 8 节列出的三处 skill 参考映射漂移**：`chat.py:9-12` 应为 `:5-14`（线程铁律）与 `:16-24`（监听器铁律）；`core/scrape/restart.rs:28` 路径已变（`core/scrape.rs` 现在是 re-export 外壳）。改 `scraper/.agents/skills/sidecar-scraping/SKILL.md` 即可，纯文档。
2. **`_diagnose_empty_page` 的「卡片选择器一个都没匹配上」分支**（`boss.py:423-424`）：这里已有信息足够的提示，但可以把「页面里有 N 个岗位链接」的 N 也打出来对照，便于判断是选择器漂移还是风控。
3. **`chat.open` 的废弃回执**：目前回 `CHAT_FAILED` + 「已并入 chat.friends」。若确认前端无调用点，可以考虑从 `handle()` 的 `chat.*` 通配里显式列出已支持命令，让 `UNKNOWN_CMD` 兜底真正生效。
