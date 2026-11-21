---
name: contract-sync
description: 三处契约真源（Tauri 命令签名 / sidecar 协议 / 数据库结构）的同步纪律：改一处必须同一次提交改哪些对端、驼峰↔蛇形双写与 serde 静默丢弃陷阱、DecisionRecord 与 AppConfigReply 的前后端字段对照、契约熔断流程、一条命令自查漂移。Use when adding or renaming a Tauri command / command 参数 / 返回结构体 / 数据库列，或排查「界面上设的值没生效」「字段恒为空」「list 返回空」。Not for 抓取选择器（sidecar-scraping）或 SQLite 事务细节（rusqlite）。
---

# contract-sync：三处契约真源与它们不会自己报错的漂移

这个仓库的失败模式不是"编译不过"，而是**编译通过、测试全绿、界面上静默失效**。
根因集中在三处契约各有一份手写副本，且两侧的序列化器对"多一个/少一个字段"的态度不同：

- Rust 侧 `serde` 对**未知字段不报错**（默认忽略），对**缺失字段**才报错（除非有 `#[serde(default)]`）；
- 前端 `types.rs` 的返回结构体几乎每个字段都带 `#[serde(default)]`（缺失即是默认值）；
- 数据库那侧则是 `PRAGMA table_info` 先查再补列，缺列/多列都不报错。

三处都不会喊。

## Routing Trigger

命中任一项就按本 skill 办事：

- 新增/改名/删除一个 `#[tauri::command]`
- 新增/改名命令的**参数结构体字段**或**返回结构体字段**
- 前端 `frontend/src/api/{mod,types}.rs` 里加一坨 `json!({...})`
- 改 `job_queue` / `jd_decisions` / `sensitive_jobs` / `app_config` 的列
- 症状：「界面上设的值没生效」「某字段永远空」「列表永远是空的」「命令不存在」

## Version Anchor

三处真源（`AGENTS.md` 3.1 的原表）：

| 契约 | 真源 | 必须同步的对端 |
|---|---|---|
| Tauri 命令签名 | `app/src-tauri/src/commands_*.rs` + `app/src-tauri/src/lib.rs:473-522` 的 `invoke_handler` | `frontend/src/api/mod.rs`、`frontend/src/api/types.rs` |
| sidecar 协议 | `app/src-tauri/src/core/sidecar/protocol.rs` | `scraper/sidecar.py` 的 `handle()`（:521-572）与 `emit()`（:47-57） |
| 数据库结构 | `app/src-tauri/src/core/domain/schema.rs` | `app/src-tauri/src/core/infra/sqlite.rs` |

命令层所有参数结构体都**定义在真源侧**（`commands_*.rs` 或 `core/domain/app_config.rs`），
前端 `types.rs` 只是镜像。改镜像不改真源 = 编译过、无效果。

## 铁律 1：驼峰 / 蛇形必须双向兼容（alias 一个都不能删）

链路事实（不要凭印象，按这两行去读）：

- 前端 `invoke` 只是把 JSON 原样丢进 `window.__TAURI_INTERNALS__.invoke`
  （`frontend/src/api/mod.rs:20-38`，`serde_wasm_bindgen::to_value(&args)` **不做任何键名转换**）；
- Rust 侧字段是蛇形（`min_score`），前端**大量**发驼峰（`minScore`）；
- `serde` 遇到"结构体里没有的键"**不报错**，直接丢弃。

所以漏一个 `alias`，那个参数就永远是 `None`/`false`/默认值，**没有一处会报错**。

### 三个范例结构体的 alias 行号

