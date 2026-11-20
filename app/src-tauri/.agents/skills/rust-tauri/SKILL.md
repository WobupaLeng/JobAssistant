---
name: rust-tauri
description: Tauri 2 desktop shell work in the jobassistant-app crate - command registration, IPC argument compatibility, AppState management, capabilities, sidecar child processes, build and startup diagnostics. Use when adding or changing a #[tauri::command], wiring state with manage(), touching capabilities/default.json or tauri.conf.json, spawning the Python sidecar, or debugging a packaged exe that white-screens or fails with os error 5. Not for writing domain rules (use rust-domain-purity) or for anything under frontend/ (use the leptos skill).
---

# rust-tauri — Tauri 2 壳层（`jobassistant-app`）

导语：本 crate 的壳层只有一件事——**把前端的 `invoke()` 翻译成 `core::` 的函数调用，
再把 `Result` 的错误转成字符串**。壳层里的每一行业务判断都会让 `core` 的单测覆盖不到它。

> 版本锚点（改前先看 `app/src-tauri/Cargo.toml`）：`tauri = "2"`、`tauri-plugin-shell = "2"`、
> `window-vibrancy = "0.8"`、`windows-sys = "0.59"`（仅 `cfg(windows)`）、edition 2021 / rust-version 1.77。

---

## Routing Trigger

命中以下任一情形才读本 skill：

- 要加 / 改 / 删一条 Tauri 命令，或改它的参数结构体
- 前端报「参数没生效 / 值是默认值」，而你怀疑是 `invoke` 的参数名对不上
- 要往 `Builder` 里塞全局状态（`manage()`）
- 要动 `capabilities/default.json` 或 `tauri.conf.json`
- sidecar 起不来、起得来但卡住、崩了不报原因
- 打包后的 exe 白屏 / 启动即 `os error 5` / `0x800700AA` / 「另一个实例正在运行」

不读本 skill：`core/domain/**` 的规则改动、`frontend/**` 的任何改动、`scraper/**` 的 DOM 抓取。

---

## The Reference Map

| 要读什么 | 文件 | 读它为了 |
|---|---|---|
| 启动编排、单实例锁、毛玻璃、命令总表 | `app/src-tauri/src/lib.rs` | 45 条命令的唯一注册点（473-522）、`manage()` 清单（412-425）、启动前置检查（392-410） |
| 命令参数与 alias 的写法范本 | `app/src-tauri/src/commands_db.rs` | `ListJobsArgs`(103-128) / `ScoreArgs`(311-328) / `RescoreArgs`(451-471) |
| 抓取命令与进程级互斥 | `app/src-tauri/src/commands_scrape.rs` | `ScrapeState`(35)、`LAST_SPEC`(37)、`RESTART_LOCK`(44)、`ScrapeArgs`(163-176) |
| 会话启停 / 重启策略 / 入库端口 | `app/src-tauri/src/core/scrape.rs` | `resolve_script_dir`(42-58)、`make_sink`(90)、`restart_decision`(243-258) |
| sidecar 协议帧 | `app/src-tauri/src/core/sidecar/protocol.rs` | `#[serde(rename = "scrape.start")]`(72)、`Envelope` |
| 序列化 / 解析的唯一入口 | `app/src-tauri/src/core/sidecar/process/codec.rs` | `protocol_line`(23)、`parse_line`(54-70)、`LineParse`(36-52) |
| 权限面 | `app/src-tauri/capabilities/default.json` | 只有 5 条权限，`windows: ["main"]` |
| 窗口 / CSP / frontendDist | `app/src-tauri/tauri.conf.json` | `frontendDist: "../../frontend/dist"`、`transparent: true` |

---

## Non-Negotiables（违反即静默出错，不报错、不崩、只是不对）

1. **命令参数必须双向兼容，`alias` 不能删。**
   前端 `serde_wasm_bindgen::to_value(&args)` 不自动转驼峰（`frontend/src/api/mod.rs:31-33`），
   实际发的是混写：`scrape_start` 发 `{"platform","keywords","maxJd","minScore"}`（`frontend/src/api/mod.rs:114-123`），
   `get_job` 发 `{"jobId"}`（:71），而 `set_limit` 发 `"minScore"`（:307）。
   Rust 侧字段是蛇形；**serde 对未知字段不报错，只会把它静默丢弃**——界面上设的值于是变成默认值，
   且**测试全绿**。新参数一律照抄这几处写法：结构体加 `#[serde(rename_all = "snake_case")]`，
   驼峰字段逐个加 `#[serde(alias = "xY")]`
   （`commands_db.rs:99-101` 是这条规矩的注释原点；`commands_db.rs:112/115/118`、`:198`、`:252`、
   `:320/323`、`:455/462/464`；`commands_scrape.rs:172/174`；`commands_auto.rs:72`；
   `core/domain/app_config.rs:152-228` 一整批）。
   双向兼容有实测用例：`commands_db.rs:864-875`（`{"useLlm":true}` 与 `{"use_llm":true}` 都认）。
   → **新增字段后必须补一条同形态的双写用例**，否则下一次改前端就会静默失效。

