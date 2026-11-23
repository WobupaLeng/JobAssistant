# AI 求职助手 · 工程宪法

> 只写**跨模块**约束。目录结构、模块内分层与红线不在本文件，各自的模块文档是唯一真源。
> **只写约束，不写进度**：已修问题、历史校准、版本号与用例数一律不写。模块内的改动写该模块 `HANDOFF.md`；跨模块状态变化只改第 0 节，原地替换。

| 模块 | 模块约定 | 交接 |
|---|---|---|
| `backend-tauri/`（Tauri 2 壳 + 内嵌 Rust 核心） | [backend-tauri/AGENTS.md](backend-tauri/AGENTS.md) | [backend-tauri/HANDOFF.md](backend-tauri/HANDOFF.md) |
| `frontend-vue/`（WebView 内的界面） | [frontend-vue/AGENTS.md](frontend-vue/AGENTS.md) | [frontend-vue/HANDOFF.md](frontend-vue/HANDOFF.md) |
| `scraper/`（Python 抓取 sidecar） | [scraper/AGENTS.md](scraper/AGENTS.md) | [scraper/HANDOFF.md](scraper/HANDOFF.md) |

---

## 0. 状态看板（动手前必读）

**当前阶段**：`E2E_VERIFIED` —— 「Tauri 子进程拉不起真实 Chrome」硬阻塞已除（Job Object 整树回收 + sidecar 常驻单车道线程模型 + `SidecarHandle` 克隆 Drop 守卫，提交 `1f5910b`）；真实小批量抓取端到端实测通过（抓取 → 入库 → 规则打分 → 回读 → 整树回收），登录标记链（`login_save` → `saved=true` → `logout` → `saved=false`）实测通过。剩余：新设备扫码登录全流程与长时间抓取的产品级验收（见 §7.2）。

| 层 | 状态 |
|---|---|
| `backend-tauri/` | ✅ 可用。命令全部定义且全部注册；CSP 已显式声明 `script-src`（缺失即白屏，见 §3.1）；sidecar 子进程 spawn 即纳入 Job Object（`KILL_ON_JOB_CLOSE`），shutdown/强杀整树回收 |
| `scraper/` | ✅ 抓取 / 登录 / 聊天 / 求职期望已通，从 Tauri 子进程拉起真实 Chrome 已修复并实测；浏览器命令走常驻单车道执行器（见模块 AGENTS §5） |
| `frontend-vue/` | ✅ Vue 3 重写完成：四页接线 + M5 补遗、Leptos `.rs` 计数 0、QA 复审收口（A54 几何/11 testid 对齐原型/A36）、vitest 全绿；CSP 收紧（删 `wasm-unsafe-eval`）+ `tauri build --no-bundle` 产物工作区外冒烟通过（Vue 渲染 + IPC 回读正常，非白屏）；Liquid Glass L2 折射克隆层 / L3 WebGL 透镜已实现（SPEC-003），周末三档前端批已落地（SPEC-004） |
| `docs/ui/` + `prototypes/ui-v1/` | ✅ 界面设计 `UI_READY`：聊天抽屉 v3–v10 裁决已冻结（[UI-SPEC-chat-detach-v3](docs/ui/UI-SPEC-chat-detach-v3.md)），单文件原型带三套无浏览器自检脚本 |

- **状态只升不降**，推进时原地替换整表。
- **不写用例数与版本号**（每轮必过期）。要数字现场跑，`#[test]` 静态计数 **≠** 通过数，两者不许混引。
- **并发依据**：`backend-tauri/` 与 `frontend-vue/` 目录完全隔离时可并发；涉及 §4 任一文件时降为单线程。

---

## 1. 项目形态

```
frontend-vue/ ──invoke()──▶ backend-tauri/ 命令层 ──直接调用──▶ backend-tauri/src/core/ (领域)
                            │
                            └──stdin/stdout JSON Lines──▶ scraper/ (Python sidecar)
```

| 目录 | 不可妥协的边界 |
|---|---|
| `backend-tauri/` | 命令层不得写领域规则；`src/core/` 不得 `use tauri::` |
| `frontend-vue/` | **不碰系统 API、不起子进程**，一切经 `invoke` |
| `scraper/` | **不打分、不落库、不持有业务状态** |

**产品约束**：单文件分发，用户无需安装任何运行环境（Python / Node / SQLite 均不进用户机器）。SQLite 编进二进制。

**仓库根只放**：`.gitignore`、`AGENTS.md`、`README.md`、`build.bat`。

**通信模型**：前端与 Rust **同进程、无 HTTP 无端口**。sidecar 是子进程，协议为 stdin/stdout 上的 JSON Lines，三种帧共用一个 `Envelope{id,event,ok,data,error}`：命令（含 `cmd`）、回执（含 `id` 无 `event`）、事件（含 `event`）。

---

## 2. 契约同源（改一处必须同一次提交内改对端）

