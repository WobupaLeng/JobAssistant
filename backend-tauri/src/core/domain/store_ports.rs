//! 存储端口的扩展 trait（`backend/app/store.py` 的补齐部分）。
//!
//! 与 `domain::job` 的分工：`JobStore` / `SensitiveStore` 是抓取与打分主链路
//! 用到的**最小**端口；本文件承载界面侧需要的管理类能力（批量标记、统计、
//! 分页浏览、配置读写）。
//!
//! **为什么拆成 trait 而不是往 `JobStore` 里继续塞方法**：`JobStore` 的每个
//! 方法都必须被所有实现类实现一遍（例如 `domain::rescore` 测试里的内存版
//! `MemStore`）。管理类能力只被 SQLite 实现与命令层需要，塞进 `JobStore`
//! 会逼着纯逻辑的内存桩跟着写一堆空实现。
//!
//! 铁律：本模块只放**签名与白名单枚举**，SQL 全部在 `crate::core::infra::sqlite`。

use serde::Serialize;

use super::job::{Job, JobQuery, SensitiveKind};

/// 岗位侧的批量操作与管理读端口。
pub trait JobAdminStore {
    type Error: std::fmt::Display;

    /// 标记不投递 / 恢复允许投递（`store.py:301 set_rejected`）。
    ///
    /// 返回实际改动的行数。**空数组返回 0 且不执行任何 SQL**：
    /// 直接拼 `IN ()` 是 SQL 语法错误，会让整条命令报错而不是「什么都没做」。
    fn set_rejected(&self, job_ids: &[String], rejected: bool) -> Result<usize, Self::Error>;

    /// 标记已投递 / 撤销已投递（`store.py:317 set_delivered`）。
    ///
    /// 语义照抄 Python：写的是 `status = 'delivered' | 'pending'`，
    /// **不是**单独的布尔列——「已投递」在旧库里一直是 status 的一个取值，
    /// 前端与列表筛选项都按 status 读。
    fn set_delivered(&self, job_ids: &[String], delivered: bool) -> Result<usize, Self::Error>;

    /// 尚未打分的岗位（`match_score IS NULL`，`store.py:330 unscored_jobs`）。
    ///
    /// 不过滤 `rejected`：被排除但没打分的行同样需要补分。
    fn unscored_jobs(&self) -> Result<Vec<Job>, Self::Error>;

    /// 总体统计（`store.py:338 stats`）。
    fn stats(&self) -> Result<JobStats, Self::Error>;

    /// 按 id 判断是否已入库（`store.py:194 exists`）。
    ///
    /// 与 `get_job().is_some()` 的差别只在代价：抓取时会逐条调它，
    /// 不该为了一个布尔值把 28 列全部反序列化。
    fn job_exists(&self, job_id: &str) -> Result<bool, Self::Error>;

    /// 按条件计数（`store.py:273 count_jobs`）。筛选语义与 [`JobQuery`] 一致，
    /// 但 `limit` / `offset` / `sort` 被忽略。
    fn count_jobs(&self, query: &JobQuery) -> Result<i64, Self::Error>;

    /// 已投递的 `job_id` 集合（供列表置灰与「不再参与投递」判定）。
    fn delivered_ids(&self) -> Result<Vec<String>, Self::Error>;

    /// 已排除的 `job_id` 集合。
    fn rejected_ids(&self) -> Result<Vec<String>, Self::Error>;

    /// 按公司名找出一个岗位 id（`job_queue.job_id`）。
    ///
    /// 用来给 Boss 会话列表补岗位 id：Boss 返回的会话行只有公司名，没有岗位 id，
    /// 而会话 uid 失效时只能靠岗位 id 重新点「立即沟通」（见 `commands_chat::fill_job_ids`）。
    /// 按公司名匹配是唯一的线索。
    fn find_job_id_by_company(&self, company: &str) -> Result<Option<String>, Self::Error>;
}

