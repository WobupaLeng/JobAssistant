//! SQLite 实现（基础设施层）。
//!
//! 这是 `JobStore` 端口的唯一实现。domain 不知道它的存在，app 通过 trait 使用它。
//! 文件路径由调用方传入，本层不猜测数据目录——那是 app 的职责。
//!
//! **按聚合根拆分**（2026-11-21，纯搬移 + 可见性调整，行为零变化）：
//!
//! | 模块 | 内容 |
//! |---|---|
//! | `row` | 列白名单 `SELECT_COLUMNS` + 行映射 `row_to_job` / `row_to_sensitive_hit` + `now_secs` |
//! | `jobs` | `JobStore` 端口实现（主链路）+ `job_queue` 筛选条件 |
//! | `sensitive` | `SensitiveStore` 端口实现（卡片阶段查重） |
//! | `admin` | `JobAdminStore` / `SensitiveAdminStore` / `DecisionStore` / `ConfigStore` |
//! | `tests` | 单元测试 |
//!
//! 对外路径 `infra::sqlite::SqliteStore` 与全部公开方法签名保持不变：
//! `conn` 字段是 `pub(super)`，各聚合根模块在同一父子树内直接借用它。

pub(crate) mod admin;
mod chat_store;
mod jobs;
mod row;
pub use chat_store::{ChatFriendRow, ChatMessageRow};
mod sensitive;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::core::domain::schema::{CHAT_SCHEMA, CHAT_SCHEMA_INDEXES, SCHEMA, SCHEMA_INDEXES};

/// 列白名单与时间戳的唯一来源。各聚合根模块与 `tests` 都经这条通道取用。
#[allow(unused_imports)]
pub(crate) use row::{now_secs, SELECT_COLUMNS};

/// 存储层错误。
#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    /// 数据库文件无法打开，通常意味着目录不存在或权限不足
    Open { path: String, source: rusqlite::Error },
    /// 批量操作的 id 数量超过 [`MAX_BATCH_IDS`]
    BatchTooLarge { got: usize, max: usize },
    /// JSON 序列化失败（写配置项时理论上不会发生）
    Json(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sql(e) => write!(f, "SQLite 错误：{e}"),
            StoreError::Open { path, source } => {
                write!(f, "无法打开数据库 {path}：{source}")
            }
            StoreError::BatchTooLarge { got, max } => {
                write!(f, "批量操作 id 数量 {got} 超过上限 {max}")
            }
            StoreError::Json(e) => write!(f, "JSON 序列化失败：{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sql(e)
    }
}

pub(super) type Result<T> = std::result::Result<T, StoreError>;

