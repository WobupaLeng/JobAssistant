---
name: sidecar-protocol
description: Rust ↔ Python sidecar 的 JSON Lines 行协议：帧形态判据、LineParse 三态语义、ScrapedJob 字段表、新增帧类型的完整同步清单、单车道（greenlet 线程绑定）在协议层的体现。Use when touching app/src-tauri/src/core/sidecar/protocol.rs、process/{codec,handle}.rs、session.rs 的帧路由，或给 scraper/sidecar.py 增删命令与事件。Not for 抓取选择器/翻页/反爬（见 sidecar-scraping），也不是 Tauri 命令层 IPC（见 rust-tauri）。
---

# sidecar-protocol：Rust ⇄ Python 的行协议

主进程 spawn `python scraper/sidecar.py`，双方只用 **stdin/stdout 上的 JSON Lines** 通信。
一行就是一条帧，没有长度前缀、没有握手协议、没有请求序号之外的关联机制。
"哪一条回执属于哪一个请求" 完全靠 `id` 字段与一张 `HashMap`。

本 skill 只讲协议本身。抓取侧行为见 `sidecar-scraping`，Tauri 命令层 IPC 见 `rust-tauri`。

## Routing Trigger

命中任一项就按本 skill 办事：

- 要新增/删除/改名一个 `Command` 变体或 `Event` 变体
- 要新增一个 `ScrapedJob` 字段（或改它的类型）
- 界面上出现 "无法解析：{...}"、回执收不到、超时、聊天回执串台
- 改 `parse_line` / `route_reply` / `protocol_line`
- 排查 "sidecar 起来了但什么事件都没有"

## Version Anchor

- 协议真源：`app/src-tauri/src/core/sidecar/protocol.rs`（435 行）
- 编解码与读线程：`app/src-tauri/src/core/sidecar/process/codec.rs`（278 行）
- 进程句柄：`app/src-tauri/src/core/sidecar/process/handle.rs`（410 行）
- 消费与记账：`app/src-tauri/src/core/sidecar/session.rs`（1154 行）
- Python 对端：`scraper/sidecar.py`（605 行）的 `handle()` :521-572 与 `emit()` :47-57
- 单行上限 1 MB：`protocol.rs:7` `MAX_LINE_BYTES = 1024 * 1024`，Python 侧 `sidecar.py:27`

## The Reference Map

| 你要干的事 | 先读 |
|---|---|
| 分不清一条行是命令/回执/事件 | `codec.rs:36-97`（`LineParse` 与 `parse_line`）、`codec.rs:159-190`（`route_reply`） |
| 加字段到 `ScrapedJob` | `protocol.rs:209-236`、`protocol.rs:243-278`（`from_value`）、`scripts` 白名单 `sidecar.py:161-175` |
| 加一条命令 | `protocol.rs:69-129`、`codec.rs:23-34`、`sidecar.py:521-572` |
| 加一条事件 | `protocol.rs:184-207`、`codec.rs:73-96`、`sidecar.py:47-57` |
| 回执路由出问题 | `handle.rs:238-284`（`request`）、`codec.rs:159-190`、`process/tests.rs:179-280`（六个 route_reply 用例） |
| 进程起不来/关不掉 | `handle.rs:133-218`（`spawn`）、`handle.rs:341-368`（`shutdown`）、`handle.rs:404-409`（`Drop`） |
| 线程绑定/greenlet 报错 | `sidecar.py:41-44`、`sidecar.py:360-390`、`chat.py:1-25` |

## 帧的三形态与判据

三种帧共用**同一个** `Envelope`（`protocol.rs:16-34`），五个字段全部
`Option` + `#[serde(skip_serializing_if = "Option::is_none")]`：

```
Envelope { id: Option<String>, event: Option<String>, ok: Option<bool>,
           data: Option<Value>, error: Option<ErrorInfo> }
ErrorInfo { code: String, msg: String }        // protocol.rs:10-14
```

| 形态 | 方向 | 判据（代码位置） |
|---|---|---|
| **命令** | 主进程 → sidecar（stdin） | 有 `cmd` 字段。Rust 侧由 `#[serde(tag = "cmd")]` 的 `Command` 枚举产生（`protocol.rs:69`）；序列化入口唯一：`codec.rs:23-34` `protocol_line` |
| **回执** | sidecar → 主进程（stdout） | **无 `event`**（`codec.rs:167-169` 返回 false）。有 `id` 才可能被路由（`codec.rs:170-172`） |
| **事件** | sidecar → 主进程（stdout） | **有 `event`**。`Event` 枚举 `#[serde(tag = "event")]`（`protocol.rs:184`） |

### 铁律 1：不能只看 `ok` 判回执

