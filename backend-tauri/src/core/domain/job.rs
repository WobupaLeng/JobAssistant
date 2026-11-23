//! 岗位实体与存储端口（SPEC-002 第 4 节分层铁律）。
//!
//! 本模块只定义**数据结构与 trait**，不依赖 `rusqlite`。
//! 具体 SQLite 实现在 `crate::core::infra::sqlite`，这样领域规则可以脱离数据库测试。

use serde::{Deserialize, Serialize};

/// 岗位状态：待投递 / 已投递。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Delivered,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Delivered => "delivered",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "delivered" => JobStatus::Delivered,
            _ => JobStatus::Pending,
        }
    }
}

/// 打分来源：规则 / 大模型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScoreSource {
    Rule,
    Llm,
}

impl ScoreSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ScoreSource::Rule => "rule",
            ScoreSource::Llm => "llm",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rule" => Some(ScoreSource::Rule),
            "llm" => Some(ScoreSource::Llm),
            _ => None,
        }
    }
}

/// 一条岗位记录。字段与 `job_queue` 表逐一对应。
///
/// 大量 `Option` 是有意为之：Boss 的列表页字段经常缺失，
/// 用空串代替 `None` 会让「没拿到」和「拿到了空值」无法区分。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Job {
    pub job_id: String,
    pub platform: String,
    pub title: Option<String>,
    pub company: Option<String>,
    pub salary: Option<String>,
    pub status: String,
    pub created_at: i64,

    pub job_url: Option<String>,
    pub jd_text: Option<String>,
    pub location: Option<String>,
    pub experience: Option<String>,
    pub degree: Option<String>,
    pub hr_name: Option<String>,
    pub hr_position: Option<String>,
    pub hr_active_status: Option<String>,
    pub company_industry: Option<String>,
    pub company_scale: Option<String>,
    pub company_stage: Option<String>,
    pub company_address: Option<String>,
    pub company_intro: Option<String>,
    pub hiring_count: Option<i64>,
    pub founded_years: Option<i64>,

    /// 0-100 分（SPEC-002 D3）
    pub match_score: Option<u8>,
    pub rejected: i64,
    pub score_source: Option<String>,
    pub score_reason: Option<String>,
    pub updated_at: Option<i64>,
    /// 打分制标记：`"10"` = 旧制未迁移，`"100"` = 当前制
    pub score_scale: Option<String>,
}

impl Job {
    /// 是否已入库（未被排除）。
    pub fn is_accepted(&self) -> bool {
        self.rejected == 0
    }

    /// 分数是否达到门槛。`None` 分数视为不达标。
    pub fn meets(&self, min_score: u8) -> bool {
        self.match_score.map(|s| s >= min_score).unwrap_or(false)
    }
}

/// 存储端口：domain 只依赖这组方法签名。
///
/// 用 trait 而不是直接调 SQLite，是为了让领域规则能在无数据库的环境下测试，
/// 也是 SPEC-002 第 4 节「domain 不得依赖 rusqlite」的落地方式。
pub trait JobStore {
    type Error: std::fmt::Display;

    /// 插入岗位；`job_id` 已存在则跳过并返回 `false`。
    fn insert_job(&self, job: &Job) -> Result<bool, Self::Error>;

    /// 按 id 读取。
    fn get_job(&self, job_id: &str) -> Result<Option<Job>, Self::Error>;

    /// 列出岗位。`min_score` 过滤在本层做，避免前端重复过滤逻辑。
    fn list_jobs(&self, query: &JobQuery) -> Result<Vec<Job>, Self::Error>;

