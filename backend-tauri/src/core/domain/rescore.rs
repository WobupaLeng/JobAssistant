//! 全量重算：按当前打分口径重算库内所有岗位（SPEC-002 第 3.3 节末条）。
//!
//! 迁移只换算量纲（×10），**不会重新走一遍五维规则**。所以历史上因口径变化、
//! 规则调整或 JD 后来补齐而本该变分的岗位，只有重算才能纠正。规格要求在迁移
//! 前后提供这个入口，本模块是它的领域实现。
//!
//! 为什么放在 domain：它只是「读一条 → 打分 → 写回」的循环，不含 SQL 也不含
//! 线程，测试可以用内存计数器跑一遍，不必碰真实数据库。

use crate::core::domain::job::{Job, JobStore, ScoreSource};
use crate::core::domain::profile::Profile;
use crate::core::domain::scoring_service::{score_job, ScoreConfig};
use crate::core::domain::scoring_llm::LlmClient;
use crate::core::scoring::SCORE_MAX;

/// 重算入参。
#[derive(Debug, Clone, Default)]
pub struct RescoreOptions {
    /// 只处理 `job_id` 大于该值的行（断点续算用）。`None` 表示从头开始。
    pub after_job_id: Option<String>,
    /// 本次最多处理多少条。`None` 表示不设上限。
    ///
    /// 存在的理由是每批之间要回前端刷进度：`list_jobs` 与重算抢同一把
    /// SQLite 写锁，一次锁到底会让界面卡住十几秒。
    pub limit: Option<u32>,
    /// 是否用大模型打分。**默认关闭**：全库走大模型是按条计费的，
    /// 不能因为用户点了「重算」就悄悄产生几百次调用。
    pub use_llm: bool,
    /// 规则初筛线（`use_llm` 为真时才有意义）
    pub llm_prefilter: u8,
    /// 用户自定义筛选条件，会拼进提示词
    pub filters: String,
    /// 是否把「变了多少分」写成一条 `jd_decisions` 记录
    pub log_changes: bool,
}

impl RescoreOptions {
    fn to_config(&self) -> ScoreConfig {
        ScoreConfig {
            use_llm: self.use_llm,
            llm_prefilter: self.llm_prefilter,
            filters: self.filters.clone(),
        }
    }
}

/// 一条岗位的重算结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RescoreChange {
    pub job_id: String,
    pub title: Option<String>,
    pub company: Option<String>,
    /// 重算前的分数；旧制未迁移的行会是 `None`
    pub old_score: Option<u8>,
    pub new_score: u8,
    /// `rule` 或 `llm`
    pub source: &'static str,
    pub reason: String,
}

impl RescoreChange {
    /// 分数是否真的变了。用于统计「本次改动了多少条」。
    pub fn changed(&self) -> bool {
        self.old_score != Some(self.new_score)
    }
}

/// 一批重算的汇总。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RescoreOutcome {
    /// 本批处理（读过）的条数
    pub scanned: u32,
    /// 分数实际变动的条数
    pub updated: u32,
    /// 写库失败的条数（失败不中断，继续算下一条）
    pub failed: u32,
    /// 下一批的起点；`None` 表示已经算到底
    pub next_cursor: Option<String>,
    /// 明细，给界面展示「哪几条变了」
    pub changes: Vec<RescoreChange>,
}

impl RescoreOutcome {
    /// 本批是否还有后续。
    pub fn has_more(&self) -> bool {
        self.next_cursor.is_some()
    }
}