/// `store.py:338 stats()` 的返回结构。
///
/// 字段名与 Python 一致（`total` / `scored` / `avg_score` / `by_status` /
/// `by_platform`），前端沿用旧字段名即可。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct JobStats {
    pub total: i64,
    /// 已打分的条数（`match_score IS NOT NULL`）
    pub scored: i64,
    /// 平均分，保留 2 位小数；一条都没打分时为 `None`
    /// （Python 用 `round(avg, 2) if avg is not None else None`，**不要**变成 0）
    pub avg_score: Option<f64>,
    pub by_status: Vec<(String, i64)>,
    pub by_platform: Vec<(String, i64)>,
}

/// 排除原因白名单（`store.py:475 list_sensitive` 的 `kind` 参数）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SensitiveFilter {
    /// 不过滤
    #[default]
    All,
    /// 只看被排除的
    Rejected,
    /// 只看已入库的
    Stored,
}

impl SensitiveFilter {
    /// 映射到库里的 `kind` 值。`All` 返回 `None`，调用方据此不拼该条件。
    pub fn kind(self) -> Option<&'static str> {
        match self {
            SensitiveFilter::All => None,
            SensitiveFilter::Rejected => Some(SensitiveKind::Rejected.as_str()),
            SensitiveFilter::Stored => Some(SensitiveKind::Stored.as_str()),
        }
    }

    /// 解析前端传来的字符串。**未知值一律降级为 `All`**，
    /// 不降级为某个具体 kind——否则前端拼错参数会看到「列表突然空了」。
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "rejected" => SensitiveFilter::Rejected,
            "stored" => SensitiveFilter::Stored,
            _ => SensitiveFilter::All,
        }
    }
}

/// 敏感库一行。列与 `sensitive_jobs` 表一一对应。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SensitiveRecord {
    pub id: i64,
    pub platform: String,
    pub job_id: Option<String>,
    pub company: Option<String>,
    pub title: Option<String>,
    pub salary: Option<String>,
    pub location: Option<String>,
    pub reason: Option<String>,
    pub job_url: Option<String>,
    /// `rejected` / `stored`
    pub kind: String,
    pub created_at: i64,
}

impl SensitiveRecord {
    pub fn is_stored(&self) -> bool {
        self.kind == SensitiveKind::Stored.as_str()
    }
}

/// 敏感库查询条件（`store.py:458 list_sensitive`）。
#[derive(Debug, Clone, Default)]
pub struct SensitiveQuery {
    pub platform: Option<String>,
    /// 模糊匹配 公司 / 岗位名 / 原因 三列
    pub keyword: Option<String>,
    pub kind: SensitiveFilter,
    pub limit: u32,
    pub offset: u32,
}

/// 敏感库侧的批量操作与管理读端口。
pub trait SensitiveAdminStore {
    type Error: std::fmt::Display;

    /// 清空整张敏感库，返回删除条数（`store.py:368 clear_sensitive`）。
    fn clear_sensitive(&self) -> Result<usize, Self::Error>;

    /// 分页浏览，`id DESC`（`store.py:458 list_sensitive`）。
    fn list_sensitive(&self, query: &SensitiveQuery) -> Result<Vec<SensitiveRecord>, Self::Error>;

    /// 计数，可按平台过滤（`store.py:483 count_sensitive`）。
    fn count_sensitive(&self, platform: Option<&str>) -> Result<i64, Self::Error>;

    /// 按主键删（`store.py:492 delete_sensitive`）。空数组返回 0。
    fn delete_sensitive(&self, ids: &[i64]) -> Result<usize, Self::Error>;

    /// 把 `job_queue` 里已有的岗位补进敏感库（kind='stored'），
    /// 返回**新补入**的条数（`store.py:407 sync_stored`）。
    ///
    /// 已存在的行不计数，但会被强制升级为 `stored`——这就是「已入库优先于
    /// 被排除」在存量数据上的落地；否则升级一个老岗位后，下一轮抓取还会
    /// 把它当成被排除的又抓一遍。
    fn sync_stored(&self) -> Result<usize, Self::Error>;
}

/// 一条过滤决策（`jd_decisions` 行）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DecisionRecord {
    pub id: i64,
    pub platform: Option<String>,
    pub company_job: Option<String>,
    pub decision: Option<String>,
    pub reason: Option<String>,
    pub created_at: i64,
    /// 重算前的分数（旧版决策表没有这两列，历史行恒为 `None`）。
    pub old_score: Option<u8>,
    /// 重算后的分数（同上）。
    pub new_score: Option<u8>,
}