    /// 按 `job_id` 升序取一页岗位，用于全量重算（SPEC-002 第 3.3 节）。
    ///
    /// 与 [`Self::list_jobs`] 的区别是**排序键固定**：重算靠游标翻页，
    /// 若按 `match_score` 排序，写回分数会让后续页的边界漂移，同一条可能被算两次
    /// 或多条被跳过。这里不做任何过滤——被排除的岗位也要重算，
    /// 否则用户「排除后又想看」时会看到旧口径的分数。
    ///
    /// `after` 为上一页最后一条的 `job_id`（不含）；`limit` 至少为 1。
    fn list_scored_ids(
        &self,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Job>, Self::Error>;

    /// 更新指定字段。
    fn update_score(
        &self,
        job_id: &str,
        score: u8,
        source: ScoreSource,
        reason: &str,
    ) -> Result<(), Self::Error>;

    /// 记录一条过滤决策（不带分量的写入口）。
    ///
    /// 保留这个签名是为了让既有实现（内存桩、只记文本的调用方）不必改；
    /// 需要把「旧分 → 新分」真正落成列的调用方走 [`Self::log_score_change`]。
    fn log_decision(
        &self,
        platform: &str,
        company_job: &str,
        decision: &str,
        reason: &str,
    ) -> Result<(), Self::Error>;

    /// 记录一条**带分数**的决策（`jd_decisions.old_score` / `new_score` 两列）。
    ///
    /// 为什么两列要真写：前端此前只能从 `reason` 文本里用正则抠「a → b」，
    /// 那是把展示格式当数据源——改一个字（加个单位、换个箭头）就全抠不出来，
    /// 界面上「分数变化」整列变成 `—`。列存在（`schema::DECISION_UPGRADE_COLUMNS`）
    /// 就该写进去。
    ///
    /// 默认实现退回纯文本的 [`Self::log_decision`]：内存桩不必为此多加字段，
    /// 但对端**无法**读到两列。真实实现（`infra::sqlite`）必须覆盖。
    fn log_score_change(
        &self,
        platform: &str,
        company_job: &str,
        decision: &str,
        reason: &str,
        old_score: Option<u8>,
        new_score: Option<u8>,
    ) -> Result<(), Self::Error> {
        let _ = (old_score, new_score);
        self.log_decision(platform, company_job, decision, reason)
    }
}

/// 敏感库（`sensitive_jobs`）的读端口。
///
/// 存在的意义是**省掉详情页**：抓取时每条详情要 3~6 秒，而「这条岗位之前
/// 已经判过」这个结论在卡片阶段就能拿到（卡片上就有 `company` + `title`）。
/// 先查这里、命中就跳过详情页，是抓取链路里性价比最高的一步。
///
/// 语义对齐 `backend/app/store.py:432 sensitive_hit` 与 `:446 sensitive_company_hit`：
/// 键是「平台 + 公司 + 岗位名」，**不含 job_id**——Boss 的岗位重新发布后
/// `job_id` 会变，但公司与岗位名不变，按 job_id 查不出这种重复。
pub trait SensitiveStore {
    /// 命中的原因（写进日志给用户看）。`None` 表示没命中。
    type Error: std::fmt::Display;

    /// 精确查重：同平台 + 同公司 + 同岗位名。
    fn sensitive_hit(
        &self,
        platform: &str,
        company: Option<&str>,
        title: Option<&str>,
    ) -> Result<Option<SensitiveHit>, Self::Error>;

    /// 粗查重：同平台 + 同公司（JD 大多是同一套模板）。
    fn sensitive_company_hit(
        &self,
        platform: &str,
        company: Option<&str>,
    ) -> Result<Option<SensitiveHit>, Self::Error>;

    /// 记一条。同平台 + 公司 + 岗位名只留一条；`Stored` 优先级高于 `Rejected`
    /// （已入库的岗位不该被后续的排除覆盖掉）。
    fn add_sensitive(&self, entry: &SensitiveEntry<'_>) -> Result<bool, Self::Error>;
}

/// 敏感库命中的结果。
#[derive(Debug, Clone, Default)]
pub struct SensitiveHit {
    /// `stored` = 已入库 / `rejected` = 已被排除
    pub kind: String,
    pub title: Option<String>,
    pub reason: Option<String>,
}

impl SensitiveHit {
    /// 是否「已入库」——这种情况下再抓一遍纯属浪费详情页。
    pub fn is_stored(&self) -> bool {
        self.kind == SensitiveKind::Stored.as_str()
    }
}

/// 敏感库条目的来路。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveKind {
    /// 被规则/门槛判掉
    Rejected,
    /// 已经入过库
    Stored,
}

impl SensitiveKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SensitiveKind::Rejected => "rejected",
            SensitiveKind::Stored => "stored",
        }
    }
}

