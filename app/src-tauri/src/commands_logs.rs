//! 日志命令层：把抓取链路产生的日志搬给界面的日志框（图 4 之外的那一块）。
//!
//! **职责边界**（AGENTS.md 2.2）：本模块只做「收参数 → 调 core → 错误转字符串」。
//! 环形缓冲的容量、增量的判定在 [`crate::core::domain::logbuf`]；这里只管
//! 把 sidecar 的日志喂进去、把增量吐出来。
//!
//! 数据来源是**进程内缓冲**而不是数据库：日志只是「用户想知道刚才发生了什么」，
//! 落库会让每次抓取都多几千行写放大，且日志里含 JD 片段，长期留存没有必要。
//! 重启应用后日志从空开始——这与旧版 `web/` 的 `logs(since)` 语义一致。

use std::sync::Mutex;

use crate::commands_db::CmdResult;
use crate::core::domain::logbuf::{LogEntry, LogRing};
use serde::{Deserialize, Serialize};

/// 全局日志缓冲。
///
/// 用 `static` 而不是 Tauri state：抓取消费线程（`core::sidecar::session`）
/// 里也要写日志，那条路径拿不到 `tauri::State`。
static LOGS: Mutex<LogRing> = Mutex::new(LogRing::new());

/// 供 `commands_scrape` / `commands_auto` 等模块在业务路径上追加日志。
///
/// 写失败（锁中毒）静默忽略：记日志永远不该让业务失败。
pub fn push(level: &str, message: impl AsRef<str>) {
    if let Ok(mut ring) = LOGS.lock() {
        ring.push(level, message.as_ref());
    }
}

/// 日志一行。
///
/// 与旧版 `web/src/api.ts` 的 `LogRow` 对齐：`ts` 是**毫秒**时间戳。
/// `seq` 是后端分配的单调序号，界面用它做列表 key——同一毫秒的多条日志
/// 靠它区分，否则 Leptos 的 `For` 会把它们当成同一个节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRow {
    pub ts: u64,
    pub seq: u64,
    pub level: String,
    pub message: String,
}

impl From<LogEntry> for LogRow {
    fn from(e: LogEntry) -> Self {
        Self {
            ts: e.ts_ms,
            seq: e.seq,
            level: e.level,
            message: e.message,
        }
    }
}

/// `list_logs` 的回执：条目 + **本次游标**。
///
/// 前端把 `now` 存下来，下次带 `since: now` 只取新增的——日志框每 1.2 秒拉一次，
/// 全量重传会让 WebView 每秒解析上百条重复文本。
#[derive(Debug, Serialize)]
pub struct LogsReply {
    pub items: Vec<LogRow>,
    pub now: u64,
}

/// 取 `timestamp >= since`（毫秒）的日志；`since` 为空表示从头取（最多 `LOG_CAPACITY` 条）。
///
/// 边界语义是**闭区间**（`>=`）：游标就是最后一条的时间戳，开区间会让同一毫秒的
/// 后几条永久取不到（不可恢复）；闭区间只是把边界那一毫秒重送一次（可恢复，前端
/// 按多重集去重）。取舍详见 [`LogRing::since`]。
#[tauri::command]
pub fn list_logs(since: Option<i64>) -> CmdResult<LogsReply> {
    // 负数（前端误传 / 旧版遗留）一律当作「从头取」，不让 u64 转换溢出成巨大游标
    let since = since.unwrap_or(0).max(0) as u64;
    let ring = LOGS
        .lock()
        .map_err(|_| "日志缓冲锁已中毒".to_string())?;
    Ok(LogsReply {
        items: ring.since(since).into_iter().map(LogRow::from).collect(),
        now: ring.now_ms(),
    })
}

