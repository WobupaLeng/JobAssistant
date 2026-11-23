//! JSON Lines 协议的消息定义。

use serde::{Deserialize, Serialize};

/// 单行消息上限。超过这个长度的 JD 文本必须截断——否则一行几 MB 的 JSON
/// 会让对端读取阻塞，且一旦写失败就整条消息丢失。
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// 错误信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorInfo {
    pub code: String,
    pub msg: String,
}

/// 一条信封：命令、回执、事件三种形态共用。
///
/// 用 `Option` 而非枚举，是为了让 `serde` 的线上格式与 SPEC-002 第 2.1 节
/// 手写的 JSON 示例逐字对应，前端/Python 两侧都能直接照文档实现。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// 命令/回执的关联 ID；事件为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 事件名；命令与回执为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

impl Envelope {
    /// 构造回执。
    pub fn reply(id: impl Into<String>, ok: bool) -> Self {
        Self {
            id: Some(id.into()),
            event: None,
            ok: Some(ok),
            data: None,
            error: None,
        }
    }

    /// 构造事件。
    pub fn event(name: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            id: None,
            event: Some(name.into()),
            ok: None,
            data: Some(data),
            error: None,
        }
    }

    /// 序列化为一行（不含换行符）。
    pub fn to_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Rust → sidecar 的命令。
///
/// **聊天类命令（`chat.*`）是请求/应答**：sidecar 执行完会用同一个 `id`
/// 回一条 `EncodedReply`。抓取类命令是单向的（进度走 event 流）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Command {
    #[serde(rename = "scrape.start")]
    ScrapeStart {
        platform: String,
        keywords: Vec<String>,
        max_jd: u32,
        min_score: u8,
        /// Boss 城市编码（如 `101020100` 上海）。缺省 = sidecar 默认城市。
        ///
        /// 以下筛选键与 `scraper/sidecar.py::_run_scrape` 读取的 payload 键
        /// **逐字对齐**（`scale`/`stage`，不是配置键 `company_scale`/`finance_stage`）。
        /// 全部 `#[serde(default)]` + 空值不序列化：旧四字段载荷仍可反序列化，
        /// 未设置筛选时线上形状与旧版完全一致。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        city: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        salary: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        experience: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        degree: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        scale: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        stage: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        industry: Vec<String>,
        /// `1901` 全职 / `1902` 兼职 / `1903` 实习；缺省 = 不限。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        job_type: Option<String>,
        /// 翻页次数；缺省 = sidecar 默认（5）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_pages: Option<u32>,
    },
    #[serde(rename = "scrape.stop")]
    ScrapeStop,
    #[serde(rename = "login.start")]
    LoginStart { platform: String },
    #[serde(rename = "ping")]
    Ping,
    /// 拉取会话列表（左侧 HR 列表）。
    #[serde(rename = "chat.friends")]
    ChatFriends,
    /// 拉取与某个 HR 的历史消息。
    #[serde(rename = "chat.messages")]
    ChatMessages { uid: String },
    /// 发送一条文本消息。
    #[serde(rename = "chat.send")]
    ChatSend { uid: String, text: String },
    /// 当前账号可发送的简历列表。
    #[serde(rename = "chat.resumes")]
    ChatResumes,
    /// 发送一份简历。
    #[serde(rename = "chat.send_resume")]
    ChatSendResume { uid: String, resume_id: String },
    /// 定位会话并进入：按公司名/HR 名找到会话，返回命中的 uid。
    #[serde(rename = "chat.open")]
    ChatOpen { query: String },
    /// 撤回最近一条自己发的消息。
    #[serde(rename = "chat.recall")]
    ChatRecall { uid: String },
    /// 读取账号里配置的「求职期望」职位。
    ///
    /// 走浏览器自带的请求能力（复用已登录 cookie），不是 DOM 抓取——
    /// 期望职位在页面上藏得深且随版本改版，接口却常年稳定。
    #[serde(rename = "expectation.list")]
    ExpectationList,
    /// 只把浏览器拉起来并报告登录态，不抓岗位、不导航。
    ///
    /// 存在的理由：聊天与「求职期望」都需要一个活着的 sidecar，但在此之前
    /// 界面上唯一的启动入口是「开始抓取」或「扫码登录」。用户想看一眼消息，
    /// 却被迫先跑一次抓取——这是把实现顺序当成了操作顺序。本命令把
    /// 「确保浏览器可用」独立出来，回执 `{logged_in, state}`。
    #[serde(rename = "session.open")]
    SessionOpen {
        platform: String,
        /// 可选目标页，"chat" = Boss 聊天页；省略则不导航。
        ///
        /// 补这个字段的原因：只启动不导航时浏览器停在 about:blank，用户点
        /// 「Boss 消息」看到的是一片空白，要等 chat.friends 自己 goto 才有
        /// 内容。把「打开哪一页」交给调用方说清楚，点开即有所见。
        #[serde(default)]
        page: Option<String>,
    },
}

