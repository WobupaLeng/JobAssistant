# 交接说明（HANDOFF）

> 写给下一位接手的 AI。读完这一页就能开工，不必再翻历史。
> 权威文档：`AGENTS.md`（工程宪法，第 2 节是唯一有效的目录边界）。本文件只讲**现状、坑、下一步**。

最后更新：2026-11-21 06:20

---

## 1. 一句话现状

界面从 TypeScript 换成了 **Rust + Leptos 0.8（WASM）**，`core/` 已并入 `app/src-tauri/src/core/`。
**概览页能跑、能打包、297 个单测全绿**；剩下三页（Boss 消息 / 配置 / 决策记录）和抽屉、骨架条、动效、
Liquid Glass 第二三层没做。**有一个阻塞性缺陷**：应用运行时 sidecar 拉不起 Chrome。

---

## 2. 能跑吗？怎么跑

### 门禁（全部已验证通过）

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:CARGO_TARGET_DIR = "D:\jt"
$env:TEMP = "C:\Users\Administrator\AppData\Local\Temp"; $env:TMP = $env:TEMP

cd app/src-tauri; cargo test                  # → 297 passed
cd frontend; cargo check --target wasm32-unknown-unknown
cd frontend; Remove-Item Env:\CARGO_TARGET_DIR; trunk build --release    # → wasm 2.9 MB
python -m compileall -q scraper
cd app; npx tauri build --no-bundle           # → D:\jt\release\jobassistant-app.exe 6.92 MB
```

**启动冒烟必须跑 `D:\jt\release\jobassistant-app.exe`**，不能跑工作区内的产物（原因见 §4.1）。

### 界面开发不用起壳

```powershell
cd frontend && Remove-Item Env:\CARGO_TARGET_DIR; trunk serve    # :8080
```

但此时没有 `__TAURI_INTERNALS__`，`invoke` 全失败。现有做法（`.tmp-test/` 里，未入库）：

```powershell
node .tmp-test/serve-frontend.cjs                # 静态服务 frontend/dist，端口 5420
node .tmp-test/make-preview.cjs                  # 生成 dist/preview.html，注入 __TAURI_INTERNALS__ 假桩
chrome --headless=new --disable-gpu --no-sandbox --hide-scrollbars `
  --user-data-dir=.tmp-test\cp6 --window-size=1600,1400 --virtual-time-budget=14000 `
  --screenshot=.tmp-test\shot.png "http://127.0.0.1:5420/preview.html?theme=dark"
```

---

## 3. 下一步做什么（按优先级）

| # | 任务 | 说明 |
|---|---|---|
| **P0** | **修 sidecar 拉不起 Chrome** | 唯一的阻塞项。根因与修法见 §5.1 |
| P1 | 前端三页：`pages/{chat,config,logs}.rs` | 目前是「施工中」占位符。逻辑可直接照抄 `frontend/src/features/` 里已有的 `ChatPanel` 思路 |
| P1 | 抽屉 / 骨架条 / 动效 | 用户明确要求「与 Web 版原样一致，不得降级」。样式可参考 `skeleton-web/src/styles/drawer.css`（只读目录） |
| P2 | Liquid Glass 第二层 | SVG `feTurbulence` + `feDisplacementMap` 湍流折射。契约在 `frontend/.agents/skills/liquid-glass/SKILL.md`（核心：**Refract a Cloned Backdrop, Never the Live DOM**） |
| P2 | Liquid Glass 第三层 | WebGL 只在卡片之间做真折射（方案 C：亚克力继续透桌面）。抽屉盖列表、气泡盖背景 |
| P2 | 修 2 处分层违规 | `components/shell.rs:202` 依赖 `pages`；`lib_ui/job.rs:3` 依赖 `api`。`tools/check_layers.py` 会报 |
| P3 | `tools/gui_e2e.py` 对齐 | 16 项 `REQUIRED_TESTIDS` 与新概览页不匹配（缺 `app-root`/`score-spec`/`scrape-status`），现在跑 `--ui-only` 会失败 |
| P3 | 拆三个巨文件 | `core/sidecar/process.rs`（750 行业务）、`commands_scrape.rs`（859 行，编排逻辑违反"命令层只转发"）、`core/infra/sqlite.rs`（2126 行） |
| P3 | `docs/` 重整 | `SPEC-001` / `ADR-001` / `ADR-002` / `ARCH-002` 描述的是上一代实现，`SPEC-002` 第 4 节蓝图与代码不符。**在新文档落地前，`AGENTS.md` 第 2 节就是唯一边界** |

---

## 4. 环境坑（照着做，别自己试）

### 4.1 产物的完整性级别

工作区 `D:\AIProject\JobAssistant` 带 **Low 完整性标签**（`S-1-16-4096`），里面的 exe 会被 Windows 降级，
**写不了 `%LOCALAPPDATA%`**，启动报 `os error 5` / `0x800700AA`。
→ **产物必须输出到 `D:\jt`**（`$env:CARGO_TARGET_DIR = "D:\jt"`，`tauri build` 产物落在 `D:\jt\release\`）。
这不是代码问题，**不要改代码去绕**。

### 4.2 trunk

- `cargo install trunk` 在本机编译 `h2` 时 rustc 会崩（`STATUS_STACK_BUFFER_OVERRUN`）。用 GitHub release 预编译包（已装在 `%USERPROFILE%\.cargo\bin\trunk.exe`）。
- **必须 `Remove-Item Env:\CARGO_TARGET_DIR`** 再跑，否则报 `cargo artifacts not found for target crate`。
- trunk **不认** `--target-dir`。
- trunk 要的是 **bin** 的 wasm。`frontend/Cargo.toml` 里的 `[[bin]]`（`path = "src/main.rs"`）**删了就构建不出 wasm**。

### 4.3 CSS

**CSS 文件绝不能以 `//!` 或 `///` 开头**。那是 Rust 注释，CSS 解析器判定整份样式表非法后**静默丢弃**，
表现是「`tokens.css` 的变量生效了、但组件样式全无」，极易误判成选择器或优先级问题。
本次踩过的定位手段：`getComputedStyle(document.querySelector(".scrape-controls")).gridTemplateColumns` 读到 `none` 就说明整份文件没加载。

