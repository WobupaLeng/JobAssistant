//! 真实 sidecar 冒烟（默认 `#[ignore]`，需显式 `--ignored` 才跑）。
//!
//! 为什么不进常规 `cargo test`：它要拉起 Python + patchright + 真实 Chrome，
//! 联网、耗时、依赖本机登录态。常规单测必须毫秒级可重复。
//!
//! 跑法（在仓库根）：
//! ```text
//! $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
//! cd backend-tauri
//! cargo test --test sidecar_smoke -- --ignored --nocapture
//! ```
//!
//! 断言口径：**进程能起来、能握手、能被关停**——这三件事是 Rust 侧的职责。
//! 抓到多少条、分数多少属于 sidecar 与领域规则，各有各的测试，这里不断言，
//! 避免网络波动让门禁变成薛定谔的绿灯。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use jobassistant_app_lib::core::domain::job::Job;
use jobassistant_app_lib::core::domain::profile::DEFAULT_PROFILE;
use jobassistant_app_lib::core::sidecar::ingest::IngestConfig;
use jobassistant_app_lib::core::sidecar::process::{locate_script_dir, SidecarConfig};
use jobassistant_app_lib::core::sidecar::session::{NoSensitive, Ports, SessionUpdate, SidecarSession};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core 必须位于仓库根下")
        .to_path_buf()
}

#[test]
#[ignore = "拉起真实 Python sidecar，需 --ignored 显式执行"]
fn sidecar_starts_handshakes_and_stops() {
    let root = repo_root();
    let script_dir = locate_script_dir(None, &root).expect("找不到 scraper/sidecar.py");

    let scfg = SidecarConfig {
        program: None,
        script_dir,
        // data_dir=None：与现行版共用 %LOCALAPPDATA%\ai-job-assistant 的登录态，
        // 否则冒烟时用户得重新扫码。
        data_dir: None,
    };
    let icfg = IngestConfig::default();

    let sink_hits = Arc::new(AtomicU32::new(0));
    let hits = Arc::clone(&sink_hits);
    let put = move |_job: &Job| -> Result<bool, String> {
        hits.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    };
    // 冒烟只关心控制通道，预筛查询器用「全不命中」的空实现
    let sink = Ports::new(
        put,
        |_e: &jobassistant_app_lib::core::domain::job::SensitiveEntry<'_>| {},
        // 冒烟只验控制通道，决策日志不接库；
        // `record` 没有默认实现，这里必须显式写出来
        |_n: &jobassistant_app_lib::core::sidecar::ingest::DecisionNote| {},
    );

    let t0 = Instant::now();
    let mut session = match SidecarSession::start(
        scfg,
        DEFAULT_PROFILE.clone(),
        icfg,
        sink,
        NoSensitive,
    ) {
        Ok(s) => s,
        // **不允许**因为起不来就静默跳过：`Spawn` 只包住了「进程创建」这一步，
        // 真正创建成功之后握手超时/协议错会走 `Handshake`/`Protocol` 分支并判红。
        // 所以能执行到 `Spawn` 失败，「python 存在但不可用」与「代码坏了」长得一样；
        // 一律判红，让门禁把问题暴露出来，而不是给一条薛定谔的绿灯。
        Err(e) => panic!("启动 sidecar 失败：{e}"),
    };
    eprintln!("ready 握手耗时 {:?}，pid={}", t0.elapsed(), session.pid());

    // 不发抓取命令：只验证控制通道存活。真实抓取由人工验收（SPEC-002 第 6 节）。
    assert!(session.send(&jobassistant_app_lib::core::sidecar::protocol::Command::Ping).is_ok());

    let mut saw_exit = false;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        match session.poll() {
            Some(SessionUpdate::Exited { code }) => {
                eprintln!("进程退出 code={code:?}");
                saw_exit = true;
                break;
            }
            Some(_) => continue,
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }

    let t1 = Instant::now();
    session.shutdown();
    eprintln!("shutdown 耗时 {:?}", t1.elapsed());

    let p = session.progress();
    eprintln!("最终进度：{p:?}");
    assert!(!saw_exit || p.finished_reason.is_some(), "进程退出必须给出原因");
    assert_eq!(sink_hits.load(Ordering::Relaxed), 0, "未发 scrape.start 不应有入库");
}
