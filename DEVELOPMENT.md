# 开发进度记录

> 本文件记录 JobAssistant 每一轮的改动、踩过的坑和待办。追加式维护，最新的在最上面。

## 2026-11-21 `core/` 并入 `app/`，前端改 Leptos，交接给下一位

### 现象 / 需求
用户决策：① 界面放弃 TypeScript，改用 **Rust + Leptos 0.8（WASM）** 重写，设计语言定为 **Liquid Glass**（深/浅双主题各自调优，折射方案 C：亚克力继续透桌面，WebGL 只做卡片间真折射）；② `core/` 不再单独成 crate，**并入 `app/src-tauri/src/core/`**；③ 重写根 `AGENTS.md`，只描述当前这条主线。

### 改动
- **合并**：`core/src/{domain,infra,sidecar}` + `lib.rs`（改名为 `mod.rs`）→ `app/src-tauri/src/core/`；`core/tests/sidecar_smoke.rs` → `app/src-tauri/tests/`。路径重写用 Python 字面量 replace（**禁止 PowerShell `-replace`**）：内部 `crate::{domain,infra,sidecar,scoring}::` → `crate::core::…`，命令层 `jobassistant_core::` → `crate::core::`，共改写 18 个文件。
- `app/src-tauri/Cargo.toml`：删 `jobassistant-core` 路径依赖，把 `thiserror` / `regex` / `rusqlite(bundled)` / `ureq` 搬进 `[dependencies]`。
- `app/src-tauri/src/lib.rs`：加 `pub mod core;`；**`CreateMutexW` 的 `*mut core::ffi::c_void` 必须写成 `*mut ::core::ffi::c_void`**——新增的顶层 `core` 模块会抢占路径解析，报 `error[E0433]: cannot find ffi in core`。
- `frontend/` 新建：Leptos 0.8 CSR + trunk 0.22.2，`index.html` + `styles/{tokens,layout,controls,jobs,scrape}.css` + `src/{api,lib_ui,components,features,pages}`；`tauri.conf.json` 的 `frontendDist` 改指 `../../frontend/dist`。
- `AGENTS.md` 全文重写（六节模板），已按合并后的真实路径逐条校正。
- `tools/check_layers.py` 重写为扫 `frontend/src/**/*.rs`；`tools/make_icons.py` 路径改为 `app/src-tauri/icons`。

### 验证
- `cd app/src-tauri && cargo test` → **297 passed / 0 failed**（261 核心 + 36 命令层），零警告。
- `cd frontend && cargo check --target wasm32-unknown-unknown` → exit 0；`trunk build --release` → wasm 2.9 MB。
- `npx tauri build --no-bundle` → `D:\jt\release\jobassistant-app.exe` **6.92 MB**（比合并前多约 160 KB，即压缩后的 Leptos 前端）。
- 深浅双主题截图核对通过（`.tmp-test/leptos-dark.png` / `leptos-light.png`）。

### 踩坑（下一位必看）
1. **CSS 文件不能以 `//!` 开头**。那是 Rust 注释语法，CSS 解析器会判整份样式表非法并**静默丢弃**，表现出来是「`tokens.css` 变量生效、组件样式全无」，极易误判成选择器问题。定位手段：`getComputedStyle(el).gridTemplateColumns` 读到 `none`。
2. **`trunk build` 必须清 `CARGO_TARGET_DIR`**（`Remove-Item Env:\CARGO_TARGET_DIR`），否则报 `cargo artifacts not found for target crate`；它也不认 `--target-dir`。另外 trunk 要的是 **bin** 的 wasm，`frontend/Cargo.toml` 的 `[[bin]]`（`path = "src/main.rs"`）不可删。
3. **Leptos 0.8 的组件 props 不能用含 `AnyView` 的类型**（`AnyView` 内含 `NonNull<()>`，不满足 `#[component]` 要求的 `Sync`）。插槽一律用 `Children` / `ChildrenFn`；`#[prop(optional)]` 已隐含 `Option`，调用点不要再包 `Some(…)`；闭包体必须 `.into_any()`。
4. **误删了旧版只读目录**（`backend/`、`web/`、`src-tauri/`、`skeleton-web/`）。`git checkout HEAD -- <目录>` 已全部恢复；`backend-exe/` 因从未被 git 跟踪**无法恢复**，需重跑旧版 `build.bat` 才能再有。用户明确要求这些目录保留作参考，任何人不得再删。

## 2026-11-21 后端补齐三处 SPEC-002 缺口：无 JD 否决失效 / 全量重算入口 / 崩溃重启一次

### 现象（三处，均为「规格要求了但实现里没有」）
1. **无 JD 岗位会被静默入库**。`core/src/sidecar/ingest.rs::prepare` 只判 `score < cfg.min_score`，
   没消费 `RuleScore::rejected_no_jd`。无 JD 岗位走降权通道（技能 12 + 薪资 10 + 经验 8 + 方向 5 + 公司 4）
   最高仍有 72 分，**门槛设 70 时会被入库**，直接违反 SPEC-002 第 3.1 节「无 JD 一律否决」。
   同一缺陷在大模型通道上更贵：`scoring_service::score_job` 会先花一次 token，再让结果被否决。
2. **没有全量重算入口**。SPEC-002 第 3.3 节末句要求「迁移前后须提供 `rescore` 入口，
   供用户以新口径重算全部岗位」。十分制→百分制迁移只换算存量数字，不会用新权重重算，
   用户升级后看到的仍是一批旧口径分数，且没有任何按钮能补救。
3. **sidecar 崩溃不自愈**。SPEC-002 第 7 节风险表要求「抓取进程崩溃后自动重启一次并保留进度」。
   `SidecarSession::restarts` 字段与 `restarts()` 方法早已存在，但**没有任何调用方**——
   崩溃后界面停在「抓取进程异常退出」，用户只能manual 重来，已抓的部分虽在库里，进度条却归零。

### 根因
三处都是「领域能力已具备、调用点没接上」：`passes()` 存在却没被 ingest 调用；
`JobStore` 缺少一个按主键游标翻页的读方法，导致重算无从批量驱动；重启策略按注释成了
「留给上层决定」，而上层一直没写。

### 改动文件
- `core/src/sidecar/ingest.rs`：`prepare()` 改为 `let rule = rule_score_all(...)` 后
  `if !rule.passes(cfg.min_score)`。新增回归 `prepare_blocks_no_jd_even_when_threshold_is_low`
  （`jd_text=Some("   ")` + `min_score=0`，断言不入库且 reason 含「无 JD」）。
- `core/src/domain/scoring_service.rs`：`score_job()` 在大模型调用前早退
  `if rule.rejected_no_jd { return rule_outcome(rule); }`；既有测试补
  `assert_eq!(fake.call_count(), 0, "无 JD 岗位不该消耗大模型 token")`。
- `core/src/domain/rescore.rs`（新增，约 400 行）：`RescoreOptions` / `RescoreChange` / `RescoreOutcome`
  + `rescore_batch<S: JobStore>(store, &Profile, &RescoreOptions, Option<&dyn LlmClient>)`。
  **游标分页**（`after_job_id` + `limit`，`limit` 夹到 ≥1），单条写失败只计数不中断，
  可选写 `jd_decisions` 审计（`log_changes`）。`use_llm` **默认 false**：全库走大模型是按条计费的操作。
- `core/src/domain/job.rs`：`JobStore` 新增 `list_scored_ids(after: Option<&str>, limit: u32)`。
  排序键**必须是 `job_id`**：按 `match_score` 排序会让重算写回造成分页边界漂移（同一岗位算两次、别的被跳过）。
- `core/src/infra/sqlite.rs`：实现 `list_scored_ids`（`WHERE job_id > ?1 ORDER BY job_id ASC LIMIT ?2`），
  不过滤 `rejected`——被排除的岗位也要重算，否则用户「排除后又想看」会看到旧口径分数。
- `core/src/sidecar/session.rs`：新增 `bump_restarts()` / `progress_handle()` / `stderr_tail()` /
  `adopt_progress(&ScrapeProgress)`。`adopt_progress` 继承五路计数、`last_title` 与非 `unknown` 的登录态，
  **不继承** `finished_reason`（新进程当然没结束），并置 `running=true`。
- `app/src-tauri/src/commands_db.rs`：新增 `rescore_jobs` 命令 + `RescoreArgs`（驼峰/蛇形双写
  `afterJobId`/`useLlm`/`llmPrefilter`，`limit` 默认 50 夹在 1..=500，`log_changes` 默认 true）
  + `RescoreReply` / `RescoreChangeReply`。
- `app/src-tauri/src/lib.rs`：注册 `commands_db::rescore_jobs`。
- `app/src-tauri/src/commands_scrape.rs`：新增 `ScrapeJobSpec` + `LAST_SPEC`（本次任务的指令快照）、
  `launch_session()`（首次启动与重启走同一条路径，避免参数分叉）、`maybe_restart()`、
  纯函数 `restart_decision()`、`tail_hint()`、`RestartAction{Skip,Restart,GiveUp}`、`MAX_AUTO_RESTARTS=1`。
  重启挂在 **`scrape_drain`** 上（`Exited` 从这里冒泡给前端，别处重启会让前端状态与日志对不上）；
  `scrape_start` 记快照。新增 `UpdateReply::Restarted { pid }`。
  **模块头注释原本写着「本层不自动重启」，已同步改写**——旧注释与实际行为相反时会误导下一个人。
- `skeleton-web/src/App.tsx`：`ScrapeUpdate` 联合类型补 `{ kind: 'restarted'; pid: number }`
  与对应文案分支（`exited` 不带 `restarted` 会让 TS 的穷尽 switch 报错，这是契约联动的必然改动）。
  **另新增「全量重算」面板**（`data-testid="rescore-panel"` / `rescore-btn` / `rescore-llm-toggle`
  / `rescore-note`）：`onRescore()` 循环 `invoke('rescore_jobs', { args: { afterJobId, useLlm } })`
  直到 `has_more === false`，分批大小交给 Rust 默认值（50），界面不暴露它——否则用户会以为「只重算了 50 条」。
  大模型勾选框在 `llm_status.configured` 为假时禁用：**重算走大模型是按条计费的真调用**。
- `tools/gui_e2e.py`：`REQUIRED_TESTIDS` 补 4 个重算锚点；新增「全量重算跑完」「重算幂等」两个断言
  （连点两次，第二遍 `改写` 必须为 0）。

### 重启判定（`restart_decision`，四情形全部有单测）
| 情形 | 结论 |
|---|---|
| `finished_reason == 已按用户请求停止` | `Skip`（退出码非 0 也不复活） |
| 退出码 `Some(0)` | `Skip`（关键词跑完是正常收工） |
| 未重启过 + 崩溃 | `Restart` |
| 已重启过又崩 | `GiveUp`，把「连续崩溃 + 退出码 + stderr 尾行」写进 `finished_reason` |

### 验证（终端实跑）
- `cd core && cargo test` → `208 passed; 0 failed`（原 197，新增 11）+ `sidecar_smoke` 1 ignored
- `cd app/src-tauri && cargo test --lib` → `29 passed; 0 failed`（原 18，新增 11）
- `cd skeleton-web && npm run build` → 通过（`dist/assets/index-BTZLfTla.js` 238.77 kB）
- 仓库根 `python -m compileall -q scraper tools` → exit 0
- `cd app && npx tauri build --no-bundle` → `Finished release profile in 4m 26s`，
  产物 `D:\jt4\release\jobassistant-app.exe` 6.66 MB（SPEC-002 V1 ≤20 MB 仍成立）
- `python tools/gui_e2e.py run --ui-only --exe D:\jt4\release\jobassistant-app.exe` → **5/5 通过**，
  其中真机数据：`重算完成：扫描 30 条，改写 17 条`，第二遍 `扫描 30 条，改写 0 条`（幂等）。
  该 30 条来自真实库 `%LOCALAPPDATA%\com.ai.jobassistant\job_assistant.db` 的只读备份——
  **真实库全程未被写入**（只读 `?mode=ro` 导出 `backup()` 到 `.tmp-test\rescore-copy\snapshot.db`），
  重算都发生在备份上。写完再用 `?mode=ro` 复查真实库：30 条 `score_scale='100'`、分数区间 62–90。

### 修掉的编译期纰漏（只在 `cargo test` 才暴露）
`core/tests/sidecar_smoke.rs` 仍按 4 参数调用 `SidecarSession::start`，而它早已是 5 参数
（多一个 `lookup`）。`cargo test --lib` **不编译 `tests/`**，所以本轮前几次门禁全绿却没发现。
已改成 `Ports { put, remember }` + `NoSensitive`，并把 HANDOFF 的门禁命令从 `cargo test --lib`
改成 `cargo test`。

