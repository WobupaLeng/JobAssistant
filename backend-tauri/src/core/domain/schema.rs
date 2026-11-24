//! 存储层 schema（`backend/app/store.py:18 SCHEMA` 的平移）。
//!
//! **兼容性铁律**：Rust 与 Python 读写**同一个库文件**，因此：
//! - 建表语句必须与 Python 逐字等价，否则旧库上 `CREATE TABLE IF NOT EXISTS`
//!   会被跳过，而新库会被建成不同结构，导致两端行为分叉；
//! - 补列逻辑必须保留：真实库实测有 31 列，而 `SCHEMA` 只声明 22 列，
//!   差额由 `_UPGRADE_COLUMNS`（即本文件的 [`UPGRADE_COLUMNS`]）补齐。

/// 建表语句，与 `store.py:18` 等价。
///
/// 注意 `founded_year` 与 `founded_years` **两个列都存在**：前者是早期遗留，
/// 后者是当前使用的。清理旧列需要数据迁移，暂不处理，但新增列时不要误以为重复。
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS job_queue (
    job_id            TEXT PRIMARY KEY,
    platform          TEXT NOT NULL,
    title             TEXT,
    company           TEXT,
    salary            TEXT,
    status            TEXT NOT NULL DEFAULT 'pending',
    created_at        INTEGER NOT NULL,

    -- 以下为打分需要的 JD / 公司侧扩展字段
    job_url           TEXT,
    jd_text           TEXT,
    location          TEXT,
    experience        TEXT,
    degree            TEXT,
    hr_name           TEXT,
    hr_position       TEXT,
    hr_active_status  TEXT,
    company_industry  TEXT,
    company_scale     TEXT,
    company_stage     TEXT,
    company_address   TEXT,
    company_intro     TEXT,
    hiring_count      INTEGER,
    founded_years     INTEGER,
    match_score       INTEGER,
    rejected          INTEGER DEFAULT 0,
    score_source      TEXT,
    score_reason      TEXT,
    updated_at        INTEGER,
    -- 默认 '10'：若旧库已存在则本行不生效（CREATE TABLE IF NOT EXISTS 被跳过），
    -- 由 UPGRADE_COLUMNS 补列与 migration.rs 的换算负责。
    score_scale       TEXT DEFAULT '10'
);

CREATE TABLE IF NOT EXISTS jd_decisions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    platform    TEXT,
    company_job TEXT,
    decision    TEXT,
    reason      TEXT,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sensitive_jobs (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    platform       TEXT NOT NULL,
    job_id         TEXT,
    company        TEXT,
    title          TEXT,
    salary         TEXT,
    location       TEXT,
    reason         TEXT,
    job_url        TEXT,
    kind           TEXT DEFAULT 'rejected',
    created_at     INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_sensitive_jobs ON sensitive_jobs(platform, company, title);
CREATE INDEX IF NOT EXISTS idx_sensitive_jobs_at ON sensitive_jobs(created_at DESC);

CREATE TABLE IF NOT EXISTS app_config (
    key    TEXT PRIMARY KEY,
    value  TEXT
);
"#;

/// Boss 聊天持久化：会话列表（左侧 HR 列表）。
///
/// 主键 `(platform, uid)`：同一平台下 uid 唯一。写入走 UPSERT，远程拉取成功后覆盖。
/// 与 Python 侧 `scraper/chat.py` 无关——Python 只读接口，不写库（AGENTS 红线 1）。
pub const CHAT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS chat_friends (
    platform    TEXT NOT NULL,
    uid         TEXT NOT NULL,
    name        TEXT,
    company     TEXT,
    title       TEXT,
    last_msg    TEXT,
    last_time   TEXT,
    unread      INTEGER DEFAULT 0,
    avatar      TEXT,
    msg_count   INTEGER DEFAULT 0,
    last_ts     INTEGER,
    synced_at   INTEGER NOT NULL,
    PRIMARY KEY (platform, uid)
);

CREATE TABLE IF NOT EXISTS chat_messages (
    platform    TEXT NOT NULL,
    uid         TEXT NOT NULL,
    msg_id      TEXT NOT NULL,
    sender      TEXT NOT NULL,
    content     TEXT,
    time        TEXT,
    recalled    INTEGER DEFAULT 0,
    synced_at   INTEGER NOT NULL,
    PRIMARY KEY (platform, uid, msg_id)
);
"#;

/// 聊天表索引。主键已覆盖 `(platform, uid)` 与 `(platform, uid, msg_id)` 查询，
/// 这里只补「按 uid 取会话消息」所需的索引（主键前缀已覆盖，故仅留会话列表排序用）。
pub const CHAT_SCHEMA_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_chat_friends_platform ON chat_friends(platform, synced_at DESC);
"#;

/// `chat_friends` 老库按需补的列。
///
/// `last_ts` 是本轮新增（用户 m01539）：会话列表判新旧只看「最后聊天时刻」，
/// 而 `last_time` 是给人看的文案（「昨天」「11-18」），没法比较大小。
/// 旧库没有这一列时必须补上，否则新建的 UPSERT 语句会直接报 `no such column`。
pub const CHAT_UPGRADE_COLUMNS: &[(&str, &str)] = &[("last_ts", "INTEGER")];

/// 建索引语句。**必须在补列之后执行**。
///
/// 与 [`SCHEMA`] 分开的原因是一次真实踩坑：`CREATE INDEX ... ON job_queue(match_score)`
/// 写在建表语句里时，若打开的是缺 `match_score` 列的老库，
/// `CREATE TABLE IF NOT EXISTS` 被跳过、索引却立即执行 → `no such column: match_score`，
/// 整个应用打不开。拆开并放到 [`UPGRADE_COLUMNS`] 补列之后即可。
pub const SCHEMA_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_job_queue_status ON job_queue(status);
CREATE INDEX IF NOT EXISTS idx_job_queue_score  ON job_queue(match_score DESC);
CREATE INDEX IF NOT EXISTS idx_job_queue_plat   ON job_queue(platform);
CREATE INDEX IF NOT EXISTS idx_jd_decisions_at  ON jd_decisions(created_at DESC);
"#;

/// 老库按需补的列（SQLite 无 `ADD COLUMN IF NOT EXISTS`）。
///
/// 补齐旧库所需的列声明：**建表列 ∪ 补列**，即 [`job_columns`] 的完整集合。
///
/// 与 Python 侧 `_UPGRADE_COLUMNS` 的差别是有意的：Python 只补「建表语句之后新增的列」，
/// 因为它的 `SCHEMA` 早就包含 `title`/`company`/`salary` 等基础列。
/// Rust 侧要做的是「无论旧库多老，打开后都满足 [`SELECT_COLUMNS`]」，
/// 因此需要按建表语句补全——否则一个缺 `title` 的库会让所有查询报 `no such column`。
///
/// 声明类型取自建表语句；带默认值的列保留默认值，避免旧行出现 `NULL` 语义错位。
pub fn upgrade_column_decls() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = SCHEMA_COLUMN_DECLS.to_vec();
    for (name, decl) in UPGRADE_COLUMNS {
        if !out.iter().any(|(n, _)| n == name) {
            out.push((name, decl));
        }
    }
    out
}