impl DecisionRecord {
    /// 「旧分 → 新分」的展示串。两列都没有时返回 `None`，界面据此不显示分值，
    /// 而不是显示 `0 → 0` 这种假数据。
    pub fn score_change(&self) -> Option<String> {
        match (self.old_score, self.new_score) {
            (Some(o), Some(n)) => Some(format!("{o} → {n}")),
            (None, Some(n)) => Some(format!("→ {n}")),
            (Some(o), None) => Some(format!("{o} →")),
            (None, None) => None,
        }
    }
}

/// 决策日志读端口。
pub trait DecisionStore {
    type Error: std::fmt::Display;

    /// `id > after_id`，按 `id DESC` 取至多 `limit` 条（`store.py:521 list_decisions`）。
    fn list_decisions(
        &self,
        limit: u32,
        after_id: i64,
    ) -> Result<Vec<DecisionRecord>, Self::Error>;

    /// `id < before_id`，按 `id DESC` 取至多 `limit` 条（向更旧翻页）。
    ///
    /// `after_id` 只能取「比游标更新」的记录，无法往下翻，决策日志的无限滚动
    /// 走这条；`before_id <= 0` 等价于「没有上界」，即从最新一条开始。
    fn list_decisions_before(
        &self,
        limit: u32,
        before_id: i64,
    ) -> Result<Vec<DecisionRecord>, Self::Error>;

    /// 决策日志总条数（与翻页无关的全表计数，供界面显示「共 N 条」）。
    fn count_decisions(&self) -> Result<i64, Self::Error>;

    /// 各决策行为（`decision` 原值）的条数。
    ///
    /// 「排除 / 入库 / 其它」怎么分档只在 `frontend-vue/src/lib_ui/logfmt.ts`
    /// 的 `decisionTone` 里定义（单一真源），后端不重复判定，只按原值分组。
    fn decision_counts(&self) -> Result<Vec<(String, i64)>, Self::Error>;
}

/// `app_config` 键值表端口。
///
/// 值统一以 `String` 进出：表里只有一个 TEXT 列，JSON 由调用方自己解析
/// （见 [`ConfigStore::get_config_json`]）。API Key 这类敏感值也走这里，
/// 本层不做任何日志输出。
/// `app_config` 里**只读**的派生键（词元计数、上次筛选）。
///
/// 配置页会把它们读出来展示，但保存时不得写回（见 `app_config::WRITABLE_KEYS`）。
pub const CONFIG_READ_ONLY_KEYS: &[&str] = &[
    "min_score",
    "data_dir",
    "tokens_total",
    "tokens_greet",
    "tokens_score",
    "platform_login",
];

/// 默认 [`ConfigStore::config_pairs`] 要扫的键：只读键 ∪ 白名单可写键。
fn config_scan_keys() -> impl Iterator<Item = &'static str> {
    CONFIG_READ_ONLY_KEYS
        .iter()
        .copied()
        .chain(crate::core::domain::app_config::WRITABLE_KEYS.iter().copied())
}

pub trait ConfigStore {
    type Error: std::fmt::Display;

    /// 读一个键。不存在返回 `default`（`store.py:531 get_config`）。
    fn get_config(&self, key: &str, default: &str) -> Result<String, Self::Error>;

    /// 写一个键（upsert）。`None` 写成空串，与 Python 一致（`store.py:537 set_config`）。
    fn set_config(&self, key: &str, value: Option<&str>) -> Result<(), Self::Error>;

    /// 读 JSON 值。**解析失败或值为空一律回退 `default`，不 panic**
    /// （`store.py:545 get_config_json`）——用户手工改坏过的配置项不该让应用起不来。
    fn get_config_json(
        &self,
        key: &str,
        default: serde_json::Value,
    ) -> Result<serde_json::Value, Self::Error>;

    /// 写 JSON 值，序列化带 `ensure_ascii=false` 语义（中文不转义）。
    fn set_config_json(&self, key: &str, value: &serde_json::Value) -> Result<(), Self::Error>;