### 没做的事（有意为之，不是遗漏）
- **`chat.send`（打招呼）未实现**：SPEC-002 第 2.1 节协议示例里出现过，但它要同时改
  `scraper/sidecar.py` + `core/src/sidecar/protocol.rs` + 前端三端，属独立特性而非缺口，用户已确认本轮不做。
- **数据库路径未动**，仍是 `<app_local_data_dir>/job_assistant.db`。

### 踩坑
`$env:CARGO_TARGET_DIR="D:\jt"` 报 `failed to open: D:\jt\debug\.cargo-build-lock / 拒绝访问 (os error 5)`，
删锁文件与 `D:\jt2`、`D:\jt3` 同样被拒；换 **`D:\jt4`** 后一次通过。同 AGENTS.md 槽位 A 的老坑，
工作区父目录 ACL 会不断套到新建目录上，遇到就再换一个短路径。

## 2026-11-21 毛玻璃其实从未生效：Mica 挡住亚克力 + Win11 上 apply_acrylic 名不副实

### 现象
用户问「毛玻璃效果呢」。实机对照截图（同一份 release 产物、同一条启动路径）确认：
窗口客户区是纯色深蓝黑，`CopyFromScreen` 截不到任何桌面模糊。

### 根因（三层叠加，缺一层都看不出来）
1. **`apply_mica` 抢先 return**：`apply_blur` 原先「先 mica、失败再 acrylic」，而
   `apply_mica` 在本机**返回 Ok**。Mica 只作用于窗口非客户区（标题栏/边框），
   客户区被 WebView2 覆盖，所以屏幕上等于什么都没发生。
2. **`window-vibrancy 0.8.1` 的 `apply_acrylic` 在 Win11 上不是亚克力**：
   `windows.rs:62-70` 只调 `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW)`，
   **完全不碰 `ACCENT_POLICY`，且把 `color` 参数丢掉**。该后景类型要客户区透明才可见。
   `is_backdroptype_supported()` 只是探测 `dwmapi.dll` 导出，不看系统版本，Win11 上必然为真，
   于是永远走这条路。
3. **WebView2 客户区永远不透明**：`wry-0.57.3/src/webview2/mod.rs:2016`
   `if is_windows_7() || a != 0 { a = 255; }`——运行期设背景色时 alpha 非 0 一律强制不透明；
   而 `tauri.conf.json` 的 `transparent: true` 只让**窗口**支持透明，不改变 WebView2 的绘制。

### 改动文件
- `app/src-tauri/src/lib.rs`
  - 新增 `apply_accent_acrylic(window, tint)`：直接走
    `SetWindowCompositionAttribute(WCA_ACCENT_POLICY, ACCENT_ENABLE_ACRYLICBLURBEHIND)`。
    它让 DWM 模糊窗口**背后**的内容，不要求客户区透明——这是 WebView2 应用里唯一能看到效果的实现。
    该 API 未文档化、`windows-sys` 不导出，故与 `window-vibrancy` 同样用 `LoadLibraryA` + `GetProcAddress` 取指针。
  - `ACRYLIC_TINT = (18, 24, 38, 125)`；`GradientColor` 必须按 **AABBGGRR** 字节序拼（**不是** RGBA）；
    `accent_flags = 2` 避免圆角处出现硬边。
  - 降级链改为「SWCA 亚克力 → `window_vibrancy::apply_acrylic` → `apply_mica` → 纯色」，
    每步失败只打日志不报错（SPEC-002 V5：绝不能白屏或透明穿透）。
  - 删除 `make_webview_transparent`（`set_background_color` 改不了客户区透明度，属无效代码）。
  - 新增 `window_diag`：设 `AJA_DIAG_WINDOW=1` 时打印 hwnd、`style`/`exstyle`（含 `WS_EX_LAYERED`）
    与两次特效调用的返回值。**这次正是靠它读出 `apply_acrylic = Ok` 却 `layered=false` 才定位到病因。**
- `app/src-tauri/Cargo.toml`：新增 `windows-sys = "0.59"`（`Win32_Foundation` / `Win32_System_LibraryLoader` / `Win32_UI_WindowsAndMessaging`），仅 Windows 目标。
- `skeleton-web/src/App.tsx` 无需改动：根容器本就是 `background: 'transparent'`，页面层没有遮挡。

### 验证
- `cd app/src-tauri && cargo test --lib` → **18 passed**。
- `cd app && npx tauri build --no-bundle` → Finished release，2m33s。
- 实机屏幕截图（`.tmp-test/glass-final.png`）：客户区透出桌面编辑器背景的模糊影，**亚克力生效**。

### 踩坑与待办
- **验证毛玻璃不能用 `PrintWindow`，也不能只截窗口**：亚克力是 DWM 合成结果，
  `PrintWindow(PW_RENDERFULLCONTENT)` 拿到的是应用自己绘制的内容（纯黑），会把「已生效」误判成「没生效」。
  必须 `SetForegroundWindow` + `CopyFromScreen` 截桌面，并让窗口背后有高对比度内容。
- **验证必须用 `npx tauri build --no-bundle` 的产物**：`cargo build` 是 dev 档（`cfg(dev)=true`），
  会去连 `devUrl: http://localhost:5184`，没起 devserver 时页面是 `ERR_CONNECTION_REFUSED`，截图无意义。
- **`MainWindowHandle` 可能指向别的窗口**：本轮有一次截到了 DSH 自己的窗口。
  取到 hwnd 后应打印 `title` 与 `rect` 核对，不能直接假定。
- `apply_accent_acrylic` 的 `hwnd` 形参类型是 `*mut c_void`（`window.hwnd()` 的返回），**不是 `isize`**，写错会报 E0308。

## 2026-11-21 真实库 `data/job_assistant_pro.db` 十分制 → 百分制迁移（用户授权「现在做」）

### 现象
`data/job_assistant_pro.db` 仍是 31 列、无 `score_scale`、分数 7/8/9，界面按 100 分制展示时
会显示成「7 分」；`app_config` 里 `llm_prefilter = 5`（初筛线本意 50）。

### 根因（两个缺陷，动手前先修，否则这次迁移会留下不可逆污染）
1. **`migrate()` 不是原子的**：`core/src/infra/sqlite.rs` 里逐条 `conn.execute()`，每条自动提交，
   与 `core/src/domain/migration.rs` 模块头注释「整体在一个事务里完成」不符。若崩在
   「分数已乘 10、`score_scale` 标记仍是 `'10'`」之间，重跑就是 **×100**。
2. **配置阈值换算从未生效**：`convert_legacy_threshold` / `SCALE_CONFIG_KEYS`（`min_score`、`llm_prefilter`）
   **没有任何调用点**，`llm_prefilter = 5` 永远不会变成 50。

### 改动文件
- `core/src/infra/sqlite.rs`
  - `migrate()` 改为 `let tx = conn.unchecked_transaction()?;` → 循环内 `tx.execute(...)` →
    `migrate_config_thresholds(&tx)?` → `tx.commit()?`，失败整体回滚。
  - 新增 `fn migrate_config_thresholds(tx: &rusqlite::Transaction<'_>) -> Result<bool>`：
    事务内 SELECT `value` / `convert_legacy_threshold` / UPDATE，复跑幂等。
  - 新增用例 `migrate_converts_config_thresholds`：`llm_prefilter '5' → '50'`、`min_score '70'` 不变、复跑 `changed=false`。
- `AGENTS.md`：第 6 节第 7 项标记已关闭；槽位 B core 用例数 196 → 197。

### 验证
- **先在副本上跑真实 `migrate()`**（临时 `core/examples/migrate_runner.rs` 调 core 代码，而非另写一份 SQL——
  否则测的不是产品代码；runner 用后即删、不进版本库）：`.tmp-test/migrate-copy.db` 结果完全符合预期后才动真库。
- 真库结果与副本一致：**31 → 32 列（+`score_scale`）、20 行不变、7/8/9 → 70×4 / 80×4 / 90×12、
  `score_scale` 全为 `100`、`llm_prefilter '5' → '50'`、`min_score` 仍为空**。
- `PRAGMA integrity_check = ok`；复跑 `migrate() -> changed=false`（幂等）。
- 备份：`data/backup/job_assistant_pro.20261121-022134.db`（含 `-wal` / `-shm`），与迁移前原件
  SHA256 一致（`A9D2666D4F22A250D9F27FC61D49BE73E8EA2D4B400C78F70690B759F36484E4`）。

### 踩坑与待办
- **`data/` 不在 git 里**（含登录 cookie，已被 `.gitignore` 忽略），所以迁移**没有版本回滚兜底**，
  物理备份是唯一退路。以后任何对真实库的写操作都必须先 `Copy-Item` 到 `data/backup/<名>.<时间戳>.db` 并核对 SHA256。
- `job_assistant_pro.db` 的 `journal_mode=wal` 且有 4 KB 未落盘 WAL：备份必须**连 `-wal` / `-shm` 一起拷**，
  只拷主文件可能丢最后几条写入。

## 2026-11-21 修回 `hr_active_status`：此前所有抓来岗位都会被「活跃度未知」否决

### 现象
交接清单第 8 项一直挂着一个「无法解决」的尾巴：DOM 通道拿不到 `hr_active_status`，
所以规则 `exclude_hr_active_days` 对抓来的岗位不生效。上一轮据此写了
「卡片与详情页都没有该信息，需先确认页面是否存在」——**这个判断是错的**。

### 根因（两层，第二层才是真正严重的）
1. **选择器过期**。活跃度实际在 `.job-boss-info` 区块（`.boss-active-time` → `"刚刚活跃"`），
   而 `scraper/boss.py` 查的是 `.job-banner` / `.info-company`——那是旧版页面结构，本站已不存在。
   `re.search` 在空串上永远不命中，`item["hr_active_status"]` 便从未被赋值。
2. **这个字段为空不是「少个字段」，是「全量否决」**。`core/src/domain/rules.rs:246` 起：
   ```rust
   if let Some(limit) = rules.exclude_hr_active_days {
       match hr_active_days(job.hr_active_status) {
           None => return Some(format!("HR 活跃度未知（要求 {limit} 天内活跃）")),
   ```
   注释原文「无法判断的按不活跃处理」。也就是说：**用户一旦开启这条规则，
   抓来的每一个岗位都会被判否决，且理由统一显示「HR 活跃度未知」**——
   界面看起来像「今天没有合适的岗位」，实际是数据通道断了。

### 改动
- `scraper/boss.py`
  - 活跃度优先取 `.job-boss-info .boss-active-time, .boss-active-time`；
    取不到时回落整块 `.job-boss-info` 再做一次正则（防文案换标记）。
  - 正则补 `[今日本周本月今天]+活跃` 与 `刚刚回复/本周回复/今日回复` 等变体。
  - `hr_name` 优先 `em.name`（纯姓名），其次 `.name`（innerText 是「姓名\n活跃度」两行，取首行）。
  - `hr_position` 优先 `p.boss-info-pub`（纯职位）；只有 `p.boss-info-attr`（「公司\n·\n职位」）时用新增的
    `_last_line()` 取末行。
- `core/src/domain/rules.rs` 新增回归用例 `scraped_variants_of_hr_activity_are_recognized`：
  把**抓取端真实产出的文案**与 `parse.rs::hr_active_days` 的口径钉在一起——
  `刚刚 / 刚刚活跃 / 在线 / 今日活跃` 必须放行，`本月活跃 / 1月内活跃 / 半年内活跃` 必须否决。
- `tools/detail_dump.py` 增加 HR 活跃度取证段（`.job-boss-info` / `.boss-active-time` 等选择器 count+text，
  以及 body 是否含「活跃/在线/分钟前」），下次同类问题可直接定位。

### 验证
- `python tools/detail_dump.py boss:78952ac10e88740e0nx-3Nm_ElNZ`
  → `.boss-active-time` count=1 text=`'刚刚活跃'`，`.job-boss-info` 全文
  `'刘晴晴\n刚刚活跃\n天津跃仕达科技发展\n·\nHR.招聘专员'`。**页面有数据，是选择器的问题。**
- `python tools/scrape_probe.py --keyword Java --pages 1 --limit 3 --budget 80`
  → `[详情字段缺失] 无`（上一轮同样命令报 `hr_active_status: 3`）；三个 HR 字段为
  `曹女士 | 招聘经理 | 刚刚`、`俞女士 | hrbp | 刚刚`、`朱瑞 | 招聘者 | 刚刚`。
- `cd core && cargo test --lib` → **196 passed; 0 failed**（新增 1 例）。
- `python tools/gui_e2e.py run --min-score 100 --budget 150` → **14/14 全绿**（Python 侧改动由 sidecar 现场加载，无需重建 Tauri）。

