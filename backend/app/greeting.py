"""AI 生成打招呼语。

两份数据来源：
1. 用户在「模型配置」里写的提示词（prompt）——决定语气、长度、需要强调什么；
2. 岗位本身（标题 / 公司 / 薪资 / 岗位详情 JD）。

再叠加「我拥有的技能」——优先用岗位抓取时 JD 里提到的技能和我画像里匹配上的，
保证生成的话术是这个岗位看得懂的，而不是一份通用模板。

大模型不可用（没配 key / 网络失败 / 返回空）时退回规则模板，并明确标注来源，
避免界面上一片空白。
"""
from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any

# 默认提示词：用户不改也能直接生成
DEFAULT_PROMPT = (
    "你是求职者的助理，要根据岗位 JD 帮他写一句发给 HR 的打招呼消息。\n"
    "要求：\n"
    "1. 只说与这个岗位最匹配的 2~3 项技能或经历，不要罗列全部简历；\n"
    "2. 语气自然、礼貌，像真人发的第一句话，不要客套套话堆砌；\n"
    "3. 控制在 60~120 字，一到两句话，不要分点、不要换行、不要加称呼以外的废话；\n"
    "4. 直接输出消息正文，不要任何解释、前缀、引号或 markdown。"
)

# 已知技能词表：用来从 JD 里捞出「这个岗位在意什么」
_SKILL_WORDS = [
    "Java", "Python", "Go", "Golang", "Node", "Node.js", "C++", "C#", "PHP",
    "JavaScript", "TypeScript", "Vue", "React", "Angular", "uni-app", "Electron",
    "Spring", "Spring Boot", "Spring Cloud", "SpringBoot", "MyBatis", "MyBatis-Plus",
    "MySQL", "Oracle", "PostgreSQL", "SQL Server", "MongoDB", "Redis", "Elasticsearch",
    "RabbitMQ", "Kafka", "RocketMQ", "Nginx", "Docker", "Kubernetes", "K8s",
    "Linux", "Git", "Jenkins", "CI/CD", "微服务", "分布式", "高并发", "多线程",
    "大模型", "LLM", "AIGC", "RAG", "Agent", "Prompt", "LangChain",
    "人工智能", "机器学习", "深度学习", "向量", "知识库", "智能客服",
    "全栈", "小程序", "H5", "低代码", "SaaS", "多租户", "支付", "对账",
    "数据采集", "爬虫", "自动化", "运维", "性能优化", "重构", "架构",
]

_WS_RE = re.compile(r"[\s\u3000]+")


@dataclass
class Greeting:
    text: str
    tokens: int
    source: str  # llm | rule
    note: str = ""


def _profile_skills(profile: dict[str, Any]) -> list[str]:
    out: list[str] = []
    # 界面「配置 → 技能」里手填的，优先级最高
    raw = profile.get("skills")
    if isinstance(raw, str):
        raw = re.split(r"[,，、;；/\n]+", raw)
    for s in raw or []:
        if isinstance(s, str) and s.strip() and s.strip() not in out:
            out.append(s.strip())
    for k in ("core_skills", "bonus_skills"):
        for s in profile.get(k) or []:
            if isinstance(s, str) and s.strip() and s.strip() not in out:
                out.append(s.strip())
    return out


def matched_skills(job: dict[str, Any], profile: dict[str, Any]) -> list[str]:
    """岗位 JD 里和我技能重合的部分（按我的画像顺序，便于话术自然）。"""
    mine = _profile_skills(profile)
    blob = " ".join(
        str(job.get(k) or "")
        for k in ("title", "jd_text", "company_intro", "company_industry")
    ).lower()
    if not blob.strip():
        return mine[:3]
    hit = [s for s in mine if s.lower() in blob]
    if hit:
        return hit[:6]
    # 一个都没命中时，退而求其次：JD 里出现过的通用技能词
    found = [w for w in _SKILL_WORDS if w.lower() in blob]
    return found[:4] or mine[:3]


def profile_with_skills(profile: dict[str, Any], skills_text: str | None) -> dict[str, Any]:
    """把「配置」里手填的技能合进画像。技能留空时退回简历里自带的技能。"""
    out = dict(profile or {})
    text = (skills_text or "").strip()
    if text:
        out["skills"] = text
    else:
        out.pop("skills", None)
    return out


def build_prompt(user_prompt: str | None, job: dict[str, Any], profile: dict[str, Any]) -> tuple[str, str]:
    """返回 (system, user)。system 用用户提示词，user 装岗位与技能事实。"""
    system = (user_prompt or "").strip() or DEFAULT_PROMPT
    skills = matched_skills(job, profile)
    jd = _WS_RE.sub(" ", str(job.get("jd_text") or "")).strip()
    if len(jd) > 1200:
        jd = jd[:1200] + "…"
    facts = [
        f"求职者：{profile.get('years', '')} 年经验，方向 {profile.get('title', '')}",
        f"掌握的技能：{'、'.join(_profile_skills(profile))}",
        f"与该岗位匹配上的技能：{'、'.join(skills) if skills else '（未直接命中，按上面的技能概括）'}",
        f"岗位标题：{job.get('title') or ''}",
        f"公司：{job.get('company') or ''}",
        f"薪资：{job.get('salary') or '未标注'}",
        f"经验要求：{job.get('experience') or '未标注'}",
    ]
    if job.get("jd_text"):
        facts.append(f"岗位详情（截断）：{jd}")
    else:
        facts.append("岗位详情：无（只能根据标题与公司信息写）")
    return system, "\n".join(facts)


