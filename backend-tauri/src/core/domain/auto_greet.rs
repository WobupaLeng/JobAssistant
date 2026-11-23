//! 自动投递（自动打招呼）的业务规则。
//!
//! **为什么不在这里起定时线程**：浏览器同一时刻只能服务一个任务，节奏本来就
//! 依赖「上一条真的发完了」。旧版 `web/src/App.tsx` 的循环也放在前端，一次
//! 只调 `auto-greet/one` 处理一条。把循环搬进 Rust 会引入第二个调度器，还要
//! 与 sidecar 的 `_worker`/`_chat_busy` 争锁，收益为零。
//!
//! 因此本模块只承载**判定**：下一次该等多久、连续失败几次要停、停止原因是
//! 什么。这些都必须是纯函数，否则「间隔被调小」「失败不暂停」这类回归只能
//! 靠人工盯屏幕发现。
//!
//! 节奏下限照抄旧版常量（`AUTO_GAP_MS = 20000`，随机 ±5 秒）：改小它等于
//! 把账号往封禁上推，属于反爬红线，任何一次「优化」都不得低于 [`AUTO_GAP_MIN_MS`]。

/// 相邻两条之间的基础间隔（毫秒）。与旧版 `AUTO_GAP_MS` 一致。
pub const AUTO_GAP_MS: u64 = 20_000;

/// 随机抖动的半宽（毫秒）：实际间隔落在 15~25 秒。
pub const AUTO_GAP_JITTER_MS: u64 = 5_000;

/// 允许的最小间隔。**任何路径都不得产生小于它的间隔**。
pub const AUTO_GAP_MIN_MS: u64 = AUTO_GAP_MS - AUTO_GAP_JITTER_MS;

/// 连续失败到这个次数就自动暂停（旧版同样是 3）。
pub const AUTO_MAX_FAILURES: u32 = 3;

/// 计算下一条之前的等待时间：`基础 20s + (随机 0~10s − 5s)` → 15~25 秒。
///
/// `roll` 是调用方给的 0.0~1.0 随机数（测试里传固定值），**越界也不 panic**：
/// 夹到 `[0,1]`，宁可间隔退化成 15 秒也不让一条随机数把整轮自动投递打断。
pub fn next_gap_ms(roll: f64) -> u64 {
    let r = if roll.is_nan() {
        0.5
    } else {
        roll.clamp(0.0, 1.0)
    };
    let jitter = AUTO_GAP_JITTER_MS as f64 * (r * 2.0 - 1.0);
    let gap = AUTO_GAP_MS as f64 + jitter;
    // 下限兜底：夹取之后再取 max，任何未来改动都撞不穿这条线
    (gap.round().max(AUTO_GAP_MIN_MS as f64)) as u64
}

/// 自动投递的运行态。放在命令层的一个 `Mutex` 里（它是进程级 UI 状态，
/// 不是领域数据），但**状态迁移规则在这里**，命令层只调用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoGreetRuntime {
    /// 用户是否点了「开始自动打招呼」
    pub running: bool,
    /// 是否处于暂停（用户手动暂停 / 连续失败自动暂停）
    pub paused: bool,
    /// 本轮已成功发出的条数
    pub sent: u32,
    /// 连续失败计数（成功一次即清零）
    pub failures: u32,
    /// 下一条的计划发送时间（Unix 毫秒，0 = 不在倒计时）
    pub next_at_ms: u64,
    /// 停止原因（空串 = 没停）
    pub stop_reason: String,
}

impl Default for AutoGreetRuntime {
    fn default() -> Self {
        Self {
            running: false,
            paused: false,
            sent: 0,
            failures: 0,
            next_at_ms: 0,
            stop_reason: String::new(),
        }
    }
}

impl AutoGreetRuntime {
    /// 点「开始」：清空上一轮的计数与停止原因。
    pub fn start(&mut self, now_ms: u64) {
        self.running = true;
        self.paused = false;
        self.sent = 0;
        self.failures = 0;
        self.stop_reason.clear();
        // 首条不等满 20 秒：用户点了「开始」等 20 秒才动，会以为按钮没生效。
        self.next_at_ms = now_ms;
    }

    /// 用户手动暂停。
    pub fn pause(&mut self, reason: &str) {
        if self.running {
            self.paused = true;
            self.next_at_ms = 0;
            self.stop_reason = reason.to_string();
        }
    }