2. **`invoke_handler` 是单点，且是唯一注册入口。**
   `lib.rs:473-522` 用 `tauri::generate_handler![...]` 注册了全部 45 条命令（16+6+7+4+5+2+2+2+2，按命令层文件分组）。
   忘了登记不会编译报错，只会表现为前端 `invoke` 被拒。改动该文件属于高冲突单点（根 `AGENTS.md` 第 2.5 节），**同一时刻只允许一个写者**。

3. **全局状态一律 `manage()`，命令里用 `State<'_, T>` 取。**
   `lib.rs:412-425` 注册了 6 个：单实例锁、`StoreState(Mutex<Option<Arc<SqliteStore>>>)`、
   `LlmState`、`ScrapeState`、`AutoGreetState`、`AppPaths`。
   store 用 `Mutex<Option<..>>` 而不是裸 `Arc`，是因为数据库打开失败时应用**照常启动**、由命令层报错；
   这条「不阻断启动」的取舍在 `lib.rs:426-472`（setup 里失败只 `eprintln`）。
   `ScrapeState` 的 `Mutex<Option<SidecarSession>>` 同理：会话只在用户点开始抓取时才创建，
   停止 / 重开要能彻底回收子进程（`commands_scrape.rs:31-35`）。

4. **命令层只做「收参数 → 调 core → 错误转字符串」。**
   会话怎么起、崩几次就放弃、敏感库怎么查，全在 `core::scrape`
   （`commands_scrape.rs:1-15` 模块注释；`restart_decision` 在 `core/scrape.rs:243-258`）。
   进程级互斥（`LAST_SPEC` / `RESTART_LOCK`）留在命令层是因为它们属于「进程全局」而非领域规则
   （`commands_scrape.rs:14-15`、`:39-44`）。

5. **sidecar 的 JSON 参数必须平铺，不许包 `params`。**
   Python 侧 `handle()` 读的是同一层的 `cmd` / `keywords`；序列化唯一入口是
   `codec::protocol_line`（`process/codec.rs:23`）。
   回归用例 `process/tests.rs:20-36` 显式断言 `v.get("params").is_none()`。
   命令名由 `#[serde(rename = "scrape.start")]` 决定（`protocol.rs:72`）——**不要**再手写一份命令字符串。

6. **构建产物必须放到工作区外。**
   `npx tauri build --no-bundle` 才内嵌前端；`cargo build --release` 不内嵌 → 启动**白屏**。
   产物放工作区内则 exe 继承 Low 完整性标签（`S-1-16-4096`），启动写 `%LOCALAPPDATA%` 时
   报 `os error 5` / `0x800700AA`——**这不是代码 bug**。诊断报文与指引由
   `lib.rs:284-304` 的 `not_writable_hint` 生成（内含 `CARGO_TARGET_DIR`、`D:\jt`、「工作区外」），
   用例 `lib.rs:527-558` 锁定这几个关键词。标准做法：`$env:CARGO_TARGET_DIR = "D:\jt"`，跑 `D:\jt\release\jobassistant-app.exe`。

7. **单实例锁是命名互斥体，不是 `app.lock` 文件。**
   `lib.rs:340-385`：`CreateMutexW("Local\\com.ai.jobassistant.rust.single")`，
   `ERROR_ALREADY_EXISTS`(183) / `ERROR_ACCESS_DENIED`(5) 两条分支给不同的中文提示，
   句柄刻意泄漏到进程退出。
   注释 `lib.rs:335-339` 明确记录了**为什么不用锁文件**：旧的 `app.lock` 方案实测
   `share_mode(0)` 排他打开 `instance.lock` 返回 `os error 5`。
   → 改互斥体名前先想清楚：同一台机器上两轨并存会互相挡门。
   `lib.rs:347-348` 还留了一个坑：`windows-sys` 的路径必须写 `::core::ffi` 绝对路径，否则会被本 crate 顶层 `core` 模块解析掉。

