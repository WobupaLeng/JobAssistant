# `backend-tauri/` 模块约定 · Tauri 2 壳 + 内嵌 Rust 核心

> 本文件是本模块**目录结构与分层约定**的唯一真源。跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。
> **只写约束，不写进度**：现状、坑、下一步写 [`HANDOFF.md`](HANDOFF.md)。

---

## 1. 目录结构

**真正的 crate 是 `app/src-tauri`**（`app/` 自身没有 `Cargo.toml`，也没有 `package.json`）。`app/node_modules/` 只放 Tauri CLI，构建走 `npx tauri`，勿删。

```
app/src-tauri/
  Cargo.toml  Cargo.lock  build.rs
  tauri.conf.json           窗口 / CSP / frontendDist / devUrl（高冲突单点）
  capabilities/default.json 权限白名单：仅 core:default + 4 个窗口权限，无 shell / fs
  icons/  gen/schemas/（gitignore）
  tests/sidecar_smoke.rs    sidecar 起停冒烟，常年 #[ignore]
  src/                      见下
```

`scripts/pack_sidecar.py` 打包态辅助脚本：PyInstaller onedir 产出 `backend-tauri/sidecar-dist/`（约 130 MB，gitignore），`tauri.conf` 的 `bundle.resources` 用 glob `sidecar-dist/**/*` 打进安装包。**resources 写成字面量目录会让 `cargo test` 的 build script 直接报 `resource path ... doesn't exist`**，必须留 glob 形式。

`src/` 五个部分：

| 部分 | 职责 | 一句话 |
|---|---|---|
| **壳层 + 命令层**（`src/` 根 9 文件） | `lib.rs` + `main.rs` + `commands_*.rs` | 收参数 → 调 core → 错误转字符串 |
| **`core/`** | 业务核心，**不得 `use tauri::`** | 全部领域规则都在这里 |
| `core/domain/` | 纯领域规则与端口 trait | 不碰 IO |
| `core/infra/` | `rusqlite` / `ureq` 的具体实现 | 实现 domain 的端口 |
| `core/sidecar/` + `core/scrape/` | JSON Lines 协议、子进程、会话、入库判定、起停编排 | 与 `scraper/` 的唯一接口 |

### 1.1 壳层与命令层（`src/`）

| 路径 | 职责 |
|---|---|
| `main.rs` | 入口，只调 `jobassistant_app_lib::run()`；`windows_subsystem = "windows"` 隐藏控制台 |
| `lib.rs` | 启动编排、单实例互斥体、WebView 目录探针、`manage()` 5 个状态、注册 46 条命令 + **内联 2 条** |
| `commands_db.rs` | 数据库与打分 14 条 + `StoreState` / `LlmState` / `store_handle` / `build_llm_client` |
| `commands_scrape.rs` | 抓取与登录 10 条 + `ScrapeState` / `LAST_SPEC` / `RESTART_LOCK` / `maybe_restart` |
| `commands_config.rs` | 配置与阈值 4 条 + `AppPaths` / `read_config` / `apply_patch` |
| `commands_auto.rs` | 自动打招呼 5 条 + `AutoGreetState` / xorshift64* 随机源 / `pick_next_pending` |
| `commands_chat.rs` | Boss 聊天 7 条 + `handle_of` / `request_chat` |
| `commands_greet.rs` | 打招呼 2 条：`greet_draft` 只生成、`greet_one` 才发送（**刻意拆两步**） |
| `commands_sensitive.rs` | 敏感库 2 条 |
| `commands_logs.rs` | 日志 2 条 + 进程级 `static LOGS: Mutex<LogRing>` |

### 1.2 `core/`（不得 `use tauri::`）

**根与 `scrape/`**

| 路径 | 职责 |
|---|---|
| `core/mod.rs` | 模块注册 + 再导出 `SCORE_MAX` / `SCORE_MIN_DEFAULT` |
| `core/scrape.rs` | 目录外壳：只 `mod` + `pub use`，对外路径 `core::scrape::*` 不变 |
| `core/scrape/launch.rs` | `resolve_script_dir`、`ScrapeJobSpec`、`launch_session` |
| `core/scrape/restart.rs` | 重启判定 `plan_restart` / `restart_decision` / `restart_session`（纯判定，可单测） |
| `core/scrape/sink.rs` | 把 `SqliteStore` 包成 `IngestPort` / `make_lookup` |
| `core/scrape/tests.rs` | `core::scrape` 单测 |