    /// 累计词元计数：`tokens_greet += greet`、`tokens_score += score`、
    /// `tokens_total += greet + score`。
    ///
    /// **内部计数专用**，不经 `set_app_config`：三键仍属
    /// [`CONFIG_READ_ONLY_KEYS`]，前端整份回传不得写它们（只读守卫必须保持绿）。
    ///
    /// 实现必须是**单次锁获取 + 单事务 + SQL 侧算术**：读方（`read_config`）
    /// 永不见半更新态，并发累计不丢增量。禁止 Rust 侧 `get_config` → `set_config`
    /// 读改写。增量为 0 的键跳过语句，双 0 为 no-op（不写库、不建键、不补 0）；
    /// 遗留值只增不清，非数字遗留值按 0 起算（与读侧 `num()` 同口径）。
    ///
    /// ADR-005: docs/adr/ADR-005-legacy-token-accumulation.md
    fn bump_tokens(&self, greet: i64, score: i64) -> Result<(), Self::Error>;

    /// 一次读出全部键值对（`SELECT key, value FROM app_config`）。
    ///
    /// 命令层装配配置回执时要用到十几个键，逐个 `get_config` 会反复抢锁。
    ///
    /// 默认实现用 [`CONFIG_READ_ONLY_KEYS`] ∪ `app_config::WRITABLE_KEYS` 逐个读：
    /// **不能返回空表**——没覆盖本方法的实现方会让配置页显示成全部出厂默认值，
    /// 静默丢数据比报错更难查。
    fn config_pairs(&self) -> Result<Vec<(String, String)>, Self::Error> {
        let mut out = Vec::new();
        for key in config_scan_keys() {
            let value = self.get_config(key, "")?;
            if !value.is_empty() {
                out.push((key.to_string(), value));
            }
        }
        Ok(out)
    }
}

/// 把读到的原始字符串解析成 JSON；空串 / 非法 JSON 回退 `default`。
///
/// 抽成自由函数，让 SQLite 实现与内存桩共用同一套回退规则。
pub fn parse_config_json(raw: &str, default: serde_json::Value) -> serde_json::Value {
    if raw.trim().is_empty() {
        return default;
    }
    serde_json::from_str(raw).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_filter_unknown_falls_back_to_all() {
        assert_eq!(SensitiveFilter::parse("rejected"), SensitiveFilter::Rejected);
        assert_eq!(SensitiveFilter::parse(" stored "), SensitiveFilter::Stored);
        // 拼错 / 空串不能变成「只看某一类」，否则列表会莫名变空
        assert_eq!(SensitiveFilter::parse("garbage"), SensitiveFilter::All);
        assert_eq!(SensitiveFilter::parse(""), SensitiveFilter::All);
        assert_eq!(SensitiveFilter::All.kind(), None);
        assert_eq!(SensitiveFilter::Stored.kind(), Some("stored"));
    }

    #[test]
    fn parse_config_json_falls_back_instead_of_panicking() {
        let d = || serde_json::json!({"a": 1});
        assert_eq!(parse_config_json("", d()), d());
        assert_eq!(parse_config_json("   ", d()), d());
        // 用户手工改坏的配置项不能 panic
        assert_eq!(parse_config_json("{not json", d()), d());
        assert_eq!(
            parse_config_json(r#"{"a":2}"#, d()),
            serde_json::json!({"a": 2})
        );
        // JSON 标量同样合法
        assert_eq!(parse_config_json("70", d()), serde_json::json!(70));
        assert_eq!(parse_config_json("null", d()), serde_json::Value::Null);
    }

    #[test]
    fn stats_default_has_no_average() {
        // 一条都没打分时平均分必须是 None 而不是 0
        assert_eq!(JobStats::default().avg_score, None);
        assert_eq!(JobStats::default().total, 0);
    }

    #[test]
    fn sensitive_record_kind_flag() {
        let mut r = SensitiveRecord::default();
        assert!(!r.is_stored());
        r.kind = SensitiveKind::Stored.as_str().to_string();
        assert!(r.is_stored());
    }
}
