//! 抓取结果 → 入库判定（SPEC-002 第 2.1 节：**判定全部归 Rust**）。
//!
//! sidecar 只回传原始字段，不打分也不落库。本模块负责把一条
//! [`ScrapedJob`] 变成可以交给 `JobStore::insert_job` 的 [`Job`]，
//! 并把「不入库」的理由也变成结构化结果——否则用户只看到岗位少了，
//! 不知道是被分数门槛筛掉、还是重复、还是被规则否决。
//!
//! 本模块是**纯函数**：不碰数据库、不碰进程，毫秒级可测。

use crate::core::domain::job::Job;
use crate::core::domain::profile::Profile;
use crate::core::domain::rules::{reject_labeled, ExcludeRules, JobInput};
use crate::core::domain::scoring::{rule_score_all, RuleScore};
use crate::core::sidecar::protocol::ScrapedJob;

/// 一条抓取结果的处置结论。
///
/// 不派生 `PartialEq`：明细里带 [`Job`]，为它引入相等语义会牵连 28 个可选字段；
/// 测试用 `matches!` 与字段断言即可。
#[derive(Debug, Clone)]
pub enum IngestVerdict {
    /// 达到门槛，应当入库（是否真的写进去由调用方调 `insert_job` 决定）
    Accepted(IngestDetail),
    /// 被硬规则否决。
    ///
    /// `rule` 是**稳定的规则标识**（`weekend` / `outsource` / `prescan:salary_floor`
    /// / `sensitive:stored` …），不是给用户看的文案：决策日志靠它回答「到底被
    /// 哪条规则否掉」，文案改了也不该让统计分档漂移。
    Rejected { rule: String, reason: String },
    /// 低于分数门槛，或缺少必要字段
    BelowThreshold {
        score: u8,
        min_score: u8,
        reason: String,
    },
}

impl IngestVerdict {
    /// 是否应当调用 `insert_job`。
    pub fn accepted(&self) -> bool {
        matches!(self, IngestVerdict::Accepted(_))
    }

    /// 给用户看的一句话说明。
    pub fn summary(&self) -> String {
        match self {
            IngestVerdict::Accepted(d) => format!("{} 分：{}", d.score, d.reason),
            IngestVerdict::Rejected { rule, reason } => format!("排除（{rule}）：{reason}"),
            IngestVerdict::BelowThreshold {
                score,
                min_score,
                reason,
            } => format!("{score} 分未达门槛 {min_score}：{reason}"),
        }
    }
}

/// 入库明细。
#[derive(Debug, Clone, Default)]
pub struct IngestDetail {
    pub job_id: String,
    pub score: u8,
    pub reason: String,
    /// 已完成打分、可直接写入的实体
    pub job: Job,
}

/// 入库参数。
#[derive(Debug, Clone)]
pub struct IngestConfig {
    /// 分数门槛；低于它的不入库
    pub min_score: u8,
    /// 排除规则。默认值来自 [`ExcludeRules::default`]，与现行版一致。
    pub rules: ExcludeRules,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            min_score: crate::core::scoring::SCORE_MIN_DEFAULT,
            rules: ExcludeRules::default(),
        }
    }
}

/// 把抓取结果映射成领域实体。
///
/// `ScrapedJob` 没有的字段（`hiring_count` / `rejected` / 分数三件套）
/// 保持默认值：`match_score` 必须由 [`prepare`] 统一填，
/// 否则会出现「入库了但没分」的脏行。
pub fn to_job(s: &ScrapedJob) -> Job {
    Job {
        job_id: s.job_id.trim().to_string(),
        platform: if s.platform.is_empty() {
            "boss".to_string()
        } else {
            s.platform.clone()
        },
        title: s.title.clone(),
        company: s.company.clone(),
        salary: s.salary.clone(),
        status: crate::core::domain::job::JobStatus::Pending.as_str().to_string(),
        // 0 = 由存储层填当前时间，避免这里各自取一次时钟
        created_at: 0,
        job_url: s.job_url.clone(),
        jd_text: s.jd_text.clone(),
        location: s.location.clone(),
        experience: s.experience.clone(),
        degree: s.degree.clone(),
        hr_name: s.hr_name.clone(),
        hr_position: s.hr_position.clone(),
        hr_active_status: s.hr_active_status.clone(),
        company_industry: s.company_industry.clone(),
        company_scale: s.company_scale.clone(),
        company_stage: s.company_stage.clone(),
        company_address: s.company_address.clone(),
        company_intro: s.company_intro.clone(),
        hiring_count: None,
        founded_years: s.founded_years.map(|v| v as i64),
        match_score: None,
        rejected: 0,
        score_source: None,
        score_reason: None,
        updated_at: None,
        score_scale: None,
    }
}