**`domain/`**（纯规则 + 端口，外部依赖只允许 `regex` / `serde` / `thiserror`）

| 路径 | 职责 |
|---|---|
| `domain/job.rs` | `Job` 实体、`JobQuery` / `SortField` / `JobStatus`、端口 `JobStore` / `SensitiveStore` |
| `domain/store_ports.rs` | `JobAdminStore` / `SensitiveAdminStore` / `DecisionStore` / `ConfigStore` |
| `domain/scoring.rs` | 百分制五维规则打分 `rule_score_all`，**口径唯一真源** |
| `domain/scoring_llm.rs` | `SYSTEM_PROMPT`、三级容错解析、`LlmClient` 端口、规则初筛 |
| `domain/scoring_service.rs` | 统一入口 `score_job`：规则 / 大模型双通道，**决策顺序不可重排** |
| `domain/rescore.rs` | 全量重算 `rescore_batch`（游标分页） |
| `domain/rules.rs` | 硬性排除规则，**判断顺序不可重排** |
| `domain/parse.rs` | 薪资 PUA 解码 / 区间解析、经验要求、HR 活跃度 |
| `domain/profile.rs` | `DEFAULT_PROFILE` |
| `domain/greeting.rs` | `build_prompt` / `matched_skills` / `rule_greeting` / `write_greeting` |
| `domain/app_config.rs` | 键白名单、默认值、**密钥掩码** `mask_api_key`、上下限 `resolve_limit` |
| `domain/auto_greet.rs` | 投放节奏 `next_gap_ms`（20s±5s，下限 15s）、`AUTO_MAX_FAILURES` |
| `domain/logbuf.rs` | `LogRing`（容量、时间戳单位、增量语义） |
| `domain/schema.rs` | `SCHEMA` / 索引 / 补列声明（**高冲突单点**） |
| `domain/migration.rs` | 十分制 → 百分制幂等迁移（`score_scale` 标记） |

**`infra/`**（唯一允许碰 `rusqlite` / `ureq` 的地方）

| 路径 | 职责 |
|---|---|
| `infra/llm.rs` | `HttpLlmClient` 实现 `LlmClient`，OpenAI 兼容 `/chat/completions` |
| `infra/sqlite.rs` | `SqliteStore`（`Mutex<Connection>`）、`StoreError`、`open` / `open_in_memory` / `add_missing_columns` |
| `infra/sqlite/jobs.rs` | `JobStore` 实现 + `push_job_filters`（列表与总数共用同一筛选） |
| `infra/sqlite/admin.rs` | 四个 Admin / Decision / Config 端口实现，命名占位符 `?1..?N` |
| `infra/sqlite/sensitive.rs` | 敏感库聚合根 |
| `infra/sqlite/row.rs` | 行映射与列白名单：`SELECT_COLUMNS` 与 `row_to_job` 索引必须一一对应 |
| `infra/sqlite/tests.rs` | 持久层单测主锚点 |

**`sidecar/`**

| 路径 | 职责 |
|---|---|
| `sidecar/protocol.rs` | JSON Lines 协议**唯一真源**，单行上限 1 MB（**高冲突单点**） |
| `sidecar/process.rs` | 目录外壳：只 `mod` + `pub use` |
| `sidecar/process/config.rs` | `SidecarConfig` / `locate_script_dir` / `PYTHON_ENV` / `DEFAULT_READY_TIMEOUT` |
| `sidecar/process/handle.rs` | `SidecarHandle`：spawn、写 stdin、等 ready、优雅关停 |
| `sidecar/process/codec.rs` | `protocol_line` / `parse_line` / `route_reply` / stderr 尾 |
| `sidecar/process/tests.rs` | 协议线形状、行解析、回执路由 |
| `sidecar/session.rs` | `SidecarSession`：消费线程、`ScrapeProgress`、`request()` |
| `sidecar/ingest.rs` | `ScrapedJob → Job → 八条规则 → 打分 → 门槛` 纯函数 |

---

## 2. 分层铁律

**单向依赖，只能向下**：`commands_*` → `core::{scrape, sidecar, domain}` → `core::infra`。