/// 聊天相关回执的数据结构。全部 `Option`/`Vec`，缺字段不算失败。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Friend {
    pub uid: String,
    pub name: String,
    pub company: Option<String>,
    pub title: Option<String>,
    pub last_msg: Option<String>,
    pub last_time: Option<String>,
    pub unread: Option<u32>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// `me` | `hr` | `system`
    pub sender: String,
    pub content: String,
    pub time: Option<String>,
    /// 服务端消息 ID，撤回时要用
    pub msg_id: Option<String>,
    pub recalled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ResumeItem {
    pub resume_id: String,
    pub name: String,
    pub updated_at: Option<String>,
}

/// 账号里配置的一条求职期望。
///
/// 字段与现行版 `/api/expectations` 对齐（`backend/app/main.py:1344`），
/// 这样前端不必区分数据来自旧版还是新版。
///
/// 每个字段都带 `#[serde(default)]`：sidecar 用 `.get(...) or ""` 拼输出，
/// 上游少给一个键就会让缺的字段整个消失。没有 default 时 `items_of` 会把
/// 整条记录当坏行跳过——「读到了期望职位却显示空列表」，还不报错。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Expectation {
    /// 职位名，如「Java」
    pub name: String,
    /// Boss 的职位编码
    pub code: String,
    pub city: String,
    pub city_code: String,
    pub salary: String,
    /// 恒为 `boss_expect`，留给将来多平台
    pub source: String,
}

/// sidecar → Rust 的事件（回执走 [`Envelope`]）。
///
/// `JobScraped` 携带的是**原始抓取结果**，不是打分后的结论。sidecar 无状态，
/// 不打分、不落库、不读配置——是否入库、得几分全部由 Rust 侧决定（SPEC-002 2.1）。
/// 字段命名与 `job_queue` 表列名逐字一致，便于直接映射。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum Event {
    #[serde(rename = "job.scraped")]
    JobScraped(ScrapedJob),
    #[serde(rename = "log")]
    Log { level: String, msg: String },
    #[serde(rename = "ready")]
    Ready { pid: u32 },
    /// 登录流程状态变化。抓取前必须先到 `logged_in`，否则 Boss 会把
    /// 搜索页重定向到登录页，白跑几分钟才报「没拿到岗位」。
    #[serde(rename = "login.state")]
    LoginState {
        platform: String,
        /// `waiting` | `logged_in` | `closed` | `error`
        state: String,
        msg: String,
    },
    /// 抓取流程收尾（自然完成 / 早退 / 异常 / 用户停止都会发）。
    /// Rust 侧靠它清 `running`——sidecar 抓完仍守在 stdin 上不发 Exited，
    /// 缺这帧界面永停「抓取中」。additive 扩展：老壳不认识该事件名，
    /// codec 按未知事件静默丢弃，不报错。
    #[serde(rename = "scrape.finished")]
    ScrapeFinished {
        /// sidecar 自报抓取条数（诊断用；权威入库计数在 Rust 侧）
        count: u32,
        /// 用户主动停止（scrape.stop）收尾时为 true
        stopped: bool,
        /// 非空 = 异常收尾（早退 / 异常），用于 finished_reason 文案
        error: String,
    },
    /// 抓取检测到安全验证（风控）并已停止。在 `scrape.finished` 之前发出。
    /// 界面据此提示「需手动过验证，过完后点恢复重新抓取」，而不是静默空列表。
    /// additive 扩展：老壳不认识该事件名时由 codec 静默丢弃。
    #[serde(rename = "scrape.risk")]
    ScrapeRisk {
        /// 平台标识（当前仅 `boss`）
        platform: String,
        /// 命中的风控原因（人话，直接展示给用户）
        reason: String,
        /// 命中时的页面地址，便于用户定位要过验证的页面
        url: String,
    },
}