### 4.4 Leptos 0.8

- 组件 props **不能用含 `AnyView` 的类型**（`AnyView` 内含 `NonNull<()>`，不满足 `#[component]` 要求的 `Sync`）。插槽一律用 `Children` / `ChildrenFn`。
- `#[prop(optional)]` **已隐含 `Option`**，调用点不要再包 `Some(…)`。
- `Box::new(move || view!{…}) as Children` 的闭包体必须 `.into_any()`。
- 不要信 `.as_read_signal()`——`Signal<T,S>` 上没有这个方法。

### 4.5 PowerShell

**批量改源码一律用 Python 的 `str.replace`（字面量），禁止 PowerShell `-replace`。** 正则引擎会吃掉替换串里的字符，本次曾把四份源码里所有 `i` 变成 `m`，269 个语法错误。

`git commit -m` 传多行中文消息会失败（报 `fatal: Invalid path`）。用 `git commit -F .tmp-test/commit-msg.txt`。

### 4.6 环境变量（历史遗留）

| 变量 | 作用 |
|---|---|
| `AJA_DATA_DIR` | 数据目录覆盖 |
| `AJA_DIAG_WINDOW` | 设 1 时 stderr 打印窗口诊断（`[diag] hwnd=… layered=…`），毛玻璃验收用 |
| `AJA_NO_STEALTH` | **严禁设为 1** 跑真实抓取（会被风控） |
| `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` | 给 WebView2 加 `--remote-debugging-port=N`，GUI E2E 靠它连进来 |

---

## 5. 阻塞项与关键缺陷

### 5.1 【阻塞】sidecar 拉不起 Chrome（P0）

**现象**：应用运行时，任何聊天 / 期望读取 / 抓取都失败：
```
TargetClosedError: BrowserType.launch_persistent_context: Target page, context or browser has been closed
```
Chrome 自身的输出是「正在现有的浏览器会话中打开」然后 `exitCode=0`。

**根因**（已定位，Sub-Agent 交叉验证）：应用运行时 `msedgewebview2.exe` 会在本机创建 Chromium 单例命名对象，
sidecar 启动的 Chrome 探测到之后认定「已有会话」，退出，playwright 随即收到 CDP 管道关闭。

**决定性对照实验**：
```
A（应用未运行）  → OK pages: 1
B（应用在运行）  → FAIL TargetClosedError
```
同一份 Python 代码、同一个 profile，唯一差别是应用是否在跑。

**已排除**（别再重复试）：Job Object（`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 后仍成功）、环境变量（含 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`）、
stdio 全管道、`CREATE_NO_WINDOW`、`--user-data-dir` 路径（已修成 `…\data\browser-profile`）、profile 脏数据/残留锁、Chrome 版本、`channel="chrome"` 与 `executable_path` 两条启动路径。

**建议修法**：给 sidecar 的 Chrome 一个与应用 WebView2 **不再撞名**的 user-data-dir；
或分离 `--user-data-dir` 与 `--apps-user-data-dir`。

### 5.2 已修但容易回退的坑（改代码前读一遍）