1. **`core/` 不得 `use tauri::`**，`tauri` 只允许出现在注释里。⚠️ **这是 code review 约定，不是编译期门禁**——`core/` 已并入 `app` crate，`cargo build` 抓不到。
2. **`domain/` 不得依赖 `rusqlite` / `ureq` / `tauri`**。同样靠 review。
3. **存储走 trait 端口**：`domain::job::JobStore` 定签名，`infra::sqlite::SqliteStore` 实现。**不许**在 `domain` 里写 SQL。
4. **命令层不得写领域规则**（打分 / 排除 / 入库判定 / 投放节奏）。⚠️ 「命令层零逻辑」**不成立**，有两处成文例外，改之前先读对应测试：
   - `commands_scrape.rs::maybe_restart`：自持 `RESTART_LOCK`，调 `session.shutdown()` + `launch_session` + `restart_session`；
   - `commands_db.rs::into_options`：`limit.unwrap_or(50).clamp(1, 500)`。

---

## 3. 命令层约定

1. **48 条**：`commands_*.rs` 里 46 条 `pub fn` + `lib.rs` 里 2 条私有 `fn`；`generate_handler!` 注册同样 48 条，**零孤儿**。新增命令必须同时登记，否则前端 `invoke` 得到「命令不存在」，同时要在 `frontend-vue/` 的 api 层加封装与类型。
2. **内联两条的理由**：`get_scoring_spec()` 返回壳层私有 struct `ScoringSpec`，口径常量只在 `lib.rs` 引入，放最靠近常量的位置免得命令层再抄一份；`app_info(paths)` 依赖 `env!("CARGO_PKG_VERSION")` 与 `setup` 填的 `AppPaths`，属 Shell 自述而非业务。
3. **参数必须双向兼容**，但**结构体参数与平铺参数的键名规则相反**：
   - **结构体参数**（`{ args }` 包装，如 `ScrapeArgs` / `ScoreArgs` / `RescoreArgs`）：前端发驼峰、Rust 字段蛇形，serde 对未知字段**不报错**，照这三个加 `alias`，否则值被静默丢弃。
   - **平铺参数**（参数列表里一个个写的）：Tauri 宏按 **camelCase** 取键（`rename_all` 默认 `"camelCase"`，见 tauri-macros `src/lib.rs`）。Rust `job_id` ↔ 前端 `jobId`（`get_job` / `update_score` 即此）。平铺参数上写 `#[serde(alias)]` **无效**（`error: cannot find attribute 'serde' in this scope`）；确实要收蛇形只能给**该条命令**加 `#[tauri::command(rename_all = "snake_case")]`，前端随之必须发蛇形。
   - ⚠️ **键名写错不会都被单测拦住**：必填平铺参数发错键 = 硬失败（界面贴 `invalid args '<camelKey>' … missing required key '<camelKey>'`）；`Option<T>` 平铺参数发错键 = **静默变 `None`**（`ipc/command.rs` 的 `deserialize_option` 缺键走 `visit_none`）。vitest 只断言前端发了什么，**验证不到 Tauri 的取键逻辑**。
4. **配置写入分两条命令**：门槛与上限走 `set_limit`，其余键走 `set_app_config`。`min_score` **不在** `WRITABLE_KEYS` 里，塞进 `set_app_config` 会被静默跳过。
5. **等远端（或等 sidecar 回执）的命令一律 `async`**。同步 `#[tauri::command]` 由 Tauri **在主线程上执行**（`Webview::on_message` → `Window::manager` 都是 `run_on_main_thread`），主线程上的 `recv_timeout` 会把 WebView2 的消息泵一起按停——窗口拖不动、按钮点不动，表现就是「程序无响应」，最长可达 `CHAT_TIMEOUT` 的 90 秒。判据只有一条：**函数体里有没有阻塞等待**。纯本地读库（`*_local`）与纯内存计算（`scrape_progress` 的锁内快照）保持同步即可。
   - ⚠️ 改成 `async` 后函数体仍在工作线程上同步执行，**同一时刻只跑一个任务**。会阻塞等回执的命令不要写成 `async fn` 里再去 `blocking_lock()` 之类的全局锁，排队会比原来更糟。

---

## 4. 壳层职责（`lib.rs`）

启动顺序不可调换：抢单实例锁 → 清理残留 WebView → 探针 WebView 数据目录可写 → 建窗口 → `setup` 填 `AppPaths` → `manage()` 五个状态 → 注册命令。