| 契约 | 真源 | 对端 |
|---|---|---|
| Tauri 命令签名 | `backend-tauri/src/commands_*.rs` + `lib.rs` 的 `invoke_handler` | `frontend-vue/src/api/` |
| sidecar 协议 | `backend-tauri/src/core/sidecar/protocol.rs` | `scraper/sidecar.py` 的 `handle()` / `emit()` |
| 数据库结构 | `backend-tauri/src/core/domain/schema.rs` | `backend-tauri/src/core/infra/sqlite*` |

**三条静默失效**：

1. **驼峰/蛇形双向兼容**：前端发 `minScore`，Rust 字段是 `min_score`；serde 对未知字段**不报错**。新增参数必须加 `alias`，否则界面设的值被静默丢弃。
2. **配置写入分两条命令**：门槛与上限走 `set_limit`，其余走 `set_app_config`。`min_score` 不在后端 `WRITABLE_KEYS` 里，塞进 `set_app_config` 会**静默跳过**。
3. **`LineParse::Ignore` 不是解析失败**：它表示「这行不该冒泡」。新增帧类型时同步 `LineParse` 枚举与 `route_reply` 测试。

---

## 3. 安全红线

1. **严禁**在视图层或命令层写业务规则（打分 / 排除 / 入库判定 / 投放节奏）。外部入参必须严格校验。
2. **API Key、登录 cookie 只落 `app_config` 表**。严禁写进源码、日志、`localStorage` 明文或提交记录。回显只给尾 4 位掩码，**掩码值不得回写**。
3. **渲染进程严禁**直接调系统 API 或起子进程。sidecar 是 `backend-tauri` 的子进程，前端不直接接触。
4. **反爬**：严禁设 `AJA_NO_STEALTH=1` 跑真实抓取，严禁把随机延时（1.5~9.8 秒）改小。调试抓取必须用独立 profile。
5. **数据安全**：严禁对 `data/*.db` 真实库做破坏性验证。迁移 / 写入验证必须在副本或临时目录进行，验完删除。

### 3.1 CSP 铁律

`backend-tauri/tauri.conf.json` 的 `security.csp`：

- **必须显式写 `script-src`**。Tauri 把内联引导脚本的 sha256 追加进 `script-src`，**该指令不存在时哈希无处落地 → 整页脚本被丢弃 → 白屏只剩背景色**。`beforeDevCommand` / `beforeBuildCommand` 为空串，这份 csp 是唯一来源。
- **不加 `'unsafe-inline'`**：效果相同却让 `script-src` 对任何内联脚本失效，白丢 XSS 防护。
- **不用** `dangerousDisableAssetCspModification` 绕过。
- **不在 HTML 里手写** `nonce` / `__TAURI_*_NONCE__` 字面量。本地 `<script src>` 拿不到 nonce（Tauri 只给 `script[src^='http']` 注入）。
- `'wasm-unsafe-eval'` 是 Chromium 130+ 关键字。若 WebView2 报 `Refused to compile or instantiate WebAssembly module`，兜底补 `'unsafe-eval'`。

---

## 4. 高冲突单点（同一时刻只允许一个写者，降为单线程）

`backend-tauri/src/lib.rs`、`backend-tauri/tauri.conf.json`、`backend-tauri/Cargo.toml`、`backend-tauri/src/core/domain/schema.rs`、`backend-tauri/src/core/sidecar/protocol.rs`、`scraper/sidecar.py`。

---

## 5. 验收门禁（完工前必须在终端真实执行）

### 5.1 槽位 A · 静态编译

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:CARGO_TARGET_DIR = "$env:TEMP\jt-<lane>"   # 独占目录，防陈旧产物重放
$env:TEMP = "$env:TEMP"; $env:TMP = $env:TEMP

