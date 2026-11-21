//! 管理类聚合根：`JobAdminStore` / `SensitiveAdminStore` / `DecisionStore` / `ConfigStore`。
//!
//! 从 `sqlite.rs` 搬出（行为零变化）。与主链路的区别：这些方法不改领域判定，
//! 只做批量标记、统计与分页浏览。所有可批量传参的地方一律生成**命名占位符**
//! （`?1..?N`），短 id 缺失时按 `NULL` 绑定——绝不用 `IS NOT NULL` 兜底，
//! 否则漏了 id 的行会被整表误判。

use rusqlite::{params, OptionalExtension};
use crate::core::domain::job::{Job, JobQuery, JobStatus, SensitiveKind};
use crate::core::domain::store_ports::{
    ConfigStore, DecisionRecord, DecisionStore, JobAdminStore, JobStats, SensitiveAdminStore,
    SensitiveQuery, SensitiveRecord,
};

use super::jobs::{push_job_filters, query_jobs_with_filter};
use super::row::{now_secs, SELECT_COLUMNS};
use super::{Result, SqliteStore, StoreError};

// ---------------------------------------------------------------------------
// 管理类端口（`store.py` 中界面侧使用的方法）
//
// 与主链路的区别：这些方法不改领域判定，只做批量标记、统计与分页浏览。
// 所有可批量传参的地方一律生成**命名占位符**（`?1..?N`），短 id 缺失时
// 按 `NULL` 绑定——绝不用 `IS NOT NULL` 兜底，否则漏了 id 的行会被整表误判。
// ---------------------------------------------------------------------------

/// `job_queue` 的 SELECT 白名单列（与 [`SELECT_COLUMNS`] 同源），供测试核对列顺序。
#[cfg(test)]
fn select_list() -> Vec<&'static str> {
    SELECT_COLUMNS.split(',').map(|s| s.trim()).collect()
}

#[cfg(test)]
#[test]
fn select_columns_match_column_count() {
    // 新增列时 SELECT_COLUMNS 与 row_to_job 的索引必须同步，否则运行期 InvalidColumnName
    let cols = select_list();
    assert_eq!(cols.len(), 28, "SELECT 列数变化，需同步 row_to_job");
    assert_eq!(cols[0], "job_id");
    assert_eq!(cols[27], "score_scale");
}

