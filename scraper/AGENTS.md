# `scraper/` 模块约定 · Python 抓取 sidecar

> 本文件是本模块**目录结构与职责边界**的唯一真源。跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。
> **只写约束，不写进度**：现状、坑、下一步写 [`HANDOFF.md`](HANDOFF.md)。

**定位**：`app` 的**子进程**，用 patchright（Playwright 反检测分叉）驱动**本机真实 Chrome**，持久化 profile 保住扫码登录态。**它只做浏览器操作**——打分、落库、读配置、规则判定、重启决策全归 Rust。

---

## 1. 目录结构

5 个 Python 文件，没有包、没有 `__init__.py`，靠 `sidecar.py::main` 把自身目录插进 `sys.path` 后直接 `import boss` / `browser` / `chat` / `base`。

| 文件 | 关键入口 / 职责 |
|---|---|
| `sidecar.py` | 进程入口：stdin 行循环 + `handle()` 分发 + 常驻单车道执行器 + `emit()` 出口。`log()` / `clamp_text()` / `interruptible_sleep()` / `data_dir()` / `profile_dir()` / `ensure_context()` / `_teardown_playwright()` / `_run_scrape()` / `_run_login()` / `busy_reason()` / `_busy_now()` / `start_scrape()` / `start_login()` / `stop_scrape()` / `_run_chat()` / `_run_expect()` / `_run_session_open()` / `_WorkerHandle` |
| `boss.py`（751 行） | 岗位抓取：卡片 DOM 解析 + 详情页补齐 + 无限滚动翻页。`decode_boss_salary()` / `map_salary_code()` / `build_search_url()` / `BossScraper`（`run` / `_load_more_by_scroll` / `_diagnose_empty_page` / `_parse_dom_cards` / `_parse_detail_dom`） |
| `chat.py`（628 行） | 聊天：会话列表 / 历史消息 / 发送 / 发简历 / 撤回。`ChatClient`（`friends` / `messages` / `send` / `resumes` / `send_resume` / `recall`）、`normalize_friend()` / `normalize_message()` / `merge_dom_status()` / `strip_internal()` |
| `browser.py`（294 行） | 持久化上下文启动、登录态判定、求职期望。`_launch_args()` / `_find_system_browser()` / `sanitize_profile_exit_state()` / `launch_context()` / `find_page_for()` / `detect_logged_in()` / `fetch_expectations()` |
| `base.py`（69 行） | `split_keywords()`、`ScrapeConfig` + `keyword_list()`。**没有基类** |
| `.agents/skills/sidecar-scraping/SKILL.md` | 本模块技能 |

**`base.py:1-8` 明确写了**：`StopFlag` 与 `BaseScraper` 基类是被**刻意删掉**的。不要加回来做「公共抽象」——每加一层抽象，「停止是否生效」就更难验证一次。

---

## 2. 协议（JSON Lines over stdin/stdout）

真源是 `backend-tauri/src/core/sidecar/protocol.rs`，这里的 `handle()` 与 `emit()` 是它的对端。**改任一端必须同一次提交改另一端。**

**三种帧，同一个 `Envelope{id, event, ok, data, error}`**：命令（含 `cmd`，Rust → stdin）／回执（含 `id`、**无** `event`，由 Rust `route_reply` 投递）／事件（含 `event`，主动推送）。

**命令分发表（`sidecar.py:557-632`）**

| 命令 | 执行位置 | 说明 |
|---|---|---|
| `ping` | **主线程**直接回 | `{"pong": true}` |
| `scrape.start` | 单车道执行器 | 已在跑则只打 warn 不报错，仍回 `accepted: true` |
| `scrape.stop` | **主线程** | **只置 `_stop`**，立即回 `{"stopped": true}` |
| `login.start` | 单车道执行器 | 浏览器被占则只打 warn，仍回 `accepted: true` |
| `login.status` | **主线程**投递探活到单车道（5s 超时） | 只读 cookie，`{"logged_in": bool}`；浏览器线程被长任务占用时超时按未登录回报 |
| `chat.*` | 单车道执行器 | 主线程 `_busy_now` 预检，占用即回 `CHAT_FAILED`；`friends` / `messages` / `send` / `resumes` / `send_resume` / `recall`；`chat.open` **已废弃**，回 `CHAT_FAILED` |
| `expectation.list` | 单车道执行器 | 主线程 `_busy_now` 预检；要等一次网络往返 |
| `session.open` | 单车道执行器 | 主线程 `_busy_now` 预检；只拉起浏览器并报登录态 |
| 其它 | — | `{"code": "UNKNOWN_CMD"}` |

**事件**：`ready`（`data.pid`）、`log`（`{"level","msg"}`，`msg` 截到 4000 字符）、`job.scraped`（`_job_to_event` 只保留 21 个键，空值不发，`jd_text` / `company_intro` 经 `clamp_text`）、`login.state`（`:263` / `:272` / `:297` / `:303`）。

