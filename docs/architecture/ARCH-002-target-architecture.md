# ARCH-002 目标架构：分层 / 端口适配 / 事件驱动（重构蓝图）

- **状态**：`PROPOSED`（待用户批准后冻结为 SPEC-002）
- **前身**：`docs/specs/SPEC-001-mvp.md`（`SPEC_FROZEN`，定义「现在是什么」）
- **本文件回答**：「要变成什么样、按什么顺序变、每步怎么验证」
- **硬约束**：**保留 SPEC-001 第 1、4、6 节的全部对外行为**（接口路径、字段名、DB 表名、exe 单文件形态）。这是纯内部重构，用户可见行为零变化，唯一例外是第 6 节列出的 5 处有意修正。

## 1. 现状诊断（可量化，不是感觉）

| 指标 | 现状 | 定位 |
|---|---|---|
| `backend/app/main.py` | 约 2000 行：40 个路由 + `Runtime` + `ChatLane` + `run_scrape` + `process_job` + 薪资解析 + 日志 | 上帝模块（God Module） |
| `backend/app/main.py` 全局可变状态 | **40 处** `app` / `DEFAULT_*` / `_rt` / `SCRAPERS` 等模块级变量 | 隐式全局单例，测试与并发不可控 |
| 双向依赖 | `main.py → scrapers.boss`，`scrapers/base.py → main._` 反引；`Runtime._ctx` 私有字段被 `_logged_in()` 直接读 | 分层倒置，无端口抽象 |
| `web/src/App.tsx` | 约 2400 行，**43 个 `useState` + 11 个 `useEffect`** | 状态散落，派生状态与副作用混在视图里 |
| 契约 | `api.ts` 手写 528 行类型，与后端 Pydantic 模型**无任何机器校验** | 靠人肉同步，必然漂移 |
| 测试 | 0 个用例 | 任何重构都是「盲改」 |
| 线程/生命周期 | `Runtime` 构造即起 2 个线程；`ensure_browser()` 懒加载；`ChatLane` 持 `rt` 反向调用 `rt.log` / `rt.ensure_browser()` | 生命周期不可注入，无法在测试中启停 |
| 平台扩展 | 新增平台要同时改 `SCRAPERS` 字典、`browser.py` 的 `PLATFORM_HOME` / `LOGIN_COOKIES`、前端 `platformName()` | 一个功能散落 3 个文件 3 种语言 |

**结论**：核心业务逻辑本身是健康的（打分口径清晰、抓取有反爬与兜底、存有迁移），问题全部集中在**结构**——缺少边界、缺少端口、缺少可测性。因此重构策略是**抽取而非重写**：业务规则一行不改地搬进领域层，只在边界上加抽象。

## 2. 目标架构总览

```
┌──────────────────────────── client (React) ────────────────────────────┐
│  pages/          queue · scrape · chat · settings        （页面壳）      │
│  widgets/        JobCard · FilterPanel · LogStream · ChatThread          │
│  features/       reject/* · deliver/* · autogreet/* · rescore/*（交互）  │
│  entities/       job · chat · config                     （模型+UI）    │
│  shared/         api(生成客户端) · ui-kit · lib · config                │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │ HTTP /api/*（OpenAPI 生成 TS 客户端，禁止手写）
┌───────────────────────────────┴───── server (FastAPI) ─────────────────┐
│ interfaces/http/   routers/* · schemas/* · deps.py   ← 只做协议转换      │
│ application/       use_cases/* · ports/* · dto/*     ← 用例编排 + 端口    │
│ domain/            job/* · scoring/* · chat/*        ← 纯规则，零 IO     │
│ infrastructure/    persistence/sqlite · browser/patchright · llm/openai  │
│                    · platform/boss · events/inproc                       │
│ bootstrap/         container.py · lifespan.py · config.py               │
└─────────────────────────────────────────────────────────────────────────┘
        Rust 壳（src-tauri）不变：单实例锁 / 解包后端 / 端口注入 / 崩溃日志
```

### 2.1 依赖方向（唯一合法的箭头，CI 用 import-linter 强制）

```
interfaces → application → domain ← infrastructure
                                  ↑
                            bootstrap 负责把 infrastructure 实现注入 application 端口
domain 不允许 import 任何其他层（含 fastapi / playwright / openai / sqlite3）
```

`domain` 层只允许：`dataclasses`、`typing`、纯函数。这条规则让 `scorer.py` 的规则打分、
`store.py` 的去重语义、`base.py` 的停止/暂停语义都能**毫秒级单测**，不启动浏览器、不连数据库。

## 3. 后端分层改造（逐文件处方）

### 3.1 `domain/`（纯内核，从现有代码平移）