- **单实例锁**：命名互斥体 `Local\com.ai.jobassistant.rust.single`（`CreateMutexW`）。`ERROR_ALREADY_EXISTS` → 「另一个实例已在运行」；`ERROR_ACCESS_DENIED` → 提示 `taskkill`。**句柄故意泄漏到进程退出**，不要加 `CloseHandle`。
  ⚠️ 写互斥体代码时 `c_void` 必须写 `::core::ffi::c_void` 绝对路径——crate 顶层有 `core` 模块，会遮蔽 std 的 `core`。
- **毛玻璃**：走 `windows-sys` 动态加载 `SetWindowCompositionAttribute`（`window-vibrancy` **已不在依赖里**），需窗口 `transparent: true` + 页面背景透明的配合。
- **WebView 目录探针失败即 `exit(2)`**，附 `not_writable_hint`。这是工作区 exe 报 `os error 5` 时的唯一可读线索。
- **CSP** 见根 `AGENTS.md` §3.1，`script-src` 缺失即白屏。

---

## 5. 测试约定

- **主门禁**：`cd app/src-tauri && cargo test`。静态 `#[test]` 计数应与实跑 `passed` 吻合，不吻合先查统计口径。
  - 只查类型、不跑用例用 `cargo check --tests`：`cargo check` 不展开 `#[cfg(test)]` 模块，测试代码里的编译错误它一条都看不见。
  - ⚠️ **新建独占 `CARGO_TARGET_DIR` 后，`cargo check` 与 `cargo test` 必须用同一个目录**。两个目录并行跑会抢同一套 build-dir/proc-macro 编译，冒出来的 `error: cannot determine resolution for the macro ::core::concat` / `could not compile windows-sys (lib)` 是竞态，不是代码问题（实测踩过）。遇到这类报错先看是不是两个 cargo 进程同时活着，再换一个**全新的单线程**目录重跑。
- **锚点**（改这些必须看对应测试）：
  - `domain/scoring.rs` 五维权重与初筛线；`domain/rules.rs` 八条排除规则的**顺序**；`domain/parse.rs` 薪资 PUA 解码；`domain/migration.rs` 幂等迁移；
  - `domain/rescore.rs` **排序键必须是 `job_id` 升序**——按 `match_score` 排序会被自己的写回推翻分页边界，同一岗位算两次、别的被跳过，**且没有任何报错**；
  - `domain/schema.rs` 列数锚点，与 `infra/sqlite/row.rs` 的 `SELECT_COLUMNS` 一一对应；
  - `sidecar/protocol.rs` `ScrapedJob` 字段数与各帧形状；`infra/sqlite/tests.rs` 读写与 NULL 排序。
- **无 JD 否决只能走 `RuleScore::passes()`**：无 JD 岗位降权后最高仍有 72 分，`score < min_score` 在门槛 ≤70 时会放行。
- **薪资排序的 NULL 语义**：SQLite 的 `ORDER BY x ASC` 把 NULL 排**最前**（与 MySQL / PG 相反）。面议 / 空必须恒为第一排序键且恒 ASC。
- **临时库测试必须设 `$env:TEMP`**，否则 `create_dir_all` 报 `os error 5`。**不要改测试代码去绕**。
- `tests/sidecar_smoke.rs` 需真实 Chrome + 登录态，常年 `#[ignore]`：`-- --ignored --nocapture`。只断言起得来 / 握手 / 关得掉。

---

## 6. 模块内技能（`app/src-tauri/.agents/skills/`，均为单文件 `SKILL.md`）

| 技能 | 用途 |
|---|---|
| `contract-sync` | 三处契约真源同步纪律 |
| `rust-domain-purity` | `core/` 分层铁律与端口方向 |
| `rust-tauri` | Tauri 2 壳层 / IPC / state / capabilities |
| `rusqlite` | 持久层与 NULL 排序陷阱 |
| `rust-testing` | `cargo test` 环境门禁 |
| `sidecar-protocol` | JSON Lines 行协议 |

---

## 7. 数据目录

- 真实库在 `data/`（含 `-wal` / `-shm`）。`data/` 被 `.gitignore` 忽略，含登录 cookie 与 Chrome profile，**不得** `git add -f`。
- 库名以 `lib.rs` 的 `AppPaths` 为准，改名前先核对。
- **严禁**对真实库做破坏性验证：迁移 / 写入验证必须在副本或临时目录进行，验完删除。