`codec.rs:62-66` 的注释把这条写死了：回执只有 `ok` 没有 `event`，但**事件帧里也带 `ok`**
（`{"event":"log","ok":true}`）。所以判据是 "没有 `event`"，不是 "有 `ok`"。
`process/tests.rs:109-127` 的 `parse_line_keeps_events_that_also_carry_ok` 就是这条的回归用例。

### 铁律 2：命令参数**平铺**，没有 `params` 包裹

`codec.rs:20-22` 注释 + `process/tests.rs:20-36` 断言 `v.get("params").is_none()`。
Python 侧 `handle()` 用 `cmd.get("keywords")` 直接读（`sidecar.py:521-572`）。
加一层 `params` 会立刻让所有参数变成缺省值——**而且不报错**。

### 铁律 3：一行一次写、带 flush

Rust 写：`handle.rs:286-305` `write_line` 一次 `write_all` 写完整行再 flush
（:298 注释：分两次写会让对端读到半行）。
Python 写：`sidecar.py:47-57` `emit()` 里 `sys.stdout.write(line+"\n")` 后立即 `flush()`；
不 flush 父进程按行读会一直阻塞等不到回执。

### 铁律 4：编码强制 UTF-8，双向都要

- Python 侧 `sidecar.py:580-581`：`sys.stdout.reconfigure(encoding="utf-8", errors="replace")`，stdin 同。
  不强制时 Windows 默认 cp936/GBK，中文 JD 直接乱码（:576-579 注释记录了实测）。
- Rust 侧 `handle.rs:150-157`：`python -u` + `PYTHONIOENCODING=utf-8` + `PYTHONUTF8=1`。

## LineParse 三态：`Ignore` ≠ 解析失败

`codec.rs:36-52`：

| 变体 | 语义 | 触发 |
|---|---|---|
| `Event(SidecarEvent)` | 该冒泡给抓取流程 | `ready` / `job.scraped` / `log` / `login.state`（`codec.rs:73-93`） |
| `Ignore` | **结构合法但与本流程无关**：命令回执、未知 `event` 名 | `codec.rs:67-70`（无 `event`）、`codec.rs:94-95`（未知事件名） |
| `Malformed` | 真畸形：非 JSON、非对象、`job.scraped` 缺 `job_id` | `codec.rs:55-61`、`codec.rs:80-82` |

`codec.rs:38-43` 记录了这次事故的原始形态：早先读线程写
`parse_line(line).unwrap_or_else(|| Malformed)`，把 `Ignore` 也当失败，
于是 `scrape.start` 的回执 `{"id":"c1","ok":true,...}` 在界面上冒出 "无法解析"，
**而单测全绿**——因为测试只断言了 `is_none()`，没断言调用方怎么解释这个 `None`。
回归用例：`process/tests.rs:92-107` `scrape_start_reply_is_not_malformed`。

> 不要把 `Ignore` 折进 `Malformed`，也不要给 `Ignore` 加"顺手打条日志"——
> 每条命令回执都会走这条路径，打日志会把 feed 淹掉。

## 回执路由：route_reply 先行消费

读线程的处理顺序在 `codec.rs:135-144`，**顺序本身是契约**：

```rust
if route_reply(line, &pending) { continue; }   // 1. 先试回执
match parse_line(line) {                        // 2. 再解析事件
    LineParse::Event(ev) => ev,
    LineParse::Ignore => continue,
    LineParse::Malformed => SidecarEvent::Malformed { .. },
}
```

`route_reply`（`codec.rs:159-190`）的四道闸：

1. 非 JSON → false（:164-166）
2. **有 `event` → false**（:167-169）——事件帧即使回显了 `id` 也不能当回执
3. 无 `id` → false（:170-172）
4. `pending.remove(id)`，命中才投递（:173-189）。:177 注释：
   "remove 而不是 get：回执只该被消费一次"；未命中 → false（:181 "不是我们发的，或早已超时放弃"）

`error_text`（`codec.rs:192-212`）必须同时认字符串与**对象**：
Python 发的是 `{"code":"EXPECT_FAILED","msg":"..."}`，早先只认字符串，
每个失败帧的原因都被吞成 `None`（:194-197 注释）。

配对机制：`handle.rs:224-236` `next_id()` 用 `AtomicU64` 自增（`c1`、`c2`…），
`handle.rs:96-110` **手写 `Clone`**——`AtomicU64` 自带的 `Clone` 是复制**值**，
两个句柄会从同一个值发号，聊天回执被投给错误的等待者，且不报错。
`handle.rs:238-284` `request` 超时要摘表并返回 "等待 sidecar 回执超时（{} 秒），命令可能仍在执行"。

## ScrapedJob：21 个字段，命名与 job_queue 列名逐字一致

