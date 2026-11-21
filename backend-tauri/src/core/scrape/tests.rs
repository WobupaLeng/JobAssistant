//! `core::scrape` 的单元测试。
//!
//! 拆分前这些用例都写在 `scrape.rs` 的 `mod tests` 里；拆分后原样搬来，
//! 一行断言都没改——子模块拆分的目的正是「行为零变化」，
//! 测试数字因此必须与拆分前完全一致。

use std::path::PathBuf;
use std::sync::Arc;

use super::*;
use crate::core::domain::job::Job;
use crate::core::infra::SqliteStore;
use crate::core::sidecar::protocol::Command;
use crate::core::sidecar::session::{IngestPort, ScrapeProgress, STOPPED_BY_USER};

#[test]
fn idle_progress_without_reason_gets_one() {
    let mut p = ScrapeProgress::new();
    p.running = false;
    let p = finalize_idle_progress(p);
    assert_eq!(p.finished_reason.as_deref(), Some(FALLBACK_STOP_REASON));

    // 真原因必须原样保留，不能被兜底覆盖
    let mut q = ScrapeProgress::new();
    q.running = false;
    q.finished_reason = Some("已按用户请求停止".to_string());
    let q = finalize_idle_progress(q);
    assert_eq!(q.finished_reason.as_deref(), Some("已按用户请求停止"));
}

// ---------- 崩溃重启的四种情形（SPEC-002 第 7 节） ----------

fn exited(reason: &str) -> ScrapeProgress {
    ScrapeProgress {
        running: false,
        finished_reason: Some(reason.to_string()),
        ..ScrapeProgress::new()
    }
}

fn spec() -> ScrapeJobSpec {
    ScrapeJobSpec {
        script_dir: PathBuf::from("scraper"),
        platform: "boss".into(),
        keywords: vec!["Java".into()],
        max_jd: 10,
        min_score: 70,
    }
}

#[test]
fn restart_skips_when_user_stopped() {
    // 用户点了停止：退出码即使非 0 也不能复活
    let p = exited(STOPPED_BY_USER);
    assert_eq!(restart_decision(&p, 0, Some(1)), RestartAction::Skip);
    assert_eq!(restart_decision(&p, 0, None), RestartAction::Skip);
}

#[test]
fn restart_skips_normal_exit() {
    // 退出码 0 = 关键词跑完，这是正常收工
    let p = exited("抓取进程正常退出");
    assert_eq!(restart_decision(&p, 0, Some(0)), RestartAction::Skip);
}

#[test]
fn restart_happens_once_on_crash() {
    let p = exited("抓取进程异常退出，退出码 1");
    assert_eq!(restart_decision(&p, 0, Some(1)), RestartAction::Restart);
    // 退出码未知（被外部杀掉 / 断管道）同样算崩溃
    assert_eq!(restart_decision(&p, 0, None), RestartAction::Restart);
}

#[test]
fn restart_gives_up_after_one_retry() {
    // 已经重启过一次又崩：再重启只会反复起浏览器、反复弹验证
    let p = exited("抓取进程异常退出，退出码 1");
    assert_eq!(restart_decision(&p, 1, Some(1)), RestartAction::GiveUp);
    assert_eq!(restart_decision(&p, 9, Some(1)), RestartAction::GiveUp);
}

#[test]
fn tail_hint_names_code_and_last_stderr_line() {
    let hint = tail_hint("Traceback...\nModuleNotFoundError: patchright\n\n", Some(1));
    assert!(hint.contains("退出码 1"), "{hint}");
    assert!(hint.contains("patchright"), "{hint}");
    // 没有 stderr 时也不能出现空括号
    assert_eq!(tail_hint("   ", None), "（退出码未知）");
}

#[test]
fn tail_hint_truncates_on_char_boundary() {
    let long = "错".repeat(400);
    let hint = tail_hint(&long, Some(2));
    assert!(hint.contains('…'), "超长 stderr 必须截断");
    assert!(hint.chars().count() < 200);
}

