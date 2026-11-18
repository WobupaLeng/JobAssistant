"""Playwright 抓取器基类。

设计要点（PRD 第 5 节）：
- 接管本地已登录的 Chrome/Edge（CDP 调试端口），复用扫码登录态，规避登录风控。
- 独立后台线程运行，主线程不阻塞；日志通过回调推给 UI。
- 随机延时 + 单日上限，降低被识破概率。
"""
from __future__ import annotations

import random
import re
import threading
import time
from dataclasses import dataclass, field
from typing import Any, Callable

# 日志回调签名：(level, message) -> None
LogFn = Callable[[str, str], None]


# 关键字分隔符：空格、中英文顿号/逗号/分号、竖线、斜杠、换行、制表符
_SPLIT_RE = re.compile(r"[\s,，、;；|/\\·\u3000]+")


def split_keywords(values: Any) -> list[str]:
    """把「Java 后端,Go、全栈；Node.js」这类输入拆成去重后的关键词列表。

    支持空格 / 中英文逗号 / 顿号 / 分号 / 竖线 / 斜杠 / 换行等任意分隔符；
    也兼容前端直接传数组的情况。
    """
    raw: list[str] = []
    if isinstance(values, str):
        raw = [values]
    elif isinstance(values, (list, tuple, set)):
        for v in values:
            if isinstance(v, str):
                raw.append(v)
    out: list[str] = []
    for chunk in raw:
        for part in _SPLIT_RE.split(chunk):
            part = part.strip()
            if part and part not in out:
                out.append(part)
    return out

# 兼容旧字段 keyword 的历史默认值（用户没填关键词时不应该真的去搜它）
DEFAULT_KEYWORD = "Java"

DEFAULT_UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
)


@dataclass
class ScrapeConfig:
    """一次抓取任务的参数，对应 UI 左侧面板的筛选条件。

    全部按 Boss 直聘岗位列表页的**真实筛选项**建模，跟界面上一模一样：
    薪资 / 经验 / 学历 / 公司规模 / 融资阶段 都是**多选**，值之间是 OR 关系；
    同一个维度内选多项时，Boss 的 URL 参数是重复键（salary=405&salary=406）。
    """

    keywords: list[str] = field(default_factory=list)   # 多关键词（兼容旧字段 keyword）
    exclude_keywords: list[str] = field(default_factory=lambda: ["外包", "驻场", "销售"])
    keyword: str = ""

    # ---- 与 Boss 官网一致的筛选项，值都是 Boss 的编码 ----
    city: str = "101020100"                              # 上海
    # 薪资：403=3K以下 404=3-5K 405=5-10K 406=10-15K 407=15-20K 408=20-30K
    #       409=30-50K 410=50K以上；另有多档：412=5-8K 413=8-12K 414=12-20K
    #       415=20-35K 416=35-50K 417=50-70K 418=70K以上
    salary: list[str] = field(default_factory=list)      # 多选，如 ["406","407"]
    experience: list[str] = field(default_factory=list)  # 101=在校/应届 102=经验不限 103=<1年 104=1-3年 105=3-5年 106=5-10年 107=10年以上
    degree: list[str] = field(default_factory=list)      # 202=大专 203=本科 204=硕士 205=博士 206=中专/中技 208=高中 209=初中及以下
    scale: list[str] = field(default_factory=list)       # 301=<20人 302=20-99 303=100-499 304=500-999 305=1000-9999 306=10000+
    stage: list[str] = field(default_factory=list)       # 801=未融资 802=天使轮 803=A轮 804=B轮 805=C轮 806=D轮及以上 807=已上市 808=不需要融资
    industry: list[str] = field(default_factory=list)    # 行业编码（如 100020=计算机软件），多选
    job_type: str = ""                                   # 1901=全职 1902=兼职 1903=实习

    max_pages: int = 5
    daily_limit: int = 100          # 候选池上限（抓到的原始岗位条目数）
    target_count: int = 0           # 目标入库数（0=不限制）；用于决定翻不翻页
    delay_min: float = 1.5         # 秒；反爬随机延时下限
    delay_max: float = 9.8         # 秒；反爬随机延时上限（1.5~9.8s = 1500~9800ms）

    def keyword_list(self) -> list[str]:
        """返回要搜索的关键词列表。keywords 为空时退回旧字段 keyword；
        两个都没有就返回空（调用方应据此报错，而不是偷偷搜默认词）。"""
        ks = split_keywords(self.keywords)
        if not ks and self.keyword:
            if self.keyword != DEFAULT_KEYWORD:
                ks = split_keywords(self.keyword)
        return ks


