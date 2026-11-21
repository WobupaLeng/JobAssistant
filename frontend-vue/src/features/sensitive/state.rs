//! 敏感库抽屉的纯状态与文案拼接（无 IO、无 DOM，可在宿主目标上单测）。
//!
//! 拆出来的理由：抽屉里最容易出错的是**选中集合与筛选结果不同步**——
//! 换了筛选条件后，上一页选中的 id 不该还留在「移出所选 (N)」的计数里，
//! 否则用户点一下会删掉屏幕上根本看不见的行。

use std::collections::BTreeSet;

use crate::api::types::SensitiveRecord;

/// 类别筛选。与后端 `SensitiveFilter::parse` 的三种取值一一对应。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Stored,
    Rejected,
}

impl Filter {
    /// 下发给 `sensitive_list` 的 `kind`。
    ///
    /// 后端 `SensitiveFilter` 把未知值当「全部」，所以「全部」下发 `"all"`
    /// 与下发空串等价；这里写 `"all"` 是为了让前端日志里的取值自解释。
    pub fn kind(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Stored => "stored",
            Filter::Rejected => "rejected",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Stored => "stored",
            Filter::Rejected => "rejected",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "全部",
            Filter::Stored => "已入库",
            Filter::Rejected => "已排除",
        }
    }
}

/// 本抽屉要显示的一切（除请求中的 flag 外）。
#[derive(Debug, Clone, Default)]
pub struct SensitiveState {
    pub items: Vec<SensitiveRecord>,
    /// 按**当前筛选条件**算出的总数，不是全库总数（后端 `SensitivePage.total`）。
    pub total: i64,
    pub keyword: String,
    /// 已提交给后端的搜索词。与 `keyword` 分开：输入框边打字边触发 IPC
    /// 会把后端 SQL 打满，旧版也是「回车 / 点搜索」才生效。
    pub applied: String,
    pub filter: Filter,
    pub selected: BTreeSet<i64>,
    pub loading: bool,
    pub error: String,
}

impl SensitiveState {
    /// 当前页是否全部选中（空页不算全选）。
    pub fn all_selected(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(|r| self.selected.contains(&r.id))
    }

    /// 选中的条目数。只算**当前页可见**的 id——`selected` 在每次刷新后
    /// 都会 retain 到当前页，所以这里再过滤一次是防御性的双保险。
    pub fn selected_visible(&self) -> Vec<i64> {
        self.items
            .iter()
            .filter(|r| self.selected.contains(&r.id))
            .map(|r| r.id)
            .collect()
    }

    /// 刷新结果回来后同步选中集合：丢掉不在本页的 id，保留本页仍被勾选的。
    pub fn retain_selection(&mut self) {
        let visible: BTreeSet<i64> = self.items.iter().map(|r| r.id).collect();
        self.selected.retain(|id| visible.contains(id));
    }

    /// 全选 / 取消全选（作用于当前页）。
    pub fn toggle_select_all(&mut self) {
        if self.all_selected() {
            self.selected.clear();
        } else {
            self.selected = self.items.iter().map(|r| r.id).collect();
        }
    }

    /// 请求里要带的 `(platform, keyword, kind)`。`platform` 恒为 `None`：
    /// 本项目目前只有一个平台（boss），按平台过滤会让抽屉看起来永远空。
    pub fn query(&self) -> (Option<&str>, Option<&str>, Option<&str>) {
        let kw = self.applied.trim();
        (
            None,
            if kw.is_empty() { None } else { Some(kw) },
            Some(self.filter.kind()),
        )
    }
}

/// 「移出所选 (N)」的文案。参数是选中数，按钮据此置灰。
pub fn remove_selected_label(n: usize) -> String {
    format!("移出所选 ({n})")
}

/// 标题行「敏感库 N 条」。
pub fn title_text(total: i64) -> String {
    format!("敏感库 {total} 条")
}

/// 卡片第 2 行：状态词 + 岗位名。已排除的岗位名由 CSS 加删除线
/// （文字本身不加删除线字符，E2E 是拿 innerText 做比对的）。
pub fn middle_line(row: &SensitiveRecord) -> String {
    let title = row.title.as_deref().unwrap_or("").trim();
    let title = if title.is_empty() { "（无岗位名）" } else { title };
    if is_stored(row) {
        format!("已入库 {title}")
    } else {
        format!("已排除 {title}")
    }
}