`protocol.rs:209-236`，`#[derive(Default)]`，每个 `Option` 都**故意**可缺：

| # | 字段 | 类型 | 说明 |
|---|---|---|---|
| 1 | `job_id` | `String` | **唯一必填**。缺它 `from_value` 返回 `None` → `Malformed`（`protocol.rs:255`、`codec.rs:80-82`） |
| 2 | `platform` | `String` | 缺省 `"boss"`（`protocol.rs:256`） |
| 3 | `title` | `Option<String>` | |
| 4 | `company` | `Option<String>` | 敏感库键由 平台+公司+岗位名 组成 |
| 5 | `salary` | `Option<String>` | 原始文案（含 Boss 字体加密字符，Python 侧先解码） |
| 6 | `experience` | `Option<String>` | |
| 7 | `degree` | `Option<String>` | |
| 8 | `location` | `Option<String>` | |
| 9 | `job_url` | `Option<String>` | |
| 10 | `jd_text` | `Option<String>` | **没有它会被规则判"无 JD 不入库"**（`boss.py:327-328` 因此宁可不推送） |
| 11 | `company_industry` | `Option<String>` | |
| 12 | `company_scale` | `Option<String>` | |
| 13 | `company_stage` | `Option<String>` | 融资轮次 |
| 14 | `company_intro` | `Option<String>` | |
| 15 | `company_address` | `Option<String>` | |
| 16 | `hr_name` | `Option<String>` | |
| 17 | `hr_position` | `Option<String>` | |
| 18 | `hr_active_status` | `Option<String>` | **空值会让 `exclude_hr_active_days` 把岗位全判成"活跃度未知"** |
| 19 | `founded_years` | `Option<u32>` | 接受数字或字符串（`protocol.rs:249-253`，`sidecar.py` 从成立年份算出来） |
| 20 | `reg_capital` | `Option<String>` | |
| 21 | `matched_keyword` | `Option<String>` | 命中的搜索词 |

Python 侧的白名单在 `sidecar.py:161-175` `_job_to_event`：
只 keep 这 21 个键，且 `if item.get(k) not in (None, "")`——**空值不发**。
所以 `from_value` 必须容忍字段缺席（`protocol.rs:241-242` 注释：
"用 Value 逐个取可以忽略未知字段，不会整体失败"）。

## 新增帧类型的完整同步清单

漏任何一步的症状都不一样，且**多数不报错**。按顺序过：

### 新增一条命令（主进程 → sidecar）

1. `protocol.rs:69-129` 加 `Command` 变体 + `#[serde(rename = "...")]` 线上名。
   参数**平铺**在变体字段里，不要包一层 `params`。
2. `codec.rs:23-34` `protocol_line` 无需改动（它对任意 `Command` 通用），
   但**必须**确认新变体的字段是 `Serialize`，且读端（Python）按 `cmd.get("字段")` 取。
3. `scraper/sidecar.py:521-572` `handle()` 加分支。兜底分支是
   `{"code": "UNKNOWN_CMD"}`（:572）——没加分支的表现是**回执报未知命令**，这还算好的。
4. 若命令会写回执：Python 用 `emit({"id": cid, "ok": True, "data": ...})`，
   失败用 `{"id": cid, "ok": False, "error": {"code": "...", "msg": "..."}}`，
   **error 必须是对象**（`codec.rs:192-212` 认 `msg`/`code`）。
5. `process/tests.rs` 补一条 `protocol_line_*` 断言（照 :20-36 的样子断言字段名与平铺）。

### 新增一条事件（sidecar → 主进程）

1. `protocol.rs:184-207` 加 `Event` 变体与线上名。
2. `codec.rs:73-96` `parse_line` 加分支。**不加**的后果是落到 `_ => LineParse::Ignore`（:94），
   事件被静默丢弃，界面上什么也没有。
3. `process/tests.rs:47-74` 的 `parse_line` 断言表加一例。
4. 若事件与抓取无关（只是进度/日志）：走已有 `log` 事件，别加新事件名。
5. Python 侧用 `emit({"event": "...", "data": {...}})`（`sidecar.py:47-57`）。
   **不要**在事件帧里塞 `id` 去冒充回执——`route_reply` 会先看到 `event` 而拒绝（`codec.rs:167-169`），
   这一帧于是既不路由也不报错（`Ignore`）。

### 新增回执字段

`codec.rs:183-189` 组装 `SidecarReply { ok, data, error }` 时 `data` 原样透传，
不用改 Rust；但**如果**新字段是为了让调用方判断失败原因，要优先放进 `error.msg`
（`error_text` 会在 `msg` 为空时退到 `code`）。

### 改动后必须验证的三件事

