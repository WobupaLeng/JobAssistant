# AI 求职助手 · 工程宪法（AGENTS.md）

> 本文件是仓库根目录唯一的工程治理文档。**AI 每次动手前必读**，任何模式不得私自降低第 0 节状态。
> 由「存量架构全能优化」模式依据仓库真实现状（2026-11-20 探测）编写，未虚构任何目录。

## 0. 工程状态看板（由架构师、编码与 QA 维护，AI 每次动手前必读）
- **当前阶段状态**：`SPEC_FROZEN`（面向存量代码的规格已冻结，允许按第 2 节目录边界并发派发 `[后端编码]` + `[前端编码]`；涉及 `backend/app/main.py`、`src-tauri/src/lib.rs`、`web/src/api.ts` 时仍降级单线程）
- **当前活跃规格**：`docs/specs/SPEC-001-mvp.md`（`SPEC_FROZEN`，2026-11-20 冻结）
- **架构决策**：`docs/adr/ADR-001-single-file-desktop-architecture.md`（单文件桌面架构）、`docs/adr/ADR-002-playwright-thread-greenlet-confinement.md`（Playwright 线程收敛）
- **Mock 全局开关状态**：`DISABLED`（本项目的「后端」是本地 Python 进程而非 Mock 服务；前端浏览器调试时用 `VITE_AJA_PORT` 直连真实后端）
  - 2026-11-20 状态变更记录：`INIT`/`DEV_IN_PROGRESS`（无 SPEC）→ `SPEC_FROZEN`（补 `docs/specs/` + `docs/adr/`）。状态只升不降。

## 1. 项目形态与核心技术栈
- **项目类型**：混合桌面端（Tauri 2 单文件 exe）——Rust 壳 + WebView 内嵌前端 + 内嵌 PyInstaller 后端进程
- **核心技术栈**：
  - **壳 / 进程编排**：Rust 1.77+（`edition 2021`）、Tauri 2、`tauri-plugin-shell`、`tokio`，`cargo 1.100.0`
  - **界面**：Vite 6 + React 19 + TypeScript 5.7 + Tailwind CSS 3.4，包管理 `npm`（锁文件 `web/package-lock.json`），Node 22
  - **后端服务**：Python 3.11+（本机实测 3.14.7）、FastAPI + uvicorn、Pydantic 2、`openai` SDK（任意 OpenAI 兼容接口）
  - **浏览器自动化**：`patchright`（Playwright 反检测分叉）+ 本机真实 Chrome + 持久化 profile
  - **存储**：SQLite（标准库 `sqlite3`，无 ORM），开发态 `data/job_assistant_pro.db`，打包态 `%LOCALAPPDATA%\ai-job-assistant\data\`
  - **打包**：`build.bat` 五步链 —— `npm run build` → PyInstaller(`backend/aja-backend.spec`) → 复制到 `backend-exe/` → `tools/make_icons.py` → `cargo tauri build`
- **进程与通信模型**：Rust 启动时 `spawn_backend()` 拉起 Python 后端（开发态找 `python` / `AJA_PYTHON`，打包态把嵌入的 exe 解包后运行），后端把监听端口打到 stdout，Rust 捕获后通过 `invoke('get_backend_port')` 交给前端；前端所有数据请求走 `http://127.0.0.1:<port>/api/*`。