### 踩坑与教训
- **「页面没有数据」这个结论必须用 DOM 原文取证，不能用选择器去猜**。上一轮之所以误判，
  是因为只跑了 `_first_text` 返回空——那只能证明「我的选择器没命中」，不能证明「页面没有」。
  `tools/detail_dump.py` 存在的意义就是把这两件事分开；`company_stage` 那次也是靠它确认的（那次是真没有）。
- **字段缺失要先算后果再排优先级**。`hr_active_status` 被当成「低优先级字段缺口」挂了很久，
  实际上它决定了一整条规则是否把所有岗位误杀。判断一个缺失字段值不值得修，
  要顺着「谁会读它、读到 None 会怎样」走一遍。
- **抓取端文案与规则端解析分居两个仓（Python / Rust），没有任何编译期约束**，
  只能靠回归用例把两端钉住。以后凡是 `boss.py` 产出的枚举型字符串（活跃度、经验、学历）
  在 Rust 侧被 `match`/`re.search` 消费的，都应有一条用真实抓取文案做输入的用例。

## 2026-11-21 卡片阶段先查敏感库，命中就不再开详情页

### 现象
用户要求：「卡片获取完了就继续向下滚动抓取」「最大滚动 100 条」
「查看卡片详情前先去敏感库对比，不存在的再去打分」「卡片不进详情就能对比，
已在敏感库中的数据一定是有排除原因的，所以先对比是正确的」。

### 改动
1. **`scraper/boss.py` 改成批次交替**（原实现是「整页详情拉完才滚动」）：
   单关键词内循环「拉本批新卡片的详情 → 到上限就停 → 滚动加载 → 重新计算本关键词的卡片」。
   `mine` 必须每轮重算（`jobs` / `hit_kw` 跨关键词共享，否则第二个关键词会把
   第一个关键词的卡片重拉一遍详情）。日志改为「第 N 批：新增 X 条卡片（累计 Y）」。
2. **滚动上限 100 条**：`scraper/base.py` 新增 `max_cards: int = 100`，
   `boss.py` 里 `card_cap = min(max_cards, daily_limit)`；`sidecar.py` 透传 `max_cards`。
3. **敏感库端口**（`core/src/domain/job.rs`）：新增 `SensitiveStore` trait +
   `SensitiveHit` / `SensitiveEntry<'a>` / `SensitiveKind{Rejected, Stored}`；
   `core/src/infra/sqlite.rs` 提供实现（精确查公司+岗位；公司粗查要求名字 ≥4 字，
   对齐 `backend/app/store.py:448`）。
4. **卡片预筛**（`core/src/sidecar/ingest.rs` 的 `prescan`）：卡片到手先查敏感库，
   再**用空 JD 跑一遍完整 `reject()`**——只会命中不依赖 JD 的规则
   （岗位名/姓名词/短期项目/HR 活跃度/成立年限/薪资），JD 相关规则
   （外包/学历/985/经验）自然留到详情页后。不另写一套规则，避免口径漂移。
5. **入库端口**（`core/src/sidecar/session.rs`）：`IngestPort { put, remember }` 取代
   原来的单闭包 `JobSink`。`put` 写岗位并登记 `kind=stored`；`remember` 把被规则
   否决的岗位登记 `kind=rejected`——下次抓取在卡片阶段直接跳过。
6. **`app/src-tauri/src/commands_scrape.rs`**：`make_sink` 改为 `Ports{put, remember}`，
   新增 `make_lookup`（精确查不中再按公司粗查；**命中即跳过，不看 kind**）。

### 关键词数据量的实测
`python tools/page_probe.py --keyword Java --pages 8 --budget 85` →
第1页 15 / 第2页 30 / 第3页 45 / 第4页 60 / 第5–8页新增 0，共 60 条 / 92.7s。
即「Java」这个关键词在该站点只有约 60 张卡，**达不到 100 上限是数据侧事实**，不是 bug。

### 验证
- `cd core && cargo test --lib` → **195 passed**（新增 `rejected_job_is_remembered_for_next_run`、
  `accepted_job_is_not_remembered_as_rejected`、敏感库 4 例、预筛若干）。
- `cd app/src-tauri && cargo test --lib` → **18 passed**；`cargo check` 无警告。
- `python -m compileall -q scraper tools` → ok。

### 踩坑
- **`prescan` 会先吃掉只靠卡片信息就能判定的规则**。给 bucket=3 写回归用例时用
  「驻场」「外包」这类词做样例，结果是 `rejected=1` 但 `remember` 一次都没调——
  因为它在卡片阶段就被 `prescan` 拦下了。这**正是期望行为**，但测试要验证
  「详情页之后才被否决」的分支，必须换成只有看 JD 才判得出来的规则
  （例：岗位名正常、JD 里写「本岗位为外包驻场」）。同理，任何统计
  `rejected` 的断言都不能区分「卡片阶段拒」与「详情后拒」，要分开计数看 `prescan`。
- **`JobSink` → `IngestPort` 的连带改动**：测试里所有 sink 闭包都要包成
  `Ports { put: .., remember: .. }`；`Job` 的 `company/title/salary/location/job_url`
  是 `Option<String>`（`platform`/`job_id` 才是 `String`），传 `&str` 要 `as_deref()`。
- **`prescan` 必须收 `FnMut`**（lookup 要改内部状态），而 `handle_job` 的泛型参数
  不能同时满足「闭包 impl」与「`&mut T` impl」（E0119），最终 `handle_job` 收
  `&mut dyn SensitiveLookup` 避开冲突。

## 2026-11-21 抓取改 DOM-only + 发现翻页是死代码（每词只抓 15 条）

### 现象
用户要求「默认不再使用接口捕获数据」。删掉接口通道后实测 DOM 通道完全可用，
但顺带挖出两个更严重的问题：**每个关键词实际只能抓到 15 条**，且没人发现过。

### 根因
1. **接口通道本来就在帮倒忙**（`scraper/boss.py`）：`joblist.json` 只在首屏
   `page.goto` 时触发一次；翻页是站内异步刷新、不重新导航，所以第 2 页起接口恒为空。
   它只在第 1 页起作用，却带来两套字段口径（DOM 能拿 `城市·区·商圈`，接口只给 `cityName`）。
   → 整段 `on_response` / `_parse_search` 删除，只走 DOM。
2. **翻页分支是死代码**：站点已从 `/web/geek/job` 迁到 `/web/geek/jobs`，并且
   **改成无限滚动**——body 里既无「下一页」也无「加载更多」，`.options-pages` 容器
   根本不存在。旧选择器 `a.ui-icon-arrow-right` 永远匹配不到，于是每轮在第 1 页
   15 条详情拉完后直接 `break`。这就是「每次恰好 15 条」的真因。
   → `SEARCH_BASE` 改新地址；新增 `_load_more_by_scroll()` 用 `mouse.wheel` +
   **卡片数增长**判定（不能按「滚动成功」判定，滚到底会回弹，否则无限翻页）。

### 字段缺口的定性（不是选择器缺陷）
`tools/detail_dump.py` 取证 `boss:78952ac10e88740e0nx-3Nm_ElNZ`（天津跃仕达）：
`.sider-company` 只有「0-20人 / 互联网」，body 全文不含「轮」「融资」——**Boss 自己没标注**，
属数据缺口。其成立日期 `2026-05-07` 算出 `founded_years = 0`（今年成立，合法值），
被 `_job_to_event` 的 `not in (None, "", 0)` 当真值过滤掉了。
`tools/scrape_probe.py` 的统计口径同步改为**只看键存在**，否则每轮都误报缺失。

### 改动文件
- `scraper/boss.py`：删接口通道与 `_parse_search`；`SEARCH_BASE` 更正；
  新增 `_load_more_by_scroll()`；docstring 记录 DOM-only 理由与 `hr_active_status` 缺口。
- `tools/scrape_probe.py`（新）：直接驱动 sidecar 统计 21 字段完整度，不经 GUI。
- `tools/detail_dump.py`（新）：区分「页面真没有」与「选择器没覆盖」。
- `tools/page_probe.py`（新）：只验翻页，不拉详情（详情每条 3~6 秒，会把预算吃光）。

### 验证
- `python tools/page_probe.py --keyword Java --pages 3` → **15 → 30 → 45 条，31.1s**，
  每页稳定新增 15 条（修复前恒为 15）。
- `python tools/scrape_probe.py --keyword Java --pages 1 --limit 2` → 27s，卡片与详情字段零缺失，
  JD 860–1518 字。
- `core && cargo test --lib` → 189 passed。
- `python -m compileall -q scraper tools` → ok。
- E2E 回归：`python tools/gui_e2e.py run --min-score 100 --budget 200` → 14/14 全绿。

### 踩坑
- **`readline()` 会吃掉预算**：`scrape_probe.py` 一开始用读线程 + 队列做截止，但 ready 握手
  又调了原始 `proc.stdout.readline()`——那一行已被读线程取走，主线程永久阻塞，
  `--budget 45` 实跑 108 秒。握手也必须从队列取。
- **收尾必须「stop + 关 stdin」**：`scrape.stop` 只结束抓取线程，主循环仍在
  `for raw in sys.stdin` 等输入，进程不会自己退出（这是设计，Rust 靠 `scrape_close` 收工）。
  脚本只发 stop 就 `wait(20)`，每次都等满超时才 kill，表现为「命令卡住」。
- `stderr=subprocess.PIPE` 只写不读会写满管道阻塞子进程，改为落文件。
- 用户已要求**所有命令硬上限 100 秒**：完整抓取（含详情）在 100 秒内只能覆盖约 15 条，
  跨页端到端验证需拆成 `page_probe.py`（翻页）+ `scrape_probe.py`（字段）两段做。

### 待办
- **`hr_active_status` DOM 通道拿不到**（卡片与详情页都没有），规则 `exclude_hr_active_days`
  对抓来的岗位**不生效**。是否改从详情页 HR 卡片解析，需先确认页面是否存在该信息。
- 完整抓取跨无限滚动边界的端到端验证，受 100 秒命令上限所限尚未跑到第 2 批详情。

## 2026-11-21 GUI 点击路径验收：抓取首次真跑通，修掉三个「静默失效」

### 现象与手段
用 CDP 连 WebView2（启动前设 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`）
真点界面按钮，跑完「登录 → 抓取 → 落库」全链路。首轮就暴露出三个问题，全部属于
**界面看起来正常、实际参数或状态没生效**的静默失效。

### 三个根因与改动

1. **门槛值被静默丢弃** —— `app/src-tauri/src/commands_scrape.rs` 的 `ScrapeArgs`
   是蛇形字段（`min_score`），前端 `App.tsx` 发的是驼峰（`minScore`）。serde 对未知字段
   不报错，界面上设的分数门槛**永远不生效**，一律按默认 70 走。
   改为 `#[serde(rename_all = "snake_case")]` + `alias = "maxJd" / "min_score"` 双向兼容，
   与 `commands_db.rs` 的 `ScoreArgs` 保持同一写法。

2. **`parse_line` 的 `None` 被调用方当成「解析失败」** —— 这是本轮最隐蔽的一个。
   `parse_line` 用 `None` 表示「这行本来就该丢」（命令回执、未知事件），
   但读线程写的是 `parse_line(line).unwrap_or_else(|| Malformed {...})`，
   于是 `scrape.start` 的回执 `{"id":"c1","ok":true,...}` 在**浏览器还没启动之前**
   就冒成一条「无法解析」推到界面。
   改为三态 `enum LineParse { Event(SidecarEvent), Ignore, Malformed }`，
   只有真畸形（非 JSON 对象、缺 `job_id` 的岗位）才记 `Malformed`。

3. **点「停止」后界面永远停在「抓取中…」** —— `scrape.stop` 只结束 sidecar 内部的
   抓取线程，**Python 主进程仍在等 stdin，不会发 `Exited`**，而 `running` 只认 `Exited`。
   `core/src/sidecar/session.rs` 的 `stop()` 现在立刻把 `running=false` 并写入
   `STOPPED_BY_USER`，`Exited` 到达后再用更精确的原因覆盖。

### 验证（实机，非单测）
工作区外产物 `D:\jt\release\jobassistant-app.exe`，CDP 真点界面：

- 无法解析次数 **恒为 0**（修复前点开始即 1 条）。
- 门槛 60 分真实生效：57 分、53 分被判「未达门槛 60」拦下。
- 硬规则生效：JD 命中「外包」被 `exclude` 否决。
- 去重生效：重复岗位显示「已存在，跳过」，`重复 5`。
- 入库 1 条（`已抓 9 / 入库 1 / 重复 5 / 低于门槛 2 / 规则否决 1`），刷新后行数 29→30。
- 停止后状态为「未在抓取 · 已按用户请求停止」。

