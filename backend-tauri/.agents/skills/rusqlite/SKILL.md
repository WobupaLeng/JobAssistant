---
name: rusqlite
description: SQLite persistence work in the jobassistant-app crate via rusqlite 0.32 bundled - connection lifecycle, PRAGMA choices, schema upgrades, migrations, transactions, and the NULL-ordering trap in SQL sorting. Use when touching backend-tauri/src/core/infra/sqlite*, adding a column or index, writing a migration, writing ORDER BY over nullable columns (salary), or writing store ports that a sidecar ingest path calls. Not for domain scoring or rule logic (use rust-domain-purity) and not for HTTP or Tauri wiring (use rust-tauri).
---

# rusqlite — 持久层（`core::infra::sqlite`）

导语：本项目**没有 ORM、没有连接池、没有 `-wal` 文件**。全部数据在一个 `.db` 文件里，
由一个 `Mutex<Connection>` 串行化。这个选择让「复制一个文件就是完整备份」成立，
代价是——**任何写死在代码里的 SQL 都是不可回滚的**，所以下面每条铁律都对应一次真实事故。

> 版本锚点：`rusqlite = { version = "0.32", features = ["bundled"] }`（SQLite 编进二进制，
> 目标机不需要装 SQLite）；数据库位置 `app_local_data_dir/job_assistant.db`（`lib.rs:426-472` 的 setup）。

---

## Routing Trigger

- 改 `core/infra/sqlite.rs`、`core/infra/sqlite/{admin,jobs,row,sensitive}.rs`
- 加表 / 加列 / 加索引 / 写迁移
- 写或改 `ORDER BY`、分页游标、`LIMIT`
- 新增 `JobStore` / `JobAdminStore` / `SensitiveAdminStore` / `DecisionStore` / `ConfigStore` 的实现方法
- 想「在真实库上验一下」

不读本 skill：纯领域规则（`rust-domain-purity`）、命令层接线（`rust-tauri`）。

---

## The Reference Map

| 要读什么 | 文件:行 | 读它为了 |
|---|---|---|
| 打开连接、PRAGMA、可写性检查 | `backend-tauri/src/core/infra/sqlite.rs:113-138` | `open()`：`journal_mode=DELETE`(129)、`busy_timeout(5s)`(131) |
| 单测用内存库 | `core/infra/sqlite.rs:141-149` | `open_in_memory()` + 同一套 `SCHEMA` |
| 补列的通用实现 | `core/infra/sqlite.rs:74-97` | `add_missing_columns`：`PRAGMA table_info` → `ALTER TABLE ADD COLUMN` |
| 补列 + 重建索引 | `core/infra/sqlite.rs:157-178` | `upgrade_columns()`，末尾 `REINDEX`(176) |
| 迁移必须在单事务里 | `core/infra/sqlite.rs:184-225` | `unchecked_transaction()`(201) 与 ×100 事故注释(188-190) |
| 补列声明表 | `core/domain/schema.rs:140-176` | `DECISION_UPGRADE_COLUMNS`(147-157)、`SCHEMA_COLUMN_DECLS`(159-176) |
| 排序表达式 | `core/domain/job.rs:342-369` | `SALARY_K_EXPR`、`SALARY_UNKNOWN_RANK_EXPR` |
| 排序拼接 | `core/infra/sqlite/jobs.rs:92-114` | 薪资排序键必须排第一且恒 ASC |
| 游标分页 | `core/infra/sqlite/jobs.rs:151-162`（SQL 在 `:157`） | `WHERE job_id > ?1 ORDER BY job_id ASC LIMIT ?2` |
| 第二个事务点（批量写） | `core/infra/sqlite/admin.rs:338-358` | `sync_stored` 整段一个事务；`:338-341` 拒绝超 `MAX_BATCH_IDS`(57) |
| 存储层测试范本 | `core/infra/sqlite/tests.rs` | 50 个用例：排序、游标、补列、迁移 |

---

## Non-Negotiables（每条都对应一次真实故障）

1. **`open()` 里两个 PRAGMA 都不能删。**
   - `conn.pragma_update(None, "journal_mode", "DELETE")`（`sqlite.rs:129`）：
     **刻意不用 WAL**。理由写在 `sqlite.rs:120-128`——WAL 模式下主库与 `-wal` 会不一致，
     而本项目的备份 / 迁移 / 排障全靠「单文件即全部数据」。
   - `conn.busy_timeout(Duration::from_secs(5))`（`sqlite.rs:131`）：
     没有它，两个线程同时写会立刻 `SQLITE_BUSY` 而不是等 5 秒。
   - 顺序也是约定：`open` → PRAGMA → `execute_batch(SCHEMA)`(132) → `upgrade_columns()`(136)。

