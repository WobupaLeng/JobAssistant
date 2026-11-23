//! 入库端口与敏感库查询器：把 `SqliteStore` 包成 `IngestPort` / lookup 闭包。

use std::sync::Arc;

use crate::core::domain::job::{Job, JobStore, SensitiveEntry, SensitiveKind, SensitiveStore};
use crate::core::infra::SqliteStore;
use crate::core::sidecar::ingest::DecisionNote;
use crate::core::sidecar::session::{IngestPort, Ports};

/// 入库端口：写达标岗位 + 把结论记进敏感库 + 写决策日志。
///
/// `Arc<SqliteStore>` 被移进消费线程，因此不占用外壳层 `StoreState` 的锁——
/// 用户在抓取过程中打开列表不会被写入阻塞。
pub fn make_sink(store: Arc<SqliteStore>) -> impl IngestPort {
    let put_store = Arc::clone(&store);
    let remember_store = Arc::clone(&store);
    let record_store = Arc::clone(&store);
    Ports {
        put: move |job: &Job| {
            let inserted = match put_store.insert_job(job) {
                Ok(v) => v,
                Err(e) => return Err(format!("入库失败：{e}")),
            };
            // 入库（或此前已入库）都登记为 stored，下次抓取能在卡片阶段直接跳过，
            // 省掉一整轮详情页（3~6 秒/条）。写失败不影响主流程，仅记日志。
            if let Err(e) = put_store.add_sensitive(&SensitiveEntry {
                platform: &job.platform,
                company: job.company.as_deref(),
                title: job.title.as_deref(),
                reason: "已入库",
                kind: SensitiveKind::Stored,
                job_id: Some(job.job_id.as_str()),
                salary: job.salary.as_deref(),
                location: job.location.as_deref(),
                job_url: job.job_url.as_deref(),
            }) {
                eprintln!("[scrape] 写入敏感库失败（stored）：{e}");
            }
            Ok(inserted)
        },
        remember: move |entry: &SensitiveEntry<'_>| {
            if let Err(e) = remember_store.add_sensitive(entry) {
                eprintln!("[scrape] 写入敏感库失败（rejected）：{e}");
            }
        },
        // 决策日志写失败不打断抓取：它是审计线索，不是主数据。但失败必须出声，
        // 否则「决策日志页面恒空」这个 bug 会再次悄无声息地回来。
        record: move |note: &DecisionNote| {
            if let Err(e) = record_store.log_score_change(
                &note.platform,
                &note.company_job,
                note.decision,
                &note.reason,
                None,
                note.new_score,
            ) {
                eprintln!("[scrape] 写决策日志失败（{}）：{e}", note.decision);
            }
        },
    }
}

/// 敏感库查询器：卡片阶段先查重 / 查敏感库，命中就不再开详情页。
///
/// 命中即跳过，**不看 kind**：`stored` 是「库里已经有了」，
/// `rejected` 是「之前用户不要」，两者都不需要再花 3~6 秒拉详情。
pub fn make_lookup(
    store: Arc<SqliteStore>,
) -> impl FnMut(&str, Option<&str>, Option<&str>) -> Option<(String, bool)> + Send + 'static {
    move |platform: &str, company: Option<&str>, title: Option<&str>| {
        // 先精确查（公司+岗位），不中再按公司粗查（任一曾判定过的公司整体回避）
        let hit = store
            .sensitive_hit(platform, company, title)
            .ok()
            .flatten()
            .or_else(|| store.sensitive_company_hit(platform, company).ok().flatten());
        hit.map(|h| {
            let stored = h.is_stored();
            let reason = h
                .reason
                .filter(|r| !r.trim().is_empty())
                .unwrap_or_else(|| "此前已判定过".to_string());
            (reason, stored)
        })
    }
}