门禁：`cd core && cargo test` → **189 passed**（新增回归用例 `scrape_start_reply_is_not_malformed`）；
`cd app/src-tauri && cargo test --lib` → 18 passed。

### 踩坑（下一轮别再踩）
- **杀进程要复核**：`taskkill /F /IM jobassistant-app.exe` 返回成功但进程仍在，
  单实例闸门会拦住新实例，于是后续「重启验证」跑的全是旧二进制，结论不可信。
  必须用 `Get-CimInstance Win32_Process -Filter "Name='jobassistant-app.exe'"` 复核为 0。
- **改完必须重建**：`npx tauri build --no-bundle` 约 2 分钟，跳过它读到的日志来自旧二进制。
  本轮的「语义矛盾日志」全部是这么来的，浪费了一整轮排查。
- **feed 是累加的**：验证「是否新增无法解析」必须重启应用开全新会话，不能在同一会话里数。

## 2026-11-21 V5 启动阻塞定位：不是毛玻璃，是产物落在 Low 完整性标签目录

### 现象
新壳启动必定崩溃，报文在 `Failed to setup app: 拒绝访问。 (os error 5)` 与
`HRESULT(0x800700AA) 请求的资源在使用中` 之间交替，位置 `tauri-2.12.7/src/app.rs:1444:11`。
此前结论「debug 必挂 / release 必成」**是错的**。

### 根因
仓库工作区 `D:\AIProject\JobAssistant` 的目录 ACL 带
`Mandatory Label\Low Mandatory Level:(OI)(CI)(NW)`，因此**在其中创建的可执行文件继承 Low 强制标签**；
Windows 会把以这类 exe 启动的进程降为 **Low 完整性级别 `S-1-16-4096`**，而 Low 进程禁止向上写
（no-write-up），于是：

- 建不了 `%LOCALAPPDATA%\com.ai.jobassistant` → WebView2 归一化成 `os error 5 拒绝访问`；
- 开不了 WebView2 环境 → `0x800700AA 请求的资源在使用中`。

两个报文都与真实病因无关，这就是它被误判成「毛玻璃 / transparent / custom-protocol」长达数轮的原因。

### 改动文件
- `app/src-tauri/src/lib.rs`
  - 新增 `probe_writable()`：在 WebView2 之前用 `File::create` 探针抓出「目录不可写」，不再等 WebView2 吐误导报文。
  - 新增 `not_writable_hint()`：把 Low IL 病因 + 处置办法（`CARGO_TARGET_DIR` 放到工作区外）打印出来，退出码 2。
  - `kill_stale_webviews()` 取代 `taskkill /F /IM msedgewebview2.exe`：只杀命令行里带本项目数据目录的进程，不再误杀 Windows Widgets 的 WebView2。
  - `acquire_instance_lock()`：`ERROR_ACCESS_DENIED(5)` 与 `ERROR_ALREADY_EXISTS(183)` 分开报，前者提示「另一个实例正以更高权限运行」。
  - 新增 `mod startup_tests`（2 个用例）：诊断报文必须含病因关键词与可执行处置；探针对缺失目录必须报错。
- `AGENTS.md`：第 4 节槽位 A 补 `CARGO_TARGET_DIR` 的第二个理由 + 槽位 C 冒烟必须跑工作区外产物；槽位 B 壳用例数 14→16；第 6 节第 1 项标记关闭并写清实证与推翻的旧结论。

### 验证
- 同一个 `whoami.exe`：拷进工作区 → `whoami /groups` 报 `S-1-16-4096`；拷到 `D:\jt` → `S-1-16-8192`。
- 同一个 release exe 在工作区内 4 个不同目录（`app\src-tauri\target\debug`、`.tmp-control`、工作区根、新建子目录）全部失败；换到 `D:\jt\release`、`C:\Users\<user>\aja-test` 全部成功。
- 同一份 debug 源码编到 `D:\jt\debug` 启动成功：窗口标题 `AI 求职助手`、hwnd 有效、6 个 `msedgewebview2` 子进程挂在 `com.ai.jobassistant` 下、`[startup] 大模型已配置`、`[startup] 数据库已打开`。
- 工作区内构建启动 → 退出码 2 + 完整中文诊断（不再 panic）。
- 门禁：`cd app/src-tauri && cargo test --lib` → 16 passed；`cd core && cargo test` → 187 passed（需 `$env:TEMP` 指向仓库内临时目录）；`cd skeleton-web && npm run build` 通过。

### 踩坑与待办
- **`$env:CARGO_TARGET_DIR = "D:\jt"` 从此是硬要求**，不只是 MSVC 长路径问题：产物留在工作区内就跑不起来。
- 之前 `os error 5` 在 `%LOCALAPPDATA%\Temp` 下让 `infra::sqlite` 两个临时库测试红，也是同一根因（Low IL 写不了 Medium 的 Temp），当时靠 `$env:TEMP` 指向仓库内规避——现在知道原因了。
- 待办：登录 + 抓取 + 落库的 **GUI 点击路径**需在工作区外产物上验收一轮（交接清单第 1、2 项的未闭环部分）。

## 2026-11-21 Rust 侧接管 sidecar：进程管理 + 领域判定入库 + 前端抓取面板

### 现象
上一轮 `scraper/` 已能真实抓取，但**应用里点不到**：`core/src/sidecar/` 只有协议结构，没有进程管理与消费逻辑；Tauri 命令层与前端均无抓取入口。

### 根因
交接清单第 2 项未落地。缺三层：① core 的进程句柄与事件消费；② 「抓来的 21 字段原始数据 → 领域判定 → 落库」的缝合；③ 命令层与界面。

### 改动
- **新增 `core/src/sidecar/process.rs`**：`SidecarHandle::spawn(cfg, timeout) -> Result<(Self, Receiver<SidecarEvent>), SidecarError>`。`locate_script_dir` 先查 exe 同级 `scraper/`（打包态），再回溯仓库开发树。`pin_python` 关掉 `PYTHONIOENCODING` 之类的外部干扰，强制 `python` 解析（可用 `AJA_PYTHON` 覆盖）。`wait_ready` 只认 `ready` 事件，超时 `DEFAULT_READY_TIMEOUT`。指针放在 `Option` 里，`shutdown()` 先写 `scrape.stop` 再 kill，幂等。
- **新增 `core/src/sidecar/session.rs`**：`SidecarSession::start()` 起进程 + 起一个消费线程，`send/stop/progress/poll/shutdown` 全部无锁竞争。`consume` 循环在 `RecvTimeoutError::Timeout` 上 `continue`（而不是退出），这样 `scrape.stop` 的响应窗口不会被读超时吃掉。
- **新增 `core/src/sidecar/ingest.rs`**：`handle_job` 是**纯函数**——`ScrapedJob` → `Job` → `reject()` 十条规则 → `score_job()` → `passes_threshold()`，四路计数分开记：`inserted` / `duplicates` / `below_threshold` / `rejected`。**写库失败不计入 inserted**，只落 `last_log`，否则「界面说入库 5 条、库里 3 条」无法解释。
- **新增 `app/src-tauri/src/commands_scrape.rs`**：`scrape_start` / `scrape_stop` / `scrape_progress` / `scrape_drain` / `login_start` / `scrape_close`。`ScrapeState` 实现 `Drop` 关停子进程——否则应用退出后 python 会占着浏览器 profile。`min_score` 越界直接报错，**不静默 clamp**。
- **`app/src-tauri/src/commands_db.rs`**：`StoreState` 改为 `Mutex<Option<Arc<SqliteStore>>>`，新增 `store_handle()`。原因：入库闭包必须 `Send + 'static`，拿不到 `tauri::State`，只能把 `Arc` 克隆进消费线程。`with_store` 改为先克隆再在锁外调用——抓取期间用户打开列表不再被写入阻塞。
- **`skeleton-web/src/App.tsx`**：新增「抓取」面板（关键词 / 入库门槛 / 开始 / 停止 / 扫码登录 / 实时计数 / 事件流），`running` 为真时 800 ms 轮询 `scrape_progress` + `scrape_drain`。类型定义同批更新（AGENTS.md 3.1）。顺手给「刷新」按钮补了 `data-testid="job-refresh"`。
- **新增 `core/tests/sidecar_smoke.rs`**：默认 `#[ignore]`，`cargo test --test sidecar_smoke -- --ignored --nocapture` 显式跑。只断言「起得来 / 握手 / 关得掉」，不断言抓多少条——网络波动不该让门禁变成薛定谔的绿灯。

### 关键决策
- **判定放消费线程，不放命令线程**：`store_handle()` 在 `scrape_start` 里就被克隆出来，消费线程自己持有 `Arc<SqliteStore>`，全程不碰 `tauri::State` 的锁。
- **崩溃不自动重启**：SPEC-002 第 7 节要求「自动重启一次并保留进度」。实现上只把退出码写进 `finished_reason`（「抓取进程异常退出，退出码 N」），重启策略留给上层——重启要重置浏览器 profile 占用，那是策略不是机制，塞进 `process.rs` 会让这个文件没法纯单测。
- **协议以 Python 为准**：`sidecar.py::handle` 读参数是**平铺在同一层**（`payload.get("keywords")`），没有 `params` 包裹。SPEC-002 第 2.1 节的 `{"cmd":...,"params":{...}}` 是过期写法，以运行时真源为准，不改 Python。
- **`data_dir` 保持 `None`**：让 sidecar 用默认的 `%LOCALAPPDATA%\ai-job-assistant`，与现行版共用登录态。改到别处会让用户被迫重新扫码。

### 验证
- `cd core && cargo test --lib` → **187 passed**（需 `$env:TEMP="D:\AIProject\JobAssistant\.tmp-test"`，见下条坑）。
- `cd app/src-tauri && cargo check` → Finished，无警告；`cargo test --lib` → **14 passed**（含 4 个新增抓取命令测试）。
- `cd skeleton-web && npm run build` → 30 modules，237.02 kB。
- 冒烟：`ready` 握手 286 ms，`shutdown` 50 ms，`finished_reason = "已按用户请求停止"`，未发抓取命令时入库数 0。

### 踩过的坑
- **`std::env::temp_dir()` 建目录被拒（os error 5）**：`core` 的 `infra::sqlite` 两个测试报 `std::fs::create_dir_all(&dir)` 权限拒绝。根因是 Rust 解析到长名 `C:\Users\Administrator\AppData\Local\Temp`，而 `$env:TEMP` 是短名 `ADMINI~1`，那条路径在当前进程上下文里建不出目录。解法是**改环境变量不改测试**：`$env:TEMP = "...\.tmp-test"; $env:TMP = $env:TEMP`。
- **`Receiver<SidecarEvent>` 不是 `Sync`**：最初想 `Arc<SidecarHandle>` 共享事件流，编译报 `cannot be shared between threads safely`。改为 `spawn()` 直接把 `Receiver` 交给调用方，句柄本身不持有事件流。
- **`Job` 没有 `PartialEq`**：给 `IngestVerdict` derive `PartialEq` 触发 `E0369`。判定结果不需要比较语义，去掉即可。
- **测试闭包不能用 `Cell`/`RefCell` 捕获**：`Cell<bool>` 不是 `Sync`，必须 `Arc<Mutex<...>>` + `move`。

### 待办
- 真实抓取链路（登录 + `scrape.start` + 落库）尚未在 GUI 里点通——被第 1 项毛玻璃启动阻塞（`os error 5`）。
- E2E 方案仍未定（第 5 项）。

## 2026-11-20 sidecar 真实抓取打通：平移 Boss 反爬逻辑 + 登录流程 + 修半成品推送

### 现象
重写版 `scraper/sidecar.py` 只是 98 行桩，只能返回一条假的 `skeleton-001`。应用里没有任何真实抓取能力。

### 根因
`backend/scrapers/boss.py`（855 行）的反爬逻辑从未平移。且原实现把「打分、入库、早停」耦合在抓取器内部（`self.arrived` / `target_count` / `on_job` 直接落库），无法直接复用。