`ScrapeArgs`（`app/src-tauri/src/commands_scrape.rs:163-176`）：

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]          // :168
pub struct ScrapeArgs {
    pub platform: Option<String>,
    pub keywords: Option<Vec<String>>,
    #[serde(default, alias = "maxJd")]        // :172
    pub max_jd: Option<u32>,
    #[serde(default, alias = "minScore")]     // :174
    pub min_score: Option<u8>,
}
```

:165-166 的注释是原话：「**`alias` 不能删**：前端发驼峰（`minScore` / `maxJd`），
Rust 字段是蛇形，serde 对未知字段不报错——少了 alias 门槛会被静默丢弃。」

`ScoreArgs`（`app/src-tauri/src/commands_db.rs:311-328`）：`:312` `rename_all = "snake_case"`，
`:320` `alias = "useLlm"`，`:323` `alias = "llmPrefilter"`，另有 `:326-327` `filters`（可省略）。

`RescoreArgs`（`app/src-tauri/src/commands_db.rs:451-471`）：`:455` `alias = "afterJobId"`、
`:462` `alias = "useLlm"`、`:464` `alias = "llmPrefilter"`、`:469` `#[serde(default = "default_true")] log_changes`。
`:460-461` 注释：`use_llm` **默认 false**——全库走大模型按条计费，必须显式打开。

`AppConfigReply` 那侧的入参 `LimitArgs`（`app/src-tauri/src/core/domain/app_config.rs:150-156`）
与 `ScrapeDefaultsArgs`（`:200-230`，14 个字段逐个 alias）同理，`:148-149` / `:198-199` 都写了同一句警告。

### 现状（已核实，别再当缺陷报）

- 前端 `types.rs` 的参数结构体（`ListJobsArgs:127-142`、`ScoreArgs:178-186`、`RescoreArgs:600-606`）
  全部是**蛇形**，与 Rust 字段同名 → 即使没有 alias 也能命中；
- 前端 `api/mod.rs` 里手写的 `json!` 用的是**驼峰**
  （`get_job` `{"jobId"}` :71、`scrape_start` `{"maxJd","minScore"}` :114-123、
  `set_limit` `{"maxJd","minScore"}` :301-309、`chat_send` `{"jobId"}` :157-162、
  `chat_send_resume` `{"resumeId"}` :365-372、`greet_draft` `{"jobId","useLlm":true}` :380-396）
  → **这一路全靠 alias 兜着**。

结论：alias 不是历史包袱，是当前生效路径。新增字段时照抄 `#[serde(default, alias = "驼峰名")]`，
并顺手加一条反序列化测试（照 `commands_db.rs:864-875`
`score_args_accept_both_camel_and_snake_case`：驼峰与蛇形两种 JSON 都要能进）。

### 返回方向的"多一个字段"是安全的，但也别依赖

前端 `types.rs` 的返回结构体字段大多带 `#[serde(default)]`，所以 Rust 少发一个字段不会炸，
只会得到空串 / 0 / `Null`。这掩盖了两类真实问题（`AppConfigReply` 为例）：

- 前端 `AppConfigReply.api_key: String`（`frontend/src/api/types.rs:349-352`）**Rust 侧根本没有这个字段**
  （`commands_config.rs:36-66` 无 `api_key`）。这是有意的——密钥只以 `api_key_hint` 回传
  （types.rs:345-347 注释）；前端拿到的恒为空串，正是"输入框占位"。
  **不要**为了"对齐"给 Rust 加上 `api_key`：那会把密钥送进 WebView。
- `commands_config.rs:64` 的注释说「旧的 23 个字段一个都没动」，而结构体实际是 **22 个字段**
  （:38-65）。以代码为准，别以注释为准。

## 铁律 2：参数外层还有一个 `args`

Rust 命令签名里叫 `args` 的那个参数，前端必须**多包一层**：

