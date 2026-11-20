//! 岗位列表的状态：拉取、四页签、排序、多选与批量标记、选中。
//!
//! 状态归属判断（与旧版 `useJobList` 一致）：只有概览页用它，
//! 所以留在 `features/jobs` 而不是提到页面层。一旦有第二处需要，再上提。
//!
//! 页签语义（SPEC 7.4 / 5.3）：
//! - `all`       全部 —— 带已排除的
//! - `pending`   未投递 —— 待投递（`status` 既非 delivered 也非 rejected）
//! - `delivered` 已投递 —— `status == 'delivered'`
//! - `rejected`  不投递 —— `rejected != 0`
//!
//! 后三个互斥且并集 = 全部，所以按钮上的三个条数可以直接相加当「共 N 条」，
//! 不必为「全部」再打一次 list_jobs。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, types::*};

/// 列表页签。
///
/// **三个页签**（未投递 / 已投递 / 不投递）——「全部」不是一个页签，
/// 而是工具行上的一个独立开关（`job-tab-all`），打开时 `list_jobs` 不带 `tab`。
/// 这样默认视图永远落在「未投递」上，用户打开就看见该干的活。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListTab {
    Pending,
    Delivered,
    Rejected,
}

impl ListTab {
    pub fn slug(self) -> &'static str {
        match self {
            ListTab::Pending => "pending",
            ListTab::Delivered => "delivered",
            ListTab::Rejected => "rejected",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ListTab::Pending => "未投递",
            ListTab::Delivered => "已投递",
            ListTab::Rejected => "不投递",
        }
    }

    /// 渲染顺序即三个互斥页签的顺序，不含「全部」。
    pub fn all() -> [ListTab; 3] {
        [ListTab::Pending, ListTab::Delivered, ListTab::Rejected]
    }

    /// 是否要让后端带上已排除的岗位。只有「不投递」需要。
    fn include_rejected(self) -> bool {
        matches!(self, ListTab::Rejected)
    }

    /// 发给 `list_jobs` 的 `tab` 参数。`None` = 查看全部（不带 tab）。
    fn as_query(self, showing_all: bool) -> Option<String> {
        if showing_all {
            None
        } else {
            Some(self.slug().to_string())
        }
    }
}

/// 排序字段。字符串值与 `core/src/domain/job.rs` 的 `SortField::parse` 对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Created,
    Salary,
    Score,
}

impl SortField {
    pub fn as_str(self) -> &'static str {
        match self {
            SortField::Created => "created",
            SortField::Salary => "salary",
            SortField::Score => "score",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortField::Created => "加入时间",
            SortField::Salary => "薪资",
            SortField::Score => "分值",
        }
    }

    /// testid 片段（`job-sort-time` / `job-sort-salary` / `job-sort-score`）。
    /// 加入时间的 slug 是 `time` 而不是 `created`——SPEC 4.4 已定死。
    pub fn slug(self) -> &'static str {
        match self {
            SortField::Created => "time",
            SortField::Salary => "salary",
            SortField::Score => "score",
        }
    }

    /// 渲染顺序即旧版顺序：加入时间 / 薪资 / 分值。
    pub fn all() -> [SortField; 3] {
        [SortField::Created, SortField::Salary, SortField::Score]
    }

    /// 换字段时的默认方向：分值降序（高分在前），时间降序（新的在前），
    /// 薪资**升序**（旧版 `默认 asc`）——薪资按升序看才有「这份给得最低」的直觉。
    /// 切换到这个排序键时的默认方向。
    ///
    /// 三个键各有各的"读法"：时间新的在上（降序）、薪资高的在上（降序）、
    /// 分值高的在上（降序）。**没有一个是升序**——旧版也是这么做的：
    /// 用户点「薪资」想看的是 30K 以上，不是 3K。
    pub fn default_ascending(self) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
pub struct JobListState {
    pub jobs: RwSignal<Vec<Job>>,
    /// `db_overview().total`：库里全部岗位数（含被规则否决的，用于「数据总览」）
    pub total: RwSignal<usize>,
    pub has_legacy: RwSignal<bool>,
    pub loading: RwSignal<bool>,
    pub tab: RwSignal<ListTab>,
    /// 「查看全部」开关：为真时忽略 `tab`（`list_jobs` 不带 `tab`）。
    /// 点任一页签会自动关掉它。
    pub show_all: RwSignal<bool>,
    pub sort: RwSignal<SortField>,
    pub ascending: RwSignal<bool>,
    /// 三个页签的条数。`job_tab_counts` 每 5 秒轮询一次（旧版行为）。
    pub counts: RwSignal<JobTabCounts>,
    /// 当前页勾选的岗位 id（批量标记用）。
    pub checked: RwSignal<Vec<String>>,
    /// 抽屉当前展示的岗位。`None` = 抽屉关闭。
    pub selected: RwSignal<Option<Job>>,
}

