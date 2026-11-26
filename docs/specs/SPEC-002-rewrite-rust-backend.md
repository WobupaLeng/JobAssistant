# SPEC-002 重写规格：Rust 主后端 + Python 抓取 sidecar（100 分制）

- **状态**：`REWRITE_SPEC_FROZEN`（2026-11-20 冻结，用户 m00167 批准）
- **取代**：`SPEC-001-mvp.md` 中「架构与打包」相关内容（其第 1、4、6 节的产品行为仍为基线）
- **批准依据**：用户 m00167「使用A，cdp抓取已被放弃，打分口径变成100，同时输入也分数选择也跟着变化」
- **本文件是不可回退的契约**：代码、测试、文档与本文件冲突时，以本文件为准；变更须出新 ADR 并升版本号。

## 1. 三项冻结决策

| # | 决策 | 理由 | 不可逆性 |
|---|---|---|---|
| D1 | **技术路线 A**：Rust 主后端（同进程）+ 极简 Python 抓取 sidecar | 体积/启动/打包三项痛点的根因是 Python 运行时（实测 `aja-backend.exe` = 63.3 MB）；但 `patchright` 是 Boss 风控绕过的唯一可用实现，Rust 侧无等价物，故抓取必须留在 Python | 高——目录结构与进程模型整体改变 |
| D2 | **放弃 CDP 抓取方案** | 手写反检测协议需持续对抗 Boss 风控更新，维护成本不可接受 | 中——不再评估 `chromiumoxide` 等路径 |
| D3 | **打分口径统一为 100 分制**，输入门槛与筛选区间同步变为 0–100 | 现状「权重合计 100 却声明满分 10」自相矛盾：`scorer.py:343` 用 `min(10, -(-total // 10))` 把百分制压成十分制，而 `scorer.py:361` 的字段贡献又是 0–40 分 | 高——影响 DB 数据、API 契约、前端 UI 三处 |

## 2. 进程模型（D1 落地形态）

```
┌──────────── ai-job-assistant.exe（唯一交付物）────────────┐
│  Rust 主进程                                              │
│   ├─ Tauri 2 窗口（毛玻璃）+ React 前端（invoke，非 HTTP） │
│   ├─ 业务核心：SQLite · 打分 · 配置 · 日志 · 事件         │
│   └─ sidecar 管理：spawn / 健康检查 / 崩溃重启 / 优雅退出 │
│                                    │ stdio(JSON lines)     │
│                                    ▼                       │
│  aja-scraper.exe（极简 Python，仅抓取）                   │
│   └─ patchright + scrapers/boss.py（逻辑平移，不改）      │
└────────────────────────────────────────────────────────────┘
```

### 2.1 进程间契约（sidecar 协议）

- 传输：**stdin/stdout 上的 JSON Lines**（一行一个 JSON 对象），不使用 HTTP、不占端口。
- Rust → sidecar（命令）：
  ```json
  {"id":"c1","cmd":"scrape.start","params":{"platform":"boss","keywords":["python"],"max_jd":40,"min_score":70}}
  {"id":"c2","cmd":"scrape.stop"}
  {"id":"c3","cmd":"login.start","params":{"platform":"boss"}}
  {"id":"c4","cmd":"chat.send","params":{"friend_id":"...","text":"..."}}
  ```
- sidecar → Rust（事件与回执）：
  ```json
  {"id":"c1","ok":true,"data":{}}
  {"event":"job.scraped","data":{"job_id":"...","title":"...","jd_text":"..."}}
  {"event":"log","data":{"level":"info","msg":"..."}}
  {"id":"c1","ok":false,"error":{"code":"BROWSER_NOT_READY","msg":"..."}}
  ```
- **约束**：sidecar 不做打分、不落库、不读配置——它是无状态的抓取执行器。所有业务判定归 Rust。

### 2.2 与前端的关系

- 前端由 `fetch('http://127.0.0.1:<port>/api/*')` 改为 Tauri `invoke('cmd_name', params)`。
- **收益**：消除端口注入、消除 CORS、消除前端轮询健康检查（`resolvePort()` 三级回退逻辑删除）。
- **代价**：`api.ts` 整体重写；`apply_patch` 前须确认 Tauri capabilities 权限配置。

## 3. 100 分制（D3 落地规格）

### 3.1 分维度权重（合计必须 = 100）

| 维度 | 满分 | 说明 |
|---|---|---|
| 技能匹配 | 50 | 核心技能 40 分（命中 4 项即满）+ 加分技能 10 分（命中 5 项即满），词边界匹配 |
| 薪资匹配 | 10 | 只看岗位上限：覆盖期望上限给满分，低于下限按距离衰减，开得更高不扣分 |
| 经验匹配 | 15 | 期望年限落在岗位要求区间内给满分 |
| 岗位方向 | 15 | 标题命中核心方向 15 / 相邻方向 10 / 其他 7 |
| 公司质量 | 10 | JD 含离职期/过渡岗/短期项目给 4，否则 10 |
| **合计上限** | **100** | 各维度得分直接相加；排除项不走扣分，命中即在打分前硬否决 |

> 薪资权重原为 20、技能 40；用户 m01018 决定降到 10，差额并入技能（40→50）。

**不变式**：
1. 单岗位 `match_score ∈ [0, 100]` 的整数。
2. 权重表合计恒等于 100，CI 用单测断言（`sum(WEIGHTS.values()) == 100`）。
3. 规则打分的 `total` 直接作为分数，**禁止再做 `/10` 或 `min(10, ...)` 截断**（`scorer.py:343` 的旧逻辑必须删除）。
4. LLM 打分同样返回 0–100；模型返回 0–10 或 0–1 时，由适配层按量纲自动放大，且该换算必须单测覆盖。

