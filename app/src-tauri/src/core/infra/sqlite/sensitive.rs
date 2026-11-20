//! 敏感库聚合根：`SensitiveStore` 端口实现（卡片阶段查重 / 查敏感库）。
//!
//! 从 `sqlite.rs` 搬出（行为零变化）。

use rusqlite::{params, OptionalExtension};

use crate::core::domain::job::{
    SensitiveEntry, SensitiveHit, SensitiveKind, SensitiveStore,
};

use super::row::{now_secs, row_to_sensitive_hit};
use super::{Result, SqliteStore, StoreError};

impl SensitiveStore for SqliteStore {
    type Error = StoreError;

    fn sensitive_hit(
        &self,
        platform: &str,
        company: Option<&str>,
        title: Option<&str>,
    ) -> Result<Option<SensitiveHit>> {
        // 缺公司或岗位名就无从查起——与 `store.py:436` 的早退一致
        let (Some(company), Some(title)) = (company, title) else {
            return Ok(None);
        };
        if company.trim().is_empty() || title.trim().is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().expect("存储锁中毒");
        conn.query_row(
            "SELECT kind, title, reason FROM sensitive_jobs
             WHERE platform = ?1 AND company = ?2 AND title = ?3
             ORDER BY id DESC LIMIT 1",
            params![platform, company, title],
            row_to_sensitive_hit,
        )
        .optional()
        .map_err(Into::into)
    }

    fn sensitive_company_hit(
        &self,
        platform: &str,
        company: Option<&str>,
    ) -> Result<Option<SensitiveHit>> {
        // 短公司名（「华为」这类）粗查重误伤太大，沿用 `store.py:448` 的 4 字门槛
        let Some(company) = company else {
            return Ok(None);
        };
        if company.trim().chars().count() < 4 {
            return Ok(None);
        }
        let conn = self.conn.lock().expect("存储锁中毒");
        conn.query_row(
            "SELECT kind, title, reason FROM sensitive_jobs
             WHERE platform = ?1 AND company = ?2
             ORDER BY id DESC LIMIT 1",
            params![platform, company.trim()],
            row_to_sensitive_hit,
        )
        .optional()
        .map_err(Into::into)
    }

    fn add_sensitive(&self, entry: &SensitiveEntry<'_>) -> Result<bool> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let affected = conn.execute(
            "INSERT OR IGNORE INTO sensitive_jobs
             (platform, job_id, company, title, salary, location, reason, job_url, kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                entry.platform,
                entry.job_id,
                entry.company,
                entry.title,
                entry.salary,
                entry.location,
                entry.reason,
                entry.job_url,
                entry.kind.as_str(),
                now_secs(),
            ],
        )?;
        if affected == 0 && entry.kind == SensitiveKind::Stored {
            // 已存在且这次是「入库」：把之前记成 rejected 的升级为 stored，
            // 否则下一轮会把一个已经在库里的岗位当成被排除的又抓一遍。
            conn.execute(
                "UPDATE sensitive_jobs SET kind = 'stored'
                 WHERE platform = ?1 AND company IS ?2 AND title IS ?3",
                params![entry.platform, entry.company, entry.title],
            )?;
        }
        Ok(affected > 0)
    }
}