impl Default for JobListState {
    fn default() -> Self {
        Self::new()
    }
}

impl JobListState {
    pub fn new() -> Self {
        Self {
            jobs: RwSignal::new(Vec::new()),
            total: RwSignal::new(0),
            has_legacy: RwSignal::new(false),
            loading: RwSignal::new(true),
            tab: RwSignal::new(ListTab::Pending),
            show_all: RwSignal::new(false),
            sort: RwSignal::new(SortField::Created),
            ascending: RwSignal::new(true),
            counts: RwSignal::new(JobTabCounts::default()),
            checked: RwSignal::new(Vec::new()),
            selected: RwSignal::new(None),
        }
    }

    /// 拉页签条数并写进 `counts`。`fail` 为空时静默。
    ///
    /// 与 `refresh` 分开：5 秒轮询若每次都重拉 500 行列表，
    /// 用户勾选的 checkbox 会被全量重渲染冲掉，滚动位置也会跳回顶部。
    pub fn refresh_counts(self, fail: Option<Callback<String>>) {
        spawn_local(async move {
            match api::job_tab_counts().await {
                Ok(c) => self.counts.set(c),
                Err(e) => {
                    if let Some(f) = fail {
                        f.run(format!("读取页签条数失败：{e}"));
                    }
                }
            }
        });
    }

    /// 同时拉概览、列表与页签条数。
    ///
    /// 三次 invoke 并发而不是串行：概览只读 `total`，列表读 500 行，
    /// 串行会让首屏多等两个往返（侧边栏的「岗位总数」明明可以先亮）。
    pub fn refresh(self, fail: Callback<String>) {
        self.loading.set(true);
        let tab = self.tab.get_untracked();
        let showing_all = self.show_all.get_untracked();
        let sort = self.sort.get_untracked();
        let ascending = self.ascending.get_untracked();
        let me = self;
        spawn_local(async move {
            let args = ListJobsArgs {
                include_rejected: showing_all || tab.include_rejected(),
                limit: Some(500),
                sort: Some(sort.as_str().to_string()),
                ascending,
                // 「查看全部」时 tab 留空，后端不按三态过滤
                tab: tab.as_query(showing_all),
                // `order` 与 `ascending` 是同一件事的两种写法（驼峰/蛇形的历史包袱），
                // 两个都给，避免后端任一路径读空。
                order: Some(if ascending { "asc".into() } else { "desc".into() }),
                ..Default::default()
            };
            let listed = api::list_jobs(args).await;
            let overview = api::db_overview().await;
            let counts = api::job_tab_counts().await;

            match overview {
                Ok(o) => {
                    me.total.set(o.total);
                    me.has_legacy.set(o.has_legacy);
                }
                // 概览失败不该让列表也白屏：列表数据是主内容
                Err(e) => fail.run(format!("读取概览失败：{e}")),
            }
            match counts {
                Ok(c) => me.counts.set(c),
                // 条数失败同样不该打断主内容，但用户会看到按钮上缺数字，值得提示
                Err(e) => fail.run(format!("读取页签条数失败：{e}")),
            }
            match listed {
                Ok(jobs) => me.jobs.set(jobs),
                Err(e) => fail.run(format!("读取岗位列表失败：{e}")),
            }
            me.loading.set(false);
        });
    }

    /// 切页签。清空勾选——上一页的勾选带到新页会误伤。
    pub fn on_tab(self, tab: ListTab, fail: Callback<String>) {
        if self.tab.get_untracked() == tab && !self.show_all.get_untracked() {
            return;
        }
        self.tab.set(tab);
        // 点任一页签自动关掉「查看全部」
        self.show_all.set(false);
        self.checked.set(Vec::new());
        self.refresh(fail);
    }

    /// 切换「查看全部」开关。开着时忽略页签；关掉时回到上次点的页签。
    pub fn on_show_all(self, on: bool, fail: Callback<String>) {
        if self.show_all.get_untracked() == on {
            return;
        }
        self.show_all.set(on);
        self.checked.set(Vec::new());
        self.refresh(fail);
    }

    /// 点排序按钮：同字段反向，换字段则用该字段的默认方向。
    ///
    /// 列表刷新由 `pages/dashboard.rs` 的信号 Effect 统一触发，
    /// 这里**不要**再调一次 `refresh`，否则一次点击打两个 list_jobs。
    pub fn on_sort(self, field: SortField) {
        if self.sort.get_untracked() == field {
            self.ascending.update(|a| *a = !*a);
        } else {
            self.sort.set(field);
            self.ascending.set(field.default_ascending());
        }
    }

