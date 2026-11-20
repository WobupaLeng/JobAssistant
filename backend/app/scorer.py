"""JD 打分器。

两种模式（PRD 要求）：
1. 配置了大模型（OpenAI 兼容）→ 用大模型给 JD 打分，返回 1-10 分 + 理由。
2. 未配置 → 用规则兜底（RuleScorer），逻辑移植自原 Java 版 ScreenRule。

对外只暴露 score(job) -> ScoreResult，调用方不关心走的是哪条路径。
"""
from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from typing import Any

# ---------- 简历 / 求职者画像（规则打分与提示词共用） ----------

PROFILE = {
    "years": 10,
    "title": "Java 后端 / 全栈工程师",
    "salary_low": 15,   # 期望薪资下限（K）
    "salary_high": 35,  # 期望薪资上限（K）
    "city": "上海",
    "core_skills": [
        "Java", "Spring Boot", "Spring Cloud", "MyBatis", "MySQL",
        "Redis", "RabbitMQ",
    ],
    "bonus_skills": [
        "Vue", "JavaScript", "ECharts", "Electron", "uni-app", "Laravel",
        "Docker", "Linux", "Git", "Kafka", "RocketMQ", "SpringBoot",
    ],
    "summary": (
        "约10年 Java 后端与全栈经验，熟悉 Spring Boot/Cloud、MyBatis、MySQL、"
        "Redis、RabbitMQ，前端 Vue/JavaScript/ECharts/Electron/uni-app，"
        "有 Laravel/Dcat 经验，熟悉 Linux/Docker/Git，"
        "做过大模型 API 接入、RAG、AI 客服与多轮上下文、异步任务与回调、多租户 SaaS。"
    ),
}

# 命中即大幅扣分/判负信号
OUTSOURCE_WORDS = [
    "外包", "驻场", "外派", "派遣", "人力外包", "劳务派遣", "驻场开发",
    "驻场工程师", "外派至", "派遣员工", "外包编制", "外包岗", "驻场岗",
]
# 公司名/岗位名里出现就排除（默认值，可在界面上改）
VETO_NAME_WORDS = [
    "外包", "驻场", "外派", "派遣", "人力", "兼职", "日薪", "跨境", "外贸",
    "海外仓", "亚马逊", "独立站", "Shopify", "电商分销", "私域结算", "金融",
    "证券", "基金", "银行", "支付清结算", "风控", "期权", "币圈", "区块链",
    "Web3", "硕士", "博士", "985", "211", "销售", "产品经理", "总监",
]
# 下列四条默认都是"不限"（None = 不排除），只有用户填了值才生效
DEFAULT_EXCLUDE_SALARY_MIN = None      # 薪资上限低于此值(K) → 排除
DEFAULT_EXCLUDE_DEGREE = []            # JD 硬卡这些学历 → 排除，如 ["硕士","博士"]
DEFAULT_EXCLUDE_FOUNDED_BELOW = None   # 公司成立年限小于此年数 → 排除
DEFAULT_EXCLUDE_RESUME_MAX_YEARS = None  # JD 要求经验超过此年限 → 排除（默认不限制）
DEFAULT_EXCLUDE_SALARY_MAX = None      # 薪资上限高于此值 → 排除


@dataclass
class ExcludeRules:
    """用户在界面上可调的排除规则。值为空 = 该项不排除。"""

    name_words: list[str] = field(default_factory=lambda: list(VETO_NAME_WORDS))
    exclude_salary_min: int | None = DEFAULT_EXCLUDE_SALARY_MIN
    # 薪资上限：岗位给的最高薪资高于这个值就排除（和下限一起用，夹出想要的薪资带）
    exclude_salary_max: int | None = DEFAULT_EXCLUDE_SALARY_MAX
    exclude_degree: list[str] = field(default_factory=lambda: list(DEFAULT_EXCLUDE_DEGREE))
    exclude_founded_below: int | None = DEFAULT_EXCLUDE_FOUNDED_BELOW
    exclude_resume_max_years: int | None = DEFAULT_EXCLUDE_RESUME_MAX_YEARS
    # HR 活跃度门槛：超过这么天内没活跃（或无法判断）就排除；空 = 不排除
    exclude_hr_active_days: int | None = None
    # 工作制：勾了「双休」表示只要双休岗（JD/标签写了单休、大小周、周六上班等就排除）
    weekend: str | None = None      # 'double'=必须双休；None=不限制

    def as_dict(self) -> dict[str, Any]:
        return {
            "name_words": list(self.name_words),
            "exclude_salary_min": self.exclude_salary_min,
            "exclude_salary_max": self.exclude_salary_max,
            "exclude_degree": list(self.exclude_degree),
            "exclude_founded_below": self.exclude_founded_below,
            "exclude_resume_max_years": self.exclude_resume_max_years,
            "exclude_hr_active_days": self.exclude_hr_active_days,
            "weekend": self.weekend,
        }
