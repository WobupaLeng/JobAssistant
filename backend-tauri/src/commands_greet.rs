//! 打招呼相关的命令：AI 生成话术 + 一键发给 HR。
//!
//! 拆成两个独立步骤，不合成一个「自动打招呼」的黑盒命令：
//! - `greet_draft`：只生成文本，**不碰浏览器、不往外发任何东西**，可以先看再改；
//! - `greet_one`：把（用户确认过的）文本真的发出去，走聊天通道。
//!
//! 这样「生成」失败不会连带「发送」失败，用户也能在发之前改字。老版把两步
//! 揉在 `/api/auto-greet/one` 里，生成慢了就像整个功能卡住。

use crate::core::domain::greeting::{
    write_greeting, Greeting, GreetingProfile, DEFAULT_PROMPT,
};
use crate::core::domain::profile::DEFAULT_PROFILE;
use crate::core::domain::job::{Job, JobStore};
use crate::core::domain::scoring_llm::LlmClient;
use crate::core::domain::store_ports::ConfigStore;
use crate::core::domain::token_meter::{TokenKind, TokenMeter};
use serde::{Deserialize, Serialize};

use crate::commands_chat::{handle_of, request_chat};
use crate::commands_db::{store_handle, LlmState, StoreState};
use crate::commands_scrape::ScrapeState;

type CmdResult<T> = Result<T, String>;

