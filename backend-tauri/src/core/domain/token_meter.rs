//! 词元计数装饰器：在 `LlmClient::complete` 边界累计真实消耗。
//!
//! ADR-005: docs/adr/ADR-005-legacy-token-accumulation.md
//!
//! **为什么装在边界而不是按回传值累计**：话术两档预算重试（1200→3000）与打分
//! 重试（1600→3200）只把**最后一轮成功**的 tokens 写进回传，失败与空内容降级
//! 一律回传 0。推理型模型思考吃光整档预算时实际消耗最大、回传恰为 0——按回传值
//! 累计会系统性少计。`infra::llm::parse_completion` 对空内容也返回 `Ok` 并带真实
//! usage，所以边界能在 domain 丢弃之前捕获每一轮已解析的用量。
//!
//! **职责边界**：本模块只做「透传 + 计数」，不改打分决策顺序、不改重试与降级
//! 语义、不改 `reply.tokens` 的含义（后者＝本次成功生成的归属消耗，计数器＝计费
//! 口径，两者数值可以不同，是有意设计）。落库走 `ConfigStore` 端口，不碰 SQL。

use super::scoring_llm::{LlmClient, LlmError};
use super::store_ports::ConfigStore;

/// 本次调用属于哪一类消耗，决定计入哪个分项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// 打招呼话术生成 → `tokens_greet`
    Greet,
    /// 岗位打分（含全量重算的逐条调用）→ `tokens_score`
    Score,
}

/// 包一层计数的大模型客户端。结果原样透传，内层行为零改动。
pub struct TokenMeter<'a, S: ConfigStore> {
    inner: &'a dyn LlmClient,
    store: &'a S,
    kind: TokenKind,
    /// 落库失败时的告警出口。domain 不得向上引用命令层，故由调用方注入
    /// （命令层传 `commands_logs::push`）。记计数失败永远不该让业务失败。
    warn: &'a (dyn Fn(&str) + Send + Sync),
}

impl<'a, S: ConfigStore + Sync> TokenMeter<'a, S> {
    pub fn new(
        inner: &'a dyn LlmClient,
        store: &'a S,
        kind: TokenKind,
        warn: &'a (dyn Fn(&str) + Send + Sync),
    ) -> Self {
        Self { inner, store, kind, warn }
    }
}