2. **⚠ `PRAGMA foreign_keys` 本项目没有开，也没有外键。**
   `core/domain/schema.rs` 里没有一处 `FOREIGN KEY` / `REFERENCES`（只有 4 处 `PRIMARY KEY`：
   `schema.rs:15/50/59/76`），`core/infra/` 里也没有任何 `foreign_keys` 调用。
   **这是现状，不是遗漏**：表之间靠 `job_id` 逻辑关联，级联由代码保证。
   → 不要「顺手」加上 `PRAGMA foreign_keys = ON`：开启后既有数据里的孤儿行会让写入整条失败。
   要加外键，先当作一次独立的 schema 变更来设计（含孤儿数据清洗）。

3. **迁移必须在单个事务里跑完。**
   `sqlite.rs:201` 用 `conn.unchecked_transaction()`（需要在 `&Connection` 上开事务，
   因为 `migrate(&self)` 拿不到 `&mut`），到 `sqlite.rs:223` 才 `tx.commit()`。
   事故原文（`sqlite.rs:188-190`）：此前逐条 `conn.execute()` 自动提交，若崩在
   「分数已乘 10、标记仍是 `'10'`」之间，**重跑会变成 ×100，且不可逆**。
   → 任何多步写（迁移、批量同步）都照 `sqlite.rs:184-225` 与 `admin.rs:354-358` 的形态写。

4. **补列用「先查再加」，绝不猜。**
   `add_missing_columns(conn, table, decls)`（`sqlite.rs:74-97`）：
   `PRAGMA table_info({table})`(85) 取已存在列名 → 已存在就 `continue`(90) → 否则
   `ALTER TABLE {table} ADD COLUMN {name} {decl}`(93)。
   拼进去的全部是**编译期常量**，所以字符串拼接在这里是安全的（`sqlite.rs:78` 注释）。
   声明表在 `schema.rs`：`SCHEMA_COLUMN_DECLS`(177-205)、`UPGRADE_COLUMNS`(115-145)、
   `DECISION_UPGRADE_COLUMNS`(152)、`upgrade_decision_columns()`(155-157)。
   → 加列的完整动作是**两处都改**：建表语句 `SCHEMA` **和**补列声明表
   （`schema.rs:140-143` 的注释就是这么要求的）；漏了后者，老库永远不会长出新列。
   `upgrade_columns()`(157-178) 末尾的 `REINDEX`(176) 也不能删：历史上索引损坏
   （`wrong # of entries in index idx_job_queue_score`）让所有查询直接失败（注释 173-175）。

5. **薪资排序的 NULL 语义与 MySQL / PostgreSQL 相反。**
   SQLite 的 `ORDER BY x ASC` 把 NULL 排**最前**，`DESC` 排最后。
   而「面议 / 空」在本项目里应当**永远垫底**。所以：
   - `SALARY_UNKNOWN_RANK_EXPR`（`job.rs:367-369`）：
     `CASE WHEN salary IS NULL OR TRIM(salary) = '' OR salary LIKE '%面议%' THEN 1 ELSE 0 END`
   - 它**必须恒 ASC，且必须是第一排序键**（注释 `job.rs:363-366`）：
     `ORDER BY {SALARY_UNKNOWN_RANK_EXPR} ASC, {SALARY_K_EXPR} {dir}, job_id {dir}`（`jobs.rs:100-104`）。
     让它跟着用户选的 `dir` 翻转，用户一点「降序」面议就会跳到最上面。
   - 非表达式列走 `jobs.rs:105-114` 的分支，但仍要 `job_id {dir}` 兜底：同分没有稳定次序
     （注释 106-109）。
   - 有效用例：`tests.rs:76`（`("d", "面议")` 注释「未知，排最后」）、`:104-105`
     （`assert_eq!(&ids[4..], &["d", "f"], "面议/空应排最后")`）。

6. **游标分页只能按 `job_id` 升序。**
   `jobs.rs:151-162`：`WHERE job_id > ?1 ORDER BY job_id ASC LIMIT ?2`。
   改成 `ORDER BY match_score`，全量重算的写回会改动 `match_score`，
   从而推翻后续页的边界——**同一条被算两次、别的被跳过，且没有任何报错**
   （trait 注释 `domain/job.rs:125-145`；用例 `tests.rs:141`
   `list_scored_ids_pages_by_job_id_cursor`、`tests.rs:168`
   `list_scored_ids_ignores_score_so_paging_is_stable`）。
   顺便：`limit` 不能传 0，`rescore` 侧用 `.max(1)` 兜底（`domain/rescore.rs:110`）。