## 2. 目录分层与绝对红线
- **物理目录边界与模块注册表（供调度中枢分配并发写权限 & 映射 Git `<scope>`）**：
  - **架构与全局契约（全员只读，仅 1/2 号架构模式可写）**：
    - `AGENTS.md`（本文件）、`README.md`、`DEVELOPMENT.md`（开发进度记录，按时间倒序追加）
    - `docs/`（ADR 决策与 SPEC 规格书）：`docs/specs/SPEC-001-mvp.md`、`docs/adr/ADR-001-single-file-desktop-architecture.md`、`docs/adr/ADR-002-playwright-thread-greenlet-confinement.md`
    - `backend/requirements.txt`、`web/package.json`、`web/package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json`、`build.bat`、`.gitignore`
  - **接口契约（跨端唯一真源）**：
    - `web/src/api.ts`（`<scope>: contracts` | 前端侧全部 `/api/*` 调用的类型与封装，**新增/改字段必须前后端同一 Milestone 内同步**）
    - `backend/app/main.py` 中的 `@app.*` 路由与 Pydantic 请求模型（`<scope>: server` | 服务端契约定义处）
  - **服务端模块（可按模块横向并发派发 [后端编码]，但 `main.py` 为高冲突单点，见下方红线）**：
    - `backend/app/main.py`（`<scope>: server` | FastAPI 入口 + `Runtime` 状态机 + `ChatLane` + 全部 40 个接口 + 抓取编排，≈2000 行）
    - `backend/app/store.py`（`<scope>: server` | SQLite 存储层：`job_queue` / `jd_decisions` / `sensitive_jobs` / `app_config`，含老库补列迁移）
    - `backend/app/scorer.py`（`<scope>: server` | 大模型 / 规则双通道打分，`PROFILE` 求职画像常量）
    - `backend/app/greeting.py`（`<scope>: server` | 打招呼文案生成：大模型 + 规则兜底）
    - `backend/app/browser.py`（`<scope>: server` | patchright 启动、profile 复用、`LoginSession`、driver 路径）
    - `backend/app/cookie_import.py`（`<scope>: server` | 从 Chrome profile 读并解密 cookie）
    - `backend/scrapers/base.py`（`<scope>: scrapers` | 抓取器基类：限速、暂停/停止信号）
    - `backend/scrapers/boss.py`（`<scope>: scrapers` | Boss 岗位解析：接口拦截 + DOM 兜底）
    - `backend/scrapers/boss_chat.py`（`<scope>: scrapers` | Boss 会话：好友列表、消息、主动发起会话、自动打招呼、简历发送）
    - `backend/entry.py`、`backend/aja-backend.spec`（`<scope>: release` | PyInstaller 引导与打包配置）
  - **客户端模块（可按端横向并发派发 [前端编码/前端微调]）**：
    - `web/src/App.tsx`（`<scope>: client` | 主界面：顶栏、筛选、队列三页签卡片、日志、设置弹窗，≈2400 行）
    - `web/src/ChatPanel.tsx`（`<scope>: client` | JD 智能对话 / 会话面板）
    - `web/src/api.ts`（`<scope>: contracts` | 见上）
    - `web/src/index.css`、`web/src/main.tsx`、`web/index.html`、`web/tailwind.config.js`、`web/postcss.config.js`、`web/vite.config.ts`、`web/tsconfig.json`（`<scope>: client`）
  - **桌面壳模块（跨进程单点，**严禁**与前端/后端并发派发）**：
    - `src-tauri/src/lib.rs`（`<scope>: desktop` | 启动编排、内嵌后端解包、崩溃日志、单实例锁、WebView 目录清理）
    - `src-tauri/src/commands.rs`（`<scope>: desktop` | `get_backend_port` / `get_data_dir` / `get_crash_log_path`）
    - `src-tauri/src/main.rs`、`src-tauri/build.rs`、`src-tauri/tauri.conf.json`、`src-tauri/icons/`（`<scope>: desktop`）
  - **工具与运行产物（不入库、不作为任何子任务写目标）**：
    - `tools/make_icons.py`（`<scope>: release` | 图标生成）
    - `data/`（`browser-profile/`、`logs/`、`*.db`、`*.db-wal/shm`，**含登录 cookie，严禁入库/严禁伪造**）
    - `backend/build/`、`backend/dist/`、`backend-exe/`、`web/dist/`、`web/node_modules/`、`src-tauri/target/`、`src-tauri/gen/`
- **逻辑分层规范**：
  - **服务端**：接口层（`main.py` 的 `@app.*` + Pydantic 模型）→ 编排层（`Runtime` / `ChatLane` / `run_scrape`）→ 平台适配层（`scrapers/*`）→ 存储层（`store.py`）。新增平台 = 在 `scrapers/` 增一个 scraper 并在 `base.py` 契约下实现，**不得**把解析逻辑写回 `main.py`。
  - **客户端**：单文件组件现状（`App.tsx` / `ChatPanel.tsx`），新增 UI 按「视图组件 + `api.ts` 数据封装」两层摆放；**不得**在组件内手写 `fetch` 或重复 DTO 类型。
  - **桌面壳**：Rust 只负责进程与系统能力，**不得**承载业务逻辑；新增前端需要的能力走 `#[tauri::command]` + `invoke`，并在 `lib.rs` 的 `generate_handler!` 注册。
  - **异步与线程铁律**：Playwright 同步 API 只能在其创建线程内使用；`main.py` 的采集线程与 `ChatLane` 各自持有 driver，**严禁跨线程复用 Playwright 对象**（历史上 `Cannot switch to a different thread` 由此产生）。