### 改动
- 新增 `scraper/base.py`：只留 `ScrapeConfig` + `split_keywords`。**刻意删掉** `StopFlag`（sidecar 用回调 + `threading.Event`）与 `BaseScraper`（只有一个平台，抽象层没有第二个实现证明它值得存在）。
- 新增 `scraper/browser.py`：`launch_context` / `find_page_for` / `detect_logged_in`。`detect_logged_in` **只读 cookie 不导航**——每 1.5 s 轮询一次，导航就是用户看到的「登录页一直闪烁」。登录判定用 `bst`/`zp_at`/`wt2`/`geek_zp_token`；`__zp_stoken__` 是访客态风控令牌（访问一次首页就下发），**不算登录**。
- 新增 `scraper/boss.py`：平移卡片选择器、`zpData.jobList` 接口解析、DOM 兜底、详情页解析、空页风控诊断、薪资 PUA 解码、翻页。日志改回调，停止信号改 `should_stop` / `sleep` 回调。
- 重写 `scraper/sidecar.py`：抓取与登录跑在**独立后台线程**，主线程只做 stdin 读循环。否则用户点「停止」要等整页抓完才生效。`interruptible_sleep` 每 0.2 s 检查停止位。
- `core/src/sidecar/protocol.rs`：`job.scraped` 由 3 字段扩到 21 字段（新增 `pub struct ScrapedJob` + `from_value` 逐字段取值以忽略未知字段）；新增 `login.state` 事件。

### 修掉的真实缺陷
`_fetch_detail` 在检测到停止信号时只 `return`，而调用方不检查返回值，仍会 `on_job(item)` 推送一条 **JD 为 0 字** 的半成品。实测复现：`JOB #2 'Java-保险经验' | JD 0 字 | HR活跃 None`。

改为 `_fetch_detail` 返回 `bool`（是否取到 JD），调用方未取到就不推送。修复后连抓 5 条，JD 全部非空（107–859 字），19 个字段零缺失。

### 验证
- `python -m compileall -q scraper` 通过；`cargo test` 165 个用例全绿。
- 登录流程：`login.start` → 3.8 s 打开登录页 → 12.9 s 检测到 `logged_in`（用户扫码）。
- 真实抓取：Java 第 1 页接口未捕获时 DOM 兜底拿到 15 条；5 条详情 JD 107/246/664/778/859 字；`company_address`/`company_industry`/`company_intro`/`company_scale`/`company_stage`/`degree`/`founded_years`/`hr_active_status`/`reg_capital` 等全部有值。
- 停止：`scrape.stop` 后 0.2 s 内结束。
- 未登录保护：开抓前 `detect_logged_in` 为 false 时立刻 `log error` 返回，不再空跑几分钟。

### 待办
- **Rust 侧尚未消费 sidecar**：缺 `core/src/sidecar/` 的进程管理（spawn / 写 stdin / 按行读事件）与 `job.scraped` 落库的 Tauri 命令。落库路径已就绪：`ScrapedJob` → `Job` → `JobStore::insert_job`。
- 抓取结果须在 Rust 侧过 `score_job` + `passes_threshold`，不得在 Python 侧打分。

## 2026-11-20 工程治理补齐：AGENTS.md 宪法 + SPEC 冻结 + 清理敏感库清空残留

### 现象
`POST /api/sensitive/clear`（界面「清空敏感库」）会抛 `AttributeError: 'Store' object has no attribute '_lock'`。

### 根因
`backend/app/store.py` 早期是「单连接 + `self._lock` / `self._conn()`」实现，后来重构成每线程独立连接
（`threading.local` + `conn()` / `tx()`），`clear_sensitive()` 是唯一没跟着改的方法。

### 改动
- `backend/app/store.py`：`clear_sensitive()` 改用 `with self.tx() as c:`，与其它写方法一致。
- 新增 `AGENTS.md`（根目录工程宪法，六节：状态看板 / 技术栈 / 目录边界 / 契约同源 / 三层验证门禁 / Git 规范）。
- 新增 `docs/specs/SPEC-001-mvp.md`（状态 `SPEC_FROZEN`）与 `docs/adr/ADR-001`、`docs/adr/ADR-002`。
- `README.md`：补「会话与自动打招呼（已实现）」接口表，更正「JD 智能对话是占位」的过时描述，新增「工程约定」。

### 验证
- `python -m compileall -q app scrapers entry.py` 通过。
- `Store(tmp).clear_sensitive()` 单点验证通过（见本轮终端输出）。

### 待办
- 引入 pytest（store 去重/筛选、scorer 规则打分、薪资解析）与 vitest（`api.ts` 端口解析），回填 `AGENTS.md` 第 4 节槽位 B。
- 决定 E2E 方案后回填槽位 C。

## 2026-11-19（深夜 4）自动打招呼误报「已发满本轮上限 0 条」+ 顶栏状态高亮

### 现象
- 点「自动打招呼 开」后立刻停下，日志：「自动打招呼：已发满本轮上限 0 条，自动打招呼已停止」。
- 用户明确要求：自动打招呼只看「待投递」列表，**列表空了才停**，不该有别的上限。

### 根因
- 前端 `tick()` 里有一条 `if (autoDoneRef.current.size >= maxJdRef.current)`，
  把抓取用的「每日 JD 上限 `maxJd`」也当成了本轮发送上限；而 `max_jd` 在库里是 **0**（0 = 不限制），
  于是 `0 >= 0` 立刻成立 → 开一次就停。
- 顺带两处错：计数用的是 `autoDoneRef`（只装「跳过」的岗位，成功发送并不往里加），
  所以上限判断本来也数不准；停止时只 `setAutoOn(false)`、没同步 `autoOnRef`。

### 改动（`web/src/App.tsx`）
- 删掉上限判断：现在只有三种停止——**待投递列表发完**、连续失败 3 次自动暂停、用户手动关闭。
- 新增 `stopAuto(message, level)`：同时清 `autoOnRef`、`autoNextAt` 和 `autoOn`，
  所有停止分支（取不到岗位 / 列表发完）统一走它，避免定时器拿旧 ref 再排一轮。
- 新增倒计时显示：`autoNextAt` + 每秒跳动的 `autoNow`，算出 `autoNextIn`。
- 顶栏「自动打招呼」按钮：运行中是**实心绿**，暂停是**实心琥珀**；旁边多一个状态胶囊
  （绿色呼吸点 +「12s 后发下一条」/「发送中…」/「已暂停」+「· 已发 N 条」），
  悬停看明细。暂停时点「继续」会清零连败计数。
- 日志里不再出现「上限」字样，开启时提示「按待投递列表逐条发，发完自动停止」。

### 验证
- `npm run build` 通过（`index-CxqdvYGf.js` 296.88 kB）；`cargo tauri build` 通过。
- 桌面 exe 已换新（71,005,184 字节，md5 `2a59fa74672ac8657d344f3668395568`）；
  `/api/auto-greet/next` 返回 11 条待投递，重启后循环能正常跑起来。

## 2026-11-19（深夜 3）关掉 Chrome 窗口后消息/打招呼不再报错：自动重新打开浏览器

### 现象
用户：「自动回复 窗口关闭后不能继续」。手动关掉采集用的 Chrome 窗口后，消息列表、打招呼一直报
「浏览器窗口已关闭或崩溃，下次操作会自动重新打开」，但**从来没有真的重新打开过**，
自动打招呼连败 3 次就自己暂停了。

### 根因
- `Runtime.ensure_browser()` 里其实有探活逻辑（`ctx.pages` 抛错 → 重启 Chrome），
  但**只有走采集线程的路径才会调用它**。
- 消息走 `ChatLane.ensure_ctx()`：它自己 `connect_over_cdp(端口)`，浏览器没了就连不上；
  原来这里直接 `raise`，从不请求采集线程重新拉起浏览器。
- `_run_chat()` 的前置检查是 `if rt._ctx is None` 才 `ensure_main_browser()`；
  窗口被关掉后 `_ctx` 仍是**过期但不为 None**，检查被跳过 → 永远没有重启的机会。
- 因此那句「下次操作会自动重新打开」是空头承诺。

### 改动（`backend/app/main.py` 的 `ChatLane`）
- `ensure_ctx()`：连不上时不再直接抛，改成最多两轮——第一轮失败就把浏览器拉起来再连一次。
- 新增 `ChatLane._relaunch(reason)`：`time.sleep(0.8)` 等旧进程让出调试端口 →
  `rt.run_in_worker(rt.ensure_browser, timeout=150)` 让**采集线程**重新启动 Chrome →
  记一条「浏览器已重新打开，消息车道正在重连」→ `time.sleep(1.2)` 等端口监听 → 再连。
  失败时抛「Chrome 窗口已关闭，自动重新打开失败：…」，前端能看到真正的原因。
- 浏览器没被关、只是消息车道自己断了的情况仍走原来的 `_drop_locked()` 重连逻辑，不受影响。

### 验证
- 真机（开发态后端 + 真实 Chrome，2026-11-19 19:0x）：
  1. 正常打开浏览器 → `消息车道已连上浏览器（端口 1273）`；
  2. 强杀 Chrome 进程模拟「关掉窗口」→ 下一次消息请求报
     「浏览器窗口已关闭或崩溃，下次操作会自动重新打开」；
  3. 再下一次请求触发重开，日志依次出现
     「浏览器不可用（窗口已关闭或浏览器已退出），正在自动重新打开」
     →「启动 patchright（持久化目录 …）」
     →「浏览器已重新打开，消息车道正在重连」
     →「消息车道已连上浏览器（端口 1273）」，请求恢复正常（耗时约 30 秒）。
- 顺带修掉两个让「自动重开」失效的坑：
  - `ensure_browser()` 原来只靠 `ctx.pages` 探活，Chrome 被关掉后它**仍返回旧列表**，
    于是把已经死掉的上下文当活的用。现在再加一次 `ctx.browser.is_connected()` 判断。
  - `ChatLane._drop_locked()` 原来每次都 `pw.stop()`，同一线程里再
    `sync_playwright().start()` 会被 patchright 误判成「在 asyncio 循环里用同步 API」
    （日志原样报这句），消息车道从此全废。现在只丢浏览器/上下文、保留驱动，
    确实连不上时（第 3 次尝试）才重建驱动；重开浏览器走新加的 `Runtime.restart_browser()`。

---

## 2026-11-19（深夜 2）配置与筛选条件没回显：配置只在启动读一次 + 筛选条件只写不读

### 现象
用户：「配置信息没有回显」「也没有记住我的筛选条件」。重启程序后配置抽屉里的 Base URL / 模型 /
提示词 / 技能看着像没保存，筛选面板（关键词、城市、薪资…）也回到默认值。

### 根因（两个独立问题）
1. **配置读取失败被静默吞掉**：`web/src/App.tsx` 启动时 `.getConfig().catch(() => {})` 只请求一次。
   Tauri 先建窗口、sidecar 后端稍后就绪，第一次请求经常直接连接失败 → 配置永远停在默认值，
   而数据库里其实是有值的（`/api/config` 本身返回是完整的）。没有任何重试，界面就一直「空着」。
2. **筛选条件只写不读**：`POST /api/start` 里 `store.set_config_json("last_filter", req.model_dump())`
   存了，但全仓库没有任何地方读回来（`grep -rn last_filter` 只有这一行 + pyc），
   `/api/config` 也不返回它 → 前端无从还原。

### 改动
- `backend/app/main.py` `get_config()`：返回值增加 `"last_filter": rt.store.get_config_json("last_filter", None)`。
- `web/src/api.ts`：`Config` 增加 `last_filter?: Record<string, any> | null`。
- `web/src/App.tsx`：
  - 启动配置加载改成 `loadConfig()`，失败每 1.5 秒重试（最多 60 次），成功才把值灌进 state；
    卸载时清理定时器。
  - 新增 `applyLastFilter(lf)`：把 `last_filter` 里的 keywords / platforms / city / salary /
    experience / degree / scale / stage / industry / job_type / max_pages / exclude_keywords
    还原到筛选面板（数组字段先做类型过滤，空数组不覆盖）。
  - 配置加载成功后调 `applyLastFilter(c.last_filter)`；effect 依赖补上 `applyLastFilter`。

### 验证
- `cd web && npm run build` 通过（`dist/assets/index-a3c-Pge9.js` 294.46 kB）。
- 库里确实有这套条件：`last_filter` = platforms=boss、keywords=[全栈工程师, Node.js, …]、
  city=101280600、salary=[sal:15-30]、experience=[106,107]、scale=[303,302]、max_pages=3。

---

---

## 2026-11-19（深夜）采集与消息并行：消息走独立车道 · 打招呼提示进日志

### 需求
用户：「消息列表和抓取 jd 不能同时运行吗」「采集、主程序、自动打招呼、自己打招呼各自使用不同的线程，互不影响」，
以及「自动打招呼的提示、错误放日志里面，现在的位置会破坏布局」。