/// 目标岗位定位：`job_id` 优先，没有就按 JD 现场生成（试算用）。
#[derive(Debug, Deserialize)]
pub struct GreetArgs {
    /// 已入库岗位的 id；为空则用 `title/company/jd` 临时造一个 Job
    #[serde(default, alias = "jobId")]
    pub job_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub salary: Option<String>,
    #[serde(default)]
    pub experience: Option<String>,
    #[serde(default)]
    pub jd: Option<String>,
    /// 是否用大模型。默认 true——用户点「生成话术」就是想要模型写，
    /// 没配 key 时 `write_greeting` 会自动降级到规则话术并给出原因。
    #[serde(default = "default_true", alias = "useLlm")]
    pub use_llm: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct GreetReply {
    pub text: String,
    pub tokens: u32,
    /// `"llm"` | `"rule"`
    pub source: String,
    /// 降级原因；为空表示没降级
    pub note: String,
}

impl From<Greeting> for GreetReply {
    fn from(g: Greeting) -> Self {
        Self {
            text: g.text,
            tokens: g.tokens,
            source: g.source,
            note: g.note,
        }
    }
}

/// 把配置里的技能文本切成技能数组。
///
/// 用户在配置里可以写「Java, Spring Boot、MySQL」任意分隔符混着来，
/// 老版 `profile_with_skills` 用的正是 `[,，、;；/\n]`。照抄，不另发明一套。
pub(crate) fn split_skills(raw: &str) -> Vec<String> {
    raw.split(|c: char| matches!(c, ',' | '，' | '、' | ';' | '；' | '/' | '\n'))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 组装「我是谁」。
///
/// - 年限与方向取自 `DEFAULT_PROFILE`（用户没有单独改年限的入口）；
/// - 技能以配置 `skills` 为准，留空才退回画像自带的核心/加分技能。
pub(crate) fn profile_from_store(
    store: &dyn ConfigStore<Error = crate::core::infra::StoreError>,
) -> GreetingProfile {
    let skills_raw = store.get_config("skills", "").unwrap_or_default();
    let skills = split_skills(&skills_raw);
    GreetingProfile {
        years: DEFAULT_PROFILE.years,
        title: DEFAULT_PROFILE.title.to_string(),
        skills: skills.clone(),
        core_skills: if skills.is_empty() {
            DEFAULT_PROFILE
                .core_skills
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        },
        bonus_skills: if skills.is_empty() {
            DEFAULT_PROFILE
                .bonus_skills
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// 生成一句话术。**不发送**，纯预览。
#[tauri::command]
pub fn greet_draft(
    store_state: tauri::State<'_, StoreState>,
    llm_state: tauri::State<'_, LlmState>,
    args: GreetArgs,
) -> CmdResult<GreetReply> {
    let store = store_handle(&store_state)?;

    // 1) 定位岗位
    let job: Job = match args.job_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => store
            .get_job(id)
            .map_err(|e| format!("读取岗位失败：{e}"))?
            .ok_or_else(|| format!("找不到岗位 {id}"))?,
        None => Job {
            job_id: String::new(),
            platform: String::new(),
            title: args.title.clone(),
            company: args.company.clone(),
            salary: args.salary.clone(),
            status: "pending".to_string(),
            created_at: 0,
            jd_text: args.jd.clone(),
            experience: args.experience.clone(),
            rejected: 0,
            ..Default::default()
        },
    };

    // 2) 提示词与画像都来自配置
    let prompt_raw = store.get_config("greet_prompt", "").unwrap_or_default();
    let prompt = if prompt_raw.trim().is_empty() {
        DEFAULT_PROMPT
    } else {
        prompt_raw.as_str()
    };
    let profile = profile_from_store(&*store);

    // 3) 大模型可选。照抄 `score_job` 的写法：`guard` 是
    //    `LockResult<MutexGuard<Option<HttpLlmClient>>>`，所以 `as_ref` 要两次。
    let guard = llm_state.0.lock().ok();
    let bare: Option<&dyn LlmClient> = if args.use_llm {
        guard
            .as_ref()
            .and_then(|g| g.as_ref())
            .map(|c| c as &dyn LlmClient)
    } else {
        None
    };
    // 计费口径的累计装在客户端边界（ADR-005），回传 tokens 语义不变。
    // 上面 `store_handle` 已保证库已初始化；计数落库失败只 warn，不失败业务。
    let warn = |m: &str| crate::commands_logs::push("warn", m);
    let meter = bare.map(|c| TokenMeter::new(c, &*store, TokenKind::Greet, &warn));
    let client: Option<&dyn LlmClient> = match meter.as_ref() {
        Some(m) => Some(m as &dyn LlmClient),
        None => bare,
    };

    let g = write_greeting(client, Some(prompt), &job, &profile);
    Ok(g.into())
}

/// 把一句话术发给指定 HR。这是**真实外发动作**，前端必须先让用户确认。
#[tauri::command]
pub fn greet_one(
    scrape: tauri::State<'_, ScrapeState>,
    uid: String,
    text: String,
) -> CmdResult<String> {
    let uid = uid.trim().to_string();
    if uid.is_empty() {
        return Err("缺少 uid：请先在 Boss 消息里选择一位 HR".to_string());
    }
    if text.trim().is_empty() {
        return Err("话术为空，没有可发送的内容".to_string());
    }
    let handle = handle_of(&scrape)?;
    let data = request_chat(
        &handle,
        &crate::core::sidecar::protocol::Command::ChatSend { uid, text },
    )?;
    Ok(data
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用户在配置里会混用逗号、顿号、斜杠和换行，全都要能切开。
    #[test]
    fn split_skills_handles_mixed_separators() {
        let got = split_skills("Java, Spring Boot、MySQL;Redis/RabbitMQ\nDocker");
        assert_eq!(
            got,
            vec!["Java", "Spring Boot", "MySQL", "Redis", "RabbitMQ", "Docker"]
        );
    }

    #[test]
    fn split_skills_drops_blank_entries() {
        assert!(split_skills("  ,  、 \n ").is_empty());
    }

    /// 前端发驼峰（`jobId` / `useLlm`）：serde 对未知字段不报错，
    /// 漏了 alias 会让「按库内岗位生成话术」静默退化成临时 JD（AGENTS.md 2.2）。
    #[test]
    fn greet_args_accept_camel_case() {
        let a: GreetArgs =
            serde_json::from_str(r#"{"jobId":"boss:1","useLlm":false}"#).expect("驼峰应能反序列化");
        assert_eq!(a.job_id.as_deref(), Some("boss:1"));
        assert!(!a.use_llm);
        let snake: GreetArgs =
            serde_json::from_str(r#"{"job_id":"boss:2","use_llm":false}"#).unwrap();
        assert_eq!(snake.job_id.as_deref(), Some("boss:2"));
        assert!(!snake.use_llm);
        let d: GreetArgs = serde_json::from_str("{}").unwrap();
        assert!(d.use_llm, "缺省应走大模型");
        assert!(d.job_id.is_none());
    }

    /// 没配 `skills` 时要退回画像自带技能，否则话术会退化成「后端与全栈」这种通用词。
    #[test]
    fn empty_config_falls_back_to_default_profile_skills() {
        struct S;
        impl ConfigStore for S {
            type Error = crate::core::infra::StoreError;
            fn get_config(&self, _k: &str, d: &str) -> Result<String, Self::Error> {
                Ok(d.to_string())
            }
            fn set_config(&self, _k: &str, _v: Option<&str>) -> Result<(), Self::Error> {
                Ok(())
            }
            fn get_config_json(
                &self,
                _k: &str,
                d: serde_json::Value,
            ) -> Result<serde_json::Value, Self::Error> {
                Ok(d)
            }
            fn set_config_json(
                &self,
                _k: &str,
                _v: &serde_json::Value,
            ) -> Result<(), Self::Error> {
                Ok(())
            }
            fn bump_tokens(&self, _greet: i64, _score: i64) -> Result<(), Self::Error> {
                Ok(())
            }
        }
        let p = profile_from_store(&S);
        assert!(!p.all_skills().is_empty(), "配置为空时必须退回画像技能");
    }
}
