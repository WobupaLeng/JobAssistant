"""抓取参数模型。从 `backend/scrapers/base.py` 平移，只保留配置对象。

**刻意删掉的**：`StopFlag` 类（sidecar 用回调 + threading.Event 实现，更轻）、
`BaseScraper` 基类（只有一个平台，抽象层没有第二个实现来证明它值得存在）。

筛选值全部是 Boss 自己的编码，与官网 URL 参数一一对应——直接照搬用户界面上的
筛选项，不做任何转译，避免「界面选 15-20K、实际按 12-20K 搜」这类偏差。
"""
from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any

# 关键字分隔符：空格、中英文顿号/逗号/分号、竖线、斜杠、换行、制表符
_SPLIT_RE = re.compile(r"[\s,，、;；|/\\·\u3000]+")

# 兼容旧字段的历史默认值（用户没填关键词时不应该真的去搜它）
DEFAULT_KEYWORD = "Java"


def split_keywords(values: Any) -> list[str]:
    """把「Java 后端,Go、全栈；Node.js」这类输入拆成去重后的关键词列表。"""
    raw: list[str] = []
    if isinstance(values, str):
        raw = [values]
    elif isinstance(values, (list, tuple, set)):
        raw = [v for v in values if isinstance(v, str)]
    out: list[str] = []
    for chunk in raw:
        for part in _SPLIT_RE.split(chunk):
            part = part.strip()
            if part and part not in out:
                out.append(part)
    return out


@dataclass
class ScrapeConfig:
    """一次抓取任务的参数。

    薪资 / 经验 / 学历 / 规模 / 融资 都是**多选**：同一维度内多项是 OR 关系，
    Boss 的 URL 里对应重复键（salary=405&salary=406）。
    """

    keywords: list[str] = field(default_factory=list)
    keyword: str = ""

    city: str = "101020100"            # 上海
    salary: list[str] = field(default_factory=list)      # 403=3K以下 ... 418=70K以上
    experience: list[str] = field(default_factory=list)  # 101=应届 ... 107=10年以上
    degree: list[str] = field(default_factory=list)      # 202=大专 ... 209=初中及以下
    scale: list[str] = field(default_factory=list)       # 301=<20人 ... 306=10000+
    stage: list[str] = field(default_factory=list)       # 801=未融资 ... 808=不需要融资
    industry: list[str] = field(default_factory=list)
    job_type: str = ""                                   # 1901=全职 1902=兼职 1903=实习

    max_pages: int = 5                 # 无限滚动的批次数（每批约 15 张卡片）
    max_cards: int = 100               # 单次运行的卡片上限（滚动加载到此为止）
    daily_limit: int = 100             # 候选池上限（原始岗位条目数），与 max_cards 取小
    delay_min: float = 1.5             # 秒；反爬随机延时下限
    delay_max: float = 9.8             # 秒；上限（1.5~9.8s = 1500~9800ms）

    def keyword_list(self) -> list[str]:
        """返回要搜索的关键词列表；keywords 为空时退回旧字段 keyword。"""
        ks = split_keywords(self.keywords)
        if not ks and self.keyword and self.keyword != DEFAULT_KEYWORD:
            ks = split_keywords(self.keyword)
        return ks
