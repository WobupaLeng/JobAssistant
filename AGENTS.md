# AI 求职助手 · 工程宪法（AGENTS.md）

> 本文件是仓库根目录**唯一**的工程治理文档。AI 每次动手前必读；任何模式不得私自降低第 0 节状态。
> 2026-11-21 全文重写：只描述**当前在做的这一条主线**（Rust 核心 + Tauri 2 壳 + Leptos WASM 界面 + Python 抓取 sidecar）。历史上任何被取代的实现都不在本文件的治理范围内。

## 0. 工程状态看板（AI 每次动手前必读）

- **当前阶段状态**：`FRONTEND_REBUILD_IN_PROGRESS`
- 下表数字的取证基准：**提交 `6f9a76e`**（`app/` 与 `frontend/` 当前有未提交改动悬在磁盘上，编译结论以该提交为准）。

| 层 | 状态 |
|---|---|
| `app/src-tauri/src/core/`（并入的 Rust 领域 + 存储 + 协议） | ✅ 可用，300 个单测（`#[test]` 计数） |
| `scraper/`（Python sidecar） | ⚠️ 抓取 / 登录 / 聊天 / 求职期望已通，**从 Tauri 子进程里拉起真实 Chrome 仍失败**（见 6.2） |
| `app/src-tauri/`（Tauri 壳 + 命令层） | ✅ **48 条命令已注册**（`invoke_handler` 48 项，另有 1 条 `#[tauri::command]` 未注册）；`cargo test --lib` 实测 **367 通过** |
| `frontend/`（Leptos 0.8 WASM） | ✅ 四页（概览 / 消息 / 配置 / 日志）全部实现，抽屉、骨架条、过渡动画已接线；🔨 Liquid Glass 第二三层（湍流折射 / WebGL 真折射）未做 |

- **允许的并发形态**：`app/src-tauri/src/core/`（`[后端编码]`）与 `frontend/`（`[前端编码]`）目录完全隔离，可并发派发。
  **降级为单线程**：涉及 `app/src-tauri/src/core/domain/schema.rs`、`app/src-tauri/src/core/sidecar/protocol.rs`、`scraper/sidecar.py`、`app/src-tauri/src/lib.rs`、`app/src-tauri/Cargo.toml` 任一文件时（高冲突单点，同一时刻只允许一个写者）。
- **Mock 全局开关**：`DISABLED`。本项目没有 Mock 服务：前端通过 `window.__TAURI_INTERNALS__.invoke` 直连同进程 Rust；浏览器里单独调试时才用 `__TAURI_INTERNALS__` 桩（调试代码**不得**进入 `src/` 的正式路径）。
- **状态变更留痕（只升不降）**：
  - 2026-11-21：`REWRITE_IN_PROGRESS` → `FRONTEND_REBUILD_IN_PROGRESS`（用户决策：界面改用 Leptos 0.8 重写，设计语言定为 Liquid Glass，深/浅双主题各自调优，折射方案 C）。
  - 2026-11-21：前端概览页打通（`cargo check` exit 0 / `trunk build` 成功 / 深浅双主题截图验收通过）。
  - 2026-11-21：`core/` 独立 crate **并入** `app/src-tauri/src/core/`（用户决策），`app` 的 `cargo test` 从 36 增至 297。分层改由模块边界 + code review 维持，`src/core/` 仍不得 `use tauri::`。
  - 2026-11-21：`app/src-tauri/tauri.conf.json` 的 `frontendDist` 改指 `../../frontend/dist`，Leptos 前端已能打进 exe（6.92 MB）。
  - 2026-11-21：看板数字校准到提交 `6f9a76e`——命令 **27 → 48**（实测注册项），单测 **297 → 367**（`#[test]` 计数 368，其中 1 个 `#[ignore]`）。此前「27 个命令 / 297 个单测」是并入 `core/` 当天的中间态，之后三个提交又加了 21 条命令与 70 个用例。

## 1. 项目形态与核心技术栈

### 1.1 一条主线，四个目录

```
frontend/  ──invoke()──▶  app/ (Tauri 2 壳)  ──直接调用──▶  core/ (Rust 领域)
   WASM                      命令层                         业务规则
                                  │
                                  └──stdin/stdout JSON Lines──▶  scraper/ (Python sidecar)
```