- **跨边界与安全红线**：
  - [通用铁律]：严禁在视图/路由层直接编写复杂业务逻辑或硬编码密钥；所有外部入参必须经过严格校验（Pydantic 模型或显式类型转换）。
  - [通用铁律]：API Key、登录 cookie 只落 `app_config` 表，**严禁**写进源码、日志、`localStorage` 明文或提交记录。
  - [混合桌面端]：渲染进程严禁直接调用系统 API 或任意子进程，必须走 `invoke` 安全桥接；后端只监听 `127.0.0.1`，CSP 已收紧为 `connect-src 'self' http://127.0.0.1:* ipc:`。
  - [反爬铁律]：**严禁**设置 `AJA_NO_STEALTH=1` 跑真实抓取，**严禁**把随机延时（1.5~9.8 秒）改小；调试抓取必须用独立 profile。
  - [并发写铁律]：`backend/app/main.py`、`src-tauri/src/lib.rs` 为高冲突单点文件，**同一时间只允许一个写者**；涉及它们时降级为单线程任务。
- **UI 控件定位铁律（仅含界面项目生效）**：
  - 命名格式统一为 `<页面/模块>-<功能>-<控件类型>`（如 `queue-reject-btn`、`chat-send-btn`）。
  - 所有可交互控件（按钮、输入框、列表项、页签）**必须**声明唯一测试锚点 `data-testid`。存量代码缺失处，在触碰该组件时补齐，不单独开一轮纯补锚点任务。

## 3. 契约同源与本地联调
- **架构图纸与接口契约**：`docs/adr/` 与 `docs/specs/SPEC-001-mvp.md` 定义原始契约（已冻结；修订必须先升版为 SPEC-002 并在 ADR 中留痕）；代码层唯一真源是 `web/src/api.ts`（前端调用与类型）+ `backend/app/main.py` 的 `@app.*` 路由与 Pydantic 模型（服务端定义）。**严禁**两端各自手写重复 DTO：前端字段以 `api.ts` 为准，后端字段改动必须同一提交内更新 `api.ts`、`README.md` 的接口表与 SPEC 第 4 节。
- **本地联调与 Mock 策略**：
  - 开发态**不使用 Mock**：Tauri 里运行时端口由 Rust 注入（`invoke('get_backend_port')`）；纯浏览器调试时 `cd web && VITE_AJA_PORT=<后端端口> npm run dev` 直连 `python -m app.main` 启动的真实后端。
  - 后端自身具备降级策略：未配置大模型时走规则打分，大模型失败自动回退规则 —— 这是**运行时兜底**，不是 Mock，不得关掉。
  - QA 验收期必须关闭所有调试开关（`AJA_NO_STEALTH` 未设、`VITE_AJA_PORT` 不硬编码），并确认 `data/` 未混入临时 profile。

## 4. 实证交付门禁（三层通用校验槽位，AI 完工前必须在终端真实执行）
- **槽位 A [静态编译与类型/规范检查]**：
  - 前端：`cd web && npm run build`（`tsc -b && vite build`，同时充当类型检查）
  - 后端：`cd backend && python -m compileall -q app scrapers entry.py`（当前未引入 ruff/mypy；引入后请在此补 `python -m ruff check .`）
  - Rust：`cd src-tauri && %USERPROFILE%\.cargo\bin\cargo.exe check`（`cargo` 未加入 PATH，`build.bat` 与本节均用绝对路径）
- **槽位 B [核心逻辑单元测试（毫秒级反馈）]**：
  - **后端（pytest，待落地）**：`cd backend && python -m pytest -q`。目标用例（Milestone 1 补齐）：`store.py` 的 `enqueue_job` 重复跳过 / `sensitive_hit` 三键去重 / `clear_sensitive` 清空行数；`scorer.py` 的 `RuleScorer` 外包减半、`decode_boss_salary` 反爬字符与区间；`main.py` 的 `salary_ranges` / `tighten_ranges`。
  - **前端（vitest，待落地）**：`cd web && npx vitest run`。目标用例：`api.ts` 的 `resolvePort()` 三级回退（invoke → `VITE_AJA_PORT` → 8791）与 `api()` 错误信息拼装。
  - **在两者落地前的最低替代门禁（必做）**：`python -m app.main` 能起服务且 `GET /api/health` 返回正常；改动打分逻辑必须用 `POST /api/rescore` 对 ≥3 条存量岗位验证分值变化；改动 `Store` 写方法必须用临时库（`tempfile` 下的 `job_assistant_pro.db`）跑一次对应方法的读写，**严禁**直接对 `data/` 里的真实库做破坏性验证。
