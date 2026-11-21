//! 抓取筛选的选项常量。
//!
//! **唯一真源是 `docs/ui/UI-SPEC-web-align.md` 第 3 节**（逐字摘录自旧版
//! `web/src/api.ts`）。这里一个值都不许自己编、不许增删、不许改顺序——
//! 这些 code 是 Boss 直聘的城市 / 经验 / 学历 / 规模 / 阶段 / 行业编码，
//! 写错只会表现为「筛不出岗位」，不会报错。
//!
//! 为什么放在 `features/scrape/` 而不是 `lib_ui/`：SPEC 第 8 节 B1 建议落在
//! `lib_ui/options.rs`，但 `lib_ui` 的依赖方向与用例都不该承担「Boss 编码表」
//! 这种业务字典；它只有抓取筛选一个消费方。`features/scrape/**` 在 Lane H
//! 的可写范围内，`lib_ui/**` 除 `text.rs` 外只读，落在这里也避免与其它 lane 撞文件。

/// 选项：(编码, 显示名)。`code` 一律是字符串——有的是数字编码，有的形如 `sal:8-12`。
pub type Opt = (&'static str, &'static str);

// ---------------------------------------------------------------- 城市（8）
pub const CITIES: [Opt; 8] = [
    ("101280600", "深圳"),
    ("101020100", "上海"),
    ("101010100", "北京"),
    ("101280100", "广州"),
    ("101210100", "杭州"),
    ("101270100", "成都"),
    ("101190400", "苏州"),
    ("101030100", "天津"),
];

/// 城市默认值（旧版 `useState('101280600')`）。
pub const CITY_DEFAULT: &str = "101280600";

// ---------------------------------------------------------------- 薪资（6 + 自定义）
pub const SALARY_OPTIONS: [Opt; 6] = [
    ("sal:0-8", "8K以下"),
    ("sal:8-12", "8-12K"),
    ("sal:12-16", "12-16K"),
    ("sal:16-20", "16-20K"),
    ("sal:20-30", "20-30K"),
    ("sal:30-9999", "30K以上"),
];

/// 自定义薪资的上界哨兵：`high >= 9000` 显示成「{low}K以上」（旧版 `App.tsx:1678`）。
pub const SALARY_OPEN_HIGH: u32 = 9999;

// ---------------------------------------------------------------- 经验（7）
pub const EXPERIENCE_OPTIONS: [Opt; 7] = [
    ("101", "在校/应届"),
    ("102", "经验不限"),
    ("103", "1年以内"),
    ("104", "1-3年"),
    ("105", "3-5年"),
    ("106", "5-10年"),
    ("107", "10年以上"),
];

// ---------------------------------------------------------------- 学历（7）
pub const DEGREE_OPTIONS: [Opt; 7] = [
    ("202", "大专"),
    ("203", "本科"),
    ("204", "硕士"),
    ("205", "博士"),
    ("206", "中专/中技"),
    ("208", "高中"),
    ("209", "初中及以下"),
];

// ---------------------------------------------------------------- 公司规模（6）
pub const SCALE_OPTIONS: [Opt; 6] = [
    ("301", "0-20人"),
    ("302", "20-99人"),
    ("303", "100-499人"),
    ("304", "500-999人"),
    ("305", "1000-9999人"),
    ("306", "10000人以上"),
];

// ---------------------------------------------------------------- 融资阶段（8）
pub const STAGE_OPTIONS: [Opt; 8] = [
    ("801", "未融资"),
    ("802", "天使轮"),
    ("803", "A轮"),
    ("804", "B轮"),
    ("805", "C轮"),
    ("806", "D轮及以上"),
    ("807", "已上市"),
    ("808", "不需要融资"),
];

// ---------------------------------------------------------------- 职位类型（3）
pub const JOB_TYPE_OPTIONS: [Opt; 3] = [("1901", "全职"), ("1902", "兼职"), ("1903", "实习")];

/// 职位类型默认值（旧版 `useState('1901')`）。
pub const JOB_TYPE_DEFAULT: &str = "1901";

// ---------------------------------------------------------------- 行业（18）
pub const INDUSTRY_OPTIONS: [Opt; 18] = [
    ("100020", "互联网"),
    ("100021", "电子商务"),
    ("100022", "计算机软件"),
    ("100023", "计算机硬件"),
    ("100024", "通信/网络设备"),
    ("100025", "电子/半导体/集成电路"),
    ("100026", "人工智能"),
    ("100027", "云计算/大数据"),
    ("100028", "企业服务"),
    ("100029", "金融/银行/证券"),
    ("100030", "游戏"),
    ("100031", "医疗健康"),
    ("100032", "教育培训"),
    ("100033", "汽车/新能源"),
    ("100034", "智能制造/工业自动化"),
    ("100035", "物流/仓储"),
    ("100036", "房地产/建筑"),
    ("100037", "消费零售"),
];