### 根因
所有碰浏览器的动作都挤在采集线程：`run_scrape` 一次调用抓完所有页，期间消息请求只能在队列里排队，
等满 90~120 秒后报错。旧代码还会误报「Boss 消息页没有响应」——看门狗读的是 `BossChat` 的全局
`_last_act`，上次成功开过消息页就会留下一个非 0 的旧时间戳。

### 方案（同一台 Chrome，两个 Playwright 客户端）
Playwright 允许**多个客户端连同一台浏览器**（`connect_over_cdp`），于是：
- 采集线程：照旧启动浏览器（`launch_persistent_context`）并跑抓取；
- 新增 `ChatLane`（`backend/app/main.py`，线程名 `aja-chat`）：自己的 `sync_playwright().start()` +
  `connect_over_cdp("http://127.0.0.1:{cdp_port}")`，连的是同一个持久化上下文，只驱动自己的标签页。
- 端口：`Runtime.cdp_port = _free_port()`，`launch_context(..., cdp_port=...)`
  （`backend/app/browser.py`）把 `--remote-debugging-port=N` 加进 Chrome 启动参数。
- 不新开浏览器 → 指纹、登录态、cookie 都不用搬，Boss 也看不出多了一个浏览器。

### 改动
- `_run_chat()`：任务投递到 `rt.chat_lane`；看门狗读 `chat_lane.worker_busy()/worker_task()`；
  只有 `submitted` 之后刷新的 `activity()` 才算本任务自己的进展；放弃时置 `state["abort"]`，
  排到队的 `_wrap` 直接跳过，避免前端 3 秒轮询在事后堆出一串重复浏览器操作。
- 消息类接口里的 `rt.ensure_browser()` 全改成 `rt.chat_lane.ensure_ctx()`（登录、登录态检查仍走采集线程）。
- `ChatLane.ensure_ctx()` 连接失败不会去杀采集用的浏览器；`reset()` 只 `pw.stop()` 断开自己的 CDP。
- 拉会话列表/聊天记录失败时用 `BossChat.cached_friends()/cached_messages()` 兜底，返回 `stale: true`。
- 前端 `web/src/App.tsx`：自动打招呼的提示/错误改走 `pushLog()` 写进日志面板（前缀 `[打招呼]`，
  warn/error 会染色）；顶栏那行状态文字删掉（它 `shrink-0` + 长句会把按钮挤变形），已发/失败计数挪进按钮 `title`。

### 验证
- 临时脚本：采集线程被 60 秒假任务占住时，消息车道仍能在 **0.0 秒**内完成 `goto + title`（并行成立）。
- 开发态起服务：`/api/health` 正常；`/api/chat/friends` 在未登录的 profile 下返回可读错误而不是卡死。
- **打包版真机并行验证**（2026-11-19 18:15，抓取「后端开发/Java」正在进行中）：
  `GET /api/chat/friends` 在 **2.5 秒**内返回真实会话列表，同时 `/api/status` 仍 `running:true`，
  采集日志照常推进；`backend.log` 记 `消息车道已连上浏览器（端口 5447），采集和消息互不阻塞`。
  同一时刻两个车道各自操作不同标签页，抓取节奏没被打乱。
- `npm run build` 通过。

### 注意
- 打包后 patchright 的 driver 仍要可用（采集线程本来就在用 `sync_playwright().start()`），消息车道只是多起一个客户端。
- 浏览器重启后端口不变（`cdp_port` 进程内固定），`ensure_ctx()` 会自动重连。

---

## 2026-11-19（晚）「生成回复」报 `大模型返回了空内容`（推理模型的思考吃光输出预算）

### 现象
点「生成回复」后提示：`大模型生成失败（ValueError: 大模型返回了空内容），已用规则话术代替`。

### 根因
用户配的模型是 **`deepseek-flash`，属于推理型模型**：它先写 `reasoning_content`（思维链），
再写 `content`，而**思考 token 也算在 `max_tokens` 里**。

`greeting.py` 里打招呼语只给了 `max_tokens=400`。实测同一份 prompt：

| max_tokens | finish_reason | reasoning_tokens | content 长度 |
|---|---|---|---|
| 400 | `length` | 400 | **0** ← 报错现场 |
| 800 | `stop` | 694 | 191 |
| 1500 | `stop` | 542 | 261 |

思考把 400 全吃光 → `choices[0].message.content` 为空 → 抛 `ValueError("大模型返回了空内容")` → 回退规则话术。

### 本轮改动
- `backend/app/greeting.py`（`GreetingWriter.write`）：输出预算 400 → **1200**，
  被截断（`finish_reason=length` 且正文为空）时自动再试一次 **3000**；
  空了也不再只说「返回了空内容」，而是带上 finish 与思考 token 数（新增 `empty_content_reason()`）：
  「输出预算被思考过程耗尽，其中思考约 N tokens；可在设置里换非推理模型，或调大最大输出」。
- `backend/app/scorer.py`（`LLMScorer.score`）：同理会话预算 800 → **1600**，被截断再试 3200；
  解析失败信息里把 `finish` 和空内容显式打出来。

### 验证
用库里真实的 `api_key / base_url / model`（`%LOCALAPPDATA%\ai-job-assistant\data\job_assistant_pro.db`
的 `app_config` 表）直接调接口：
- 修复前 `GreetingWriter.write()` → `source=rule`，note 正是那句报错（复现成功）；
- 修复后连续两次 → `source=llm`，tokens 1027 / 1508，正文 200+ 字，`note=''`；
- `LLMScorer.score()` → `score=8, source=llm, tokens=604`，JSON 解析正常。

### 经验
推理型模型（`deepseek-flash` / `deepseek-reasoner` / 各家 thinking 模型）不能按「答案长度」给
`max_tokens`，要按「思考 + 答案」给；判断依据看 `usage.completion_tokens_details.reasoning_tokens`。

---

## 2026-11-19（更晚）Boss 登录报 `Cannot switch to a different thread`（greenlet 回归）

### 现象
点「登录 Boss」后前端报：

```
Cannot switch to a different thread
Current:  <greenlet.greenlet object ... current active started main>
Expected: <greenlet.greenlet object ... suspended active started main>
```

### 根因
上一轮 commit `7ec4585`（「采集线程改常驻 greenlet」）把 `main.py` 里登录会话的注入改成了直连：

```python
get_context=rt.ensure_browser,
start_browser=rt.ensure_browser,
step_runner=lambda fn, *a: fn(*a),   # 等于没投递
```

当时的注释断言「登录会话本身就跑在采集线程里（`rt.submit(sess.start)`）」，**这个前提是错的**：
`LoginSession.start()`（`backend/app/browser.py`）只是
`threading.Thread(target=self._run, daemon=True).start()`，跑在独立的登录调度线程上。
patchright 的 sync API 绑定在 `sync_playwright().start()` 所在线程（采集线程）的 greenlet 上，
在登录线程里直接调用就会被 greenlet 拒绝跨线程 switch。

另有一处漏网：`_steps()` 开头的 `ctx = self._open_browser()` 是**绕过 step_runner 的直连调用**，
即使 step_runner 修好，「拿上下文」这一步仍会在登录线程里启动浏览器。

### 本轮改动
- `backend/app/main.py`（`login_start`）：改回
  `get_context=None, start_browser=rt.ensure_browser, step_runner=rt.run_in_worker`，
  注释改写为「`sess.start()` 只是再起一个登录调度线程，每一步碰浏览器的动作都必须显式投递到采集线程」。
- `backend/app/browser.py`（`_steps()` 开头）：`ctx = self._open_browser()` → `ctx = yield (self._open_browser,)`。
- `backend/app/main.py`（`Runtime.run_in_worker`）：补 `_wrap.__wrapped_label__`，
  否则采集线程日志只会刷「开始执行：_wrap」，看不到具体步骤名。

> 注意：`start_browser` 要传**裸的** `rt.ensure_browser`，不能传 `rt.ensure_browser_on_worker` --
> 后者内部自己就投递一次，再被 step_runner 包一层会在采集线程里二次投递，抛
> 「内部错误：run_in_worker 不能在采集线程里再投递任务」。

### 验证
源码后端（`AJA_PORT=8791 python -m app.main`）：`POST /api/login/start {"platform":"boss"}` →
12 秒后 `GET /api/login/status` 返回 `{"state":"logged_in","message":"登录成功"}`，
`data/logs/backend.log` 时间线正常（开始 → 启动 patchright → 登录窗口已打开 → 登录成功），无 greenlet 报错。

### 待办
- 已确认除登录外的其他调用点（`run_scrape`、各 `_do/_find/_new/_run/_start`）本来就走
  `rt.submit(...)` / `_run_chat(...)` 投递到采集线程，无需改动。
- 可考虑在 login 的 step_runner 处加断言/注释防回归。

---

## 2026-11-19（晚）「把 exe 放到别的文件夹打开会闪退」排查与修复

### 现象
把 `src-tauri\target\release\ai-job-assistant.exe` 复制到别的文件夹双击，窗口一闪就没了；
放在原地双击也一样。`crash.log` 里只有「主进程启动」，没有任何退出原因。

### 根因（已复现，证据在 crash.log）
用临时 build 打出 panic 详情，实际是 **WebView2 创建失败**：

```
Failed to setup app: runtime error: failed to create webview:
WebView2 error: WindowsError(Error { code: HRESULT(0x800700AA), message: "请求的资源在使用中。" })
```

→ 上一轮 `ai-job-assistant.exe` 被异常结束（panic abort / 任务管理器结束）后，
它拉起的 `msedgewebview2.exe` 变成孤儿，继续占着 WebView2 的用户数据目录，
新进程建 webview 时直接 `E_ACCESSDENIED`，panic 后整个进程消失。
**和 exe 放在哪个文件夹无关**——换个目录只是让人误以为是路径问题。

排查中另外发现三个放大器：
1. panic hook 里又调了 `default_hook` → 二次 panic 输出会 `process::abort()`（退出码 -1073740791），
   日志根本来不及落盘；
2. 单实例锁用的是 `app_local_data_dir()`（Tauri 下其实是 `%APPDATA%\com.ai.jobassistant`），
   不是 `main.rs` 释放后端用的 `%LOCALAPPDATA%\ai-job-assistant`，两边目录对不上，锁形同虚设；
3. 锁抢不到时 `exit(0)`，和「正常退出」完全无法区分。

### 本轮改动（`src-tauri/src/lib.rs`、`commands.rs`）
- **启动前置检查**（`run()` 里，Builder 建 WebView **之前**）：
  `prepare_webview_dir()` 先抢单实例锁（固定路径 `%LOCALAPPDATA%\ai-job-assistant\app.lock`），
  抢不到 → `exit(2)`；抢到后 `taskkill /F /IM msedgewebview2.exe` 清残留、
  预建 `EBWebView` 目录，日志记下 WebView2 用户数据目录。
- **按路径杀残留**：`kill_pids_under(%LOCALAPPDATA%\ai-job-assistant)`，只结束从释放目录起的实例，
  不影响从 `target/` 打开的调试实例。
- **panic hook 不再转发 `default_hook`**：panic 变成普通 unwind，稳定拿到退出码，不再 abort。
- **WebView2 失败时写人话**：`crash.log` 追加 `[fatal]` + 三步处理指引
  （结束 msedgewebview2 → 删 `EBWebView` → 反馈日志）。
- 新命令 `get_crash_log_path`（Rust + `generate_handler!`），给界面一个日志入口。

### 验证
- `cargo tauri build` 通过（`cargo build` 在受限沙箱里被 `target/.cargo-build-lock`
  挡住过一次，切到完全权限后按 `build.bat` 的步骤正常构建成功）。
  产物：`src-tauri\target\release\ai-job-assistant.exe`（70 996 992 字节）、
  `bundle\nsis\AI Job Assistant_0.1.0_x64-setup.exe`（64.10 MiB）。