def _clean(text: str) -> str:
    t = (text or "").strip()
    # 模型偶尔会加引号/前缀
    t = re.sub(r"^```[a-zA-Z]*\s*|\s*```$", "", t).strip()
    t = t.strip("\"'“”‘’「」")
    t = t.split("\n")[0].strip()
    return t


def rule_greeting(job: dict[str, Any], profile: dict[str, Any]) -> str:
    """大模型不可用时的兜底话术：技能 + 岗位名，尽量不像模板。"""
    skills = matched_skills(job, profile)[:3]
    title = str(job.get("title") or "").strip()
    years = profile.get("years") or ""
    tech = "、".join(skills) if skills else "后端与全栈"
    if title:
        return (
            f"您好，我在{tech}方向有{years}年经验，看到贵司在招「{title}」，"
            f"和我的技术栈比较匹配，想和您聊聊岗位细节，方便发一份简历给您吗？"
        )
    return (
        f"您好，我在{tech}方向有{years}年经验，看到贵司有合适的岗位，"
        f"想和您聊聊，方便发一份简历给您吗？"
    )


def empty_content_reason(resp: Any, finish: str = "") -> str:
    """把「空内容」翻译成人能看懂的失败原因（尤其是推理型模型的思考挤占预算）。

    实测 deepseek-flash：max_tokens=400 时 completion_tokens_details.reasoning_tokens
    可以正好等于 400，content 为空、finish_reason=length。
    """
    usage = getattr(resp, "usage", None)
    details = getattr(usage, "completion_tokens_details", None) if usage is not None else None
    reasoning = getattr(details, "reasoning_tokens", None) if details is not None else None
    if finish == "length":
        spent = f"，其中思考约 {reasoning} tokens" if reasoning else ""
        return f"大模型返回了空内容（输出预算被思考过程耗尽{spent}；可在设置里换非推理模型，或调大最大输出）"
    if reasoning:
        return f"大模型返回了空内容（思考了约 {reasoning} tokens 但正文为空，finish={finish or '?'}）"
    return f"大模型返回了空内容（finish={finish or '?'}）"


class GreetingWriter:
    """调用 OpenAI 兼容接口生成打招呼语，返回正文 + token 消耗。"""

    def __init__(self, llm: Any) -> None:
        # llm 就是 scorer.LLMScorer（复用同一个 client / model / base_url）
        self.llm = llm

    @property
    def enabled(self) -> bool:
        return self.llm is not None

    def write(self, user_prompt: str | None, job: dict[str, Any], profile: dict[str, Any]) -> Greeting:
        if self.llm is None:
            return Greeting(text=rule_greeting(job, profile), tokens=0, source="rule", note="未配置大模型，使用规则话术")
        system, user = build_prompt(user_prompt, job, profile)
        try:
            # 输出预算：**推理型模型**（deepseek-flash / deepseek-reasoner 等）会先写
            # reasoning_content，思考 token 也算进 max_tokens。原来给 400，实测思考就
            # 吃掉 400 → content 为空、finish_reason=length，界面报「大模型返回了空内容」。
            # 所以先给 1200；万一还是被截断，再加一次 3000 的预算重试。
            resp = None
            raw = ""
            finish = ""
            for budget in (1200, 3000):
                resp = self.llm._client.chat.completions.create(
                    model=self.llm.model,
                    messages=[
                        {"role": "system", "content": system},
                        {"role": "user", "content": user},
                    ],
                    temperature=0.7,
                    max_tokens=budget,
                )
                raw = (resp.choices[0].message.content or "").strip()
                finish = getattr(resp.choices[0], "finish_reason", "") or ""
                # 有正文就收；或者不是「被预算截断」，说明模型确实什么都没写，重试也没用
                if raw or finish != "length":
                    break
            usage = getattr(resp, "usage", None)
            tokens = int(getattr(usage, "total_tokens", 0) or 0) if usage is not None else 0
            text = _clean(raw)
            if not text:
                raise ValueError(empty_content_reason(resp, finish))
            return Greeting(text=text, tokens=tokens, source="llm")
        except Exception as exc:  # noqa: BLE001 - 生成失败要给用户可见的原因
            return Greeting(
                text=rule_greeting(job, profile),
                tokens=0,
                source="rule",
                note=f"大模型生成失败（{type(exc).__name__}: {exc}），已用规则话术代替",
            )