# ---------- 工作制（双休规则） ----------
# 出现这些说法 = 不是双休（或休息不固定），用户要求双休时排除
WEEKEND_BAD_RE = re.compile(
    r"(单休|大小周|大周|小周|六天工作制|六天制|每周\s*6\s*天|月休\s*[1-7]\s*天|"
    r"周六(?:上班|加班|需上班|要上班)|周日(?:上班|加班)|做六休一|995|996|997|"
    r"上班时间[:：]?\s*单休|排班|轮休|调休制度|不固定休息|休息不固定)"
)
WEEKEND_GOOD_RE = re.compile(r"(双休|周末双休|五天工作制|5天工作制|做五休二|朝九晚六双休)")
# 大小周/单休类（勾了「不要大小周」时使用）
BIGWEEK_RE = re.compile(
    r"(大小周|大周|小周|单休|六天工作制|六天制|每周\s*6\s*天|"
    r"月休\s*[1-7]\s*天|周六(?:上班|加班|需上班|要上班)|做六休一)"
)


def bigweek_hit(text: str | None) -> str | None:
    """返回命中的大小周/单休说法；没命中返回 None。"""
    if not text:
        return None
    cleaned = text.replace("非单休", "__NEG__").replace("无大小周", "__NEG__")
    m = BIGWEEK_RE.search(cleaned)
    return m.group(1) if m else None


def weekend_bad(text: str | None) -> str | None:
    """要求双休时，返回命中的"非双休"说法；没命中返回 None。"""
    if not text:
        return None
    # 先剔除否定语境：「非常重视双休」之类不算问题
    cleaned = text.replace("非双休", "__NEG__")
    m = WEEKEND_BAD_RE.search(cleaned)
    return m.group(1) if m else None


SHORT_TERM_RE = re.compile(
    r"\d+\s*[-–~至]\s*\d+\s*个?月|周期\s*\d+|短期项目|项目制|过渡岗"
)
SALARY_RE = re.compile(r"(\d{1,3})\s*[-–~至]\s*(\d{1,3})\s*[Kk千]")
# Boss 列表卡片把薪资数字换成私用区字符做反爬：U+E031→"0"、U+E032→"1" …（码点 - 0xE031）
_PUA_BASE = 0xE031


def decode_boss_salary(text: str | None) -> str:
    """把 Boss 卡片里的私用区字符还原成阿拉伯数字。

    列表页的 span.job-salary 长这样：`\ue032\ue037-\ue032\ue038K`，真实值是「16-17K」。
    码点 0xE031 对应 0，0xE032 对应 1，以此类推；不在这个区的字符原样保留。
    """
    if not text:
        return ""
    out = []
    for ch in str(text):
        code = ord(ch)
        if _PUA_BASE <= code <= _PUA_BASE + 9:
            out.append(str(code - _PUA_BASE))
        elif 0xE000 <= code <= 0xF8FF:
            continue  # 其它私用区字符（字体占位）直接丢掉
        else:
            out.append(ch)
    return "".join(out)
SALARY_WAN_RE = re.compile(r"(\d{1,2}(?:\.\d)?)\s*[-–~至]\s*(\d{1,2}(?:\.\d)?)\s*万")
EXP_REQUIRED_RE = re.compile(r"(\d{1,2})\s*年\s*(?:及|)\s*(?:以上|或以上|以上的)")