/// 待写入敏感库的一条记录。
///
/// 用借用切片而不是 `String`，是为了让抓取线程不必为每条岗位克隆一遍文本
/// （一次运行最多上百条，字段还都挺长）。
#[derive(Debug, Clone)]
pub struct SensitiveEntry<'a> {
    pub platform: &'a str,
    pub company: Option<&'a str>,
    pub title: Option<&'a str>,
    pub reason: &'a str,
    pub kind: SensitiveKind,
    pub job_id: Option<&'a str>,
    pub salary: Option<&'a str>,
    pub location: Option<&'a str>,
    pub job_url: Option<&'a str>,
}

/// 列表查询条件（`store.py:218 list_jobs` 的参数子集）。
#[derive(Debug, Clone, Default)]
pub struct JobQuery {
    pub status: Option<String>,
    pub platform: Option<String>,
    pub min_score: Option<u8>,
    /// 是否包含被排除的岗位
    pub include_rejected: bool,
    /// 只看被排除的岗位（列表页「不投递」页签）。
    ///
    /// **优先级高于 [`Self::include_rejected`]**：两者同时为真时按
    /// 「只要否决」拼条件，否则 `include_rejected=true` 会把正常的岗位也带进来，
    /// 页签计数与列表内容就会自相矛盾。默认 false，老调用方行为不变。
    pub rejected_only: bool,
    pub limit: u32,
    pub offset: u32,
    /// 排序字段：`score` | `created` | `updated`
    pub sort: SortField,
    pub ascending: bool,
}

impl JobQuery {
    /// 三个状态页签的查询条件（列表「未投递 / 已投递 / 不投递」）。
    ///
    /// 抽成构造器而**不是**在命令层三处手拼：页签条件一旦两处各写一份
    /// （列表一处、计数一处），「未投递 0」而列表里有 12 条的矛盾必然出现。
    ///
    /// - `pending`：未被否决 + status 为 pending（含历史 NULL status）
    /// - `delivered`：status = delivered，**不看**否决位（已投递是既成事实）
    /// - `rejected`：只看否决位
    pub fn for_tab(tab: &str) -> Self {
        match tab {
            "delivered" => JobQuery {
                status: Some(JobStatus::Delivered.as_str().to_string()),
                include_rejected: true,
                ..Default::default()
            },
            "rejected" => JobQuery {
                rejected_only: true,
                include_rejected: true,
                ..Default::default()
            },
            // 未知页签降级为 pending：与 JobStatus::parse 的取向一致，
            // 前端拼错参数时看到默认列表而不是空列表。
            _ => JobQuery {
                status: Some(JobStatus::Pending.as_str().to_string()),
                include_rejected: false,
                ..Default::default()
            },
        }
    }
}

/// 三个状态页签的条数（列表页签头「未投递 / 已投递 / 不投递」）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobTabCounts {
    pub pending: i64,
    pub delivered: i64,
    pub rejected: i64,
}

/// 排序字段白名单。
///
/// 用枚举而不是裸字符串，是因为排序字段会拼进 SQL：
/// 用户可传的字符串一旦直接拼接就是注入面。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortField {
    #[default]
    Score,
    Created,
    Updated,
    /// 按薪资下限排序（`backend/app/store.py` 的 `salary_k`）。
    Salary,
}

