# `backend-tauri/` 交接文档（HANDOFF）

> **本文件只写「现状 / 坑 / 下一步」**，不写历史、不写用例数、不写版本变化。
> 目录结构与分层约定见 [`AGENTS.md`](AGENTS.md)，跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。

**最后更新**：2026-11-23

---

## 1. 现状

命令面、领域规则、存储、协议、会话编排都已落地并有测试覆盖。

**本轮功能接线完成**（提交 d3cd57d / b6bebde）：① `scrape_defaults` 运行时消费——`Command::ScrapeStart` 协议扩 9 个可选字段（city/salary/experience/degree/scale/stage/industry/job_type/max_pages，蛇形、`skip_serializing_if` 向后兼容，与 `scraper/sidecar.py` payload 键逐字对齐），`ScrapeJobSpec` 同步扩展 + `start_command()` 透传，`scrape_start` 读库填 spec（前端 args 优先、`ScrapeArgs` 契约零改动），新增 `core/scrape/filters.rs`（`filters_from_defaults`/`exclude_rules_from_defaults`），用户排除词经 `ExcludeRules::from_keywords` 注入 `IngestConfig`（留 Rust core，禁下放 sidecar）；② serde alias 补缺（`GreetArgs` jobId/useLlm、`RescoreArgs` logChanges）；③ 登录链——`lib.rs` setup 起 `login-state-watcher` 线程 500ms 轮询 `ScrapeState`，`(platform,state)` 变化即 `app.emit("login-state", {platform,state,logged_in})`（蛇形冻结契约，纯函数 `login_snapshot`/`login_emit_decision` 去重 + 会话消失补播 closed），`session.rs` `ScrapeProgress` 增 `login_platform` 并在 consume/adopt 继承，`login_status` 查询结论写回 progress。`cargo test --lib` 392 passed。

**词元边界累计已接线**（ADR-005，提交 7445545）：`TokenMeter` 装饰器包住 `LlmClient::complete` 边界，greet/score 每轮成功（含重试）各计一次，`reply.tokens` 语义不变；`ConfigStore::bump_tokens` 单锁事务 upsert `tokens_total`/`tokens_greet`/`tokens_score` 三键（SQL 侧加法，遗留值只增不清）；bump 失败只 warn，绝不失败业务。

**周末三档筛选已实现**（SPEC-004，提交 009c643）：`work_weekend` 支持 `double`/`no_bigweek`/`single`（double ⊂ no_bigweek ⊂ single），JD 命中按休息制度族归类（无休＞单休＞大小周，多重命中取最重族），未标注归一化为双休、三档均含；命中池补「无休」。前端 `single` 选项与 hint 校正归前端批（options.ts/types.ts），落地前两端字面量集尚未对齐。

**Chrome 硬阻塞已除**（提交 `1f5910b`）：① sidecar 子进程 spawn 即纳入 Job Object（`KILL_ON_JOB_CLOSE`，`core/sidecar/process/job.rs`），shutdown/强杀关句柄回收 python + node driver + Chrome 整树；② `SidecarHandle::Drop` 加 `Arc::strong_count` 守卫——克隆共享子进程，只有最后一个持有者触发 shutdown，修掉「前端挂载即 `login_status`，其瞬态克隆出 scope 就关掉共享 stdin 杀掉会话」；③ sidecar 侧改常驻单车道执行器（见 `scraper/HANDOFF.md`）。真实小批量抓取 E2E 实测通过：`scrape_start`(Java, maxJd=10, 门槛 70) → scraped=15 / inserted=7 / below_threshold=5 / rejected=3，新入库行规则五维打分字段齐全，日志无撞名 / 无 `TargetClosedError`，`scrape_close` 整树回收约 1.6s，`login_save` → `saved=true` → `logout` → `saved=false` 标记链通过。`login_status.saved` 恒 false 已修（提交 `348eb1a`）。