# HR 活跃度 -> 活跃于多少天内（None = 无法判断/更久）
HR_ACTIVE_DAYS: list[tuple[str, int]] = [
    ("在线", 0), ("刚刚", 0), ("今日活跃", 0), ("今日", 0), ("今天活跃", 0), ("今日回复", 0),
    ("3日内活跃", 3), ("三日内活跃", 3), ("3天前活跃", 3), ("3日", 3),
    ("7日内活跃", 7), ("一周内活跃", 7), ("本周活跃", 7), ("一周内", 7), ("7天前活跃", 7),
    ("2周内活跃", 14), ("14日内活跃", 14), ("半个月内活跃", 15),
    ("1月内活跃", 30), ("本月活跃", 30), ("一月内活跃", 30), ("30日内活跃", 30),
]


def hr_active_days(status: str | None) -> int | None:
    """把 Boss 的 HR 活跃度文案换算成「多少天内活跃」。"""
    if not status:
        return None
    text = str(status).strip()
    for key, days in HR_ACTIVE_DAYS:
        if key in text:
            return days
    import re as _re
    m = _re.search(r"(\d{1,3})\s*(天|日)内活跃", text)
    if m:
        return int(m.group(1))
    m = _re.search(r"(\d{1,3})\s*(小时|分钟)内活跃", text)
    if m:
        return 0
    if "今日" in text or "在线" in text:
        return 0
    return None


@dataclass
class ScoreResult:
    score: int          # 1-10
    source: str         # "llm" | "rule"
    reason: str
    tokens: int = 0     # 本次大模型调用消耗的 token（规则打分为 0）

    def as_dict(self) -> dict[str, Any]:
        return {
            "score": self.score,
            "source": self.source,
            "reason": self.reason,
            "tokens": self.tokens,
        }


# ======================================================================
# 规则打分
# ======================================================================