8. **capabilities 只放必要权限。**
   `capabilities/default.json` 只有 `core:default` + 4 条 window 权限（拖动 / 最小化 / 最大化 / 关闭），
   该文件 description 写死的口径是「渲染进程不得直接调用系统 API 或任意子进程，一切走 invoke 安全桥接」。
   想加 `shell:allow-execute` 之类的权限前先问：这条能力为什么不能走一条 `#[tauri::command]`。

9. **`transparent: true` 与 CSP 是毛玻璃的地基。**
   `tauri.conf.json` 里 `transparent: true`、`csp` 为 `default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'`。
   改 CSP 会直接打破 `style-src 'unsafe-inline'`（内联样式是 Liquid Glass 的关键路径）。
   `frontendDist` 指向 `../../frontend/dist`，所以**必须先 `trunk build` 再 `npx tauri build`**。

---

## Workflow

### 加一条命令

1. 在对应的 `commands_<域>.rs` 写 `#[tauri::command] pub fn xxx(...) -> CmdResult<T>`。
   - 有参数就定义 `#[derive(Deserialize)]` 的结构体，套 `#[serde(rename_all = "snake_case")]`，
     驼峰字段加 `#[serde(alias = "...")]`（见铁律 1）。
   - 业务判断一律下沉到 `core::`；本层只允许「校验入参形状 → 调 core → 错误转字符串」。
2. 在 `lib.rs:473-522` 的 `generate_handler![...]` 里登记（**漏了不会编译报错**）。
3. 补两条测试：一条驼峰 / 蛇形双写反序列化（范本 `commands_db.rs:864-875`），
   一条缺省值语义（范本 `commands_db.rs:900-909`）。
4. 前端同步改 `frontend/src/api/mod.rs` 与 `types.rs`（契约同源，三处必须同一次提交改对端）。

### 碰 sidecar

1. 起停编排改 `core/scrape.rs`（`launch_session` 167 / `ensure_session_locked` 203 / `restart_decision` 243）。
2. 崩溃重启只允许一次：`restart_decision` 的三条早退（用户主动停 → `Skip`；退出码 0 → `Skip`；
   已重启过 → `GiveUp`）——无限重启只会反复起浏览器、反复弹验证（`core/scrape.rs:240-258`）。
3. 起不来时**必须留痕迹**：`resolve_script_dir` 会打 `[sidecar] 解析目录 ...`
   （`core/scrape.rs:49`），历史上 sidecar 静默死亡就是没人打这行。
4. 想验证真进程：`cargo test --test sidecar_smoke -- --ignored --nocapture`（见 `rust-testing`）。

### 打包与启动冒烟

```powershell
# 环境
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:CARGO_TARGET_DIR = "D:\jt"
$env:TEMP = "$env:TEMP"; $env:TMP = $env:TEMP

# 前端先进 dist（否则 frontendDist 是空的 / 旧的）
cd frontend
Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue   # trunk 只在项目内找产物
trunk build

# 壳层打包：必须 tauri build；cargo build --release 不内嵌前端 → 白屏
cd ..\app
npx tauri build --no-bundle

# 冒烟只能用工作区外的产物
D:\jt\release\jobassistant-app.exe
```

排障顺序：白屏 → 查是不是 `cargo build` 的产物 / `frontend/dist` 是否存在；
`os error 5` / `0x800700AA` → 查 exe 是不是在仓库里（Low 完整性标签）；
「另一个实例正在运行」→ 查有没有残留进程持有命名互斥体。

---

## 本模块实证速查

| 现象 | 真因 | 依据 |
|---|---|---|
| 前端传的参数没生效，值总是默认 | serde 未知字段静默丢弃，缺 `alias` | `commands_db.rs:99-101`、`commands_db.rs:864-875` |
| 新命令前端调到就被拒 | 忘了登记 `generate_handler!` | `lib.rs:473-522` |
| sidecar 收到命令但读到空参数 | 参数被包进了 `params` | `process/tests.rs:20-36`（`v.get("params").is_none()`） |
| 打包后白屏 | 用了 `cargo build --release` | `tauri.conf.json` 的 `frontendDist` |
| 启动即 `os error 5` | 产物在工作区内，继承 `S-1-16-4096` | `lib.rs:284-304`、`lib.rs:527-558` |
| 「另一个实例正在运行」 | 命名互斥体被占（不是锁文件） | `lib.rs:340-385` |
| 崩溃重启无限循环 | 绕过了 `restart_decision` | `core/scrape.rs:243-258` |