**抓取自然完成信号已接线**（本轮 fix(sidecar)）：协议新增 `scrape.finished` 事件（`{count,stopped,error}`，additive，Envelope 三帧形状不变）；`session.rs` consume 收到即 `running=false` 并按帧写 `finished_reason`（`stopped`→「已按用户请求停止」；`error` 非空→「抓取异常结束：…」；否则「抓取完成，共 N 条」，N 用 Rust 权威计数 `p.scraped`），五段计数保留；`if p.running` 守卫保证与手动 stop 竞态不互相覆盖原因。**consume 侧只信帧内 `stopped`，不用 `stop_requested`**——后者只在 stop/Drop 置 true 从不复位，跨轮次粘滞会把上一轮用户停止误记到新一轮自然完成。前端零改动：Dashboard 每 tick 轮询 `scrape_progress`，下一 tick 自动退出「抓取中」。真实 E2E（Java，maxJd=10，已登录真实 profile，自然跑完、全程不手动 stop）：+151s `running=false`、reason=「抓取完成，共 15 条」、scraped=15 / inserted=4 / below_threshold=6 / rejected=5 计数保留，CDP 取证顶条转「空闲」，`scrape_close` 整树回收 1.7s，硬杀 exe 后残留 0（证据 `%TEMP%\jt-running\t8\RESULT-t8.json`）。

---

## 2. 坑（踩过的，别再踩）

### 2.1 构建与运行