| 位置 | 规则 |
|---|---|
| `core/sidecar/session.rs::send` | 只有 `Command::ScrapeStart` 才置 `running = true`。曾经对所有命令置位，导致只点「扫码登录」也显示「抓取中」 |
| `core/sidecar/session.rs` 的 `Ready` 分支 | 只清 `finished_reason`，**不**置 `running` |
| `core/sidecar/process.rs::route_reply` | 判据是**有 `id` 且**无 `event`。事件帧也可能带 id，只看 id 会吞事件 |
| `core/sidecar/process.rs::error_text` | sidecar 的错误是**对象** `{code,msg}`，只 `as_str()` 会把原因吞成「未给出失败原因」 |
| `core/domain/job.rs` 的薪资排序 | SQLite `ORDER BY x ASC` 把 NULL 排**最前**（与 MySQL/PG 相反）。`SALARY_UNKNOWN_RANK_EXPR` 必须恒 ASC 且作第一排序键 |
| `core/domain/job.rs::list_scored_ids` | 必须按 `job_id` 升序。按 `match_score` 排序会被自己的写回推翻分页边界，同一岗位算两次**且不报错** |
| `core/sidecar/ingest.rs` | 无 JD 否决只能走 `RuleScore::passes()`。写 `score < min_score` 在门槛 ≤70 时放行 |
| `commands_*.rs` 的 Args 结构 | 前端发驼峰、Rust 蛇形，serde 对未知字段**不报错**。新增参数必须加 `alias`，否则值被静默丢弃 |
| `scraper/sidecar.py` | `page.on("response")` 回调里**绝不能**调 `resp.text()`，否则 `Connection closed while reading the driver`，之后全部 `Event loop is closed` |
| `scraper/sidecar.py` | `scrape.stop` 不结束进程，必须再关 stdin（= `scrape_close`），否则脚本 wait 超时 |
| `scraper/boss.py` | 翻页是**无限滚动**，按「卡片数增长」判定。`hr_active_status` 选择器是 `.job-boss-info .boss-active-time`，抓不到会让所有岗位被 `exclude_hr_active_days` 误杀 |
| `lib.rs` | `CreateMutexW` 的指针必须写 `*mut ::core::ffi::c_void`（带前导 `::`），否则被新增的顶层 `core` 模块抢占解析 |

---

## 6. 仓库纪律

- **只读参考目录，禁止删除、禁止改动**：`backend/`、`web/`、`src-tauri/`、`skeleton-web/`。
  `backend-exe/` 已被误删且**无法恢复**（从未入 git）。这些是用户明确要求保留的参考。
- **`data/` 含登录 cookie 与 Chrome profile**，已在 `.gitignore`，不得 `git add -f` 绕过。
- 提交规范 `Conventional Commits`，scope 用 `core` / `desktop` / `client` / `sidecar` / `contracts` / `arch` / `release`。
- 未通过 `cargo test` 前禁止 `git commit`。

---

## 7. 当前工作区状态（交接时刻）

未提交改动：

- `M AGENTS.md`（全文重写）、`M README.md`（全文重写）、`M DEVELOPMENT.md`（顶部加本轮记录）
- `M app/src-tauri/{Cargo.toml,Cargo.lock,tauri.conf.json}` + 五个 `commands_*.rs` / `lib.rs`
- `D core/**`（25 个文件，已并入 `app/src-tauri/src/core/`）
- `?? frontend/`（整个新前端 crate）、`?? app/src-tauri/src/core/`、`?? app/src-tauri/tests/`、`?? tools/chat_readonly.py`

建议的第一个提交：`refactor(core): 将 core 并入 app crate，界面改用 Leptos`，
把 `core/` 删除与 `app/src-tauri/src/core/` 新增放进**同一个**提交，否则中间态编译不过。

---

## 8. 能力坐标（避免重复摸索）

| 能力 | 现状 |
|---|---|
| 抓取 Boss 岗位 | ✅ sidecar 直连可用；❌ 从应用内拉起 Chrome 失败（§5.1） |
| 打分（规则 / 大模型 / 重算） | ✅ 261 单测覆盖 |
| 存储（岗位 / 敏感库 / 决策 / 配置） | ✅ 全 CRUD 已实现 |
| Boss 聊天（列表 / 消息 / 发送 / 简历 / 撤回） | ✅ 协议与 UI 逻辑齐备，受 §5.1 阻塞 |
| 打招呼（生成 → 确认 → 发送） | ✅ 两步拆分，规则兜底 |
| 求职期望读取 | ✅ 复用浏览器会话，无需解密 cookie |
| 界面 | 概览页 ✅；消息 / 配置 / 决策记录 ❌ |