// ---------------------------------------------------------------- 工作制（3）
//
// 字段名是 `value` 不是 `code`（SPEC 3.9 特意点出），这里用带名字的结构体，
// 免得调用方把 `value` 当成编码理解。
pub struct WeekendOpt {
    pub value: &'static str,
    pub name: &'static str,
    pub hint: &'static str,
}

pub const WEEKEND_OPTIONS: [WeekendOpt; 3] = [
    WeekendOpt {
        value: "any",
        name: "不限",
        hint: "不限制休息制度",
    },
    WeekendOpt {
        value: "double",
        name: "只投双休",
        hint: "只要双休岗位；职位描述或岗位名出现单休 / 大小周 / 周六上班 / 做六休一 / 月休N天 就排除",
    },
    WeekendOpt {
        value: "no_bigweek",
        name: "大小周",
        hint: "排除大小周 / 单休 / 做六休一 / 月休N天 的岗位",
    },
];

// ---------------------------------------------------------------- HR 活跃度（4 含不限）
pub struct HrActiveOpt {
    pub value: &'static str,
    pub name: &'static str,
    pub hint: &'static str,
}

pub const HR_ACTIVE_OPTIONS: [HrActiveOpt; 4] = [
    HrActiveOpt {
        value: "any",
        name: "不限",
        hint: "不按 HR 活跃度排除",
    },
    HrActiveOpt {
        value: "1",
        name: "今日活跃",
        hint: "只保留最近 1 天内活跃的 HR；活跃度未知的也会被排除",
    },
    HrActiveOpt {
        value: "3",
        name: "3天内",
        hint: "只保留最近 3 天内活跃的 HR；活跃度未知的也会被排除",
    },
    HrActiveOpt {
        value: "7",
        name: "一周内",
        hint: "只保留最近 7 天内活跃的 HR；活跃度未知的也会被排除",
    },
];

// ---------------------------------------------------------------- JD 学历要求（5 含不限）
pub const DEGREE_VETO_OPTIONS: [&'static str; 4] = ["大专", "本科", "硕士", "博士"];

// ---------------------------------------------------------------- 平台 / 推荐词 / 排除词
pub struct PlatformOpt {
    pub id: &'static str,
    pub name: &'static str,
    pub ready: bool,
}

pub const PLATFORMS: [PlatformOpt; 3] = [
    PlatformOpt { id: "boss", name: "Boss直聘", ready: true },
    PlatformOpt { id: "zhilian", name: "智联招聘", ready: false },
    PlatformOpt { id: "51job", name: "前程无忧", ready: false },
];

/// 平台 id → 显示名。找不到时原样回显 id（旧版 `platformName`）。
pub fn platform_name(id: &str) -> String {
    PLATFORMS
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.to_string())
        .unwrap_or_else(|| id.to_string())
}

pub const RECOMMEND_KEYWORDS: [&str; 6] = [
    "Java",
    "后端开发",
    "全栈工程师",
    "Node.js",
    "服务端开发",
    "Java开发工程师",
];

/// 排除关键词的 textarea 占位（旧版 `App.tsx:1802`）。
pub const EXCLUDE_PLACEHOLDER: &str = "外包 驻场 兼职 日薪 跨境 外贸 金融 银行 证券 区块链 Web3 985 211 硕士 博士 销售 产品经理 总监";

/// 排除关键词默认值（旧版 `DEFAULT_EXCLUDE`，`App.tsx:212`）。
pub const DEFAULT_EXCLUDE: &str = "外包,驻场,人力,兼职,跨境,金融,银行,证券,区块链,Web3,985,211,硕士,博士,销售";

/// 翻页次数的默认值与取值范围（旧版 `useState(3)`、`min={1} max={20}`）。
pub const MAX_PAGES_DEFAULT: u32 = 3;
pub const MAX_PAGES_MIN: u32 = 1;
pub const MAX_PAGES_MAX: u32 = 20;

/// 把选项值转成可安全放进 `data-testid` 的片段。
///
/// 薪资编码形如 `sal:8-12`，冒号在 testid / CSS 选择器里会带来歧义，
/// 按 SPEC 7.7 统一把 `:` 换成 `-`：`sal:8-12` → `sal-8-12`。
pub fn tid_safe(code: &str) -> String {
    code.chars()
        .map(|c| match c {
            ':' | '/' | '\\' | ' ' => '-',
            other => other,
        })
        .collect()
}