/// 薪资下限的 SQL 表达式（单位统一成 K）。
///
/// `salary` 列存的是 `"20-35K"` / `"35-65K·15薪"` / `"2.5-4万"` 这类文本。
/// 旧实现用 `substr(salary,1,3)` 取前 3 位，于是 `"35-65K"` 被当成 35、
/// `"1-2万"` 被当成 1 —— 排序结果错乱。这里按单位抽数值：
/// 「万」乘以 10 换算成 K。
///
/// **未知薪资（面议/空）返回 NULL**。注意 SQLite 的 NULL 排序规则与
/// PostgreSQL/MySQL 默认**相反**：`ORDER BY x ASC` 时 NULL 排**最前**，
/// DESC 时排最后（实测见 `.tmp-test/probe_salary.py`）。所以不能只靠
/// NULL 语义，排序时必须再加一列「是否无薪资」把 NULL 强制压到末尾，
/// 见 [`SALARY_UNKNOWN_RANK_EXPR`]。
pub const SALARY_K_EXPR: &str = "CASE \
     WHEN salary IS NULL OR TRIM(salary) = '' OR salary LIKE '%面议%' THEN NULL \
     WHEN salary LIKE '%万%' THEN \
       CAST(COALESCE(SUBSTR(salary, 1, INSTR(salary, '万') - 1), '0') AS REAL) * 10 \
     ELSE \
       CAST(COALESCE(SUBSTR(TRIM(salary), 1, INSTR(TRIM(salary) || '-', '-') - 1), '0') AS REAL) \
     END";

/// 「无薪资」标记位：面议/空值算 1，有薪资算 0。
///
/// 排序时它**永远 ASC 且永远是第一排序键**，于是无论用户选升序还是降序，
/// 「面议」「薪资未填」的岗位都被压到列表末尾——用户按薪资排序时想看的是
/// 真实数字，而不是一群没有数字的岗位占据榜首。
pub const SALARY_UNKNOWN_RANK_EXPR: &str = "CASE \
     WHEN salary IS NULL OR TRIM(salary) = '' OR salary LIKE '%面议%' THEN 1 \
     ELSE 0 END";