| 目录 | crate / 包 | 职责 | 不可妥协的边界 |
|---|---|---|---|
| `app/src-tauri/` | `jobassistant-app` 0.1.0 | Tauri 2 壳 + **内嵌业务核心 `src/core/`**：窗口、单实例锁、毛玻璃、命令注册、领域规则 | 命令层**只做「收参数 → 调 core → 错误转字符串」**；`src/core/` **不得 `use tauri::`** |
| `frontend/` | `aja-frontend` 0.1.0 | Leptos 0.8 CSR，编译为 WASM，跑在 WebView 内 | **不碰系统 API、不起子进程**，一切经 `invoke` |
| `scraper/` | Python 3.14 | patchright 浏览器自动化：抓取、登录、聊天、期望职位 | **不打分、不落库、不持有业务状态** |

**只读参考目录（禁止删除、禁止改动）**：`backend/`、`backend-exe/`、`web/`、`src-tauri/`、`skeleton-web/` 是**旧版实现**，用户明确要求保留作为参考。它们不在任何构建链路里，不参与门禁，但**任何模式都不得删除或"清理"它们**——曾有一次误删，靠 `git checkout HEAD -- <目录>` 才恢复（`backend-exe/` 未跟踪，无法恢复）。需要参考旧版行为时可直接读这些目录。

### 1.2 技术栈（版本以 Cargo.toml 为准）

- **`app/`（含内嵌核心）**：Rust edition 2021。Tauri 2、`tauri-plugin-shell 2`、`window-vibrancy 0.8`、`windows-sys 0.59`（仅 Windows 诊断读数与 `SetWindowCompositionAttribute`）；领域层依赖 `serde` / `serde_json` / `thiserror` / `regex` / `rusqlite 0.32`（`bundled`，SQLite 编进二进制）/ `ureq 2`（`json` + `tls`，rustls 而非 native-tls）。release 档：`codegen-units=1` / `lto=true` / `opt-level="s"` / `panic="abort"` / `strip=true`。
- **`frontend/`**：`leptos 0.8`（`csr`）、`serde-wasm-bindgen 0.6`、`wasm-bindgen 0.2`、`js-sys`、`web-sys`、`console_error_panic_hook`。构建工具 **trunk 0.22.2**。release 档：`codegen-units=1` / `lto=true` / `opt-level="s"`。
- **`scraper/`**：Python 3.14 + `patchright`（Playwright 反检测分叉）+ 本机真实 Chrome + 持久化 profile。

### 1.3 进程与通信模型

- **同进程**：前端和 Rust 之间没有 HTTP、没有端口。`invoke` → Tauri command → `core/`（同 crate，直接函数调用）。
- **sidecar 是子进程**：`app` spawn `python scraper/sidecar.py`，协议是 **stdin/stdout 上的 JSON Lines**，只有三种帧形态共用同一个 `Envelope{id,event,ok,data,error}`：
  - **命令**（含 `cmd`）→ 主进程写入 stdin；
  - **回执**（含 `id`、无 `event`）→ sidecar 写出，由 `process.rs::route_reply` 投递给等待方；
  - **事件**（含 `event`）→ sidecar 主动推送（`ready` / `job.scraped` / `log` / `login.state`）。

## 2. 目录分层与绝对红线

### 2.1 `app/src-tauri/src/core/`（业务核心，可按模块并发派发 `[后端编码]`）

> 2026-11-21 起这是 `app` crate 内的一个模块（用户决策：不再单独成 crate）。它**仍然不得 `use tauri::`**——分层从「编译器强制」降级为「模块边界 + `cargo test` 可独立跑」+ code review。