/// 薪资档位 → 显示名。常量表里查得到就用表里的，
/// 自定义档按旧版规则：`high >= 9000` 显示「{low}K以上」。
pub fn salary_label(code: &str) -> String {
    if let Some((_, name)) = SALARY_OPTIONS.iter().find(|(c, _)| *c == code) {
        return name.to_string();
    }
    let Some(rest) = code.strip_prefix("sal:") else {
        return code.to_string();
    };
    let mut it = rest.splitn(2, '-');
    let low = it.next().unwrap_or("0");
    let high = it.next().unwrap_or("0");
    match high.parse::<u32>() {
        Ok(h) if h >= 9000 => format!("{low}K以上"),
        _ => format!("{low}-{high}K"),
    }
}

/// 是否为常量表之外的自定义薪资档（用于决定是否显示「×」删除）。
pub fn is_custom_salary(code: &str) -> bool {
    code.starts_with("sal:") && !SALARY_OPTIONS.iter().any(|(c, _)| *c == code)
}

/// 由「下限 / 上限」两个输入产出薪资编码。
///
/// 上限留空或 ≤ 下限 → 视为「{low}K以上」（哨兵 9999），与旧版一致。
/// 两个都空 → `None`（什么都不加）。
pub fn salary_code(low: &str, high: &str) -> Option<String> {
    let lo: u32 = low.trim().parse().ok().unwrap_or(0);
    let hi: u32 = high.trim().parse().ok().unwrap_or(0);
    if lo == 0 && hi == 0 {
        return None;
    }
    Some(if hi > lo {
        format!("sal:{lo}-{hi}")
    } else {
        format!("sal:{lo}-{SALARY_OPEN_HIGH}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 计数锚点：SPEC 第 3 节的项数一个都不许变。
    #[test]
    fn option_counts_match_spec() {
        assert_eq!(CITIES.len(), 8);
        assert_eq!(SALARY_OPTIONS.len(), 6);
        assert_eq!(EXPERIENCE_OPTIONS.len(), 7);
        assert_eq!(DEGREE_OPTIONS.len(), 7);
        assert_eq!(SCALE_OPTIONS.len(), 6);
        assert_eq!(STAGE_OPTIONS.len(), 8);
        assert_eq!(JOB_TYPE_OPTIONS.len(), 3);
        assert_eq!(INDUSTRY_OPTIONS.len(), 18);
        assert_eq!(WEEKEND_OPTIONS.len(), 3);
        assert_eq!(HR_ACTIVE_OPTIONS.len(), 4);
        assert_eq!(DEGREE_VETO_OPTIONS.len(), 4);
        assert_eq!(RECOMMEND_KEYWORDS.len(), 6);
    }

    #[test]
    fn first_options_are_the_documented_defaults() {
        assert_eq!(CITIES[0].0, CITY_DEFAULT);
        assert_eq!(CITIES[0].1, "深圳");
        assert_eq!(JOB_TYPE_OPTIONS[0].0, JOB_TYPE_DEFAULT);
        assert_eq!(INDUSTRY_OPTIONS[0], ("100020", "互联网"));
        assert_eq!(INDUSTRY_OPTIONS[17], ("100037", "消费零售"));
        assert_eq!(STAGE_OPTIONS[7], ("808", "不需要融资"));
        assert_eq!(DEGREE_OPTIONS[6], ("209", "初中及以下"));
    }

    #[test]
    fn salary_labels_cover_builtin_and_custom() {
        assert_eq!(salary_label("sal:0-8"), "8K以下");
        assert_eq!(salary_label("sal:30-9999"), "30K以上");
        assert_eq!(salary_label("sal:25-40"), "25-40K");
        // 上限满哨兵 → 「以上」
        assert_eq!(salary_label("sal:25-9999"), "25K以上");
        assert!(!is_custom_salary("sal:8-12"));
        assert!(is_custom_salary("sal:25-40"));
    }

    #[test]
    fn salary_code_follows_legacy_rules() {
        assert_eq!(salary_code("10", "20").as_deref(), Some("sal:10-20"));
        // 上限留空 → 「10K以上」
        assert_eq!(salary_code("10", "").as_deref(), Some("sal:10-9999"));
        // 上限 ≤ 下限 → 同上
        assert_eq!(salary_code("20", "10").as_deref(), Some("sal:20-9999"));
        // 两个都空 → 什么都不加
        assert_eq!(salary_code("", ""), None);
    }

    #[test]
    fn tid_safe_replaces_colon() {
        assert_eq!(tid_safe("sal:8-12"), "sal-8-12");
        assert_eq!(tid_safe("101280600"), "101280600");
        // 行业名里的斜杠不会进 testid（用的是 code），但这里保证通用安全
        assert_eq!(tid_safe("100024"), "100024");
    }

    #[test]
    fn platform_name_falls_back_to_id() {
        assert_eq!(platform_name("boss"), "Boss直聘");
        assert_eq!(platform_name("unknown"), "unknown");
    }
}