/// 一条抓取到的岗位原始数据。
///
/// 与 `job_queue` 表列一一对应，全部 `Option`：Boss 页面经常缺少某些字段
/// （比如卡片视图没有公司规模），不能因为一个字段缺失就丢弃整条岗位。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ScrapedJob {
    pub job_id: String,
    pub platform: String,
    pub title: Option<String>,
    pub company: Option<String>,
    pub salary: Option<String>,
    pub experience: Option<String>,
    pub degree: Option<String>,
    pub location: Option<String>,
    pub job_url: Option<String>,
    pub jd_text: Option<String>,
    pub company_industry: Option<String>,
    pub company_scale: Option<String>,
    pub company_stage: Option<String>,
    pub company_intro: Option<String>,
    pub company_address: Option<String>,
    pub hr_name: Option<String>,
    pub hr_position: Option<String>,
    pub hr_active_status: Option<String>,
    pub founded_years: Option<u32>,
    pub reg_capital: Option<String>,
    pub matched_keyword: Option<String>,
}

impl ScrapedJob {
    /// 从 JSON 值构造，缺失字段按 `None` 处理。
    ///
    /// 不用 `serde_json::from_value` 直接反序列化成员，是因为 sidecar 可能
    /// 传来未知的多余字段——用 `Value` 逐个取可以忽略它们，不会整体失败。
    pub fn from_value(v: &serde_json::Value) -> Option<Self> {
        if !v.is_object() {
            return None;
        }
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string());
        // founded_years 可能是数字也可能是字符串（Python 侧可能给 "3"）
        let founded_years = v.get("founded_years").and_then(|x| {
            x.as_u64()
                .map(|n| n as u32)
                .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
        });
        Some(Self {
            job_id: s("job_id")?,
            platform: s("platform").unwrap_or_else(|| "boss".to_string()),
            title: s("title"),
            company: s("company"),
            salary: s("salary"),
            experience: s("experience"),
            degree: s("degree"),
            location: s("location"),
            job_url: s("job_url"),
            jd_text: s("jd_text"),
            company_industry: s("company_industry"),
            company_scale: s("company_scale"),
            company_stage: s("company_stage"),
            company_intro: s("company_intro"),
            company_address: s("company_address"),
            hr_name: s("hr_name"),
            hr_position: s("hr_position"),
            hr_active_status: s("hr_active_status"),
            founded_years,
            reg_capital: s("reg_capital"),
            matched_keyword: s("matched_keyword"),
        })
    }
}

