//! 行映射与列白名单：`job_queue` 的查询列、`Job` 解析、敏感库行解析、Unix 秒。
//!
//! 从 `sqlite.rs` 搬出（行为零变化）。**列顺序只有这里一处定义**：
//! [`SELECT_COLUMNS`] 与 [`row_to_job`] 的索引必须一一对应，加字段时两边同步改，
//! 否则症状是运行期的 `InvalidColumnName`。

use rusqlite::Row;

use crate::core::domain::job::{Job, SensitiveHit, SensitiveKind};

/// `SELECT` 用的列顺序。与 [`row_to_job`] 严格对应。
///
/// 抽成常量是为了让「查询列」和「解析列」只有一处定义——
/// 两处各写一遍的话，加字段时必然有一边漏掉，症状是运行期 `InvalidColumnName`。
pub(crate) const SELECT_COLUMNS: &str = "\
job_id, platform, title, company, salary, status, created_at, \
job_url, jd_text, location, experience, degree, hr_name, hr_position, \
hr_active_status, company_industry, company_scale, company_stage, \
company_address, company_intro, hiring_count, founded_years, \
match_score, rejected, score_source, score_reason, updated_at, score_scale";

/// 把一行结果解析成 [`Job`]。索引必须与 [`SELECT_COLUMNS`] 一一对应。
pub(crate) fn row_to_job(row: &Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        job_id: row.get(0)?,
        platform: row.get(1)?,
        title: row.get(2)?,
        company: row.get(3)?,
        salary: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
        job_url: row.get(7)?,
        jd_text: row.get(8)?,
        location: row.get(9)?,
        experience: row.get(10)?,
        degree: row.get(11)?,
        hr_name: row.get(12)?,
        hr_position: row.get(13)?,
        hr_active_status: row.get(14)?,
        company_industry: row.get(15)?,
        company_scale: row.get(16)?,
        company_stage: row.get(17)?,
        company_address: row.get(18)?,
        company_intro: row.get(19)?,
        hiring_count: row.get(20)?,
        founded_years: row.get(21)?,
        // match_score 在库里是 INTEGER，但语义是 0-100，
        // 用 u8 会更好；老数据若超界则饱和而非报错
        match_score: row.get::<_, Option<i64>>(22)?.map(|v| v.clamp(0, 255) as u8),
        rejected: row.get::<_, Option<i64>>(23)?.unwrap_or(0),
        score_source: row.get(24)?,
        score_reason: row.get(25)?,
        updated_at: row.get(26)?,
        score_scale: row.get(27)?,
    })
}
pub(crate) fn row_to_sensitive_hit(r: &Row<'_>) -> rusqlite::Result<SensitiveHit> {
    Ok(SensitiveHit {
        // 老库可能没有 kind 列值，按「被排除」处理更安全：
        // 误判成 rejected 最多多抓一次详情，误判成 stored 会漏掉岗位
        kind: r
            .get::<_, Option<String>>(0)?
            .unwrap_or_else(|| SensitiveKind::Rejected.as_str().to_string()),
        title: r.get(1)?,
        reason: r.get(2)?,
    })
}

/// 当前 Unix 秒。`created_at` / `updated_at` 在旧库中就是这个维度。
pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
