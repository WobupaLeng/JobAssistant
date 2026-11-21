//! 自动投递运行态的纯函数部分（无 IO、无 DOM，可在宿主目标上单测）。
//!
//! ## 节奏规则的归属
//! 「下一条什么时候发」**完全由后端决定**（`core/domain/auto_greet.rs`：
//! 20 秒基准 ± 5 秒随机，下限 15 秒）。前端只做两件事：
//!
//! 1. 每秒读一次 `auto_greet_status()` 刷新倒计时文字；
//! 2. 当后端回执的 `next_at_ms` **已到期**时，调 `auto_greet_next(None)` 发一条。
//!
//! 前端**不得**自己算间隔、不得自己排下一次时刻——那会把反爬红线搬到
//! 一个可被刷新 / 最小化打断的定时器上。窗口最小化时定时器被节流、循环自然
//! 停止，这是已知且接受的行为。

use crate::api::types::{AutoNextReply, AutoStatusReply};

/// 轮询间隔（毫秒）。1 秒足够让「下次 12 秒」读起来是活的，
/// 又不至于每秒打一次 IPC 让 WebView 空转。
pub const TICK_MS: i32 = 1_000;

/// 界面上的一次「用户意图」。`running` / `paused` 仍以**后端**为准：
/// 用户点了开始，前端不能自行假定已开始（后端可能因失败 3 次立刻暂停）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    /// 没开始过（后端 `running=false` 且 `stop_reason` 为空）
    #[default]
    Idle,
    /// 正在跑（含「到点发送中」）
    Running,
    /// 用户手动暂停
    Paused,
    /// 本轮已停（发完 / 连续失败 / 用户点重置）
    Stopped,
}

impl Phase {
    /// 由后端状态推导。`sending` 是前端自己的瞬时标志（一条正在路上）。
    pub fn from_status(st: &AutoStatusReply, sending: bool) -> Phase {
        if sending {
            return Phase::Running;
        }
        if st.paused {
            Phase::Paused
        } else if st.running {
            Phase::Running
        } else if st.stop_reason.is_empty() {
            Phase::Idle
        } else {
            Phase::Stopped
        }
    }

    /// 主按钮文案：开始 / 暂停 / 继续。
    pub fn toggle_label(self) -> &'static str {
        match self {
            Phase::Idle | Phase::Stopped => "开始",
            Phase::Running => "暂停",
            Phase::Paused => "继续",
        }
    }
}

/// 现在是否该调 `auto_greet_next`。
///
/// 判据只有一条：后端说「在跑、没暂停」且 `next_at_ms` 已经到期。
/// `next_at_ms == 0`（不在倒计时）一律不发——那是暂停 / 停止态。
pub fn should_send(now_ms: u64, st: &AutoStatusReply, sending: bool) -> bool {
    if sending || st.paused || !st.running {
        return false;
    }
    st.next_at_ms != 0 && now_ms >= st.next_at_ms
}

/// 「下次 12 秒」里的秒数。`remaining_ms <= 0` 时给 0（显示「下次 0 秒」而不是负数）。
pub fn seconds_left(remaining_ms: u64) -> u64 {
    // 精确到秒向上取整：12.4 秒显示「13 秒」而不是「12 秒」，
    // 否则会先跳 0 再真的发出去，看起来像卡了一下。
    remaining_ms.saturating_add(999) / 1_000
}

/// 状态胶囊文案。四段优先级：发送中 > 已暂停 > 已停 > 倒计时。
///
/// **必须始终含「已发 N 条」**：`tools/gui_e2e.py` 用正则识别这一段，
/// 少了它「自动投递到底发了几条」在界面上就无从核对。
pub fn status_text(st: &AutoStatusReply, phase: Phase, sending: bool, sending_label: &str) -> String {
    let count = format!("已发 {} 条", st.sent);
    if sending {
        let who = if sending_label.is_empty() { "下一条".to_string() } else { sending_label.to_string() };
        return format!("正在发送这条…{who} · {count}");
    }
    match phase {
        Phase::Paused => {
            let reason = if st.stop_reason.is_empty() { "已暂停" } else { st.stop_reason.as_str() };
            format!("{reason} · {count}")
        }
        Phase::Stopped => {
            let reason = if st.stop_reason.is_empty() { "已停止" } else { st.stop_reason.as_str() };
            format!("{reason} · {count}")
        }
        Phase::Idle => format!("未开始 · {count}"),
        Phase::Running => {
            let mut parts = vec![
                count,
                format!("下次 {} 秒", seconds_left(st.remaining_ms)),
            ];
            parts.push("待投递列表发完自动停止".to_string());
            if st.failures > 0 {
                parts.push(format!("连续失败 {} 次", st.failures));
            }
            if st.pending >= 0 {
                parts.push(format!("待投递 {}", st.pending));
            }
            parts.join(" · ")
        }
    }
}

/// 状态点的语气：`running` 呼吸 / `warn` 暂停 / `idle` 未开始 / `bad` 出错停。
pub fn status_dot(phase: Phase, failures: u32) -> &'static str {
    match phase {
        Phase::Running => "running",
        Phase::Paused => "warn",
        Phase::Idle => "idle",
        Phase::Stopped => {
            if failures >= 3 {
                "bad"
            } else {
                "idle"
            }
        }
    }
}