class StopFlag:
    """跨线程 停止 / 暂停 信号。

    - stop()   彻底结束本轮抓取：所有循环判断都会退出
    - pause()  只是挂起：正在等待的位置原地阻塞，页面保持在当前 JD，
               方便用户切到浏览器里观察页面细节；resume() 继续
    """

    def __init__(self) -> None:
        self._ev = threading.Event()      # 停止
        self._pause = threading.Event()   # 暂停
        self._resume = threading.Event()  # 恢复通知
        self._resume.set()

    # ---- 停止 ----
    def stop(self) -> None:
        self._ev.set()
        self._resume.set()  # 别让暂停把停止卡住

    # 兼容写法：有些调用点写成 stop.set()
    set = stop

    def is_set(self) -> bool:
        return self._ev.is_set()

    def clear(self) -> None:
        self._ev.clear()

    # ---- 暂停 ----
    def pause(self) -> None:
        self._pause.set()
        self._resume.clear()

    def resume(self) -> None:
        self._pause.clear()
        self._resume.set()

    def is_paused(self) -> bool:
        return self._pause.is_set()

    def wait_if_paused(self, poll: float = 0.25) -> None:
        """若处于暂停态就原地等待，直到恢复或停止。"""
        while self._pause.is_set() and not self._ev.is_set():
            self._resume.wait(poll)

    def sleep(self, seconds: float) -> bool:
        """睡眠；若中途被要求停止则提前返回 True。暂停期间也会等住。"""
        end = time.time() + seconds
        while True:
            self.wait_if_paused()
            if self._ev.is_set():
                return True
            left = end - time.time()
            if left <= 0:
                return False
            # 用小步长睡眠，保证暂停/停止能及时生效
            if self._ev.wait(min(left, 0.2)):
                return True


class BaseScraper:
    platform = "base"

    def __init__(self, cfg: ScrapeConfig, stop: StopFlag, log: LogFn) -> None:
        self.cfg = cfg
        self.stop = stop
        self.log = log
        self.collected = 0
        # 已真正入库的条数：由 app.main 在处理每条岗位时回写，
        # 用于「入库数达到目标就停止翻页」（被排除的不算）
        self.arrived = 0

    # ---------- 供子类使用的工具 ----------
    def info(self, msg: str) -> None:
        self.log("info", f"[{self.platform}] {msg}")

    def warn(self, msg: str) -> None:
        self.log("warn", f"[{self.platform}] {msg}")

    def error(self, msg: str) -> None:
        self.log("error", f"[{self.platform}] {msg}")

    def jitter(self, lo: float | None = None, hi: float | None = None) -> float:
        """随机延时（反爬）：默认 1500~9800ms，可临时指定区间；返回实际延时秒数。

        注意：睡眠是**可中断**的（StopFlag.sleep 内部按 0.2 秒切片并检查停止位），
        用户点「停止抓取」最多 0.2 秒内就会返回，不会被随机等待拖住。
        """
        a = self.cfg.delay_min if lo is None else lo
        b = self.cfg.delay_max if hi is None else hi
        if b < a:
            a, b = b, a
        secs = random.uniform(a, b)
        self.stop.sleep(secs)          # 可中断：停止/暂停标志会提前打破等待
        if self.should_stop():
            return 0.0
        return secs

    def should_stop(self) -> bool:
        return self.stop.is_set()

    # ---------- 子类必须实现 ----------
    def run(self, page: Any) -> list[dict[str, Any]]:
        """在已就绪的 page 上执行抓取，返回岗位 dict 列表。"""
        raise NotImplementedError

    # ---------- 通用流程 ----------
    def read_expectations(self, ctx: Any) -> list[dict[str, Any]]:
        """读取该平台账号里配置的「期望职位」（子类实现，默认返回空）。"""
        return []

    def scrape(self, ctx: Any, on_job: Any = None) -> list[dict[str, Any]]:
        """ctx 是 patchright 的 BrowserContext；需要页面时自己开一个。

        patchright 的持久化上下文可能一个页面都没有（用户关过窗口），
        所以这里统一 new_page()，用完关闭。

        on_job: 可选回调——抓到一条（含 JD 详情）就立刻交给它入库打分，
        不必等整页/整轮抓完；返回 True 表示已达标，可以提前停止。
        """
        try:
            page = ctx.new_page()
            # 让抓取页始终显示在最前：用户能实时看到"正在看哪条 JD"，
            # 也避免后台标签页被 Chrome 节流导致定时器/渲染异常
            try:
                page.bring_to_front()
            except Exception:
                pass
            page.set_default_timeout(60_000)
            try:
                jobs = self.run(page, on_job=on_job) or []
            finally:
                try:
                    page.close()
                except Exception:
                    pass
            self.collected = len(jobs)
            self.info(f"采集完成，共 {self.collected} 条")
            return jobs
        except Exception as exc:  # 单平台失败不影响其他平台
            self.error(f"抓取异常：{type(exc).__name__}: {exc}")
            return []


def now_ms() -> int:
    return int(time.time() * 1000)
