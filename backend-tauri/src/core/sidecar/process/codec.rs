//! JSON Lines 编解码与三条读线程：命令序列化、行解析、回执路由、stderr 尾巴、退出等待。
//!
//! 从 `process.rs` 搬出（行为零变化）。三条线程（stdout / stderr / waiter）都
//! 在这里，因为它们共享同一套「行怎么切、怎么丢、怎么冒泡」的判断；
//! 把这些判断与句柄的状态机分开写，是这次拆分唯一的目的。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStdout};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::core::sidecar::protocol::{Command as ProtocolCommand, Event, ScrapedJob, MAX_LINE_BYTES};

use super::handle::{SidecarEvent, SidecarReply};

/// 把命令序列化成一行 JSON。
///
/// 线上格式以 `scraper/sidecar.py::handle` 为准：命令名在 `cmd` 字段，
/// 参数**平铺**在同一层（`payload.get("keywords")`），没有 `params` 包裹。
/// `protocol.rs` 的枚举派生格式恰好与此一致，这里再补一个 `id`。
pub fn protocol_line(id: &str, cmd: &ProtocolCommand) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(cmd)?;
    let mut obj = match value {
        serde_json::Value::Object(m) => m,
        _ => return Err(serde_json::Error::io(std::io::Error::other("命令不是 JSON 对象"))),
    };
    obj.insert(
        "id".to_string(),
        serde_json::Value::String(id.to_string()),
    );
    serde_json::to_string(&serde_json::Value::Object(obj))
}

/// 读线程对一行的处置结论。
///
/// 存在的理由：`Option` 无法区分「这行本来就不该冒泡」（命令回执、未知事件）
/// 与「这行是坏的」（Python traceback 混进 stdout）。早先读线程用
/// `parse_line(line).unwrap_or_else(|| Malformed)` 把两者都当失败，结果
/// `scrape.start` 的回执 `{"id":"c1","ok":true,…}` 会在界面上冒出一条
/// 「无法解析」——测试却全绿，因为单测只断言了 `is_none()`，
/// 没覆盖调用方如何解释这个 `None`。
#[derive(Debug, Clone)]
pub enum LineParse {
    /// 一个可冒泡的事件
    Event(SidecarEvent),
    /// 结构合法但与抓取无关（命令回执、未知 event 名）：静默丢弃
    Ignore,
    /// 不是 JSON 对象等畸形内容：记 `Malformed` 供排查
    Malformed,
}

pub fn parse_line(line: &str) -> LineParse {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return LineParse::Malformed,
    };
    if !v.is_object() {
        return LineParse::Malformed;
    }
    // 回执（`{"id":"c1","ok":true,"data":…}`）只有 ok 没有 event，必须整行丢弃：
    // 协议事件带 `#[serde(tag = "event")]`，缺 event 时反序列化必报 missing field，
    // 读线程会把它当成 Malformed 冒泡成「无法解析：{"id":"c1",…}」。
    // 反面同样成立——sidecar 的事件帧里**也**带 ok（`{"event":"log","ok":true}`），
    // 所以判据是「没有 event 且有 ok」，不能只看 ok 就丢。
    let event_name = match v.get("event").and_then(|e| e.as_str()) {
        Some(name) => name.to_string(),
        None => return LineParse::Ignore,
    };
    let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);

    match event_name.as_str() {
        "ready" => {
            let pid = data.get("pid").and_then(|p| p.as_u64()).unwrap_or(0) as u32;
            LineParse::Event(SidecarEvent::Ready { pid })
        }
        "job.scraped" => {
            match ScrapedJob::from_value(&data) {
                Some(job) => LineParse::Event(SidecarEvent::Event(Event::JobScraped(job))),
                // 缺 job_id 的岗位无法去重入库：算畸形，读线程会记一条 Malformed
                None => LineParse::Malformed,
            }
        }
        "log" => LineParse::Event(SidecarEvent::Event(Event::Log {
            level: json_str(&data, "level").unwrap_or_else(|| "info".to_string()),
            msg: json_str(&data, "msg").unwrap_or_default(),
        })),
        "login.state" => LineParse::Event(SidecarEvent::Event(Event::LoginState {
            platform: json_str(&data, "platform").unwrap_or_else(|| "boss".to_string()),
            state: json_str(&data, "state").unwrap_or_else(|| "error".to_string()),
            msg: json_str(&data, "msg").unwrap_or_default(),
        })),
        "scrape.finished" => LineParse::Event(SidecarEvent::Event(Event::ScrapeFinished {
            count: data.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as u32,
            stopped: data.get("stopped").and_then(|s| s.as_bool()).unwrap_or(false),
            error: json_str(&data, "error").unwrap_or_default(),
        })),
        "scrape.risk" => LineParse::Event(SidecarEvent::Event(Event::ScrapeRisk {
            platform: json_str(&data, "platform").unwrap_or_else(|| "boss".to_string()),
            reason: json_str(&data, "reason").unwrap_or_default(),
            url: json_str(&data, "url").unwrap_or_default(),
        })),
        // 未来新增的事件类型：老版本壳不认识，丢弃而不是报错
        _ => LineParse::Ignore,
    }
}
fn json_str(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// 读线程：逐行解析 sidecar 的 stdout。
///
/// 这里多做一件事——**回执路由**。带 `id` 无 `event` 的行是某条请求的回执，
/// 要先在在途表里查一次：命中就投递给等待方并从事件流摘掉，未命中才当
/// `Ignore` 丢掉。不这样做的话聊天命令永远等不到结果（历史上这些行就是
/// 被静默丢弃的，见 [`parse_line`] 的注释）。
pub(super) fn spawn_stdout_reader(
    stdout: ChildStdout,
    tx: Sender<SidecarEvent>,
    pending: Arc<Mutex<HashMap<String, Sender<SidecarReply>>>>,
) {
    std::thread::Builder::new()
        .name("sidecar-stdout".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
            loop {
                buf.clear();
                // 限制单行长度：sidecar 承诺 ≤1 MB，一旦对端违约（比如把
                // 超长 traceback 打到 stdout），读线程不能无限吃内存
                let read = reader
                    .by_ref()
                    .take(MAX_LINE_BYTES as u64 + 1)
                    .read_until(b'\n', &mut buf);
                match read {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let text = String::from_utf8_lossy(&buf);
                let line = text.trim_end_matches(['\r', '\n']);
                if line.trim().is_empty() {
                    continue;
                }
                if route_reply(line, &pending) {
                    continue;
                }
                let ev = match parse_line(line) {
                    LineParse::Event(ev) => ev,
                    LineParse::Ignore => continue,
                    LineParse::Malformed => SidecarEvent::Malformed {
                        line: truncate(line, 2000),
                    },
                };
                if tx.send(ev).is_err() {
                    break; // 接收端已销毁，收工
                }
            }
            // 收工时把所有还在等的请求唤醒：让它们立刻拿到错误，
            // 而不是各自干等到超时（用户点一下就没反应了，体验更差）
            if let Ok(mut g) = pending.lock() {
                g.clear();
            }
            let _ = tx.send(SidecarEvent::Exited { code: None });
        })
        .expect("无法创建 sidecar stdout 读线程");
}

/// 若这一行是某条在途请求的回执，投递并返回 `true`。
///
/// 判据与 [`parse_line`] 一致：**有 `id` 且没有 `event`**。sidecar 的普通
/// 事件帧也可能带 id（回显请求号），所以不能只看 id。
pub(super) fn route_reply(line: &str, pending: &Mutex<HashMap<String, Sender<SidecarReply>>>) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return false;
    };
    if v.get("event").is_some() {
        return false;
    }
    let Some(id) = v.get("id").and_then(|x| x.as_str()) else {
        return false;
    };
    let waiter = {
        let Ok(mut guard) = pending.lock() else {
            return false;
        };
        // remove 而不是 get：回执只该被消费一次
        guard.remove(id)
    };
    let Some(tx) = waiter else {
        return false; // 不是我们发的，或早已超时放弃
    };
    let reply = SidecarReply {
        ok: v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false),
        data: v.get("data").cloned().unwrap_or(serde_json::Value::Null),
        error: error_text(v.get("error")),
    };
    let _ = tx.send(reply);
    true
}

