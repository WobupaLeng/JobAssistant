# SPEC-001 MVP · AI 求职助手（冻结版）

- **状态**：`SPEC_FROZEN`
- **冻结日期**：2026-11-20
- **契约真源**：`web/src/api.ts`（前端）+ `backend/app/main.py` 的 `@app.*` 路由与 Pydantic 模型（服务端）
- **变更方式**：冻结后任何需求/字段改动 → 切回 `[存量架构全能优化]` 修订本文件并升版本号（SPEC-002），**严禁**编码模式自行改契约。

## 1. 产品边界

| 项 | 冻结结论 |
|---|---|
| 形态 | Tauri 2 单文件桌面 exe，Rust 壳内嵌前端 + 内嵌 PyInstaller Python 后端 |
| 做什么 | 自动抓取招聘网站岗位与 JD → 打分筛选 → 进「待投递队列」；Boss 会话面板辅助沟通 |
| **不做什么** | **不自动投递**（投递由消费队列的一方完成）；**不做简历上传**；不接入智联 / 51job（界面置灰） |
| 平台范围 | 仅 `boss`。新增平台 = `backend/scrapers/` 下按 `base.py` 契约新增 scraper + `browser.py` 补登录页/域名 |
| 单实例 | 同一时间只允许一个实例占用 `data/browser-profile`，第二实例直接退出 |

## 2. 运行时主线（冻结）

```
Rust(lib.rs)
  ├─ 单实例锁 → spawn_backend()：开发态 python -m app.main / 打包态解包内嵌 exe
  ├─ 捕获 stdout 的 AJA_BACKEND_PORT=<port> → invoke('get_backend_port')
  └─ 前端 fetch http://127.0.0.1:<port>/api/*
Python(app/main.py)
  ├─ Runtime：Store + Scorer + StopFlag + 采集线程(aja-worker，patchright 唯一宿主)
  ├─ ChatLane：独立线程 + 独立 Playwright 客户端，连同一台 Chrome 的 cdp_port
  └─ run_scrape → scrapers/boss.BossScraper → process_job → scorer → store
```

- **线程铁律**：Playwright 同步对象只在创建它的线程内使用；采集线程只跑一个常驻 greenlet；跨线程一律走 `Runtime` 内部 `queue` / 任务投递。
- **浏览器铁律**：统一 `launch_context()` 启动，本机真实 Chrome（缺失回退内置 patchright），持久化 profile，stealth 常开。

## 3. 数据契约（`backend/app/store.py`）

| 表 | 主键 | 关键字段 | 不变式 |
|---|---|---|---|
| `job_queue` | `job_id` | `platform,title,company,salary,status(pending/delivered),match_score,rejected,score_source,score_reason,token_used,jd_text,company_*` | 同 `job_id` 重复入库 → `ON CONFLICT DO NOTHING`；`rejected=1` 仅打标不删除 |
| `jd_decisions` | `id` | `platform,company_job,decision,reason,created_at` | 只追加，决策日志流数据源 |
| `sensitive_jobs` | `id` | `platform,job_id,company,title,salary,location,reason,job_url,kind(rejected/stored)` | 唯一索引 `(platform,company,title)`；重复时 `stored` 优先；**移出一条后该岗位下轮会重新评估** |
| `app_config` | `key` | `value`(JSON 或纯文本) | 键：`api_key,base_url,model,exclude_rules,max_jd,min_score,tokens_*,last_auth,data_dir` |

迁移策略：`_upgrade()` 基于 `PRAGMA table_info` 补列，SQLite 无 `ADD COLUMN IF NOT EXISTS`，**只加不删不改类型**。

## 4. 接口契约（只列分组与语义，逐字段以代码为准）

| 分组 | 路径 | 语义 |
|---|---|---|
| 探活 | `GET /api/health` | 后端就绪探针，QA 与 Rust 启动都依赖它 |
| 抓取控制 | `POST /api/start` `POST /api/stop` `POST /api/pause` `POST /api/resume` `GET /api/status` `POST /api/limit` | 启停/暂停/状态/上限与门槛；`/api/status` 同时返回进度、统计、登录态 |
| 平台与登录 | `GET /api/platforms` `POST /api/login/start` `POST /api/login/save` `GET /api/login/status` `POST /api/login/logout` `GET /api/expectations` | 登录窗口与 cookie 落库（`app_config.last_auth`） |
| 队列 | `GET /api/jobs` `POST /api/jobs/reject` `POST /api/jobs/allow` `POST /api/jobs/deliver` | 列表（分页/排序/三页签）与单/批标注 |
| 敏感库 | `GET /api/sensitive` `POST /api/sensitive/delete` `POST /api/sensitive/clear` | 查重库维护 |
| 配置与打分 | `GET/POST /api/config` `POST /api/rescore` `GET /api/logs` | 配置读写、批量重算、日志增量拉取 |
| 会话 | `GET /api/chat/friends` `GET /api/chat/messages` `POST /api/chat/send` `GET /api/chat/resumes` `POST /api/chat/resume` `POST /api/chat/recall` `POST /api/chat/open` `POST /api/greet` | 会话面板：好友/消息/发送/简历/撤回/打开会话/生成打招呼 |
| 自动打招呼 | `GET /api/auto-greet/next` `POST /api/auto-greet/one` | 取下一个待招呼对象 / 对单个执行 |

**契约红线**：新增或改名接口/字段 → 同一 Milestone 内同步 `web/src/api.ts`、`README.md` 接口表、本文件第 4 节。

## 5. 打分契约（`backend/app/scorer.py`）

- 双通道：配置了大模型 → `LLMScorer`（任意 OpenAI 兼容接口）；未配置或调用失败 → `RuleScorer` 兜底，**流程不中断**。
- 规则权重（满分 10 折算前的原始 100 分制）：技能 40 / 薪资 20 / 经验 15 / 方向 15 / 公司质量 10；命中外包/驻场/短期/项目制信号 → **总分减半**。
- 求职画像唯一来源：`PROFILE` 常量（10 年 Java 后端/全栈，期望 15–35K，上海）。
- 入库门槛：`min_score`（默认 7），低于门槛不入库但记入 `sensitive_jobs`。
- 已知负债（不在 MVP 修复范围）：终分口径写成以上权重之和（实为 100 分制）却按「满分 10」声明，`rescore` 时需以 `match_score` 实际分布为准核对，**任何调整必须先在 SPEC-002 中定稿**。

## 6. 验收标准（MVP 达成线）

1. `build.bat` 全流程成功；`src-tauri\target\release\ai-job-assistant.exe` 双击运行、窗口正常、后端端口就绪。
2. 登录 Boss 成功后抓取一批岗位：合格岗位入库并带 `match_score` / `score_source` / `score_reason`，不合格岗位进 `sensitive_jobs` 且下轮被跳过。
3. 队列三页签（未投递/已投递/不投递）分页、排序、单击展开、批量操作可用。
4. 暂停/继续/停止秒级生效；关闭 Chrome 窗口后再操作会话不报错（自动重开）。
5. 大模型未配置时完整走规则打分；配置错误时自动回退且日志可见原因。
6. 第 4 节三层门禁（`AGENTS.md`）全绿。

## 7. 非目标与已知限制

- Boss 改版会导致解析失效（接口拦截 + DOM 兜底两条路径需同步改）。
- 高频抓取触发风控：页面停在「确认中…」，接口与 DOM 皆空 → 只能降频等待，**不得**靠关 stealth 绕过。
- 薪资筛选项为 Boss 模糊命中，本地需二次精确过滤。
- 零自动化测试（见 `AGENTS.md` 第 4 节槽位 B/C 待回填）。