**回执**：成功 `emit({"id": cid, "ok": True, "data": ...})`；失败 `error` **是对象** `{"code": "...", "msg": "..."}`，Rust `process/codec.rs` 认 `msg` / `code` 两个键。

---

## 3. 红线

1. **不打分、不落库、不读配置、不持业务状态**（`sidecar.py:1-16` 原文）。权威计数在 Rust `ScrapeProgress`。
2. **DOM-only**，不要加回接口通道。三条理由（`boss.py:1-23`）：接口丢字段（DOM 有城市·区·商圈，接口只有 `cityName`）、`joblist.json` 只在首屏触发第 2 页起恒为空、字段随时被改。详情页也没有可用的 `detail.json`（实测 404）。
3. **翻页判据是「卡片数增长」**，不是「滚动成功」也不是「选择器出现」——滚到底页面可能只是回弹。
4. **`hr_active_status` 只认 `.job-boss-info .boss-active-time`**，且必须过正则再回传。取不到时一旦勾了「HR 活跃度」规则，`exclude_hr_active_days` 会把**全部岗位**判成 `hr_active_unknown` 否决。默认配置是 `None` 不触发，但界面上能勾。
5. **监听器里绝不能调阻塞 API**（`chat.py:16-24`）：`page.on("response")` 回调运行在 playwright 调度栈，调 `resp.text()` 会让 driver 崩成 `Event loop is closed`。只把响应对象塞进队列，正文由主流程读。
6. **`scrape.stop` 不结束进程**：只置 `_stop`，进程继续挂在 `for raw in sys.stdin`。收尾靠 Rust 关 stdin，超时才 `kill()`。不关 stdin 的表现是「命令卡住」。
7. **所有等待走 `interruptible_sleep`**（每 0.2 秒看一次 `_stop`），**不用 `time.sleep`**——否则最长 9.8 秒的反爬等待会让界面像卡死。
8. **随机延时 1.5~9.8 秒不许调小**（`base.py:61-62`）。调小不会变快，只会触发风控；风控页在 DOM 上是「空列表」，看起来像解析坏了。
9. **不许引入 `AJA_NO_STEALTH` 之类的反检测开关**（代码里零命中）。调试用独立 profile。
10. **碰浏览器的命令必须排队到同一个工作线程**。patchright 同步 API 绑死创建它的线程，跨线程立刻 `Cannot switch to a different thread`。
11. **`emit()` 必须 flush，单行不超过 1 MB**。超限整条替换成 error 日志，不静默丢弃。
12. **stdout 只走协议**，任何 `print()` 调试都会被 Rust 判成 `Malformed`。日志走 `emit` 的 `log` 事件。

**职责边界（唯一允许的「业务判断」是详情拿不到就不推送，`boss.py:327-328`——减少无意义传输，不是打分）**

| 想做的事 | 允许 | 该放哪 |
|---|---|---|
| 解码字体加密薪资（U+E031..U+E03A → 0-9） | ✅ | `boss.py::decode_boss_salary`，产出仍是原始文案 |
| 判「达不达标」 | ❌ | Rust `core/sidecar/ingest.rs::handle_job` |
| 判「重复 / 在敏感库」 | ❌ | Rust `ingest.rs::prescan`（卡片阶段就判） |
| 决定排除规则 | ❌ | Rust `core/domain/rules.rs` |
| 写数据库 | ❌ | Rust `core/infra/sqlite/` |
| 读 `app_config` / API Key | ❌ | Rust `commands_config` |
| 调大模型打分 | ❌ | Rust `core/domain/scoring_llm.rs` |
| 决定「是否重启 sidecar」 | ❌ | Rust `core/scrape/` |

---

## 4. 反检测启动姿态（改它会**静默**变成「必被检测」）

| 设置 | 位置 | 为什么 |
|---|---|---|
| `headless=False` | `browser.py:13` | 无头必被检测（UA 带 `Headless` → 风控 code=37） |
| `channel="chrome"` | `browser.py:135` | 用本机真实 Chrome；注册表探测失败时兜底改 `executable_path` 直连（`:157-164`） |
| `launch_persistent_context` | `browser.py:113` | **只有它**会把扫码登录态落到 profile |
| 不设 viewport / user_agent / locale | `browser.py:15-16` | 锁视口是自动化特征；伪造 UA 会自相矛盾 |
| `--no-proxy-server` / `--proxy-bypass-list=*` | `browser.py:52-67` | 强制直连；代理出口 IP 更容易被标记 |
| 不加 `--no-sandbox` | `browser.py:152` | 它本身就是自动化特征 |