| 前端写法 | 例子 |
|---|---|
| `invoke(cmd, args)` 直接传结构体 | `list_jobs`（mod.rs:66-68）、`score_job`（:78-80） |
| `invoke(cmd, {"args": {...}})` | `rescore_jobs`（:86-96）、`list_decisions`（:354-362）、`set_limit`（:301-309）、`set_scrape_defaults`（:294-298）、`sensitive_list`（:168-192）、`reject_jobs`/`allow_jobs`/`mark_delivered`（:206-211）、`auto_greet_next`（:242-249）、`greet_draft`（:380-396） |
| `invoke(cmd, {...})` 直接传裸字段 | `get_job`（:70-72）、`scrape_start`（:108-124）、`scrape_drain`（:104-106）、`chat_send`（:157-163）、`chat_messages`（:148-150）、`login_*`/`logout`/`session_open`（:132-142、:314-329）、`list_logs`（:339-344）、`greet_one`（:398-400）、`auto_greet_pause`（:251-258）、`set_app_config`（:284-286） |

**判据**：Rust 命令的最后一个参数是不是一个具名的 `XxxArgs` 结构体。
是，就要包 `args`；不是（直接 `since: Option<i64>`、`platform: &str` 这类散参），就平铺。
包错的表现是「参数全取默认值」或「缺字段反序列化失败」，**不会**提示"多包了一层"。

## 铁律 3：命令注册是单点，且没有编译期检查

`invoke_handler` 在 `app/src-tauri/src/lib.rs:473-522`，**48 条**注册项；
`#[tauri::command]` 定义也是 **48 条**（分布见 `rust-tauri` skill 铁律 2）。
2026-11-21 之前这里还挂着一条"定义了没注册"的 `commands_auto::auto_greet_gap`，
已在提交 `bb9d16c`「删除死命令 auto_greet_gap」里清掉——**双向都对上了**。

- 命令**定义了没注册** → 前端 `invoke` 报「命令不存在」，其余一切正常；
- 命令**注册了但前端没写封装** → 没人调，不影响；
- 改了命令名但只改了一侧 → 同上，运行期才炸。

实测口径（2026-11-21 `bb9d16c`：`Select-String '#\[tauri::command\]'` = 48 / `lib.rs:473-522` = 48）：
若你数出来的不是 48/48，说明有人动过了，**以你数到的为准并同步本文档**。
两个数一旦不等，先查"是不是有人只加了命令没注册"，而不是先怀疑统计脚本。

## 铁律 4：数据库列是第三份手写副本

`schema.rs` 的 `SCHEMA`（:13-47 建 `job_queue`，:49-56 `jd_decisions`，:58- 起 `sensitive_jobs`）
与 `sqlite.rs` 的运行时补列是**两处**清单，都不是编译期可校验的：

- `SCHEMA_COLUMN_DECLS`（`schema.rs:177-205`）→ `upgrade_column_decls()`（:104-112）→
  `add_missing_columns`（`sqlite.rs:79-97`，`PRAGMA table_info` 先查再 `ALTER TABLE ADD COLUMN`）；
- `SCHEMA_INDEXES`（`schema.rs:87-92`）**必须在补列之后执行**（:81-86 注释：
  否则老库 `no such column: match_score` 让应用打不开）；
- 新增列要同时进 `SCHEMA` 与 `SCHEMA_COLUMN_DECLS`——`schema.rs:140-144` 的 `score_scale`
  注释写明了漏两处各自的表现（新建库缺列 → 写入报 `no such column`；旧库缺列 → 补列漏了同样失败）；
- `jd_decisions` 的补列单独一份：`DECISION_UPGRADE_COLUMNS`（:152）→ `upgrade_decision_columns()`（:155-157）。

Python 侧**不读 schema**，它只读写同一个 `.db`，所以数据库契约的"对端"只有
`sqlite.rs` 与 `schema.rs`（`schema.rs:1-7` 注释：真实库实测 31 列，`SCHEMA` 只声明 22 列）。

## 前后端字段对照（改这两个结构体前必读）

### `DecisionRecord`

Rust 真源：`app/src-tauri/src/core/domain/store_ports.rs:169-181`
（`#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]`）

