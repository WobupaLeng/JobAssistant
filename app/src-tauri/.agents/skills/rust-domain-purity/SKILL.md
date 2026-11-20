---
name: rust-domain-purity
description: Layering rules for app/src-tauri/src/core - keeping domain rules pure and testable, using store and ingest ports instead of concrete SQLite/HTTP/Tauri calls, and the three "silent wrong answer" traps (no-JD veto, ignored protocol lines, rescore paging). Use when adding a rule, a scoring branch, a parse function, an ingest decision, or any code in core/domain that seems to need IO, a clock, randomness, a database handle or a Tauri type. Not for wiring commands or state (use rust-tauri) and not for the SQL itself (use rusqlite).
---

# rust-domain-purity — `core/` 的分层铁律

导语：`core/` 曾经是独立 crate，2026-11-21 并入 `app` crate 后**编译器不再替你守边界**
（根 `AGENTS.md` 第 0 节），守边界的人就是读这份文件的你。
收益非常具体：`cargo test --lib` 里 **367 个用例、0.3 秒跑完**，不需要窗口、不需要浏览器、不需要数据库文件。

规则一句话：**`core/` 里的纯函数只吃参数、只吐结论；一切副作用经 trait 端口注入。**

---

## Routing Trigger

- 要在 `core/domain/**` 加规则、分支、解析、判定
- 发现某段领域逻辑「需要一个数据库 / 一个 HTTP 客户端 / 当前时间 / 随机数」
- 要动 `core/sidecar/ingest.rs` 的入库判定或计数口径
- 要动 `core/domain/scoring*.rs` 的通过 / 否决语义
- 要动 `core/sidecar/process/codec.rs` 的帧解析
- 要动 `core/domain/rescore.rs` 的全量重算

不读本 skill：命令层接线（`rust-tauri`）、SQL 写法（`rusqlite`）、测试环境（`rust-testing`）。

---

## The Reference Map

| 要读什么 | 文件:行 | 读它为了 |
|---|---|---|
| 分层铁律的原文 | `app/src-tauri/src/core/mod.rs:1-7`、`:11-15` | 「本 crate 不得依赖 tauri」「domain 不得依赖 rusqlite」 |
| 打分口径唯一真源 | `core/mod.rs:18-27` | `SCORE_MAX=100` / `SCORE_MIN_DEFAULT=70` |
| 存储端口 | `core/domain/job.rs:113`（JobStore）、`:192`（SensitiveStore） | 端口签名与「为什么游标按 job_id」的注释 |
| 四个管理端口 | `core/domain/store_ports.rs:19/144/197/233` | `JobAdminStore` / `SensitiveAdminStore` / `DecisionStore` / `ConfigStore` |
| LLM 端口 | `core/domain/scoring_llm.rs:58` | `LlmClient: Send + Sync`，实现只在 `infra/llm.rs:122` |
| 入库纯函数 | `core/sidecar/ingest.rs:1-8`、`:252-322`、`:324-339` | 模块自称纯函数；`DecisionNote` / `decision_note()` 纯映射；三卡预筛 |
| 副作用端口 | `core/sidecar/session.rs:148-163`、`:166-203` | `IngestPort` 三方法 + 默认空实现的坑 |
| 无 JD 否决 | `core/domain/scoring.rs:262-280`、`ingest.rs:180-190` | `RuleScore::passes()` 与 72 分事故 |
| 帧解析语义 | `core/sidecar/process/codec.rs:36-52`、`:54-70`、`:73-96` | `LineParse::Ignore` ≠ 解析失败 |
| 重算分页 | `core/domain/rescore.rs:105-130`、`core/infra/sqlite/jobs.rs:151-165` | 游标必须按 `job_id` |
| 可测性范式 | `core/domain/auto_greet.rs:1-13`、`:31-41`；`core/domain/logbuf.rs:99-105` | 时间 / 随机数一律做参数 |

---

## Non-Negotiables