```
app/src-tauri/src/core/
  lib.rs                    scoring::{SCORE_MAX, SCORE_MIN_DEFAULT} —— 打分口径唯一真源
  domain/
    job.rs                  Job 实体 + JobStore 端口 trait + JobQuery + SortField
    store_ports.rs          JobAdminStore / SensitiveAdminStore / DecisionStore / ConfigStore
    scoring.rs              五维规则打分：技能 40 / 薪资 20 / 经验 15 / 方向 15 / 公司质量 10
    scoring_llm.rs          LlmClient trait + 大模型打分通道（三级容错解析）
    scoring_service.rs      规则 / 大模型双通道编排
    rescore.rs              全量重算（游标分页）
    rules.rs                十条排除规则，**顺序不可重排**
    parse.rs                薪资 PUA 解码、区间解析、经验要求、HR 活跃度
    profile.rs              DEFAULT_PROFILE 求职画像常量
    greeting.rs             打招呼：build_prompt / rule_greeting / write_greeting
    schema.rs               建表语句 SCHEMA / 索引 SCHEMA_INDEXES / 补列声明
    migration.rs            十分制 → 百分制幂等迁移
    mod.rs                  模块注册 + 再导出
  infra/
    sqlite.rs               JobStore / 四个管理 Store 的 rusqlite 实现
    llm.rs                  HttpLlmClient（ureq）
  sidecar/
    protocol.rs             Envelope / Command / Event / ScrapedJob 等全部帧与结构
    process.rs              目录外壳（42 行）：只 re-export，对外路径 core::sidecar::process::* 不变
    process/config.rs       SidecarConfig / locate_script_dir / PYTHON_ENV / DEFAULT_READY_TIMEOUT
    process/handle.rs       SidecarError / SidecarEvent / SidecarHandle：spawn / 写 stdin / 关停
    process/codec.rs        protocol_line / parse_line / LineParse / 回执路由 / 三条读线程
    session.rs              消费线程 + 进度记账 + SessionUpdate + request()
    ingest.rs               ScrapedJob → Job → 十条规则 → 打分 → 门槛（纯函数）
  scrape.rs                 会话启停编排：resolve_script_dir / make_sink / restart_decision / plan_restart
```

**分层铁律**：`domain` 不得依赖 `rusqlite` / `ureq` / `tauri`；`core` 不得依赖 `tauri`。存储走 trait 端口，`domain::job::JobStore` 定义签名，`infra::sqlite::SqliteStore` 提供实现。

### 2.2 `app/src-tauri/src/`（桌面壳，跨进程单点，**严禁**与前端/核心并发派发）

```
lib.rs              启动编排、单实例锁、毛玻璃、invoke_handler 注册（48 条命令）
commands_db.rs      数据库与打分（14 条）：list_jobs / get_job / db_overview / migrate_scores /
                    score_job / passes_threshold / update_score / rescore_jobs / llm_status /
                    job_tab_counts / reject_jobs / allow_jobs / mark_delivered / list_decisions
commands_scrape.rs  抓取与登录（10 条）：scrape_start / scrape_stop / scrape_progress / scrape_drain /
                    scrape_close / session_open / login_start / login_status / login_save / logout
                    （起停编排已下沉到 core/scrape.rs，本文件只剩收参数→转调）
commands_chat.rs    Boss 聊天（7 条）：chat_friends / chat_messages / chat_send / chat_resumes /
                    chat_send_resume / chat_recall / list_expectations
commands_config.rs  配置与阈值（4 条）：get_app_config / set_app_config / set_limit / set_scrape_defaults
commands_auto.rs    自动打招呼（已注册 5 条）：auto_greet_next / auto_greet_pause / auto_greet_resume /
                    auto_greet_status / auto_greet_reset（另定义 auto_greet_gap 未注册，见 6.2）
commands_sensitive.rs 敏感库（2 条）：sensitive_list / sensitive_remove
commands_logs.rs    运行日志（2 条）：list_logs / clear_logs
commands_greet.rs   打招呼（2 条）：greet_draft（生成）/ greet_one（发送）——刻意拆两步
tauri.conf.json     frontendDist / devUrl / 窗口与 CSP
```

**命令层职责**：只做「接收参数 → 调用 core → 错误转字符串」。**任何业务规则都不得写在 `commands_*.rs` 或 `lib.rs`**。

### 2.3 `frontend/`（Leptos WASM，可并发派发 `[前端编码]`）

```
frontend/
  index.html            trunk 入口（<link data-trunk rel="css"> 顺序：tokens → layout → controls → jobs → scrape）
  src/main.rs           bin 入口（trunk 要的是 bin 的 wasm，缺 [[bin]] 会报 artifacts not found）
  src/lib.rs            start()：panic hook + mount_to_body(App)
  src/api/{mod,types}.rs    契约类型 + invoke 封装；types.rs 无运行时代码
  src/lib_ui/           clock / job / logfmt / motion / nav / session / text / theme / time（纯函数 + 单测）
  src/components/       shell（顶栏 + `dispatch` 插槽，**不含页面分发**）+ widgets（Card/Stat/Chip/StatusDot/SkeletonRows/Banner/ErrorBar/Field/Collapsible/EmptyState）
  src/features/         auto / chat / config / jobs（state/list/cards/drawer/rescore）/ score / scrape（filter/options/panel/expectations）/ sensitive
  src/pages/            mod.rs 的 `Page` 组件承担页面分发；dashboard / chat / config / logs 均为正式实现
  styles/               tokens（Liquid Glass 设计令牌）/ layout / controls / jobs / scrape
  .agents/skills/       liquid-glass / leptos / apple-design / leptos-component
  AGENTS.md             前端模块约定（分层、玻璃三层分工、装载的 skill）
```

