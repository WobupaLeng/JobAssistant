//! `process` 的单元测试（从 `process.rs` 原样搬出，断言一条未改）。
//!
//! 用例按被测对象就近分布在各子模块，本文件只保留跨模块的综合用例：
//! 协议线形状、行解析、回执路由。路径仍是 `core::sidecar::process::tests::*`。

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::Duration;

use crate::core::sidecar::protocol::Event;
use crate::core::sidecar::protocol::{Command as ProtocolCommand, Expectation};

use super::codec::*;
use super::config::{locate_script_dir, SidecarConfig};
use super::handle::{SidecarEvent, SidecarHandle, SidecarReply};
use super::handle::SidecarError;

    #[test]
    fn protocol_line_matches_python_handler_shape() {
        // sidecar.py::handle 读的是同一层的 cmd / keywords，没有 params 包裹
        let c = ProtocolCommand::ScrapeStart {
            platform: "boss".into(),
            keywords: vec!["java".into()],
            max_jd: 40,
            min_score: 70,
            city: Some("101020100".into()),
            salary: vec!["406".into()],
            experience: Vec::new(),
            degree: Vec::new(),
            scale: Vec::new(),
            stage: Vec::new(),
            industry: Vec::new(),
            job_type: None,
            max_pages: Some(3),
        };
        let line = protocol_line("c1", &c).unwrap();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], "c1");
        assert_eq!(v["cmd"], "scrape.start");
        assert_eq!(v["keywords"][0], "java");
        assert_eq!(v["min_score"], 70);
        assert_eq!(v["city"], "101020100");
        assert_eq!(v["salary"][0], "406");
        assert_eq!(v["max_pages"], 3);
        assert!(v.get("params").is_none(), "参数必须平铺，Python 侧按平铺读取");
    }

    #[test]
    fn protocol_line_handles_bare_commands() {
        let line = protocol_line("c7", &ProtocolCommand::Ping).unwrap();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["cmd"], "ping");
        assert_eq!(v["id"], "c7");
    }

    #[test]
    fn parse_line_reads_ready_log_login_and_job() {
        assert!(matches!(
            parse_line(r#"{"event":"ready","data":{"pid":42}}"#),
            LineParse::Event(SidecarEvent::Ready { pid: 42 })
        ));
        match parse_line(r#"{"event":"log","data":{"level":"error","msg":"没登录"}}"#) {
            LineParse::Event(SidecarEvent::Event(Event::Log { level, msg })) => {
                assert_eq!(level, "error");
                assert_eq!(msg, "没登录");
            }
            other => panic!("实际 {other:?}"),
        }
        match parse_line(
            r#"{"event":"login.state","data":{"platform":"boss","state":"logged_in","msg":"ok"}}"#,
        ) {
            LineParse::Event(SidecarEvent::Event(Event::LoginState { state, .. })) => {
                assert_eq!(state, "logged_in")
            }
            other => panic!("实际 {other:?}"),
        }
        match parse_line(r#"{"event":"job.scraped","data":{"job_id":"boss:1","title":"后端"}}"#) {
            LineParse::Event(SidecarEvent::Event(Event::JobScraped(j))) => {
                assert_eq!(j.job_id, "boss:1");
                assert_eq!(j.platform, "boss");
            }
            other => panic!("实际 {other:?}"),
        }
    }

    #[test]
    fn parse_line_ignores_replies_and_unknowns() {
        // 回执只表示「收到了」，不应作为事件冒泡给抓取流程
        assert!(matches!(
            parse_line(r#"{"id":"c1","ok":true,"data":{}}"#),
            LineParse::Ignore
        ));
        assert!(matches!(
            parse_line(r#"{"event":"某天新增","data":{}}"#),
            LineParse::Ignore
        ));
        // 非 JSON 才是真畸形
        assert!(matches!(parse_line("这不是 JSON"), LineParse::Malformed));
        assert!(matches!(parse_line(""), LineParse::Malformed));
    }

    /// 回归用例：命令回执曾经被 `unwrap_or_else(Malformed)` 包装成「无法解析」。
    ///
    /// 实测现象是点「开始抓取」后浏览器还没起来，feed 里就先冒出一行
    /// `无法解析：{"id": "c1", "ok": true, "data": {"accepted": true}}`。
    /// 单测当时全绿，因为只断定了 `parse_line(..).is_none()`，
    /// 没有覆盖调用方把这个 `None` 解释成了什么。
    #[test]
    fn scrape_start_reply_is_not_malformed() {
        let reply = r#"{"id":"c1","ok":true,"data":{"accepted":true}}"#;
        assert!(matches!(parse_line(reply), LineParse::Ignore));
        // 真正的畸形行仍然要能被看见
        assert!(matches!(
            parse_line("Traceback (most recent call last):"),
            LineParse::Malformed
        ));
    }

    /// sidecar 的事件帧本身也带 `ok`，不能因为看到 ok 就当回执丢掉。
    #[test]
    fn parse_line_keeps_events_that_also_carry_ok() {
        let ev = parse_line(
            r#"{"id":"e1","event":"log","ok":true,"data":{"level":"info","msg":"开始"}}"#,
        );
        match ev {
            LineParse::Event(SidecarEvent::Event(Event::Log { level, msg })) => {
                assert_eq!(level, "info");
                assert_eq!(msg, "开始");
            }
            other => panic!("事件帧被误判为回执：{other:?}"),
        }
        // ready 帧同理
        assert!(matches!(
            parse_line(r#"{"event":"ready","ok":true,"data":{"pid":7}}"#),
            LineParse::Event(SidecarEvent::Ready { pid: 7 })
        ));
    }

    #[test]
    fn job_scraped_without_id_is_not_parsed() {
        // 缺 job_id 的岗位无法去重入库；解析失败后读线程会发 Malformed 记日志
        assert!(matches!(
            parse_line(r#"{"event":"job.scraped","data":{"title":"无 ID"}}"#),
            LineParse::Malformed
        ));
    }

    #[test]
    fn truncate_keeps_char_boundary() {
        let s = "汉".repeat(2000);
        let out = truncate(&s, 10);
        assert!(out.ends_with("…[截断]"));
        assert!(out.len() < 40);
    }

    #[test]
    fn spawn_rejects_missing_script() {
        let cfg = SidecarConfig::new(std::env::temp_dir().join("aja_no_such_dir"));
        match SidecarHandle::spawn(&cfg, Duration::from_millis(200)) {
            Err(SidecarError::Spawn { program, .. }) => {
                assert!(program.contains("sidecar.py"), "实际 {program}")
            }
            Err(other) => panic!("应报找不到脚本，实际 {other}"),
            Ok(_) => panic!("不该启动成功"),
        }
    }

    /// 回归：临时克隆出 scope 不得关停仍在用的 sidecar。
    ///
    /// `SidecarHandle` 是 Arc 克隆共享，`handle_clone()` 供 login_status /
    /// 聊天做锁外请求。历史上 `Drop` 无条件 shutdown，请求结束克隆被丢弃
    /// 就会关 stdin 杀进程（KILL_ON_JOB_CLOSE 连带整棵 Chrome 树）——
    /// 前端挂载时的 login_status 兜底查询会杀死刚拉起的 sidecar。
    #[test]
    fn dropping_handle_clone_keeps_sidecar_alive() {
        let dir = std::env::temp_dir().join(format!("aja-clone-drop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        std::fs::write(
            dir.join("sidecar.py"),
            "import sys, os, json\n\
             print(json.dumps({'event': 'ready', 'data': {'pid': os.getpid()}}), flush=True)\n\
             for _ in sys.stdin:\n    pass\n",
        )
        .expect("写假 sidecar");
        let cfg = SidecarConfig::new(&dir);
        let (handle, _rx) = SidecarHandle::spawn(&cfg, Duration::from_secs(10)).expect("spawn");
        {
            let clone = handle.clone();
            assert!(clone.is_alive());
        } // 克隆在此出 scope：不得触发 shutdown
        std::thread::sleep(Duration::from_millis(300));
        assert!(handle.is_alive(), "临时克隆 drop 杀死了仍在用的 sidecar");
        handle
            .write_line("{\"id\":\"t1\",\"cmd\":\"ping\"}")
            .expect("克隆 drop 后 stdin 应仍可写");
        handle.shutdown(Duration::from_millis(200));
        assert!(!handle.is_alive(), "显式 shutdown 仍应生效");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn locate_script_dir_finds_repo_scraper() {
        // 开发树里 core/ 的上一级就有 scraper/，这条锚点保证路径推导没写错层级
        let found = locate_script_dir(None, Path::new(env!("CARGO_MANIFEST_DIR")));
        assert!(
            found.is_some(),
            "应能从 {} 找到 scraper/sidecar.py",
            env!("CARGO_MANIFEST_DIR")
        );
    }

    // ---- 回执路由 ----
    //
    // 聊天是请求/应答：命令发出去必须能等到同一个 id 的回执。这些用例盯住
    // 的正是「回执被静默丢掉」这个失效模式——它不会报错，只会让界面点了没反应。

    fn pending_map() -> Mutex<HashMap<String, Sender<SidecarReply>>> {
        Mutex::new(HashMap::new())
    }

    #[test]
    fn route_reply_delivers_to_waiting_request() {
        let pending = pending_map();
        let (tx, rx) = mpsc::channel();
        pending.lock().unwrap().insert("c9".into(), tx);

        let routed = route_reply(
            r#"{"id":"c9","ok":true,"data":{"items":[1,2]}}"#,
            &pending,
        );
        assert!(routed, "在途请求的回执必须被路由，不能落到 Ignore");
        let reply = rx.recv_timeout(Duration::from_millis(100)).expect("应收到回执");
        assert!(reply.ok);
        assert_eq!(reply.data["items"][0], 1);
        // 消费一次后必须摘表：否则表格会随每次聊天无限增长
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn route_reply_carries_failure_reason() {
        let pending = pending_map();
        let (tx, rx) = mpsc::channel();
        pending.lock().unwrap().insert("c3".into(), tx);

        assert!(route_reply(
            r#"{"id":"c3","ok":false,"error":"未登录"}"#,
            &pending
        ));
        let reply = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        assert!(!reply.ok);
        assert_eq!(reply.into_result().unwrap_err(), "未登录");
    }

    /// sidecar 发的 `error` 是**对象**（`{"code","msg"}`），不是字符串。
    ///
    /// 这条用例盯的是一个真实发生过的静默故障：`error` 只按字符串解析，于是
    /// 每个失败帧的原因都变成 `None`，界面统一显示「sidecar 未给出失败原因」，
    /// 排查时完全无从下手。
    #[test]
    fn route_reply_carries_object_error_msg() {
        let pending = pending_map();
        let (tx, rx) = mpsc::channel();
        pending.lock().unwrap().insert("c7".into(), tx);

        assert!(route_reply(
            r#"{"id":"c7","ok":false,"error":{"code":"EXPECT_FAILED","msg":"ValueError: 未登录"}}"#,
            &pending
        ));
        let reply = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        assert_eq!(reply.into_result().unwrap_err(), "ValueError: 未登录");
    }

    /// 只带 code 也要透出来：比「未给出失败原因」多给了唯一可查的线索。
    #[test]
    fn error_text_falls_back_to_code() {
        assert_eq!(
            error_text(Some(&serde_json::json!({"code": "CHAT_FAILED"}))).unwrap(),
            "sidecar 返回错误 CHAT_FAILED（未附说明）"
        );
        assert_eq!(error_text(None), None);
        assert_eq!(error_text(Some(&serde_json::json!("纯字符串"))).unwrap(), "纯字符串");
    }

    #[test]
    fn route_reply_ignores_events_even_when_they_echo_an_id() {
        let pending = pending_map();
        let (tx, _rx) = mpsc::channel();
        pending.lock().unwrap().insert("e1".into(), tx);

        // 事件帧也会带 id；它属于事件流，绝不能抢走这个 id 的回执槽位
        assert!(!route_reply(
            r#"{"id":"e1","event":"log","data":{"level":"info","msg":"x"}}"#,
            &pending
        ));
        assert!(pending.lock().unwrap().contains_key("e1"));
    }

    #[test]
    fn route_reply_ignores_unknown_and_unparseable_lines() {
        let pending = pending_map();
        // 不是我们发的 id：不投递，也不 panic
        assert!(!route_reply(r#"{"id":"zzz","ok":true}"#, &pending));
        // 没有 id 的回执无法归属
        assert!(!route_reply(r#"{"ok":true}"#, &pending));
        assert!(!route_reply("Traceback (most recent call last):", &pending));
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn route_reply_leaves_other_pending_requests_alone() {
        let pending = pending_map();
        let (tx_a, rx_a) = mpsc::channel();
        let (tx_b, rx_b) = mpsc::channel();
        pending.lock().unwrap().insert("c1".into(), tx_a);
        pending.lock().unwrap().insert("c2".into(), tx_b);

        assert!(route_reply(r#"{"id":"c2","ok":true,"data":{}}"#, &pending));
        assert!(rx_b.recv_timeout(Duration::from_millis(100)).is_ok());
        // c1 没收到回执，必须仍在表里等
        assert!(rx_a.recv_timeout(Duration::from_millis(50)).is_err());
        assert!(pending.lock().unwrap().contains_key("c1"));
    }

    #[test]
    fn chat_commands_serialize_to_flat_fields() {
        // 与 Python 侧 `handle(cmd)` 的取值方式对齐：字段平铺，不套 params
        let line = protocol_line(
            "c5",
            &ProtocolCommand::ChatSend {
                uid: "u1".into(),
                text: "您好".into(),
            },
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["cmd"], "chat.send");
        assert_eq!(v["uid"], "u1");
        assert_eq!(v["text"], "您好");

        let line = protocol_line("c6", &ProtocolCommand::ChatMessages { uid: "u2".into() }).unwrap();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["cmd"], "chat.messages");
        assert_eq!(v["uid"], "u2");
    }

    /// `expectation.list` 的帧名必须与 `scraper/sidecar.py::handle` 的分支逐字一致，
    /// 写错不会编译失败，只会让点击「读取期望职位」永远报 UNKNOWN_CMD。
    #[test]
    fn expectation_list_frame_name_matches_python() {
        let line = protocol_line("c7", &ProtocolCommand::ExpectationList).unwrap();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["cmd"], "expectation.list");
        assert!(v.get("uid").is_none());
    }

    /// 回执里的期望职位要能直接反序列化成 `Expectation`；
    /// 缺字段按空串处理（sidecar 的 `.get(...) or ""` 在某些键缺失时会缺字段）。
    #[test]
    fn expectation_deserializes_with_missing_optional_fields() {
        let raw = serde_json::json!({"name": "Java"});
        let e: Expectation = serde_json::from_value(raw).unwrap();
        assert_eq!(e.name, "Java");
        assert_eq!(e.city, "");
        assert_eq!(e.code, "");
}
