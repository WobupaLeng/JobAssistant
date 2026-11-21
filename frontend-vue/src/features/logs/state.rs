//! 日志页的状态与**纯逻辑**（排序 / 增量合并 / 筛选 / 文案）。
//!
//! ## 双视图的来源契约（2026-11-21 实测后端源码）
//!
//! | 视图 | 命令 | Rust 侧真源 | 时间戳单位 |
//! |---|---|---|---|
//! | 运行日志 | `list_logs(since)` / `clear_logs()` | `commands_logs.rs` 的 2000 条 `LogRing` | **毫秒** |
//! | 决策日志 | `list_decisions({limit, afterId})` | `core::domain::store_ports::DecisionRecord` | **秒** |
//!
//! 两者时间单位不同（`LogRow.ts` 是毫秒，`DecisionRecord.created_at` 是秒），
//! 所以格式化分成 `lib_ui::logfmt::fmt_log_time`（带秒）与 `fmt_decision_time`
//! 两套，不要混用。
//!
//! ## 为什么是「合并」而不是整表替换
//!
//! `list_logs` 是**增量**语义：带 `since` 只回新增条目，且 `LogRing::since`
//! 是**严格大于**——同一毫秒内后写的那条会被下一次拉取重复送回。自动刷新
//! 每 2 秒跑一次，若每拍都整表覆盖，界面会「闪空 → 重填」；若直接追加，
//! 又会累积重复行。合并规则见 [`merge_logs`]：尾部追加 + 按
//! `(ts, 级别, 消息)` **多重集**去重（旧表里有 k 条，新批里最多再收一次）。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::api::types::{DecisionRecord, LogRow as RawLogRow};
use crate::lib_ui::logfmt::{self, LogLevel};

/// 页面上的两套日志。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogView {
    Runtime,
    Decision,
}

impl LogView {
    /// `data-testid="logs-tab-runtime"` / `logs-tab-decision`：
    /// 这两个串是用户裁决第 ⑦ 条的验收锚点，改文案可以，改 slug 不行。
    pub fn slug(self) -> &'static str {
        match self {
            LogView::Runtime => "runtime",
            LogView::Decision => "decision",
        }
    }

    /// 中文页签名（顶部分段控件上显示的字）。
    pub fn label(self) -> &'static str {
        match self {
            LogView::Runtime => "运行日志",
            LogView::Decision => "决策日志",
        }
    }

    pub fn all() -> [LogView; 2] {
        [LogView::Runtime, LogView::Decision]
    }
}

/// 运行日志一行（级别已归一化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRow {
    /// 毫秒
    pub ts: u64,
    pub level: LogLevel,
    /// 级别原始串（title 提示用；中文名走 `level.cn()`）
    pub level_raw: String,
    pub message: String,
    /// 列表标识：`{ts}-{seq}`。同一毫秒的多行靠 `seq` 区分，
    /// 否则 Leptos 的 `For` 会把它们当成同一节点。
    pub key: String,
}

impl RunRow {
    fn new(ts: u64, level_raw: &str, message: &str, seq: u32) -> Self {
        Self {
            ts,
            level: LogLevel::parse(level_raw),
            level_raw: level_raw.trim().to_string(),
            message: message.to_string(),
            key: format!("{ts}-{seq}"),
        }
    }
}

/// `list_logs` 的一条回执转成行。`seq_base` 是该批次的起始序号。
pub fn rows_from_reply(items: &[RawLogRow], seq_base: u32) -> Vec<RunRow> {
    items
        .iter()
        .enumerate()
        .map(|(i, r)| RunRow::new(r.ts, &r.level, &r.message, seq_base + i as u32))
        .collect()
}

