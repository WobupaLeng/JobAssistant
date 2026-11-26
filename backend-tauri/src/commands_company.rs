//! Tauri 命令层：按公司名去 Boss 搜岗位，再把选中的那条加进待投递列表。
//!
//! 与 `commands_chat.rs` 的分工：那边管会话流（以及「会话先来、岗位没抓过」的补抓），
//! 这边管用户主动发起的公司搜索。读详情页的能力是同一份
//! [`job_from_detail`]，区别只在落库时机——这里只有用户点「加入待投递」才写库。
//!
//! **本模块不写业务规则**：清洗、硬规则与分数一律走 `core::sidecar::ingest`
//! 与 `core::domain::scoring`，命令层只做参数校验与接线。

use crate::commands_chat::{handle_of, job_from_detail, request_chat};
use crate::commands_db::{store_err, with_store, CmdResult};
use crate::commands_scrape::ScrapeState;
use crate::core::domain::job::{Job, JobStore};
use crate::core::domain::profile::DEFAULT_PROFILE;
use crate::core::domain::scoring::rule_score_all;
use crate::core::sidecar::ingest::{self, IngestConfig, IngestVerdict};
use crate::core::sidecar::protocol::{Command, ScrapedJob};
use serde::{Deserialize, Serialize};

/// 搜索结果里的一条岗位（只含界面要显示的字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyJobItem {
    /// Boss 的加密岗位 id（不带 `boss:` 前缀）
    pub job_id: String,
    pub title: String,
    pub salary: String,
    /// 接口说的公司名（可能和用户输入的不同：包含匹配时就是差在这里）
    pub company: String,
    pub location: String,
    pub experience: String,
    pub degree: String,
    pub url: String,
}

/// 公司搜索结果。
#[derive(Debug, Clone, Serialize)]
pub struct CompanyJobsReply {
    /// 用户输入的公司名，原样回显
    pub company: String,
    /// `exact` = 严格同名；`contains` = 公司名包含关键词；`none` = 没搜到
    pub matched: String,
    pub jobs: Vec<CompanyJobItem>,
}

/// 加入待投递的入参。
///
/// 字段全部可选：搜索接口给什么就传什么，缺的（比如 JD）由命令层去详情页补。
#[derive(Debug, Deserialize)]
pub struct JobImportArgs {
    #[serde(default, alias = "jobId")]
    pub job_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub salary: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub experience: Option<String>,
    #[serde(default)]
    pub degree: Option<String>,
    #[serde(default, alias = "jobUrl")]
    pub job_url: Option<String>,
    /// JD 正文。为空时命令层去详情页读一次（只读，不改任何状态）。
    #[serde(default)]
    pub jd: Option<String>,
}

/// 按公司名去 Boss 搜岗位（只读：不落库、不投递）。
#[tauri::command]
pub async fn company_jobs(
    state: tauri::State<'_, ScrapeState>,
    company: String,
) -> CmdResult<CompanyJobsReply> {
    let want = company.trim().to_string();
    if want.is_empty() {
        return Err("先填公司名再搜".to_string());
    }
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::CompanyJobs { company: want.clone() })?;
    let matched = data
        .get("matched")
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();
    let jobs = data
        .get("jobs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| serde_json::from_value::<CompanyJobItem>(v).ok())
        .collect();
    Ok(CompanyJobsReply {
        company: want,
        matched,
        jobs,
    })
}

/// 读一条岗位的 JD 正文给界面预览。
///
/// 与 `job_fetch_detail` 的关键差别：**不写库**。预览过的岗位在用户点「加入待投递」
/// 之前不应出现在任何列表里（用户 m01529 的裁决），所以这里只把正文读回来。
#[tauri::command]
pub async fn company_job_preview(
    state: tauri::State<'_, ScrapeState>,
    job_id: String,
) -> CmdResult<Job> {
    let jid = normalize_job_id(&job_id)?;
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ChatJobDetail { job_id: jid.clone() })?;
    job_from_detail(&jid, &data)
}

