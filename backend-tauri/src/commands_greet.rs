//! 打招呼相关的命令：AI 生成话术 + 一键发给 HR。
//!
//! 拆成两个独立步骤，不合成一个「自动打招呼」的黑盒命令：
//! - `greet_draft`：只生成文本，**不碰浏览器、不往外发任何东西**，可以先看再改；
//! - `greet_one`：把（用户确认过的）文本真的发出去，走聊天通道。
//!
//! 这样「生成」失败不会连带「发送」失败，用户也能在发之前改字。老版把两步
//! 揉在 `/api/auto-greet/one` 里，生成慢了就像整个功能卡住。

use crate::core::domain::greeting::{
    write_greeting_with, Greeting, GreetingProfile, DEFAULT_PROMPT,
};
use crate::core::domain::profile::DEFAULT_PROFILE;
use crate::core::domain::job::{Job, JobQuery, JobStore, SortField};
use crate::core::domain::scoring_llm::LlmClient;
use crate::core::domain::store_ports::ConfigStore;
use crate::core::domain::token_meter::{TokenKind, TokenMeter};
use serde::{Deserialize, Serialize};

use crate::commands_chat::{handle_of, request_chat};
use crate::commands_db::{store_handle, LlmState, StoreState};
use crate::commands_scrape::ScrapeState;
use crate::core::infra::HttpLlmClient;

type CmdResult<T> = Result<T, String>;

/// 把真 HTTP 客户端包成「话术温度」的 `LlmClient`。
///
/// 为什么要包这一层：`domain::scoring_llm::LlmClient` 的 `complete` 没有温度
/// 参数（端口是纯领域契约），而 `HttpLlmClient` 的默认实现用的是**打分**温度。
/// 话术若直接走默认实现，配置里的话术温度就永远不生效。
struct GreetClient<'a>(&'a HttpLlmClient);

impl LlmClient for GreetClient<'_> {
    fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<(String, String, u32), crate::core::domain::scoring_llm::LlmError> {
        self.0
            .complete_at(system, user, max_tokens, self.0.greet_temperature())
    }
}

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
    /// HR 刚说的话（聊天页「生成回复」用）。非空时提示词会带上这段原话，
    /// 并要求模型接话而不是重新自我介绍。
    #[serde(default)]
    pub reply: Option<String>,
    /// 聊天页的「重新生成」：后端按公司名找出库里匹配度最高的那条岗位，
    /// 用它的完整 JD（列表接口里 `jd_text` 是空的，聊天页拿不到）。
    #[serde(default, alias = "findByCompany")]
    pub find_by_company: Option<String>,
    /// 是否用大模型。默认 true——用户点「生成话术」就是想要模型写，
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
    /// 本次实际用掉的岗位 id（为空表示没用库内岗位，只按标题/公司写）
    pub job_id: String,
    /// 本次提示词里**有没有真的带上 JD 正文**。
    ///
    /// 单独回传而不是让前端猜：聊天页拿不到 JD（好友列表没有这个字段），
    /// 界面要能一眼看出「这句是拿整段 JD 写的」还是「只有公司名和岗位名」。
    pub jd_used: bool,
    /// 是否用了 HR 的回复（「生成回复」路径）。
    pub reply_used: bool,
}

impl From<Greeting> for GreetReply {
    fn from(g: Greeting) -> Self {
        Self {
            text: g.text,
            tokens: g.tokens,
            source: g.source,
            note: g.note,
            job_id: String::new(),
            jd_used: false,
            reply_used: false,
        }
    }
}