/// 打分并判定是否入库。
///
/// 顺序不可调换（对齐 `backend/app/scorer.py:612 Scorer.reject` 的调用口径）：
/// 1. 文本字段清洗（全空白视为没抓到）
/// 2. 必要字段校验（`job_id` 为空则根本无法去重）
/// 3. 硬规则否决（[`reject`]，十条规则顺序固定，否决优先于分数）
/// 4. 规则打分 → 门槛判定
pub fn prepare(s: &ScrapedJob, profile: &Profile, cfg: &IngestConfig) -> IngestVerdict {
    let mut job = to_job(s);
    normalize(&mut job);

    if job.job_id.is_empty() {
        return IngestVerdict::BelowThreshold {
            score: 0,
            min_score: cfg.min_score,
            reason: "缺少 job_id，无法入库".to_string(),
        };
    }
    if blank_option(&job.title) && blank_option(&job.jd_text) {
        return IngestVerdict::BelowThreshold {
            score: 0,
            min_score: cfg.min_score,
            reason: "标题与 JD 均为空，视为无效抓取".to_string(),
        };
    }

    let rule = rule_score_all(
        job.company.as_deref(),
        job.title.as_deref(),
        job.salary.as_deref(),
        job.experience.as_deref(),
        job.jd_text.as_deref(),
        profile,
    );
    let RuleScore { score, reason, .. } = rule.clone();

    // 成立年数与 HR 活跃度必须一并传入：`exclude_founded_below` /
    // `exclude_hr_active_days` 两条规则的判定完全依赖它们，
    // 漏传会让这两条规则静默失效（默认配置下不排除，所以不会报错）。
    let input = JobInput {
        company: job.company.as_deref(),
        title: job.title.as_deref(),
        salary: job.salary.as_deref(),
        experience: job.experience.as_deref(),
        jd: job.jd_text.as_deref(),
        founded_years: job.founded_years.map(|v| v.clamp(0, u32::MAX as i64) as u32),
        hr_active_status: job.hr_active_status.as_deref(),
    };
    if let Some((rule, why)) = reject_labeled(&input, &cfg.rules) {
        return IngestVerdict::Rejected {
            rule: rule.to_string(),
            reason: format!("{why}（规则分 {score}）"),
        };
    }

    // 门槛判定必须走 `RuleScore::passes`：它同时检查「无 JD 否决」。
    // 这里原先只比 `score < cfg.min_score`，而 `rejected_no_jd` 的岗位在
    // 薪资与经验都匹配时能拿到 57~72 分，门槛调到 70 以下就会被静默入库，
    // 直接违反 SPEC-002 第 3.1 节「无 JD 一律判不入库」。
    if !rule.passes(cfg.min_score) {
        return IngestVerdict::BelowThreshold {
            score,
            min_score: cfg.min_score,
            reason,
        };
    }

    job.match_score = Some(score);
    job.score_source = Some("rule".to_string());
    job.score_reason = Some(reason.clone());
    IngestVerdict::Accepted(IngestDetail {
        job_id: job.job_id.clone(),
        score,
        reason,
        job,
    })
}

/// 文本清洗：去首尾空白，空白串归一成 `None`。
///
/// Boss 的 DOM 里空元素会给出 `Some("")` 或 `Some(" ")`，
/// 直接入库会让前端显示成空块，而不是「未抓取」。
pub fn normalize(job: &mut Job) {
    for f in [
        &mut job.title,
        &mut job.company,
        &mut job.salary,
        &mut job.location,
        &mut job.experience,
        &mut job.degree,
        &mut job.hr_name,
        &mut job.hr_position,
        &mut job.hr_active_status,
        &mut job.company_industry,
        &mut job.company_scale,
        &mut job.company_stage,
        &mut job.company_address,
        &mut job.company_intro,
        &mut job.job_url,
    ] {
        trim_into_none(f);
    }
    // JD 单独处理：它是打分主依据，空白 JD 必须真的变成 None，
    // 否则打分器会把 `Some("")` 当成「有 JD 但内容为空」，绕开无 JD 否决
    trim_into_none(&mut job.jd_text);
}

