# `backend-tauri/` 交接文档（HANDOFF）

> **本文件只写「现状 / 坑 / 下一步」**，不写历史、不写用例数、不写版本变化。
> 目录结构与分层约定见 [`AGENTS.md`](AGENTS.md)，跨模块约束见根 [`AGENTS.md`](../AGENTS.md)。

**最后更新**：2026-11-22

---

## 1. 现状

`app/` 完全可用，是当前三个模块中唯一不需要施工的。命令面、领域规则、存储、协议、会话编排都已落地并有测试覆盖。

**唯一未验证的路径**是 sidecar 在真实 Tauri 窗口里拉起 Chrome，见根 `AGENTS.md` 7.2 第 1 项（阻塞项，根因在 WebView2，不在本模块代码）。

**工作区里有未提交改动**（`lib.rs`、`Cargo.toml`、`Cargo.lock`、`tauri.conf.json`），其中 `lib.rs` 的毛玻璃实现改动（`apply_blur` 改为 `apply_system_glass` + 纯色兜底，新增 `BLUR_TINT`）来自用户本地手工调整，**不是任何 lane 所为，提交前须问用户**。

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
- **`tauri.conf.json` 的 `version` 是 `0.2.0`，`Cargo.toml` 是 `0.1.0`**：两者不一致，改版本号时两个文件都要动。
- **`beforeDevCommand` / `beforeBuildCommand` 都是空串**：Tauri 不由它拉起前端，构建编排在根 `build.bat`。**没有 Tauri 侧兜底意味着 `tauri.conf.json` 的 csp 是唯一来源**（见根 `AGENTS.md` 3.4）。

### 2.3 契约面

- **驼峰 / 蛇形参数**：新增命令参数不加 `alias` 会被 serde **静默丢弃**。
- **`min_score` 不在 `WRITABLE_KEYS`**：塞进 `set_app_config` 被静默跳过，门槛只走 `set_limit`。
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

1. **sidecar 拉 Chrome**：阻塞项，见根 `AGENTS.md` 7.2 第 1 项。修法方向已定（给 sidecar 的 Chrome 换不与 WebView2 撞名的 `--user-data-dir`），需要真实桌面环境验证。
2. **`set_limit` 夹值语义**：`max_jd > 500` 时返回错误而非静默夹住，产品口径待定。

### 3.2 可独立开工的小项

1. **预筛 `rule` 字段**：给 `ingest::PreScan::Rule` 加承载规则信息的字段，让决策日志能看出具体命中哪条卡片规则。纯后端，不动前端契约。
2. **`domain` 注释里的 `reqwest` → `ureq`**：文案修正。
3. **`tauri.conf.json` 与 `Cargo.toml` 版本号对齐**：两个文件都是高冲突单点，**必须单线程做**。
4. **`app/src-tauri/.agents/skills/rust-tauri` 的过期锚点**：仍写 `window-vibrancy 0.8`；`rust-domain-purity` / `rust-testing` 的分组用例数与实测不符。
5. **`app/node_modules/` 用途不明**：`app/` 下没有 `package.json`，这个目录来源待查。查清前**不要删**，`npx tauri` 依赖它。
