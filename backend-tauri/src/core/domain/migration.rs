//! 打分口径迁移：十分制 → 百分制（SPEC-002 第 3.3 节）。
//!
//! **幂等性是这个模块存在的全部理由**：
//! 无标记的 `UPDATE job_queue SET match_score = match_score * 10` 一旦重跑，
//! 70 分就会变成 700 分。因此每条记录带 `score_scale` 标记，
//! 迁移只处理标记为旧制（`"10"` 或空）的行，处理完立即置为 `"100"`。
//!
//! 迁移必须**先加列、再打标记、再换算**，且整体在一个事务里完成，
//! 中途崩溃不会留下"换了一半"的状态。

/// 当前打分制标记。写入库里的值。
pub const SCALE_CURRENT: &str = "100";

/// 旧打分制标记。空字符串与 `NULL` 同样视为旧制。
pub const SCALE_LEGACY: &str = "10";

/// 迁移涉及的配置键（`app_config` 表）。
///
/// 实测 `app_config` 里**没有** `min_score` 键（门槛是内存默认值，未持久化），
/// 但用户可能手工写入过，因此仍按"存在才处理"处理。
/// `llm_prefilter` 是真实存在的键（实测值 `5`），必须换算是本次迁移的重点。
pub const SCALE_CONFIG_KEYS: &[(&str, u32)] = &[
    ("min_score", 10),
    ("llm_prefilter", 10),
];

/// 生成幂等迁移 SQL 序列。调用方在**一个事务**内按序执行。
///
/// 返回的语句全部可重复执行：第二次执行时 `WHERE` 条件不再命中任何行。
pub fn migration_statements() -> Vec<&'static str> {
    vec![
        // 1. 加标记列。SQLite 无 ADD COLUMN IF NOT EXISTS，
        //    调用方须先查 PRAGMA table_info 判断是否已存在。
        "ALTER TABLE job_queue ADD COLUMN score_scale TEXT DEFAULT '10'",
        // 2. 把 NULL 统一成旧制标记，否则 WHERE 条件漏掉它们
        "UPDATE job_queue SET score_scale = '10' WHERE score_scale IS NULL",
        // 3. 换算：只处理标记为旧制的行
        "UPDATE job_queue SET match_score = match_score * 10 \
         WHERE score_scale = '10' AND match_score IS NOT NULL",
        // 4. 打上新标记，防止重跑
        "UPDATE job_queue SET score_scale = '100' WHERE score_scale = '10'",
    ]
}

/// 从旧值换算成百分制。已在新制里的值原样返回。
///
/// 单独的纯函数便于单测：不依赖数据库。
pub fn convert_legacy_score(score: u8, scale: Option<&str>) -> u8 {
    match scale {
        Some(SCALE_CURRENT) => score,
        // None/空/"10" 都按旧制处理
        _ => score.saturating_mul(10),
    }
}

/// 配置值是否需要换算。返回 `Some(新值)` 表示需要改写。
///
/// 用于 `min_score` / `llm_prefilter` 这类门槛配置。
pub fn convert_legacy_threshold(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let n: u32 = trimmed.parse().ok()?;
    // 已经是百分制区间（>10）就不动，避免二次乘 10
    if n > 10 {
        return None;
    }
    Some((n * 10).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_score_is_scaled() {
        assert_eq!(convert_legacy_score(7, Some("10")), 70);
        assert_eq!(convert_legacy_score(9, None), 90);
        assert_eq!(convert_legacy_score(1, Some("")), 10);
    }

    #[test]
    fn current_score_untouched() {
        // 关键：已经是百分制的分数绝不能再乘 10
        assert_eq!(convert_legacy_score(70, Some("100")), 70);
        assert_eq!(convert_legacy_score(100, Some("100")), 100);
    }

    #[test]
    fn scaling_saturates_instead_of_wrapping() {
        // u8 上限 255：30 * 10 = 300 必须饱和成 255 而不是溢出成 44
        assert_eq!(convert_legacy_score(30, Some("10")), 255);
        assert_eq!(convert_legacy_score(255, Some("10")), 255);
    }

    #[test]
    fn threshold_scaled_once() {
        assert_eq!(convert_legacy_threshold("7"), Some("70".into()));
        assert_eq!(convert_legacy_threshold("5"), Some("50".into()));
        assert_eq!(convert_legacy_threshold("0"), Some("0".into()));
    }

    #[test]
    fn threshold_already_percent_untouched() {
        // 幂等核心：70 不该变成 700
        assert_eq!(convert_legacy_threshold("70"), None);
        assert_eq!(convert_legacy_threshold("100"), None);
    }

    #[test]
    fn threshold_handles_whitespace_and_garbage() {
        assert_eq!(convert_legacy_threshold(" 5 "), Some("50".into()));
        assert_eq!(convert_legacy_threshold("abc"), None);
        assert_eq!(convert_legacy_threshold(""), None);
    }

    #[test]
    fn migration_is_ordered_column_before_update() {
        let stmts = migration_statements();
        // 加列必须是第一步，否则后续 UPDATE 引用不存在的列会报错
        assert!(stmts[0].starts_with("ALTER TABLE"));
        // 打标记必须是最后一步
        assert!(stmts[stmts.len() - 1].contains("SET score_scale = '100'"));
    }

    #[test]
    fn migration_updates_are_guarded_by_scale_flag() {
        let stmts = migration_statements();
        let scale_stmt = stmts
            .iter()
            .find(|s| s.contains("match_score = match_score * 10"))
            .expect("缺少换算语句");
        // 幂等的唯一保障：换算必须带 WHERE score_scale = '10'
        assert!(scale_stmt.contains("WHERE score_scale = '10'"));
        assert!(scale_stmt.contains("match_score IS NOT NULL"));
    }

    #[test]
    fn config_keys_cover_llm_prefilter() {
        // llm_prefilter 是真实存在的配置项（实测值 5），漏了会让 LLM 初筛失效
        assert!(SCALE_CONFIG_KEYS.iter().any(|(k, _)| *k == "llm_prefilter"));
        assert!(SCALE_CONFIG_KEYS.iter().any(|(k, _)| *k == "min_score"));
    }
}
