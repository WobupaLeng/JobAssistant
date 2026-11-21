---
name: rust-testing
description: Test and verification gates for the jobassistant-app crate - the exact PowerShell environment cargo test needs on Windows, which failures are environment artifacts, host-vs-wasm target separation, ignored sidecar smoke runs, and panic=abort debugging. Use when running cargo test, interpreting os error 5 / 0xC0000409 / D8050 failures, deciding whether a test should be #[ignore], or designing a new unit test for core/ or the command layer. Not for frontend trunk/wasm gates beyond knowing they are separate (use the leptos skill), and not for the domain rules themselves (use rust-domain-purity).
---

# rust-testing — 门禁与环境（`app/src-tauri`）

导语：本项目的测试门禁**不是 `cargo test` 一条命令**。缺一个环境变量，
`core::infra::sqlite` 的 3 个用例就会红，而代码是对的。
所以读这份文件的第一目的不是「学会写测试」，而是**别把环境问题当成代码 bug 去改测试**。

> 当前基线（2026-11-21 在 `bb9d16c` 上按 `#[test]` 静态计数；`core/` 305 + 命令层与壳 71 =
> **376 个可执行用例**，另加 `tests/sidecar_smoke.rs:35-37` 的 1 个 `#[ignore]`）。
> 它们跑在 host 目标上，0 窗口、0 网络、0 数据库文件，秒级完成。
> 数字随提交变（`AGENTS.md` 第 0 节每次校准都会写清取证基准），**以你数到的为准并同步本文档**。

---

## Routing Trigger

- 要跑 `cargo test` / `cargo test --lib` / `cargo test --test <name>`
- 看到 `os error 5`、`PermissionDenied`、`0xC0000409`、`STATUS_STACK_BUFFER_OVERRUN`、`D8050`
- 想给一个测试加 `#[ignore]`
- 要写新用例，纠结该放 `#[cfg(test)] mod tests` 还是 `tests/`
- 疑惑「前端门禁为什么不是 cargo test」

不读本 skill：领域规则本身怎么设计（`rust-domain-purity`）、命令参数怎么加（`rust-tauri`）。

---

## The Reference Map

| 要读什么 | 文件:行 | 读它为了 |
|---|---|---|
| 门禁的权威命令块 | 仓库根 `AGENTS.md` 第 4 节「槽位 A」 | 四条命令 + 每个环境变量的理由 |
| 临时目录用例为何需要 `TEMP` | `app/src-tauri/src/core/infra/sqlite/tests.rs:508-516` | `std::env::temp_dir()` + `create_dir_all` (513) |
| 内存库用例范式 | `core/infra/sqlite.rs:141-149` | `open_in_memory()` 不开文件就能测 |
| 命令层双写用例范本 | `src/commands_db.rs:864-875`、`:900-909` | 参数兼容 / 缺省语义怎么测 |
| 入库计数用例 | `core/sidecar/session.rs:690`、`:724` | 「写库失败不能算入库」「重复单列」怎么断言 |
| 分页稳定性用例 | `core/infra/sqlite/tests.rs:141`、`:168` | 游标语义怎么锁 |
| sidecar 冒烟（默认忽略） | `tests/sidecar_smoke.rs:1-15`、`:35-36` | 为什么 ignore、断言到什么颗粒度 |
| 启动诊断报文用例 | `src/lib.rs:527-558`（`startup_tests`） | 纯函数式断言的写法（不启窗口） |

---

## Non-Negotiables