/// 给 `table` 补齐 `decls` 里声明的列（SQLite 无 `ADD COLUMN IF NOT EXISTS`）。
///
/// 必须先 `PRAGMA table_info` 查再补：直接 `ALTER TABLE ADD COLUMN` 在列已存在时
/// 报 `duplicate column name`，而旧库里这些列多半已存在。
/// `table` / `name` / `decl` 全部是编译期常量，不来自用户输入，拼接安全。
fn add_missing_columns(
    conn: &Connection,
    table: &str,
    decls: Vec<(&'static str, &'static str)>,
) -> Result<()> {
    let mut existing: Vec<String> = {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (name, decl) in decls {
        if existing.iter().any(|c| c == name) {
            continue;
        }
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {name} {decl}"), [])?;
        existing.push(name.to_string());
    }
    Ok(())
}

/// SQLite 岗位存储。
///
/// 内部用 `Mutex` 而非连接池：桌面应用只有一条主线程写路径，
/// 连接池带来的复杂度（连接生命周期、WAL 争用）在这里不划算。
pub struct SqliteStore {
    /// 各聚合根模块（`jobs` / `sensitive` / `admin`）直接借用这条连接。
    pub(super) conn: Mutex<Connection>,
}

impl SqliteStore {
    /// 打开（或创建）数据库并建表，随后补齐旧库缺失的列。
    ///
    /// 建表用 `IF NOT EXISTS`，因此对已存在的旧库是安全的：
    /// 不会删表，也不会覆盖数据。补列由 [`Self::upgrade_columns`] 负责。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let display = path.display().to_string();
        let conn = Connection::open(path).map_err(|source| StoreError::Open {
            path: display,
            source,
        })?;
        // 显式固定为 DELETE 日志模式，**不要**用 WAL。
        //
        // 起因是一次真实事故：旧库被别的连接以 WAL 模式打开过，主库文件与
        // `-wal` 是两个不一致的残缺部分。用户把 `.db` 拷来拷去（备份、换机、
        // 我们自己的迁移验证），只拷主库就会读到「表还在、行数为 0」的空壳，
        // 甚至报 `database disk image is malformed`。
        //
        // DELETE 模式下每次提交都写回主库，单个 `.db` 文件即是完整数据，
        // 这才配得上这个应用「一个文件就是全部数据」的定位。
        conn.pragma_update(None, "journal_mode", "DELETE")?;
        // 存在多连接时（比如用户同时开着备份工具）不立即失败，等一会儿
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(CHAT_SCHEMA)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.upgrade_columns()?;
        Ok(store)
    }

    /// 内存库，仅用于测试。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(CHAT_SCHEMA)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.upgrade_columns()?;
        Ok(store)
    }

    /// 补齐旧库缺失的列（SQLite 无 `ADD COLUMN IF NOT EXISTS`）。
    ///
    /// 必须用 `PRAGMA table_info` 先查再补：直接 `ALTER TABLE ADD COLUMN`
    /// 在列已存在时会报 `duplicate column name`，而旧库里这些列多半已存在。
    ///
    /// 补列完成后才建索引——索引引用的列刚由本步骤保证存在。
    fn upgrade_columns(&self) -> Result<()> {
        let conn = self.conn.lock().expect("存储锁中毒");
        add_missing_columns(
            &conn,
            "job_queue",
            crate::core::domain::schema::upgrade_column_decls(),
        )?;
        // 决策表同样要补列：新版界面显示「旧分 → 新分」，旧库没有这两列。
        // 只做 `ADD COLUMN`（可空），不动任何既有数据。
        add_missing_columns(
            &conn,
            "jd_decisions",
            crate::core::domain::schema::upgrade_decision_columns(),
        )?;

        conn.execute_batch(SCHEMA_INDEXES)?;
        conn.execute_batch(CHAT_SCHEMA_INDEXES)?;
        // 索引可能与表失去同步（历史上真的发生过：`wrong # of entries in
        // index idx_job_queue_score` 让所有查询直接失败）。`REINDEX` 会按主库
        // 现有数据重建全部索引，代价是启动时扫一遍表——表很小，可接受。
        conn.execute_batch("REINDEX")?;
        Ok(())
    }

    /// 执行十分制→百分制迁移。幂等：重复调用无副作用。
    ///
    /// 返回 `true` 表示本次确实做了换算。
    ///
    /// **注意**：第 1 条语句是 `ALTER TABLE ADD COLUMN score_scale`，
    /// 而建表语句现在也包含该列，因此这里必须先查 `PRAGMA table_info`，
    /// 只在列确实缺失时执行加列，否则报 `duplicate column name`。
    ///
    /// **必须在单个事务内完成**（2026-11-21 修）：此前逐条 `conn.execute()` 自动提交，
    /// 若崩在「分数已乘 10、标记仍是 '10'」之间，重跑会变成 ×100，且不可逆。
    /// 现在整段包在一个写事务里，失败即整体回滚。
    pub fn migrate(&self) -> Result<bool> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let has_scale = {
            let mut stmt = conn.prepare("PRAGMA table_info(job_queue)")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .any(|c| c == "score_scale")
        };

        let tx = conn.unchecked_transaction()?;
        let mut changed = false;
        for (i, stmt) in crate::core::domain::migration::migration_statements()
            .into_iter()
            .enumerate()
        {
            // 第 0 条是加列语句，列已存在时跳过
            if i == 0 && has_scale {
                continue;
            }
            let affected = tx.execute(stmt, [])?;
            // 换算语句会改动行数，加列/打标记也可能改动；任一非零即视为变更
            if affected > 0 {
                changed = true;
            }
        }
        // 配置门槛同样要换算（`min_score` / `llm_prefilter`）。
        // 此前 `convert_legacy_threshold` 没有任何调用点，`llm_prefilter = 5`
        // 会永远停在十分制，导致 LLM 初筛线实际按 5 分执行。
        if migrate_config_thresholds(&tx)? {
            changed = true;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// 统计各分数区间的岗位数，供 UI 展示。
    pub fn count_by_scale(&self) -> Result<Vec<(String, i64)>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let mut stmt = conn.prepare(
            "SELECT COALESCE(score_scale, 'null'), COUNT(*) FROM job_queue GROUP BY score_scale",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// 读出整张 `app_config` 表。
    ///
    /// 沿用 Python 时代的「键值对平表」结构，这里只做原样搬运——键名语义
    /// 属于调用方（外壳层 / 侧车），core 不解释具体内容。
    pub fn config_pairs(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let mut stmt = conn.prepare("SELECT key, value FROM app_config")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                // value 允许为 NULL（老数据），统一成空串，避免调用方到处判空
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }
}

/// 把 `app_config` 里以十分制存的门槛换算成百分制。
///
/// 只处理 `SCALE_CONFIG_KEYS` 列出的键，且只在该键存在时改写；
/// `convert_legacy_threshold` 返回 `None`（已大于 10 / 非数字 / 空）时不动。
/// 返回 `true` 表示至少改写了一个键。
fn migrate_config_thresholds(tx: &rusqlite::Transaction<'_>) -> Result<bool> {
    let mut changed = false;
    for (key, _) in crate::core::domain::migration::SCALE_CONFIG_KEYS {
        let current: Option<String> = tx
            .query_row("SELECT value FROM app_config WHERE key = ?1", [key], |r| {
                Ok(r.get::<_, Option<String>>(0)?)
            })
            .ok()
            .flatten();
        let Some(raw) = current else { continue };
        if let Some(new_value) = crate::core::domain::migration::convert_legacy_threshold(&raw) {
            tx.execute(
                "UPDATE app_config SET value = ?1 WHERE key = ?2",
                rusqlite::params![new_value, key],
            )?;
            changed = true;
        }
    }
    Ok(changed)
}