| 字段 | Rust 类型（store_ports.rs） | 前端类型（types.rs:435-452） | 备注 |
|---|---|---|---|
| `id` | `i64` | `i64` | 分页游标 |
| `platform` | `Option<String>` | `String`（`#[serde(default)]`） | Rust 可能给 `null`，前端吃成空串 |
| `company_job` | `Option<String>` | `String`（同上） | 格式 `公司/岗位`（`ingest.rs:259` 注释：前端按最后一个 `/` 拆分） |
| `decision` | `Option<String>` | `String`（同上） | `accept` / `reject` / `skip`（`ingest.rs:261`）+ 另有 `rescore` 一种来源（重算审计） |
| `reason` | `Option<String>` | `String`（同上） | 第一段是稳定规则标识（见下） |
| `created_at` | `i64` | `i64` | **秒**（前端 `features/logs/state.rs:8` 注释：与 `LogRow.ts` 毫秒不同） |
| `old_score` | `Option<u8>` | `Option<u8>` | 旧行 `NULL` |
| `new_score` | `Option<u8>` | `Option<u8>` | 旧行 `NULL`；`score_change()`（store_ports.rs:186-193）负责 `"o → n"` / `"→ n"` / `"o →"` / `None` |

**不对称点（合法）**：Rust 四个文本字段是 `Option`，前端是 `String` + `#[serde(default)]`。
改任何一侧都要两边一起看：前端若改成 `Option<String>`，视图层所有 `.clone()` 都要跟着改。

**`reason` 的第一段是稳定标识**（不是给人看的文案）：
`ingest.rs:26-28`、`:346-367` `rule_ident()` 维护一张 16 项白名单
（`weekend`/`name_word`/`short_term`/`outsource`/`degree`/`degree_985`/`experience`/
`hr_active_unknown`/`hr_inactive`/`founded`/`salary_floor`/`salary_ceiling`/
`prescan`/`sensitive`/`duplicate`/`exclude`），认不出来的一律落 `"rule"`。
新增排除规则时，若忘了往 `rule_ident` 的 `KNOWN` 表里加名，决策日志会显示 `"rule"` 而不是具体规则——**不报错**。

`Prescan` 的标识口径在 `ingest.rs:404-411`：`duplicate` / `sensitive:stored` / `sensitive:rejected` /
`prescan:<规则名>`；`session.rs:528-534` 把 `pre.rule_ident()` 塞进 `IngestVerdict::Rejected.rule`
（注释：曾经一律写死 `"prescan"`，看不出命中哪条规则）。

### `AppConfigReply`

Rust 真源：`app/src-tauri/src/commands_config.rs:36-66`，**22 个字段**，全部 `String`/数值/`serde_json::Value`：

| # | 字段 | 类型 | 说明 |
|---|---|---|---|
| 1 | `base_url` | `String` | |
| 2 | `model` | `String` | |
| 3 | `api_key_hint` | `String` | `****abcd`，未配置时空串 |
| 4 | `has_key` | `bool` | |
| 5 | `theme` | `String` | |
| 6 | `score_mode` | `String` | `rule` / `llm` |
| 7 | `llm_prefilter` | `u8` | 规则初筛线 |
| 8 | `max_jd` | `u32` | 派生值，由 Rust 算好 |
| 9 | `min_score` | `u8` | 派生值 |
| 10 | `score_max` | `u8` | 派生值（**前端不得硬编码 100**） |
| 11 | `greet_prompt` | `String` | |
| 12 | `greet_prompt_default` | `String` | 「恢复默认」按钮用 |
| 13 | `skills` | `String` | |
| 14 | `exclude_keywords` | `String` | |
| 15 | `exclude_rules` | `String` | |
| 16 | `resume_path` | `String` | |
| 17 | `data_dir` | `String` | 绝对路径 |
| 18 | `last_filter` | `String` | |
| 19 | `tokens_total` | `i64` | |
| 20 | `tokens_greet` | `i64` | |
| 21 | `tokens_score` | `i64` | |
| 22 | `scrape_defaults` | `serde_json::Value` | 14 项聚合；没配过是 `{}` 不是 `null` |