**依赖方向（单向，只能向下）**：`pages → features → components → lib_ui`；`api` 可被任意层引用，但 `lib_ui` 不得反向引用 `api`。页面分发在 `pages/mod.rs::Page`，由 `frontend/src/lib.rs` 的 `App` 作为 `dispatch` 插槽传给 `AppShell`——这样 `components` 不需要反向依赖 `pages`。

**前端红线**：
- **禁止硬编码满分与门槛**（100 / 70 之类），一律由 `get_scoring_spec` 下发。
- **页面背景必须透明**，否则毛玻璃失效。
- **可交互控件必须带 `data-testid`**，命名 `<页面/模块>-<功能>-<控件类型>`（如 `job-refresh`、`scrape-start-btn`）。
- **`data-testid` 与关键文案是 E2E 契约**：`tools/gui_e2e.py` 用正则解析 `scrape-progress` 的 innerText（`已抓 N · 入库 N · 重复 N · 低于门槛 N · 规则否决 N`）、用 `feed.count("✔")` 统计入库条数、检查 `scrape_start` 回执含「已启动抓取进程」。改这些文案前先改脚本。
- **CSS 文件不允许 `//!` 或 `///` 注释**。这是 Rust 语法，CSS 解析器会判定整份样式表非法并**静默丢弃**（曾导致「变量生效、组件样式全无」的误诊）。

### 2.4 `scraper/`（抓取 sidecar，协议单点，改动须同步 `app/src-tauri/src/core/sidecar/protocol.rs`）

```
sidecar.py   stdin 循环 + handle(cmd) 分发 + 工作线程 + 进度记账
boss.py      Boss 直聘 DOM 抓取：卡片解析 + 详情页补齐 + 无限滚动翻页（DOM-only）
chat.py      Boss 聊天：会话列表 / 历史消息 / 发送 / 简历 / 撤回（监听响应 + 主流程读正文）
browser.py   patchright 持久化上下文启动、登录态判定、求职期望读取
base.py      平台基类
```

**sidecar 职责**：只负责浏览器操作与 DOM/响应解析后回传原始字段。**不打分、不落库、不持有业务状态**。

**抓取铁律**：
- **DOM-only**：不要加回接口通道（`joblist.json` 只在首屏触发、翻页恒为空、丢字段）。
- **翻页是无限滚动**：判定用「卡片数增长」，不要用「滚动/点击成功」。
- **`hr_active_status` 选择器是 `.job-boss-info .boss-active-time`**；抓不到该字段时 `exclude_hr_active_days` 会按「不活跃」**否决全部岗位**。
- **监听器里绝不能调阻塞 API**：`page.on("response")` 的回调运行在 playwright 调度栈里，调 `resp.text()` 会 `Connection closed while reading the driver`，之后全部 `Event loop is closed`。只记响应对象，正文回主流程读。
- **`scrape.stop` 不结束进程**：只置停止标志。收尾必须 `scrape.stop` + 关 stdin（等价 `scrape_close`），否则进程挂在等 stdin，表现为「命令卡住」。

### 2.5 跨边界与安全红线

- [通用] 严禁在视图层或命令层写业务逻辑；外部入参必须严格校验（`serde` + 显式类型转换）。
- [通用] API Key、登录 cookie 只落数据库 `app_config` 表，**严禁**写进源码、日志、`localStorage` 明文或提交记录。
- [桌面端] 渲染进程严禁直接调系统 API 或任意子进程，必须走 `invoke`；sidecar 是 `app` 的子进程，前端不直接接触。
- [反爬] **严禁**设置 `AJA_NO_STEALTH=1` 跑真实抓取，**严禁**把随机延时（1.5~9.8 秒）改小；调试抓取必须用独立 profile。
- [并发写] `app/src-tauri/src/lib.rs`、`app/src-tauri/src/core/domain/schema.rs`、`app/src-tauri/src/core/sidecar/protocol.rs`、`scraper/sidecar.py` 为高冲突单点，**同一时间只允许一个写者**。
- [数据安全] **严禁**对 `data/*.db` 真实库做破坏性验证。迁移/写入验证必须在副本或临时目录进行，验证完删除。