class RuleScorer:
    """与 Java 版 ScreenRule 对齐的规则打分：技能40 + 薪资20 + 经验15 + 方向15 + 公司质量10。"""

    def salary_out_of_range(self, text: str | None, ranges: list[tuple[int, int]] | None) -> bool:
        """岗位薪资与用户勾选的区间是否有交集；无交集返回 True。
        解析不出来时返回 False（宁可放行，不误杀）。"""
        if not ranges:
            return False
        parsed = self.parse_salary(text)
        if not parsed:
            return False
        low, high = parsed
        return not any(high >= r_low and low <= r_high for r_low, r_high in ranges)

    def parse_salary(self, text: str | None) -> tuple[int, int] | None:
        if not text:
            return None
        # 先解 Boss 的反爬字符，否则列表卡片的「16-17K」永远匹配不到
        text = decode_boss_salary(text)
        m = SALARY_RE.search(text)
        if m:
            return int(m.group(1)), int(m.group(2))
        m = SALARY_WAN_RE.search(text)
        if m:
            return int(float(m.group(1)) * 10), int(float(m.group(2)) * 10)
        return None

    # ---- 硬性排除（返回原因，None 表示通过） ----
    def reject(
        self,
        company: str | None,
        title: str | None,
        salary: str | None,
        experience: str | None,
        jd: str | None,
        rules: "ExcludeRules | None" = None,
        founded_years: int | None = None,
        hr_active_status: str | None = None,
    ) -> str | None:
        r = rules or ExcludeRules()
        # 双休规则：勾了「只投双休」时，JD/岗位名里出现单休、大小周、周六上班等就排除
        if r.weekend == "double":
            bad = weekend_bad(f"{title or ''} {jd or ''}")
            if bad:
                return f"非双休（JD/岗位名出现「{bad}」）"
        name_text = f"{company or ''} {title or ''}"
        for w in r.name_words:
            if w and w in name_text:
                return f"公司/岗位名命中排除词「{w}」"
        if title and SHORT_TERM_RE.search(title):
            return "岗位名显示短期项目/项目制"
        if jd:
            # 剔除否定语境后再判断外包类
            cleaned = re.sub(
                r"(非|不是|无需|拒绝|不做|不接)\s*(外包|驻场|派遣)", r"\1__NEG__", jd
            )
            for w in OUTSOURCE_WORDS:
                if w in cleaned:
                    return f"JD 命中外包/驻场类表述「{w}」"
            # 学历硬卡：只有用户勾选了学历门槛才判断
            for lv in r.exclude_degree:
                if not lv:
                    continue
                if re.search(rf"{re.escape(lv)}\s*(研究生)?\s*(及以上|以上|学历)", jd):
                    return f"JD 要求{lv}学历"
            if re.search(r"(985|211)\s*(院校|高校|大学)?\s*(毕业|及以上|统招|全日制)", jd):
                return "JD 硬卡 985/211"
            # 经验门槛：默认不限制（exclude_resume_max_years 为空即跳过）
            if r.exclude_resume_max_years:
                m = EXP_REQUIRED_RE.search(jd)
                if m and int(m.group(1)) > int(r.exclude_resume_max_years):
                    return f"JD 要求 {m.group(1)} 年以上经验（上限 {r.exclude_resume_max_years} 年）"
        # HR 活跃度：只有填了门槛才判断；无法判断活跃度的岗位按"不活跃"处理
        if r.exclude_hr_active_days is not None:
            days = hr_active_days(hr_active_status)
            limit = int(r.exclude_hr_active_days)
            if days is None:
                return f"HR 活跃度未知（要求 {limit} 天内活跃）"
            if days > limit:
                return f"HR 最近活跃 {hr_active_status}，超过 {limit} 天门槛"

        # 公司成立年限：只有填了门槛才判断
        if r.exclude_founded_below and founded_years is not None:
            if int(founded_years) < int(r.exclude_founded_below):
                return f"公司成立 {founded_years} 年，不足 {r.exclude_founded_below} 年"
        # 薪资下限 / 上限：只有填了门槛才判断
        if r.exclude_salary_min or r.exclude_salary_max:
            rng = self.parse_salary(salary)
            if rng is not None:
                low, high = rng
                if r.exclude_salary_min and high < int(r.exclude_salary_min):
                    return f"薪资 {low}-{high}K 低于下限 {r.exclude_salary_min}K"
                if r.exclude_salary_max and low > int(r.exclude_salary_max):
                    return f"薪资 {low}-{high}K 高于上限 {r.exclude_salary_max}K"
        return None

    # ---- 打分 ----
    def score(
        self,
        company: str | None,
        title: str | None,
        salary: str | None,
        experience: str | None,
        jd: str | None,
    ) -> ScoreResult:
        parts: list[str] = []
        total = 0

        # 详情页没打开时 jd 为空：技能分与公司分都会按「无 JD」的降权处理，
        # 总分会明显偏低，很容易低于分值门槛而被整批判掉。这里记下来，日志里好解释。
        no_jd = not (jd or "").strip()
        if no_jd:
            penalty = "（详情页没拿到 JD，按保守值打分）"
        else:
            penalty = ""

        s_skill = self._skill(jd)
        total += s_skill
        parts.append(f"技能 {s_skill}/40")

        s_salary = self._salary(salary)
        total += s_salary
        parts.append(f"薪资 {s_salary}/20")

        s_exp = self._experience(experience, jd)
        total += s_exp
        parts.append(f"经验 {s_exp}/15")

        s_dir = self._direction(title)
        total += s_dir
        parts.append(f"方向 {s_dir}/15")

        s_company = self._company(company, title, jd)
        total += s_company
        parts.append(f"公司质量 {s_company}/10")

        text_all = f"{company or ''} {title or ''} {jd or ''}"
        if any(w in text_all for w in OUTSOURCE_WORDS) or SHORT_TERM_RE.search(
            f"{title or ''} {jd or ''}"
        ):
            total //= 2
            penalty += "；外包/短期信号→总分减半"

        score = max(1, min(10, -(-total // 10)))
        return ScoreResult(
            score=score,
            source="rule",
            reason=" | ".join(parts) + penalty,
        )

    def _skill(self, jd: str | None) -> int:
        if not jd:
            return 12  # 无 JD 降权：无法判断，不判死
        text = jd.lower()
        weight = 0.0
        for s in PROFILE["core_skills"]:
            if s.lower() in text:
                weight += 1.0
        for s in PROFILE["bonus_skills"]:
            if s.lower() in text:
                weight += 0.5
        return int(min(1.0, weight / 5.0) * 40)

    def _salary(self, salary: str | None) -> int:
        rng = self.parse_salary(salary)
        if rng is None:
            return 10
        low, high = rng
        # 上限用 9999 这种写法时，high 会远大于期望区间，必须先夹到画像上限再算，
        # 否则 min(high, salary_high) 之后 hi-lo 反而可能溢出给出满分
        high = min(high, PROFILE["salary_high"])
        low = min(low, PROFILE["salary_high"])
        if high <= 0:
            return 10
        lo = max(low, PROFILE["salary_low"])
        hi = min(high, PROFILE["salary_high"])
        if hi <= lo:
            gap = low - PROFILE["salary_high"]
            if gap <= 5:
                return 10
            if gap <= 15:
                return 5
            return 2
        span = PROFILE["salary_high"] - PROFILE["salary_low"]
        return int((hi - lo) / span * 20)

    def _experience(self, exp: str | None, jd: str | None) -> int:
        text = f"{exp or ''} {jd or ''}"
        m = re.search(r"(\d{1,2})\s*[-]\s*(\d{1,2})\s*年", text)
        if m:
            lo, hi = int(m.group(1)), int(m.group(2))
            years = PROFILE["years"]
            if lo <= years <= hi:
                return 15 if lo >= 5 else 12
            if years > hi:
                return 8
            return 6
        if "不限" in text or "应届" in text:
            return 10
        m = EXP_REQUIRED_RE.search(text)
        if m:
            need = int(m.group(1))
            if need <= PROFILE["years"]:
                return 15 if need >= 5 else 10
            return 3
        return 10

    def _direction(self, title: str | None) -> int:
        t = (title or "").lower()
        if any(k in t for k in ("java", "后端", "服务端", "全栈", "backend")):
            return 15
        if any(k in t for k in ("node", "go", "python", "前端")):
            return 10
        return 7

    def _company(self, company: str | None, title: str | None, jd: str | None) -> int:
        text = f"{company or ''} {title or ''} {jd or ''}"
        if any(w in text for w in OUTSOURCE_WORDS):
            return 0
        if SHORT_TERM_RE.search(f"{title or ''} {jd or ''}"):
            return 0
        if jd and any(w in jd for w in ("离职期", "过渡岗", "短期项目")):
            return 4
        return 10


# ======================================================================
# 大模型打分
# ======================================================================

SYSTEM_PROMPT = (
    "你是一名资深技术招聘评估专家。请根据求职者画像，判断岗位与求职者的匹配度。\n"
    "只输出 JSON，不要任何解释文字，格式：{\"score\": 1-10 的整数, \"reason\": \"一句话中文理由\"}\n"
    "评分标准：10 分完美匹配；1 分完全不匹配。薪资、技术栈、经验年限、岗位方向、"
    "公司性质（外包/驻场/短期项目要显著扣分）都要考虑。"
)


def build_user_prompt(
    job: dict[str, Any],
    profile: dict[str, Any] | None = None,
    filters: str = "",
) -> str:
    pf = profile or PROFILE
    lines = [
        f"【求职者画像】{pf.get('summary') or PROFILE['summary']}",
        f"期望薪资 {pf.get('salary_low', PROFILE['salary_low'])}-"
        f"{pf.get('salary_high', PROFILE['salary_high'])}K，"
        f"期望城市 {pf.get('city', PROFILE['city'])}，"
        f"工作年限约 {pf.get('years', PROFILE['years'])} 年。",
    ]
    from .greeting import _profile_skills  # 兼容「配置→技能」里手填的字符串

    skills = _profile_skills(pf)
    lines.append(f"技能：{'、'.join(dict.fromkeys(skills))}")
    if filters:
        lines += ["", f"【我的筛选条件（投递偏好）】{filters}"]
    lines += [
        "",
        "【岗位信息】",
        f"平台：{job.get('platform')}",
        f"职位：{job.get('title')}",
        f"公司：{job.get('company')}",
        f"薪资：{job.get('salary')}",
        f"地点：{job.get('location')}",
        f"经验要求：{job.get('experience')}",
        f"学历要求：{job.get('degree')}",
        f"行业：{job.get('company_industry')}",
        f"公司规模：{job.get('company_scale')}",
        f"融资阶段：{job.get('company_stage')}",
        f"在招岗位数：{job.get('hiring_count')}",
        f"成立年限：{job.get('founded_years')}",
        f"HR 活跃度：{job.get('hr_active_status')}",
        "",
        "【JD 全文】",
        (job.get("jd_text") or "（该平台未提供 JD 详情）")[:6000],
    ]
    return "\n".join(lines)


def parse_llm_score(raw: str) -> ScoreResult | None:
    """从大模型输出里抽出 {score, reason}。容忍 markdown 代码块与多余文字。"""
    if not raw:
        return None
    text = raw.strip()
    # 去掉 ```json ... ``` 包裹
    fence = re.search(r"```(?:json)?\s*(.+?)```", text, re.S)
    if fence:
        text = fence.group(1).strip()
    # 找第一个平衡的 JSON 对象
    start = text.find("{")
    if start == -1:
        return None
    depth = 0
    for idx in range(start, len(text)):
        ch = text[idx]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                blob = text[start : idx + 1]
                try:
                    obj = json.loads(blob)
                except json.JSONDecodeError:
                    break
                return _score_from_obj(obj)
    # 输出被截断（finish_reason=length 很常见）：JSON 没闭合，但 score 通常已经出现了，
    # 直接从残缺文本里抠分，别把已经拿到的分数丢掉。
    return _score_from_partial(text[start:])


def _score_from_obj(obj: dict[str, Any]) -> ScoreResult | None:
    try:
        score = int(round(float(obj.get("score"))))
    except (TypeError, ValueError):
        return None
    score = max(1, min(10, score))
    reason = str(obj.get("reason") or "").strip()[:300]
    return ScoreResult(score=score, source="llm", reason=reason)


_SCORE_FIELD_RE = re.compile(r'"score"\s*[:：]\s*(\d{1,2})')
_REASON_FIELD_RE = re.compile(r'"reason"\s*[:：]\s*"(.+)$', re.S)


def _score_from_partial(text: str) -> ScoreResult | None:
    """从被截断的 JSON 里抠出 score 和（可能的半边）reason。"""
    m = _SCORE_FIELD_RE.search(text)
    if not m:
        return None
    score = max(1, min(10, int(m.group(1))))
    reason = ""
    rm = _REASON_FIELD_RE.search(text)
    if rm:
        # 去掉截断残留的半个转义 / 引号
        reason = rm.group(1).replace('\\n', " ").replace('\\"', '"').rstrip('"\\, ')
        reason = reason.strip()[:300]
    return ScoreResult(score=score, source="llm", reason=reason)


class LLMScorer:
    """OpenAI 兼容客户端封装（DeepSeek / OpenAI / 通义千问 / 本地 Ollama 均可）。"""

    def __init__(
        self,
        api_key: str,
        base_url: str | None = None,
        model: str = "deepseek-chat",
        timeout: float = 60.0,
    ) -> None:
        from openai import OpenAI  # 延迟导入，未配置大模型时不需要

        self.model = model
        self._client = OpenAI(
            api_key=api_key, base_url=base_url or None, timeout=timeout
        )

    def score(self, job: dict[str, Any], profile: dict[str, Any] | None = None,
              filters: str = "") -> ScoreResult:
        messages = [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": build_user_prompt(job, profile, filters)},
        ]
        resp = None
        raw = ""
        finish = ""
        # 中文 reason 动辄上百字，300 token 会被截断成不完整的 JSON（实测 deepseek-flash）；
        # 而且推理型模型（deepseek-flash / deepseek-reasoner）的思考 token 也算在
        # max_tokens 里，800 会被思考吃光 → content 为空。先给 1600，被截断再加一次 3200。
        for budget in (1600, 3200):
            resp = self._client.chat.completions.create(
                model=self.model,
                messages=messages,
                temperature=0.2,
                max_tokens=budget,
            )
            raw = (resp.choices[0].message.content or "").strip()
            finish = getattr(resp.choices[0], "finish_reason", "") or ""
            if raw or finish != "length":
                break
        parsed = parse_llm_score(raw)
        if parsed is None:
            raise ValueError(f"大模型返回无法解析为分数（finish={finish or '?'}）：{raw[:200] or '（空内容）'}")
        if finish == "length" and "（输出被截断，理由可能不完整）" not in parsed.reason:
            parsed.reason = (parsed.reason + "（输出被截断，理由可能不完整）")[:300]
        # 记录 token 消耗（用于界面累计展示）
        usage = getattr(resp, "usage", None)
        if usage is not None:
            parsed.tokens = int(getattr(usage, "total_tokens", 0) or 0)
        return parsed


# ======================================================================
# 统一入口
# ======================================================================

class Scorer:
    """按配置自动选择大模型或规则打分；大模型失败时自动回退规则。"""

    def __init__(self, llm: LLMScorer | None = None, rules: "ExcludeRules | None" = None,
                 use_llm: bool = False) -> None:
        self.rule = RuleScorer()
        self.llm = llm
        self.rules = rules or ExcludeRules()
        # 默认规则打分（快、不花钱）；界面打开开关后才走大模型
        self.use_llm = bool(use_llm)
        self.profile: dict[str, Any] = PROFILE
        self.filters = ""
        # 大模型打分前的规则初筛线：规则分低于它的直接判掉，不浪费 token（0=不设）
        self.llm_prefilter = 5

    def reject(self, job: dict[str, Any]) -> str | None:
        return self.rule.reject(
            job.get("company"), job.get("title"), job.get("salary"),
            job.get("experience"), job.get("jd_text"),
            rules=self.rules, founded_years=job.get("founded_years"),
            hr_active_status=job.get("hr_active_status"),
        )

    def _rule_score(self, job: dict[str, Any]) -> ScoreResult:
        return self.rule.score(
            job.get("company"),
            job.get("title"),
            job.get("salary"),
            job.get("experience"),
            job.get("jd_text"),
        )

    def score(self, job: dict[str, Any]) -> ScoreResult:
        """两种打分方式二选一，只有一方的分生效。

        - 规则模式：直接用规则分。
        - 大模型模式：先用规则分做初筛，低于初筛线的直接判掉（不花 token），
          过了初筛就**只采用大模型的分**，最终分数以模型为准。
        """
        rule_res = self._rule_score(job)
        if self.llm is None or not self.use_llm:
            return rule_res
        # 两种模式共用的第一关：规则初筛（抓 JD 之前已经过了敏感库/关键词/硬性排除）
        if self.llm_prefilter > 0 and rule_res.score < self.llm_prefilter:
            rule_res.reason = f"[规则初筛 {rule_res.score} 分 < {self.llm_prefilter}，未调用大模型] {rule_res.reason}"
            return rule_res
        try:
            return self.llm.score(job, self.profile, self.filters)
        except Exception as exc:  # 大模型不可用时退回规则分，保证流程不断
            rule_res.reason = f"[大模型失败:{type(exc).__name__}] {rule_res.reason}"
            return rule_res

    def score(self, job: dict[str, Any]) -> ScoreResult:
        """规则打分做初筛，过了初筛再交给大模型细打分。

        初筛不过（明显不匹配）就不花 token 调大模型，直接按规则分入库/排除；
        初筛过了才用「简历 + 配置技能 + 筛选条件 + JD 全文」让大模型打分。
        大模型失败时回退到规则分，流程不断。
        """
        rule_res = self._rule_score(job)
        if self.llm is None or not self.use_llm:
            return rule_res
        if self.llm_prefilter > 0 and rule_res.score < self.llm_prefilter:
            rule_res.reason = f"[规则初筛 {rule_res.score} 分 < {self.llm_prefilter}，未调用大模型] {rule_res.reason}"
            return rule_res
        try:
            llm_res = self.llm.score(job, self.profile, self.filters)
            if rule_res.score != llm_res.score:
                llm_res.reason = f"{llm_res.reason}（规则初筛 {rule_res.score} 分）"
            return llm_res
        except Exception as exc:  # 网络/额度/解析失败都回退，保证流程不断
            rule_res.reason = f"[大模型失败:{type(exc).__name__}] {rule_res.reason}"
            return rule_res
