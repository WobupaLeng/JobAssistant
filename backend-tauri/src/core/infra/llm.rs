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

/// OpenAI 兼容 HTTP 客户端。
pub struct HttpLlmClient {
    api_key: String,
    base_url: String,
    model: String,
    timeout: Duration,
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
        }
    }

    /// 覆盖超时时间（主要给测试和特殊网络环境用）。
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
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
    fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<(String, String, u32), LlmError> {
        if self.api_key.trim().is_empty() {
            return Err(LlmError::NotConfigured);
        }

        // temperature 0.2 与 Python 侧一致：需要稳定的评分，不要发散
        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0.2,
            "max_tokens": max_tokens,
        });

        let body = ureq::post(&self.endpoint())
            .timeout(self.timeout)
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_string(&payload.to_string())
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
    }
}