/// 顺序无关，但**不能删减**：删掉任何一项都会让已存在的旧库缺列。
pub const UPGRADE_COLUMNS: &[(&str, &str)] = &[
    ("job_url", "TEXT"),
    ("jd_text", "TEXT"),
    ("location", "TEXT"),
    ("experience", "TEXT"),
    ("degree", "TEXT"),
    ("hr_name", "TEXT"),
    ("hr_position", "TEXT"),
    ("hr_active_status", "TEXT"),
    ("company_industry", "TEXT"),
    ("company_scale", "TEXT"),
    ("company_stage", "TEXT"),
    ("company_address", "TEXT"),
    ("company_intro", "TEXT"),
    ("hiring_count", "INTEGER"),
    ("founded_year", "INTEGER"),
    ("founded_years", "INTEGER"),
    ("reg_capital", "TEXT"),
    ("hr_active_time", "INTEGER"),
    ("token_used", "INTEGER DEFAULT 0"),
    ("match_score", "INTEGER"),
    ("rejected", "INTEGER DEFAULT 0"),
    ("score_source", "TEXT"),
    ("score_reason", "TEXT"),
    ("updated_at", "INTEGER"),
    // 打分制标记：'10' = 旧制未迁移，'100' = 当前制。
    // 必须同时出现在建表语句和补列列表里，否则：
    // - 新建库缺列 → 写入 score_scale 直接报 no such column；
    // - 旧库缺列 → 迁移语句 ALTER TABLE 才能补上，但补列若漏了这里同样会失败。
    ("score_scale", "TEXT DEFAULT '10'"),
];

/// `jd_decisions` 的补列声明（与 `job_queue` 分开，见 [`upgrade_decision_columns`]）。
///
/// 为什么需要：新版界面要显示「旧分 → 新分」，旧决策表只有一行文本 `reason`。
/// 加两列而不是解析 `reason` 字符串——那是把展示格式当数据源，改一个字就全崩。
/// 历史行这两列是 `NULL`，界面据此不显示分值（见 `DecisionRecord::score_change`）。
pub const DECISION_UPGRADE_COLUMNS: &[(&str, &str)] = &[("old_score", "INTEGER"), ("new_score", "INTEGER")];

/// `jd_decisions` 建表后应存在的列（建表列 ∪ 补列）。
pub fn upgrade_decision_columns() -> Vec<(&'static str, &'static str)> {
    DECISION_UPGRADE_COLUMNS.to_vec()
}