fn trim_into_none(f: &mut Option<String>) {
    if let Some(v) = f {
        let t = v.trim();
        if t.is_empty() {
            *f = None;
        } else if t.len() != v.len() {
            *f = Some(t.to_string());
        }
    }
}

fn blank_option(v: &Option<String>) -> bool {
    v.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true)
}

/// 写入 `jd_decisions` 的一条决策（纯数据，不含任何存储细节）。
///
/// 本模块仍然是纯函数：这里只**构造**要写的内容，真正落库由调用方通过
/// [`IngestPort::record`](crate::core::sidecar::session::IngestPort::record)
/// 注入的端口完成。这样「什么算一条决策」可以单测，「怎么写进去」留在 infra。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionNote {
    pub platform: String,
    /// 写成 `公司/岗位`（历史格式，前端按最后一个 `/` 拆分）。
    pub company_job: String,
    /// `accept` / `reject` / `skip`
    pub decision: &'static str,
    /// 稳定的规则标识（`weekend` / `outsource` / `sensitive` / …），用于回答
    /// 「到底被哪条规则否掉」。**不是**从 `reason` 文本反推的——那会在改一次
    /// 文案时静默失配。不入库（`DecisionRecord` 字段零变化），只进 `reason`。
    pub rule: &'static str,
    /// 给人看的一句话
    pub reason: String,
    /// 入库时是入库分；被规则否决时是规则分；其余为 `None`
    pub new_score: Option<u8>,
}

impl DecisionNote {
    /// 决策日志的 company 侧：空值写 `-`，避免拼出 `/岗位` 被前端拆成「未知公司 + 岗位名」以外的形态。
    fn company_job_of(job: &Job) -> String {
        format!(
            "{}/{}",
            job.company.as_deref().unwrap_or("-"),
            job.title.as_deref().unwrap_or("-")
        )
    }
}

/// 把一条判定结论翻译成决策日志条目。
///
/// **每种结论都必须有去处**，否则前端筛选档会静默变空——这正是本次要修的 bug：
/// 生产路径上 `reject` / `accept` 从未被写过，决策日志里只剩 `rescore`。
///
/// - 入库 → `accept`，`new_score` 是入库分
/// - 硬规则否决 → `reject`，`reason` 带规则名与规则分
/// - 其余（低于门槛 / 缺字段）→ `skip`，前端归到「其它」档
///
/// 用 `skip` 而不是把门槛不足也记成 `reject`：用户点「排除」筛选时想看的是
/// **规则判死的**岗位，把分数不够的混进去会让「为什么这个岗位被排除」更难回答。
///
/// `rule` 是 `IngestVerdict::Rejected` 里的规则标识，由调用方在构造否决结论时
/// 就给准（规则否决 / 卡片规则 / 敏感库 / 空 JD …）。这里只负责把它并进 `reason`
/// 的第一段。
pub fn decision_note(s: &ScrapedJob, verdict: &IngestVerdict) -> DecisionNote {
    // 被否决时 `IngestVerdict` 里不带 Job（规则否决发生在打分成功之前），
    // 所以平台与公司/岗位取自原始抓取结果，走与 `prepare` 同一套归一化。
    let job = {
        let mut j = to_job(s);
        normalize(&mut j);
        j
    };
    match verdict {
        IngestVerdict::Accepted(d) => DecisionNote {
            platform: job.platform.clone(),
            company_job: DecisionNote::company_job_of(&d.job),
            decision: "accept",
            rule: "score",
            reason: format!("{} 分：{}", d.score, d.reason),
            new_score: Some(d.score),
        },
        IngestVerdict::Rejected { rule, reason } => DecisionNote {
            platform: job.platform.clone(),
            company_job: DecisionNote::company_job_of(&job),
            decision: "reject",
            // 规则标识必须是 `'static`：中文文案随规则改，中文标识会让统计漂移
            rule: rule_ident(rule),
            reason: format!("{rule}：{reason}"),
            // 规则分从 reason 里读不到数值就不写，宁可空着也不猜
            new_score: None,
        },
        IngestVerdict::BelowThreshold {
            score,
            min_score,
            reason,
        } => DecisionNote {
            platform: job.platform.clone(),
            company_job: DecisionNote::company_job_of(&job),
            decision: "skip",
            reason: format!("{score} 分未达门槛 {min_score}：{reason}"),
            new_score: None,
            rule: "threshold",
        },
    }
}