1. `cd app/src-tauri; cargo test --lib core::sidecar::process` —— 20 个 protocol/route_reply 用例。
2. `python -m compileall -q scraper`
3. 手动：`cargo test --test sidecar_smoke -- --ignored --nocapture`（只断言起得来/握手/关得掉）。

## 单车道铁律：patchright 同步 API 绑死线程，协议层怎么体现

"单车道" 不是协议字段，而是**协议两端的线程模型必须一致**才能成立：

### Rust 侧

- 事件接收端不在 `Arc<SidecarHandle>` 里（`session.rs:212-213` 注释：
  `mpsc::Receiver` 不是 `Sync`，塞进 `Arc` 会让句柄无法送进消费线程）。
- 三条读线程各管一条管道：stdout（`codec.rs:102-157`）、stderr（`codec.rs:214-244`）、
  退出（`codec.rs:246-267` 每 120ms `try_wait`）。
- `handle.rs:191-193` 注释：pending 表**必须在读线程启动前**建好；
  `handle.rs:198-200`：stderr 必须有人读，管道写满会同归于尽。
- `handle.rs:372-402` `wait_ready`：`ready` 之前到达的事件一律丢弃
  （:389-390 "否则调用方会看到「还没 spawn 完就抓到岗位」的时序幻觉"）。

### Python 侧（这是协议能成立的前提）

- `sidecar.py:41-44`：patchright 的 sync API 绑死创建它的线程，
  浏览器只在 `_run_scrape` / `_run_login` 这同一个后台线程里碰，两者互斥、不并发。
- `sidecar.py:1-16`：主线程只做 stdin 读循环——否则点"停止"要等整页抓完才生效。
- `sidecar.py:360-390`：聊天命令与抓取命令走**同一把 `_worker_lock`**，
  双锁 `_chat_busy` + `_worker_lock`，worker 存活时 `raise busy_reason`。
  协议上表现为：**同一时刻只有一条命令在执行**，回执可能很晚到，`request` 必须带超时。
- `chat.py:9-12`：历史上 `Cannot switch to a different thread` 就是跨线程调用引起的。

> 推论：**不要在 Rust 侧并发 `send` 两条命令然后等回执**。
> 第二条会排在 `_worker_lock` 后面，第一条的长任务（抓取详情页每条 3~6 秒）会让第二条超时。
> 抓取类命令因此设计成单向（`protocol.rs:66-68`：进度走事件流，只有 `scrape.start` 立刻回 `accepted`）。

## Non-Negotiables

1. `parse_line` 的 `Ignore` 不是失败；把两者合并会让每条回执在 feed 里冒一行 "无法解析"。
2. 判回执看 "没有 `event`"，不看 `ok`——事件帧也带 `ok`。
3. 命令参数平铺，不加 `params` 包裹。
4. `error` 是对象 `{code,msg}`，不是裸字符串。
5. 每行一次 `write_all` + `flush`；两侧都强制 UTF-8。
6. `ScrapedJob` 只有 `job_id` 必填，其余缺失不得让整条记录变畸形。
7. `job.scraped` 缺 `job_id` 算 `Malformed`（无法去重入库）。
8. 新增事件必须同时改 `Event` 枚举与 `parse_line`，否则落进 `Ignore` 静默消失。
9. `pending` 表配对靠 `AtomicU64` 自增 id；手写 `Clone` 不能复制计数器值。
10. 超时路径要摘表并给出 "命令可能仍在执行" 这句人话，不要只说 "超时"。
11. `shutdown` 是**关 stdin** 让 Python 的 `for raw in sys.stdin` 走到 EOF，
    不是 `kill()`（`handle.rs:341-368`）；`Drop` 里 800ms 兜底（:404-409），
    否则 python 留在后台占着浏览器 profile。

## 排障速查

| 症状 | 先看 |
|---|---|
| feed 里冒出 `无法解析：{"id":...}` | `codec.rs:135-144` 的顺序是否被改；`Ignore` 是否被并进 `Malformed` |
| 回执永远等不到、超时 | `handle.rs:173-179` pending 是否在读线程启动前插入；`codec.rs:167-169` 是否把带 `id` 的事件当回执 |
| 中文全乱码 | `sidecar.py:580-581` 的 reconfigure 是否还在 |
| 参数全是缺省值 | 是否给命令加了 `params` 包裹；`serde` 未知字段不报错 |
| 某事件界面永远收不到 | `parse_line` 是否漏了分支（落进 `Ignore`） |
| 聊天回执投给了别人的请求 | `handle.rs:96-110` 手写 `Clone` 是否被换成 `#[derive(Clone)]` |
| 关应用后 python 还在 | `handle.rs:404-409` `Drop` → `shutdown` 是否还在 |