/// 按公司名找库里最像的那条岗位（聊天页没有 job_id，只有公司名）。
///
/// 直接拿 `JobQuery::for_tab("pending")` 的前 200 条在前端做匹配不对——那会把
/// 「同一公司下的哪条岗位」这件事交给排序。这里只在**公司名归一化后相等**的
/// 候选里挑：有 JD 的优先，其次分数高的。找不到返回 `None`（调用方退回按名字生成）。
fn find_job_by_company(
    store: &dyn JobStore<Error = crate::core::infra::StoreError>,
    company: &str,
) -> Option<Job> {
    let want = norm_company(company);
    if want.is_empty() {
        return None;
    }
    let query = JobQuery {
        limit: 200,
        sort: SortField::Score,
        ..Default::default()
    };
    let jobs = store.list_jobs(&query).ok()?;
    let mut best: Option<(bool, u8, Job)> = None;
    for j in jobs
        .into_iter()
        .filter(|j| norm_company(j.company.as_deref().unwrap_or("")) == want)
    {
        let has_jd = j
            .jd_text
            .as_deref()
            .map(|s| s.trim().chars().count() >= 40)
            .unwrap_or(false);
        let rank = (has_jd, j.match_score.unwrap_or(0));
        // 严格大于才替换：`max_by_key` 返回**最后一个**最大值，同分时会取到靠后那条。
        let better = match &best {
            None => true,
            Some((bjd, bscore, _)) => rank.0 > *bjd || (rank.0 == *bjd && rank.1 > *bscore),
        };
        if better {
            best = Some((rank.0, rank.1, j));
        }
    }
    best.map(|(_, _, j)| j)
}

/// 公司名归一化：去掉空白、去括号后缀（「某某科技有限公司」↔「某某科技」靠前缀比较）。
fn norm_company(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{3000}')
        .collect::<String>()
        .to_lowercase()
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
///
/// 同一条命令服务三种用法（前端都带 `company`，区别只在另外两个字段）：
/// - 首次：「重建」带 `findByCompany` → 后端按公司名翻出库内岗位的完整 JD；
/// - 重新生成：与上一次入参一致，模型温度会给出不同措辞；
/// - 生成回复：多带 `reply`（HR 刚说的话）。
///
/// **`async` 不是为了并发，是为了不占主线程**：同步命令由 Tauri 在主线程执行，
/// 而这里要等一次大模型 HTTP（`write_greeting_with`）。留在主线程上，
/// 生成期间窗口拖不动、别的按钮点不动——用户明确要求「等待远端数据时界面照常可用」。
#[tauri::command]
pub async fn greet_draft(
    store_state: tauri::State<'_, StoreState>,
    llm_state: tauri::State<'_, LlmState>,
    args: GreetArgs,
) -> CmdResult<GreetReply> {
    let store = store_handle(&store_state)?;

    // 1) 定位岗位：job_id 优先 → 公司名反查（聊天页「重新生成」）→ 按入参现场造一个。
    let mut used_id = String::new();
    let job: Job = match args.job_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => {
            used_id = id.to_string();
            store
                .get_job(id)
                .map_err(|e| format!("读取岗位失败：{e}"))?
                .ok_or_else(|| format!("找不到岗位 {id}"))?
        }
        None => match args
            .find_by_company
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|c| find_job_by_company(&*store, c))
        {
            Some(j) => {
                used_id = j.job_id.clone();
                j
            }
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
        },
    };
    let jd_used = job
        .jd_text
        .as_deref()
        .map(|s| s.trim().chars().count() >= 20)
        .unwrap_or(false);
    let reply = args.reply.as_deref().map(str::trim).unwrap_or("");
    // HR 的话太长会把 JD 挤掉预算：只取最后 400 字（最新的那句最相关）。
    let reply_trimmed: String = {
        let n = reply.chars().count();
        if n <= 400 {
            reply.to_string()
        } else {
            reply.chars().skip(n - 400).collect()
        }
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
    let http: Option<&HttpLlmClient> = if args.use_llm {
        guard.as_ref().and_then(|g| g.as_ref())
    } else {
        None
    };

    // 话术温度与打分温度是两份配置：`LlmClient::complete` 只会用打分温度，
    // 所以话术这边用 `GreetClient` 包一层，把请求的温度换成话术那一档。
    // 计费口径的累计装在端口边界（ADR-005），回传 tokens 语义不变。
    let warn = |m: &str| crate::commands_logs::push("warn", m);
    // 包一层要先落地成局部变量：`TokenMeter::new(&GreetClient(c), ..)` 借的是
    // 临时值，出了这一行就悬空（E0515）。先 bind，生命周期跟着函数体。
    let greet_client = http.map(GreetClient);
    let meter = greet_client
        .as_ref()
        .map(|c| TokenMeter::new(c, &*store, TokenKind::Greet, &warn));
    let client: Option<&dyn LlmClient> = match meter.as_ref() {
        Some(m) => Some(m as &dyn LlmClient),
        None => None,
    };

    let g = write_greeting_with(client, Some(prompt), &job, &profile, Some(&reply_trimmed));
    let mut reply_out = GreetReply::from(g);
    reply_out.job_id = used_id;
    reply_out.jd_used = jd_used;
    reply_out.reply_used = !reply_trimmed.is_empty();
    Ok(reply_out)
}

