//! 敏感库命令层：抽屉里的「搜索 / 按类型筛 / 移出所选」。
//!
//! **职责边界**（AGENTS.md 2.2）：本模块只做「收参数 → 调 core → 错误转字符串」。
//! 敏感库怎么建、卡片阶段怎么命中，全在 `core::scrape::make_lookup` 与
//! `core::infra::sqlite` 里；这里的 `kind` / `keyword` 只是把查询条件原样下推。
//!
//! 对应旧版 `web/src/api.ts` 的 `sensitive()` 与 `deleteSensitive(ids)`。

use crate::commands_db::{store_err, with_store, CmdResult, StoreState};
use crate::core::domain::store_ports::{
    SensitiveAdminStore, SensitiveFilter, SensitiveQuery, SensitiveRecord,
};
use serde::{Deserialize, Serialize};

/// 敏感库查询入参。
///
/// `kind` 取 `all` / `stored` / `rejected`（旧版抽屉的三个按钮）；
/// 未知值按「全部」处理——抽屉里多一个标签不该让整屏报错。
#[derive(Debug, Deserialize)]
pub struct SensitiveListArgs {
    pub platform: Option<String>,
    pub keyword: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl SensitiveListArgs {
    fn into_query(self) -> SensitiveQuery {
        SensitiveQuery {
            platform: self.platform.filter(|s| !s.trim().is_empty()),
            // 前后空格的搜索词没有意义，去掉更符合「搜索框」直觉
            keyword: self
                .keyword
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty()),
            kind: SensitiveFilter::parse(self.kind.as_deref().unwrap_or("all")),
            limit: self.limit.unwrap_or(100),
            offset: self.offset.unwrap_or(0),
        }
    }
}

/// 敏感库一页：`items` + **按同一套条件算出的** `total`。
///
/// `total` 必须和 items 走同一个 WHERE，否则抽屉标题会写成「154 条」而列表
/// 只有 20 条（分页后连翻都翻不完），用户会以为数据丢了。
#[derive(Debug, Serialize)]
pub struct SensitivePage {
    pub items: Vec<SensitiveRecord>,
    pub total: i64,
}

/// 列出敏感库条目。
#[tauri::command]
pub fn sensitive_list(
    state: tauri::State<'_, StoreState>,
    args: SensitiveListArgs,
) -> CmdResult<SensitivePage> {
    let query = args.into_query();
    with_store(&state, |s| {
        let items = s.list_sensitive(&query).map_err(store_err)?;
        let total = crate::core::infra::sqlite::admin::count_sensitive_query(s, &query)
            .map_err(store_err)?;
        Ok(SensitivePage { items, total })
    })
}

/// 移出所选条目的入参。
///
/// 旧版是 `deleteSensitive(ids: number[])`，这里是尽量贴近的 `ids`；
/// 同时接受 `{"ids": [...]}` 与裸数组之外的常见驼峰写法。
#[derive(Debug, Deserialize)]
pub struct SensitiveRemoveArgs {
    #[serde(default)]
    pub ids: Vec<i64>,
}

impl SensitiveRemoveArgs {
    /// 去重并丢掉非正 id：SQLite 的 rowid 从 1 开始，0/负数不可能命中，
    /// 留着它们只会让「移出 N 条」的回执数字虚高。
    fn clean(self) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        for id in self.ids {
            if id > 0 && !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }
}

/// 删除计数：`{ok, deleted}`（与旧版回执字段一致）。
#[derive(Debug, Serialize)]
pub struct RemoveReply {
    pub ok: bool,
    pub deleted: usize,
}