/// 卡片第 3 行。已入库显示「已投递信息：…」，已排除显示「排除原因：…」。
///
/// 已排除且后端没给原因时写 `-`，**不得**留空——空串会让这一行看起来像渲染坏了。
pub fn last_line(row: &SensitiveRecord) -> String {
    let reason = row.reason.as_deref().unwrap_or("").trim();
    if is_stored(row) {
        let info = if reason.is_empty() || reason == "已入库" {
            "已入库（抓取过，下一轮自动跳过）"
        } else {
            reason
        };
        format!("已投递信息：{info}")
    } else {
        format!("排除原因：{}", if reason.is_empty() { "-" } else { reason })
    }
}

/// `kind` 字段缺失时按「已入库」处理：旧库里确实存在没有 kind 的行，
/// 把它们渲染成「已排除 + 红字」会凭空造出一批排除项。
pub fn is_stored(row: &SensitiveRecord) -> bool {
    row.kind != "rejected"
}

pub fn status_label(row: &SensitiveRecord) -> &'static str {
    if is_stored(row) {
        "已入库"
    } else {
        "已排除"
    }
}

/// 平台 chip 的显示名。未知平台原样显示，不吞信息。
pub fn platform_label(raw: &str) -> String {
    match raw {
        "boss" => "Boss直聘".to_string(),
        "" => "未知".to_string(),
        other => other.to_string(),
    }
}

/// 卡片右下角时间：`MM-DD HH:mm`（`tt.created_at` 是 Unix 秒）。
pub fn time_text(created_at: i64) -> String {
    crate::lib_ui::time::fmt_time(created_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, kind: &str, reason: Option<&str>) -> SensitiveRecord {
        SensitiveRecord {
            id,
            created_at: 1_700_000_000,
            platform: "boss".into(),
            title: Some(format!("岗位{id}")),
            reason: reason.map(str::to_string),
            kind: kind.into(),
            ..SensitiveRecord::default()
        }
    }

    #[test]
    fn filter_maps_to_backend_kind() {
        assert_eq!(Filter::All.kind(), "all");
        assert_eq!(Filter::Stored.kind(), "stored");
        assert_eq!(Filter::Rejected.kind(), "rejected");
    }

    #[test]
    fn blank_keyword_is_not_sent() {
        let mut s = SensitiveState::default();
        s.applied = "   ".into();
        assert_eq!(s.query(), (None, None, Some("all")));
        s.applied = " 后端 ".into();
        assert_eq!(s.query(), (None, Some("后端"), Some("all")));
    }

    #[test]
    fn select_all_is_per_page_and_survives_refresh_only_for_visible() {
        let mut s = SensitiveState {
            items: vec![row(1, "stored", None), row(2, "rejected", Some("薪资太低"))],
            ..SensitiveState::default()
        };
        assert!(!s.all_selected(), "未选时不算全选");
        s.toggle_select_all();
        assert!(s.all_selected());
        assert_eq!(s.selected_visible(), vec![1, 2]);
        s.toggle_select_all();
        assert!(s.selected.is_empty());

        // 页面换了一页：旧 id 必须被丢掉，否则「移出所选」会删看不见的行
        s.selected.insert(1);
        s.selected.insert(99);
        s.items = vec![row(3, "stored", None)];
        s.retain_selection();
        assert!(s.selected.is_empty(), "换了页之后旧选中必须清空");
    }

    #[test]
    fn labels_match_spec_wording() {
        assert_eq!(remove_selected_label(0), "移出所选 (0)");
        assert_eq!(remove_selected_label(3), "移出所选 (3)");
        assert_eq!(title_text(12), "敏感库 12 条");
    }

    #[test]
    fn card_lines_cover_stored_and_rejected() {
        let stored = row(1, "stored", Some("已入库"));
        assert_eq!(middle_line(&stored), "已入库 岗位1");
        assert_eq!(last_line(&stored), "已投递信息：已入库（抓取过，下一轮自动跳过）");
        assert_eq!(status_label(&stored), "已入库");

        let rejected = row(2, "rejected", Some("薪资太低"));
        assert_eq!(middle_line(&rejected), "已排除 岗位2");
        assert_eq!(last_line(&rejected), "排除原因：薪资太低");
        assert_eq!(status_label(&rejected), "已排除");

        let bare = row(3, "rejected", None);
        assert_eq!(last_line(&bare), "排除原因：-");
    }

    #[test]
    fn missing_kind_defaults_to_stored() {
        let r = SensitiveRecord { id: 1, ..SensitiveRecord::default() };
        assert!(is_stored(&r));
        assert_eq!(status_label(&r), "已入库");
    }
}