写入方向是 `set_app_config` 的 `patch`（`commands_config.rs:188-213` `apply_patch`）：
- 只写 `WRITABLE_KEYS`（`core/domain/app_config.rs:74`），`min_score`/`data_dir`/`tokens_*` 一律忽略；
  `KEY_MIN_SCORE` 刻意不在白名单里（`app_config.rs:104-106`）——门槛只能走 `set_limit`；
- 值以 `****` 开头视为"未修改的掩码"，跳过（`commands_config.rs:200-202`）；
- `null` 表示删除该键（:198）。
- 契约：`set_limit` 的 `max_jd` 上限 `MAX_JD_CAP = 500`（`app_config.rs:162`），
  `min_score` 不得超 `score_max`（`resolve_limit` :175-189），越界返回中文错误串。

## 契约熔断流程（`AGENTS.md` 第 5 节）

触发条件（任一）：

1. `core/sidecar/protocol.rs` 与 `scraper/sidecar.py` 的帧形状不一致；
2. `commands_*.rs` 与 `frontend/src/api/types.rs` 的字段不一致；
3. SPEC/文档与实际行为冲突。

处置顺序（一步一步，不要跳）：

1. **立刻停止修改业务代码**——不要"先改一半再说"，这时候改什么都会被熔断覆盖；
2. `git status` 汇报未提交改动范围（哪个文件、几个 hunk）；
3. 把冲突点写成"真源 vs 对端 vs 实际"三列，交给用户裁决保留或回滚；
4. 切回架构模式**先改文档**，文档定稿后才恢复编码。

本项目已经用这套流程处理过一次 `LineParse` / `route_reply` 事故
（`app/src-tauri/src/core/sidecar/process/codec.rs:38-43`：把 `Ignore` 当 `Malformed`，
界面上每条命令回执都冒"无法解析"，而单测全绿）。

## 一条命令自查契约漂移

在仓库根跑（只读，不改任何文件）：

```powershell
# 1) 命令真源 vs 注册表：定义数与注册项数必须对得上，差的那条要看是谁
(Select-String -Path app/src-tauri/src/*.rs -Pattern '#\[tauri::command\]').Count
#   2026-11-21 实测 48
$h = (Get-Content app/src-tauri/src/lib.rs)[472..521]          # 473..522 行（0 基切片）
@($h | Where-Object { $_ -match '^\s+[\w:]+,\s*$' }).Count       # 同一次实测 48

# 2) 命令真源 vs 前端封装：前端出现了 Rust 里没注册的命令名
$joined = $h -join "`n"
$front = (Select-String -Path frontend/src/api/mod.rs -Pattern 'invoke\(\s*"(\w+)"' -AllMatches).Matches |
         ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique
$front | Where-Object { $joined -notmatch ("\b" + $_ + "\b") }
#   输出为空 = 未漂移（实测 31 个前端命令名，0 个缺失）

# 3) 入参字段漏 alias：跨 2 行取上下文，排除 Serialize 的返回结构体
Select-String -Path app/src-tauri/src/commands_*.rs,app/src-tauri/src/core/domain/app_config.rs `
  -Pattern 'pub \w+_\w+:' -Context 2,0 |
  Where-Object { ($_.Context.PreContext -join ' ') -notmatch 'alias|Serialize' } |
  ForEach-Object { "$([IO.Path]::GetFileName($_.Path)):$($_.LineNumber) $($_.Line.Trim())" }
```

> 第 2 条必须先把切片 `-join "\`n"` 再匹配：PowerShell 的 `-notmatch` 作用在**数组**上时
> 返回的是"过滤后的元素数组"，配合 `Where-Object` 会把每个命令名都判成缺失（我实测踩过，
> 输出 31 个假阳性）。切片下标也要注意：`473..522` 行对应 `[472..521]`。