/// 截断超长文本，避免单行 JSON 过大。
pub fn clamp_text(text: &str) -> String {
    if text.len() <= MAX_LINE_BYTES / 2 {
        return text.to_string();
    }
    // 按字符边界安全截断，避免把 UTF-8 多字节字符切一半
    let mut end = MAX_LINE_BYTES / 2;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[截断]", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_serializes_like_spec_example() {
        // 必须与 SPEC-002 第 2.1 节的 {"id":"c1","ok":true,...} 形状一致
        let line = Envelope::reply("c1", true).to_line().unwrap();
        assert_eq!(line, r#"{"id":"c1","ok":true}"#);
    }

    #[test]
    fn event_serializes_without_id_and_ok() {
        let line = Envelope::event("log", serde_json::json!({"level":"info","msg":"hi"}))
            .to_line()
            .unwrap();
        assert!(!line.contains("\"id\""));
        assert!(!line.contains("\"ok\""));
        assert!(line.contains("\"event\":\"log\""));
    }

    #[test]
    fn command_uses_cmd_tag() {
        let c = Command::Ping;
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["cmd"], "ping");
    }

    #[test]
    fn scrape_start_roundtrip() {
        let c = Command::ScrapeStart {
            platform: "boss".into(),
            keywords: vec!["python".into()],
            max_jd: 40,
            min_score: 70,
            city: Some("101020100".into()),
            salary: vec!["406".into()],
            experience: Vec::new(),
            degree: Vec::new(),
            scale: Vec::new(),
            stage: Vec::new(),
            industry: Vec::new(),
            job_type: None,
            max_pages: Some(3),
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: Command = serde_json::from_str(&s).unwrap();
        match back {
            Command::ScrapeStart { min_score, city, salary, max_pages, .. } => {
                assert_eq!(min_score, 70);
                assert_eq!(city.as_deref(), Some("101020100"));
                assert_eq!(salary, vec!["406".to_string()]);
                assert_eq!(max_pages, Some(3));
            }
            _ => panic!("类型不匹配"),
        }
    }

    /// 筛选键必须平铺、蛇形、与 sidecar `_run_scrape` 的 payload 键逐字一致。
    /// 名字写错不会报错，只会让用户的城市/薪资筛选静默失效（AGENTS.md 2.2）。
    #[test]
    fn scrape_start_serializes_filters_flat_with_sidecar_key_names() {
        let c = Command::ScrapeStart {
            platform: "boss".into(),
            keywords: vec!["Java".into()],
            max_jd: 0,
            min_score: 70,
            city: Some("101280600".into()),
            salary: vec!["406".into()],
            experience: vec!["104".into()],
            degree: vec!["203".into()],
            scale: vec!["303".into()],
            stage: vec!["803".into()],
            industry: vec!["100020".into()],
            job_type: Some("1901".into()),
            max_pages: Some(3),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["cmd"], "scrape.start");
        assert_eq!(v["city"], "101280600");
        assert_eq!(v["salary"][0], "406");
        assert_eq!(v["experience"][0], "104");
        assert_eq!(v["degree"][0], "203");
        assert_eq!(v["scale"][0], "303");
        assert_eq!(v["stage"][0], "803");
        assert_eq!(v["industry"][0], "100020");
        assert_eq!(v["job_type"], "1901");
        assert_eq!(v["max_pages"], 3);
        assert!(v.get("params").is_none(), "参数必须平铺");
        assert!(v.get("company_scale").is_none(), "协议键是 scale 不是配置键");
        assert!(v.get("finance_stage").is_none(), "协议键是 stage 不是配置键");
    }

    /// 旧四字段载荷必须仍能反序列化；筛选全空时序列化不得多出任何键。
    #[test]
    fn scrape_start_without_filters_keeps_legacy_shape() {
        let back: Command = serde_json::from_str(
            r#"{"cmd":"scrape.start","platform":"boss","keywords":["Java"],"max_jd":0,"min_score":70}"#,
        )
        .unwrap();
        match &back {
            Command::ScrapeStart { city, salary, max_pages, job_type, .. } => {
                assert!(city.is_none());
                assert!(salary.is_empty());
                assert!(max_pages.is_none());
                assert!(job_type.is_none());
            }
            _ => panic!("类型不匹配"),
        }
        let v = serde_json::to_value(&back).unwrap();
        for key in ["city", "salary", "experience", "degree", "scale", "stage", "industry", "job_type", "max_pages"] {
            assert!(v.get(key).is_none(), "空筛选不得序列化出 {key}");
        }
    }

    /// `session.open` 的 `page` 必须真的出现在 JSON 里，且省略时能被反序列化。
    ///
    /// 两件事都在防同一个静默故障：Rust 侧加了字段但 Python 侧读不到（名字不一致
    /// 或是嵌套在某个对象里），表现为「点了 Boss 消息，浏览器还是空白页」——
    /// 不报错，只是没导航。
    #[test]
    fn session_open_carries_page_flat() {
        let c = Command::SessionOpen {
            platform: "boss".into(),
            page: Some("chat".into()),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["cmd"], "session.open");
        assert_eq!(v["platform"], "boss");
        // 平铺，不是嵌套：sidecar.py 用 cmd.get("page") 直接读
        assert_eq!(v["page"], "chat");
    }

    #[test]
    fn session_open_page_is_optional() {
        let back: Command = serde_json::from_str(r#"{"cmd":"session.open","platform":"boss"}"#).unwrap();
        match back {
            Command::SessionOpen { platform, page } => {
                assert_eq!(platform, "boss");
                assert_eq!(page, None);
            }
            _ => panic!("类型不匹配"),
        }
    }

    #[test]
    fn clamp_text_keeps_short_text() {
        assert_eq!(clamp_text("短文本"), "短文本");
    }

    #[test]
    fn clamp_text_truncates_long_text_on_char_boundary() {
        let long = "汉".repeat(MAX_LINE_BYTES);
        let out = clamp_text(&long);
        assert!(out.len() <= MAX_LINE_BYTES / 2 + 16);
        assert!(out.ends_with("…[截断]"));
    }

    #[test]
    fn scraped_job_ignores_unknown_fields() {
        // sidecar 将来加字段时，旧 Rust 侧不能整体失败，只能忽略
        let v = serde_json::json!({
            "job_id": "boss:abc",
            "title": "Java 工程师",
            "某天新增的字段": 123,
        });
        let job = ScrapedJob::from_value(&v).expect("应能解析");
        assert_eq!(job.job_id, "boss:abc");
        assert_eq!(job.title.as_deref(), Some("Java 工程师"));
        assert_eq!(job.platform, "boss", "platform 缺失时默认 boss");
        assert!(job.jd_text.is_none());
    }

    #[test]
    fn scraped_job_requires_job_id() {
        let v = serde_json::json!({ "title": "无 ID" });
        assert!(ScrapedJob::from_value(&v).is_none(), "缺 job_id 视为非法条目");
        assert!(ScrapedJob::from_value(&serde_json::json!("不是对象")).is_none());
    }

    #[test]
    fn founded_years_accepts_number_or_string() {
        let num = serde_json::json!({"job_id": "a", "founded_years": 7});
        assert_eq!(ScrapedJob::from_value(&num).unwrap().founded_years, Some(7));
        let txt = serde_json::json!({"job_id": "a", "founded_years": "12"});
        assert_eq!(ScrapedJob::from_value(&txt).unwrap().founded_years, Some(12));
    }

    #[test]
    fn job_scraped_event_carries_full_fields() {
        let job = ScrapedJob {
            job_id: "boss:x1".into(),
            platform: "boss".into(),
            title: Some("后端".into()),
            company: Some("某公司".into()),
            salary: Some("20-30K".into()),
            hr_active_status: Some("今日活跃".into()),
            founded_years: Some(5),
            ..Default::default()
        };
        let ev = Event::JobScraped(job);
        let line = serde_json::to_string(&ev).unwrap();
        assert!(line.contains("\"event\":\"job.scraped\""), "实际：{line}");
        assert!(line.contains("今日活跃"), "HR 活跃度必须能过线");
        let back: Event = serde_json::from_str(&line).unwrap();
        match back {
            Event::JobScraped(j) => {
                assert_eq!(j.salary.as_deref(), Some("20-30K"));
                assert_eq!(j.founded_years, Some(5));
            }
            _ => panic!("类型不匹配"),
        }
    }

    /// `scrape.finished` 的帧名与字段必须和 `scraper/sidecar.py::_run_scrape`
    /// 的 emit 逐字一致；写错不会编译失败，只会让界面永停「抓取中」。
    #[test]
    fn scrape_finished_event_roundtrip() {
        let ev = Event::ScrapeFinished { count: 7, stopped: false, error: String::new() };
        let line = serde_json::to_string(&ev).unwrap();
        assert!(line.contains("\"event\":\"scrape.finished\""), "实际：{line}");
        let back: Event = serde_json::from_str(&line).unwrap();
        match back {
            Event::ScrapeFinished { count, stopped, error } => {
                assert_eq!(count, 7);
                assert!(!stopped);
                assert!(error.is_empty());
            }
            _ => panic!("类型不匹配"),
        }
    }
}