/// 把新一批并进旧表，结果按时间**倒序**（最新在上），同毫秒保持原顺序。
///
/// - `replace = true`（首次全量 / 手动刷新）→ 直接用新批
/// - `replace = false`（自动刷新增量）→ 尾部追加，丢掉「旧表里已经有过次数」的重复项
pub fn merge_logs(old: Vec<RunRow>, incoming: Vec<RunRow>, replace: bool) -> Vec<RunRow> {
    let mut merged = if replace {
        incoming
    } else {
        let mut acc = old;
        let mut seen: std::collections::HashMap<(u64, &'static str, String), usize> =
            std::collections::HashMap::new();
        for r in acc.iter() {
            *seen
                .entry((r.ts, r.level.as_str(), r.message.clone()))
                .or_insert(0) += 1;
        }
        for r in incoming {
            let k = (r.ts, r.level.as_str(), r.message.clone());
            let n = seen.entry(k).or_insert(0);
            if *n == 0 {
                acc.push(r);
            } else {
                // 旧表里已有同一条：新批送来的这份视为重复，计数回落
                *n -= 1;
            }
        }
        acc
    };
    merged.sort_by(|a, b| b.ts.cmp(&a.ts));
    merged
}

/// 运行日志过滤（级别多选 + 关键字），保持传入顺序。纯函数，可单测。
pub fn filter_logs(rows: &[RunRow], selected: &[LogLevel], keyword: &str) -> Vec<RunRow> {
    rows.iter()
        .filter(|r| logfmt::log_visible(r.level, &r.message, keyword, selected))
        .cloned()
        .collect()
}

/// 决策日志一行（公司与岗位已从 `company_job` 拆开）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRow {
    pub id: i64,
    pub company: String,
    pub title: String,
    pub decision: String,
    pub reason: String,
    pub created_at: i64,
    pub old_score: Option<u8>,
    pub new_score: Option<u8>,
}

impl DecisionRow {
    pub fn from_record(r: &DecisionRecord) -> Self {
        let (company, title) = logfmt::split_company_job(&r.company_job);
        Self {
            id: r.id,
            company,
            title,
            decision: r.decision.clone(),
            reason: r.reason.clone(),
            created_at: r.created_at,
            old_score: r.old_score,
            new_score: r.new_score,
        }
    }

    pub fn label(&self) -> String {
        logfmt::decision_label(&self.decision)
    }

    pub fn tone(&self) -> &'static str {
        logfmt::decision_tone(&self.decision)
    }

    pub fn reason_text(&self) -> String {
        logfmt::reason_text(&self.reason)
    }

    /// `—`（无分数）/ `72 → 65` / `→ 65`。列全空时回落到 `reason` 文本。
    pub fn score(&self) -> logfmt::ScoreDelta {
        let (o, n) = logfmt::decision_scores(self.old_score, self.new_score, &self.reason);
        logfmt::score_delta(o, n)
    }
}

/// 决策日志的行为筛选。四个固定项 + 「其它」（兜住将来的新 kind）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionFilter {
    All,
    Rescore,
    Reject,
    Accept,
    Other,
}

impl DecisionFilter {
    pub fn slug(self) -> &'static str {
        match self {
            DecisionFilter::All => "all",
            DecisionFilter::Rescore => "rescore",
            DecisionFilter::Reject => "reject",
            DecisionFilter::Accept => "accept",
            DecisionFilter::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DecisionFilter::All => "全部",
            DecisionFilter::Rescore => "重算",
            DecisionFilter::Reject => "排除",
            DecisionFilter::Accept => "入库",
            DecisionFilter::Other => "其它",
        }
    }

    pub fn all() -> [DecisionFilter; 5] {
        [
            DecisionFilter::All,
            DecisionFilter::Rescore,
            DecisionFilter::Reject,
            DecisionFilter::Accept,
            DecisionFilter::Other,
        ]
    }

    pub fn accept(self, kind: &str) -> bool {
        let k = kind.trim();
        match self {
            DecisionFilter::All => true,
            DecisionFilter::Rescore => k == "rescore",
            DecisionFilter::Reject => k == "reject",
            DecisionFilter::Accept => matches!(k, "accept" | "store" | "stored"),
            DecisionFilter::Other => {
                !matches!(k, "rescore" | "reject" | "accept" | "store" | "stored")
            }
        }
    }
}

/// 决策日志过滤：kind + 关键字（公司 / 岗位 / 原因）。
pub fn filter_decisions(
    rows: &[DecisionRow],
    filter: DecisionFilter,
    keyword: &str,
) -> Vec<DecisionRow> {
    rows.iter()
        .filter(|r| filter.accept(&r.decision))
        .filter(|r| {
            logfmt::matches_keyword(&r.company, keyword)
                || logfmt::matches_keyword(&r.title, keyword)
                || logfmt::matches_keyword(&r.reason, keyword)
        })
        .cloned()
        .collect()
}

/// 自动刷新间隔（用户裁决：2 秒）。
pub const AUTO_REFRESH_MS: i32 = 2_000;