impl<S: ConfigStore + Sync> LlmClient for TokenMeter<'_, S> {
    fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<(String, String, u32), LlmError> {
        let out = self.inner.complete(system, user, max_tokens);
        // 只在拿到已解析用量时计数：`Err`（Transport / NotConfigured）没有 usage，
        // `t == 0` 是服务端没给 usage——两者都不写库、不建键、不补 0。
        if let Ok((_, _, tokens)) = &out {
            if *tokens > 0 {
                let t = *tokens as i64;
                let (greet, score) = match self.kind {
                    TokenKind::Greet => (t, 0),
                    TokenKind::Score => (0, t),
                };
                if let Err(e) = self.store.bump_tokens(greet, score) {
                    // 计数失败不得让业务失败，也不得静默吞（照 `commands_logs::push`
                    // 的「记日志永远不该让业务失败」先例，出口由命令层注入）。
                    (self.warn)(&format!("词元计数落库失败，本次 {t} tokens 未计入：{e}"));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 假客户端：按预设顺序回话，并记下每次的预算档位。
    struct FakeLlm {
        replies: Mutex<Vec<Result<(String, String, u32), LlmError>>>,
        budgets: Mutex<Vec<u32>>,
    }

    impl FakeLlm {
        fn new(replies: Vec<Result<(String, String, u32), LlmError>>) -> Self {
            Self { replies: Mutex::new(replies), budgets: Mutex::new(Vec::new()) }
        }
    }

    impl LlmClient for FakeLlm {
        fn complete(
            &self,
            _system: &str,
            _user: &str,
            max_tokens: u32,
        ) -> Result<(String, String, u32), LlmError> {
            self.budgets.lock().unwrap().push(max_tokens);
            let mut q = self.replies.lock().unwrap();
            if q.is_empty() {
                return Err(LlmError::Transport("假回复已用尽".to_string()));
            }
            q.remove(0)
        }
    }

    /// 假配置端口：只记录 bump 调用，可注入落库失败。
    #[derive(Default)]
    struct MemConfig {
        bumps: Mutex<Vec<(i64, i64)>>,
        fail: bool,
    }

    impl ConfigStore for MemConfig {
        type Error = String;

        fn get_config(&self, _key: &str, default: &str) -> Result<String, Self::Error> {
            Ok(default.to_string())
        }
        fn set_config(&self, _key: &str, _value: Option<&str>) -> Result<(), Self::Error> {
            Ok(())
        }
        fn get_config_json(
            &self,
            _key: &str,
            default: serde_json::Value,
        ) -> Result<serde_json::Value, Self::Error> {
            Ok(default)
        }
        fn set_config_json(
            &self,
            _key: &str,
            _value: &serde_json::Value,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
        fn bump_tokens(&self, greet: i64, score: i64) -> Result<(), Self::Error> {
            self.bumps.lock().unwrap().push((greet, score));
            if self.fail {
                Err("库写失败".to_string())
            } else {
                Ok(())
            }
        }
    }

    fn ok(text: &str, finish: &str, tokens: u32) -> Result<(String, String, u32), LlmError> {
        Ok((text.to_string(), finish.to_string(), tokens))
    }

    #[test]
    fn ok_with_tokens_bumps_greet_once_and_passes_result_through() {
        let llm = FakeLlm::new(vec![ok("你好，我对这个岗位很感兴趣", "stop", 1200)]);
        let store = MemConfig::default();
        let warns: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let warn = |m: &str| warns.lock().unwrap().push(m.to_string());
        let meter = TokenMeter::new(&llm, &store, TokenKind::Greet, &warn);

        let got = meter.complete("sys", "user", 1200).unwrap();

        // 结果必须原样透传：计数不得改写内容 / finish / tokens
        assert_eq!(got, ("你好，我对这个岗位很感兴趣".to_string(), "stop".to_string(), 1200));
        assert_eq!(*store.bumps.lock().unwrap(), vec![(1200, 0)]);
        assert!(warns.lock().unwrap().is_empty());
    }

    #[test]
    fn zero_usage_and_errors_are_not_counted() {
        let llm = FakeLlm::new(vec![
            ok("x", "stop", 0),
            Err(LlmError::Transport("超时".to_string())),
            Err(LlmError::NotConfigured),
        ]);
        let store = MemConfig::default();
        let warns: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let warn = |m: &str| warns.lock().unwrap().push(m.to_string());
        let meter = TokenMeter::new(&llm, &store, TokenKind::Score, &warn);

        assert_eq!(meter.complete("s", "u", 800).unwrap().2, 0);
        assert!(meter.complete("s", "u", 800).is_err());
        assert!(meter.complete("s", "u", 800).is_err());

        // usage 缺失或压根没拿到响应：不写库、不建键、不补 0
        assert!(store.bumps.lock().unwrap().is_empty());
        assert!(warns.lock().unwrap().is_empty());
    }

    #[test]
    fn score_kind_bumps_score_column_only() {
        let llm = FakeLlm::new(vec![ok(r#"{"score":88,"reason":"匹配"}"#, "stop", 1600)]);
        let store = MemConfig::default();
        let warns: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let warn = |m: &str| warns.lock().unwrap().push(m.to_string());
        let meter = TokenMeter::new(&llm, &store, TokenKind::Score, &warn);

        meter.complete("s", "u", 1600).unwrap();

        assert_eq!(*store.bumps.lock().unwrap(), vec![(0, 1600)]);
    }

    #[test]
    fn retry_rounds_are_all_counted_while_reply_keeps_last_round_only() {
        use crate::core::domain::greeting::{write_greeting, GreetingProfile};
        use crate::core::domain::job::Job;

        // 推理型模型把 1200 档思考吃光：content 空、finish=length，但 usage 真实存在
        let llm = FakeLlm::new(vec![ok("", "length", 1600), ok("你好", "stop", 50)]);
        let store = MemConfig::default();
        let warns: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let warn = |m: &str| warns.lock().unwrap().push(m.to_string());
        let meter = TokenMeter::new(&llm, &store, TokenKind::Greet, &warn);
        let job = Job {
            title: Some("Java 后端".to_string()),
            company: Some("某公司".to_string()),
            jd_text: Some("负责服务端开发".to_string()),
            ..Default::default()
        };
        let profile = GreetingProfile {
            years: 5,
            title: "后端工程师".to_string(),
            skills: Vec::new(),
            core_skills: vec!["Java".to_string()],
            bonus_skills: Vec::new(),
        };

        let g = write_greeting(Some(&meter), None, &job, &profile);

        // 回传语义零改动：只归属本次成功生成的那一轮
        assert_eq!(g.tokens, 50);
        assert_eq!(g.source, "llm");
        // 计费口径：两档重试的每一轮都计入
        assert_eq!(*store.bumps.lock().unwrap(), vec![(1600, 0), (50, 0)]);
        assert_eq!(*llm.budgets.lock().unwrap(), vec![1200, 3000]);
    }

    #[test]
    fn bump_failure_warns_once_and_never_fails_the_call() {
        let llm = FakeLlm::new(vec![ok("话术正文", "stop", 900)]);
        let store = MemConfig { fail: true, ..Default::default() };
        let warns: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let warn = |m: &str| warns.lock().unwrap().push(m.to_string());
        let meter = TokenMeter::new(&llm, &store, TokenKind::Greet, &warn);

        let got = meter.complete("s", "u", 1200).unwrap();

        // 计数失败不得冒泡进业务结果
        assert_eq!(got, ("话术正文".to_string(), "stop".to_string(), 900));
        let w = warns.lock().unwrap();
        assert_eq!(w.len(), 1, "必须告警恰一次，不得静默吞");
        assert!(w[0].contains("900"), "告警要带上未计入的用量：{}", w[0]);
    }
}