**profile 路径**：`profile_dir()` = `<data_dir>/data/browser-profile`；`data_dir()` 优先读 `AJA_DATA_DIR`，否则 `%LOCALAPPDATA%/ai-job-assistant`。**必须与旧版应用共用同一个 profile**，否则用户要重新扫码，新 profile 也更容易被判陌生设备。

**不能把 `data_dir()` 当 `--user-data-dir`**——里面还有 `data/job_assistant.*.db`，Chrome 会把 `Default/`、`Local State` 洒在应用数据根下；且一旦该目录被别的 Chrome 会话认可（「正在现有的浏览器会话中打开」），`launch_persistent_context` 立刻 `Target page, context or browser has been closed`，表现为聊天与求职期望全部失败。

---

## 5. 线程模型（常驻单车道）

```
主线程      for raw in sys.stdin → handle()
              ├── ping / scrape.stop        直接执行（不能阻塞）
              ├── login.status              submit 探活到单车道，.result(timeout=5)
              └── 其余命令 → _busy_now 预检（占用即回 busy）
                             → _browser_q.submit(...)

_browser_q   ThreadPoolExecutor(max_workers=1) 常驻单车道（sidecar.py:36）
             _run_scrape / _run_login / _run_chat / _run_expect / _run_session_open
             全部在同一条线程上排队，patchright 连接与进程同寿

_WorkerHandle                长任务 Future 包装成 is_alive()（scrape/login 占用标记）
_chat_busy                   同一时刻只有一条聊天命令在动浏览器
_stop/_worker/_worker_lock/_occupant   模块级运行态
_pw/_ctx                     patchright 实例与 context，只在 _browser_q 线程里碰
```

**为什么必须常驻**：patchright 同步 API 绑死创建它的线程。旧实现每条命令各起一个新线程，首条命令的线程一退出 `_pw/_ctx` 就成尸体——第二条命令探活必报 `Playwright connection closed`，随后误判「浏览器已失效」去 teardown 重启，撞上仍活着的 Chrome 单例窗（「正在现有的浏览器会话中打开」→ `TargetClosedError`）。跨线程 `pw.stop()` 同样必报 greenlet 错，等于没关。

推论：**抓取进行中发起聊天会排队**（刻意取舍，宁可让用户看到「排队中」，也不为并发去碰线程铁律）；**长任务期间其它命令只能排队**，所以 Rust 对回执必须设超时；**`login.status` 至多阻塞主线程 5 秒**，超时按未登录回报；**`scrape.stop` 能在抓取中途生效**，正是因为抓取跑在单车道、主线程始终在读 stdin。**stdin EOF 的兜底 teardown 也必须投递到 `_browser_q`**（`main()` 尾部，仅无长任务占用时尝试）——主线程直接 stop 必炸。

**`_occupant`** 取值 `"scrape"` / `"login"` / `None`，只用于把「占用中」说清楚。教训（`sidecar.py:312-323`）：早先所有占用场景都回「正在抓取或登录，请先停止再操作聊天」，但**登录期间根本没有停止按钮**——`stop_scrape` 会让登录循环误报「已取消登录」。**提示语必须指向一条真能走通的路。**

---

## 6. 编码与 I/O

`main()` 开头必须 `sys.stdout.reconfigure(encoding="utf-8", errors="replace")`，`sys.stdin` 同样。Windows 上 Python 默认 cp936/GBK 写 stdout，而协议约定 UTF-8——不重配置会让**所有中文 JD 变成乱码**（实测 `{"msg": "已启动浏览器"}` 输出成 `{"msg": "������..."}`）。用 `errors="replace"` 而非 `"strict"`：宁可个别字符坏掉，也不能让整条消息抛异常中断抓取。

---

## 7. 新增 / 修改一条命令的检查表

1. `sidecar.py::handle()` 加分支；兜底必须是 `UNKNOWN_CMD`。
2. 需要浏览器就必须 submit 到 `_browser_q` 单车道（照 `start_scrape` / `session.open` 分支的写法），主线程用 `_busy_now` 预检占用。
3. 按 §2 的回执形状 `emit`，**`error` 是对象**。
4. 同步改 Rust 侧 `protocol.rs` 的 `Command` 枚举与 `handle()` 参数名（**平铺，不加 `params` 包装**）。
5. `python -m compileall -q scraper`。

---

## 8. 模块内技能

`scraper/.agents/skills/sidecar-scraping/SKILL.md`（224 行）：职责边界、五条抓取铁律（每条附真实事故）、反爬不许动的两个东西、单车道、注入层边界、命令检查表、12 条 Non-Negotiables。

**改 `scraper/*.py` 前先读它。** 参考映射已核对、行号准确；两处路径以现有写法为准：`restart_decision` 在 `backend-tauri/src/core/scrape.rs:28`（不是子目录），`chat.py:9-12` 讲线程铁律、`:16-24` 才是监听器铁律。
