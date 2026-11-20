//! 「一串文本」与字符串数组之间的转换。
//!
//! 配置面板里关键词、排除词、技能都是这种编辑形式：用户在一个输入框里敲
//! `Java, Spring`，提交时切成数组，回显时再拼回去。

/// 多值文本 → 去重数组。
///
/// 分隔符与旧版 `web/src/App.tsx:8-12` 的 `SPLIT_KEYWORDS` 的字符类
/// `[\s,，、;；|/\\]+` **逐字对齐**：空白（含换行/制表符）/ 中英文逗号 /
/// 顿号 / 中英文分号 / 竖线 / 正斜杠 / 反斜杠。
///
/// 顿号与竖线曾经缺失，导致用户敲 `Java、Go` 会被当成一个关键词——用户
/// 只会看到「抓不到岗位」，没有任何报错。
///
/// **行为不变的两处调用方**：`pages/dashboard.rs`（合并期望职位）与
/// `features/scrape/expectations.rs`（展示期望职位名）都不含这几个字符，
/// 切分结果不变。
pub fn split_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(|c: char| {
        matches!(
            c,
            ',' | '，' | '、' | ';' | '；' | '|' | '/' | '\\' | '\n' | '\r' | '\t' | ' '
        )
    }) {
        let s = raw.trim();
        if !s.is_empty() && !out.iter().any(|x| x == s) {
            out.push(s.to_string());
        }
    }
    out
}

pub fn join_list(list: &[String]) -> String {
    list.join(", ")
}

/// 把若干新词并进现有关键词串，**追加不覆盖**。
///
/// 独立成函数的原因：这个「去重且保持原顺序」的规则被期望职位面板与抓取面板
/// 各写过一遍，任一处写错都会静默丢掉用户手填的关键词。
pub struct MergedKeywords {
    pub text: String,
    pub added: Vec<String>,
}

pub fn merge_keywords(current: &str, incoming: &[String]) -> MergedKeywords {
    let mut merged = split_list(current);
    let mut added = Vec::new();
    for n in incoming {
        let name = n.trim();
        if name.is_empty() {
            continue;
        }
        if !merged.iter().any(|x| x == name) {
            merged.push(name.to_string());
            added.push(name.to_string());
        }
    }
    MergedKeywords { text: join_list(&merged), added }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_handles_cjk_punctuation() {
        assert_eq!(split_list("Java, Spring；Go\nRust"), vec!["Java", "Spring", "Go", "Rust"]);
    }

    #[test]
    fn split_dedups_keeping_order() {
        assert_eq!(split_list("a b a"), vec!["a", "b"]);
    }

    /// 旧版 `SPLIT_KEYWORDS` 的字符类必须完整搬过来：顿号 `、`、竖线 `|`、
    /// 正斜杠 `/`、反斜杠 `\` 曾经缺失（BUG：`Java、Go` 被当成一个关键词）。
    #[test]
    fn split_handles_dunhao_pipe_and_slashes() {
        assert_eq!(split_list("Java、Go"), vec!["Java", "Go"]);
        assert_eq!(split_list("Java|Go"), vec!["Java", "Go"]);
        assert_eq!(split_list("Java/Go"), vec!["Java", "Go"]);
        assert_eq!(split_list("Java\\Go"), vec!["Java", "Go"]);
        // 混用：一整串里所有分隔符都要生效，且去重保序
        assert_eq!(
            split_list("Java、Go, Rust；C/C++|Python\\SQL Java"),
            vec!["Java", "Go", "Rust", "C", "C++", "Python", "SQL"]
        );
    }

    /// 与旧版逐字对齐的回归锚点：`web/src/App.tsx:8-12` 用
    /// `.split(/[\s,，、;；|/\\]+/)`，这里把同一输入过一遍。
    #[test]
    fn split_matches_legacy_split_keywords() {
        let cases = [
            ("Java, Spring Cloud, 后端开发", vec!["Java", "Spring", "Cloud", "后端开发"]),
            ("Java、Go、Rust", vec!["Java", "Go", "Rust"]),
            ("  Java\tGo\nRust  ", vec!["Java", "Go", "Rust"]),
            ("Node.js/TypeScript|K8s", vec!["Node.js", "TypeScript", "K8s"]),
            ("", Vec::<&str>::new()),
            ("、、 ，，", Vec::<&str>::new()),
        ];
        for (input, want) in cases {
            assert_eq!(split_list(input), want, "输入 {input:?}");
        }
    }

    #[test]
    fn merge_appends_without_overwriting_user_input() {
        let m = merge_keywords("Java", &["Spring".into(), "Java".into()]);
        assert_eq!(m.text, "Java, Spring");
        assert_eq!(m.added, vec!["Spring"]);
    }
}