/// `IngestVerdict::Rejected.rule` 里可能带参数（`prescan:weekend`、`sensitive:stored`），
/// 决策日志只需要前半段的规则标识。
///
/// 认不出来的规则名（调用方自造的）一律落到 `"rule"`：宁可粗一档，也不能让
/// 某个未来的否决结论在决策日志里静默变成空标识。
fn rule_ident(rule: &str) -> &'static str {
    const KNOWN: &[&str] = &[
        "weekend",
        "name_word",
        "short_term",
        "outsource",
        "degree",
        "degree_985",
        "experience",
        "hr_active_unknown",
        "hr_inactive",
        "founded",
        "salary_floor",
        "salary_ceiling",
        "salary_range",
        "prescan",
        "sensitive",
        "duplicate",
        "exclude",
    ];
    let head = rule.split(':').next().unwrap_or(rule);
    KNOWN.iter().copied().find(|k| *k == head).unwrap_or("rule")
}

/// 卡片阶段就做出结论的预筛结果。
///
/// 目的只有一个：**别为一条注定要扔的岗位打开详情页**。详情页每条 3~6 秒，
/// 一次运行上百条，省下来的是分钟级的时间。
///
/// 但预筛必须**保守**：宁可多开一次详情页，也不能把本该入库的岗位扔掉。
/// 所以这里只判「不依赖 JD 就能定论」的条件，凡是 JD 相关的规则一律留到
/// 详情页之后的 [`prepare`]。
#[derive(Debug, Clone)]
pub enum PreScan {
    /// 卡组里已经有同 `job_id` 的卡片：详情抓一次就够
    Duplicate,
    /// 敏感库命中（之前抓过/排除过）
    Sensitive { reason: String, stored: bool },
    /// 不依赖 JD 的硬规则命中
    Rule { rule: &'static str, reason: String },
}

impl PreScan {
    /// 给用户看的一句话。
    pub fn summary(&self) -> String {
        match self {
            PreScan::Duplicate => "卡片重复，跳过".to_string(),
            PreScan::Sensitive { reason, stored: true } => {
                format!("已在库中，跳过详情（{reason}）")
            }
            PreScan::Sensitive { reason, .. } => format!("命中敏感库，跳过详情（{reason}）"),
            PreScan::Rule { reason, .. } => format!("卡片阶段即被规则排除：{reason}"),
        }
    }

