//! 日志环形缓冲（domain 层，零依赖）。
//!
//! 为什么在 `core/domain` 而不是命令层：日志的容量、时间戳单位、增量语义
//! 都是**可测的业务规则**，而锁与 Tauri 状态不是。放在这里能让
//! 「前端每 1.2 秒拉一次增量」这条路径有单测，而不是靠肉眼盯界面。
//!
//! 容量固定：日志只是「刚才发生了什么」，无限增长会让长跑进程吃到几百 MB。

/// 环形缓冲容量上限。一条日志约 200 字节，2000 条 ≈ 400 KB。
pub const LOG_CAPACITY: usize = 2000;

/// 一条日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Unix 毫秒时间戳
    pub ts_ms: u64,
    /// `info` / `warn` / `error`
    pub level: String,
    pub message: String,
    /// 写入时分配的**单调递增**序号（从 1 开始）。
    ///
    /// 存在的理由：同一毫秒很容易落进两条日志（`push` 一次抓取结果 + 一次进度），
    /// 而 `ts + level + message` 完全相同的两条**真日志**（重试两次、内容一致）
    /// 也是会发生的。界面按这条序号做列表 key 才能不混节点，也让「谁先谁后」
    /// 在时间戳相同时仍然确定。
    pub seq: u64,
}

/// 定长环形缓冲；满了丢最旧的。
#[derive(Debug)]
pub struct LogRing {
    items: std::collections::VecDeque<LogEntry>,
    /// 最近一次写入的**单调**计数，用来给游标兜底（同毫秒多条时不丢不重）
    seq: u64,
}

impl Default for LogRing {
    fn default() -> Self {
        Self::new()
    }
}

impl LogRing {
    pub const fn new() -> Self {
        Self {
            items: std::collections::VecDeque::new(),
            seq: 0,
        }
    }

    /// 时间戳由调用方注入，便于单测；生产路径用 [`now_ms`]。
    pub fn push_at(&mut self, ts_ms: u64, level: &str, message: &str) {
        self.seq += 1;
        if self.items.len() >= LOG_CAPACITY {
            self.items.pop_front();
        }
        self.items.push_back(LogEntry {
            ts_ms,
            level: if level.trim().is_empty() { "info" } else { level }.to_string(),
            message: message.to_string(),
            seq: self.seq,
        });
    }

    pub fn push(&mut self, level: &str, message: &str) {
        self.push_at(now_ms(), level, message);
    }

    /// 取 `timestamp >= since` 的条目（**闭区间**）。
    ///
    /// 用 `>=`（闭区间）而不是 `>`（开区间），取舍如下：
    /// - 开区间：前端把回执里的 `now`（= 上批最后一条的 `ts`）原样当游标带回来，
    ///   同一毫秒写入的多条日志除首条外**永久取不到**——时间戳没变，下一轮增量
    ///   仍然取不到，界面上表现为「日志停在半截」，且**不可恢复**。
    /// - 闭区间：边界那一毫秒的条目会重送一次，前端按 `(ts, level, message)`
    ///   多重集去重消化（`frontend/src/features/logs/state.rs::merge_logs`）。
    ///
    /// 两边都有代价：开区间丢数据、闭区间多传几条。丢数据不可恢复，重送可恢复、
    /// 且在界面上可见可修，故选闭区间。原作者选开区间是为了「一条都不重复」，
    /// 但那要求前端在**同一毫秒内不写两条**——抓取链路的「入库 + 进度」是同一
    /// 毫秒写的，这个前提不成立。
    ///
    /// 需要「恰好一次」语义的调用方，用 [`LogRing::since_seq`]（按单调序号取）。
    pub fn since(&self, since: u64) -> Vec<LogEntry> {
        self.items
            .iter()
            .filter(|e| e.ts_ms >= since)
            .cloned()
            .collect()
    }

    /// 取序号严格大于 `after_seq` 的条目（精确增量，不会重送）。
    ///
    /// 与 [`LogRing::since`] 的区别：序号是布尔的「谁更新」，与时间戳无关，
    /// 因此同毫秒多条既不重也不漏。命令层暂时用时间戳游标（旧版 `web/`
    /// 的 `logs(since)` 就是这个形状），本方法留给需要**恰好一次**语义的
    /// 调用方（例如把日志转发给别的界面通道）。
    pub fn since_seq(&self, after_seq: u64) -> Vec<LogEntry> {
        self.items
            .iter()
            .filter(|e| e.seq > after_seq)
            .cloned()
            .collect()
    }