/// 决策日志一次拉多少条。后端 `limit == 0` 兜到 200（`sqlite/admin.rs:430`），
/// 这里显式给 300，让「全部」页签一屏有足够历史。
pub const DECISION_LIMIT: u32 = 300;

/// 页面状态。全部是信号：视图层只读、按钮写。
#[derive(Clone, Copy)]
pub struct LogState {
    pub view: RwSignal<LogView>,
    /// 运行日志（已倒序）
    pub run_rows: RwSignal<Vec<RunRow>>,
    /// 决策日志（后端顺序：id 倒序 = 新→旧）
    pub decisions: RwSignal<Vec<DecisionRow>>,
    pub run_loading: RwSignal<bool>,
    pub dec_loading: RwSignal<bool>,
    /// 运行日志：选中的级别（空 = 不过滤）
    pub levels: RwSignal<Vec<LogLevel>>,
    pub run_keyword: RwSignal<String>,
    pub run_auto: RwSignal<bool>,
    /// 决策日志：行为筛选与关键字
    pub dec_filter: RwSignal<DecisionFilter>,
    pub dec_keyword: RwSignal<String>,
    /// 清空按钮的二次确认态
    pub confirming_clear: RwSignal<bool>,
    /// 上次 `list_logs` 的游标（毫秒，取自回执 `now`）
    pub since: RwSignal<Option<i64>>,
    /// 刚清空过：空态文案要说「已清空」而不是「还没有」
    pub just_cleared: RwSignal<bool>,
    /// 行内错误（不抢全局 error bar：日志读失败不该盖住整屏）
    pub run_error: RwSignal<String>,
    pub dec_error: RwSignal<String>,
}

impl LogState {
    pub fn new() -> Self {
        Self {
            view: RwSignal::new(LogView::Runtime),
            run_rows: RwSignal::new(Vec::new()),
            decisions: RwSignal::new(Vec::new()),
            run_loading: RwSignal::new(true),
            dec_loading: RwSignal::new(true),
            levels: RwSignal::new(Vec::new()),
            run_keyword: RwSignal::new(String::new()),
            run_auto: RwSignal::new(false),
            dec_filter: RwSignal::new(DecisionFilter::All),
            dec_keyword: RwSignal::new(String::new()),
            confirming_clear: RwSignal::new(false),
            since: RwSignal::new(None),
            just_cleared: RwSignal::new(false),
            run_error: RwSignal::new(String::new()),
            dec_error: RwSignal::new(String::new()),
        }
    }

    /// 拉运行日志。`full = true` 清游标做全量；`false` 走增量（自动刷新）。
    ///
    /// 失败时**保留旧数据**，只写行内错误：日志读失败通常是瞬时 IPC 问题，
    /// 把已有内容清掉只会让用户更慌。
    pub fn load_runs(self, full: bool) {
        if full {
            self.run_loading.set(true);
            self.since.set(None);
            self.just_cleared.set(false);
        }
        spawn_local(async move {
            let since = if full { None } else { self.since.get_untracked() };
            match api::list_logs(since).await {
                Ok(reply) => {
                    let base = self.run_rows.get_untracked().len() as u32;
                    let fresh = rows_from_reply(&reply.items, base);
                    let merged = merge_logs(self.run_rows.get_untracked(), fresh, full);
                    self.run_rows.set(merged);
                    // 游标取回执的 `now`：它是**后端时钟**，与条目 ts 同源
                    // 回执 `now` 是 u64（Unix 毫秒），游标信号用 i64 与命令签名一致
                    self.since.set(Some(reply.now as i64));
                    self.run_error.set(String::new());
                }
                Err(e) => self.run_error.set(format!("读取运行日志失败：{e}")),
            }
            self.run_loading.set(false);
        });
    }

    /// 拉决策日志（新→旧）。
    pub fn load_decisions(self) {
        self.dec_loading.set(true);
        spawn_local(async move {
            match api::list_decisions(Some(DECISION_LIMIT), None).await {
                Ok(list) => {
                    self.decisions
                        .set(list.iter().map(DecisionRow::from_record).collect());
                    self.dec_error.set(String::new());
                }
                Err(e) => self.dec_error.set(format!("读取决策日志失败：{e}")),
            }
            self.dec_loading.set(false);
        });
    }

