---
name: sidecar-scraping
description: Python 抓取 sidecar（scraper/）的职责边界与抓取铁律：DOM-only、无限滚动判据、hr_active_status 选择器、监听器禁阻塞调用、scrape.stop 不等于结束进程、反爬延时与线程单车道。Use when editing scraper/*.py（boss.py 选择器/翻页、chat.py 监听器、browser.py 启动参数、sidecar.py 命令分发），或排查「抓到 0 条 / 字段为空 / 命令卡住 / driver 断开」。Not for Rust 侧协议与命令层（见 sidecar-protocol / rust-tauri），也不负责评分与落库——那些不归 Python。
---

# sidecar-scraping：Python 抓取侧的职责与铁律

`scraper/` 是 `app` 的子进程，用 patchright（Playwright 反检测分叉）驱动**本机真实 Chrome**，
持久化 profile 保住扫码登录态。它**只做浏览器操作**，其余一切归 Rust。

## Routing Trigger

命中任一项就按本 skill 办事：

- 改任何 `scraper/*.py` 的选择器、翻页、延时、启动参数
- 症状：抓到 0 条 / 卡片解析出空字段 / JD 为空 / HR 活跃度为空
- 症状：`Connection closed while reading the driver`、`Event loop is closed`、
  `Cannot switch to a different thread`、`Sync API inside the asyncio loop`
- 症状：命令卡住、点"停止"没反应、关应用后 python 还在
- 想"顺手"在 Python 里打分/落库/读配置

## Version Anchor

| 文件 | 行数 | 职责（一句话） |
|---|---|---|
| `scraper/sidecar.py` | 605 | **进程入口**：stdin 行循环、`handle()` 分发、工作线程、进度记账、`emit()` 出口 |
| `scraper/boss.py` | 751 | Boss 岗位抓取：搜索页卡片 DOM 解析 + 详情页补齐 + 无限滚动翻页 |
| `scraper/chat.py` | 628 | Boss 聊天：会话列表 / 历史消息 / 发送 / 简历 / 撤回 |
| `scraper/browser.py` | 294 | patchright 持久化上下文启动、登录态判定、求职期望读取 |
| `scraper/base.py` | 69 | `ScrapeConfig` 数据类 + 关键词切分（**没有基类**，见下） |

`base.py:1-8` 明确写了：**StopFlag 与 `BaseScraper` 基类是被刻意删掉的**。
不要再把它们加回来做"公共抽象"——每增加一层抽象，"停止是否生效"就更难验证一次。

## The Reference Map

| 你要干的事 | 先读 |
|---|---|
| 加一条命令 / 改分发 | `sidecar.py:521-572`（`handle`）、协议对端 `backend-tauri/src/core/sidecar/protocol.rs:69-129` |
| 改卡片解析 | `boss.py:442-527`（`_parse_dom_cards`）、`boss.py:431-439`（容器选择器） |
| 改翻页 | `boss.py:361-384`（`_load_more_by_scroll`） |
| 详情页字段 | `boss.py:565-751`（`_parse_detail_dom`），HR 活跃度在 `:621-637` |
| 空页诊断 | `boss.py:386-429`（验证码/风控/登录失效/加载中/真无结果 翻成人话） |
| 登录态 | `browser.py:226-244`（只读 cookie）、`browser.py:34-39`（哪些 cookie 算登录） |
| 聊天监听器 | `chat.py:227-266`（`_catch` / `_drain`）、`chat.py:16-24`（铁律原文） |
| 线程模型 | `sidecar.py:41-44`、`sidecar.py:360-390`、`chat.py:9-12` |
| 延时 | `base.py:38-69`（`delay_min` / `delay_max`）、`boss.py:186-198`（`jitter`） |

## 五条抓取铁律（每条都有真实事故）

### 1. DOM-only：不要再加接口通道

`boss.py:1-23` 记录了 2026-11-21 的整体删除，理由是**三条，不是一条**：

1. **接口丢字段**：DOM 能拿到 城市·区·商圈，接口只给 `cityName`；
2. **不翻页就没数据**：`joblist.json` 只在 `page.goto` 首屏触发，第 2 页起恒为空；
3. **接口名与字段随时被改**（`joblist.json` / `zpData.jobList` / `bossActiveTimeDesc`）。

详情页同理：`boss.py:532-533` 实测 Boss 详情页**没有**可用的 `detail.json`（返回 404）。

反例警戒：唯一"接口拿得到而 DOM 拿不到"的字段已用 DOM 补齐了
（`hr_active_status` 在卡片上没有，但在详情页 `.job-boss-info` 里，见 `boss.py:21-22`、`:621-637`）。

### 2. 翻页是无限滚动，判据是「卡片数增长」

`boss.py:361-384` `_load_more_by_scroll(page, before, rounds=4)`：

```python
page.mouse.wheel(0, 4000)
self._sleep(2.5)                        # 等网络把新卡片追加进 DOM，实测稳妥值
now = page.locator("li.job-card-box").count()
if now > prev: return True              # 只有数量增长才算拿到
```

:364 注释写死了理由：**滚到底后页面可能只是回弹，不判断数量就会无限翻下去**。
不要改成"滚动成功返回 True"或"等待选择器出现"——回弹时选择器一直存在。

### 3. `hr_active_status` 选择器是 `.job-boss-info .boss-active-time`

`boss.py:625`：

```python
active = _first_text(page, ".job-boss-info .boss-active-time, .boss-active-time")
```

:623-624 注释：**不是**老版本的 `.job-banner`——那里的 `.boss-info-attr` 在本站已不存在，
所以此前 `hr_active_status` 一直是空，规则 `exclude_hr_active_days` **静默失效**。

取到文案后必须过 :630-637 的正则再写回（`m.group(1)`），因为原生文案是「刚刚活跃」这种长串，
而 Rust 侧 `core/domain/parse.rs:101-124` 的 `HR_ACTIVE_DAYS` 表按子串匹配（含「刚刚」「在线」「本月活跃」）。
Rust 侧有一条会因此变红的用例：`core/domain/rules.rs:436-451`
`scraped_variants_of_hr_activity_are_recognized`（:431-435 注释：
若它挂了，说明抓取文案变了而 parse 侧没跟上——规则会静默把所有岗位判成"活跃度未知"）。

**最坏情况不是"少抓"，而是"全否决"**：`rules.rs:269-276`，`exclude_hr_active_days = Some(n)` 时
`hr_active_days(None)` 走 `None` 分支，直接返回 `hr_active_unknown` 排除；
而 `ExcludeRules::default()` 里该项是 `None`（`rules.rs:66`），所以**默认配置下不会触发**，
一旦用户在界面上勾了"HR 活跃度"就会。

### 4. 监听器里绝不能调阻塞 API

`chat.py:16-24` 是原文，症状记在代码里：

```
Response.text: Connection closed while reading from the driver
```

之后所有请求变成 `Event loop is closed` / `Sync API inside the asyncio loop`，整个会话报废。

正确写法（`chat.py:227-266`）：

- `_catch(page, key, handler)` 里的 `_on_response` **只**做
  `self._signals.setdefault(key, []).append((resp, handler))`（:236），
  并且过滤 `key not in resp.url or status != 200`；
- 正文由主流程 `_drain(key)` 里 `handler(resp.text())` 读（:255-266）；
- 同一 `(page, key)` 只挂一次监听器：`chat.py:244-253` 用 `(id(page), key)` 做键，
  旧回调先 `remove_listener` 再挂新的（重复挂会让回调栈越积越深）；
- `chat.py:241` 注释：**监听器里绝不能抛异常**，会被调度器吞掉并连带弄坏连接。

同一条规则在 `boss.py` 的表述是"DOM-only"——那里的监听通道已整条删除。

### 5. `scrape.stop` 不结束进程；收尾是 stop + 关 stdin

- `sidecar.py:354-357` `stop_scrape()` **只** `_stop.set()` + 打一条日志，进程继续等 stdin。
- 停止在业务层的落地靠**可中断睡眠**：`sidecar.py:72-85` `interruptible_sleep(seconds)`
  每 0.2 秒看一次 `_stop`；:75-76 注释：不能用 `time.sleep`，否则随机反爬等待（最长 9.8 秒）会让界面像卡死。
  `boss.py` 里所有等待都走注入的 `sleep`（`boss.py:166`、`:172`、`:186-198`）。
- 真正的收尾在 Rust 侧：`process/handle.rs:341-368` `shutdown` **关 stdin**，
  让 Python 的 `for raw in sys.stdin`（`sidecar.py:589-598`）走到 EOF，:601 再 `_stop.set()`。
  超时才 `kill()`。`handle.rs:404-409` 的 `Drop` 里 800ms 兜底。
- 不关 stdin 的表现就是"命令卡住"：进程挂在 `for raw in sys.stdin` 上等下一行。

## 反爬：不许改小的两个东西

### 随机延时 1.5~9.8 秒

`base.py:61-62`：`delay_min = 1.5` / `delay_max = 9.8`（秒）。
`boss.py:186-198` `jitter()`：`random.uniform(cfg.delay_min, cfg.delay_max)` 后 `self._sleep(secs)`，
:187 docstring 原文"随机延时（反爬）：默认 1500~9800ms"。
`boss.py:538` 详情页 goto 前也走 `self.jitter()`。

> **严禁调小**。调小不会让抓取"变快"，只会让详情页密集请求触发验证码/风控，
> 而风控页在 DOM 上是"空列表"，你会以为是解析坏了。

### `AJA_NO_STEALTH`

在 `scraper/` 里 `grep AJA_NO_STEALTH` **零命中**——这个开关已经不存在。
不要为了"调试快一点"重新引入它（项目宪法把它列为红线：严禁用它跑真实抓取）。
调试抓取要用**独立 profile**（`sidecar.py:101-113` `profile_dir()`），而不是关掉反检测。

其它不可动的启动姿态（改它会**静默**变成"必被检测"）：

| 设置 | 位置 | 为什么 |
|---|---|---|
| `headless=False` | `browser.py:13` | 无头必被检测（UA 带 `Headless` → `code=37` 风控） |
| `channel="chrome"` | `browser.py:113-189` | 用本机真实 Chrome，不用 playwright 自带 chromium |
| `launch_persistent_context` | `browser.py:1-17` | 只有它会把扫码登录态落到 profile |
| 不设 viewport / user_agent / locale | `browser.py:15-16` | 锁视口是自动化特征；伪造 UA 会自相矛盾 |
| `--no-proxy-server` / `--proxy-bypass-list=*` | `browser.py:52-67` | 强制直连；代理出口 IP 更容易被标记 |
| 不加 `--no-sandbox` | `browser.py:152` | 它本身就是自动化特征 |

## 单车道：patchright 同步 API 绑死创建它的线程

`sidecar.py:41-44`：浏览器只在 `_run_scrape` / `_run_login` 这同一个后台线程里碰，两者互斥。
`sidecar.py:360-390`：聊天命令与抓取命令走**同一把 `_worker_lock`**，双锁
（`_chat_busy` + `_worker_lock`），worker 存活时 `raise busy_reason`。
`chat.py:9-12`：历史上 `Cannot switch to a different thread` 就是这么来的。

推论：

- 新增任何碰 `page`/`ctx` 的命令，必须在工作线程里跑，**不能**在主线程的 stdin 循环里直接调；
- 不能"为了快"开第二条线程——greenlet/线程绑定会立刻炸；
- 长任务期间其它命令只能排队，所以 Rust 那边对回执必须设超时（见 `sidecar-protocol`）。

## 注入层：哪些事不许在 Python 里做

`sidecar.py:1-16` 的 docstring 是判据原文：**无状态：不打分、不落库、不读配置——业务判定全部归 Rust**。

| 想做的事 | 允许吗 | 该放哪 |
|---|---|---|
| 解码 Boss 字体加密薪资（U+E031..U+E03A → 0-9） | ✅ 允许 | `boss.py:53-70` `decode_boss_salary`，产出仍是原始文案 |
| 判断"这条岗位达不达标" | ❌ 禁止 | Rust `core/sidecar/ingest.rs:502` `handle_job` |
| 判断"这条岗位重复/在敏感库" | ❌ 禁止 | Rust 预筛 `ingest.rs:424` `prescan`（卡片阶段就判） |
| 决定 `exclude_hr_active_days` 之类规则 | ❌ 禁止 | Rust `core/domain/rules.rs:190` `reject_labeled` |
| 写数据库（job_queue / sensitive_jobs / jd_decisions） | ❌ 禁止 | Rust `core/infra/sqlite/*` |
| 读 `app_config` 表 / 读 API Key | ❌ 禁止 | Rust `commands_config::read_config` |
| 调大模型打分 | ❌ 禁止 | Rust `core/domain/scoring_llm.rs` |
| 记"本次抓了多少条"的统计 | ⚠️ 只记过程日志，不是权威计数 | 权威计数在 Rust `ScrapeProgress`（`session.rs:597-615`） |
| 决定"是否应该重启 sidecar" | ❌ 禁止 | Rust `core/scrape/restart.rs:28` `restart_decision`（`core/scrape.rs` 只是 43 行 re-export 外壳） |

**唯一允许的"业务判断"边界**：`boss.py:327-328` —— 详情拿不到就不推送这条岗位，
理由写在代码里："没有 JD 的条目入库后会被规则判「无 JD 不入库」，白占一条数据库记录"。
这不是打分，是减少无意义传输。

数据形态上的对应关系：Python 用 `_job_to_event`（`sidecar.py:161-175`）只保留 21 个键，
空值不发，`jd_text` / `company_intro` 经 `clamp_text` 截断。
Rust 侧 `ScrapedJob`（`protocol.rs:209-236`）全部字段可缺，只有 `job_id` 必填。

## 新增/修改一条命令的检查表

1. `sidecar.py:521-572` `handle()` 加分支；兜底分支是 `UNKNOWN_CMD`（:572）。
2. 需要浏览器就必须经 `_worker_lock` 派到工作线程（照 `_with_chat` / `start_scrape` 的写法）。
3. 成功 `emit({"id": cid, "ok": True, "data": ...})`；
   失败 `emit({"id": cid, "ok": False, "error": {"code": "...", "msg": "..."}})`——
   **`error` 是对象**，Rust `codec.rs:192-212` 认 `msg`/`code`。
4. 同步改 Rust 侧 `Command` 枚举（`protocol.rs:69-129`）与 `handle()` 的参数名（**平铺，不加 params**）。
5. `python -m compileall -q scraper`。

## Non-Negotiables

1. DOM-only：不加接口通道（三条理由见 `boss.py:1-23`）。
2. 翻页只认「卡片数增长」（`boss.py:364`）。
3. `hr_active_status` 走 `.job-boss-info .boss-active-time`，且必须过正则再回传。
4. 监听器只收集响应对象，正文回主流程读（`chat.py:16-24`）。
5. `scrape.stop` 只置标志；收尾靠 Rust 关 stdin（`handle.rs:341-368`）。
6. 所有等待走可中断 `sleep`，不用 `time.sleep`（`sidecar.py:72-85`）。
7. 随机延时 1.5~9.8 秒不许调小（`base.py:61-62`）。
8. 不引入 `AJA_NO_STEALTH` 之类的"反检测开关"；调试用独立 profile。
9. 碰浏览器的命令必须排队到同一个工作线程。
10. 不打分、不落库、不读配置、不持业务状态（`sidecar.py:1-16`）。
11. `emit()` 必须 flush，且总行不超过 1 MB（`sidecar.py:47-57` 超限改成一条 error 日志）。
12. stdout 只走协议：`print()` 一行调试就会污染协议流，被 Rust 判成 `Malformed`
    （`backend-tauri/src/core/sidecar/process/codec.rs:38-43`：Python traceback 混进 stdout
    就是该模块存在的理由）。日志请走 `emit` 的 `log` 事件。
