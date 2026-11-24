//! 大模型 HTTP 客户端（OpenAI 兼容接口）。
//!
//! 实现 `domain::scoring_llm::LlmClient` 端口。支持 DeepSeek / OpenAI / 通义千问 /
//! 本地 Ollama 等任何兼容 `/chat/completions` 的服务——用户只需改 `base_url` + `model`。
//!
//! 为什么不用 `reqwest`：本项目只需要发一次 POST，用 `ureq` 可以少引入
//! tokio/hyper 这一整棵异步运行时依赖树，对「单个 exe 体积」这个硬指标更友好。

use std::time::Duration;

use crate::core::domain::scoring_llm::{LlmClient, LlmError};

/// 默认请求超时（Python 侧 `timeout=60.0`）。
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// 默认模型名（与 `app_config.model` 的实际取值保持一致）。
pub const DEFAULT_MODEL: &str = "deepseek-chat";

/// 打分温度：需要稳定的分值，同一条 JD 不该每次跑出不同的分。
pub const SCORE_TEMPERATURE: f32 = 0.2;

/// 话术温度：招呼语需要一点变化，0.2 会写出千篇一律的模板句。
pub const GREET_TEMPERATURE: f32 = 0.8;

/// 允许配置的温度区间（含两端）。
pub const TEMPERATURE_MIN: f32 = 0.0;
pub const TEMPERATURE_MAX: f32 = 2.0;

/// 把用户填的温度夹进合法区间并收到两位小数。
///
/// 非有限值（NaN / ±∞）一律夹到边界：`NaN` **不能**原样放过——它序列化后是
/// `null`，服务端会当成「没传温度」，静默改变行为比报错更难查。
pub fn clamp_temperature(v: f32, dflt: f32) -> f32 {
    let finite = if v.is_nan() { dflt } else { v };
    let c = finite.clamp(TEMPERATURE_MIN, TEMPERATURE_MAX);
    (c * 100.0).round() / 100.0
}

/// 解析配置里的温度文本：空串 / 非数字 / 越界都落到一个确定值。
pub fn resolve_temperature(raw: Option<&str>, dflt: f32) -> f32 {
    raw.and_then(|s| s.trim().parse::<f32>().ok())
        .map(|v| clamp_temperature(v, dflt))
        .unwrap_or(dflt)
}

/// 把温度写成 JSON 数值（两位小数）。
///
/// 直接用 `f32` 拼进 `json!` 会得到 `0.800000011920929`——数值上没错，但用户
/// 在抓包/日志里看到的是另一回事，也让按值比较的测试全都得用近似。
pub fn temperature_json(v: f32) -> serde_json::Number {
    // 两位小数在 f64 里先算：((0.8_f32 * 100).round() / 100) as f64 会带出
    // f32 的二进制尾巴（0.8000000119…），而 (80.0_f64 / 100.0) 恰好是 0.8。
    let two = (v as f64 * 100.0).round() / 100.0;
    serde_json::Number::from_f64(two).unwrap_or_else(|| {
        // NaN / ±∞ 才会走到这里：用 `clamp_temperature` 落成确定值再拼
        let c = clamp_temperature(v, GREET_TEMPERATURE) as f64;
        serde_json::Number::from_f64(c).unwrap_or_else(|| serde_json::Number::from(0))
    })
}

/// OpenAI 兼容 HTTP 客户端。
pub struct HttpLlmClient {
    api_key: String,
    base_url: String,
    model: String,
    timeout: Duration,
    /// 打分与话术各一份温度：同一套参数喂两种任务是话术模板化的根因之一。
    score_temperature: f32,
    greet_temperature: f32,
}

impl HttpLlmClient {
    /// 构造客户端。`base_url` 为空时用 OpenAI 官方地址。
    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<impl Into<String>>,
        model: impl Into<String>,
    ) -> Self {
        let base = base_url
            .map(Into::into)
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        Self {
            api_key: api_key.into(),
            base_url: base.trim_end_matches('/').to_string(),
            model: model.into(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            score_temperature: SCORE_TEMPERATURE,
            greet_temperature: GREET_TEMPERATURE,
        }
    }

    /// 覆盖超时时间（主要给测试和特殊网络环境用）。
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// 用配置里的原始文本装填两份温度（`None` / 空串 / 非数字 → 默认值）。
    pub fn with_temperature_text(mut self, score: Option<&str>, greet: Option<&str>) -> Self {
        self.score_temperature = resolve_temperature(score, SCORE_TEMPERATURE);
        self.greet_temperature = resolve_temperature(greet, GREET_TEMPERATURE);
        self
    }

    /// 打分温度（已夹进合法区间）。
    pub fn score_temperature(&self) -> f32 {
        self.score_temperature
    }

    /// 话术温度（已夹进合法区间）。
    pub fn greet_temperature(&self) -> f32 {
        self.greet_temperature
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    /// 组装请求体。抽出来是为了不发网络请求也能断言温度真的进了 body。
    pub fn payload(&self, system: &str, user: &str, max_tokens: u32, temperature: f32) -> String {
        serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": temperature_json(temperature),
            "max_tokens": max_tokens,
        })
        .to_string()
    }
}