| 新模块 | 来源 | 平移方式 |
|---|---|---|
| `domain/job/models.py` | `main.py` 里的 `job` dict 约定 | 改为 `@dataclass(frozen=True) Job` + `JobQueue` 枚举（`pending`/`delivered`） |
| `domain/scoring/rules.py` | `scorer.py:200 RuleScorer` + `OUTSOURCE_WORDS` / `VETO_NAME_WORDS` | 逻辑原样搬移；`PROFILE` 从常量改为 `UserProfile` dataclass 注入（默认值不变） |
| `domain/scoring/exclusions.py` | `scorer.py:61 ExcludeRules` + `bigweek_hit` / `weekend_bad` / `hr_active_days` | 原样搬移，去掉对 `Any` 的宽泛依赖 |
| `domain/scoring/salary.py` | `main.py:877-917` `parse_salary_code` / `salary_ranges` / `tighten_ranges` / `describe_salary_ranges` + `scorer.py:129 decode_boss_salary` | **从接口层下沉到领域层**（这是当前最典型的层次错位） |
| `domain/scoring/ports.py` | 新增 | `class ScorePort(Protocol): def score(job, profile) -> ScoreResult` |
| `domain/chat/models.py` | `boss_chat.py` 返回值 dict 约定 | `Friend` / `Message` / `Resume` dataclass |
| `domain/chat/greeting.py` | `greeting.py` 的 `rule_greeting` / `build_prompt` / `matched_skills` | 原样搬移；`GreetingWriter` 的 LLM 部分下沉到 infrastructure |

### 3.2 `application/`（用例层，替代 `Runtime` 上帝对象）

`Runtime`（40 处全局状态）拆成 4 个有明确生命周期的对象，全部由 `container.py` 显式构造注入：

| 新对象 | 职责 | 替代的旧状态 |
|---|---|---|
| `ScrapeSession` | 会话实体：`progress` / `max_jd` / `min_score` / `salary_filter` / 状态机（`IDLE→RUNNING→PAUSED→STOPPED`） | `rt.progress` / `rt.max_jd` / `rt.salary_filter` / `rt.running` / `rt.stop` |
| `ScrapeRunner` | 一次抓取的编排：取 scraper → 流式 `on_job` → `ProcessJobUseCase` → 达上限终止 | `run_scrape()` + 闭包 `_on_job` |
| `ProcessJobUseCase` | 单岗位管道：敏感库查重 → 排除规则 → 打分（LLM 优先 / 规则兜底）→ 入库 / 记排除 → 发事件 | `process_job()` |
| `TokenMeter` | 三项 token 计数（`total`/`score`/`greet`）+ 持久化 | `rt.tokens_*` |
| `EventBus`（端口） | 领域事件：`JobStored` / `JobRejected` / `ScrapeProgressed` / `LogEmitted` | 直接调 `rt.log()` 的 20+ 处散点 |

用例清单（每个 ≤ 80 行，一个文件）：
`StartScrape` / `StopScrape` / `PauseScrape` / `ResumeScrape` / `GetStatus` / `SetLimit`
`LoginPlatform` / `SaveLogin` / `Logout` / `GetExpectations` / `ListPlatforms`
`ListJobs` / `RejectJobs` / `AllowJobs` / `DeliverJobs` / `RescoreJobs`
`ListSensitive` / `DeleteSensitive` / `ClearSensitive`
`GetConfig` / `UpdateConfig` / `ReadLogs`
`ListFriends` / `ListMessages` / `SendMessage` / `OpenConversation` / `SendResume` / `RecallMessage` / `GenerateGreeting`
`NextAutoGreet` / `RunAutoGreetOne`

### 3.3 `application/ports/`（端口定义，让领域与用例不感知具体技术）

```python
class JobRepository(Protocol):          # 由 infrastructure/persistence/sqlite 实现
    def enqueue(self, job: Job) -> bool
    def list(self, f: JobFilter) -> list[Job]
    def count(self, f: JobFilter) -> int
    def set_rejected(self, ids: list[str], rejected: bool) -> int
    def set_delivered(self, ids: list[str], delivered: bool) -> int
class SensitiveRepository(Protocol): ...
class ConfigRepository(Protocol): ...
class BrowserPort(Protocol):            # launch_context / ensure / logged_in / cookies
class PlatformPort(Protocol):           # scrape(ctx, on_job) / read_expectations(ctx)
class ChatPort(Protocol):               # friends / messages / send / open / resumes
class LlmPort(Protocol):                # chat_json / chat_text / usage
class ClockPort(Protocol):              # now_ms()，让时间可注入（单测"HR 活跃度"必需）
```

**关键收益**：`PlatformPort` 一旦存在，**新增一个招聘平台 = 新增一个 `infrastructure/platform/zhilian.py` + 注册一行**，
不再需要改 `main.py`、`browser.py`、`App.tsx` 三处（现状的扩展成本痛点）。