- **槽位 C [集成交互、E2E 与最终产物冒烟（QA/发布阶段执行）]**：
  - *源码级集成/UI 交互测试*：`<填入：尚无 Playwright E2E 用例，Milestone 2 回填 `npx playwright test`（仅测本应用 UI，不测招聘网站）>`
  - *最终交付物构建与启动冒烟*：`build.bat` 全流程 + 启动 `src-tauri\target\release\ai-job-assistant.exe`，确认窗口正常、后端端口就绪、`data/logs` 无新 `crash.log`；安装包产物在 `src-tauri\target\release\bundle\nsis\`。
  - *打包前置条件*：必须先关闭运行中的 `ai-job-assistant.exe` / `aja-backend.exe`，否则 `cargo tauri build` 报 `os error 32`；必须使用 `cargo tauri build` 而非 `cargo build --release`，否则前端不内嵌导致白屏。

## 5. 契约熔断与 Git 洁净规范
- **契约熔断处理**：当编码或 QA 阶段触发契约冲突熔断（`api.ts` 与后端路由/字段不一致、或 SPEC 与实际行为冲突）时，AI 必须立即停止修改业务代码，执行 `git status` 汇报当前未提交改动范围，提示用户决定保留暂存（`git stash`）或回滚后，再开新会话切回 `[2. 存量架构全能优化]` 修订 `SPEC`。
- **禁止污染版本库**：严禁提交 `.env` 密钥、编译产物（`dist/`、`bin/`、`build/`、`src-tauri/target/`、`backend-exe/`、`web/dist/`）及 Playwright/测试生成的临时截图（`.diag-page.png` 等）。`data/` 整目录含登录 cookie 与 240 MB+ Chrome profile，`.gitignore` 已忽略，**不得**用 `git add -f` 绕过。
- **提交信息规范 (Conventional Commits)**：
  - 格式强制为 `<type>(<scope>): <subject>`。
  - `<type>` 仅限：`feat`（新功能/UI骨架）、`fix`（修Bug）、`docs`（ADR/SPEC/宪法/DEVELOPMENT）、`style`（纯样式微调/动效）、`test`（补测试）、`refactor`（重构）、`chore`（脚手架/构建发布）。
  - `<scope>` 必须对齐本文件第 2 节物理目录：`server`（`backend/app`）、`scrapers`（`backend/scrapers`）、`client`（`web/src`）、`contracts`（`api.ts` / 路由契约）、`desktop`（`src-tauri`）、`release`（`build.bat` / spec / 图标）、`arch`（`AGENTS.md` / `docs/`）。
  - **提交门禁**：严禁在未通过第 4 节终端校验（槽位 A，以及槽位 B 已回填时的 B）前执行 `git commit`；每次仅提交当前完成的单个 Milestone 改动，严禁混杂无关文件。
- **文档同步**：修 Bug 或完成一个可观察行为的改动后，在 `DEVELOPMENT.md` 顶部按既有格式（日期 + 现象 / 根因 / 改动文件 / 验证）追加一条；接口或使用方式变化同步更新 `README.md`。

## 6. 交接清单（Handoff，未决项必须在此列明）
*已关闭项（2026-11-20 本轮）：SPEC 与 ADR 已建立并冻结；`README.md`「已知限制」脱节已更正并补会话接口表；`Store.clear_sensitive()` 的 `self._lock` 残留已修。*

1. **测试仍未落地**：第 4 节槽位 B 目前给出的是**目标命令 + 替代门禁**，仓库仍是零用例。下一 Milestone 必须新建 `backend/tests/`（`test_store.py`、`test_scorer.py`）与 `web/src/__tests__/api.test.ts`，并在 `backend/requirements.txt` 补 `pytest`、`web/package.json` 补 `vitest`。
2. **槽位 C 的 E2E 方案未定**：需在「Playwright 驱动真实 Tauri 窗口」与「仅测 `npm run dev` 的 WebView 页面」之间二选一；决定后回填命令，测试脚本只允许在 `tests/e2e/` 新增。
3. **工作区存在未提交改动**（本轮修改前 12 个 + 本轮 6 个：`AGENTS.md`、`README.md`、`DEVELOPMENT.md`、`docs/**`、`backend/app/store.py`）：并发派发前必须先让用户决定提交、暂存或回滚。建议提交信息：`docs(arch): 补齐工程宪法与 SPEC-001/ADR` + `fix(server): 敏感库清空改用 tx()`。
4. **`cargo` 未加入 PATH**：所有 Rust 命令用 `%USERPROFILE%\.cargo\bin\cargo.exe`。
5. **打分终分口径待定稿**：`scorer.py` 规则权重合计 100（技能 40 / 薪资 20 / 经验 15 / 方向 15 / 公司质量 10），但文档与 UI 均声明「满分 10」。已记入 SPEC-001 第 5 节，调整前不得改代码，须先出 SPEC-002。