第 3 条在 2026-11-21 输出的 50 条**全部是误报**，原因是：
`-Context` 前一行是文档注释、`#[derive(Debug, Serialize)]` 或空行，
而真正带 `alias` 的那一行在**上上行**（例如 `ScrapeArgs.min_score` 的 alias 在 `commands_scrape.rs:174`，
字段在 :175，前一行 :174 确实有 alias——所以它没被列出，说明 `-Context 2,0` 对单行注释结构有效）。
剩下被列出的都是 `Serialize` 的返回结构与 `#[derive(Debug, Clone)]` 的纯输出结构体。
实战用法：**先人工确认输出里的每一项是不是入参结构体**，入参才需要 alias。
真正的信号组合是"前端用驼峰发 + Rust 是蛇形 + 无 alias"，前端驼峰发送点这样列：

```powershell
Select-String -Path frontend/src/api/mod.rs -Pattern '"[a-z]+[A-Z]\w*"\s*:'
```

结果（2026-11-21 实测）里出现的驼峰键只有这些：
`jobId`、`jobIds`、`maxJd`、`minScore`、`afterJobId`、`afterId`、`resumeId`、`useLlm`、`llmPrefilter`、
`logChanges`；其余（`platform`/`keywords`/`city`/`salary`/`experience`/`degree`/`industry`/`title`/
`company`/`jd`/`uid`/`text`/`reason`/`since`/`limit`/`patch`/`ids`/`args`）与 Rust 字段同名或走散参。
`include_rejected` / `rejected_only` / `company_scale` / `finance_stage` / `job_type` / `max_pages` /
`exclude_keywords` / `hr_active` / `work_weekend` / `jd_degree` / `min_years` / `city`/`salary`/`experience`/
`degree`/`industry` 的 alias（`commands_db.rs:112-118`、`app_config.rs:202-228`）**是给旧前端或手工调试用的兜底**，
当前 `api/mod.rs` 走的是 `types.rs` 里的蛇形结构体——两边都通，正是"双向兼容"的含义。

落差检查 = `commands_db.rs` 里所有 alias 名，减去上面这 10 个前端实际在发的驼峰键，
剩下的就是"埋着但当前无人使用"的兜底 alias（无害，**但不要因为看起来没人用就删掉**）。

## Non-Negotiables

1. 改 `commands_*.rs` 的字段 → 同一次提交改 `frontend/src/api/types.rs` 与 `mod.rs`。
2. 新增驼峰发送的参数 → 真源侧加 `#[serde(default, alias = "驼峰名")]` 并加一条双写反序列化测试。
3. 参数结构体 → 前端 `invoke(cmd, {"args": {...}})`；散参 → 平铺。不要凭感觉。
4. 新命令必须同时出现在 `lib.rs:473-522` 与前端的 `mod.rs`（或明确记录"故意不开放给前端"）。
5. 改 `schema.rs` 的列 → 同一次提交改 `SCHEMA_COLUMN_DECLS`（或 `DECISION_UPGRADE_COLUMNS`），
   索引仍留在 `SCHEMA_INDEXES` 里、在补列之后执行。
6. `api_key` 永远不出 Rust：只回 `api_key_hint`；`set_app_config` 的 patch 里带掩码会被跳过，但那是兜底不是许可。
7. `min_score` / `score_max` 由 `get_scoring_spec` 与 `get_app_config` 下发，前端不得硬编码。
8. 新增排除规则 → 同时更新 `rules.rs:190` 的分支与 `ingest.rs:346-367` 的 `KNOWN` 表。
9. 时间单位不许混：`LogRow.ts` 是毫秒、`DecisionRecord.created_at` 是秒、sidecar 协议不传时间（由 Rust 盖）。
10. 契约对不上就走熔断流程，不要在漂移状态下继续写业务代码。