/// `job_queue` 的合法列名，用于过滤写入字段。
///
/// 与 `store.py:188 _job_columns()` 的用途一致：调用方传进来的 dict
/// 可能含未知键，不存在的列必须丢弃，否则 SQL 报错。
pub fn job_columns() -> Vec<&'static str> {
    let mut cols: Vec<&'static str> = SCHEMA_COLUMNS_JOB_QUEUE.iter().copied().collect();
    for (name, _) in UPGRADE_COLUMNS {
        if !cols.contains(name) {
            cols.push(name);
        }
    }
    cols
}

/// `job_queue` 的列名与声明类型（建表语句 + 本 crate 新增列）。
///
/// 这是补列的唯一真源：`upgrade_column_decls()` 与 [`job_columns`] 都从这里派生，
/// 避免「列名清单」和「类型声明」两处各写一遍导致漂移。
const SCHEMA_COLUMN_DECLS: &[(&str, &str)] = &[
    ("job_id", "TEXT"),
    ("platform", "TEXT"),
    ("title", "TEXT"),
    ("company", "TEXT"),
    ("salary", "TEXT"),
    ("status", "TEXT DEFAULT 'pending'"),
    ("created_at", "INTEGER"),
    ("job_url", "TEXT"),
    ("jd_text", "TEXT"),
    ("location", "TEXT"),
    ("experience", "TEXT"),
    ("degree", "TEXT"),
    ("hr_name", "TEXT"),
    ("hr_position", "TEXT"),
    ("hr_active_status", "TEXT"),
    ("company_industry", "TEXT"),
    ("company_scale", "TEXT"),
    ("company_stage", "TEXT"),
    ("company_address", "TEXT"),
    ("company_intro", "TEXT"),
    ("hiring_count", "INTEGER"),
    ("founded_years", "INTEGER"),
    ("match_score", "INTEGER"),
    ("rejected", "INTEGER DEFAULT 0"),
    ("score_source", "TEXT"),
    ("score_reason", "TEXT"),
    ("updated_at", "INTEGER"),
];

/// `job_queue` 在建表语句里声明的列（补列前）。
const SCHEMA_COLUMNS_JOB_QUEUE: &[&str] = &[
    "job_id",
    "platform",
    "title",
    "company",
    "salary",
    "status",
    "created_at",
    "job_url",
    "jd_text",
    "location",
    "experience",
    "degree",
    "hr_name",
    "hr_position",
    "hr_active_status",
    "company_industry",
    "company_scale",
    "company_stage",
    "company_address",
    "company_intro",
    "hiring_count",
    "founded_years",
    "match_score",
    "rejected",
    "score_source",
    "score_reason",
    "updated_at",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_declares_core_tables() {
        for t in ["job_queue", "jd_decisions", "sensitive_jobs", "app_config"] {
            assert!(
                SCHEMA.contains(&format!("CREATE TABLE IF NOT EXISTS {t}")),
                "缺少表 {t}"
            );
        }
    }

    #[test]
    fn sensitive_unique_index_preserved() {
        // 这条唯一索引是"不重复抓取"的基础，丢掉会导致同一岗位被反复处理
        assert!(SCHEMA.contains("uk_sensitive_jobs"));
        assert!(SCHEMA.contains("(platform, company, title)"));
    }

    #[test]
    fn upgrade_columns_cover_real_db() {
        // 真实库实测 31 列（`PRAGMA table_info(job_queue)`，data/job_assistant_pro.db）。
        // 在此之上本 crate 新增了 score_scale 一列 → 32 列。
        //
        // 这个数字是兼容性锚点：若 Python 侧将来加列而这里没同步，本断言会失败。
        // 失败时的正确处理是**核对真实库**再更新数字，而不是直接改数字。
        let cols = job_columns();
        assert_eq!(cols.len(), 32, "实际列：{cols:?}");
        assert!(
            cols.contains(&"score_scale"),
            "score_scale 必须在列清单里，否则迁移无法标记打分制"
        );
    }

    #[test]
    fn upgrade_columns_has_no_duplicates() {
        let cols = job_columns();
        let mut sorted = cols.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(cols.len(), sorted.len(), "job_columns 有重复项");
    }

    #[test]
    fn legacy_and_current_founded_columns_both_exist() {
        // founded_year（遗留）与 founded_years（当前）必须都在，
        // 否则旧库补列会漏掉其中一列
        let cols = job_columns();
        assert!(cols.contains(&"founded_year"));
        assert!(cols.contains(&"founded_years"));
    }

    #[test]
    fn token_used_has_default() {
        let decl = UPGRADE_COLUMNS
            .iter()
            .find(|(n, _)| *n == "token_used")
            .map(|(_, d)| *d)
            .unwrap();
        assert_eq!(decl, "INTEGER DEFAULT 0");
    }
}