| 坑 | 怎么踩的 | 正确做法 |
|---|---|---|
| 工作区里的 exe 起不来 | 报 `os error 5` / `0x800700AA`，或 WebView 数据目录不可写 | 产物**必须**放工作区外。工作区目录带 Low 完整性标签，里面进程被降为 `S-1-16-4096` |
| 改了代码但测试结果不变 | Cargo 重放了旧二进制 | 换独占的 `CARGO_TARGET_DIR`（如 `$env:TEMP\jt-<lane>`） |
| `cargo test` 报 `create_dir_all` 权限拒绝 | 没设 `$env:TEMP` | 显式设 `$env:TEMP` / `$env:TMP`。**不要改测试代码去绕** |
| `D:\` 下建目录报 `os error 5` | 该盘 ACM 策略 | 改用 `$env:TEMP\jt-<lane>` |
| `cargo build --release` 出来的 exe 白屏 | 没内嵌前端 | **必须** `npx tauri build --no-bundle` |
| 毛玻璃诊断用 `PrintWindow` 截图 | 拿到的是应用自绘内容，把已生效误判成没生效 | 用 `SetForegroundWindow` + `CopyFromScreen`；或看 `AJA_DIAG_WINDOW=1` 的 stderr `[diag] hwnd=… layered=…` |

### 2.2 代码里容易写错的地方

- **`c_void` 必须写绝对路径 `::core::ffi::c_void`**：crate 顶层有 `core` 模块，会遮蔽 std 的 `core`。
- **`window-vibrancy` 已不在依赖里**：毛玻璃走 `windows-sys` 动态加载 `SetWindowCompositionAttribute`。`Cargo.toml` 的注释写着原因，别再把它加回去。
- **单实例互斥体句柄不能 `CloseHandle`**：设计上故意泄漏到进程退出。
- **`domain` 里的注释写着 `reqwest`，实际依赖是 `ureq`**：这是文案漂移，改到那里顺手修正。
- **`commands_auto.rs` 有 2 个既有 unused import 警告**（`GreetingProfile`/`DEFAULT_PROFILE`）：历史遗留，与功能提交分开清理。
- **`tauri.conf.json` 的 `version` 是 `0.2.0`，`Cargo.toml` 是 `0.1.0`**：两者不一致，改版本号时两个文件都要动。
- **`beforeDevCommand` / `beforeBuildCommand` 已设为 `npm --prefix ../frontend-vue run dev` / `run build`**（Vue 重写后接线；`backend-tauri/package.json` 锚 `@tauri-apps/cli` 修 cwd，否则 `../frontend-vue` 解析到仓库外 ENOENT）。`tauri.conf.json` 的 csp 仍是唯一来源（`script-src 'self'`，`wasm-unsafe-eval` 已删，见根 `AGENTS.md` 3.1）。
- **`SidecarHandle` 是共享克隆，Drop 必须查 `strong_count`**：全字段 Arc 共享、手写 Clone，任一克隆出 scope 都会跑 Drop。守卫在 `process/handle.rs` 的 Drop（`strong_count(&self.child) > 1` 直接 return）；删掉守卫 = 瞬态克隆（`login_status` / `request_chat` 的 `handle_clone`）随手杀会话。回归测试 `dropping_handle_clone_keeps_sidecar_alive` 钉住此行为。
- **Job Object 回收的是整树不是单个 child**：python + node driver + Chrome 全在 job 里，shutdown 尾部 drop job 句柄触发 `KILL_ON_JOB_CLOSE`。不要「优化」成只 kill child——那会留下持 Chrome 单例窗的孤儿。

### 2.3 契约面

- **驼峰 / 蛇形参数**：新增命令参数不加 `alias` 会被 serde **静默丢弃**。
- **`min_score` 不在 `WRITABLE_KEYS`**：塞进 `set_app_config` 被静默跳过，门槛只走 `set_limit`。
- **词元三键的累计口径**：`tokens_total`/`tokens_greet`/`tokens_score` 只经 `ConfigStore::bump_tokens` SQL 侧加法累计，遗留值只增不清，非数字 CAST→0（与读侧 `num()` 同口径）。
- **`LineParse::Ignore` 不是解析失败**：它是「这行不该冒泡」的正常分支。
- **`session.rs` 的预筛否决 `rule` 字段恒写 `"prescan"`**：决策日志里看不出具体命中了哪条卡片规则。

### 2.4 数据库

- **`rescore` 的排序键必须是 `job_id` 升序**：按 `match_score` 排序会被自己的写回推翻分页边界，同一岗位算两次、别的被跳过，**且没有任何报错**。
- **SQLite 的 `ORDER BY x ASC` 把 NULL 排最前**（与 MySQL / PG 相反）：面议 / 空必须恒为第一排序键且恒 ASC。
- **`migrate()` 必须整体事务**：崩在中途分数会 ×100。动真实库前先 `Copy-Item` 到 `data/backup/` 并核对 SHA256。
- **`schema.rs` 无任何外键约束**：父子表靠应用层维护。
- **`LogRing::since` 是闭区间**：边界那一毫秒会重送一条。需要「恰好一次」用 `since_seq(after_seq)`。

### 2.5 测试

- **静态 `#[test]` 计数会与实跑数完全吻合**（本次实测 376 = 376）。不吻合就先查统计口径（正则漏匹配、glob 不递归），不要先怀疑代码。
- **`tests/sidecar_smoke.rs` 常年 `#[ignore]`**：它是唯一没被门禁覆盖的集成路径。

---

## 3. 下一步

### 3.1 等外部条件

1. **`set_limit` 夹值语义**：`max_jd > 500` 时返回错误而非静默夹住，产品口径待定。
2. **新设备扫码登录全流程**：本轮 E2E 用已登录 profile；扫码 → 保存 → 抓取全流程待产品级验收（根 `AGENTS.md` §7.2）。

### 3.2 可独立开工的小项

1. **预筛 `rule` 字段**：给 `ingest::PreScan::Rule` 加承载规则信息的字段，让决策日志能看出具体命中哪条卡片规则。纯后端，不动前端契约。
2. **`domain` 注释里的 `reqwest` → `ureq`**：文案修正。
3. **`tauri.conf.json` 与 `Cargo.toml` 版本号对齐**：两个文件都是高冲突单点，**必须单线程做**。
4. **`app/src-tauri/.agents/skills/rust-tauri` 的过期锚点**：仍写 `window-vibrancy 0.8`；`rust-domain-purity` / `rust-testing` 的分组用例数与实测不符。
5. **`app/node_modules/` 用途不明**：`app/` 下没有 `package.json`，这个目录来源待查。查清前**不要删**，`npx tauri` 依赖它。