- 桌面实测（已复制到 `C:\Users\Administrator\Desktop\ai-job-assistant.exe`，md5 与构建产物一致）：
  1. **从桌面启动正常**：`crash.log` 依次出现「已清理残留的 msedgewebview2.exe」
     →「WebView2 用户数据目录: …\EBWebView」→「后端已就绪，端口 2628」，窗口正常，进程常驻；
  2. **多开被挡**：第二个实例 `exit=2`，日志写明「另一个实例正在运行（锁 …app.lock 被占用）」；
  3. **把 exe 复制到另一个文件夹（`data\.smoketest\`）启动正常**，端口 13356；
  4. **强杀后再启动能自愈**：任务管理器结束进程后重开，
     日志出现「已结束残留进程 pid=19836,20180」+「已清理残留的 msedgewebview2.exe」，随后正常起后端。
- 也就是当初「换文件夹就闪退」的场景已经走通，不再复现。

### 待办
- 若 `[fatal]` 指向 `EBWebView` 目录权限，考虑给 WebView2 换一个可控的 user data 目录。
- 前端「崩溃日志」入口还没做界面按钮，只加好了 Rust 命令。

## 2026-11-19（傍晚）Boss 主动发起会话 · 采集线程与事件循环大修 · 自动打招呼实跑

### 本轮做了什么
- **主动发起会话**：`BossChat.start_conversation()` 走「岗位详情页 → `a.btn-startchat`（立即沟通）→ 跳聊天页 → 读选中的 HR → 刷新列表 → 再点开」。返回 `{ok, uid, friend, created}`，前端「打招呼」按钮（`openJobChat(j, true)`）和 `POST /api/auto-greet/one` 都走这条路，用户不需要先在 Boss 上手点。
- **uid 来源**：会话行 DOM 上**没有任何 uid 属性**，`data-v-*` 只是 Vue 作用域标记。改成从 Vue 实例链取：`__vueParentComponent` / `__vue__.$parent` 的 `props.friend/item/data/info` 里读 `{name, uid}`（`_selected_uid()`）。
- **自动打招呼实跑通过**：真实账号上连发两条（明岩 8.3s、邝女士 9.0s/1095 token），随后自动建立下一个岗位的会话。顶栏开关 → 逐条取待投递 → 生成话术 → 发送 → 标记已投递 → 20s±5s 再来。
- **token 三项**：右上角「累计 / 模型打分 / 打招呼」；模型配置小窗可编辑提示词与技能。

### 踩过的坑（全部是真 bug，按严重程度排）
1. **`page.on("response")` 回调里调 `resp.text()` 会把 driver 连接搞断**。报 `Connection closed while reading from the driver`，之后所有操作变成 `Event loop is closed` / `Playwright Sync API inside the asyncio loop`，「浏览器卡住」误报不断。改法：监听器只把 resp 塞进队列（`self._signals`），正文在主流程里 `_drain()` 读。
2. **每个任务起一个新 greenlet 会让已启动的浏览器失效**。改成一个常驻 greenlet 循环取任务；`run_in_worker` 加自检，禁止在采集线程里再投递。
3. **`_poll` / `_wait_items` 不打活动时间戳**，30 秒活动上限把「正常但慢」的流程判死 → 触发浏览器重建 → driver 断开。现在每次轮询都 `_touch()`。
4. **`_chat_page` 缓存页停在岗位详情页**：`start_conversation` 点完「立即沟通」后页面已不是 `/web/geek/chat`，但仍被当缓存页复用 → 监听器挂不上 → 27 秒超时。现在发现路径不符就重新导航。
5. **`friends(force=True)` 的 reload 会把刚选中的会话清掉**，导致「会话建好了但界面空白」。现在先读 uid 再刷新列表再点开。
6. **单实例锁的 File 是局部变量**，`setup` 返回就 drop，锁实际没生效。改成放进 managed state。
7. **`_run_chat` 的固定总超时**会把长流程误判成「可能需要重新登录」。改成默认 120 秒，只要还在推进就顺延一次。
8. **进程被外部关掉浏览器后，后端会一直报「事件循环已关闭」**。新增 `Runtime.reset_browser()`，遇到 `Event loop is closed` / `Target closed` / `Playwright already stopped` 就丢掉上下文，下次操作自动重开。

### 待办
- 打包版启动约 20 秒后主进程会自行退出，暂未查明（已确认不是多开锁：`crash.log` 里没有「禁止多开」记录）。下一步在 `run()` 的 `run(...)` 返回值里加退出原因记录。
- 自动打招呼的「每日投递上限」目前沿用抓取上限，是否单独配置待定。

## 2026-11-19（下午·七次）「打招呼」= 主动发起会话

用户反馈：「我点击打招呼按钮，说明我正在主动发起请求，这时候应该新建一个与该 hr 的会话」，
以及「自动打招呼也是如此，根据 jd，主动发起会话」「跳转到聊天页面并新建会话」。

- 以前的行为：只去 Boss 会话列表里找，找不到就提示「右侧已列出全部 HR，挑一位」。
  但 Boss 只有在**打过招呼之后**才会为该岗位建会话，所以没聊过的岗位永远找不到。
- 实测可用入口：岗位详情页 `https://www.zhipin.com/job_detail/{encryptJobId}.html` 上的
  `a.btn-startchat`（文案「立即沟通」）。点它会跳进 `/web/geek/chat` 并自动选中该 HR，
  会话列表里立刻出现这条会话——**只建会话，不需要真的发消息**。
- `backend/scrapers/boss_chat.py` 新增 `start_conversation(ctx, friend, encrypt_job_id)`：
  1. 会话列表里已经有 → 直接 `_open_conversation`，返回 `created=False`；
  2. 否则 goto 岗位详情页 → 等 `a.btn-startchat` 可见 → 点击 → 等进入 `/web/geek/chat`；
  3. **先读下 `.chat-conversation .name-text`**（刷新列表会把它清掉），再 `friends(force=True)`
     把这位 HR 的真实条目找回来；
  4. 按 uid → 按名字 → 按 DOM 里刚建出来的行 依次兜底，最后重新 `_open_conversation`，
     保证右侧聊天框真的打开了。
  踩过的坑：`self._friends` 是 **list** 不是 dict（写成 `.values()` 直接 AttributeError）；
  `friends(force=True)` 会 reload 聊天页，把「立即沟通」刚选中的会话清空。
- `backend/app/main.py`：
  - `ChatOpenReq` 增加 `force_new: bool = False`；`/api/chat/open` 在 `force_new` 且带
    `encrypt_job_id` 时走 `start_conversation`，返回多一个 `created` 字段。
  - `POST /api/auto-greet/one`：找不到会话时不再挑「同公司唯一 HR」凑数，而是直接
    `start_conversation` 主动建会话，然后照样「生成话术 → 发送」，顺序是
    **先开会话 → 生成话术 → 发送**。
- `web/src/api.ts`：`chatOpen` 参数加 `force_new`，返回类型加 `created`。
- `web/src/App.tsx`：`openJobChat(j, forceNew = false)`；卡片展开仍只用已有会话（不打扰 Boss），
  只有「打招呼」按钮调 `openJobChat(j, true)`；建会话成功后提示
  「已向 XX 的 HR 发起沟通，右侧可以直接发消息」。顺手修掉把 `j.hr_name`（HR 名字）
  当 `uid` 传的 bug。
- 验证（dev 副本 profile，不影响用户真实数据）：`start_conversation` 返回
  `created=True uid=40761977 name=刘女士`，页面 URL 为 `/web/geek/chat`，
  `.chat-conversation .name-text` = 「刘女士」，会话区有消息卡片，输入区可用。

---
## 2026-11-19（下午·六次）大模型打分截断 + Boss 薪资反爬解码

### 1. 「第 1 页解析出 9 条却没入库、敏感库也没有」——数据其实都入库了
查库：`data/job_assistant_pro.db` 的 `job_queue` 20 条、`sensitive_jobs` 42 条，
最近几条（珠海脉冲计算、瓦石能源、深圳鼎驰）都在。用户当时看不到，是因为同一轮里
大模型打分抛异常退回了规则分，界面刷新时机也错过去了。**顺带发现了下面这个真 bug。**

### 2. 大模型打分一直静默失败（真 bug，影响所有大模型模式下的打分）
- 现象：库里 3 条 `score_reason` 是 `[大模型失败:ValueError] …`。
- 根因（两个叠在一起）：
  1. `LLMScorer.score` 里 `max_tokens=300` 太小。`deepseek-flash` 的 `reason` 是中文长句，
     300 token 只够 150 来个汉字，输出被截断，`finish_reason="length"`。
  2. `parse_llm_score` 只认**完整闭合**的 JSON，一截断就返回 `None` → 抛 ValueError → 回退规则分。
- 改法：
  - `max_tokens=300 → 800`。
  - 新增 `_score_from_obj(obj)` 与 `_score_from_partial(text)`：完整 JSON 走前者；
    被截断时用正则 `"score"\s*[:：]\s*(\d{1,2})` 从残缺文本里抠分，`reason` 能抠多少算多少。
  - 截断时在 reason 末尾标注「（输出被截断，理由可能不完整）」，不再谎报「无法解析」。
- 验证：同一个库里 3 条任务实跑大模型后返回真实分数 6 / 5 / 4（tokens 1284 / 1621 / 1319）。

### 3. Boss 薪资反爬字符解码（薪资二次校验以前完全失效）
- 实测：列表卡片 `span.job-salary` 的 innerText 是私用区字符，码点 `U+E032→"1"`、`U+E036→"5"`，
  即**码点 - 0xE031 = 数字**。例：`\ue032\ue037-\ue032\ue038K` = `16-17K`。
- `backend/app/scorer.py` 新增 `decode_boss_salary(text)`（`_PUA_BASE = 0xE031`，其它私用区字符丢弃），
  `RuleScorer.parse_salary` 入口先解码。以前 `SALARY_RE` 永远匹配不到加密文本 → 返回 None →
  `salary_out_of_range` 一律放行，薪资区间等于没生效；现在真正会拦。
- `RuleScorer._salary` 修 9999 上限溢出：先把 `low/high` 夹到画像上限（`PROFILE["salary_high"]`）
  再算 `hi - lo`，避免「15-9999K」被当成满分区间的正中拿满分。
- `backend/scrapers/boss.py`：`_parse_dom_cards` 的 `salary` 过 `decode_boss_salary`；
  `_parse_detail_dom` 的薪资选择器收窄为 `.job-banner .info-primary .salary` /
  `.job-banner .salary` / `.info-primary .salary`，兜底才用 `.job-banner .salary`——
  注意裸 `.salary` 会命中页面里全部推荐岗位（实测 37 个），绝不能直接用。
- 验证：真实搜索页 15 张卡片全部解出正确薪资（`25-35K`、`8-9K`、`18-28K`、`15-18K·13薪`），空值 0；
  `8-9K` / `6-10K` 在 `15-35` 区间下被正确排除，`16-17K` 在 `20-30` 下被正确排除。

### 待办
1. 自动打招呼真实环境验证（会真发消息给 HR）。
2. 已有 3 条 `[大模型失败:ValueError]` 的旧记录不会自动重算，需要的话点「重新打分」。

---
## 2026-11-19（下午·五次）打分开关进顶栏 + token 三项统计

用户需求：
- 「大模型打分和规则打分 放在外边，铭感库的左边，不是在铭感库的弹窗页面出现」
- 「累计 token 分模型打分和打招呼的消耗，共计 3 个显示」

- `web/src/App.tsx`
  - 顶部工具栏原来那个只读小标签换成可点的分段开关「大模型打分 | 规则打分」（位置：招聘网站之后、「敏感库」之前），切到「大模型打分」时右侧出现「初筛≥ [5] 分」输入框；敏感库弹窗里那份开关与初筛输入已删除。
  - 右上角 token 改成三项：`累计 N token | 模型打分 N | 打招呼 N`；配置小窗里也同步显示三项。
- `backend/app/main.py`
  - `Runtime` 新增 `self.tokens_score`（读 `app_config.tokens_score`）；老数据没有该键时按 `tokens_total - tokens_greet` 回填。
  - `add_tokens(n, kind)` 现在 `score` 与 `greet` 分别累计并各自持久化；`/api/config`、`/api/status`、`/api/auto-greet/next`、`/api/greet` 的返回体都带上 `tokens_score`。
- `web/src/api.ts`：`Config` / `GreetResp` / `AutoGreetJob` 补 `tokens_score`。

回答用户提问「现在可以边打招呼边抓取数据吗」：可以，但**不能真并行**。抓取与消息操作共用同一个采集线程（patchright 页面必须留在该线程），只能排队；`_run_chat` 的看门狗会识别「浏览器正被「抓取…」占用」，只在超过 90 秒时提示，不会误报浏览器卡死。

---
## 2026-11-19（下午·四次）大模型打分前的规则初筛

用户需求：「大模型打分前先用规则做一次初步筛选，然后调用大模型结合 jd 详情和自己的技能打分」「规则给分和大模型给分只生效一个」。

- `backend/app/scorer.py`
  - 新增 `Scorer.llm_prefilter`（默认 5 分，0=不设初筛）。
  - `Scorer.score()` 流程：先跑 `_rule_score()`；
    - 规则模式 → 直接返回规则分；
    - 大模型模式 → 规则分 < `llm_prefilter` 时直接判掉并注明「[规则初筛 N 分 < 5，未调用大模型]」，**不花 token**；过了初筛才调 `llm.score(job, profile, filters)`，并且**最终只采用大模型的分**（两种给分不叠加、不混用）；大模型异常时才退回规则分。