#[test]
fn plan_replays_recorded_spec_on_crash() {
    let p = exited("抓取进程异常退出，退出码 1");
    let s = spec();
    assert_eq!(
        plan_restart(&p, 0, Some(1), "boom", Some(&s)),
        RestartPlan::Restart(s.clone()),
        "崩溃且没重启过，必须照原样重放"
    );
    let cmd = s.start_command();
    match cmd {
        Command::ScrapeStart {
            platform,
            keywords,
            max_jd,
            min_score,
        } => {
            assert_eq!(platform, "boss");
            assert_eq!(keywords, vec!["Java".to_string()]);
            assert_eq!(max_jd, 10);
            assert_eq!(min_score, 70);
        }
        other => panic!("实际 {other:?}"),
    }
}

#[test]
fn plan_gives_up_with_reason_after_one_retry() {
    let p = exited("抓取进程异常退出，退出码 1");
    let plan = plan_restart(&p, 1, Some(1), "ModuleNotFoundError: patchright", Some(&spec()));
    match plan {
        RestartPlan::GiveUp(reason) => {
            assert!(reason.contains("已自动重启 1 次后停止"), "{reason}");
            assert!(reason.contains("退出码 1"), "{reason}");
            assert!(reason.contains("patchright"), "{reason}");
        }
        other => panic!("实际 {other:?}"),
    }
}

#[test]
fn plan_does_nothing_without_recorded_spec() {
    let p = exited("抓取进程异常退出，退出码 1");
    // 登录 / session_open 起出来的会话没有抓取快照，崩了也不该复活
    assert_eq!(plan_restart(&p, 0, Some(1), "boom", None), RestartPlan::None);
    assert_eq!(
        plan_restart(&p, 0, Some(0), "boom", Some(&spec())),
        RestartPlan::None
    );
}

// 敏感库端口用的是真 `SqliteStore`，这里只核对「写库失败不影响主流程」的
// 契约：写入即命中，且第二次命中能看到 stored 标记。
#[test]
fn sink_and_lookup_share_the_same_store() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("内存库"));
    let job = Job {
        job_id: "boss:sink-1".into(),
        platform: "boss".into(),
        title: Some("Java 后端".into()),
        company: Some("某公司".into()),
        ..Default::default()
    };
    let mut sink = make_sink(Arc::clone(&store));
    sink.put(&job).expect("入库成功");

    let mut lookup = make_lookup(Arc::clone(&store));
    let hit = lookup("boss", Some("某公司"), Some("Java 后端"));
    let (reason, stored) = hit.expect("已入库的岗位必须命中敏感库");
    assert!(stored, "stored 标记必须为真");
    assert_eq!(reason, "已入库");

    // 同一条岗位重复 put：返回 false（已存在），但仍然登记 stored
    assert!(!sink.put(&job).expect("重复入库不报错"));
}

/// 生产端口必须把决策真的写进 `jd_decisions`。
///
/// 这是本次修复的落点：`make_sink` 早先只实现了 `put` / `remember`，
/// `IngestPort::record` 走默认空实现，于是决策日志在生产上**一条都没有**，
/// 界面只剩 `rescore` 那一档有内容。
#[test]
fn sink_writes_decisions_into_the_store() {
    use crate::core::domain::store_ports::DecisionStore;
    use crate::core::sidecar::ingest::DecisionNote;

    let store = Arc::new(SqliteStore::open_in_memory().expect("内存库"));
    let mut sink = make_sink(Arc::clone(&store));
    sink.record(&DecisionNote {
        platform: "boss".into(),
        company_job: "某公司/Java 后端".into(),
        decision: "reject",
        rule: "outsource",
        reason: "exclude：外包".into(),
        new_score: None,
    });
    let rows = store.list_decisions(0, 0).expect("读决策日志");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].decision.as_deref(), Some("reject"));
    assert_eq!(rows[0].company_job.as_deref(), Some("某公司/Java 后端"));
    assert_eq!(rows[0].reason.as_deref(), Some("exclude：外包"));
}