/// 从 OpenAI 兼容响应里抽出 `(content, finish_reason, total_tokens)`。
///
/// 抽成独立函数是为了能用固定 JSON 样本单测，不必真发网络请求。
pub fn parse_completion(body: &str) -> Result<(String, String, u32), LlmError> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| LlmError::Transport(format!("响应不是合法 JSON: {}", e)))?;

    // 服务端错误通常放在 error 字段里
    if let Some(err) = v.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        return Err(LlmError::Transport(format!("服务端返回错误: {}", msg)));
    }

    let choice = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .ok_or_else(|| LlmError::Transport("响应缺少 choices".to_string()))?;

    let content = choice
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let finish = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .unwrap_or("")
        .to_string();

    let tokens = v
        .get("usage")
        .and_then(|u| u.get("total_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;

    Ok((content, finish, tokens))
}

impl LlmClient for HttpLlmClient {
    /// 打分路径按打分温度发请求。
    ///
    /// 话术路径不直接走这里：`domain` 的端口签名里没有「用途」，温度由
    /// `commands_greet` 显式传 `with_temperature(GREET_TEMPERATURE)` 的副本调用
    /// [`HttpLlmClient::complete_at`]。这样端口契约一行都不用改。
    fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<(String, String, u32), LlmError> {
        self.complete_at(system, user, max_tokens, self.score_temperature)
    }
}

impl HttpLlmClient {
    /// 指定温度打一次请求（话术路径用）。
    pub fn complete_at(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
        temperature: f32,
    ) -> Result<(String, String, u32), LlmError> {
        if self.api_key.trim().is_empty() {
            return Err(LlmError::NotConfigured);
        }
        let payload = self.payload(system, user, max_tokens, temperature);
        let body = ureq::post(&self.endpoint())
            .timeout(self.timeout)
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_string(&payload)
            .map_err(|e| LlmError::Transport(describe_ureq_error(e)))?
            .into_string()
            .map_err(|e| LlmError::Transport(format!("读取响应失败: {}", e)))?;

        parse_completion(&body)
    }
}

/// 把 `ureq::Error` 转成一句人话。
///
/// 直接 `to_string()` 只能得到 "status code 401" 这类信息，
/// 用户看不出是 API Key 错了还是余额不足，所以要带上响应体片段。
fn describe_ureq_error(e: ureq::Error) -> String {
    // 按值匹配：Response 不是 Copy，无法从 &Error 里把它移出来。
    match e {
        ureq::Error::Status(code, resp) => {
            // ureq 2.x 的 Response 需先 into_reader()；take() 限流避免恶意响应打爆内存。
            use std::io::Read;
            let mut buf = Vec::new();
            let _ = resp.into_reader().take(4096).read_to_end(&mut buf);
            let snippet = String::from_utf8_lossy(&buf)
                .chars()
                .take(300)
                .collect::<String>();
            format!("HTTP {}：{}", code, snippet.trim())
        }
        ureq::Error::Transport(t) => format!("网络错误：{}", t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_normal_response() {
        let body = r#"{
            "choices": [{"message": {"content": "{\"score\": 88, \"reason\": \"匹配\"}"}, "finish_reason": "stop"}],
            "usage": {"total_tokens": 456}
        }"#;
        let (content, finish, tokens) = parse_completion(body).unwrap();
        assert!(content.contains("\"score\": 88"));
        assert_eq!(finish, "stop");
        assert_eq!(tokens, 456);
    }

    #[test]
    fn parse_length_truncated_response() {
        let body = r#"{
            "choices": [{"message": {"content": ""}, "finish_reason": "length"}],
            "usage": {"total_tokens": 1600}
        }"#;
        let (content, finish, tokens) = parse_completion(body).unwrap();
        assert!(content.is_empty());
        assert_eq!(finish, "length");
        assert_eq!(tokens, 1600);
    }

    #[test]
    fn parse_missing_usage_defaults_to_zero() {
        let body = r#"{"choices":[{"message":{"content":"x"},"finish_reason":"stop"}]}"#;
        assert_eq!(parse_completion(body).unwrap().2, 0);
    }

    #[test]
    fn parse_server_error_payload() {
        let body = r#"{"error": {"message": "Incorrect API key provided"}}"#;
        let err = parse_completion(body).unwrap_err();
        match err {
            LlmError::Transport(m) => assert!(m.contains("Incorrect API key")),
            other => panic!("应为 Transport，实际 {:?}", other),
        }
    }

    #[test]
    fn parse_rejects_non_json() {
        assert!(parse_completion("<html>502 Bad Gateway</html>").is_err());
    }

    #[test]
    fn parse_rejects_missing_choices() {
        assert!(parse_completion(r#"{"id": "x"}"#).is_err());
    }

    #[test]
    fn endpoint_joins_without_double_slash() {
        let c = HttpLlmClient::new("k", Some("https://api.deepseek.com/v1/"), "deepseek-chat");
        assert_eq!(c.endpoint(), "https://api.deepseek.com/v1/chat/completions");
    }

    #[test]
    fn endpoint_defaults_to_openai() {
        let c = HttpLlmClient::new("k", None::<String>, "gpt-4o-mini");
        assert_eq!(c.endpoint(), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn empty_api_key_is_not_configured() {
        let c = HttpLlmClient::new("", None::<String>, "m");
        assert_eq!(
            c.complete("s", "u", 100).unwrap_err(),
            LlmError::NotConfigured
        );
        assert_eq!(
            c.complete_at("s", "u", 100, GREET_TEMPERATURE).unwrap_err(),
            LlmError::NotConfigured
        );
    }

    #[test]
    fn score_and_greet_temperatures_are_independent() {
        let c = HttpLlmClient::new("k", None::<String>, "m");
        assert_eq!(c.score_temperature(), SCORE_TEMPERATURE);
        assert_eq!(c.greet_temperature(), GREET_TEMPERATURE);
        assert!(
            c.greet_temperature() > c.score_temperature(),
            "话术要比打分更能发散，否则招呼语千篇一律"
        );
    }

    #[test]
    fn temperature_text_is_clamped_and_rounded() {
        let c = HttpLlmClient::new("k", None::<String>, "m")
            .with_temperature_text(Some("1.5"), Some("9"));
        assert_eq!(c.score_temperature(), 1.5);
        assert_eq!(c.greet_temperature(), TEMPERATURE_MAX, "越界必须夹住，不能发给服务端换 400");

        let c = HttpLlmClient::new("k", None::<String>, "m")
            .with_temperature_text(Some("-3"), Some("0.123"));
        assert_eq!(c.score_temperature(), TEMPERATURE_MIN);
        assert_eq!(c.greet_temperature(), 0.12, "收到两位小数，diff 才干净");
    }

    #[test]
    fn blank_or_garbage_temperature_falls_back_to_default() {
        let c = HttpLlmClient::new("k", None::<String>, "m")
            .with_temperature_text(Some("  "), Some("abc"));
        assert_eq!(c.score_temperature(), SCORE_TEMPERATURE);
        assert_eq!(c.greet_temperature(), GREET_TEMPERATURE);

        let c = HttpLlmClient::new("k", None::<String>, "m").with_temperature_text(None, None);
        assert_eq!(c.score_temperature(), SCORE_TEMPERATURE);
        assert_eq!(c.greet_temperature(), GREET_TEMPERATURE);
    }

    #[test]
    fn nan_temperature_falls_back_instead_of_becoming_null() {
        // NaN 序列化成 null 会被服务端当成「没传温度」，静默改变行为
        assert_eq!(clamp_temperature(f32::NAN, 0.8), 0.8);
        assert_eq!(clamp_temperature(f32::INFINITY, 0.2), TEMPERATURE_MAX);
        assert_eq!(clamp_temperature(f32::NEG_INFINITY, 0.2), TEMPERATURE_MIN);
        // NaN 也必须是合法的 JSON 数字（夹到区间内），不能变成 null
        let nan = temperature_json(f32::NAN);
        let n = nan.as_f64().expect("必须是数字");
        assert!(n >= TEMPERATURE_MIN as f64 && n <= TEMPERATURE_MAX as f64, "{n}");
    }

    #[test]
    fn payload_carries_the_given_temperature() {
        let c = HttpLlmClient::new("k", None::<String>, "deepseek-chat");
        let body: serde_json::Value =
            serde_json::from_str(&c.payload("sys", "user", 1200, 0.8)).unwrap();
        // 0.8 必须原样写成 0.8，不能写成 0.800000011920929
        assert_eq!(body["temperature"], 0.8);
        assert_eq!(body["max_tokens"], 1200);
        assert_eq!(body["model"], "deepseek-chat");

        let greet: serde_json::Value =
            serde_json::from_str(&c.payload("sys", "user", 1200, GREET_TEMPERATURE)).unwrap();
        assert_eq!(greet["temperature"], 0.8);

        // 打分那一档也得真的进 body（默认实现走的是打分温度）
        let score: serde_json::Value =
            serde_json::from_str(&c.payload("sys", "user", 1600, SCORE_TEMPERATURE)).unwrap();
        assert_eq!(score["temperature"], 0.2);
    }
}