1. **`core/` 不得 `use tauri::`。**
   `core/mod.rs:4` 与 `lib.rs:8`（`//! 改动 core/ 时仍然禁止 use tauri::`）都写死这条。
   现状是全 `core/` 里 57 处 `tauri` 全部出现在注释里，**没有一行 `use tauri::`**。
   唯一合法的「Tauri 相关物」在壳层：状态结构（`ScrapeState` / `StoreState`）定义在 `commands_*.rs`，
   进程级 `static` 互斥放在命令层（`commands_scrape.rs:14-15`、`:37-44`），因为「core 不得持有 Tauri 生命周期里的状态」。
   违反的症状不是编译错误，而是 `core` 的用例从此必须起一个窗口才能跑。

2. **`domain` 不得依赖 `rusqlite` / `ureq` / `tauri`，存储走 trait 端口。**
   （`core/mod.rs:5`）。端口都在 `domain/` 定义、在 `infra/` 实现：
   - `JobStore`（`domain/job.rs:113`）、`SensitiveStore`（`domain/job.rs:192`）
   - `JobAdminStore`（`store_ports.rs:19`）、`SensitiveAdminStore`（:144）、
     `DecisionStore`（:197）、`ConfigStore`（:233）
   - `LlmClient`（`domain/scoring_llm.rs:58`）——HTTP 唯一落点是 `infra/llm.rs:122` 的 `ureq::post`
   → 在 `domain/` 里 `use rusqlite::Connection` 一行，就把整个领域层绑死在「必须有一个 `.db` 文件」上。

3. **加副作用一律经端口注入，端口要能被一个闭包替身替掉。**
   范本是抓取链路：
   - `IngestPort`（`session.rs:148-163`）只有三件事：`put`（写库）、`remember`（记敏感库）、
     `record`（写决策日志）。
   - ⚠ **`record` 带默认空实现**（`session.rs:162`），注释原话是「生产实现 `scrape::make_sink`
     必须覆盖，否则决策日志生产路径恒空」。默认空实现是为了让测试少写代码，代价是——
     新写一个 `impl IngestPort` 时代码能编译、用例能绿、**生产环境决策日志一条不写**。
   - `Ports`（`session.rs:166-203`）用三个闭包 + `no_decision` 拼装，测试直接塞闭包。
   - 纯的那一半：`decision_note(&ScrapedJob, &IngestVerdict) -> DecisionNote`（`ingest.rs:286-322`）
     是纯数据映射（Accepted→`"accept"` + `new_score=Some`；Rejected→`"reject"` + `None`；
     BelowThreshold→`"skip"` + `None`），脏的那一半只有 `record`。

4. **时间 / 随机数必须是入参，不是调用。**
   - `domain/auto_greet.rs:1-13` 模块注释：本模块不启动定时线程、只承载判定，**必须是纯函数**；
     `next_gap_ms(roll: f64) -> u64`（:31-41）把随机数当参数收，`NaN → 0.5`、越界 clamp、
     下限 `AUTO_GAP_MIN_MS` 兜底。
   - `domain/logbuf.rs:99-105` 把 `now_ms()` 与 `push_at(ts, ..)` 分开，测试只用 `push_at`
     （`logbuf.rs:111-139` 三条用例全部走显式时间戳）。
   → 「测试里 `sleep(20s)` 等抖动」是本项目明确不接受的设计。

5. **无 JD 否决只能走 `RuleScore::passes()`。**
   `domain/scoring.rs:275-280`：
   ```rust
   pub fn passes(&self, min_score: u8) -> bool { !self.rejected_no_jd && self.score >= min_score }
   ```
   事故原文（`scoring.rs:262-273`）：无 JD 岗位在薪资与经验都匹配时**实测能拿 72 分**，
   越过默认 70 分门槛。所以 `score < min_score` 这种写法在门槛 ≤70 时会**直接放行**。
   `ingest.rs:180-190` 记录了同一处修复（「原先只比 `score < cfg.min_score`」→ 改成 `if !rule.passes(cfg.min_score)`），
   `scoring_service.rs:33-40` 是规则 / 大模型双通道的同名入口。
   → 任何新的「够不够格」判断，都问一句：**这里要不要连否决位一起看？**