    /// 刷新当前视图（顶部刷新按钮）。
    pub fn refresh(self) {
        match self.view.get_untracked() {
            LogView::Runtime => self.load_runs(true),
            LogView::Decision => self.load_decisions(),
        }
    }

    /// 两条数据一起重拉（进入页面 / 切回本页）。
    pub fn reload_all(self) {
        self.load_runs(true);
        self.load_decisions();
    }

    /// 清空运行日志。**只有二次确认之后才调用**（确认在视图层）。
    ///
    /// 成功后本地也清空：后端环形缓冲清了，但 DOM 里还挂着旧行会让人以为没生效。
    pub fn clear_runs(self) {
        spawn_local(async move {
            match api::clear_logs().await {
                Ok(_) => {
                    self.run_rows.set(Vec::new());
                    self.since.set(None);
                    self.just_cleared.set(true);
                    self.run_error.set(String::new());
                }
                Err(e) => self.run_error.set(format!("清空运行日志失败：{e}")),
            }
        });
    }

    /// 运行日志：过滤后的可见行。
    pub fn visible_runs(&self) -> Vec<RunRow> {
        filter_logs(
            &self.run_rows.get(),
            &self.levels.get(),
            &self.run_keyword.get(),
        )
    }

    /// 决策日志：过滤后的可见行。
    pub fn visible_decisions(&self) -> Vec<DecisionRow> {
        filter_decisions(
            &self.decisions.get(),
            self.dec_filter.get(),
            &self.dec_keyword.get(),
        )
    }
}