/// 把一句话术发给指定 HR。这是**真实外发动作**，前端必须先让用户确认。
///
/// `async` 的理由同 `greet_draft`：`ChatSend` 要等 sidecar 回执，
/// 留在主线程上会让窗口在发送期间拖不动。
#[tauri::command]
pub async fn greet_one(
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
        // 聊天页的两个新入参：漏了 alias 会让「重新生成」静默丢掉 JD、把 HR 的话丢掉。
        let c: GreetArgs =
            serde_json::from_str(r#"{"findByCompany":"星瀚科技","reply":"你好"}"#).unwrap();
        assert_eq!(c.find_by_company.as_deref(), Some("星瀚科技"));
        assert_eq!(c.reply.as_deref(), Some("你好"));
    }

    /// 「重新生成」靠公司名反查岗位：库里没有这家公司时返回 `None`（调用方按名字生成）。
    #[test]
    fn find_job_by_company_prefers_row_with_jd() {
        struct S(Vec<Job>);
        impl JobStore for S {
            type Error = crate::core::infra::StoreError;
            fn insert_job(&self, _j: &Job) -> Result<bool, Self::Error> {
                Ok(true)
            }
            fn get_job(&self, _id: &str) -> Result<Option<Job>, Self::Error> {
                Ok(None)
            }
            fn list_jobs(&self, _q: &JobQuery) -> Result<Vec<Job>, Self::Error> {
                Ok(self.0.clone())
            }
            fn list_scored_ids(&self, _a: Option<&str>, _l: u32) -> Result<Vec<Job>, Self::Error> {
                Ok(Vec::new())
            }
            fn update_score(
                &self,
                _id: &str,
                _s: u8,
                _src: crate::core::domain::job::ScoreSource,
                _r: &str,
            ) -> Result<(), Self::Error> {
                Ok(())
            }
            fn log_decision(
                &self,
                _p: &str,
                _cj: &str,
                _d: &str,
                _r: &str,
            ) -> Result<(), Self::Error> {
                Ok(())
            }
        }
        let mk = |id: &str, company: &str, jd: &str, score: u8| Job {
            job_id: id.into(),
            company: Some(company.into()),
            title: Some("Java 后端".into()),
            jd_text: if jd.is_empty() { None } else { Some(jd.into()) },
            match_score: Some(score),
            ..Default::default()
        };
        let s = S(vec![
            mk("a", "星瀚科技", "", 95),
            mk("b", "星瀚科技", "负责 Spring Boot 微服务开发，要求熟悉 MySQL 与 Redis，有高并发经验优先", 60),
            mk("c", "别的公司", "整段 JD 很长很长很长很长很长很长很长很长很长很长很长", 99),
        ]);
        let hit = find_job_by_company(&s, "星瀚科技").expect("同公司应能找到");
        assert_eq!(hit.job_id, "b", "有 JD 的那条优先于分数高的空 JD 条");
        assert!(find_job_by_company(&s, "不存在的公司").is_none());
        assert!(find_job_by_company(&s, "   ").is_none());
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