/// 把回执里的 `error` 抬成一句人话。
///
/// `scraper/sidecar.py` 发的是**对象** `{"code": "EXPECT_FAILED", "msg": "..."}`，
/// 早先这里只认字符串（`as_str()`），于是每个失败帧的原因都被吞成 `None`，
/// 界面统一显示「sidecar 未给出失败原因」——用户与开发者都拿不到任何线索。
/// 现在对象形态取 `msg`（没有就用 `code`），字符串形态原样返回。
pub(super) fn error_text(err: Option<&serde_json::Value>) -> Option<String> {
    let v = err?;
    if let Some(s) = v.as_str() {
        return (!s.trim().is_empty()).then(|| s.to_string());
    }
    let msg = v.get("msg").and_then(|x| x.as_str()).unwrap_or("").trim();
    if !msg.is_empty() {
        return Some(msg.to_string());
    }
    // 只有 code 时至少把 code 报出来，总比「未给出失败原因」强
    v.get("code")
        .and_then(|x| x.as_str())
        .filter(|c| !c.trim().is_empty())
        .map(|c| format!("sidecar 返回错误 {c}（未附说明）"))
}

pub(super) fn spawn_stderr_reader(stderr: impl Read + Send + 'static, tail: Arc<Mutex<String>>) {
    std::thread::Builder::new()
        .name("sidecar-stderr".into())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line_buf = Vec::new();
            loop {
                line_buf.clear();
                match reader.read_until(b'\n', &mut line_buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let text = String::from_utf8_lossy(&line_buf).trim().to_string();
                if text.is_empty() {
                    continue;
                }
                eprintln!("[sidecar] {text}");
                if let Ok(mut t) = tail.lock() {
                    t.push_str(&text);
                    t.push('\n');
                    // 只保末尾 4 KB：traceback 的重尾才是关键，前面都是噪声
                    if t.len() > 4096 {
                        let cut = t.len() - 4096;
                        let cut = (0..=cut).rev().find(|i| t.is_char_boundary(*i)).unwrap_or(0);
                        *t = t[cut..].to_string();
                    }
                }
            }
        })
        .expect("无法创建 sidecar stderr 读线程");
}

pub(super) fn spawn_waiter(child: Arc<Mutex<Child>>, tx: Sender<SidecarEvent>) {
    std::thread::Builder::new()
        .name("sidecar-waiter".into())
        .spawn(move || {
            let code = loop {
                {
                    let mut c = match child.lock() {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    match c.try_wait() {
                        Ok(Some(status)) => break status.code(),
                        Ok(None) => {}
                        Err(_) => return,
                    }
                }
                std::thread::sleep(Duration::from_millis(120));
            };
            let _ = tx.send(SidecarEvent::Exited { code });
        })
        .expect("无法创建 sidecar 等待线程");
}

pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[截断]", &s[..end])
}