1. **Windows 下 `cargo test` 必须带三个环境变量。**
   ```powershell
   $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"   # rustc/cargo 可被找到
   $env:CARGO_TARGET_DIR = "D:\jt"                        # 短路径：避免 MSVC CL 命令行超限 → D8050
   $env:TEMP = "$env:TEMP"; $env:TMP = $env:TEMP          # 不设则临时库用例 os error 5
   ```
   实测（本机，`6f9a76e` 那天的 367 用例基数上）：**只设 `PATH` → 364 passed / 3 failed**；
   加上 `CARGO_TARGET_DIR` 与 `TEMP`/`TMP` → **367 passed / 0 failed**（同基数下全部转绿）。
   红的三条是 `commands_db::tests::open_store_creates_parent_dir`、
   `core::infra::sqlite::tests::single_db_file_holds_all_rows`、
   `core::infra::sqlite::tests::upgrade_columns_fills_legacy_table`，
   报错落点 `src\core\infra\sqlite\tests.rs:513:39`：
   `called Result::unwrap() on an Err value: Os { code: 5, kind: PermissionDenied, message: "拒绝访问。" }`。
   → **这是环境问题，不是代码问题。不要改测试代码去绕**（比如把临时目录换成写死的路径）。
   补充：`$env:TEMP = "$env:TEMP"` 本身是空操作，它的作用是**保证变量存在**——
   `pwsh` 里一般已有值，但从某些服务 / 计划任务继承来的环境可能是空的。
  另外 `CARGO_TARGET_DIR` 指向工作区外还有第二个理由：工作区内的产物会继承 Low 完整性标签，
   exe 启动必报 `os error 5`（`src/lib.rs:284-304`）。
   **第三件事：每次门禁都要用一个干净的 `CARGO_TARGET_DIR`**（如 `D:\jt-check`）。
   共享目录被其它 lane 或旧提交的产物占着时，Cargo 可能直接重放陈旧二进制——
   实测重放过一份旧 exe，测试结果与当前源码无关（根 `AGENTS.md` 第 4 节已记）。

2. **前端门禁与后端门禁是两套，别混着跑。**
   - 后端（host 目标）：`cd app/src-tauri; cargo test`
   - 前端（wasm 目标）：`cd frontend; cargo check --target wasm32-unknown-unknown`，再
     `Remove-Item Env:\CARGO_TARGET_DIR` 之后 `trunk build`
   `trunk build` **必须清掉 `CARGO_TARGET_DIR`**：trunk 只在项目内找产物，
   指向工作区外会报 `cargo artifacts not found for target crate`，且它不认 `--target-dir`。
   `trunk` 要的是 **bin** 的 wasm，所以 `frontend/Cargo.toml` 的 `[[bin]]` 不能删。

3. **host 目标下不要碰 `js_sys` / `web_sys`，也不要依赖「当前时间」「随机数」。**
   那类调用在 host 测试里会让进程直接 abort（`0xc0000409`），panic 不可展开、栈也读不到。
   本项目的做法是把不确定性做成**入参**：
   - 抖动：`core/domain/auto_greet.rs:31-41` 的 `next_gap_ms(roll: f64) -> u64`（模块注释 `:1-13` 明说不在此起定时线程）
   - 时间：`core/domain/logbuf.rs:99-105` 的 `now_ms()` 与 `push_at(ts, ..)` 分离
   → 写新逻辑时先问：这段能不能变成「吃参数吐结论」的纯函数？能，就别让它自己去取时间。
   真实案例：`frontend/src/lib_ui/logfmt.rs` 曾在 host 测试里调 wasm 侧时间 API，
   测试进程直接 `0xc0000409` 无法展开（提交 `7b6aa90`「logfmt 时间格式化纯函数化」修的）。

4. **`#[ignore]` 只是把问题藏起来，不是修好。**
   本仓库**只允许一种 `#[ignore]`**：需要真实外部进程 / 网络 / 登录态的冒烟
   （`tests/sidecar_smoke.rs:35-36`，理由写在 `:1-15`）。
   单元测试加 `#[ignore]` 等于把红变绿但没解决问题——尤其别用它掩盖第 1 条的 `os error 5`。

5. **sidecar 冒烟只断言「起得来 / 握手 / 关得掉」。**
   ```powershell
   cd app/src-tauri
   cargo test --test sidecar_smoke -- --ignored --nocapture
   ```
   断言口径（`tests/sidecar_smoke.rs:13-15`）：进程能起来、能握手、能被关停；
   **不断言抓到多少条、分数多少**——那属于 sidecar 与领域规则，各有各的测试；
   断在网络上只会得到一个薛定谔的绿灯。
   实现细节也值得照抄：
   - `SidecarError::Spawn` 现在**一律判红**（`tests/sidecar_smoke.rs:66-79`；`:74-77` 注释：
     「不允许因为起不来就静默跳过……让门禁把问题暴露出来，而不是给一条薛定谔的绿灯」）；
   - 端口三参如实写出（`:57-63`；`:60-61` 注释「`record` 没有默认实现，这里必须显式写出来」）；
   - 退出必须给出原因（`:105`），未发 `scrape.start` 时断言零入库（`:104`）。
   跑法与 `:6-11` 的文档注释一致，在 **`app/src-tauri`** 下执行。

