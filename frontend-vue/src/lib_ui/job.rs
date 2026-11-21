//! 岗位域内的判定与格式化。纯函数，不依赖框架。

/// 岗位实体（界面层视角）。
///
/// **为什么定义在这里**：`lib_ui` 是依赖图的最底层，`is_rejected` 之类的判定
/// 要用到它的字段，但 `lib_ui` 不得反向引用 `api`。字段与
/// `api::types::Job`（= Rust 侧 serde 输出）逐字一致；那条 re-export 路径
/// `api::types::Job` 指向的就是**本结构体**，因此既有的 `use` 路径与类型名
/// 全部不变，反序列化仍在 `api` 侧编译期校验。
///
/// 改字段时必须同一次改动里核对 `backend-tauri/src/core/domain/job.rs`：
/// 这里少一个字段不会报错，只会在运行时显示空白。
#[derive(Debug, Clone, Default)]
pub struct Job {
    pub job_id: String,
    pub platform: String,
    pub title: String,
    pub company: String,
    pub salary: String,
    pub status: String,
    pub created_at: i64,
    pub job_url: String,
    pub jd_text: String,
    pub location: String,
    pub experience: String,
    pub degree: String,
    pub hr_name: String,
    pub hr_position: String,
    pub hr_active_status: String,
    pub company_industry: String,
    pub company_scale: String,
    pub company_stage: String,
    pub company_address: String,
    pub company_intro: String,
    pub hiring_count: i64,
    pub founded_years: i64,
    pub match_score: u8,
    pub rejected: i64,
    pub score_source: String,
    pub score_reason: String,
    pub updated_at: i64,
    pub score_scale: i64,
}

impl Job {
    /// 规则否决的岗位。`rejected` 是 i64 不是 bool（史遗留列），非 0 即否决。
    pub fn is_rejected(&self) -> bool {
        self.rejected != 0
    }
}

/// SQLite 没有布尔列：`rejected != 0` 即被排除。
pub fn is_rejected(job: &Job) -> bool {
    job.rejected != 0
}

/// 分数落在门槛的哪一侧：用于 CSS 类 `score-num is-{tone}`。
///
/// `score == 0` 视为「未打分」，必须与「0 分」区分开——否则未打分岗位
/// 会被渲染成刺眼的失败红。门槛未知（`None`）时一律按通过处理，
/// 因为此时还无从判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreTone {
    Pass,
    Fail,
    None,
}

impl ScoreTone {
    pub fn as_str(self) -> &'static str {
        match self {
            ScoreTone::Pass => "pass",
            ScoreTone::Fail => "fail",
            ScoreTone::None => "none",
        }
    }
}

pub fn score_tone(score: u8, min_score: Option<u8>) -> ScoreTone {
    if score == 0 {
        return ScoreTone::None;
    }
    match min_score {
        None => ScoreTone::Pass,
        Some(min) if score >= min => ScoreTone::Pass,
        Some(_) => ScoreTone::Fail,
    }
}