### 3.4 `infrastructure/`（技术实现的唯一归属地）

- `persistence/sqlite/`：`Store` 拆成 `SqliteJobRepo` / `SqliteSensitiveRepo` / `SqliteConfigRepo` + `migrations.py`（保留「只加不删」策略）；**表名与列名一字不改**（SPEC-001 第 3 节不变式）。
- `browser/patchright/`：现 `browser.py` 整体迁入，对外只暴露 `PatchrightBrowser`（实现 `BrowserPort`）；`data/browser-profile` 路径规则不变。
- `llm/openai_compat.py`：`LLMScorer` / `GreetingWriter` 的 HTTP 部分合并为一个客户端（同 base_url / api_key / model 配置源）。
- `platform/boss.py` + `platform/boss_chat.py`：现 `scrapers/*` 迁入，实现 `PlatformPort` / `ChatPort`。
- `events/inproc.py`：进程内 `EventBus`（`subscribe(topic, handler)`），日志与 SSE 都挂在这里；**为未来换 Redis/队列留好缝合点**。

### 3.5 `interfaces/http/` 与 `bootstrap/`

- `routers/{scrape,jobs,sensitive,config,chat,auth,logs}.py`：每个文件只做「解析入参 → 调用例 → 转换出参」，**每个 handler ≤ 10 行**。
- `schemas/`：Pydantic 模型按路由分组，字段名与 SPEC-001 第 4 节完全一致。
- `deps.py`：`Depends` 注入用例实例（来自 `container`），**取代模块级 `_rt` 全局**。
- `lifespan.py`：`startup` 构造容器 / 启动采集与会话线程；`shutdown` 停线程、关浏览器、`store.close()`（现状没有优雅停机）。

## 4. 前端重构（Feature-Sliced Design）

| 目标目录 | 搬迁来源 | 说明 |
|---|---|---|
| `shared/api/` | `api.ts` 的 fetch 封装 | 保留 `resolvePort()` 三级回退；类型改为 **`openapi-typescript` 生成**，`api.ts` 只留请求器 |
| `shared/config/` | `PLATFORM_CHIP_COLOR` / `STATUS_CHIP` / `PAGE_SIZE` | 常量集中 |
| `shared/lib/` | `SPLIT_KEYWORDS` / `NUM_OR_NULL` / `fmtTime` / `fmtDate` / `fmtTokens` | 纯函数，配 vitest |
| `entities/job/` | `JobRow` 类型 + `ReviewSwitch` + `CARD_SCORE_COLOR` | 卡片与徽章 |
| `entities/chat/` | `ChatPanel.tsx` 的消息气泡与回执渲染 | 拆成 `MessageBubble` / `ThreadList` |
| `features/reject-job/` `features/deliver-job/` `features/auto-greet/` `features/rescore/` | `App.tsx` 中对应的 `handle*` 闭包 | 每个 feature 导出 `useXxx()` hook，**接口调用不外泄** |
| `widgets/filter-panel/` `widgets/job-card/` `widgets/log-stream/` `widgets/settings-dialog/` | `App.tsx` 的大块 JSX | 受控组件，`props` 只收数据与回调 |
| `pages/queue/` `pages/chat/` `pages/settings/` | `App.tsx` 顶栏与三页签 | 每个 page 组合 widget，不写业务规则 |

**状态管理处方**：**43 个 `useState` 收敛为 3 类**
1. **服务端状态** → `TanStack Query`（分页、轮询 `/api/status`、缓存与失效），删掉手写 loading/error 的 effect；
2. **跨页 UI 状态**（筛选条件、主题、当前页签）→ 一个 `zustand` store；
3. **局部交互状态**（展开态、勾选）→ 留在组件 `useState`。

**同时落地的规范**：全部交互控件补 `data-testid`（`AGENTS.md` 第 2 节已定命名规则），这是未来 E2E 的前提。

## 5. 契约同源：从「人肉同步」到「机器校验」

1. 后端开启 `/openapi.json` → CI 执行 `openapi-typescript` 生成 `web/src/shared/api/schema.d.ts`。
2. CI 校验：`npx openapi-typescript ... && git diff --exit-code web/src/shared/api/schema.d.ts`——**后端改了字段而前端没同步，流水线直接红**。
3. `AGENTS.md` 第 3 节相应改为：「前端类型只允许来自生成文件，**禁止手写 DTO**」。
4. 该步骤同时产出 SPEC-001 第 4 节的机器可读版本，`README.md` 的接口表改为生成或由脚本校验。

## 6. 有意修正的行为（除这些以外，用户可见行为零变化）