impl Default for LogState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(ts: u64, level: &str, message: &str) -> RawLogRow {
        // `seq` 是后端分配的单调序号，只用于列表 key；测试构造时给 0 即可
        // （`LogRow` 的 `seq` 带 `#[serde(default)]`，旧回执也能反序列化）
        RawLogRow {
            ts,
            seq: 0,
            level: level.to_string(),
            message: message.to_string(),
        }
    }

    fn dec(id: i64, kind: &str, company_job: &str, reason: &str) -> DecisionRecord {
        DecisionRecord {
            id,
            platform: "boss".into(),
            company_job: company_job.into(),
            decision: kind.into(),
            reason: reason.into(),
            created_at: 1_700_000_000,
            old_score: None,
            new_score: None,
        }
    }

    #[test]
    fn merge_sorts_desc_and_dedupes_same_millisecond_retries() {
        let first = rows_from_reply(
            &[
                raw(1_000, "info", "a"),
                raw(1_500, "warn", "b"),
                raw(1_500, "info", "c"),
            ],
            0,
        );
        let merged = merge_logs(Vec::new(), first, true);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].message, "b", "最新在上");
        let keys: std::collections::HashSet<_> = merged.iter().map(|r| r.key.clone()).collect();
        assert_eq!(keys.len(), 3, "同毫秒的行也必须有唯一 key");

        // 增量：`since` 严格大于 → 同毫秒的旧条目会被再送一次
        let retry = rows_from_reply(&[raw(1_500, "warn", "b"), raw(2_000, "info", "d")], 3);
        let merged = merge_logs(merged, retry, false);
        assert_eq!(merged.len(), 4, "重复的 b 不再追加，新的 d 追加");
        assert_eq!(merged[0].message, "d");
        assert_eq!(merged.iter().filter(|r| r.message == "b").count(), 1);
    }

    #[test]
    fn same_message_twice_is_not_deduped() {
        // 同一毫秒、同一条消息写两次是合法的（循环里连着两行日志）。
        // 但**跨增量批次**无法区分「新的一行」与「游标重送的同一行」——
        // `LogRing::since` 是严格大于，重算时同毫秒的后一条会被再送一次。
        // 因此合并规则选择「宁可少一条也不重复」：旧表 1 条 + 新批 1 条同键 = 仍 1 条。
        let first = rows_from_reply(&[raw(5_000, "info", "tick")], 0);
        let merged = merge_logs(Vec::new(), first, true);
        assert_eq!(merged.len(), 1);
        let dup = rows_from_reply(&[raw(5_000, "info", "tick")], 1);
        let merged = merge_logs(merged, dup, false);
        assert_eq!(merged.len(), 1, "同毫秒同消息的二次送回收敛成一条");

        // 一次全量拉取里，同毫秒的两条不同数字仍是两条（用不同的 seq）
        let both = rows_from_reply(&[raw(5_000, "info", "tick"), raw(5_000, "info", "tick")], 0);
        let merged = merge_logs(Vec::new(), both, true);
        assert_eq!(merged.len(), 2);
        let keys: std::collections::HashSet<_> = merged.iter().map(|r| r.key.clone()).collect();
        assert_eq!(keys.len(), 2, "同毫秒两条要能一起显示，key 不能撞");
    }

    #[test]
    fn filter_logs_by_level_and_keyword() {
        let rows = rows_from_reply(
            &[
                raw(300, "error", "启动失败"),
                raw(200, "warn", "低于门槛"),
                raw(100, "info", "已启动抓取进程"),
            ],
            0,
        );
        assert_eq!(filter_logs(&rows, &[], "").len(), 3, "空筛选 = 全量");
        assert_eq!(filter_logs(&rows, &[LogLevel::Error], "").len(), 1);
        assert_eq!(
            filter_logs(&rows, &[LogLevel::Warn, LogLevel::Error], "").len(),
            2
        );
        assert_eq!(filter_logs(&rows, &[], "抓取").len(), 1);
        // 搜级别名也能命中（搜 error 出错误行）
        assert_eq!(filter_logs(&rows, &[], "error").len(), 1);
        assert_eq!(filter_logs(&rows, &[LogLevel::Warn], "error").len(), 0);
    }

    #[test]
    fn decision_rows_split_and_render_scores() {
        let r = DecisionRow::from_record(&dec(
            9,
            "rescore",
            "腾讯/高级 Java",
            "8 → 85（rule）薪资偏低",
        ));
        assert_eq!(r.company, "腾讯");
        assert_eq!(r.title, "高级 Java");
        // rescore 目前不写 old/new 列：从 reason 回落
        assert_eq!(r.score().text, "8 → 85");
        assert_eq!(r.score().dir, "up");
        assert_eq!(r.label(), "重算");
        assert_eq!(r.tone(), "accent");

        // 列里有值时以列为准
        let mut rec = dec(10, "reject", "A公司/后端", "外包");
        rec.old_score = Some(88);
        rec.new_score = Some(40);
        let r = DecisionRow::from_record(&rec);
        assert_eq!(r.score().text, "88 → 40");
        assert_eq!(r.score().dir, "down");
        assert_eq!(r.label(), "排除");
        assert_eq!(r.tone(), "bad");

        // 空 reason + 空分数：破折号 + 兜底文案，不出现 null / undefined
        let r = DecisionRow::from_record(&dec(11, "", "", ""));
        assert_eq!(r.score().text, "—");
        assert_eq!(r.reason_text(), "（未记录原因）");
        assert_eq!(r.label(), "未标注");
    }

    #[test]
    fn decision_filter_by_kind_and_keyword() {
        let rows: Vec<DecisionRow> = [
            dec(4, "rescore", "腾讯/Java", "8 → 85"),
            dec(3, "reject", "外包公司/前端", "外包"),
            dec(2, "accept", "字节/后端", "匹配"),
            dec(1, "sync", "阿里/测试", ""),
        ]
        .iter()
        .map(DecisionRow::from_record)
        .collect();

        assert_eq!(filter_decisions(&rows, DecisionFilter::All, "").len(), 4);
        assert_eq!(filter_decisions(&rows, DecisionFilter::Reject, "").len(), 1);
        assert_eq!(filter_decisions(&rows, DecisionFilter::Accept, "").len(), 1);
        assert_eq!(filter_decisions(&rows, DecisionFilter::Rescore, "").len(), 1);
        assert_eq!(filter_decisions(&rows, DecisionFilter::Other, "").len(), 1);

        assert_eq!(filter_decisions(&rows, DecisionFilter::All, "外包").len(), 1);
        assert_eq!(filter_decisions(&rows, DecisionFilter::All, "腾讯").len(), 1);
        assert_eq!(filter_decisions(&rows, DecisionFilter::Reject, "腾讯").len(), 0);
    }

    #[test]
    fn view_tabs_are_the_two_contracted_slugs() {
        let slugs: Vec<&str> = LogView::all().iter().map(|v| v.slug()).collect();
        assert_eq!(slugs, ["runtime", "decision"]);
        let labels: Vec<&str> = LogView::all().iter().map(|v| v.label()).collect();
        assert_eq!(labels, ["运行日志", "决策日志"]);
    }
}