/// 清空日志（界面「清空」按钮）。
#[tauri::command]
pub fn clear_logs() -> CmdResult<()> {
    let mut ring = LOGS
        .lock()
        .map_err(|_| "日志缓冲锁已中毒".to_string())?;
    ring.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_rows_carry_millis_and_level() {
        // 序列化形态是 E2E 契约：前端按 ts 做游标，单位必须是毫秒
        let row = LogRow::from(LogEntry {
            ts_ms: 1_700_000_000_123,
            level: "warn".into(),
            message: "低于门槛".into(),
            seq: 7,
        });
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["ts"], 1_700_000_000_123u64);
        assert_eq!(json["level"], "warn");
        assert_eq!(json["message"], "低于门槛");
        assert_eq!(json["seq"], 7);
    }

    #[test]
    fn list_logs_is_incremental() {
        // 直接验证缓冲语义（命令本身读全局缓冲，单测不去碰进程级状态）
        let mut ring = LogRing::new();
        ring.push_at(1_700_000_000_100, "info", "第一条");
        let first_now = ring.now_ms();
        ring.push_at(1_700_000_001_000, "info", "第二条");
        let all = ring.since(0);
        assert_eq!(all.len(), 2);
        // 用第一条的时间戳当游标：闭区间下第一条会重送一次（前端靠多重集去重消化），
        // 但第二条必须拿到——开区间会把它连同同毫秒的兄弟一起永久吞掉。
        let delta = ring.since(first_now);
        assert_eq!(delta.len(), 2, "闭区间：游标那一毫秒会重送一条");
        assert_eq!(delta[0].message, "第一条");
        assert_eq!(delta[1].message, "第二条");
        // 同一毫秒两条：用最后一条的 ts 当游标，闭区间必须都取得到
        let mut same = LogRing::new();
        same.push_at(1_700_000_000_777, "info", "同毫秒 A");
        same.push_at(1_700_000_000_777, "info", "同毫秒 B");
        assert_eq!(same.since(1_700_000_000_777).len(), 2);
        // 精确增量走序号
        assert_eq!(same.since_seq(1).len(), 1);
    }

    #[test]
    fn list_logs_since_future_is_empty() {
        // 边界：前端把 now 存成了未来时间戳（时钟跳变）不应该 panic 也不应回全量
        let mut ring = LogRing::new();
        ring.push("info", "x");
        assert!(ring.since(u64::MAX).is_empty());
    }

    #[test]
    fn push_then_read_through_global_ring() {
        push("info", "测试写入");
        let reply = list_logs(Some(0)).expect("读日志应成功");
        assert!(reply.items.iter().any(|r| r.message == "测试写入"));
        assert!(reply.now > 0);
    }

    #[test]
    fn since_is_inclusive_through_the_command() {
        // 契约级回归：同一毫秒的两条，用「最后一条的时间戳」当 since 必须都拿得到。
        // 前端 load_runs(false) 每次把回执的 now 当下一轮 since，开区间会永久丢日志。
        let ts = 1_700_000_000_777u64;
        {
            let mut ring = LOGS.lock().unwrap();
            ring.clear();
            ring.push_at(ts, "info", "同毫秒 A");
            ring.push_at(ts, "info", "同毫秒 B");
        }
        let reply = list_logs(Some(ts as i64)).expect("读日志应成功");
        let same: Vec<&LogRow> = reply.items.iter().filter(|r| r.ts == ts).collect();
        assert_eq!(same.len(), 2, "since 闭区间：同毫秒两条都要回");
        assert_eq!(reply.now, ts, "游标就是最后一条的时间戳");
        LOGS.lock().unwrap().clear();
    }

    #[test]
    fn clear_logs_empties_the_ring_through_the_command() {
        {
            let mut ring = LOGS.lock().unwrap();
            ring.clear();
            ring.push_at(42, "warn", "将被清空");
        }
        assert!(!list_logs(None).unwrap().items.is_empty());
        clear_logs().expect("清空应成功");
        let reply = list_logs(None).expect("读日志应成功");
        assert!(reply.items.is_empty(), "清空后 list_logs 必须返回空");
        assert_eq!(reply.now, list_logs(Some(0)).unwrap().now);
    }

    #[test]
    fn negative_since_is_treated_as_beginning() {
        {
            let mut ring = LOGS.lock().unwrap();
            ring.clear();
            ring.push_at(7, "info", "x");
        }
        // -1 若按 u64 转换会变成 u64::MAX，把新建的日志全部挡掉
        assert_eq!(list_logs(Some(-1)).unwrap().items.len(), 1);
        LOGS.lock().unwrap().clear();
    }

    #[test]
    fn rows_expose_monotonic_seq_for_list_keys() {
        let mut ring = LogRing::new();
        ring.push_at(9, "info", "一");
        ring.push_at(9, "info", "二");
        let rows: Vec<LogRow> = ring.since(0).into_iter().map(LogRow::from).collect();
        assert_eq!(rows[0].seq, 1);
        assert!(rows[1].seq > rows[0].seq, "同毫秒也要有可区分的 key");
    }
}