/// 重算一批岗位。
///
/// `store` 同时用于读与写：这是**运维动作**，不是用户交互路径，
/// 没必要为它拆出一个只读端口。调用方（命令层）负责分批调用。
///
/// 单条失败不中断整批：一条脏数据不该让其余 300 条都算不了。
pub fn rescore_batch<S: JobStore>(
    store: &S,
    profile: &Profile,
    opts: &RescoreOptions,
    llm: Option<&dyn LlmClient>,
) -> RescoreOutcome {
    let mut out = RescoreOutcome::default();
    let cfg = opts.to_config();

    // 上限夹到 1 条以上：`limit: Some(0)` 会让「还有多少要算」永远推进不了，
    // 调用方以为在续算，实际原地打转。
    // `None` = 不分批，一次读完当前游标之后的所有行。
    // 这里不能用 `SET 大数字` 的写法：`list_scored_ids` 的 `limit` 是 u32，
    // 而 SQLite 的 LIMIT 能接受任意大得多的值，取 u32 上限即可覆盖任何真实库。
    let page = opts.limit.unwrap_or(u32::MAX).max(1);

    let jobs = match store.list_scored_ids(opts.after_job_id.as_deref(), page) {
        Ok(v) => v,
        Err(_) => return out,
    };

    for job in jobs {
        out.scanned += 1;
        out.next_cursor = Some(job.job_id.clone());

        let verdict = score_job(&job, profile, &cfg, llm);
        // 越界分先夹住再写：`update_score` 对越界值是拒绝而不是 clamp，
        // 一条脏岗位不该让整批在最后一列失败。
        let new_score = verdict.score.min(SCORE_MAX);
        let source = ScoreSource::parse(verdict.source).unwrap_or(ScoreSource::Rule);

        if let Err(_e) = store.update_score(&job.job_id, new_score, source, &verdict.reason) {
            out.failed += 1;
            continue;
        }

        let change = RescoreChange {
            job_id: job.job_id.clone(),
            title: job.title.clone(),
            company: job.company.clone(),
            old_score: job.match_score,
            new_score,
            source: verdict.source,
            reason: verdict.reason.clone(),
        };
        if change.changed() {
            out.updated += 1;
            if opts.log_changes {
                // 决策日志写失败同样不中断：它是审计线索，不是主数据。
                //
                // 走 `log_score_change` 而不是 `log_decision`：两端分数必须落进
                // `old_score` / `new_score` 两列。此前只拼进 `reason` 文本，界面
                // 得靠正则从人话里抠数字，格式一改整列「分数变化」就没了。
                let _ = store.log_score_change(
                    &job.platform,
                    &format!(
                        "{}/{}",
                        job.company.as_deref().unwrap_or("-"),
                        job.title.as_deref().unwrap_or("-")
                    ),
                    "rescore",
                    &format!(
                        "{} → {}（{}）{}",
                        old_label(&job),
                        new_score,
                        verdict.source,
                        verdict.reason
                    ),
                    // 旧分是 NULL（未打分的行）时传 None：写成 0 会让人以为
                    // 「原来是 0 分」，界面按 `→ N` 渲染才是真话。
                    job.match_score,
                    Some(new_score),
                );
            }
        }
        out.changes.push(change);
    }

    // 本批不足一页，说明已经到底
    if out.scanned < page {
        out.next_cursor = None;
    }
    out
}