6. **`LineParse::Ignore` 是「这行不该冒泡」，不是解析失败。**
   `codec.rs:36-52` 的三分类：
   - `Event` — 真正的 sidecar 事件
   - `Ignore` — 结构合法但与本层无关（**命令回执、未知 event 名**），静默丢弃
   - `Malformed` — 不是 JSON 对象等畸形内容，才上报

   事故原文（`codec.rs` 枚举注释）：早先读线程写的是 `parse_line(line).unwrap_or_else(|| Malformed)`，
   把两者都当失败，结果 `scrape.start` 的回执 `{"id":"c1","ok":true,…}` 在界面上冒出「无法解析」——
   **测试却全绿**。现在的顺序是：先 `route_reply` 消费回执，再 `parse_line`，
   `Ignore` → `continue`，`Malformed` → 带截断行的 `SidecarEvent::Malformed`（`codec.rs:102-157`）。
   → 新增帧类型时，`LineParse` 枚举与 `route_reply` 的测试必须同步。

7. **`JobStore::list_scored_ids` 必须按 `job_id` 升序。**
   全量重算要用它翻页，而重算**自己会写 `match_score`**。
   按 `match_score` 排序 → 写回后后续页的边界整体漂移 → 同一条被算两次、别的被跳过，
   **且没有任何报错**（`domain/job.rs:125-137` 注释；实现 `infra/sqlite/jobs.rs:151-165`；
   用例 `infra/sqlite/tests.rs:141-165`、`:168-175`）。
   配套两条：`rescore.rs:110` 的 `.max(1)` 防止 `limit=Some(0)` 原地打转；
   `rescore.rs:124` 的 `verdict.score.min(SCORE_MAX)` 防止越界分数写进 `u8` 列。

8. **`core/domain/schema.rs` 的建表语句与补列声明表必须两处一致**（`schema.rs:140-143` 注释），
   因为老库靠 `upgrade_columns` 补列，新库靠 `SCHEMA` 建表。详见 `rusqlite` skill。

---

## Workflow：把一段「想调 IO 的领域逻辑」拆干净

1. **先写纯判定**：入参是 `&Job` / `&ScrapedJob` / `&Config` 等值类型，出参是结论枚举
   （范例 `IngestVerdict`、`RuleScore`、`RestartAction`）。
2. **再找它需要的副作用**：读库？写库？发 HTTP？看时间？各对应一个 trait 方法
   （范例 `IngestPort::put/remember/record`、`LlmClient`、`next_gap_ms(roll)`）。
3. **在 `infra/` 写实现，在 `core/scrape.rs` 拼装**（`make_sink`(90) / `make_lookup`(143) 就是装配点）。
4. **测试只测第 1 步**，用闭包 / `no_decision` / 假实现替第 2 步
   （范例 `sidecar_smoke.rs:57-60` 用一个不写决策日志的端口）。
5. **回查否决语义**：这段结论会不会被 `score < min_score` 之类的朴素比较绕过（铁律 5）。

### 自查清单（提 PR 前逐条打勾）

- [ ] `core/` 里没有新增 `use tauri::`（`grep -n "use tauri" src/core` 应为空）
- [ ] `core/domain/` 里没有新增 `rusqlite` / `ureq`
- [ ] 新增的 trait 方法没有「默认空实现」——如果有，生产实现必须覆盖并补一条用例
- [ ] 时间 / 随机数都是入参
- [ ] 「够不够格」走的是 `passes()` 而不是分数比较
- [ ] 翻页游标是稳定键（`job_id`），不是会被自己改写的列

---

## 本模块实证速查

| 现象 | 真因 | 依据 |
|---|---|---|
| 无 JD 岗位进了库（门槛 ≤70） | 用了 `score < min_score` | `scoring.rs:262-280`、`ingest.rs:180-190` |
| 界面冒出「无法解析」但数据正常 | 把回执当成 `Malformed` | `codec.rs:36-52` |
| 重算漏算 / 重复算，无报错 | 游标按 `match_score` | `job.rs:125-137`、`jobs.rs:151-165` |
| 决策日志生产环境一条没有 | `IngestPort::record` 用了默认空实现 | `session.rs:148-163` |
| `core` 的测试突然要起窗口 | `core` 里混进了 `tauri::` | `core/mod.rs:4`、`lib.rs:8` |
| 领域测试需要等 20 秒 | 抖动没做成入参 | `auto_greet.rs:1-13`、`:31-41` |
| 分数偶发超过 100 | 写回前没 `.min(SCORE_MAX)` | `rescore.rs:124` |