    /// 当前明细按页签（或「查看全部」）再过滤一次。
    ///
    /// 后端 `list_jobs` 目前不认 `tab`（`tab` 字段虽在 `ListJobsArgs` 里，
    /// 但命令层没有转发给 `JobQuery`），所以界面必须自己兜一层——
    /// 否则点「已投递」看到的还是全部。等后端支持 `tab` 后这层是无害的幂等过滤。
    pub fn visible(&self, jobs: &[Job], tab: ListTab, show_all: bool) -> Vec<Job> {
        jobs.iter()
            .filter(|j| {
                if show_all {
                    return true;
                }
                match tab {
                    ListTab::Rejected => j.is_rejected(),
                    ListTab::Delivered => j.status == "delivered" && !j.is_rejected(),
                    ListTab::Pending => j.status != "delivered" && !j.is_rejected(),
                }
            })
            .cloned()
            .collect()
    }

    /// 「共 N 条」：查看全部 = 三个互斥计数之和，否则用对应计数。
    ///
    /// 不直接用 `total`：`db_overview().total` 是含历史否决的全库行数，
    /// 与页签口径（三态互斥）不是一回事，混用会出现「三个页签加起来不等于全部」。
    pub fn head_count(&self, tab: ListTab, show_all: bool) -> usize {
        let c = self.counts.get();
        if show_all {
            return (c.pending + c.delivered + c.rejected).max(0) as usize;
        }
        let v = match tab {
            ListTab::Pending => c.pending,
            ListTab::Delivered => c.delivered,
            ListTab::Rejected => c.rejected,
        };
        v.max(0) as usize
    }

    /// 勾选/取消单个岗位。
    pub fn toggle_check(self, id: &str) {
        self.checked.update(|v| {
            if let Some(pos) = v.iter().position(|x| x == id) {
                v.remove(pos);
            } else {
                v.push(id.to_string());
            }
        });
    }

    pub fn is_checked(self, id: &str) -> bool {
        self.checked.get().iter().any(|x| x == id)
    }

    /// 全选/全不选「本页」——传进来的就是当前筛选后的可见列表。
    pub fn set_check_all(self, ids: Vec<String>, on: bool) {
        if on {
            self.checked.set(ids);
        } else {
            self.checked.set(Vec::new());
        }
    }

    /// 批量标记。`op` 决定调哪条命令。
    ///
    /// 成功后清空勾选并刷新：被标记的岗位会从当前页签里消失（如「不投递」），
    /// 留着勾选只会让下一次批量误伤。
    pub fn batch(self, op: BatchOp, notify: Callback<String>, fail: Callback<String>) {
        let ids = self.checked.get_untracked();
        if ids.is_empty() {
            fail.run("请先勾选岗位".to_string());
            return;
        }
        let n = ids.len();
        spawn_local(async move {
            let res = match op {
                BatchOp::Reject => api::reject_jobs(ids).await,
                BatchOp::Allow => api::allow_jobs(ids).await,
            };
            match res {
                Ok(hit) => {
                    self.checked.set(Vec::new());
                    notify.run(format!("{}：影响 {hit} 条（勾选 {n} 条）", op.done_label()));
                    self.refresh(fail);
                }
                Err(e) => fail.run(format!("{}失败：{e}", op.label())),
            }
        });
    }

    /// 单条行内标记（卡片上的「不投递」/「允许投递」各一个）。
    ///
    /// 与批量走同一条命令，只是 id 只有一个；刷新后卡片状态立即变。
    pub fn mark_one(self, id: String, op: BatchOp, notify: Callback<String>, fail: Callback<String>) {
        spawn_local(async move {
            let res = match op {
                BatchOp::Reject => api::reject_jobs(vec![id]).await,
                BatchOp::Allow => api::allow_jobs(vec![id]).await,
            };
            match res {
                Ok(_) => {
                    notify.run(op.done_label().to_string());
                    self.refresh(fail);
                }
                Err(e) => fail.run(format!("{}失败：{e}", op.label())),
            }
        });
    }

    /// 标记「已投递」（卡片上的 ✓）。只有 `mark_delivered` 走这条，
    /// 因为它与 reject/allow 是三个不同命令。
    pub fn mark_delivered_one(
        self,
        id: String,
        notify: Callback<String>,
        fail: Callback<String>,
    ) {
        spawn_local(async move {
            match api::mark_delivered(vec![id]).await {
                Ok(_) => {
                    notify.run("已标记投递".to_string());
                    self.refresh(fail);
                }
                Err(e) => fail.run(format!("标记投递失败：{e}")),
            }
        });
    }