/// 把所选条目移出敏感库。
///
/// 空列表直接回 0，不去碰数据库：用户只是点了个没选中的按钮。
#[tauri::command]
pub fn sensitive_remove(
    state: tauri::State<'_, StoreState>,
    args: SensitiveRemoveArgs,
) -> CmdResult<RemoveReply> {
    let ids = args.clean();
    if ids.is_empty() {
        return Ok(RemoveReply {
            ok: true,
            deleted: 0,
        });
    }
    with_store(&state, |s| {
        let deleted = s.delete_sensitive(&ids).map_err(store_err)?;
        Ok(RemoveReply { ok: true, deleted })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: serde_json::Value) -> SensitiveListArgs {
        serde_json::from_value(v).expect("入参应可反序列化")
    }

    #[test]
    fn list_args_defaults_are_full_library() {
        let q = args(serde_json::json!({})).into_query();
        assert_eq!(q.kind, SensitiveFilter::All);
        assert_eq!(q.limit, 100);
        assert_eq!(q.offset, 0);
        assert!(q.platform.is_none());
        assert!(q.keyword.is_none());
    }

    #[test]
    fn list_args_parse_kind_labels() {
        // 抽屉三个按钮：全部 / 已入库 / 已排除
        assert_eq!(args(serde_json::json!({"kind": "stored"})).into_query().kind, SensitiveFilter::Stored);
        assert_eq!(args(serde_json::json!({"kind": "rejected"})).into_query().kind, SensitiveFilter::Rejected);
        assert_eq!(args(serde_json::json!({"kind": "全部"})).into_query().kind, SensitiveFilter::All);
    }

    #[test]
    fn list_args_trim_keyword_and_drop_blank_platform() {
        let q = args(serde_json::json!({"keyword": "  华为  ", "platform": "  "})).into_query();
        assert_eq!(q.keyword.as_deref(), Some("华为"), "搜索词应去掉前后空格");
        assert!(q.platform.is_none(), "空白平台名下发到 SQL 会一条都查不到");
    }

    #[test]
    fn remove_args_drop_non_positive_and_duplicates() {
        let ids = serde_json::from_value::<SensitiveRemoveArgs>(serde_json::json!({
            "ids": [3, 3, 0, -1, 7]
        }))
        .expect("ids 应可解析")
        .clean();
        assert_eq!(ids, vec![3, 7]);
    }

    #[test]
    fn remove_args_empty_list_is_noop() {
        // 空列表与缺字段都必须落到「删 0 条」，不能报错
        let empty = SensitiveRemoveArgs { ids: Vec::new() }.clean();
        assert!(empty.is_empty());
        let missing = serde_json::from_value::<SensitiveRemoveArgs>(serde_json::json!({}))
            .expect("缺 ids 应按空列表处理")
            .clean();
        assert!(missing.is_empty());
    }

    #[test]
    fn sensitive_list_and_remove_roundtrip_on_memory_db() {
        use crate::core::domain::job::{SensitiveEntry, SensitiveKind, SensitiveStore};
        use crate::core::infra::SqliteStore;

        let store = SqliteStore::open_in_memory().expect("内存库应可打开");
        store
            .add_sensitive(&SensitiveEntry {
                platform: "boss",
                company: Some("某公司"),
                title: Some("Java 后端"),
                reason: "已入库",
                kind: SensitiveKind::Stored,
                job_id: Some("j1"),
                salary: None,
                location: None,
                job_url: None,
            })
            .expect("写入应成功");
        store
            .add_sensitive(&SensitiveEntry {
                platform: "boss",
                company: Some("另一家"),
                title: Some("Golang 后端"),
                reason: "薪资太低",
                kind: SensitiveKind::Rejected,
                job_id: Some("j2"),
                salary: None,
                location: None,
                job_url: None,
            })
            .expect("写入应成功");

        let all = SensitiveQuery {
            platform: None,
            keyword: None,
            kind: SensitiveFilter::All,
            limit: 100,
            offset: 0,
        };
        let items = store.list_sensitive(&all).expect("列表应成功");
        assert_eq!(items.len(), 2);
        let total = crate::core::infra::sqlite::admin::count_sensitive_query(&store, &all)
            .expect("计数应成功");
        assert_eq!(total, 2, "total 必须与 items 的过滤条件一致");

        // kind 过滤要同时影响 items 与 total —— 这是抽屉标题与列表不漂移的关键
        let stored = SensitiveQuery {
            kind: SensitiveFilter::Stored,
            ..all.clone()
        };
        assert_eq!(store.list_sensitive(&stored).expect("列表应成功").len(), 1);
        assert_eq!(
            crate::core::infra::sqlite::admin::count_sensitive_query(&store, &stored).expect("计数应成功"),
            1
        );

        let first_id = items[0].id;
        let deleted = store.delete_sensitive(&[first_id]).expect("删除应成功");
        assert_eq!(deleted, 1);
        assert_eq!(store.list_sensitive(&all).expect("列表应成功").len(), 1);
    }
}