impl SortField {
    /// 映射到真实列名。**只允许出现编译期已知的常量**。
    pub fn column(self) -> &'static str {
        match self {
            SortField::Score => "match_score",
            SortField::Created => "created_at",
            SortField::Updated => "updated_at",
            SortField::Salary => SALARY_K_EXPR,
        }
    }

    /// 解析前端传来的排序键（旧版 `web/src/api.ts` 的 `sort` 取值）。
    ///
    /// 与 [`Self::parse`] 的差别只有一处：旧版把「按时间」叫 **`time`**，
    /// 新版内部叫 `created`。不认 `time` 的话用户选「时间」会被静默降级成
    /// 按分数排——界面上排序按钮亮了，列表顺序却没动。
    pub fn parse(s: &str) -> Self {
        match s {
            "time" | "created" | "created_at" => SortField::Created,
            "updated" | "updated_at" => SortField::Updated,
            "salary" | "salary_k" => SortField::Salary,
            _ => SortField::Score,
        }
    }

    /// 反向映射，回给前端做「当前排序列」高亮。
    pub fn as_str(self) -> &'static str {
        match self {
            SortField::Score => "score",
            SortField::Created => "time",
            SortField::Updated => "updated",
            SortField::Salary => "salary",
        }
    }

    /// 是否为「表达式排序」而非裸列排序。
    ///
    /// 表达式排序时 `list_jobs` 要额外按 `job_id` 兜底，否则同薪资的岗位
    /// 在不同页之间顺序不定，翻页会重复或漏掉记录。
    pub fn is_expression(self) -> bool {
        matches!(self, SortField::Salary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_status_roundtrip() {
        assert_eq!(JobStatus::parse("pending"), JobStatus::Pending);
        assert_eq!(JobStatus::parse("delivered"), JobStatus::Delivered);
        // 未知值降级为 pending 而不是报错，避免脏数据导致整个列表打不开
        assert_eq!(JobStatus::parse("garbage"), JobStatus::Pending);
        assert_eq!(JobStatus::Pending.as_str(), "pending");
    }

    #[test]
    fn score_source_roundtrip() {
        assert_eq!(ScoreSource::parse("rule"), Some(ScoreSource::Rule));
        assert_eq!(ScoreSource::parse("llm"), Some(ScoreSource::Llm));
        assert_eq!(ScoreSource::parse("garbage"), None);
    }

    #[test]
    fn sort_field_only_returns_whitelisted_columns() {
        // 注入防御：无论传入什么字符串，column() 只会返回常量
        for s in [
            "score",
            "created",
            "updated",
            "salary",
            "'; DROP TABLE job_queue; --",
            "",
        ] {
            let col = SortField::parse(s).column();
            let allowed = col == SALARY_K_EXPR
                || ["match_score", "created_at", "updated_at"].contains(&col);
            assert!(allowed, "非法列名泄露：{col}");
        }
    }

    #[test]
    fn salary_sort_parses_and_uses_expression() {
        assert_eq!(SortField::parse("salary"), SortField::Salary);
        assert_eq!(SortField::parse("salary_k"), SortField::Salary);
        assert!(SortField::Salary.is_expression());
        // 其它字段仍是裸列排序，不该被当成表达式
        assert!(!SortField::Score.is_expression());
        assert!(!SortField::Created.is_expression());
        assert!(!SortField::Updated.is_expression());
        // 表达式里必须带「万」换 K 的换算，否则 2.5-4万 会排到 35-65K 后面
        assert!(SALARY_K_EXPR.contains("'万'"));
        assert!(SALARY_K_EXPR.contains("* 10"));
        // 未知薪资必须落成 NULL：用 -1 会在升序时抢到最前面（真被测出来过）
        assert!(SALARY_K_EXPR.contains("面议"));
        assert!(SALARY_K_EXPR.contains("THEN NULL"));
        assert!(!SALARY_K_EXPR.contains("THEN -1"));
    }

    #[test]
    fn meets_threshold() {
        let mut job = Job::default();
        assert!(!job.meets(70), "无分数不应达标");
        job.match_score = Some(70);
        assert!(job.meets(70));
        job.match_score = Some(69);
        assert!(!job.meets(70));
    }

    #[test]
    fn is_accepted_reads_rejected_flag() {
        let mut job = Job::default();
        assert!(job.is_accepted(), "默认 rejected=0 应视为已入库");
        job.rejected = 1;
        assert!(!job.is_accepted());
    }

    #[test]
    fn sort_field_accepts_legacy_time_key() {
        // 旧版 web/ 传的是 `time`：不认它就会静默降级成按分数排
        assert_eq!(SortField::parse("time"), SortField::Created);
        assert_eq!(SortField::parse("created"), SortField::Created);
        assert_eq!(SortField::as_str(SortField::parse("time")), "time");
        assert_eq!(SortField::parse("score").as_str(), "score");
        assert_eq!(SortField::parse("salary").as_str(), "salary");
    }

    #[test]
    fn tab_query_never_mixes_rejected_with_normal_rows() {
        // pending：既不包含否决位，也只认 status=pending
        let pending = JobQuery::for_tab("pending");
        assert!(!pending.include_rejected);
        assert!(!pending.rejected_only);
        assert_eq!(pending.status.as_deref(), Some("pending"));

        // delivered：已投递是既成事实，不看否决位
        let delivered = JobQuery::for_tab("delivered");
        assert!(delivered.include_rejected);
        assert!(!delivered.rejected_only);
        assert_eq!(delivered.status.as_deref(), Some("delivered"));

        // rejected：只要否决位；include_rejected 必须同时为真，
        // 否则 push_job_filters 的 `= 0` 条件会把结果清空
        let rejected = JobQuery::for_tab("rejected");
        assert!(rejected.rejected_only);
        assert!(rejected.include_rejected);
        assert_eq!(rejected.status, None);

        // 未知页签降级为默认列表而不是空列表
        let unknown = JobQuery::for_tab("garbage");
        assert_eq!(unknown.status.as_deref(), Some("pending"));
        assert!(!unknown.include_rejected);
    }

    #[test]
    fn default_query_is_backward_compatible() {
        // 新增 rejected_only 不能改变既有调用方的语义
        let q = JobQuery::default();
        assert!(!q.rejected_only);
        assert!(!q.include_rejected);
        assert_eq!(JobTabCounts::default().pending, 0);
    }
}