| # | 问题 | 修正 |
|---|---|---|
| 1 | 打分权重合计 100 却声明「满分 10」（SPEC-001 第 5 节已锁） | 归一化为 0–10 分并给出迁移说明；**必须先出 SPEC-002 定稿**，历史 `match_score` 通过一次性 `rescore` 重算 |
| 2 | `POST /api/sensitive/clear` 曾因 `self._lock` 崩溃（本轮已修） | 补回归测试锁死 |
| 3 | 无优雅停机（进程退出时 `store` 连接与浏览器不关） | `lifespan.shutdown` 统一释放 |
| 4 | CORS `allow_origins=["*"]` | 收紧为 `http://localhost:5183` + `tauri://localhost`（WebView 需实测确认） |
| 5 | 日志缓冲 `rt.logs` 无上限 | 环形缓冲（默认 5000 条），`/api/logs` 语义不变 |

## 7. 分阶段落地路线（每阶段独立可交付、可回滚）

| 阶段 | 目标 | 验收门禁 | 风险 |
|---|---|---|---|
| **P0 安全网** | 建 `backend/tests/`（pytest）+ `web/src/__tests__/`（vitest），先给**现状**写特征测试（黄金用例）：规则打分、薪资解析、去重、`clear_sensitive` | 槽位 A+B 全绿；覆盖率：`domain` 相关代码 ≥ 70% | 无（只加测试） |
| **P1 后端领域层抽取** | 新建 `domain/`，平移打分/薪资/排除规则；`main.py` 改为调用领域函数 | 特征测试全绿 + 前后对比黄金数据一致 | 低（纯搬移） |
| **P2 端口与用例层** | 建 `application/ports` + `use_cases`，拆 `Runtime`；`main.py` 退化为 `interfaces/http` 路由注册 | 20 个接口契约测试（响应结构快照）+ 抓取冒烟 | 中（线程与生命周期） |
| **P3 契约生成** | 引入 `openapi-typescript`，`api.ts` 改由生成类型驱动 | CI diff 校验通过 | 低 |
| **P4 前端 FSD** | 按第 4 节拆 `App.tsx` / `ChatPanel.tsx`，引入 TanStack Query + zustand，补 `data-testid` | `npm run build` + vitest + 手动回归清单 | 中（纯前端，可灰度） |
| **P5 一致性收尾** | 第 6 节 5 项修正 + 优雅停机 + 日志环形缓冲 | 槽位 C 冒烟：`build.bat` 全流程 + exe 启动 | 中（涉及打包） |

**每个阶段都满足**：① 单独提交（Conventional Commits，`refactor(scope):`）；② 阶段结束跑一次 `build.bat` 冒烟；③ 不通过的阶段不进入下一阶段。

## 8. 新增质量门禁（写入 `AGENTS.md` 第 4 节）

- **A 静态**：`compileall` / `npm run build` / `cargo check` / **`ruff check`（新引入）** / **`import-linter`（分层依赖方向）**。
- **B 单元**：`pytest -q`（domain + use_cases，目标覆盖率 ≥ 70%）/ `vitest run`。
- **C 集成**：`pytest -m integration`（FastAPI `TestClient` 打真实 SQLite 临时库，不碰 `data/`）+ `npx playwright test`（前端 UI，不测招聘网站）。
- **D 冒烟**：`build.bat` + exe 启动 + `GET /api/health`。

## 9. 扩展性验收（重构成功的可测定义）

以下三件事在重构后必须能「只改一处」完成，否则视为 P2 未达标：

1. **新增招聘平台**（如智联）：新增 `infrastructure/platform/zhilian.py` + 注册；前端平台名与颜色从后端 `/api/platforms` 下发（消除 `App.tsx` 里的 `platformName()` 硬编码）。
2. **更换存储**（SQLite → Postgres）：新增 `persistence/postgres/` 实现 `JobRepository`，业务代码零改动。
3. **新增打分维度**：在 `domain/scoring/rules.py` 加一个 `Dimension`，配置与 UI 自动通过 `/api/config` 透出。

## 10. 与现有文档的关系

- `SPEC-001` 定义**现状契约**（对外行为基线），本文件定义**目标结构**，二者不冲突：本文件只在第 6 节列出的 5 处改变行为，且需另出 `SPEC-002` 定稿后才可动代码。
- `ADR-001`（单文件桌面）、`ADR-002`（Playwright 线程收敛）**继续有效**，本架构完全遵守：Rust 壳与打包链不变，Playwright 仍按车道独占。
- 本文件批准后：新增 `ADR-003`（分层架构与端口适配）、`ADR-004`（契约由 OpenAPI 生成）各一份，并更新 `AGENTS.md` 第 0 节状态为 `ARCH_REFROZEN`。