cd backend-tauri;   cargo test
cd frontend-vue;        <静态检查见 frontend-vue/AGENTS.md>
python -m compileall -q scraper
```

**每次用独占 `CARGO_TARGET_DIR`**。共享目录里的旧产物会被直接复用——实测重放过旧 exe，测试结果与当前源码无关。**排查「改了代码但测试结果不变」先换干净目录。**

### 5.2 槽位 B · 单元测试

用例内容与锚点属于模块内部约定，见各模块 `AGENTS.md`。本文件只约束一件事：**跑实测，不许拿静态计数冒充通过数**。

### 5.3 槽位 C · 集成与产物冒烟

- **构建**：`cd backend-tauri && npx tauri build --no-bundle`。**必须**用 `tauri build`——`cargo build --release` 不内嵌前端 → 白屏。
- **启动冒烟必须用工作区外的产物**。工作区内的 exe 一律跑不起来（§6.1）。
- `tools/*.py` 门禁脚本已随旧版实现删除，不再需要。端到端验证按模块 `HANDOFF.md` 手工进行。

### 5.4 环境坑（Windows）

- **`$env:TEMP` 必须显式设置**：否则 `core/infra/sqlite` 的临时库测试报 `create_dir_all` 权限拒绝（`os error 5`）。**不要改测试代码去绕**。
- **`D:\` 下建目录可能报 `os error 5`**：改用 `$env:TEMP\jt-<lane>`。
- **超短路径不是可选项**：MSVC CL 命令行超限报 `D8050`；工作区内产物继承 Low 完整性标签，启动必报 `os error 5`。
- **`trunk build` 前须清掉 `CARGO_TARGET_DIR`**（Leptos 版遗留）：trunk 只在项目内找产物，且不认 `--target-dir`。

---

## 6. Git 洁净

- **契约熔断**：`core/` 与 `scraper/` 协议不一致、命令层与前端类型不一致、或文档与实际行为冲突时，**立即停止修改业务代码**，`git status` 汇报未提交范围，由用户决定保留或回滚。
- **禁止污染版本库**：不提交密钥、编译产物（`backend-tauri/target/`、`frontend-vue/target/`、`frontend-vue/dist/`、`frontend-vue/node_modules/`）、临时截图与日志。`data/` 含 cookie 与 Chrome profile，已忽略，**不得** `git add -f`。
- **提交信息** `<type>(<scope>): <subject>`：`<type>` ∈ `feat` / `fix` / `docs` / `style` / `test` / `refactor` / `chore`；`<scope>` ∈ `core` / `desktop` / `client` / `sidecar` / `contracts` / `release` / `arch`。
- **提交门禁**：未通过槽位 A 前禁止 `git commit`。每次只提交一个完整 Milestone。
- **用户本地改动保护**：非本人所做的改动不得擅自提交或还原，须先询问。
- **多 lane 并发禁止裸 `git commit`**：索引里可能有别的 lane 已 `git add` 的文件（真实发生过：本想提交 4 个文档，实际带走 12 个）。必须 `git commit --only -- <精确路径>`，提交前后分别用 `git status --porcelain` / `git show --stat HEAD` 核对。消息文件放仓库内（放 `$env:TEMP` 会被 git 拒绝）。
- **批量改源码**用 Python `str.replace`（字面量），**禁止** PowerShell `-replace`（正则引擎会吃掉替换串字符，曾把四份源码所有 `i` 变成 `m`）。

---

## 7. 交接清单

### 7.1 会被违反的既有结论

| 议题 | 结论 |
|---|---|
| 启动报 `os error 5` / `0x800700AA` | 根因不是代码：工作区目录带 Low 完整性标签，里面的 exe 被降为 `S-1-16-4096`，写不了 `%LOCALAPPDATA%`。**产物必须放工作区外**。 |
| 单实例锁 | **不是锁文件**，是 Windows 命名互斥体 `Local\com.ai.jobassistant.rust.single`（`ERROR_ALREADY_EXISTS` 即报重复实例，句柄故意泄漏到进程退出）。旧 `app.lock` 方案已作废。 |
| 真实库迁移 | `migrate()` 必须整体事务。动真实库前先 `Copy-Item` 到 `data/backup/` 并核对 SHA256。 |
| 写库失败不计入 `inserted` | 判定只在 `backend-tauri/src/core/sidecar/ingest.rs`；sidecar 不打分不落库。 |
| 日志游标 | `LogRing::since` 是**闭区间**（`ts_ms >= since`），边界那一毫秒会重送一条，前端按 `(ts, level, message)` 多重集去重。需要恰好一次语义用 `since_seq(after_seq)`。**不要改回开区间**——同毫秒多条日志会永久取不到。 |
| `schema.rs` 无外键 | 父子表靠应用层维护。`rusqlite` 的 `bundled` 默认 `foreign_keys=OFF`，加约束前先 `conn.pragma_update(None, "foreign_keys", "ON")`。 |
| `core/` 禁 `use tauri::` | **是 code review 约定，不是编译期门禁**（`core/` 已不是独立 crate）。同类：`domain` 禁依赖 `rusqlite` / `ureq`。 |
| Chrome 单例撞名 | Chromium Windows 单例身份 = 标题为**字面 user-data-dir 路径**的 `Chrome_MessageWindow` 消息窗口（不是路径哈希）；WebView2 UDF 与 sidecar UDF 路径不同，天然不撞名。真凶是孤儿 Chrome 持窗与 sidecar 自身「每命令新线程 → 探活必炸 → teardown 重启」循环。修法 = Job Object 整树回收 + 常驻单车道，**不是**换 UDF 躲名。 |

### 7.2 未决项

1. **新设备扫码登录全流程未实测**：本轮 E2E 用的是已登录的真实 profile；新设备 / 登录失效态的扫码 → 保存 → 抓取全流程待产品级验收。长时间 / 大批量抓取同样未实测。
2. **抓取自然完成不清 `running`**：sidecar 抓取结束只发 log（`scraper/sidecar.py:258`），Rust 只在 stop/Exited 置 `running=false`（`core/sidecar/session.rs`），界面会停在「抓取中」直到用户手动停止。需协议完成信号 + 前端消费（动 `protocol.rs` 高冲突单点，必须单线程）。
3. **预筛否决的 `rule` 字段恒写 `"prescan"`**，丢失具体命中规则。需给 `ingest::PreScan::Rule` 加承载字段（纯后端，不动前端契约）。
4. **`set_limit` 夹值语义未定**：`max_jd > 500` 时返回错误而非静默夹住，表现为「保存失败但上限已生效」。
5. **`tests/sidecar_smoke.rs` 常年 `#[ignore]`**（需真实 Chrome + 登录态），是唯一未被门禁覆盖的集成路径。