## 3. 契约同源与联调

### 3.1 契约真源（三处，改任一处必须同一次提交内改对端）

| 契约 | 真源 | 必须同步的对端 |
|---|---|---|
| Tauri 命令签名 | `app/src-tauri/src/commands_*.rs` + `lib.rs` 的 `invoke_handler` | `frontend/src/api/mod.rs` 与 `types.rs` |
| sidecar 协议 | `app/src-tauri/src/core/sidecar/protocol.rs` | `scraper/sidecar.py` 的 `handle()` 与 `emit()` |
| 数据库结构 | `app/src-tauri/src/core/domain/schema.rs` | `app/src-tauri/src/core/infra/sqlite.rs` |

### 3.2 两条容易静默失效的规则

1. **命令参数必须双向兼容**：前端发驼峰（`minScore`）、Rust 字段是蛇形（`min_score`），serde 对未知字段**不报错**。新增命令参数时照 `ScrapeArgs` / `ScoreArgs` / `RescoreArgs` 的写法加 `alias`，否则界面上设的值会被静默丢弃。
2. **`parse_line` 的 `None` 语义**：`LineParse::Ignore` 表示「这行不该冒泡」（命令回执、未知事件），**不是**解析失败。命令回执由 `route_reply` 先行消费（判据：有 `id` 且无 `event`）。新增帧类型时务必同步 `LineParse` 枚举与 `route_reply` 的测试。

### 3.3 本地联调

- 开发态不使用 Mock。未配置大模型时走规则打分、大模型失败自动回退规则，这是**运行时兜底**，不得关掉。
- 浏览器里单独看界面：`cd frontend && trunk serve`（默认 8080），需用 `__TAURI_INTERNALS__` 桩提供假数据；桩脚本不得进入 `src/`。

## 4. 实证交付门禁（AI 完工前必须在终端真实执行）

### 槽位 A [静态编译与类型检查]

```powershell
# 环境（Windows 必配，两个变量都有理由）
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:CARGO_TARGET_DIR = "D:\jt"          # 短路径：避免 MSVC CL 命令行超限 D8050；
                                          # 且工作区内产物会继承 Low 完整性标签，启动必报 os error 5
$env:TEMP = "$env:TEMP"; $env:TMP = $env:TEMP

cd app/src-tauri;   cargo test               # 编译 + 367 个单测一次到位（核心 300 + 命令层 67）
cd frontend;        cargo check --target wasm32-unknown-unknown
cd frontend;        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue; trunk build
python -m compileall -q scraper
```

- **`trunk build` 必须清掉 `CARGO_TARGET_DIR`**：trunk 只在项目内找产物，指向工作区外会报 `cargo artifacts not found for target crate`；且它**不认** `--target-dir`。
- `trunk` 需要 **bin** 的 wasm 产物，`frontend/Cargo.toml` 里的 `[[bin]]`（`path = "src/main.rs"`）不可删。
- **每次门禁都要用独占的 `CARGO_TARGET_DIR`**：`D:\jt` 被上一次（其它 lane / 旧提交）的产物占着时，Cargo 可能直接重放陈旧二进制——实测重放过一份旧 exe，测试结果与当前源码无关。换一个干净目录（如 `D:\jt-check`）重跑，数字对得上再用。

### 槽位 B [单元测试]

