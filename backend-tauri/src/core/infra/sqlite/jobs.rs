//! 岗位聚合根：`JobStore` 端口实现 + `job_queue` 筛选条件拼接。
//!
//! 从 `sqlite.rs` 搬出（行为零变化）。`list_jobs` 与 `count_jobs` 共用
//! [`push_job_filters`]，保证「列表条数」与「总数」不会漂移。

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::domain::job::{Job, JobQuery, JobStatus, JobStore, ScoreSource};

use super::row::{now_secs, row_to_job, SELECT_COLUMNS};
use super::{Result, SqliteStore, StoreError};

impl JobStore for SqliteStore {
    type Error = StoreError;

    fn insert_job(&self, job: &Job) -> Result<bool> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let affected = conn.execute(
            "INSERT OR IGNORE INTO job_queue (
                job_id, platform, title, company, salary, status, created_at,
                job_url, jd_text, location, experience, degree, hr_name, hr_position,
                hr_active_status, company_industry, company_scale, company_stage,
                company_address, company_intro, hiring_count, founded_years,
                match_score, rejected, score_source, score_reason, updated_at, score_scale
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28
            )",
            params![
                job.job_id,
                job.platform,
                job.title,
                job.company,
                job.salary,
                if job.status.is_empty() {
                    JobStatus::Pending.as_str()
                } else {
                    job.status.as_str()
                },
                if job.created_at == 0 {
                    now_secs()
                } else {
                    job.created_at
                },
                job.job_url,
                job.jd_text,
                job.location,
                job.experience,
                job.degree,
                job.hr_name,
                job.hr_position,
                job.hr_active_status,
                job.company_industry,
                job.company_scale,
                job.company_stage,
                job.company_address,
                job.company_intro,
                job.hiring_count,
                job.founded_years,
                job.match_score,
                job.rejected,
                job.score_source,
                job.score_reason,
                job.updated_at,
                // 新写入的数据一律是百分制
                job.score_scale
                    .clone()
                    .unwrap_or_else(|| crate::core::domain::migration::SCALE_CURRENT.to_string()),
            ],
        )?;
        // INSERT OR IGNORE：重复 job_id 时 affected == 0
        Ok(affected > 0)
    }

    fn get_job(&self, job_id: &str) -> Result<Option<Job>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let sql = format!("SELECT {SELECT_COLUMNS} FROM job_queue WHERE job_id = ?1");
        conn.query_row(&sql, params![job_id], row_to_job)
            .optional()
            .map_err(Into::into)
    }

    fn list_jobs(&self, query: &JobQuery) -> Result<Vec<Job>> {
        let conn = self.conn.lock().expect("存储锁中毒");

        // 动态拼 WHERE：所有值走占位符，只有列名来自枚举常量
        let mut sql = format!("SELECT {SELECT_COLUMNS} FROM job_queue WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        // 与 `count_jobs` 共用同一套条件，避免「列表条数」与「总数」漂移
        push_job_filters(&mut sql, &mut args, query);

        let dir = if query.ascending { "ASC" } else { "DESC" };
        // 薪资排序要拼三键，每一条都有必须存在的理由：
        //  1. `SALARY_UNKNOWN_RANK_EXPR ASC` —— 把面议/空值压到末尾。SQLite 的
        //     NULL 在 ASC 时排**最前**（与 PG/MySQL 相反），不给这一列的话，
        //     升序时「面议」岗位会霸占榜首（真被测出来过）。
        //  2. `SALARY_K_EXPR {dir}` —— 真正的数值排序。
        //  3. `job_id {dir}` —— 兜底成全序。同薪资的岗位在分页边界上顺序不定，
        //     翻页会重复或漏条（同 `list_scored_ids` 必须按 job_id 排序的道理）。
        if query.sort.is_expression() {
            use crate::core::domain::job::{SALARY_K_EXPR, SALARY_UNKNOWN_RANK_EXPR};
            sql.push_str(&format!(
                " ORDER BY {SALARY_UNKNOWN_RANK_EXPR} ASC, {SALARY_K_EXPR} {dir}, job_id {dir}"
            ));
        } else {
            // 裸列排序同样要有 `job_id` 兜底键：按 match_score 排序时同分岗位
            // 的顺序由 SQLite 自行决定，翻页（LIMIT/OFFSET）会重复或漏条。
            // 这也让「未打分（NULL）一律沉底」成为必然结果——SQLite 的
            // NULL 在同向 ORDER BY 里恒排末尾，无需额外 CASE。
            sql.push_str(&format!(
                " ORDER BY {} {dir}, job_id {dir}",
                query.sort.column()
            ));
        }

        let limit = if query.limit == 0 { 100 } else { query.limit };
        sql.push_str(&format!(" LIMIT ?{}", args.len() + 1));
        args.push(Box::new(i64::from(limit)));
        sql.push_str(&format!(" OFFSET ?{}", args.len() + 1));
        args.push(Box::new(i64::from(query.offset)));

        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), row_to_job)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn update_score(
        &self,
        job_id: &str,
        score: u8,
        source: ScoreSource,
        reason: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("存储锁中毒");
        conn.execute(
            "UPDATE job_queue SET match_score = ?1, score_source = ?2, score_reason = ?3,
             updated_at = ?4, score_scale = ?5 WHERE job_id = ?6",
            params![
                i64::from(score),
                source.as_str(),
                reason,
                now_secs(),
                crate::core::domain::migration::SCALE_CURRENT,
                job_id
            ],
        )?;
        Ok(())
    }

    fn list_scored_ids(&self, after: Option<&str>, limit: u32) -> Result<Vec<Job>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        // 游标用 `job_id > ?` 而不是 OFFSET：重算会改动 match_score，
        // 而本查询按主键排序，分数改动不会让分页边界漂移。
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM job_queue WHERE job_id > ?1 \
             ORDER BY job_id ASC LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![after.unwrap_or(""), i64::from(limit.max(1))],
            row_to_job,
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn log_decision(
        &self,
        platform: &str,
        company_job: &str,
        decision: &str,
        reason: &str,
    ) -> Result<()> {
        // 不带分数的写入口：两列显式写 NULL，而不是让列默认值替我们决定
        self.log_score_change(platform, company_job, decision, reason, None, None)
    }

    fn log_score_change(
        &self,
        platform: &str,
        company_job: &str,
        decision: &str,
        reason: &str,
        old_score: Option<u8>,
        new_score: Option<u8>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("存储锁中毒");
        conn.execute(
            "INSERT INTO jd_decisions
                (platform, company_job, decision, reason, created_at, old_score, new_score)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                platform,
                company_job,
                decision,
                reason,
                now_secs(),
                old_score.map(i64::from),
                new_score.map(i64::from),
            ],
        )?;
        Ok(())
    }
}
/// 把 [`JobQuery`] 的筛选条件拼到 `WHERE 1=1` 之后，值一律走占位符。
///
/// `list_jobs` 与 `count_jobs` 共用，保证「列表条数」与「总数」不会因为
/// 两处各写一遍条件而打架（旧 Python 侧正是这样写的两份，容易漂移）。
pub(crate) fn push_job_filters(
    sql: &mut String,
    args: &mut Vec<Box<dyn rusqlite::ToSql>>,
    q: &JobQuery,
) {
    if q.rejected_only {
        // 「不投递」页签：只要否决位。**优先于 include_rejected**——
        // 两者同时为真时若走 `include_rejected` 分支，条件会变成「不过滤」，
        // 页签计数把全部岗位都算进来。
        sql.push_str(" AND COALESCE(rejected, 0) = 1");
    } else if !q.include_rejected {
        sql.push_str(" AND COALESCE(rejected, 0) = 0");
    }
    if let Some(status) = &q.status {
        sql.push_str(&format!(" AND status = ?{}", args.len() + 1));
        args.push(Box::new(status.clone()));
    }
    if let Some(platform) = &q.platform {
        sql.push_str(&format!(" AND platform = ?{}", args.len() + 1));
        args.push(Box::new(platform.clone()));
    }
    if let Some(min) = q.min_score {
        // COALESCE 与 Python 一致：没打分的行不能因为 NULL >= 70 求值成 NULL 被剔除
        sql.push_str(&format!(" AND COALESCE(match_score, 0) >= ?{}", args.len() + 1));
        args.push(Box::new(i64::from(min)));
    }
}

pub(crate) fn query_jobs_with_filter(
    conn: &Connection,
    sql: &str,
    args: &[Box<dyn rusqlite::ToSql>],
) -> Result<Vec<Job>> {
    let mut stmt = conn.prepare(sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), row_to_job)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}