### 3.2 门槛输入的三处联动（漏改任一处即静默错误）

| 位置 | 现状 | 改为 | 校验 |
|---|---|---|---|
| `backend/app/main.py:196` | `self.min_score = 7` | 默认 `70` | 单测：新建会话默认值 = 70 |
| `backend/app/main.py:1236` | `max(0, min(int(req.min_score), 10))` | 上限 `100` | 单测：传 150 得 100，传 -5 得 0 |
| `web/src/App.tsx:376` | `useState(7)` | `useState(70)` | vitest：初始状态 = 70 |
| `web/src/App.tsx:1480` | `Math.max(1, Math.min(10, ...))` | `Math.max(0, Math.min(100, ...))` | vitest：输入 150 → 100 |
| `web/src/App.tsx:1187` | `Math.max(0, Math.min(10, ...))` | `Math.max(0, Math.min(100, ...))` | vitest：输入 150 → 100 |

> **复核结论（2026-11-20）**：实测共 **5 处**副本，而非最初记录的 4 处。`App.tsx:1187` 为后补发现，证明"同一常量散落多文件"是真实缺陷模式。重写时该常量必须收敛为单一来源（见下）。

**新增单一来源**：Rust 侧定义 `const SCORE_MAX: u8 = 100; const SCORE_MIN_DEFAULT: u8 = 70;`，通过 `invoke('get_scoring_spec')` 下发给前端，前端**不得硬编码 100**（沿用 ARCH-002 第 5 节"契约同源"原则）。

### 3.3 历史数据迁移

- 迁移脚本 `migrations/0002_score_100.sql`：`UPDATE job_queue SET match_score = match_score * 10 WHERE match_score <= 10;`
- 幂等保护：新增列 `score_scale`（`TEXT`，值 `'10'` / `'100'`，默认 `'10'`），迁移时只处理 `score_scale='10'` 的行，完成后置 `'100'`。**禁止无标记的全表乘 10**（重跑会放大 10 倍）。
- `min_score` 配置项（`app_config` 表）：值为 `7` 等旧值时，同步乘 10 并写入 `'100'` 标记。
- 迁移前后须提供 `rescore` 入口，供用户以新口径重算全部岗位。

## 4. 目标目录边界（重写后）

```
core/            Rust 业务核心 crate（无 Tauri 依赖，可独立测试）
  domain/        纯规则：打分（100 分制）· 去重 · 薪资解析
  storage/       rusqlite + migrations
  ports/         trait 定义（Platform · Llm · Clock）
  sidecar/       sidecar 进程管理与 JSON Lines 协议
app/src-tauri/   Tauri 壳：窗口 · 毛玻璃 · capabilities · 命令注册
scraper/         Python sidecar（仅 patchright + boss 抓取，无 FastAPI）
web/             React 前端（invoke 替代 fetch）
```

**分层铁律**：`core` 不得依赖 `tauri`；`core/domain` 不得依赖 `rusqlite`/`reqwest`。

## 5. 验收门禁（重写专属）

- **V1 体积**：`ai-job-assistant.exe` ≤ **20 MB**（当前 63.3 MB，目标压缩 ≥ 68%）。
- **V2 启动**：冷启动到窗口可交互 ≤ **1.5 s**（当前两段式 + 端口轮询）。
- **V3 打包**：`build.bat` 全链路 ≤ **5 min**，且**不再需要 PyInstaller**。
- **V4 单文件**：交付目录仅一个 exe（sidecar 按需释放到 `%LOCALAPPDATA%`，与现状一致的既有策略）。
- **V5 毛玻璃**：Windows 10/11 均可见亚克力/Mica 效果；不支持时**优雅降级为不透明背景**（不得白屏或透明穿透）。
- **V6 行为等价**：`SPEC-001` 第 6 节 6 条验收全部复现；抓取 Boss 成功率不低于现状。
- **V7 打分**：`sum(WEIGHTS) == 100`、`match_score ∈ [0,100]`、门槛四常量单测全绿。

## 6. 非目标（本次重写明确不做）

- 不做 CDP 直连抓取（D2）。
- 不新增招聘平台（保留 `PlatformPort` 扩展点，但本期只接 Boss）。
- 不改「不自动投递 / 不做简历上传」的产品边界（沿用 `SPEC-001` 第 1 节）。
- 不做多语言、不做云端同步、不做自动更新。

## 7. 风险与缓解

| 风险 | 等级 | 缓解 |
|---|---|---|
| Rust 侧重写打分，与旧口径结果不一致 | 高 | 先用同一批 JD 固化「黄金样本」，新旧结果逐条比对；差异须逐条给出解释 |
| sidecar 进程崩溃导致抓取中断 | 中 | Rust 侧监控退出码，自动重启一次并保留进度；失败则事件通知前端 |
| 毛玻璃在 Win10 与 Win11 表现不同 | 中 | 双系统验证；`window-vibrancy` 失败时降级为纯色背景 |
| JSON Lines 协议阻塞（大 JD 文本） | 中 | 单行上限 1 MB，超限截断；写失败立即报错而非静默丢弃 |
| 用户既有 `data/` 数据被迁移脚本破坏 | 高 | 迁移前自动备份 `data/app.db` 为 `app.db.bak-<时间戳>`；提供回滚说明 |