- **`src/core/`（300 个用例）**：打分五维权重、排除规则十条顺序、PUA 薪资解码、幂等迁移、SQLite 读写、schema 列数锚点、LLM 三级容错与初筛线、sidecar 协议（`ScrapedJob` 21 字段 / `login.state` / 聊天帧平铺 / `session.open` 的 `page`）、回执路由六例、入库判定与四路计数、无 JD 否决在 ingest 路径上生效、全量重算游标分页。
  - **重算排序键铁律**：`JobStore::list_scored_ids` 必须按 `job_id` 升序。按 `match_score` 排序会被自己的写回推翻分页边界——同一岗位算两次、别的被跳过，**且没有任何报错**。
  - **无 JD 否决只能走 `RuleScore::passes()`**：无 JD 岗位降权后最高仍有 72 分，`score < min_score` 的写法在门槛 ≤70 时会放行。
  - **薪资排序的 NULL 语义**：SQLite 的 `ORDER BY x ASC` 把 NULL 排**最前**（与 MySQL/PG 相反）。`SALARY_UNKNOWN_RANK_EXPR`（面议/空=1，有值=0）必须**恒 ASC 且作为第一排序键**。
  - **Windows 必配 `$env:TEMP`**：否则 `core::infra::sqlite` 的临时库测试报 `create_dir_all` 权限拒绝（os error 5）。**不要改测试代码去绕**。
- **命令层 + 壳（67 个用例）**：排序字段注入防御、无 JD 否决、`ScoreArgs`/`ScrapeArgs` 驼峰与下划线双写、LLM 缺客户端回退规则、密钥只回尾 4 位、抓取增量转前端结构、未结束会话补结束原因、启动诊断报文、数据目录探测器。
- **`frontend/`（61 个用例）**：`lib_ui/`（clock / logfmt / motion / session / text / time）与 `features/`（auto / jobs / scrape / sensitive）的纯函数；`pages/` 仍是零用例，补齐是当前缺口。
- **sidecar 起停冒烟（默认忽略）**：`cd app/src-tauri && cargo test --test sidecar_smoke -- --ignored --nocapture`。只断言起得来 / 握手 / 关得掉，不断言抓多少条。

### 槽位 C [集成、E2E 与最终产物冒烟]

- **构建**：`cd app && npx tauri build --no-bundle`（**必须**用 `tauri build`：`cargo build --release` 不内嵌前端 → 白屏）。
- **启动冒烟必须用工作区外的产物**：`D:\jt\release\jobassistant-app.exe`。工作区内的 exe 一律跑不起来（见第 6.1）。
- **GUI 端到端**：`python tools/gui_e2e.py run --ui-only`（约 15 秒，不碰网络，CI 可跑，当前 5/5）。
  路径是 WebView2 远程调试端口 + patchright `connect_over_cdp` 驱动**真实 Tauri 窗口**。只测 WebView 页面的方案里没有 `__TAURI_INTERNALS__`，`invoke()` 全失败，等于什么都没测。
- **聊天只读验证**：`python tools/chat_readonly.py`（只点只读按钮，断言全程未触发写操作）。
- **抓取质量 / 翻页 / 字段取证**（三者分开跑，否则详情会吃光预算）：
  - `python tools/scrape_probe.py --keyword Java --pages 1 --limit 2 --budget 45`（每条详情 3~6 秒）
  - `python tools/page_probe.py --keyword Java --pages 3`（只验翻页，约 30 秒）
  - `python tools/detail_dump.py <job_id>`（区分「页面真没有」与「选择器没覆盖」）
  - **不要**用 `Select-Object -First N` 接长任务（会提前关管道杀掉子进程）。
- **前端分层校验**：`python tools/check_layers.py`（Leptos 版落地后需同步规则表，见 6.2）。
- **毛玻璃冒烟（手工）**：设 `AJA_DIAG_WINDOW=1` 启动，stderr 应出现 `[diag] hwnd=… layered=…` 且**不**出现「SWCA 亚克力不可用」。目视核对需 `SetForegroundWindow` + `CopyFromScreen`；**不能用 `PrintWindow`**（拿到的是应用自绘内容，会把已生效误判成没生效）。
- **前端视觉**：`trunk build` 后用 `.tmp-test/serve-frontend.cjs`（端口 5420）+ `make-preview.cjs` 生成桩页面，无头 Chrome 截图核对深浅两主题。

## 5. 契约熔断与 Git 洁净规范