- `backend/app/main.py`
  - `_build_scorer()` 从 `app_config.llm_prefilter` 读初筛线；`/api/config` 暴露并可保存 `llm_prefilter`（0~10 夹取）。
- `web/src/App.tsx` / `api.ts`
  - 敏感库浮层的打分开关旁，切到「大模型打分」时出现「初筛≥ [5] 分」输入框，失焦即保存。

已验证（fake LLM 单测）：好岗位 → `9 llm`（调用 1 次）；差岗位 → `4 rule` 且不调用大模型；规则模式 → `8 rule` 且不调用。

---
## 2026-11-19（下午·三次）打招呼兜底 + 打分方式开关

用户需求：
- 「打招呼时提示：还没有和这个hr聊过，修复它，点击打招呼生成直接与hr沟通对话框」
- 「敏感库列表左边加上一个 大模型打分|规则打分 的开关 / 使用大模型打分时，获取jd后直接使用简历+配置技能+过滤条件和jd匹配打分」「默认使用规则打分」

- `backend/app/main.py`
  - `/api/chat/open` 不再返回「还没聊过」：先按 `encrypt_job_id` 找会话，找不到再按公司名筛 HR；只有 1 位就直接返回该会话（带 `fallback: "brand"`），多位则返回 `candidates` 列表让界面挑。
  - `/api/auto-greet/one` 同样加了「同公司唯一 HR」兜底，否则 `skip` 掉并说明原因。
  - `Runtime._build_scorer()` 读 `score_mode`（`rule`/`llm`，默认 `rule`），把 `use_llm`、`profile`（含「配置→技能」）、`filters`（薪资区间 + 分值门槛 + 排除关键词/公司拼成一句话）都灌进 `Scorer`。
  - 新增 `Runtime._filter_text()`；`/api/config` 增加并保存 `score_mode`，返回体带 `score_mode`。
- `backend/app/scorer.py`
  - `build_user_prompt(job, profile=None, filters="")`：技能列表复用 `greeting._profile_skills()`（**注意**：直接对字符串做遍历会把「Java」拆成 J、a、v，必须走这个函数按分隔符切），末尾追加「【我的筛选条件（投递偏好）】…」。
  - `LLMScorer.score(job, profile, filters)`；`Scorer.__init__(..., use_llm=False)`，只有开关打开且配了 key 才走大模型，否则规则打分。
- `web/src/App.tsx` / `api.ts`
  - 敏感库浮层标题左边加「大模型打分 | 规则打分」分段开关，默认规则；切换即 `POST /api/config {score_mode}` 并重建打分器，图片/日志立刻可见。
  - `openJobChat` 找不到会话时改为提示「这家公司有 N 位 HR 可选，点右侧任意一位即可开始沟通」，不再当成错误弹红字。

待办：
- 「自动打招呼」链路已实现（`/api/auto-greet/next`、`/api/auto-greet/one` + 前端 20s 间隔驱动），但还没在真实环境跑通（会真的发消息给 HR，需要人工确认后再测）。
- 用户报告的「第 1 页接口未捕获，DOM 解析 9 条却没入库」：DOM 卡片确实进了 `jobs` 并交给 `on_job`，怀疑卡在 `process_job` —— 详情页没拿到 JD 导致规则分被压低、低于 `min_score` 被排除。待从日志取证。
- 薪资二次校验仍欠修：Boss 卡片薪资是私用区加密字符（`\ue031`=0 … `\ue03a`=9），`parse_salary` 匹配不到；`tighten_ranges` 会把多档并成连续区间；上限 9999 时 `hi-lo` 溢出给满分。

---
## 2026-11-19（下午·二次）消息气泡与列表视图修正

用户反馈：「选中的聊天高亮 boss，聊天内容没有左右区分，消息没有加背景色，消息类型也不对 / 刷新两个字变成了两行，适当加宽boss列表」。

- `web/src/ChatPanel.tsx`
  - 会话行加 `border-l-[3px]`，选中 `border-primary bg-primary/10 ring-1 ring-primary/30 font-semibold`，头像放大到 38。
  - 气泡改微信式配色：我方 `bg-[#95ec69]` 右上小圆角、对方白底描边左上小圆角，`max-w-[80%]`；外层按 `from_me` 对齐到两侧；回执挂在自己气泡右下方。
  - 系统行（附件简历 / 职位卡片 / 类型兜底）改为居中的浅灰小胶囊，不再和对话混在一起。
  - 顶部「刷新」「收起对话框」按钮加 `shrink-0 whitespace-nowrap`，不再挤成两行。
  - 列表与对话框比例从 1:3 调成 1.2:2；`web/src/App.tsx` 右栏展开宽度 52% → 60%。
- `backend/scrapers/boss_chat.py` 的 `normalize_message`
  - `btype` 安全转 int（原来是字符串，`btype == 8` 永远为假，才会出现「[消息类型1]」）。
  - 去掉 `[消息类型N]` 这种兜底，改成「图片 / 附件简历 / 职位卡片 / 语音 / 其他消息」。
  - 新增「正文是否真的存在」判定：只靠类型兜底生成的文字一律当系统行，避免把系统消息画成对方说的话。
  - 16~21 类（纯系统/时间线）不再画气泡。
- 打包版运行时会独占浏览器 profile，dev 后端无法同时用 Chrome。用 mock 接口（`C:\Users\Administrator\AppData\Local\Temp\mock_shot.py`，路由注入 window.fetch 桩）在浏览器里验证渲染，再打包。

## 2026-11-19（下午·三次）「有的会话能分清我和 Boss，有的不能」

根因：`normalize_message` 判断「是不是我发的」只看 `raw.received`。这个字段**并非每条都有**，
缺的时候就退化成 `body.fromId == my_id`，而 `my_id` 要靠猜（拿不到就一直是 None），于是这些会话
里全部消息都被当成对方发的，左右自然就乱了。

- `from_me` 现在允许三态：`True` 我发的 / `False` 对方 / `None` 不确定。
  判定顺序：`received` → `body.fromId/from` 与 `my_id` 比对 → `None`。
- `_merge_status()` 除了合并回执，现在也用 DOM 校正左右：DOM 的 `li.message-item`
  一定有 `item-myself` / `item-friend`，按 `mid` 对齐后以 DOM 为准；实在对不上的再按顺序兜底。
- 正文处理：先 `html.unescape` 再 `_TAG_RE` 去掉 `<copy>…</copy>` 这类标签，
  `_NAV_TEXT_RE` 去掉「拒绝/同意/复制微信号/查看详细分析」这类纯按钮文字；
  正文为空的消息直接丢弃（原来会画出空胶囊）。
- 前端 `ChatPanel.tsx` 增加 `SYS_KINDS = {2,3,5,6,7,8}`：「附件简历 / 图片 / 职位卡片 / 语音」
  一律画居中的浅灰胶囊，不管接口怎么标。
- 验证（用 profile 副本跑 dev 后端，避免占用真实浏览器 profile）：
  周俞 8 条/我 1 条、卞女士 5/2、朱鸿凡 4/1、王女士 4/2、黎女士 14/3、江伟阁 12/3，空文本 0 条。
- 环境提示：打包版运行时独占 `data/browser-profile`，dev 后端必须改用副本目录
  （`AJA_DATA_DIR=...\Temp\devdata`，里面放一份 `browser-profile`）才能和打包版并存。

## 2026-11-19（下午）Boss 消息面板 · 简历发送 · 回执

### 本轮做了什么

**Boss 消息面板（`web/src/ChatPanel.tsx`）**

- 布局：右栏常驻「Boss 消息」列表；展开时列表 `flex-[1]` + 对话框 `flex-[3]`；收起时列表独占右栏、排 2 列；左侧投递列表在收起时可排 3 列。
- 配色统一：列表只保留灰色的选中/悬停态，不再给每个 HR 配不同颜色（`hueOf()` 固定 `#64748b`）。
- 气泡：Boss 消息靠左、我的消息靠右；颜色只用于回执。
- 回执：自己发的气泡下面显示 `已读`（绿）/ `已送达`（灰）/ `未读`（橙）。
- 输入区上方一行：`生成回复`、`发送简历`/`收起简历`。
- 点会话自动展开对话框；未投递岗位点「打招呼」直接跳到对应 HR。

**AI 生成招呼语（`backend/app/greeting.py`）**

- 提示词可在「模型配置」小窗里改（`greet_prompt`），默认值 `DEFAULT_PROMPT`。
- 生成依据：提示词 + 岗位 JD（截前 1200 字）+ 我的技能（`matched_skills()` 取 JD 与画像技能的交集）。
- 没有配模型时用 `rule_greeting()` 兜底，并在界面注明「未配置大模型，使用规则话术」。
- 消耗的 token 累加，显示在「模型配置」左边（`累计 N token（其中生成 M）`）。

**发送简历**

- `GET /api/chat/resumes` 读 Boss 附件简历列表（拦截 `wapi/zpgeek/resume/attachment/checkbox.json`）。
- `POST /api/chat/resume` 给指定 HR 发附件简历。
- 面板默认不请求，点「读取 Boss 上的附件简历并发送」才去读；点「发送」时若还没读过会先自动读取。

### 踩过的坑

1. **historyMsg 里没有已读标识**。`flag` 恒为 0，`uncount` 是未读数，都不代表已读。回执只能从 DOM 的 `.message-status.status-read / status-delivery` 读，再按 `mid` 合并回接口数据（`_merge_status`）。
2. **消息 body.type 是模板 id 不是消息类型**，导致聊天记录显示「[消息类型7]」。必须用 `raw.type`。
3. **Boss 文本里有 HTML 实体**（`&lt;copy&gt;`），要 `html.unescape`。
4. **会话串台**：`_on_history` 原来用 `zp["bossId"]` 做缓存键，但响应里常没有；改为 `_open_uid` 回退 + 点击后用 `.name-text` 校验。
5. **固定延时拖慢读取**：全改成 `_poll()` 轮询，读一次聊天记录从 1.4–7s 降到 0.2–0.4s。
6. **简历列表监听丢失**：只要先调过一次 `friends()`（会 reload 页面），之后再给 `page` 挂 `response` 监听就收不到 `resume/attachment/checkbox` 响应，界面上「发简历」也不弹确认框。目前改用「临时监听 + 用完即摘」仍未根治，**待查**。

### 待办

- [ ] 修「读简历列表」在 UI 里仍报「没有取到 Boss 的附件简历列表」（优先怀疑页面 reload 后监听失效）。
- [ ] 薪资校验修复：PUA 字体解密、`tighten_ranges()` 不再合并区间、`_salary()` 上限 9999 封顶、详情页 `.salary` 选择器。
- [ ] 重新打包（`npm run build` → PyInstaller → `cargo tauri build`）。

---

## 2026-11-19（上午）Boss 消息模块从零到可用

- 新建 `backend/scrapers/boss_chat.py`：会话列表、聊天记录、发送、撤回。
- `backend/app/main.py` 新增 `/api/chat/friends|messages|send|recall|open`。
- 前端新增 `web/src/ChatPanel.tsx`，右栏替换掉原来的「JD 智能对话」占位。

### 踩过的坑

1. **死锁**：`friends()` 用 `threading.Lock()` 且在锁内回调自身 → 改 `RLock`。
2. **输入框不是 textarea**：真实是 `div#chat-input[contenteditable]`，页面里的 `textarea.input` 是 0×0 隐藏镜像。
3. **会话项不能用 `has_text` 过滤**（会超时），改用 `_item_rows()` 取索引再 `nth(idx).click()`。
4. **Boss 求职端网页版没有撤回入口**：自己发的气泡 hover 无操作区、右键菜单为空，「更多」里只有置顶/备注/黑名单/删除/举报。撤回只能在 App 里做（约 10 秒内）。
5. **孤立代理字符导致整页白屏**：Boss 用私用区字体反爬，个别昵称带回 `\udcab` 这类字符，渲染前统一 `clean()` 过滤。

---

## 2026-11-18 打包与稳定性

- `build.bat` 五步：`npm build` → PyInstaller → 复制到 `backend-exe/` → 图标 → `cargo tauri build`。
- 「登录页一直闪烁、日志刷 `_wrap`」：`context_alive()` 用 `ctx.new_page()` 探活，每秒新建一个标签页。改为 `ctx.cookies()`。
- 「程序被直接关闭、无日志」：加了 `crash.log`、`backend.log`、panic hook、单实例锁。
- 「没有可用的登录 cookie」：`/api/expectations` 只读 `app_data_dir()/browser-profile`，而真实登录态在项目根的 `data/browser-profile`。改成多目录回退 + `app_config.data_dir` 记忆真正有 profile 的目录。