    /// 迁移十分制残留数据。成功后必须刷新，否则界面上的分数还是旧的。
    pub fn migrate(self, notify: Callback<String>, fail: Callback<String>) {
        spawn_local(async move {
            match api::migrate_scores().await {
                Ok(true) => {
                    notify.run("已迁移到百分制".to_string());
                    self.refresh(fail);
                }
                Ok(false) => notify.run("没有需要迁移的数据".to_string()),
                Err(e) => fail.run(format!("迁移失败：{e}")),
            }
        });
    }
}

/// 批量/单条标记的两种动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchOp {
    Reject,
    Allow,
}

impl BatchOp {
    /// 动词（用于失败提示）。
    pub fn label(self) -> &'static str {
        match self {
            BatchOp::Reject => "标记不投递",
            BatchOp::Allow => "允许投递",
        }
    }

    /// 完成语（用于成功提示）。
    pub fn done_label(self) -> &'static str {
        match self {
            BatchOp::Reject => "已标记不投递",
            BatchOp::Allow => "已允许投递",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, status: &str, rejected: i64) -> Job {
        Job {
            job_id: id.to_string(),
            status: status.to_string(),
            rejected,
            ..Default::default()
        }
    }

    #[test]
    fn tabs_are_the_three_exclusive_ones() {
        let slugs: Vec<&str> = ListTab::all().iter().map(|t| t.slug()).collect();
        assert_eq!(slugs, ["pending", "delivered", "rejected"]);
        let labels: Vec<&str> = ListTab::all().iter().map(|t| t.label()).collect();
        assert_eq!(labels, ["未投递", "已投递", "不投递"]);
        // 「全部」不是页签：它由 show_all 开关表达，list_jobs 不带 tab
        assert_eq!(ListTab::Pending.as_query(false).as_deref(), Some("pending"));
        assert_eq!(ListTab::Pending.as_query(true), None);
    }

    #[test]
    fn tab_filter_is_mutually_exclusive_and_exhaustive() {
        let me = JobListState::new();
        let all = vec![
            job("p1", "pending", 0),
            job("d1", "delivered", 0),
            job("r1", "pending", 1),
        ];
        let n = |t: ListTab| me.visible(&all, t, false).len();
        assert_eq!(me.visible(&all, ListTab::Pending, true).len(), 3);
        assert_eq!(n(ListTab::Pending), 1);
        assert_eq!(n(ListTab::Delivered), 1);
        assert_eq!(n(ListTab::Rejected), 1);
        // 三个子页签互斥且并集 = 全部
        assert_eq!(
            n(ListTab::Pending) + n(ListTab::Delivered) + n(ListTab::Rejected),
            me.visible(&all, ListTab::Pending, true).len()
        );
    }

    #[test]
    fn rejected_wins_over_delivered() {
        let me = JobListState::new();
        // 既是已投递又被标记不投递：按「不投递」归类（页签语义以 rejected 为准）
        let rows = vec![job("x", "delivered", 1)];
        assert_eq!(me.visible(&rows, ListTab::Delivered, false).len(), 0);
        assert_eq!(me.visible(&rows, ListTab::Rejected, false).len(), 1);
    }

    #[test]
    fn sort_defaults_follow_reading_order() {
        assert_eq!(SortField::all().map(|s| s.slug()), ["time", "salary", "score"]);
        // 三个键默认都降序：点「薪资」想看 30K 以上、点「分值」想看高分、
        // 点「加入时间」想看刚抓的。升序一律要用户再点一次才出现。
        assert!(!SortField::Created.default_ascending());
        assert!(!SortField::Salary.default_ascending());
        assert!(!SortField::Score.default_ascending());
    }

    #[test]
    fn head_count_uses_tab_counts_not_db_total() {
        let me = JobListState::new();
        me.counts.set(JobTabCounts {
            pending: 12,
            delivered: 3,
            rejected: 5,
        });
        assert_eq!(me.head_count(ListTab::Pending, true), 20);
        assert_eq!(me.head_count(ListTab::Pending, false), 12);
        assert_eq!(me.head_count(ListTab::Delivered, false), 3);
        assert_eq!(me.head_count(ListTab::Rejected, false), 5);
    }

    #[test]
    fn check_toggle_and_select_all() {
        let me = JobListState::new();
        me.toggle_check("a");
        me.toggle_check("b");
        assert!(me.is_checked("a"));
        me.toggle_check("a");
        assert!(!me.is_checked("a"));
        me.set_check_all(vec!["x".into(), "y".into()], true);
        assert_eq!(me.checked.get_untracked(), vec!["x", "y"]);
        me.set_check_all(vec!["x".into(), "y".into()], false);
        assert!(me.checked.get_untracked().is_empty());
    }
}