- **契约熔断**：当 `src/core/` 与 `scraper/` 协议不一致、或 `commands_*.rs` 与前端 `types.rs` 不一致、或 SPEC 与实际行为冲突时，**立即停止修改业务代码**，执行 `git status` 汇报未提交改动范围，提示用户决定保留或回滚，再切回架构模式修订文档。
- **禁止污染版本库**：严禁提交密钥、编译产物（`app/src-tauri/target/`、`frontend/target/`、`frontend/dist/`、`dist/`）、临时截图与日志。`data/` 含登录 cookie 与 Chrome profile，`.gitignore` 已忽略，**不得**用 `git add -f` 绕过。
- **提交信息规范（Conventional Commits）**：`<type>(<scope>): <subject>`
  - `<type>`：`feat` / `fix` / `docs` / `style` / `test` / `refactor` / `chore`
  - `<scope>` 对齐第 2 节目录：`core` / `desktop`（`app/`）/ `client`（`frontend/`）/ `sidecar`（`scraper/`）/ `contracts`（协议与命令签名）/ `release` / `arch`（`AGENTS.md` / `docs/`）
  - **提交门禁**：未通过槽位 A（及已落地的槽位 B）前禁止 `git commit`；每次只提交一个完整 Milestone，严禁混杂无关文件。
  - **用户本地改动保护**：提交前逐项检查 `git status`。非本人所做的改动（如用户手工调过的 `profile.rs`）**不得**擅自提交或还原，须先询问。
- **多 lane 并发时禁止裸 `git commit`**：索引里可能有别的 lane 已经 `git add` 好的文件，裸提交会把它们一起卷进你的提交（本项目真实发生过一次：本想提交 4 个文档，实际带走了 12 个文件）。并发时必须写 `git commit --only -- <精确路径>`（或 `git commit -F <消息文件> --only -- <精确路径>`），提交前用 `git status --porcelain` 核对，提交后用 `git show --stat HEAD` 复核只包含自己的文件。消息文件放仓库内（如 `.git/COMMIT_MSG.txt`），放 `$env:TEMP` 会被 git 拒绝（`is outside repository`）。
- **PowerShell 批量改源码铁律**：一律用 Python 的 `str.replace`（字面量），**禁止** PowerShell `-replace`（正则引擎会吃掉替换串里的字符，曾把四份源码所有 `i` 变成 `m`）。
- **文档同步**：修 Bug 或完成可观察行为改动后，在 `DEVELOPMENT.md` 顶部按「日期 + 现象 / 根因 / 改动文件 / 验证」追加一条。

## 6. 交接清单（未决项必须在此列明）

### 6.1 会被违反的既有结论（只留约束，不留取证过程）