    /// 用户手动继续：**清零连续失败**，否则「失败 3 次自动暂停」之后一点继续
    /// 就会立刻再次暂停，看起来像按钮失灵。
    pub fn resume(&mut self, now_ms: u64) {
        self.running = true;
        self.paused = false;
        self.failures = 0;
        self.stop_reason.clear();
        self.next_at_ms = now_ms;
    }

    /// 整轮结束（发完 / 出错 / 用户关闭）：不再倒计时。
    pub fn stop(&mut self, reason: &str) {
        self.running = false;
        self.paused = false;
        self.next_at_ms = 0;
        self.stop_reason = reason.to_string();
    }

    /// 成功发出一条：计数 +1、失败清零、排下一次。
    pub fn record_sent(&mut self, now_ms: u64, roll: f64) {
        self.sent = self.sent.saturating_add(1);
        self.failures = 0;
        self.next_at_ms = now_ms.saturating_add(next_gap_ms(roll));
    }

    /// 一条失败。返回 `true` 表示已达连续失败上限、本轮必须停下。
    ///
    /// 不在这里改 `running`：停下涉及「要不要杀会话」等进程级动作，
    /// 由调用方（命令层）按返回值决定。
    pub fn record_failure(&mut self) -> bool {
        self.failures = self.failures.saturating_add(1);
        self.failures >= AUTO_MAX_FAILURES
    }

    /// 距下一次还有多少毫秒（0 = 已到点或不在倒计时）。
    pub fn remaining_ms(&self, now_ms: u64) -> u64 {
        self.next_at_ms.saturating_sub(now_ms)
    }

    /// 是否正在跑（跑了但暂停不算）。
    pub fn is_active(&self) -> bool {
        self.running && !self.paused
    }

    /// 没有待投递岗位时的收尾：按旧版文案停止。
    pub fn finish_exhausted(&mut self) {
        self.stop("待投递列表已经处理完了");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_stays_within_legacy_band() {
        // 旧版 = 20000 ± 5000，任何路径都不得低于 15 秒
        let mut min = u64::MAX;
        let mut max = 0;
        for i in 0..=100 {
            let gap = next_gap_ms(f64::from(i) / 100.0);
            min = min.min(gap);
            max = max.max(gap);
            assert!(gap >= AUTO_GAP_MIN_MS, "间隔 {gap}ms 低于下限，等于给封号递刀");
        }
        assert_eq!(min, 15_000);
        assert_eq!(max, 25_000);
        // 中点正好是基础间隔
        assert_eq!(next_gap_ms(0.5), AUTO_GAP_MS);
    }

    #[test]
    fn gap_clamps_absurd_rolls_instead_of_panicking() {
        // 随机数越界 / NaN 都不能把间隔算成 0
        assert_eq!(next_gap_ms(-5.0), AUTO_GAP_MIN_MS);
        assert_eq!(next_gap_ms(5.0), 25_000);
        assert_eq!(next_gap_ms(f64::NAN), AUTO_GAP_MS);
    }

    #[test]
    fn three_consecutive_failures_stop_and_success_resets() {
        let mut rt = AutoGreetRuntime::default();
        rt.start(1_000);
        assert_eq!(rt.next_at_ms, 1_000, "首条不等满 20 秒");
        assert!(!rt.record_failure());
        assert!(!rt.record_failure());
        assert!(rt.record_failure(), "第 3 次连续失败必须要求停止");
        // 成功一次就清零，否则「继续」按钮会立刻再触发暂停
        rt.record_sent(5_000, 0.5);
        assert_eq!(rt.failures, 0);
        assert_eq!(rt.sent, 1);
        assert_eq!(rt.next_at_ms, 25_000);
    }

    #[test]
    fn pause_resume_and_stop_transitions() {
        let mut rt = AutoGreetRuntime::default();
        rt.start(0);
        assert!(rt.is_active());
        rt.pause("手动暂停");
        assert!(!rt.is_active());
        assert_eq!(rt.next_at_ms, 0);
        rt.resume(9_000);
        assert!(rt.is_active());
        assert_eq!(rt.remaining_ms(4_000), 5_000);
        rt.finish_exhausted();
        assert!(!rt.running);
        assert!(!rt.paused);
        assert_eq!(rt.stop_reason, "待投递列表已经处理完了");
        assert_eq!(rt.remaining_ms(9_000), 0);
    }

    #[test]
    fn default_runtime_is_idle() {
        let rt = AutoGreetRuntime::default();
        assert!(!rt.running);
        assert!(!rt.paused);
        assert_eq!(rt.stop_reason, "");
        assert_eq!(rt.sent, 0);
    }
}