/// 一次 `auto_greet_next` 回执 → 一条给人看的结果行。空串表示没什么可说的。
pub fn reply_note(r: &AutoNextReply) -> String {
    if r.ok {
        format!("已发送：{} · {}", r.company, r.title)
    } else if r.stage == "done" {
        // 待投递列表发完了：这不是错误，不要染成红色
        String::new()
    } else if r.error.is_empty() {
        String::new()
    } else {
        format!("发送失败（{}）：{}", r.stage, r.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(running: bool, paused: bool, next_at_ms: u64, remaining_ms: u64) -> AutoStatusReply {
        AutoStatusReply {
            running,
            paused,
            sent: 3,
            failures: 0,
            next_at_ms,
            remaining_ms,
            stop_reason: String::new(),
            pending: 7,
        }
    }

    #[test]
    fn only_expired_next_at_triggers_send() {
        let st = status(true, false, 10_000, 4_000);
        assert!(!should_send(9_999, &st, false));
        assert!(should_send(10_000, &st, false));
        assert!(should_send(10_001, &st, false));
        assert!(!should_send(10_001, &st, true), "一条在途时不得再发");
    }

    #[test]
    fn paused_or_stopped_never_sends() {
        let paused = status(true, true, 0, 0);
        assert!(!should_send(u64::MAX, &paused, false));
        let stopped = status(false, false, 0, 0);
        assert!(!should_send(u64::MAX, &stopped, false));
        let running_but_no_deadline = status(true, false, 0, 0);
        assert!(!should_send(u64::MAX, &running_but_no_deadline, false), "next_at_ms=0 表示不在倒计时");
    }

    #[test]
    fn phase_comes_from_backend_not_from_local_hopes() {
        assert_eq!(Phase::from_status(&status(false, false, 0, 0), false), Phase::Idle);
        assert_eq!(Phase::from_status(&status(true, false, 1, 0), false), Phase::Running);
        assert_eq!(Phase::from_status(&status(true, true, 0, 0), false), Phase::Paused);
        let mut done = status(false, false, 0, 0);
        done.stop_reason = "待投递列表已经处理完了".into();
        assert_eq!(Phase::from_status(&done, false), Phase::Stopped);
        assert_eq!(Phase::from_status(&done, true), Phase::Running, "在途时优先显示发送中");
    }

    #[test]
    fn toggle_labels_are_the_three_legacy_words() {
        assert_eq!(Phase::Idle.toggle_label(), "开始");
        assert_eq!(Phase::Running.toggle_label(), "暂停");
        assert_eq!(Phase::Paused.toggle_label(), "继续");
        assert_eq!(Phase::Stopped.toggle_label(), "开始");
    }

    #[test]
    fn status_text_always_carries_sent_count() {
        let st = status(true, false, 1, 12_400);
        for phase in [Phase::Idle, Phase::Running, Phase::Paused, Phase::Stopped] {
            let text = status_text(&st, phase, false, "");
            assert!(text.contains("已发 3 条"), "文案缺「已发 N 条」：{text}");
        }
        let sending = status_text(&st, Phase::Running, true, "某公司 · Java");
        assert!(sending.contains("正在发送这条…"));
        assert!(sending.contains("已发 3 条"));
    }

    #[test]
    fn running_text_shows_countdown_and_auto_stop_hint() {
        let st = status(true, false, 1, 12_400);
        let text = status_text(&st, Phase::Running, false, "");
        assert!(text.contains("下次 13 秒"), "{text}");
        assert!(text.contains("待投递列表发完自动停止"), "{text}");
        assert_eq!(seconds_left(0), 0);
        assert_eq!(seconds_left(1), 1);
        assert_eq!(seconds_left(1_000), 1);
        assert_eq!(seconds_left(1_001), 2);
    }

    #[test]
    fn paused_text_shows_stop_reason() {
        let mut st = status(true, true, 0, 0);
        st.stop_reason = "连续失败 3 次已暂停".into();
        let text = status_text(&st, Phase::Paused, false, "");
        assert!(text.contains("连续失败 3 次已暂停"), "{text}");
        assert!(text.contains("已发 3 条"), "{text}");
    }

    #[test]
    fn done_stage_is_not_an_error() {
        let r = AutoNextReply {
            ok: false,
            stage: "done".into(),
            error: "待投递列表已经处理完了".into(),
            ..AutoNextReply::default()
        };
        assert_eq!(reply_note(&r), "");
        let ok = AutoNextReply {
            ok: true,
            stage: "mark".into(),
            company: "某公司".into(),
            title: "Java".into(),
            ..AutoNextReply::default()
        };
        assert_eq!(reply_note(&ok), "已发送：某公司 · Java");
        let bad = AutoNextReply {
            ok: false,
            stage: "send".into(),
            error: "浏览器未就绪".into(),
            ..AutoNextReply::default()
        };
        assert!(reply_note(&bad).contains("浏览器未就绪"));
    }
}