/// 生成从 `start` 开始的 `n` 个编号占位符：`numbered_placeholders(3, 2)` → `"?3, ?4"`。
///
/// **`n == 0` 返回空串，调用方必须先判空**，否则会拼出非法的 `IN ()`。
fn numbered_placeholders(start: usize, n: usize) -> String {
    (start..start + n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `IN (?)` 的占位符个数上限。
///
/// SQLite 单个语句的宿主参数默认上限是 32766；一次性绑 3 万个 id 既无意义
/// （界面上的批量操作不会超过一屏）又会把参数列表撑爆，这里直接拒绝。
pub(crate) const MAX_BATCH_IDS: usize = 1000;

/// `LIKE` 转义：把用户关键词里的 `%` / `_` / `\` 变成字面量。
///
/// 不转义的话，关键词 `100%` 会退化成「以 100 开头」的匹配。
/// 配套的 `ESCAPE '\'` 写在各条 SQL 里。
fn escape_like(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    for ch in raw.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// 敏感库查询的 WHERE 片段 + 绑定参数（**不含** `WHERE 1=1` 之后的分页）。
///
/// `list_sensitive` 与 `count_sensitive_query` 共用：抽屉标题的条数与卡片
/// 列表必须由同一套条件派生。值全部走占位符，只有 `COALESCE(kind,'rejected')`
/// 这类字面量是常量。
fn sensitive_where(query: &SensitiveQuery) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut sql = String::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(platform) = &query.platform {
        sql.push_str(&format!(" AND platform = ?{}", args.len() + 1));
        args.push(Box::new(platform.clone()));
    }
    if let Some(kind) = query.kind.kind() {
        // kind 来自枚举常量，但值仍然走占位符：统一规则比「这里安全所以可以拼」更好维护
        sql.push_str(&format!(" AND COALESCE(kind, 'rejected') = ?{}", args.len() + 1));
        args.push(Box::new(kind.to_string()));
    }
    if let Some(kw) = &query.keyword {
        let kw = kw.trim();
        if !kw.is_empty() {
            let pat = format!("%{}%", escape_like(kw));
            let n = args.len();
            sql.push_str(&format!(
                " AND (company LIKE ?{a} ESCAPE '\\' OR title LIKE ?{b} ESCAPE '\\' \
                 OR reason LIKE ?{c} ESCAPE '\\')",
                a = n + 1,
                b = n + 2,
                c = n + 3
            ));
            args.push(Box::new(pat.clone()));
            args.push(Box::new(pat.clone()));
            args.push(Box::new(pat));
        }
    }
    (sql, args)
}

/// 与 `list_sensitive` **同一套条件**的计数（抽屉标题「敏感库 154 条」）。
///
/// 刻意不放进 trait：`SensitiveAdminStore` 的既有实现方（含测试内存桩）
/// 不该为了一个计数方法跟着改签名。命令层直接对 `SqliteStore` 调用本方法。
/// 旧的 `SensitiveAdminStore::count_sensitive` 只认 platform，保留不动。
pub(crate) fn count_sensitive_query(store: &SqliteStore, query: &SensitiveQuery) -> Result<i64> {
    let conn = store.conn.lock().expect("存储锁中毒");
    let (mut sql, args) = sensitive_where(query);
    sql.insert_str(0, "SELECT COUNT(*) FROM sensitive_jobs WHERE 1=1");
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    stmt.query_row(refs.as_slice(), |r| r.get(0))
        .map_err(Into::into)
}

impl JobAdminStore for SqliteStore {
    type Error = StoreError;

    fn set_rejected(&self, job_ids: &[String], rejected: bool) -> Result<usize> {
        if job_ids.is_empty() {
            return Ok(0);
        }
        if job_ids.len() > MAX_BATCH_IDS {
            return Err(StoreError::BatchTooLarge {
                got: job_ids.len(),
                max: MAX_BATCH_IDS,
            });
        }
        let conn = self.conn.lock().expect("存储锁中毒");
        // ?1 = rejected 标志, ?2 = updated_at, ?3.. = job_id（与下面 params 的压入顺序严格一致）
        let sql = format!(
            "UPDATE job_queue SET rejected = ?1, updated_at = ?2 WHERE job_id IN ({})",
            numbered_placeholders(3, job_ids.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(job_ids.len() + 2);
        let flag = i64::from(rejected);
        let now = now_secs();
        params.push(&flag);
        params.push(&now);
        for id in job_ids {
            params.push(id);
        }
        let affected = stmt.execute(params.as_slice())?;
        Ok(affected)
    }

    fn set_delivered(&self, job_ids: &[String], delivered: bool) -> Result<usize> {
        if job_ids.is_empty() {
            return Ok(0);
        }
        if job_ids.len() > MAX_BATCH_IDS {
            return Err(StoreError::BatchTooLarge {
                got: job_ids.len(),
                max: MAX_BATCH_IDS,
            });
        }
        let conn = self.conn.lock().expect("存储锁中毒");
        let sql = format!(
            "UPDATE job_queue SET status = ?1, updated_at = ?2 WHERE job_id IN ({})",
            numbered_placeholders(3, job_ids.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let status = if delivered {
            JobStatus::Delivered.as_str()
        } else {
            JobStatus::Pending.as_str()
        };
        let now = now_secs();
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(job_ids.len() + 2);
        params.push(&status);
        params.push(&now);
        for id in job_ids {
            params.push(id);
        }
        Ok(stmt.execute(params.as_slice())?)
    }

    fn unscored_jobs(&self) -> Result<Vec<Job>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let sql = format!("SELECT {SELECT_COLUMNS} FROM job_queue WHERE match_score IS NULL");
        query_jobs_with_filter(&conn, &sql, &[])
    }

    fn stats(&self) -> Result<JobStats> {
        let conn = self.conn.lock().expect("存储锁中毒");
        // COUNT(*) 走全表，五个数字放一次扫描不现实但表很小（上千行级别），
        // 拆成四条语句比一条带 5 个子查询的 SQL 更好读。
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM job_queue", [], |r| r.get(0))?;
        let scored: i64 = conn.query_row(
            "SELECT COUNT(*) FROM job_queue WHERE match_score IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        let avg: Option<f64> = conn.query_row(
            "SELECT AVG(match_score) FROM job_queue WHERE match_score IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        // round(x, 2)：与 Python 的 round(avg, 2) 对齐，避免前端拿到 78.33333...
        let avg_score = avg.map(|v| (v * 100.0).round() / 100.0);

        let by_status = {
            let mut stmt = conn.prepare(
                "SELECT COALESCE(status, 'pending'), COUNT(*) FROM job_queue GROUP BY 1",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let by_platform = {
            let mut stmt =
                conn.prepare("SELECT COALESCE(platform, ''), COUNT(*) FROM job_queue GROUP BY 1")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(JobStats {
            total,
            scored,
            avg_score,
            by_status,
            by_platform,
        })
    }

    fn job_exists(&self, job_id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let found: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM job_queue WHERE job_id = ?1 LIMIT 1",
                params![job_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    fn count_jobs(&self, query: &JobQuery) -> Result<i64> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let mut sql = String::from("SELECT COUNT(*) FROM job_queue WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        push_job_filters(&mut sql, &mut args, query);
        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        conn.query_row(&sql, refs.as_slice(), |r| r.get(0))
            .map_err(Into::into)
    }

    fn delivered_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let mut stmt = conn.prepare("SELECT job_id FROM job_queue WHERE status = ?1")?;
        let rows = stmt.query_map(params![JobStatus::Delivered.as_str()], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn rejected_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let mut stmt = conn.prepare("SELECT job_id FROM job_queue WHERE COALESCE(rejected, 0) = 1")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }
}

impl SensitiveAdminStore for SqliteStore {
    type Error = StoreError;

    fn clear_sensitive(&self) -> Result<usize> {
        let conn = self.conn.lock().expect("存储锁中毒");
        Ok(conn.execute("DELETE FROM sensitive_jobs", [])?)
    }

    fn list_sensitive(&self, query: &SensitiveQuery) -> Result<Vec<SensitiveRecord>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let (mut sql, mut args) = sensitive_where(query);
        sql.insert_str(
            0,
            "SELECT id, platform, job_id, company, title, salary, location, \
             reason, job_url, kind, created_at FROM sensitive_jobs WHERE 1=1",
        );
        sql.push_str(" ORDER BY id DESC");
        // limit = 0 视为默认 500（与 store.py 的默认值一致），而不是「一条都不给」
        let limit = if query.limit == 0 { 500 } else { query.limit };
        sql.push_str(&format!(" LIMIT ?{}", args.len() + 1));
        args.push(Box::new(i64::from(limit)));
        sql.push_str(&format!(" OFFSET ?{}", args.len() + 1));
        args.push(Box::new(i64::from(query.offset)));

        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), |r| {
            Ok(SensitiveRecord {
                id: r.get(0)?,
                platform: r.get(1)?,
                job_id: r.get(2)?,
                company: r.get(3)?,
                title: r.get(4)?,
                salary: r.get(5)?,
                location: r.get(6)?,
                reason: r.get(7)?,
                job_url: r.get(8)?,
                kind: r
                    .get::<_, Option<String>>(9)?
                    .unwrap_or_else(|| SensitiveKind::Rejected.as_str().to_string()),
                created_at: r.get(10)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn count_sensitive(&self, platform: Option<&str>) -> Result<i64> {
        let conn = self.conn.lock().expect("存储锁中毒");
        match platform {
            Some(p) => conn
                .query_row(
                    "SELECT COUNT(*) FROM sensitive_jobs WHERE platform = ?1",
                    params![p],
                    |r| r.get(0),
                )
                .map_err(Into::into),
            None => conn
                .query_row("SELECT COUNT(*) FROM sensitive_jobs", [], |r| r.get(0))
                .map_err(Into::into),
        }
    }

    fn delete_sensitive(&self, ids: &[i64]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        if ids.len() > MAX_BATCH_IDS {
            return Err(StoreError::BatchTooLarge {
                got: ids.len(),
                max: MAX_BATCH_IDS,
            });
        }
        let conn = self.conn.lock().expect("存储锁中毒");
        let sql = format!(
            "DELETE FROM sensitive_jobs WHERE id IN ({})",
            numbered_placeholders(1, ids.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        Ok(stmt.execute(params.as_slice())?)
    }

    fn sync_stored(&self) -> Result<usize> {
        let conn = self.conn.lock().expect("存储锁中毒");
        // 整段一个事务：中途崩了不能留下「补了一半」的敏感库，
        // 否则下一轮抓取会按残缺的敏感库跳岗位。
        let tx = conn.unchecked_transaction()?;
        let mut inserted = 0usize;
        {
            let mut stmt = tx.prepare(
                "SELECT platform, job_id, company, title, salary, location, job_url FROM job_queue",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })?;
            let all: Vec<_> = rows.collect::<rusqlite::Result<_>>()?;
            drop(stmt);

            let now = now_secs();
            for (platform, job_id, company, title, salary, location, job_url) in all {
                // 与 add_sensitive 同一条 INSERT OR IGNORE：唯一键是 (platform, company, title)
                let affected = tx.execute(
                    "INSERT OR IGNORE INTO sensitive_jobs
                     (platform, job_id, company, title, salary, location, reason, job_url, kind, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        platform,
                        job_id,
                        company,
                        title,
                        salary,
                        location,
                        "已入库（历史数据）",
                        job_url,
                        SensitiveKind::Stored.as_str(),
                        now,
                    ],
                )?;
                if affected > 0 {
                    inserted += 1;
                }
                // 已存在（含 company/title 为 NULL 的行）：强制升级为 stored。
                // 注意 `company IS ?` / `title IS ?` 而不是 `= ?`：SQLite 里
                // `NULL = NULL` 为 NULL，用 `=` 永远匹配不到 NULL 行。
                tx.execute(
                    "UPDATE sensitive_jobs SET kind = ?1
                     WHERE platform = ?2 AND company IS ?3 AND title IS ?4",
                    params![
                        SensitiveKind::Stored.as_str(),
                        platform,
                        company,
                        title,
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }
}

impl DecisionStore for SqliteStore {
    type Error = StoreError;

    fn list_decisions(&self, limit: u32, after_id: i64) -> Result<Vec<DecisionRecord>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let mut stmt = conn.prepare(
            "SELECT id, platform, company_job, decision, reason, created_at, old_score, new_score
             FROM jd_decisions WHERE id > ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let limit = if limit == 0 { 200 } else { limit };
        let rows = stmt.query_map(params![after_id, i64::from(limit)], |r| {
            Ok(DecisionRecord {
                id: r.get(0)?,
                platform: r.get(1)?,
                company_job: r.get(2)?,
                decision: r.get(3)?,
                reason: r.get(4)?,
                created_at: r.get(5)?,
                // 补列声明里的 `old_score` / `new_score` 是 INTEGER，
                // 历史行为 NULL → None；越界值(>255)按 None 处理而不是 panic
                old_score: r.get::<_, Option<i64>>(6)?.and_then(|v| u8::try_from(v).ok()),
                new_score: r.get::<_, Option<i64>>(7)?.and_then(|v| u8::try_from(v).ok()),
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }
}

impl ConfigStore for SqliteStore {
    type Error = StoreError;

    fn get_config(&self, key: &str, default: &str) -> Result<String> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let v: Option<Option<String>> = conn
            .query_row("SELECT value FROM app_config WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?;
        // 三种「没有值」统一成 default：行不存在 / 值 NULL / 值为空串
        Ok(match v {
            Some(Some(s)) if !s.is_empty() => s,
            _ => default.to_string(),
        })
    }

    fn set_config(&self, key: &str, value: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().expect("存储锁中毒");
        conn.execute(
            "INSERT INTO app_config (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value.unwrap_or("")],
        )?;
        Ok(())
    }

    fn get_config_json(
        &self,
        key: &str,
        default: serde_json::Value,
    ) -> Result<serde_json::Value> {
        // 注意这里不能用 get_config(key, "")：值本身可能是合法 JSON `""`（空 JSON 字符串），
        // 空串判定会把它误当成「没有配置」。
        let conn = self.conn.lock().expect("存储锁中毒");
        let raw: Option<Option<String>> = conn
            .query_row("SELECT value FROM app_config WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(match raw.flatten() {
            Some(s) => crate::core::domain::store_ports::parse_config_json(&s, default),
            None => default,
        })
    }

    fn set_config_json(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        // serde_json 默认不转义非 ASCII，与 Python 的 ensure_ascii=False 一致
        let text = serde_json::to_string(value).map_err(|e| StoreError::Json(e.to_string()))?;
        self.set_config(key, Some(&text))
    }
}