6. **`panic = "abort"` 会让启动 panic 变成 `0xC0000409`。**
   `Cargo.toml` 的 `[profile.release]` 里 `panic = "abort"`（为体积与启动速度），
   副作用是 Tauri 启动期 panic 从 `101` 变成 `0xC0000409 STATUS_STACK_BUFFER_OVERRUN`，
   栈不可展开、日志一片空白。**排障时先临时把它改成 `"unwind"`**、定位完再改回来
   （这条注释就写在 `Cargo.toml` 的 release 段里）。
   同理，`release` 档 `strip = true` / `opt-level = "s"` / `lto = true` 会让 release 断点几乎不可用——
   排障一律用 debug 档。

7. **新用例先选对落点。**
   | 要测什么 | 放哪 | 现状 |
   |---|---|---|
   | 领域规则、解析、端口替身 | `core/**/mod` 内的 `#[cfg(test)] mod tests` | 占绝大多数 |
   | 命令层入参形状 / 缺省语义 | `commands_*.rs` 尾部 `mod tests` | 当前 `commands_db.rs` 23 个最多，其次 `commands_logs.rs` 8 个 |
   | 真实进程 / 网络 | `tests/*.rs`（集成测试） | 只有 `sidecar_smoke.rs` |
   单元测试优先用 `Connection::open_in_memory()`（`core/infra/sqlite.rs:141-149`），
   它跑同一套 `SCHEMA` + `upgrade_columns`，比临时文件快且不污染磁盘。

8. **提交前必过（否则禁止 `git commit`，见根 `AGENTS.md` 第 5 节）**：
   `cargo test`（`--lib` 过滤掉集成测试，是最快的一道）→ 前端 `cargo check --target wasm32` + `trunk build`
   → `python -m compileall -q scraper`。任何一项红都不得提交。

---

## Workflow

### 红线排查顺序（测试红了先按这个顺序怀疑）

1. 环境：`PATH` / `CARGO_TARGET_DIR` / `TEMP` / `TMP` 三个是否都在？（铁律 1）
2. 是不是本机残留进程 / 文件锁（临时目录里上一个 pid 的残留）？
3. 是否是并发跑的副作用（同一临时目录路径被两个测试共用）？
4. 最后才怀疑代码。

### 写一条新用例

1. 先把它写成纯函数调用：入参 → 结论。做不到就说明设计没过关（回 `rust-domain-purity` 铁律 3/4）。
2. 断言要**锁语义**而不是锁实现：例如「写库失败不能算入库」（`session.rs:724`）、
   「游标按 job_id，所以翻页稳定」（`tests.rs:168`）。
3. 每条铁律至少一条反向用例（错的做法必须能被抓住）。

### 跑一次完整门禁

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:CARGO_TARGET_DIR = "D:\jt"
$env:TEMP = "$env:TEMP"; $env:TMP = $env:TEMP

cd app/src-tauri
cargo test                     # 376 passed / 0 failed + sidecar_smoke 1 ignored（按 bb9d16c 计）

cd ..\..\frontend
cargo check --target wasm32-unknown-unknown
Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
trunk build

cd ..
python -m compileall -q scraper
```

---

## 本模块实证速查

| 症状 | 真因 | 处置 |
|---|---|---|
| 3 条 sqlite 用例 `os error 5`（`tests.rs:513`） | 未设 `TEMP` / `TMP` | 设环境变量，**不改测试** |
| 报 `D8050` / CL 命令行超限 | 产物路径太长 | `CARGO_TARGET_DIR=D:\jt` |
| `trunk build` 报 artifacts not found | `CARGO_TARGET_DIR` 还指向工作区外 | `Remove-Item Env:\CARGO_TARGET_DIR` |
| 测试进程 `0xc0000409` 无栈 | host 下调了 `js_sys`/`web_sys`，或 `panic=abort` | 改成入参纯函数；排障改 `"unwind"` |
| 启动即 `os error 5` / `0x800700AA` | exe 在工作区内，继承 Low 完整性标签 | 产物放 `D:\jt\release\` |
| `sidecar_smoke` 找不到 | 跑错目录（旧路径 `cd core`） | 在 `app/src-tauri` 下跑 |
| 为了过门禁加了 `#[ignore]` | 掩盖问题 | 只允许用于真实外部进程 / 网络 |