7. **写库失败不得计入 `inserted`。**
   入库计数只在 `core/sidecar/session.rs::handle_job`（`:502` 起）里分类与记账：
   `:548-552` 定初值（0 入库 / 3 规则否决 / 2 门槛）、`:557-570` `put` 返回 `Err` 时 bucket=4、
   `:599-614` 五个分支记账——**bucket=4 只写 `last_log`，不加 `inserted`**（`:565-566` 注释：
   「否则用户会以为『抓到了 10 条却一条没进』」）。
   用例锁死在此：`session.rs:724`（`store_error_does_not_count_as_inserted`）、
   `session.rs:690`（`duplicate_is_counted_separately_from_inserted`）。
   → 在存储层「吞掉错误返回 Ok」等于把失败伪装成入库，是把这条铁律从背后捅穿。

8. **严禁对真实库做破坏性验证。**
   `data/` 里含登录 cookie 与 Chrome profile，`data/*.db` 是用户真实数据
   （根 `AGENTS.md` 2.5 与 6.1）。验证迁移 / 写入只能：
   - 单测用 `Connection::open_in_memory()`（`sqlite.rs:141-149`，同一套 `SCHEMA` + `upgrade_columns`），或
   - 用临时目录造一个假库（范本 `tests.rs:508-516`：`std::env::temp_dir().join(format!("aja_test_{}", process::id()))`，
     用完即删），或
   - 真库必须 `Copy-Item` 到 `data/backup/` 并核对 SHA256 后再动副本。

9. **连接是 `Mutex<Connection>`，不是连接池。**
   `sqlite.rs:103-106`（注释 101-102 说明取舍）。所以：
   - 不要为了「并发」在里面再套一层 spawn；
   - 跨线程持有 `Arc<SqliteStore>` 是设计允许的：`core/scrape.rs:90` 的 `make_sink(Arc<SqliteStore>)`
     把它移进消费线程，从而**不占用壳层 `StoreState` 的锁**，用户抓取中打开列表不会被写阻塞。

---

## Workflow

### 加一列

1. 在 `core/domain/schema.rs` 的 `SCHEMA` 建表语句里加列。
2. 同步加进 `SCHEMA_COLUMN_DECLS` / `DECISION_UPGRADE_COLUMNS`（两处都改，见铁律 4）。
3. `core/infra/sqlite/row.rs` 的 `SELECT_COLUMNS`(15-20) 与 `row_to_job`(23-) **两边同步加**——
   列顺序只有这一处定义，漏一边的症状是运行期 `InvalidColumnName`（`row.rs:3-5`、`:13-14` 注释）。
4. 加一条「老表补列」测试，形态照 `tests.rs:508-516`（临时目录 + 最小旧表 + `upgrade_columns`）。

### 加一个排序字段

1. 在 `domain/job.rs` 的 `SortField` 加分支（`column()` 371-380 / `parse()` 387-389）。
2. 若是表达式列（像薪资），在 `jobs.rs:100-104` 的表达式分支里拼，
   **先确认「未知值」是否要垫底**——要垫底就得有 rank 表达式且恒 ASC 打头。
3. 补一条排序测试（形态照 `tests.rs:67-80`）。

### 写一次迁移

1. 放在 `sqlite.rs` 的迁移函数里，**先 `let tx = conn.unchecked_transaction()?;`**。
2. 每一步操作用 `tx.execute`，全部成功才 `tx.commit()`。
3. 迁移必须**幂等**：重跑不改变结果（范本是 `domain/migration.rs` 的十分制→百分制，9 个用例）。
4. 加「崩在中途重跑」的等价测试：手工造一个「半迁移」状态的库，跑两次，断言结果一致。

---

## 本模块实证速查

| 现象 | 真因 | 依据 |
|---|---|---|
| 「降序」时面议岗位跑到最上面 | NULL 在 SQLite 里 ASC 排最前 | `job.rs:362-369`、`jobs.rs:100-104` |
| 全量重算漏算 / 重复算，无报错 | 游标按 `match_score` 排序 | `jobs.rs:151-165`、`job.rs:125-137` |
| 迁移重跑后分数变成 ×100 | 迁移没包事务，崩在中途 | `sqlite.rs:188-190`、`:201` |
| 老库缺新列，新装用户正常 | 只改了 `SCHEMA`，没加进补列表 | `schema.rs:140-143`、`sqlite.rs:74-97` |
| 所有查询突然失败 | 索引损坏，缺 `REINDEX` | `sqlite.rs:173-176` |
| 备份 / 迁移后数据对不上 | 有人把 `journal_mode` 改成了 WAL | `sqlite.rs:120-129` |
| 并发写直接报 SQLITE_BUSY | `busy_timeout` 被删 | `sqlite.rs:131` |
| 计数显示入库但列表里没有 | 存储层把 Err 吞成了 Ok | `session.rs:549-562`、`:589-607` |
