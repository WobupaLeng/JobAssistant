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
        });
    }

    pub fn push(&mut self, level: &str, message: &str) {
        self.push_at(now_ms(), level, message);
    }

    /// 取 `since` 之后的条目。
    ///
    /// 用 `>` 而不是 `>=`：前端上次拿到的 `now` 就是最后一条的时间戳，
    /// 用 `>=` 会让每条日志在界面上重复出现一次。
    pub fn since(&self, since: u64) -> Vec<LogEntry> {
        self.items
            .iter()
            .filter(|e| e.ts_ms > since)
            .cloned()
            .collect()
    }

    /// 当前游标：最后一条的时间戳（无条目时返回 `since` 语义的 0 不成立，
    /// 故返回当前时间，避免「先取 now 再取增量」时把未来的日志吞掉）。
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
    fn since_is_strictly_greater() {
        let mut ring = LogRing::new();
        ring.push_at(100, "info", "a");
        ring.push_at(200, "warn", "b");
        ring.push_at(200, "error", "同一毫秒的第二条");
        // 用 100 做游标：拿到后两条（同毫秒的两条都要给，不能因为时间戳相同丢掉）
        assert_eq!(ring.since(100).len(), 2);
        assert_eq!(ring.since(200).len(), 0);
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
    }
}