    /// 写进 `IngestVerdict::Rejected.rule` 的标识。
    ///
    /// 敏感库与卡片规则必须是**两个不同的口径**（用户裁决里它们分别统计），
    /// 卡片规则再带上具体规则名——否则决策日志里只会剩一个笼统的「被预筛否掉」。
    pub fn rule_ident(&self) -> String {
        match self {
            PreScan::Duplicate => "duplicate".to_string(),
            PreScan::Sensitive { stored: true, .. } => "sensitive:stored".to_string(),
            PreScan::Sensitive { .. } => "sensitive:rejected".to_string(),
            PreScan::Rule { rule, .. } => format!("prescan:{rule}"),
        }
    }
}

/// 卡片阶段的预筛。
///
/// `sensitive` 由调用方注入（core 不直接碰数据库，保持纯函数可测）：
/// 传入 `|platform, company, title| -> Option<(reason, stored)>`。
///
/// **为什么不自己写一套规则判断**：硬规则的判定顺序决定用户看到的排除文案
/// （`rules.rs` 顶部明写「顺序不可重排」）。这里直接拿空 JD 跑一遍完整的
/// [`reject`]，只会命中「不依赖 JD」的那几条；凡是需要 JD 才能判的规则
/// 在空 JD 下必然不命中，等价于自动跳过。这样以后给 `reject` 增删规则时，
/// 预筛不会与它悄悄分叉。
pub fn prescan<F>(s: &ScrapedJob, cfg: &IngestConfig, mut sensitive: F) -> Option<PreScan>
where
    F: FnMut(&str, Option<&str>, Option<&str>) -> Option<(String, bool)>,
{
    let mut job = to_job(s);
    normalize(&mut job);

    if job.job_id.is_empty() {
        return Some(PreScan::Duplicate);
    }

    // 1) 敏感库：键是平台 + 公司 + 岗位名（不含 job_id，见 SensitiveStore 的说明）
    if let Some((reason, stored)) = sensitive(
        &job.platform,
        job.company.as_deref(),
        job.title.as_deref(),
    ) {
        return Some(PreScan::Sensitive { reason, stored });
    }

    // 2) 不依赖 JD 的硬规则：空 JD 跑一遍完整 reject，只可能命中这类
    let input = JobInput {
        company: job.company.as_deref(),
        title: job.title.as_deref(),
        salary: job.salary.as_deref(),
        experience: job.experience.as_deref(),
        jd: None, // 关键：没有 JD
        founded_years: job.founded_years.map(|v| v.clamp(0, u32::MAX as i64) as u32),
        hr_active_status: job.hr_active_status.as_deref(),
    };
    if let Some((rule, reason)) = reject_labeled(&input, &cfg.rules) {
        return Some(PreScan::Rule { rule, reason });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::profile::DEFAULT_PROFILE;

    fn good() -> ScrapedJob {
        ScrapedJob {
            job_id: "boss:101".into(),
            platform: "boss".into(),
            title: Some("Java 后端开发".into()),
            company: Some("上海某某信息科技有限公司".into()),
            salary: Some("25-40K".into()),
            experience: Some("5-10年".into()),
            degree: Some("本科".into()),
            location: Some("上海".into()),
            job_url: Some("https://www.zhipin.com/job_detail/101.html".into()),
            jd_text: Some(format!(
                "负责后端服务开发，熟悉 {}，有微服务经验。",
                DEFAULT_PROFILE.core_skills.join("、")
            )),
            hr_active_status: Some("今日活跃".into()),
            founded_years: Some(8),
            ..Default::default()
        }
    }

    #[test]
    fn to_job_carries_all_fields() {
        let j = to_job(&good());
        assert_eq!(j.job_id, "boss:101");
        assert_eq!(j.platform, "boss");
        assert_eq!(j.status, "pending");
        assert_eq!(j.founded_years, Some(8));
        assert!(j.match_score.is_none(), "打分必须由 prepare 统一填");
    }

    /// 判定 → 决策日志条目的翻译必须**每种结论都有去处**。
    ///
    /// 这条链此前整段缺失：生产路径上 `reject` / `accept` 从未写入，
    /// 界面决策日志里只剩 `rescore`，用户问「这个岗位为什么被排除」时无从回答。
    #[test]
    fn decision_note_maps_every_verdict() {
        let s = good();
        let cfg = IngestConfig {
            min_score: 0,
            ..Default::default()
        };
        match prepare(&s, &DEFAULT_PROFILE, &cfg) {
            IngestVerdict::Accepted(d) => {
                let n = decision_note(&s, &IngestVerdict::Accepted(d.clone()));
                assert_eq!(n.decision, "accept");
                assert_eq!(n.company_job, "上海某某信息科技有限公司/Java 后端开发");
                assert_eq!(n.new_score, Some(d.score));
                assert_eq!(n.platform, "boss");
            }
            other => panic!("实际 {other:?}"),
        }

        let verdict = IngestVerdict::Rejected {
            rule: "exclude".into(),
            reason: "外包".into(),
        };
        let n = decision_note(&s, &verdict);
        assert_eq!(n.decision, "reject");
        assert_eq!(n.reason, "exclude：外包");
        assert_eq!(n.new_score, None);

        let verdict = IngestVerdict::BelowThreshold {
            score: 55,
            min_score: 70,
            reason: "技能不符".into(),
        };
        let n = decision_note(&s, &verdict);
        assert_eq!(n.decision, "skip", "门槛不足不是规则否决");
        assert!(n.reason.contains("55"), "{}", n.reason);
        assert!(n.reason.contains("70"), "{}", n.reason);

        // 缺公司/标题时不能拼出空串，否则前端会画出空行
        let bare = ScrapedJob {
            job_id: "boss:9".into(),
            platform: String::new(),
            ..Default::default()
        };
        let n = decision_note(&bare, &IngestVerdict::Rejected {
            rule: "exclude".into(),
            reason: "空 JD".into(),
        });
        assert_eq!(n.company_job, "-/-");
        assert_eq!(n.platform, "boss", "平台为空时与 to_job 一致回落 boss");
    }

    #[test]
    fn prepare_scores_and_marks_source() {
        let cfg = IngestConfig {
            min_score: 0,
            ..Default::default()
        };
        match prepare(&good(), &DEFAULT_PROFILE, &cfg) {
            IngestVerdict::Accepted(d) => {
                assert!(d.score > 0 && d.score <= 100, "实际 {}", d.score);
                assert_eq!(d.job.score_source.as_deref(), Some("rule"));
                assert_eq!(d.job.match_score, Some(d.score));
                assert_eq!(d.job.rejected, 0);
                assert!(!d.reason.is_empty());
            }
            other => panic!("应入库，实际 {other:?}"),
        }
    }

    #[test]
    fn prepare_blocks_below_threshold() {
        let cfg = IngestConfig {
            min_score: 100,
            ..Default::default()
        };
        let v = prepare(&good(), &DEFAULT_PROFILE, &cfg);
        match &v {
            IngestVerdict::BelowThreshold { min_score, .. } => assert_eq!(*min_score, 100),
            other => panic!("应被门槛拦下，实际 {other:?}"),
        }
        assert!(!v.accepted());
    }

    #[test]
    fn prepare_rejects_empty_title_and_jd() {
        let mut s = good();
        s.title = Some("   ".into());
        s.jd_text = None;
        let v = prepare(&s, &DEFAULT_PROFILE, &IngestConfig::default());
        assert!(matches!(v, IngestVerdict::BelowThreshold { score: 0, .. }));
    }

    #[test]
    fn prepare_blocks_no_jd_even_when_threshold_is_low() {
        // 回归：这里原先只比 `score < min_score`，漏掉了 `rejected_no_jd`。
        // 无 JD 岗位在薪资与经验都匹配时可拿到 57~72 分，门槛调到 70 以下
        // 就会被静默入库，违反 SPEC-002 第 3.1 节。把门槛压到 0 是最强对照：
        // 分数门槛完全不设限，仍必须被「无 JD 否决」挡住。
        let mut s = good();
        s.jd_text = Some("   ".into());
        let cfg = IngestConfig {
            min_score: 0,
            ..Default::default()
        };
        let v = prepare(&s, &DEFAULT_PROFILE, &cfg);
        assert!(!v.accepted(), "无 JD 必须不入库，实际 {v:?}");
        match v {
            IngestVerdict::BelowThreshold { score, reason, .. } => {
                assert!(score < 100, "无 JD 的保守分不应冲到满分，实际 {score}");
                assert!(reason.contains("无 JD"), "理由须说明病根，实际 {reason}");
            }
            other => panic!("应落 BelowThreshold，实际 {other:?}"),
        }
    }

    #[test]
    fn prepare_requires_job_id() {
        let mut s = good();
        s.job_id = " ".into();
        let v = prepare(&s, &DEFAULT_PROFILE, &IngestConfig::default());
        assert!(matches!(v, IngestVerdict::BelowThreshold { .. }));
    }

    #[test]
    fn prepare_applies_hard_rules_before_score() {
        // 命中默认排除词「外包」：即便分数够也不能入库
        let mut s = good();
        s.title = Some("Java 外包开发".into());
        let cfg = IngestConfig {
            min_score: 0,
            ..Default::default()
        };
        let v = prepare(&s, &DEFAULT_PROFILE, &cfg);
        assert!(matches!(v, IngestVerdict::Rejected { .. }), "实际 {v:?}");
        assert!(v.summary().contains("外包"));
    }

    #[test]
    fn normalize_blank_strings_to_none() {
        let mut j = to_job(&good());
        j.degree = Some("  ".into());
        j.company = Some("  上海某某信息科技有限公司 ".into());
        normalize(&mut j);
        assert!(j.degree.is_none(), "空白串应归一为 None");
        assert_eq!(j.company.as_deref(), Some("上海某某信息科技有限公司"));
    }

    #[test]
    fn blank_jd_becomes_none_so_no_jd_veto_works() {
        let mut s = good();
        s.jd_text = Some("   ".into());
        let job = {
            let mut j = to_job(&s);
            normalize(&mut j);
            j
        };
        assert!(job.jd_text.is_none(), "空白 JD 不得残留成 Some(\"\")");
    }

    #[test]
    fn empty_platform_defaults_to_boss() {
        let mut s = good();
        s.platform = String::new();
        assert_eq!(to_job(&s).platform, "boss");
    }
}
