//! `SqliteStore` 的单元测试。
//!
//! 从 `sqlite.rs` 原样搬出（断言一条未改）。条目数量是槽位 B 的锚点：
//! `cargo test` 的 core 用例数不得因此变化。
// 直接写在 `sqlite::tests` 模块里（不再套一层 `mod tests`）：
// `#[cfg(test)] mod tests;` 已在 `sqlite.rs` 声明本文件，再套一层会让用例
// 路径变成 `sqlite::tests::tests::*`，干扰按模块名筛选用例。
use rusqlite::params;

use super::*;
use crate::core::domain::job::{
    Job, JobQuery, JobStore, ScoreSource, SensitiveEntry, SensitiveKind, SensitiveStore,
    SortField,
};
use crate::core::domain::store_ports::{
    ConfigStore, DecisionStore, JobAdminStore, SensitiveAdminStore, SensitiveFilter,
    SensitiveQuery,
};
use crate::core::infra::sqlite::admin::MAX_BATCH_IDS;

    fn sample(job_id: &str, score: Option<u8>) -> Job {
        Job {
            job_id: job_id.to_string(),
            platform: "boss".into(),
            title: Some("Java 后端".into()),
            company: Some("某公司".into()),
            salary: Some("15-35K".into()),
            match_score: score,
            ..Default::default()
        }
    }

    #[test]
    fn insert_then_get_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.insert_job(&sample("j1", Some(85))).unwrap());

        let got = store.get_job("j1").unwrap().expect("应能读回");
        assert_eq!(got.job_id, "j1");
        assert_eq!(got.title.as_deref(), Some("Java 后端"));
        assert_eq!(got.match_score, Some(85));
        // 新写入的数据应自动带百分制标记
        assert_eq!(got.score_scale.as_deref(), Some("100"));
        // status / created_at 有默认值，不应是空串或 0
        assert_eq!(got.status, "pending");
        assert!(got.created_at > 0);
    }

    #[test]
    fn duplicate_job_id_is_ignored() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.insert_job(&sample("j1", Some(80))).unwrap());
        // 第二次同 id：不报错，但返回 false，且不覆盖原分数
        assert!(!store.insert_job(&sample("j1", Some(99))).unwrap());
        let got = store.get_job("j1").unwrap().unwrap();
        assert_eq!(got.match_score, Some(80));
    }

    #[test]
    fn get_missing_returns_none() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.get_job("nope").unwrap().is_none());
    }

    // ---------- 薪资排序 ----------

    #[test]
    fn salary_sort_orders_by_numeric_lower_bound() {
        // 旧实现用 substr 取前 3 位：「2.5-4万」被当成 2，「35-65K」被当成 35，
        // 于是 2.5 万的岗位排到了 3.5 万的后面。这里用真实文案盯住换算。
        let store = SqliteStore::open_in_memory().unwrap();
        let cases = [
            ("a", "2.5-4万"),      // 25K
            ("b", "35-65K"),       // 35K
            ("c", "1-2万"),        // 10K
            ("d", "面议"),          // 未知，排最后
            ("e", "15-25K·15薪"),  // 15K
            ("f", ""),             // 空，排最后
        ];
        for (id, salary) in cases {
            let mut job = sample(id, Some(50));
            job.salary = Some(salary.to_string());
            store.insert_job(&job).unwrap();
        }

        let q = JobQuery {
            sort: SortField::Salary,
            ascending: true,
            limit: 100,
            ..Default::default()
        };
        let ids: Vec<String> = store
            .list_jobs(&q)
            .unwrap()
            .into_iter()
            .map(|j| j.job_id)
            .collect();

        // 升序：1-2万(10) < 15-25K(15) < 2.5-4万(25) < 35-65K(35) < 面议/空(-1 排最后)
        assert_eq!(ids[0], "c", "1-2万 应排最前，实际顺序：{ids:?}");
        assert_eq!(ids[1], "e", "15-25K 应第二，实际顺序：{ids:?}");
        assert_eq!(ids[2], "a", "2.5-4万 应第三（25K），实际顺序：{ids:?}");
        assert_eq!(ids[3], "b", "35-65K 应第四，实际顺序：{ids:?}");
        // 面议与空值是 NULL，只能靠 job_id 兜底排序
        assert_eq!(&ids[4..], &["d", "f"], "面议/空应排最后，实际顺序：{ids:?}");
    }

    #[test]
    fn salary_sort_desc_puts_unknown_last_too() {
        // 未知薪资（NULL）在降序时必须仍在最后。
        // 若表达式写成 -1，降序时 -1 会跑到**最前面**，把没写薪资的岗位
        // 顶到「薪资最高」的位置上——那比不排序更误导人。
        let store = SqliteStore::open_in_memory().unwrap();
        for (id, salary) in [("a", "35-65K"), ("b", "面议"), ("c", "10-15K")] {
            let mut job = sample(id, Some(50));
            job.salary = Some(salary.to_string());
            store.insert_job(&job).unwrap();
        }
        let q = JobQuery {
            sort: SortField::Salary,
            ascending: false,
            limit: 100,
            ..Default::default()
        };
        let ids: Vec<String> = store
            .list_jobs(&q)
            .unwrap()
            .into_iter()
            .map(|j| j.job_id)
            .collect();
        assert_eq!(
            ids,
            vec!["a", "c", "b"],
            "降序：35K > 10K > 面议(NULL)，实际顺序：{ids:?}"
        );
    }

    // ---------- 全量重算的游标分页 ----------

    #[test]
    fn list_scored_ids_pages_by_job_id_cursor() {
        let store = SqliteStore::open_in_memory().unwrap();
        for id in ["c", "a", "e", "b", "d"] {
            store.insert_job(&sample(id, Some(50))).unwrap();
        }

        // 第一页：按主键升序取 2 条
        let p1 = store.list_scored_ids(None, 2).unwrap();
        assert_eq!(
            p1.iter().map(|j| j.job_id.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        // 第二页：从 b 之后继续，不重不漏
        let p2 = store.list_scored_ids(Some("b"), 2).unwrap();
        assert_eq!(
            p2.iter().map(|j| j.job_id.as_str()).collect::<Vec<_>>(),
            vec!["c", "d"]
        );

        // 最后一页不足一页长，调用方据此判断到底
        let p3 = store.list_scored_ids(Some("d"), 2).unwrap();
        assert_eq!(p3.len(), 1);
        assert_eq!(p3[0].job_id, "e");
    }

    #[test]
    fn list_scored_ids_ignores_score_so_paging_is_stable() {
        // 重算会改动 match_score：若按分数排序，写回会让分页边界漂移，
        // 同一条被算两次或多条被跳过。这里锁定「排序键是主键」这一事实。
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("a", Some(10))).unwrap();
        store.insert_job(&sample("b", Some(90))).unwrap();
        assert_eq!(store.list_scored_ids(None, 10).unwrap()[0].job_id, "a");
    }

    #[test]
    fn list_scored_ids_includes_rejected_rows() {
        // 被排除的岗位也要重算，否则用户「排除后又想看」会看到旧口径的分数
        let store = SqliteStore::open_in_memory().unwrap();
        let mut j = sample("r1", Some(30));
        j.rejected = 1;
        store.insert_job(&j).unwrap();
        assert_eq!(store.list_scored_ids(None, 10).unwrap().len(), 1);
    }

    // ---------- 敏感库：抓详情**之前**的预筛依据 ----------

    fn entry<'a>(company: &'a str, title: &'a str, kind: SensitiveKind) -> SensitiveEntry<'a> {
        SensitiveEntry {
            platform: "boss",
            company: Some(company),
            title: Some(title),
            reason: "测试",
            kind,
            job_id: Some("j1"),
            salary: None,
            location: None,
            job_url: None,
        }
    }

    #[test]
    fn sensitive_hit_matches_on_company_and_title() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store
            .add_sensitive(&entry("某公司", "Java 后端", SensitiveKind::Stored))
            .unwrap());

        let hit = store
            .sensitive_hit("boss", Some("某公司"), Some("Java 后端"))
            .unwrap()
            .expect("同公司同岗位应命中");
        assert!(hit.is_stored());

        // 换平台不命中：不同站点的同名岗位是两回事
        assert!(store
            .sensitive_hit("zhilian", Some("某公司"), Some("Java 后端"))
            .unwrap()
            .is_none());
        // 换岗位名不命中
        assert!(store
            .sensitive_hit("boss", Some("某公司"), Some("Go 后端"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn sensitive_hit_needs_both_fields() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 缺公司或缺岗位名时直接不查：卡片上偶尔会漏字段，
        // 这时应该「没命中」而不是误判成重复
        assert!(store.sensitive_hit("boss", None, Some("Java")).unwrap().is_none());
        assert!(store.sensitive_hit("boss", Some("某公司"), None).unwrap().is_none());
        assert!(store
            .sensitive_hit("boss", Some("  "), Some("Java"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn company_hit_rejects_short_names() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .add_sensitive(&entry("华为", "Java", SensitiveKind::Rejected))
            .unwrap();
        // 3 字公司名粗查重误伤太大，门限是 4 字
        assert!(store
            .sensitive_company_hit("boss", Some("华为"))
            .unwrap()
            .is_none());
        // 精确命中仍然有效
        assert!(store
            .sensitive_hit("boss", Some("华为"), Some("Java"))
            .unwrap()
            .is_some());
    }

    #[test]
    fn stored_upgrades_rejected_on_reinsert() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store
            .add_sensitive(&entry("某公司", "Java 后端", SensitiveKind::Rejected))
            .unwrap());
        // 同键再插入：不新增行，但 kind 升级为 stored
        assert!(!store
            .add_sensitive(&entry("某公司", "Java 后端", SensitiveKind::Stored))
            .unwrap());
        let hit = store
            .sensitive_hit("boss", Some("某公司"), Some("Java 后端"))
            .unwrap()
            .unwrap();
        assert!(hit.is_stored(), "已入库应覆盖之前的被排除标记");
    }

    #[test]
    fn list_filters_by_min_score() {
        let store = SqliteStore::open_in_memory().unwrap();
        for (id, score) in [("a", 50), ("b", 70), ("c", 90)] {
            store.insert_job(&sample(id, Some(score))).unwrap();
        }
        let q = JobQuery {
            min_score: Some(70),
            ..Default::default()
        };
        let jobs = store.list_jobs(&q).unwrap();
        assert_eq!(jobs.len(), 2, "应只剩 b 与 c");
    }

    #[test]
    fn list_excludes_rejected_by_default() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("ok", Some(80))).unwrap();
        let mut bad = sample("bad", Some(80));
        bad.rejected = 1;
        store.insert_job(&bad).unwrap();

        let default_q = JobQuery::default();
        assert_eq!(store.list_jobs(&default_q).unwrap().len(), 1);

        let incl = JobQuery {
            include_rejected: true,
            ..Default::default()
        };
        assert_eq!(store.list_jobs(&incl).unwrap().len(), 2);
    }

    #[test]
    fn list_orders_by_score_desc_by_default() {
        let store = SqliteStore::open_in_memory().unwrap();
        for (id, score) in [("low", 60), ("high", 95), ("mid", 75)] {
            store.insert_job(&sample(id, Some(score))).unwrap();
        }
        let jobs = store.list_jobs(&JobQuery::default()).unwrap();
        let scores: Vec<_> = jobs.iter().filter_map(|j| j.match_score).collect();
        assert_eq!(scores, vec![95, 75, 60]);
    }

    #[test]
    fn list_respects_limit() {
        let store = SqliteStore::open_in_memory().unwrap();
        for i in 0..5 {
            store
                .insert_job(&sample(&format!("j{i}"), Some(70 + i as u8)))
                .unwrap();
        }
        let q = JobQuery {
            limit: 2,
            ..Default::default()
        };
        assert_eq!(store.list_jobs(&q).unwrap().len(), 2);
    }

    #[test]
    fn update_score_sets_scale_marker() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("j1", None)).unwrap();
        store
            .update_score("j1", 88, ScoreSource::Llm, "技能高度匹配")
            .unwrap();
        let got = store.get_job("j1").unwrap().unwrap();
        assert_eq!(got.match_score, Some(88));
        assert_eq!(got.score_source.as_deref(), Some("llm"));
        assert_eq!(got.score_reason.as_deref(), Some("技能高度匹配"));
        assert_eq!(got.score_scale.as_deref(), Some("100"));
        assert!(got.updated_at.unwrap_or(0) > 0);
    }

    #[test]
    fn log_decision_appends_rows() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.log_decision("boss", "A公司/Java", "reject", "外包").unwrap();
        store.log_decision("boss", "B公司/Go", "accept", "匹配").unwrap();
        let conn = store.conn.lock().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM jd_decisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }

    /// `log_score_change` 必须把两端分数真写进 `old_score` / `new_score` 两列。
    ///
    /// 这两列是 `DECISION_UPGRADE_COLUMNS` 补出来的，此前只有 `reason` 文本里
    /// 有分数，界面得用正则从人话里抠——格式一改整列「分数变化」就没了。
    #[test]
    fn log_score_change_writes_both_score_columns() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .log_score_change("boss", "A公司/Java", "rescore", "旧 → 新", Some(72), Some(85))
            .unwrap();
        // 未打分的行：旧分必须是 NULL，不是 0
        store
            .log_score_change("boss", "B公司/Go", "rescore", "无分 → 新", None, Some(66))
            .unwrap();
        // 不带分数的写入口两列都得是 NULL
        store.log_decision("boss", "C公司/Rust", "reject", "外包").unwrap();

        let rows = store.list_decisions(0, 0).unwrap();
        assert_eq!(rows.len(), 3);
        let by_title = |t: &str| {
            rows.iter()
                .find(|r| r.company_job.as_deref() == Some(t))
                .unwrap_or_else(|| panic!("缺 {t}"))
                .clone()
        };
        let a = by_title("A公司/Java");
        assert_eq!(a.decision.as_deref(), Some("rescore"));
        assert_eq!(a.old_score, Some(72));
        assert_eq!(a.new_score, Some(85));
        assert_eq!(a.score_change().as_deref(), Some("72 → 85"));

        let b = by_title("B公司/Go");
        assert_eq!(b.old_score, None, "未打分不能写成 0");
        assert_eq!(b.new_score, Some(66));

        let c = by_title("C公司/Rust");
        assert_eq!(c.old_score, None);
        assert_eq!(c.new_score, None);
    }

    #[test]
    fn migrate_is_idempotent_on_empty_db() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(!store.migrate().unwrap(), "空库无需换算");
        assert!(!store.migrate().unwrap(), "重复调用仍无变更");
    }

    #[test]
    fn migrate_converts_legacy_scores_once() {
        let store = SqliteStore::open_in_memory().unwrap();

        // 造一条旧制数据：score_scale='10'，分数 8
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO job_queue (job_id, platform, created_at, match_score, score_scale)
                 VALUES ('legacy', 'boss', 1, 8, '10')",
                [],
            )
            .unwrap();
        }

        assert!(store.migrate().unwrap(), "首次应发生换算");
        let got = store.get_job("legacy").unwrap().unwrap();
        assert_eq!(got.match_score, Some(80), "8 分应换算成 80 分");
        assert_eq!(got.score_scale.as_deref(), Some("100"));

        // 第二次不应再放大
        store.migrate().unwrap();
        let again = store.get_job("legacy").unwrap().unwrap();
        assert_eq!(again.match_score, Some(80), "重复迁移不得二次放大");
    }

    #[test]
    fn migrate_leaves_null_scores_untouched() {
        let store = SqliteStore::open_in_memory().unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO job_queue (job_id, platform, created_at, match_score, score_scale)
                 VALUES ('nullscore', 'boss', 1, NULL, '10')",
                [],
            )
            .unwrap();
        }
        store.migrate().unwrap();
        let got = store.get_job("nullscore").unwrap().unwrap();
        assert_eq!(got.match_score, None, "NULL 不应变成 0");
        assert_eq!(got.score_scale.as_deref(), Some("100"));
    }

    #[test]
    fn migrate_converts_config_thresholds() {
        // `llm_prefilter` 是真实存在的键（Python 时代写成 5，十分制）。
        // 此前 `convert_legacy_threshold` 没有调用点，它会永远停在 5，
        // 让 LLM 初筛线按 5 分（百分制）执行 —— 等于不筛选。
        let store = SqliteStore::open_in_memory().unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO app_config (key, value) VALUES ('llm_prefilter', '5')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO app_config (key, value) VALUES ('min_score', '70')",
                [],
            )
            .unwrap();
        }

        assert!(store.migrate().unwrap(), "存在旧制门槛时应报告变更");
        let cfg: std::collections::HashMap<_, _> =
            store.config_pairs().unwrap().into_iter().collect();
        assert_eq!(cfg.get("llm_prefilter").map(String::as_str), Some("50"));
        assert_eq!(
            cfg.get("min_score").map(String::as_str),
            Some("70"),
            "已是百分制（>10）不得二次乘 10"
        );

        // 幂等：再跑一次不变
        store.migrate().unwrap();
        let cfg2: std::collections::HashMap<_, _> =
            store.config_pairs().unwrap().into_iter().collect();
        assert_eq!(cfg2.get("llm_prefilter").map(String::as_str), Some("50"));
    }

    #[test]
    fn open_in_memory_has_score_scale_column() {
        // 回归测试：score_scale 只出现在补列列表里、建表语句漏写时，
        // 新建库会缺列 → 所有写入报 `table job_queue has no column named score_scale`。
        let store = SqliteStore::open_in_memory().unwrap();
        let conn = store.conn.lock().unwrap();
        let cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(job_queue)").unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        for required in ["score_scale", "match_score", "rejected", "created_at"] {
            assert!(
                cols.contains(&required.to_string()),
                "建表后缺少列 {required}，实际列：{cols:?}"
            );
        }
    }

    #[test]
    fn upgrade_columns_fills_legacy_table() {
        // 模拟真实旧库：直接用 SCHEMA 建库会包含全部声明的列，
        // 因此这里手写一个「最小旧表」模拟 2023 年的老库结构。
        let dir = std::env::temp_dir().join(format!("aja_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("legacy.db");
        let _ = std::fs::remove_file(&db);

        {
            let conn = Connection::open(&db).unwrap();
            // 老库只有裸表，没有任何索引（索引由后续 SCHEMA 补）
            conn.execute_batch(
                "CREATE TABLE job_queue (
                    job_id TEXT PRIMARY KEY,
                    platform TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    created_at INTEGER NOT NULL
                );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO job_queue (job_id, platform, created_at) VALUES ('old', 'boss', 1)",
                [],
            )
            .unwrap();
        }

        let store = SqliteStore::open(&db).unwrap();
        // 补列后应能正常读写带新列的数据
        let got = store.get_job("old").unwrap().expect("旧行应能读回");
        assert_eq!(got.job_id, "old");
        assert_eq!(got.match_score, None);
        store
            .update_score("old", 77, ScoreSource::Rule, "补列后写入")
            .unwrap();
        assert_eq!(store.get_job("old").unwrap().unwrap().match_score, Some(77));

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn count_by_scale_groups_rows() {
        let store = SqliteStore::open_in_memory().unwrap();
        {
            let conn = store.conn.lock().unwrap();
            for (id, scale) in [("a", "100"), ("b", "100"), ("c", "10")] {
                conn.execute(
                    "INSERT INTO job_queue (job_id, platform, created_at, score_scale)
                     VALUES (?1, 'boss', 1, ?2)",
                    params![id, scale],
                )
                .unwrap();
            }
        }
        let counts = store.count_by_scale().unwrap();
        let map: std::collections::HashMap<_, _> = counts.into_iter().collect();
        assert_eq!(map.get("100"), Some(&2));
        assert_eq!(map.get("10"), Some(&1));
    }

    #[test]
    fn config_pairs_returns_all_rows_and_null_as_empty() {
        let store = SqliteStore::open_in_memory().unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO app_config (key, value) VALUES ('api_key', 'sk-abc'), ('base_url', NULL)",
                [],
            )
            .unwrap();
        }
        let pairs = store.config_pairs().unwrap();
        let map: std::collections::HashMap<_, _> = pairs.into_iter().collect();
        assert_eq!(map.get("api_key").map(String::as_str), Some("sk-abc"));
        // NULL 统一成空串，调用方不必到处判空
       assert_eq!(map.get("base_url").map(String::as_str), Some(""));
    }

    /// 回归测试：单靠 `.db` 一个文件就必须能读到全部数据。
    ///
    /// 曾经因为库处于 WAL 模式，`.db` 与 `-wal` 是两个不一致的残缺部分，
    /// 用户只拷主库时读到「表存在但 0 行」，甚至报
    /// `database disk image is malformed`。
    // ================= 管理类端口（store.py 补齐部分） =================

    #[test]
    fn set_rejected_batch_marks_and_restores() {
        let store = SqliteStore::open_in_memory().unwrap();
        for id in ["a", "b", "c"] {
            store.insert_job(&sample(id, Some(80))).unwrap();
        }
        let n = store
            .set_rejected(&["a".into(), "b".into()], true)
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(store.get_job("a").unwrap().unwrap().rejected, 1);
        assert_eq!(store.get_job("b").unwrap().unwrap().rejected, 1);
        assert_eq!(store.get_job("c").unwrap().unwrap().rejected, 0);
        assert!(store.get_job("a").unwrap().unwrap().updated_at.unwrap_or(0) > 0);

        // 恢复：只动传入的 id
        assert_eq!(store.set_rejected(&["a".into()], false).unwrap(), 1);
        assert_eq!(store.get_job("a").unwrap().unwrap().rejected, 0);
        assert_eq!(store.get_job("b").unwrap().unwrap().rejected, 1);
        // 不存在的 id 不计数
        assert_eq!(store.set_rejected(&["zzz".into()], true).unwrap(), 0);
    }

    #[test]
    fn set_delivered_writes_status_column_not_boolean() {
        // 语义锚点：已投递写的是 `status = 'delivered'`，旧库与前端都按 status 读，
        // 写成单独的布尔列会让已投递状态在列表里完全看不到。
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("a", Some(80))).unwrap();
        store.insert_job(&sample("b", Some(70))).unwrap();

        assert_eq!(store.set_delivered(&["a".into()], true).unwrap(), 1);
        assert_eq!(store.get_job("a").unwrap().unwrap().status, "delivered");
        assert_eq!(store.get_job("b").unwrap().unwrap().status, "pending");
        assert_eq!(store.delivered_ids().unwrap(), vec!["a".to_string()]);
        // rejected 不受影响：投递过 != 被排除
        assert_eq!(store.get_job("a").unwrap().unwrap().rejected, 0);

        // 撤销
        assert_eq!(store.set_delivered(&["a".into()], false).unwrap(), 1);
        assert_eq!(store.get_job("a").unwrap().unwrap().status, "pending");
        assert!(store.delivered_ids().unwrap().is_empty());
    }

    #[test]
    fn empty_batches_are_noops_not_invalid_sql() {
        // 传空数组不能拼出 `IN ()`——那是语法错误，会让整个命令报错
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("a", Some(80))).unwrap();
        assert_eq!(store.set_rejected(&[], true).unwrap(), 0);
        assert_eq!(store.set_delivered(&[], true).unwrap(), 0);
        assert_eq!(store.delete_sensitive(&[]).unwrap(), 0);
        assert_eq!(store.get_job("a").unwrap().unwrap().rejected, 0);
    }

    #[test]
    fn oversized_batch_is_rejected_without_touching_rows() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("a", Some(80))).unwrap();
        let ids: Vec<String> = (0..MAX_BATCH_IDS + 1).map(|i| format!("id{i}")).collect();
        assert!(matches!(
            store.set_rejected(&ids, true),
            Err(StoreError::BatchTooLarge { .. })
        ));
        assert_eq!(store.get_job("a").unwrap().unwrap().rejected, 0);
    }

    #[test]
    fn batch_of_many_ids_uses_numbered_placeholders() {
        // 20 个 id 走一遍，确认 `?3..?22` 的编号与绑定顺序没有错位
        let store = SqliteStore::open_in_memory().unwrap();
        let ids: Vec<String> = (0..20).map(|i| format!("j{i:02}")).collect();
        for id in &ids {
            store.insert_job(&sample(id, Some(60))).unwrap();
        }
        assert_eq!(store.set_rejected(&ids, true).unwrap(), 20);
        assert_eq!(store.rejected_ids().unwrap().len(), 20);
    }

    #[test]
    fn unscored_jobs_lists_only_null_scores() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("scored", Some(88))).unwrap();
        store.insert_job(&sample("null", None)).unwrap();
        // 被排除但没打分的行同样要补分
        let mut rej = sample("rejected_null", None);
        rej.rejected = 1;
        store.insert_job(&rej).unwrap();

        let ids: Vec<String> = store
            .unscored_jobs()
            .unwrap()
            .into_iter()
            .map(|j| j.job_id)
            .collect();
        assert_eq!(ids, vec!["null".to_string(), "rejected_null".to_string()]);
    }

    #[test]
    fn stats_reports_totals_groups_and_rounded_average() {
        let store = SqliteStore::open_in_memory().unwrap();
        for (id, score, platform) in [("a", 80u8, "boss"), ("b", 70, "boss"), ("c", 91, "zhilian")] {
            let mut j = sample(id, Some(score));
            j.platform = platform.into();
            store.insert_job(&j).unwrap();
        }
        store.insert_job(&sample("unscored", None)).unwrap();
        store.set_delivered(&["a".into()], true).unwrap();

        let s = store.stats().unwrap();
        assert_eq!(s.total, 4);
        assert_eq!(s.scored, 3);
        // (80+70+91)/3 = 80.333... → 保留 2 位
        assert_eq!(s.avg_score, Some(80.33));
        let by_status: std::collections::HashMap<_, _> = s.by_status.into_iter().collect();
        assert_eq!(by_status.get("pending"), Some(&3));
        assert_eq!(by_status.get("delivered"), Some(&1));
        let by_platform: std::collections::HashMap<_, _> = s.by_platform.into_iter().collect();
        assert_eq!(by_platform.get("boss"), Some(&3));
        assert_eq!(by_platform.get("zhilian"), Some(&1));
    }

    #[test]
    fn stats_average_is_none_when_nothing_scored() {
        // Python 是 `round(avg, 2) if avg is not None else None`，
        // 变成 0 会让界面显示「平均 0 分」
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("a", None)).unwrap();
        let s = store.stats().unwrap();
        assert_eq!(s.total, 1);
        assert_eq!(s.scored, 0);
        assert_eq!(s.avg_score, None);
    }

    #[test]
    fn job_exists_matches_insert() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("a", Some(80))).unwrap();
        assert!(store.job_exists("a").unwrap());
        assert!(!store.job_exists("b").unwrap());
        // 排除/投递都不影响「存在」
        store.set_rejected(&["a".into()], true).unwrap();
        assert!(store.job_exists("a").unwrap());
    }

    #[test]
    fn count_jobs_matches_list_filters() {
        let store = SqliteStore::open_in_memory().unwrap();
        for (id, score) in [("a", 50), ("b", 70), ("c", 90)] {
            store.insert_job(&sample(id, Some(score))).unwrap();
        }
        let mut rej = sample("r", Some(99));
        rej.rejected = 1;
        store.insert_job(&rej).unwrap();
        store.insert_job(&sample("u", None)).unwrap();

        let q = JobQuery {
            min_score: Some(70),
            ..Default::default()
        };
        assert_eq!(store.count_jobs(&q).unwrap(), 2);
        assert_eq!(store.list_jobs(&q).unwrap().len(), 2, "count 与 list 必须一致");

        // 默认不包含被排除的 r
        assert_eq!(store.count_jobs(&JobQuery::default()).unwrap(), 4);
        let incl = JobQuery {
            include_rejected: true,
            ..Default::default()
        };
        assert_eq!(store.count_jobs(&incl).unwrap(), 5);
        // 没打分的行不能因为 NULL >= 70 求值成 NULL 而被静默剔除（COALESCE 语义）
        let only_min = JobQuery {
            min_score: Some(0),
            ..Default::default()
        };
        assert_eq!(store.count_jobs(&only_min).unwrap(), 4);
    }

    #[test]
    fn rejected_ids_lists_only_marked_rows() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.insert_job(&sample("a", Some(80))).unwrap();
        store.insert_job(&sample("b", Some(80))).unwrap();
        store.set_rejected(&["b".into()], true).unwrap();
        assert_eq!(store.rejected_ids().unwrap(), vec!["b".to_string()]);
    }

    // ---------- 敏感库管理 ----------

    fn seed_sensitive(store: &SqliteStore) {
        store
            .add_sensitive(&entry("某科技公司", "Java 后端", SensitiveKind::Stored))
            .unwrap();
        store
            .add_sensitive(&SensitiveEntry {
                job_id: Some("j2"),
                reason: "外包",
                ..entry("某某外包公司", "Go 后端", SensitiveKind::Rejected)
            })
            .unwrap();
        store
            .add_sensitive(&SensitiveEntry {
                job_id: Some("j3"),
                reason: "100% 远程",
                ..entry("第三家公司", "前端", SensitiveKind::Rejected)
            })
            .unwrap();
    }

    #[test]
    fn list_sensitive_orders_by_id_desc_and_pages() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_sensitive(&store);

        let all = store.list_sensitive(&SensitiveQuery::default()).unwrap();
        assert_eq!(
            all.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![3, 2, 1],
            "必须 id DESC（最新在前）"
        );
        assert!(all[0].is_stored() == false && all[2].is_stored());

        let page = store
            .list_sensitive(&SensitiveQuery {
                limit: 1,
                offset: 1,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id, 2);
    }

    #[test]
    fn list_sensitive_filters_by_platform_kind_and_keyword() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_sensitive(&store);
        store
            .add_sensitive(&SensitiveEntry {
                platform: "zhilian",
                ..entry("另一平台公司", "Java 后端", SensitiveKind::Rejected)
            })
            .unwrap();

        let stored = store
            .list_sensitive(&SensitiveQuery {
                kind: SensitiveFilter::Stored,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].title.as_deref(), Some("Java 后端"));

        let boss_only = store
            .list_sensitive(&SensitiveQuery {
                platform: Some("boss".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(boss_only.len(), 3);

        // 关键词命中公司 / 岗位名 / 原因三列之一
        let kw = store
            .list_sensitive(&SensitiveQuery {
                keyword: Some("外包".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(kw.len(), 1);
        assert_eq!(kw[0].reason.as_deref(), Some("外包"));
        let kw2 = store
            .list_sensitive(&SensitiveQuery {
                keyword: Some("Java".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(kw2.len(), 2);

        // `%` 必须当字面量：不转义时 "100%" 会退化成「以 100 开头」
        let pct = store
            .list_sensitive(&SensitiveQuery {
                keyword: Some("100%".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(pct.len(), 1);
        let no_pct = store
            .list_sensitive(&SensitiveQuery {
                keyword: Some("%远程".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(no_pct.len(), 0, "前导 % 不应被当成通配符");
    }

    #[test]
    fn count_sensitive_by_platform() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_sensitive(&store);
        assert_eq!(store.count_sensitive(None).unwrap(), 3);
        assert_eq!(store.count_sensitive(Some("boss")).unwrap(), 3);
        assert_eq!(store.count_sensitive(Some("zhilian")).unwrap(), 0);
    }

    #[test]
    fn delete_sensitive_by_ids() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_sensitive(&store);
        assert_eq!(store.delete_sensitive(&[1, 3]).unwrap(), 2);
        let left = store.list_sensitive(&SensitiveQuery::default()).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, 2);
        // 不存在的 id 不计数
        assert_eq!(store.delete_sensitive(&[999]).unwrap(), 0);
    }

    #[test]
    fn clear_sensitive_empties_table() {
        let store = SqliteStore::open_in_memory().unwrap();
        seed_sensitive(&store);
        assert_eq!(store.clear_sensitive().unwrap(), 3);
        assert_eq!(store.count_sensitive(None).unwrap(), 0);
        assert_eq!(store.clear_sensitive().unwrap(), 0);
    }

    #[test]
    fn sync_stored_backfills_and_upgrades_kind() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 库里已有 a（带 reason='外包' 的 rejected 记录，模拟先抓到又被排除再入库）
        store.insert_job(&sample("a", Some(80))).unwrap();
        let mut b = sample("b", None);
        b.title = Some("Go 后端".into());
        b.company = Some("某公司".into());
        store.insert_job(&b).unwrap();
        store
            .add_sensitive(&entry("某公司", "Java 后端", SensitiveKind::Rejected))
            .unwrap();

        // 本次补入：只新增 b 那一条；a 已存在 → 不计数但升级为 stored
        assert_eq!(store.sync_stored().unwrap(), 1);
        assert_eq!(store.count_sensitive(None).unwrap(), 2);

        let hit_a = store
            .sensitive_hit("boss", Some("某公司"), Some("Java 后端"))
            .unwrap()
            .unwrap();
        assert!(hit_a.is_stored(), "存量岗位必须升级为 stored");

        let recs = store.list_sensitive(&SensitiveQuery::default()).unwrap();
        let b_rec = recs
            .iter()
            .find(|r| r.title.as_deref() == Some("Go 后端"))
            .expect("b 应被补进敏感库");
        assert!(b_rec.is_stored());
        assert_eq!(b_rec.reason.as_deref(), Some("已入库（历史数据）"));
        assert_eq!(b_rec.job_id.as_deref(), Some("b"));

        // 幂等：再跑一次不再新增
        assert_eq!(store.sync_stored().unwrap(), 0);
    }

    #[test]
    fn sync_stored_on_empty_job_queue_is_zero() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(store.sync_stored().unwrap(), 0);
    }

    // ---------- 决策日志 ----------

    #[test]
    fn list_decisions_pages_by_id_cursor() {
        let store = SqliteStore::open_in_memory().unwrap();
        for i in 0..5 {
            store
                .log_decision("boss", &format!("公司{i}/Java"), "reject", "外包")
                .unwrap();
        }
        let first = store.list_decisions(2, 0).unwrap();
        assert_eq!(first.iter().map(|d| d.id).collect::<Vec<_>>(), vec![5, 4]);
        assert_eq!(first[0].decision.as_deref(), Some("reject"));

        // 游标语义照抄 Python：`id > after_id`（**不是**「比游标更小的」），
        // 所以 after_id=3 返回的是比 3 更新的 5、4
        let next = store.list_decisions(2, 3).unwrap();
        assert_eq!(next.iter().map(|d| d.id).collect::<Vec<_>>(), vec![5, 4]);
        // 从最新的之后查 → 空
        assert!(store.list_decisions(2, 5).unwrap().is_empty());

        // limit = 0 → 默认 200，而不是「一条都不给」
        assert_eq!(store.list_decisions(0, 0).unwrap().len(), 5);
    }

    // ---------- app_config ----------

    #[test]
    fn get_config_returns_default_when_missing_null_or_empty() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(store.get_config("nope", "fallback").unwrap(), "fallback");

        store.set_config("api_key", Some("sk-abc")).unwrap();
        assert_eq!(store.get_config("api_key", "fallback").unwrap(), "sk-abc");

        // upsert：二次写入覆盖原值
        store.set_config("api_key", Some("sk-new")).unwrap();
        assert_eq!(store.get_config("api_key", "").unwrap(), "sk-new");

        // None → 空串（与 Python 一致）
        store.set_config("blank", None).unwrap();
        assert_eq!(store.get_config("blank", "fallback").unwrap(), "fallback");

        // 库里是 NULL 的行同样回退
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("INSERT INTO app_config (key, value) VALUES ('nullv', NULL)", [])
                .unwrap();
        }
        assert_eq!(store.get_config("nullv", "fallback").unwrap(), "fallback");
    }

    #[test]
    fn config_json_roundtrip_and_fallback() {
        let store = SqliteStore::open_in_memory().unwrap();
        let default = serde_json::json!({"min_score": 70});

        // 缺失 → 默认
        assert_eq!(
            store.get_config_json("llm", default.clone()).unwrap(),
            default
        );

        store
            .set_config_json("llm", &serde_json::json!({"model": "deepseek", "n": 2}))
            .unwrap();
        assert_eq!(
            store.get_config_json("llm", default.clone()).unwrap(),
            serde_json::json!({"model": "deepseek", "n": 2})
        );

        // 中文不转义（与 Python ensure_ascii=False 一致）
        store
            .set_config_json("kw", &serde_json::json!(["后端", "Java"]))
            .unwrap();
        assert_eq!(store.get_config("kw", "").unwrap(), r#"["后端","Java"]"#);

        // 坏 JSON 回退 default 而不是 panic
        store.set_config("broken", Some("{not json")).unwrap();
        assert_eq!(
            store.get_config_json("broken", default.clone()).unwrap(),
            default
        );

        // 合法 JSON 空串：不能被「空值判定」吃掉
        store.set_config("emptystr", Some("\"\"")).unwrap();
        assert_eq!(
            store.get_config_json("emptystr", default.clone()).unwrap(),
            serde_json::json!("")
        );
    }

    #[test]
    fn migration_does_not_touch_unrelated_config_keys() {
        // 回归：新增配置方法后，迁移仍只动 SCALE_CONFIG_KEYS 里的键
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .set_config_json("ui_state", &serde_json::json!({"tab": "queue"}))
            .unwrap();
        store.migrate().unwrap();
        assert_eq!(
            store.get_config_json("ui_state", serde_json::Value::Null).unwrap(),
            serde_json::json!({"tab": "queue"})
        );
    }

    #[test]
    fn single_db_file_holds_all_rows() {
        let dir = std::env::temp_dir().join(format!("aja_single_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("one.db");

        {
            let store = SqliteStore::open(&db).unwrap();
            let mut job = Job {
                job_id: "j-single".into(),
                platform: "boss".into(),
                title: Some("Java 后端".into()),
                ..Default::default()
            };
            job.match_score = Some(88);
            store.insert_job(&job).unwrap();
            // 显式落盘后再读，确保不是靠进程内缓存
            assert_eq!(
                store.get_job("j-single").unwrap().unwrap().match_score,
                Some(88)
            );
        }

        // 没有 -wal / -shm 残留：DELETE 模式下提交即写回主库
        for suffix in ["-wal", "-shm"] {
            let side = dir.join(format!("one.db{suffix}"));
            assert!(
                !side.exists() || std::fs::metadata(&side).map(|m| m.len()).unwrap_or(0) == 0,
                "不应留下非空的 {suffix}：{}",
                side.display()
            );
        }

        // 只拷主库再打开，数据必须还在
        let copy = dir.join("copied.db");
        std::fs::copy(&db, &copy).unwrap();
        let reopened = SqliteStore::open(&copy).unwrap();
        assert_eq!(
            reopened.get_job("j-single").unwrap().unwrap().match_score,
            Some(88),
            "只复制 .db 文件后不应丢数据"
        );
        drop(reopened);

        let _ = std::fs::remove_dir_all(&dir);
    }