| 议题 | 结论 |
|---|---|
| 启动报 `os error 5` / `0x800700AA` | 根因**不是代码**：工作区目录带 Low 完整性标签，里面的 exe 会被降为 `S-1-16-4096`，写不了 `%LOCALAPPDATA%`。**产物必须放工作区外**（`D:\jt\release\`）。取舍细节见 `app/src-tauri/src/lib.rs::not_writable_hint`。 |
| 单实例锁 | **不是锁文件**：进程启动时创建 Windows 命名互斥体 `Local\com.ai.jobassistant.rust.single`（`app/src-tauri/src/lib.rs:340-385`，`CreateMutexW`，`ERROR_ALREADY_EXISTS` 即报「另一个实例正在运行」，句柄故意泄漏到进程退出）。**旧方案 `app.lock` 已作废**——实测 `share_mode(0)` 排他打开锁文件返回 `os error 5`（`lib.rs:335-339` 注释）。 |
| 门禁会重放陈旧二进制 | 共享的 `CARGO_TARGET_DIR`（如 `D:\jt`）在其它 lane 或旧提交留下产物时，Cargo 可能直接复用而不重编——实测重放过 07:19:38 的旧 exe，测试照旧 abort，换成干净目录 `D:\jt-check` 后得 67 passed。**排查「改了代码但测试结果不变」先换独占 target 目录**。 |
| 真实库迁移 | `migrate()` 必须整体事务（崩在中途会 ×100）；动真实库前先 `Copy-Item` 到 `data/backup/` 并核对 SHA256。 |
| 卡片先查敏感库 | 三道预筛在详情页前完成，其中第 3 道是**空 JD 跑完整 `reject()`**，不要另写一套卡片规则；`SensitiveEntry` 的 `stored` / `rejected` 两种 kind 都要写。 |
| 写库失败不计入 `inserted` | 判定只在 `app/src-tauri/src/core/sidecar/ingest.rs::handle_job`；sidecar 不打分不落库。 |

### 6.2 未决项

1. **【阻塞】从 Tauri 子进程里拉起真实 Chrome 失败**。根因已定位：应用运行时 `msedgewebview2.exe` 会在本机创建 Chromium 单例命名对象，sidecar 启动的 Chrome 探测到后报「正在现有的浏览器会话中打开」并 `exitCode=0`，playwright 收到 CDP 管道关闭 → `TargetClosedError: BrowserType.launch_persistent_context: Target page, context or browser has been closed`。已排除：Job Object、环境变量（含 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`）、stdio/CREATE_NO_WINDOW、`--user-data-dir` 路径、profile 脏数据、Chrome 版本。**待定修法**：给 sidecar 的 Chrome 用与应用 WebView2 不再撞名的 user-data-dir，或分离 `--user-data-dir` 与 `--apps-user-data-dir`。
2. **Liquid Glass 第二三层未做**（唯一剩下的前端设计项）：第二层 SVG `feTurbulence` + `feDisplacementMap` 湍流折射；第三层 WebGL 卡片间真折射（方案 C：亚克力继续透桌面，WebGL 只渲染抽屉盖列表、气泡盖背景）。第一层 CSS 玻璃已完成。**注**：`frontend/` 里除 `skills/liquid-glass/REFERENCE.md` 与 `skills/liquid-glass/scripts/generate_glass_map.js`（参考文档与工具）外，没有任何运行时代码引用 `feTurbulence` / `feDisplacementMap` / WebGL。
3. **`pages/` 零单测**：`frontend/` 现有 61 个用例全部落在 `lib_ui/` 与 `features/`，`pages/{dashboard,chat,config,logs}.rs` 仍无覆盖。
4. **`commands_auto::auto_greet_gap` 定义了但没注册**（`commands_auto.rs`，`#[tauri::command]` 共 6 个，`invoke_handler` 只列了 5 个 `auto_greet_*`）。前端若 `invoke("auto_greet_gap")` 会得到「命令不存在」。要么注册，要么删掉。
5. **`IngestPort::record` 有默认空实现**（`app/src-tauri/src/core/sidecar/session.rs:157-162`）。注释自认「生产实现必须覆盖，否则决策日志恒空」——这是端口默认实现掩盖漏接的典型：测试桩不实现也编译通过。建议改成必需方法。
6. **`tests/sidecar_smoke.rs` 的跑法注释残留旧路径**（`:6-11` 写 `cd core`，但 `core/` 已并入 `app/src-tauri`），照抄会报错。另 `:63-77` 在 `SidecarError::Spawn` 时只 `eprintln` 就 `return`，进程起不来时测试仍然绿——机器没装 python 和真正坏了长得一样。
7. **`schema.rs` 无任何外键约束**（全仓搜不到 `FOREIGN KEY` / `REFERENCES` / `PRAGMA foreign_keys`）。父子表（如 `job_queue` ↔ `jd_decisions`）靠应用层维护，`rusqlite 0.32` 的 `bundled` SQLite 默认 `foreign_keys=OFF`，真去加约束前要先补 `conn.pragma_update(None, "foreign_keys", "ON")`。
8. **`docs/` 需重整**：`SPEC-001` / `ADR-001` / `ADR-002` / `ARCH-002` 描述的是已被取代的实现，`SPEC-002` 第 4 节蓝图与代码不符（写的 `storage/ ports/` 实际是 `infra/`，端口 token 放在 `domain/`）。**新文档落地前，本文件的第 2 节就是唯一有效的目录边界。**
9. **`tools/gui_e2e.py` 与 Leptos 版未对齐**：`REQUIRED_TESTIDS`（`:69` / `:89` / `:346` / `:366` / `:582-584`）里仍含旧结构锚点，`run --ui-only` 是否全过需在 UI 定型后重跑一次核对。
10. **旧版只读目录 `backend-exe/` 已丢失**：误删时它未被 git 跟踪，无法用 `git checkout` 恢复。要恢复只有重跑旧版 `build.bat`（`backend/` 已恢复，产物可重建）。
11. **`app/src-tauri/Cargo.toml:13` 的 `tauri-build = { version = "2", features = [] }` 是冗余写法**：`features = []` 可删（不影响行为，纯清理）。
12. **`app/src-tauri/src/core/scrape.rs` 已 583 行**，是当前 `core/` 里最大的单文件（含会话启停、sink/lookup 构造、重启判定、进度收尾）。第 2.2 节要求编排逻辑不得留在 `commands_scrape.rs`，它确实已下沉，但下沉目标本身现在偏大，下一轮可考虑按 `launch` / `restart` / `sink` 拆目录。