/// 把一条搜到的岗位加进待投递列表。
///
/// 判定口径与抓取入库一致（清洗 → 硬规则 → 规则打分），但**一律入库**：
/// 用户是主动挑的，过不过门槛都该出现在「未投递」里让人自己看（`rejected` 保持 0，
/// 分数与原因照常写）。重复点是幂等的：已在库直接返回原行，不重抓、不覆盖。
///
/// 入参没带 JD 时先去详情页读一次：没有正文的岗位打出来的分与「没有关联岗位」
/// 一样没意义，而用户点「加入待投递」本身就是明确的「我要这个岗位」。
#[tauri::command]
pub async fn job_import(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, crate::commands_db::StoreState>,
    args: JobImportArgs,
) -> CmdResult<Job> {
    let jid = normalize_job_id(&args.job_id)?;
    if let Some(hit) = with_store(&store, |s| s.get_job(&jid).map_err(store_err))? {
        return Ok(hit);
    }

    let mut scraped = scraped_of(&jid, &args);
    if scraped.jd_text.is_none() {
        let handle = handle_of(&state)?;
        let data = request_chat(&handle, &Command::ChatJobDetail { job_id: jid.clone() })?;
        let detail = job_from_detail(&jid, &data)?;
        // 只补空着的字段：搜索接口给的值优先（它带的是列表页口径，与抓取一致）。
        scraped.jd_text = detail.jd_text.clone();
        if scraped.title.is_none() {
            scraped.title = detail.title.clone();
        }
        if scraped.company.is_none() {
            scraped.company = detail.company.clone();
        }
        if scraped.salary.is_none() {
            scraped.salary = detail.salary.clone();
        }
        if scraped.job_url.is_none() {
            scraped.job_url = detail.job_url.clone();
        }
    }

    let verdict = ingest::prepare(&scraped, &DEFAULT_PROFILE, &IngestConfig::default());
    let mut job = ingest::to_job(&scraped);
    ingest::normalize(&mut job);

    // 分数与原因分开取：`prepare` 的 Rejected 分支不带分数（被硬规则否决时
    // 本来就不需要分），但列表里要显示分，所以统一按规则分写一遍。
    let rule = rule_score_all(
        job.company.as_deref(),
        job.title.as_deref(),
        job.salary.as_deref(),
        job.experience.as_deref(),
        job.jd_text.as_deref(),
        &DEFAULT_PROFILE,
    );
    job.match_score = Some(rule.score);
    job.score_source = Some(crate::core::domain::job::ScoreSource::Rule.as_str().to_string());
    job.score_reason = Some(match verdict_note(&verdict) {
        Some(note) => format!("{}（{note}）", rule.reason),
        None => rule.reason,
    });

    with_store(&store, |s| s.insert_job(&job).map_err(store_err))?;
    with_store(&store, |s| s.get_job(&jid).map_err(store_err))?
        .ok_or_else(|| "岗位写入后读不到，请刷新列表".to_string())
}

/// 把 `boss:<enc>` 与裸加密 id 统一成库里的写法。
fn normalize_job_id(raw: &str) -> CmdResult<String> {
    let bare = raw.trim();
    let bare = bare.strip_prefix("boss:").unwrap_or(bare).trim();
    if bare.is_empty() {
        return Err("缺少岗位 id".to_string());
    }
    Ok(format!("boss:{bare}"))
}

/// 命令层入参 → 抓取侧的岗位结构（领域映射走 `ingest::to_job`，这里只搬运）。
fn scraped_of(jid: &str, args: &JobImportArgs) -> ScrapedJob {
    let text = |v: &Option<String>| {
        v.as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    ScrapedJob {
        job_id: jid.to_string(),
        platform: "boss".to_string(),
        title: text(&args.title),
        company: text(&args.company),
        salary: text(&args.salary),
        location: text(&args.location),
        experience: text(&args.experience),
        degree: text(&args.degree),
        job_url: text(&args.job_url),
        jd_text: text(&args.jd),
        ..Default::default()
    }
}

/// 只给「分数旁边的一句话」用：说明这次为什么没过门槛 / 被哪条规则命中。
fn verdict_note(verdict: &IngestVerdict) -> Option<String> {
    match verdict {
        IngestVerdict::Accepted(_) => None,
        IngestVerdict::BelowThreshold { min_score, .. } => Some(format!("未达门槛 {min_score}")),
        IngestVerdict::Rejected { rule, reason } => {
            Some(format!("命中排除规则「{rule}」：{reason}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_id_accepts_both_forms() {
        assert_eq!(normalize_job_id("boss:abc").unwrap(), "boss:abc");
        assert_eq!(normalize_job_id(" abc ").unwrap(), "boss:abc");
        assert!(normalize_job_id("boss:").is_err());
    }

    #[test]
    fn scraped_drops_blank_fields() {
        let args = JobImportArgs {
            job_id: "enc-1".to_string(),
            title: Some("  Java开发工程师  ".to_string()),
            company: Some("   ".to_string()),
            salary: None,
            location: Some("上海".to_string()),
            experience: None,
            degree: None,
            job_url: None,
            jd: Some("负责后端".to_string()),
        };
        let s = scraped_of("boss:enc-1", &args);
        assert_eq!(s.title.as_deref(), Some("Java开发工程师"));
        assert_eq!(s.company, None);
        assert_eq!(s.location.as_deref(), Some("上海"));
        assert_eq!(s.jd_text.as_deref(), Some("负责后端"));
    }
}