fn old_label(job: &Job) -> String {
    match job.match_score {
        Some(s) => s.to_string(),
        // 旧制未迁移的行分数可能是 NULL，直接写 0 会让人误以为「原来是 0 分」
        None => "无分".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::job::JobQuery;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// 内存版 JobStore：只为验证「读 → 打分 → 写回」的循环，不涉及 SQL。
    #[derive(Default)]
    struct MemStore {
        rows: RefCell<BTreeMap<String, Job>>,
        updates: RefCell<Vec<(String, u8, String)>>,
        decisions: RefCell<Vec<(String, String)>>,
        /// `(decision, old, new)`：验证两列真的被写进来，而不是只拼进 reason
        score_logs: RefCell<Vec<(String, Option<u8>, Option<u8>)>>,
        fail_on: Option<String>,
    }

    impl JobStore for MemStore {
        type Error = String;

        fn insert_job(&self, job: &Job) -> Result<bool, String> {
            self.rows
                .borrow_mut()
                .insert(job.job_id.clone(), job.clone());
            Ok(true)
        }
        fn get_job(&self, job_id: &str) -> Result<Option<Job>, String> {
            Ok(self.rows.borrow().get(job_id).cloned())
        }
        fn list_jobs(&self, _q: &JobQuery) -> Result<Vec<Job>, String> {
            Ok(self.rows.borrow().values().cloned().collect())
        }
        fn update_score(
            &self,
            job_id: &str,
            score: u8,
            source: ScoreSource,
            reason: &str,
        ) -> Result<(), String> {
            if self.fail_on.as_deref() == Some(job_id) {
                return Err("模拟写库失败".to_string());
            }
            if let Some(j) = self.rows.borrow_mut().get_mut(job_id) {
                j.match_score = Some(score);
                j.score_source = Some(source.as_str().to_string());
                j.score_reason = Some(reason.to_string());
            }
            self.updates
                .borrow_mut()
                .push((job_id.to_string(), score, reason.to_string()));
            Ok(())
        }
        fn list_scored_ids(
            &self,
            after: Option<&str>,
            limit: u32,
        ) -> Result<Vec<Job>, String> {
            Ok(self
                .rows
                .borrow()
                .values()
                .filter(|j| match after {
                    Some(c) => j.job_id.as_str() > c,
                    None => true,
                })
                .take(limit.max(1) as usize)
                .cloned()
                .collect())
        }
        fn log_decision(
            &self,
            _platform: &str,
            company_job: &str,
            decision: &str,
            _reason: &str,
        ) -> Result<(), String> {
            self.decisions
                .borrow_mut()
                .push((company_job.to_string(), decision.to_string()));
            Ok(())
        }
        fn log_score_change(
            &self,
            platform: &str,
            company_job: &str,
            decision: &str,
            reason: &str,
            old_score: Option<u8>,
            new_score: Option<u8>,
        ) -> Result<(), String> {
            self.log_decision(platform, company_job, decision, reason)?;
            self.score_logs
                .borrow_mut()
                .push((decision.to_string(), old_score, new_score));
            Ok(())
        }
    }

    fn job(id: &str, score: Option<u8>) -> Job {
        let jd = format!(
            "{} 负责后端服务开发",
            crate::core::domain::profile::DEFAULT_PROFILE.core_skills.join(" ")
        );
        Job {
            job_id: id.to_string(),
            platform: "boss".to_string(),
            status: "pending".to_string(),
            title: Some("Java 后端工程师".to_string()),
            company: Some("某某科技有限公司".to_string()),
            salary: Some("20-35K".to_string()),
            experience: Some("5-10年".to_string()),
            jd_text: Some(jd),
            match_score: score,
            ..Default::default()
        }
    }

    fn store_with(ids: &[(&str, Option<u8>)]) -> MemStore {
        let s = MemStore::default();
        for (id, score) in ids {
            s.rows
                .borrow_mut()
                .insert(id.to_string(), job(id, *score));
        }
        s
    }

    #[test]
    fn rescore_updates_every_row_and_records_changes() {
        // 旧制 7 分是「差匹配」，用新口径重算应得到真实分数而不是 7
        let store = store_with(&[("a", Some(7))]);
        let opts = RescoreOptions {
            log_changes: true,
            ..Default::default()
        };
        let out = rescore_batch(
            &store,
            &crate::core::domain::profile::DEFAULT_PROFILE,
            &opts,
            None,
        );
        assert_eq!(out.scanned, 1);
        assert_eq!(out.updated, 1);
        assert_eq!(out.failed, 0);
        let c = &out.changes[0];
        assert_eq!(c.old_score, Some(7));
        assert!(c.new_score > 7, "重算后的分数应远高于旧制 7 分");
        assert_eq!(store.updates.borrow().len(), 1);
        assert_eq!(store.decisions.borrow().len(), 1, "变更应留一条决策日志");

        // 两端分数必须走带分量的写入口：只写 reason 文本会让界面的
        // 「分数变化」列退回正则抠数，格式一改就整列变成 `—`。
        let logs = store.score_logs.borrow();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].0, "rescore");
        assert_eq!(logs[0].1, Some(7), "old_score 必须是重算前的分数");
        assert_eq!(logs[0].2, Some(c.new_score), "new_score 必须是重算后的分数");
    }

    /// 未打分的行（`match_score = None`）重算时旧分两列必须是 `NULL`，
    /// 不是 0——写成 0 会让界面显示 `0 → 85`，等于凭空说「原来是 0 分」。
    #[test]
    fn rescore_logs_null_old_score_for_unscored_rows() {
        let store = store_with(&[("a", None)]);
        let out = rescore_batch(
            &store,
            &crate::core::domain::profile::DEFAULT_PROFILE,
            &RescoreOptions {
                log_changes: true,
                ..Default::default()
            },
            None,
        );
        assert_eq!(out.updated, 1);
        let logs = store.score_logs.borrow();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].1, None);
        assert_eq!(logs[0].2, Some(out.changes[0].new_score));
    }

    #[test]
    fn rescore_counts_unchanged_rows_as_scanned_not_updated() {
        let store = store_with(&[("a", None)]);
        // 先算一次拿到基准分，再原样写回
        let opts = RescoreOptions::default();
        let first = rescore_batch(
            &store,
            &crate::core::domain::profile::DEFAULT_PROFILE,
            &opts,
            None,
        );
        let base = first.changes[0].new_score;
        store.rows.borrow_mut().get_mut("a").unwrap().match_score = Some(base);

        let second = rescore_batch(
            &store,
            &crate::core::domain::profile::DEFAULT_PROFILE,
            &opts,
            None,
        );
        assert_eq!(second.scanned, 1);
        assert_eq!(second.updated, 0, "分数没变不该计入 updated");
    }

    #[test]
    fn rescore_pages_with_cursor_until_exhausted() {
        let store = store_with(&[("a", None), ("b", None), ("c", None)]);
        let profile = crate::core::domain::profile::DEFAULT_PROFILE;
        let mut cursor: Option<String> = None;
        let mut seen = Vec::new();
        for _ in 0..10 {
            let opts = RescoreOptions {
                after_job_id: cursor.clone(),
                limit: Some(2),
                ..Default::default()
            };
            let out = rescore_batch(&store, &profile, &opts, None);
            seen.extend(out.changes.iter().map(|c| c.job_id.clone()));
            if !out.has_more() {
                break;
            }
            cursor = out.next_cursor;
        }
        assert_eq!(seen, vec!["a", "b", "c"], "分页必须走遍且不重不漏");
    }

    #[test]
    fn rescore_single_write_failure_does_not_abort_batch() {
        let mut store = store_with(&[("a", None), ("b", None)]);
        store.fail_on = Some("a".to_string());
        let out = rescore_batch(
            &store,
            &crate::core::domain::profile::DEFAULT_PROFILE,
            &RescoreOptions::default(),
            None,
        );
        assert_eq!(out.scanned, 2);
        assert_eq!(out.failed, 1);
        assert_eq!(store.updates.borrow().len(), 1, "b 仍应被写入");
    }

    #[test]
    fn rescore_logs_none_when_changes_are_disabled() {
        let store = store_with(&[("a", Some(7))]);
        let out = rescore_batch(
            &store,
            &crate::core::domain::profile::DEFAULT_PROFILE,
            &RescoreOptions {
                log_changes: false,
                ..Default::default()
            },
            None,
        );
        assert_eq!(out.updated, 1);
        assert!(
            store.decisions.borrow().is_empty(),
            "关掉日志时不应写 jd_decisions"
        );
    }

    #[test]
    fn rescore_limit_zero_still_makes_progress() {
        // 调用方传 Some(0) 时必须仍然推进游标，否则续算会原地打转
        let store = store_with(&[("a", None), ("b", None)]);
        let out = rescore_batch(
            &store,
            &crate::core::domain::profile::DEFAULT_PROFILE,
            &RescoreOptions {
                limit: Some(0),
                ..Default::default()
            },
            None,
        );
        assert_eq!(out.scanned, 1);
        assert!(out.next_cursor.is_some());
    }

    #[test]
    fn rescore_on_empty_store_is_a_noop() {
        let store = MemStore::default();
        let out = rescore_batch(
            &store,
            &crate::core::domain::profile::DEFAULT_PROFILE,
            &RescoreOptions::default(),
            None,
        );
        assert_eq!(out.scanned, 0);
        assert!(!out.has_more());
    }
}