    /// 当前游标：最后一条的时间戳。
    ///
    /// 空缓冲返回「当前时间」而不是 0：否则前端拿 0 当游标，下一轮会把
    /// 之后写入的日志连同历史一起重拉一遍。
    pub fn now_ms(&self) -> u64 {
        self.items
            .back()
            .map(|e| e.ts_ms)
            .unwrap_or_else(now_ms)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// 最近一次分配的序号（清空后**不**归零：序号是全局单调的，
    /// 归零会让「清空前拿到的游标」在清空后重新命中新日志）。
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

/// 当前 Unix 毫秒。
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_newest_and_drops_oldest() {
        let mut ring = LogRing::new();
        for i in 0..(LOG_CAPACITY + 5) {
            ring.push_at(1000 + i as u64, "info", &format!("第 {i} 条"));
        }
        assert_eq!(ring.len(), LOG_CAPACITY, "容量不得无限增长");
        let all = ring.since(0);
        assert_eq!(all[0].message, "第 5 条", "最旧的应被挤出");
        assert_eq!(all[all.len() - 1].message, format!("第 {} 条", LOG_CAPACITY + 4));
    }

    #[test]
    fn since_is_inclusive_so_same_millisecond_rows_are_not_lost() {
        let mut ring = LogRing::new();
        ring.push_at(100, "info", "a");
        ring.push_at(200, "warn", "b");
        ring.push_at(200, "error", "同一毫秒的第二条");
        // 游标 100：100 本身与之后都要给
        assert_eq!(ring.since(100).len(), 3);
        // 关键回归：游标 200（= 最后一条的时间戳）必须**仍能取到**同一毫秒的两条。
        // 用 `>` 时这里是 0 条，前端会永久丢掉第二段日志。
        let same_ms = ring.since(200);
        assert_eq!(same_ms.len(), 2, "since 必须是闭区间，否则同毫秒条目被吞");
        assert!(same_ms.iter().all(|e| e.ts_ms == 200));
        assert!(ring.since(201).is_empty(), "比所有条目都新时才是空");
    }

    #[test]
    fn since_seq_gives_each_entry_exactly_once() {
        let mut ring = LogRing::new();
        ring.push_at(500, "info", "同毫秒一");
        ring.push_at(500, "info", "同毫秒二");
        let first_seq = ring.since(0)[0].seq;
        let tail = ring.since_seq(first_seq);
        assert_eq!(tail.len(), 1, "序号游标不重送也不漏送");
        assert_eq!(tail[0].message, "同毫秒二");
        assert!(ring.since_seq(ring.seq()).is_empty());
    }

    #[test]
    fn seq_is_monotonic_and_survives_clear() {
        let mut ring = LogRing::new();
        ring.push_at(1, "info", "a");
        let before = ring.seq();
        ring.clear();
        assert_eq!(ring.seq(), before, "清空不归零，避免清空前的游标复活");
        ring.push_at(2, "info", "b");
        assert_eq!(ring.seq(), before + 1);
        assert_eq!(ring.since(0)[0].seq, before + 1, "序号从 1 开始且单调");
    }

    #[test]
    fn clear_makes_every_read_empty() {
        let mut ring = LogRing::new();
        for i in 0..10 {
            ring.push_at(100 + i, "info", &format!("第 {i} 条"));
        }
        assert_eq!(ring.len(), 10);
        ring.clear();
        assert!(ring.is_empty());
        assert!(ring.since(0).is_empty());
        assert!(ring.since_seq(0).is_empty());
    }

    #[test]
    fn blank_level_falls_back_to_info() {
        let mut ring = LogRing::new();
        ring.push_at(1, "", "x");
        assert_eq!(ring.since(0)[0].level, "info");
    }

    #[test]
    fn empty_ring_reports_current_time_as_cursor() {
        let ring = LogRing::new();
        let now = ring.now_ms();
        assert!(now > 0, "空缓冲也应给出可用游标");
        assert!(ring.since(0).is_empty());
        assert_eq!(ring.seq(), 0);
    }
}
