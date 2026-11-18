"""FastAPI 后端：把抓取、打分、存储串起来，供 Tauri 前端调用。

启动时由 Tauri 作为 sidecar 拉起，监听 127.0.0.1 的本机端口。
"""
from __future__ import annotations

import json
import os
import re
import socket
import sys
import threading
import time
import traceback
from pathlib import Path
from typing import Any

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

from .scorer import ExcludeRules, LLMScorer, PROFILE, Scorer
from .store import DB_NAME, Store
from scrapers.base import ScrapeConfig, StopFlag, split_keywords
from scrapers.boss import BossScraper

SCRAPERS = {"boss": BossScraper}


def app_data_dir() -> Path:
    """数据目录：打包后放在 exe 同级的 data/，开发时放项目根的 data/。"""
    env = os.environ.get("AJA_DATA_DIR")
    if env:
        p = Path(env)
    elif getattr(sys, "frozen", False):
        p = Path(sys.executable).parent / "data"
    else:
        p = Path(__file__).resolve().parents[2] / "data"
    p.mkdir(parents=True, exist_ok=True)
    return p


# ----------------------------------------------------------------------
# 全局运行时
# ----------------------------------------------------------------------

class Runtime:
    def __init__(self) -> None:
        self.store = Store(app_data_dir() / DB_NAME)
        self.scorer = self._build_scorer()
        self.stop = StopFlag()
        self.thread: threading.Thread | None = None
        self.running = False
        self.platforms: list[str] = []
        self.logs: list[dict[str, Any]] = []
        self.log_lock = threading.Lock()
        self.progress = {"collected": 0, "scored": 0, "filtered": 0, "total": 0, "tokens": 0}
        # 有效 JD 上限（0 = 不限制），抓取中改动即时生效
        self.max_jd = 0        # 有效 JD 上限（0=不限制，界面可改）
        # 本次抓取勾选的 Boss 薪资编码（用于入库前二次校验）
        self.salary_filter: list[str] = []
        # 分值门槛：匹配度低于该分值的 JD 不入库（0/1 = 不限制）
        self.min_score = 7       # 默认分值门槛 7 分，低于它的 JD 不入库
        try:
            self.min_score = int(self.store.get_config("min_score", "7") or 7)
            self.max_jd = int(self.store.get_config("max_jd", "0") or 0)
        except Exception:
            pass
        self.logins: dict[str, Any] = {}
        # 常驻 patchright 浏览器（登录态 + 抓取共用同一个持久化 profile）
        self._pw = None
        self._ctx = None
        self._pw_lock = threading.Lock()
        import queue as _queue

        self._task: _queue.Queue = _queue.Queue()
        # 采集线程 = 唯一允许启动/使用 patchright 的线程
        self.worker_thread: threading.Thread | None = threading.Thread(
            target=self._worker_loop, name="aja-worker", daemon=True
        )
        self.worker_thread.start()

    # ---------- 采集线程（patchright 的唯一宿主）----------
    def _worker_loop(self) -> None:
        while True:
            task = self._task.get()
            if task is None:
                return
            fn, args = task
            name = getattr(fn, "__name__", "task")
            try:
                self.log("info", f"采集线程开始执行：{name}")
            except Exception:
                pass
            try:
                fn(*args)
            except BaseException as exc:  # 单条任务出任何问题都不能把采集线程打死
                try:
                    self.log("error", f"任务异常：{exc}")
                except Exception:
                    pass

    def submit(self, fn, *args) -> None:
        """把任务丢进采集线程执行。"""
        self._task.put((fn, args))

    # ---------- 常驻 patchright 浏览器 ----------
    def run_in_worker(self, fn, *args, timeout: float = 180.0):
        """把 fn(*args) 投递到采集线程执行并等结果（浏览器只能在该线程里碰）。"""
        box = {}
        done = threading.Event()

        def _wrap():
            try:
                box["res"] = fn(*args)
            except Exception as exc:
                box["err"] = exc
            finally:
                done.set()

        self.submit(_wrap)
        if not done.wait(timeout):
            raise RuntimeError("采集线程执行超时（浏览器任务可能被阻塞，请重试或重启程序）")
        if "err" in box:
            raise box["err"]
        return box.get("res")

    def ensure_browser_on_worker(self):
        """给登录线程用：在采集线程里拿到（必要时启动）浏览器上下文。"""
        return self.run_in_worker(self.ensure_browser)

    def ensure_browser(self):
        """返回常驻的 patchright 持久化上下文，必要时启动。

        登录和抓取共用这一个上下文（同一个 profile），这是复刻 Java 项目
        能绕过 Boss 风控的关键：跨浏览器搬 cookie 会被判定为环境异常。
        """
        # patchright sync API 绑定创建它的线程：只有本线程（采集线程）可以创建/使用浏览器。
        # 其他线程（HTTP/登录线程）必须先通过 rt.run_in_worker(...) 投递到本线程执行，
        # 见 Runtime.call_on_worker / Runtime.ensure_browser_on_worker。

        with self._pw_lock:
            if self._ctx is not None:
                from .browser import context_alive

                if context_alive(self._ctx):
                    return self._ctx
                self.log("warn", "浏览器已失效（窗口被关闭或连接断开），重新启动")
                self._shutdown_browser_locked()
            from patchright.sync_api import sync_playwright

            from .browser import launch_context

            self._pw = sync_playwright().start()
            try:
                self._ctx = launch_context(self._pw, app_data_dir(), self.log)
            except Exception:
                # 启动失败必须把 playwright 也停掉，否则下次 ensure_browser 会
                # 在一个已经坏掉的连接上重试，报 "Playwright connection closed"。
                try:
                    self._pw.stop()
                except Exception:
                    pass
                self._pw = None
                self._ctx = None
                raise
            return self._ctx

    # ---- 浏览器必须在采集线程里启动/使用（patchright sync API 绑定 event loop）----

    def set_worker(self) -> None:
        self.worker_thread = threading.current_thread()

    def _shutdown_browser_locked(self) -> None:
        try:
            if self._ctx is not None:
                self._ctx.close()
        except Exception:
            pass
        try:
            if self._pw is not None:
                self._pw.stop()
        except Exception:
            pass
        self._ctx = None
        self._pw = None

    def shutdown_browser(self) -> None:
        with self._pw_lock:
            self._shutdown_browser_locked()

    # ---- 大模型可选 ----
    def exclude_rules(self) -> "ExcludeRules":
        """读取用户配置的排除规则（没配的项用默认值：不排除）。"""
        raw = self.store.get_config_json("exclude_rules", None)
        rules = ExcludeRules()
        if not isinstance(raw, dict):
            return rules
        try:
            if isinstance(raw.get("name_words"), list):
                rules.name_words = [str(w).strip() for w in raw["name_words"] if str(w).strip()]
            rules.exclude_salary_min = _to_int(raw.get("exclude_salary_min"))
            rules.exclude_salary_max = _to_int(raw.get("exclude_salary_max"))
            if isinstance(raw.get("exclude_degree"), list):
                rules.exclude_degree = [str(w).strip() for w in raw["exclude_degree"] if str(w).strip()]
            rules.exclude_founded_below = _to_int(raw.get("exclude_founded_below"))
            rules.exclude_resume_max_years = _to_int(raw.get("exclude_resume_max_years"))
            rules.exclude_hr_active_days = _to_int(raw.get("exclude_hr_active_days"))
            wk = str(raw.get("weekend") or "").strip()
            rules.weekend = wk if wk in ("double", "no_bigweek") else None
        except Exception:
            pass
        return rules

    def _build_scorer(self) -> Scorer:
        rules = self.exclude_rules()
        key = self.store.get_config("api_key") or ""
        if not key.strip():
            return Scorer(None, rules)
        try:
            return Scorer(
                LLMScorer(
                    api_key=key.strip(),
                    base_url=self.store.get_config("base_url") or None,
                    model=self.store.get_config("model") or "deepseek-chat",
                ),
                rules,
            )
        except Exception:
            return Scorer(None, rules)

    def rebuild_scorer(self) -> None:
        self.scorer = self._build_scorer()

    # ---- 日志 ----
    def log(self, level: str, message: str) -> None:
        entry = {"ts": time.time(), "level": level, "message": message}
        with self.log_lock:
            self.logs.append(entry)
            if len(self.logs) > 2000:
                del self.logs[:500]
        if level in ("info", "warn", "error"):
            # 打包成窗口程序（PyInstaller console=False / Tauri 释放的后端）时
            # sys.stdout 为 None，print() 会抛
            #   OSError: [Errno 22] Invalid argument
            # 而这里是从抓取/登录线程里调用的，异常会把整条登录流程带崩，
            # 表面症状就是"窗口已弹出然后没了"。所以 stdout 不可用时直接跳过。
            stream = sys.stdout
            if stream is None:
                return
            try:
                print(f"[{level}] {message}", file=stream, flush=True)
            except Exception:
                # 控制台编码/句柄异常同样不能影响业务线程
                pass

    def recent_logs(self, since: float = 0.0) -> list[dict[str, Any]]:
        with self.log_lock:
            return [e for e in self.logs if e["ts"] > since]


RT: Runtime | None = None


def _to_int(v: Any) -> int | None:
    """把界面传来的空串/None 统一当成"不设置"。"""
    if v is None or (isinstance(v, str) and not v.strip()):
        return None
    try:
        return int(float(v))
    except (TypeError, ValueError):
        return None


# 「读取期望职位」的异步任务状态：{status: idle|running|done|error, items, error, ts}


def get_rt() -> Runtime:
    global RT
    if RT is None:
        RT = Runtime()
    return RT


# ----------------------------------------------------------------------
# 抓取任务
# ----------------------------------------------------------------------

# Boss 官网 salary 筛选编码 -> (下限K, 上限K)
# Boss 直聘「薪资待遇」下拉框的 6 档（前端用 sal:下限-上限 这种自定义编码）
SALARY_CODES: dict[str, tuple[int, int]] = {
    "sal:0-8": (0, 8),        # 8K以下
    "sal:8-12": (8, 12),      # 8-12K
    "sal:12-16": (12, 16),    # 12-16K
    "sal:16-20": (16, 20),    # 16-20K
    "sal:20-30": (20, 30),    # 20-30K
    "sal:30-9999": (30, 9999),  # 30K以上
    # 兼容老版本界面用过的数字编码
    "403": (0, 3), "404": (3, 5), "405": (5, 10), "414": (10, 20), "415": (20, 50), "418": (50, 9999),
    "406": (10, 15), "407": (15, 20), "408": (20, 30), "409": (30, 50),
    "410": (50, 9999), "412": (5, 8), "413": (8, 12), "416": (35, 50), "417": (50, 70),
}


def parse_salary_code(code: str) -> tuple[int, int] | None:
    """把薪资编码解析成 (下限, 上限)。
    支持 sal:8-12 / sal:8-9999 这类自定义档位，也支持老数字编码。"""
    code = str(code or "").strip()
    if not code:
        return None
    if code in SALARY_CODES:
        return SALARY_CODES[code]
    mm = re.match(r"^sal:(\d{1,3})-(\d{1,4})$", code)
    if mm:
        low, high = int(mm.group(1)), int(mm.group(2))
        if low < 0 or high <= low:
            return None
        return (low, high)
    return None


def describe_salary_ranges(ranges: list[tuple[int, int]]) -> str:
    return "、".join(f"{a}-{b}K" if b < 9000 else f"{a}K以上" for a, b in ranges)


def salary_ranges(codes: list[str] | None) -> list[tuple[int, int]]:
    out: list[tuple[int, int]] = []
    for c in codes or []:
        r = parse_salary_code(str(c))
        if r:
            out.append(r)
    return out


def tighten_ranges(ranges: list[tuple[int, int]]) -> list[tuple[int, int]]:
    """二次校验用的区间就是用户勾选的区间本身。

    以前这里会把区间「收紧」到 Boss 档位，导致自定义 18-25K 被改成 20-25K、
    30K以上变成 50K以上，自定义档位形同虚设。现在严格按用户填的区间判。
    """
    return list(ranges)


def _exclude(rt: "Runtime", job: dict[str, Any], label: str, reason: str) -> bool:
    """统一排除出口：写决策日志 + 存进敏感库（下一轮抓取前直接跳过）。"""
    rt.store.log_decision(job.get("platform") or "", label, "已排除", reason)
    rt.progress["filtered"] += 1
    try:
        rt.store.add_sensitive(
            platform=job.get("platform") or "",
            company=job.get("company"),
            title=job.get("title"),
            reason=reason,
            job_id=job.get("job_id"),
            salary=job.get("salary"),
            location=job.get("location"),
            job_url=job.get("job_url"),
            kind="rejected",
        )
    except Exception as exc:
        rt.log("warn", f"写入敏感库失败：{exc}")
    rt.log("info", f"[已排除] {label} — {reason}")
    return False


def process_job(rt: Runtime, job: dict[str, Any]) -> bool:
    """入库 + 打分 + 写决策日志。返回是否入队。"""
    label = f"{job.get('company')} | {job.get('title')}"

    # 0) 敏感库：之前被判过"不投递"的岗位，直接跳过（不看 JD）
    plat = job.get("platform") or ""
    try:
        hit = rt.store.sensitive_hit(plat, job.get("company"), job.get("title"))
        if hit is None:
            hit = rt.store.sensitive_company_hit(plat, job.get("company"))
            if hit is not None:
                return _exclude(rt, job, label, f"该公司此前已被敏感库排除（{hit.get('title')}：{hit.get('reason')}）")
        if hit is not None:
            rt.progress["filtered"] += 1
            rt.log("info", f"[已排除] {label} — 命中敏感库（原因：{hit.get('reason')}）")
            return False
    except Exception as exc:
        rt.log("warn", f"敏感库查重失败：{exc}")

    # 1) 关键词排除
    raw_excludes = rt.store.get_config_json("exclude_keywords", DEFAULT_EXCLUDE_KEYWORDS) or []
    cfg_excludes = split_keywords(raw_excludes)
    hay = f"{job.get('company') or ''} {job.get('title') or ''}"
    for w in cfg_excludes:
        if w and w in hay:
            return _exclude(rt, job, label, f"命中排除关键词「{w}」")

    # 1.5) 薪资区间二次校验：Boss 接口偶尔会返回区间外的岗位，这里按勾选区间复核
    ranges = salary_ranges(getattr(rt, "salary_filter", None))
    if ranges:
        ranges = tighten_ranges(ranges)
        if rt.scorer.rule.salary_out_of_range(job.get("salary"), ranges):
            return _exclude(rt, job, label,
                            f"薪资 {job.get('salary')} 不在勾选区间 {describe_salary_ranges(ranges)} 内")

    # 2) 规则硬性排除
    reason = rt.scorer.reject(job)
    if reason:
        return _exclude(rt, job, label, reason)

    # 3) 打分
    result = rt.scorer.score(job)
    job["match_score"] = result.score
    job["score_source"] = result.source
    job["score_reason"] = result.reason
    job["token_used"] = result.tokens
    if result.tokens:
        # 累计本次运行的大模型 token 消耗
        rt.progress["tokens"] = rt.progress.get("tokens", 0) + result.tokens

    # 3.5) 分值门槛：低于设定分值的 JD 不入库
    gate = int(getattr(rt, "min_score", 0) or 0)
    if gate > 1 and result.score < gate:
        return _exclude(rt, job, label, f"匹配度 {result.score}/10 低于设定门槛 {gate} 分")

    # 4) 入库（重复自动跳过）
    is_new = rt.store.enqueue_job(job)
    # 入库成功的岗位也写进敏感库（kind=stored），下一轮抓到直接跳过，避免重复抓取
    try:
        rt.store.add_sensitive(
            platform=job.get("platform") or "",
            company=job.get("company"),
            title=job.get("title"),
            reason="已入库" if is_new else "已入库（重复跳过）",
            job_id=job.get("job_id"),
            salary=job.get("salary"),
            location=job.get("location"),
            job_url=job.get("job_url"),
            kind="stored",
        )
    except Exception as exc:
        rt.log("warn", f"写入已入库库失败：{exc}")
    if not is_new:
        rt.store.update_job(
            job["job_id"], match_score=result.score,
            score_source=result.source, score_reason=result.reason,
            token_used=result.tokens,
        )

    # 5) 低分也记录，便于回溯
    decision = "加入队列" if result.score >= 5 else "低分保留"
    rt.store.log_decision(job["platform"], label, decision, f"{result.score}/10 {result.reason}")
    rt.progress["scored"] += 1
    if is_new:
        rt.progress["collected"] += 1
    rt.log(
        "info",
        f"[{decision}] {result.score}/10 {label} — {job.get('salary') or '-'} "
        f"({result.source})",
    )
    return True


def run_scrape(rt: Runtime, platforms: list[str], cfg: ScrapeConfig,
               run_max_jd: int | None = None) -> None:
    """执行一轮抓取。

    run_max_jd：本次运行的有效 JD 上限（None 或 0 = 不限制）。
    由 /api/start 按「请求里指定的 > 界面上配置的」决定后传进来；
    之前漏传导致 `name 'run_max_jd' is not defined`，抓取一启动就崩。
    """
    if run_max_jd is None:
        run_max_jd = rt.max_jd or 0
    run_max_jd = max(0, int(run_max_jd))
    # 真正开始跑的时候再置 running（排队期间 UI 显示空闲）
    rt.running = True
    rt.stop.resume()   # 新一轮开始，清掉上一轮遗留的暂停态
    rt.progress = {"collected": 0, "scored": 0, "filtered": 0, "total": 0, "tokens": 0}
    rt.salary_filter = list(cfg.salary or [])
    rt.log("info", f"开始抓取：平台={platforms} 关键词={cfg.keyword_list()} 城市={cfg.city} "
                   f"薪资={cfg.salary or '不限'} 经验={cfg.experience or '不限'}")
    if rt.salary_filter:
        rt.log("info", f"薪资二次校验已启用：只保留 {describe_salary_ranges(salary_ranges(rt.salary_filter))} 内的岗位")

    ctx = None
    try:
        # 复用常驻的 patchright 持久化浏览器（登录态就在这里）
        from .browser import detect_logged_in_from_page

        ctx = rt.ensure_browser()
        for plat in platforms:
            try:
                if not any(detect_logged_in_from_page(pg, plat) for pg in ctx.pages):
                    rt.log("warn", f"{plat} 尚未登录，抓取可能失败（点左侧「登录」按钮）")
            except Exception:
                pass

        for plat in platforms:
            if rt.stop.is_set():
                break
            cls = SCRAPERS.get(plat)
            if cls is None:
                rt.log("warn", f"未知平台 {plat}，跳过")
                continue
            rt.log("info", f"=== 开始处理 {plat} ===")
            # 候选池：过滤损耗较大，按目标的 5 倍抓（上限 1000），保证"入库数"能凑够
            if run_max_jd:
                cfg.daily_limit = max(int(cfg.daily_limit or 0), min(run_max_jd * 5, 1000))
                cfg.target_count = run_max_jd
            scraper = cls(cfg, rt.stop, rt.log)
            # 有效 JD 上限：达标就停止本轮抓取
            limit = run_max_jd
            if limit and rt.progress.get("collected", 0) >= limit:
                rt.log("info", f"有效 JD 已达上限 {limit} 条，停止抓取")
                break
            rt.stop.clear()   # 达到有效 JD 上限后清掉停止位，方便下次继续

            # 流式入库：抓到一条 JD 就立刻过滤/打分/落库，
            # 不必等整页抓完，界面与数据库都是实时增长的
            def _on_job(job: dict[str, Any]) -> bool:
                if rt.stop.is_set():
                    return True
                try:
                    process_job(rt, job)
                except Exception as exc:
                    rt.log("error", f"处理岗位失败 {job.get('job_id')}: {exc}")
                # 已入库数达到上限 → 停止本轮（被排除的不算数）
                scraper.arrived = rt.progress.get("collected", 0)
                limit = run_max_jd
                if limit and scraper.arrived >= limit:
                    rt.log("info", f"有效 JD 已达上限 {limit} 条（入库 {scraper.arrived} 条），停止抓取")
                    scraper.arrived = max(scraper.arrived, limit)  # 让抓取侧也认为已达标
                    rt.stop.set()
                    return True
                return False

            jobs = scraper.scrape(ctx, on_job=_on_job)
            rt.progress["total"] += len(jobs)
            rt.log("info", f"{plat} 抓到 {len(jobs)} 条（已在抓取过程中实时入库）")
            rt.log("info", f"=== {plat} 完成 ===")
    except Exception as exc:
        rt.log("error", f"抓取任务异常：{exc}\n{traceback.format_exc()}")
    finally:
        # 浏览器保持常驻，不关闭：下次抓取直接复用，登录态不丢
        rt.running = False
        rt.log("info", f"任务结束：新入库 {rt.progress['collected']}，"
                       f"打分 {rt.progress['scored']}，排除 {rt.progress['filtered']}")


# ----------------------------------------------------------------------
# FastAPI
# ----------------------------------------------------------------------

# 默认排除关键词（与前端筛选面板默认值保持一致）
DEFAULT_EXCLUDE_KEYWORDS = [
    "外包", "驻场", "人力", "兼职", "跨境", "金融", "银行", "证券",
    "区块链", "Web3", "985", "211", "硕士", "博士", "销售",
]

app = FastAPI(title="AI Job Assistant", version="0.1.0")
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=False,
    allow_methods=["*"],
    allow_headers=["*"],
)


def _logged_in(rt: Runtime) -> dict[str, bool]:
    """从常驻浏览器读各平台登录态（按 cookie 判定，不依赖已关闭的登录窗）。

    浏览器由采集线程独占，这里用**线程安全**的方式读 cookies：
    ctx.cookies() 可以带 browserContextId 走 CDP，不经过 JS 执行，不依赖 event loop。
    """
    from .browser import PLATFORM_HOME, LOGIN_COOKIES

    result = {p: False for p in PLATFORM_HOME}
    try:
        cookies = rt._ctx.cookies() if rt._ctx is not None else []   # 线程安全：纯 CDP 调用
    except Exception:
        return result
    for plat, (domain, names) in LOGIN_COOKIES.items():
        try:
            result[plat] = any(
                domain in (c.get("domain") or "") and c.get("name") in names
                for c in cookies
            )
        except Exception:
            result[plat] = False
    return result


class StartReq(BaseModel):
    """筛选条件，字段与 Boss 直聘官网筛选项一一对应（全部多选）。"""

    platforms: list[str] = Field(default_factory=lambda: ["boss"])
    keywords: list[str] | str = Field(default_factory=list)          # 多关键词
    keyword: str = ""                                                # 兼容旧字段
    city: str = "101020100"                                          # Boss 城市编码，上海=101020100
    salary: list[str] = Field(default_factory=list)                  # 406,407,415...
    experience: list[str] = Field(default_factory=list)              # 103,104,105...
    degree: list[str] = Field(default_factory=list)                  # 202,203,204...
    scale: list[str] = Field(default_factory=list)                   # 301..306
    stage: list[str] = Field(default_factory=list)                   # 801..808
    industry: list[str] = Field(default_factory=list)                # 行业编码
    job_type: str = ""                                               # 1901/1902/1903
    exclude_keywords: list[str] | str = Field(default_factory=list)
    max_pages: int = 5
    daily_limit: int = 100
    max_jd: int | None = None   # 有效 JD 上限，0=不限制；不传则沿用界面上设置的值


@app.get("/api/health")
def health() -> dict[str, Any]:
    """探活：Tauri 侧与前端都靠它判断后端是否已经起来。"""
    rt = get_rt()
    return {"ok": True, "running": rt.running}


@app.post("/api/start")
def start(req: StartReq) -> dict[str, Any]:
    rt = get_rt()
    if rt.running:
        raise HTTPException(409, "已有任务在运行")
    if not req.platforms:
        raise HTTPException(400, "至少选择一个平台")

    if not split_keywords(req.keywords) and not (req.keyword or "").strip():
        raise HTTPException(400, "请先填写关键词（可多个，回车/逗号分隔）")

    rt.store.set_config_json("exclude_keywords", req.exclude_keywords)
    # 请求里的上限只管「这一次」，不写回配置（配置以界面上的「有效 JD 上限」为准）
    run_max_jd = max(0, int(req.max_jd)) if req.max_jd is not None else (rt.max_jd or 0)
    cfg = ScrapeConfig(
        keywords=req.keywords or ([req.keyword] if req.keyword else []),
        keyword=req.keyword,
        city=req.city,
        salary=req.salary, experience=req.experience, degree=req.degree,
        scale=req.scale, stage=req.stage, industry=req.industry,
        job_type=req.job_type,
        exclude_keywords=req.exclude_keywords, max_pages=req.max_pages,
        daily_limit=req.daily_limit,
    )
    # 记住这次选的条件，下次打开程序还在
    rt.store.set_config_json("last_filter", req.model_dump())
    rt.stop = StopFlag()
    rt.platforms = list(req.platforms)
    rt.running = True          # 立刻置位，防止连点两次启动两个任务
    rt.submit(run_scrape, rt, list(req.platforms), cfg, run_max_jd)
    return {"ok": True, "message": "任务已启动"}


class LimitReq(BaseModel):
    max_jd: int = 0
    min_score: int | None = None


@app.post("/api/limit")
def set_limit(req: LimitReq) -> dict[str, Any]:
    """设置「有效 JD」上限：抓取过程中改动立即生效（达到上限即停止入库）。"""
    rt = get_rt()
    rt.max_jd = max(0, int(req.max_jd or 0))
    rt.store.set_config("max_jd", rt.max_jd)
    if req.min_score is not None:
        rt.min_score = max(0, min(int(req.min_score), 10))
        rt.store.set_config("min_score", rt.min_score)
    return {"ok": True, "max_jd": rt.max_jd, "min_score": rt.min_score}


@app.get("/api/expectations")
def expectations(platform: str = "boss") -> dict[str, Any]:
    """读取 Boss 账号里的「求职期望」职位。

    实现要点（踩过的坑）：
    - **不经过采集线程/浏览器**：从 last_auth 里的 cookie 直接发 HTTP 请求，
      不进采集队列（否则会被前一个卡住的任务阻塞，界面一直"读取中"）
    - 失败时回退到浏览器 profile 的 Cookies 库（Chrome 关着才读得到）
    """
    import urllib.request

    rt = get_rt()

    def _fetch() -> list[dict[str, Any]]:
        from .browser import LOGIN_COOKIES, profile_dir
        from .cookie_import import read_cookies_from_profile

        domain, names = LOGIN_COOKIES.get("boss", ("zhipin.com", ()))
        cookies: dict[str, str] = {}
        raw = rt.store.get_config_json("last_auth") or {}
        for c in (raw.get("cookies") or []):
            try:
                if domain in (c.get("domain") or ""):
                    cookies[c.get("name", "")] = c.get("value", "")
            except Exception:
                continue
        if not cookies:
            for c in read_cookies_from_profile(profile_dir(app_data_dir()), domain) or []:
                cookies[c.get("name", "")] = c.get("value", "")
        if not cookies:
            raise RuntimeError("没有可用的登录 cookie，请先登录 Boss直聘")

        url = "https://www.zhipin.com/wapi/zpgeek/pc/recommend/expect/list.json"
        req = urllib.request.Request(url, headers={
            "User-Agent": (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
                "(KHTML, like Gecko) Chrome/157.0.0.0 Safari/537.36"
            ),
            "Referer": "https://www.zhipin.com/web/geek/job",
            "Accept": "application/json, text/plain, */*",
            "Cookie": "; ".join(f"{k}={v}" for k, v in cookies.items() if v),
        })
        with urllib.request.urlopen(req, timeout=15) as resp:
            data = json.loads(resp.read().decode("utf-8", "replace"))
        out: list[dict[str, Any]] = []
        for e in (data.get("zpData") or {}).get("expectList") or []:
            name = str(e.get("positionName") or "").strip()
            if not name:
                continue
            out.append({
                "name": name,
                "code": str(e.get("position") or ""),
                "city": e.get("locationName") or "",
                "city_code": str(e.get("location") or ""),
                "salary": e.get("salary") or "",
                "source": "boss_expect",
            })
        return out

    try:
        items = _fetch()
    except Exception as exc:
        rt.log("warn", f"读取期望职位失败：{exc}")
        return {"ok": False, "status": "error", "items": [], "error": str(exc)}
    if items:
        rt.log("info", f"读取期望职位 {len(items)} 条：" + "、".join(i["name"] for i in items))
    else:
        rt.log("warn", "读取期望职位：没有拿到数据")
    return {"ok": True, "status": "done", "items": items,
            "error": "" if items else "没有读到期望职位（账号里可能没配置，或登录已失效）"}


@app.post("/api/stop")
def stop() -> dict[str, Any]:
    rt = get_rt()
    rt.stop.stop()
    return {"ok": True, "message": "已发送停止指令"}


@app.post("/api/pause")
def pause() -> dict[str, Any]:
    """暂停抓取：停在当前页面，浏览器里能继续观察这条 JD。"""
    rt = get_rt()
    rt.stop.pause()
    rt.log("info", "已暂停抓取（页面停留在当前 JD，可切到浏览器查看）")
    return {"ok": True, "paused": True, "message": "已暂停"}


@app.post("/api/resume")
def resume() -> dict[str, Any]:
    rt = get_rt()
    rt.stop.resume()
    rt.log("info", "已恢复抓取")
    return {"ok": True, "paused": False, "message": "已恢复"}


@app.get("/api/status")
def status() -> dict[str, Any]:
    rt = get_rt()
    sessions = {p: s.snapshot() for p, s in rt.logins.items()}
    return {
        "running": rt.running,
        "platforms": rt.platforms,
        "progress": rt.progress,
        "paused": rt.stop.is_paused(),
        "max_jd": rt.max_jd,
        "min_score": rt.min_score,
        "progress_text": (
            f"已入库 {rt.progress.get('collected', 0)} 条"
            + (f" · 已排除 {rt.progress.get('filtered', 0)} 条" if rt.progress.get('filtered') else "")
            + (f" / 上限 {rt.max_jd}" if rt.max_jd else "")
        ),
        "stats": rt.store.stats(),
        "logged_in": _logged_in(rt),
        "login_sessions": sessions,
    }


# ----------------------------------------------------------------------
# 平台登录（弹窗登录 → 保存登录态 → 可关闭窗口）
# ----------------------------------------------------------------------

class LoginReq(BaseModel):
    platform: str


@app.get("/api/platforms")
def platforms() -> dict[str, Any]:
    rt = get_rt()
    return {
        "platforms": [
            {"key": "boss", "name": "Boss直聘"},
            {"key": "zhilian", "name": "智联招聘"},
            {"key": "51job", "name": "51job"},
        ],
        "logged_in": _logged_in(rt),
        "login_sessions": {
            k: (v.snapshot() if hasattr(v, "snapshot") else {}) for k, v in rt.logins.items()
        },
    }


@app.post("/api/login/start")
def login_start(req: LoginReq) -> dict[str, Any]:
    """弹出该平台的登录窗口；登录成功后自动保存登录态。"""
    from .browser import LoginSession

    rt = get_rt()
    plat = req.platform
    if plat not in SCRAPERS:
        raise HTTPException(400, f"不支持的平台：{plat}")
    old = rt.logins.get(plat)
    if old and old.snapshot()["running"]:
        return {"ok": True, "message": "登录窗口已打开", "session": old.snapshot()}

    # 关键：浏览器必须在**同一个线程**里启动和使用。
    # patchright 的 sync API 绑定了创建它的 event loop，如果在 FastAPI 线程池
    # 里 ensure_browser()，之后采集线程再用就会报
    # "RuntimeError: no running event loop"。
    # 所以这里只登记登录会话，真正的启动交给采集线程 rt.worker()。
    sess = LoginSession(
        plat, app_data_dir(), rt.log,
        get_context=None,
        start_browser=rt.ensure_browser_on_worker,  # 已自带一次 run_in_worker 投递
        step_runner=rt.run_in_worker,               # 其余每步都投递到采集线程
    )
    rt.logins[plat] = sess
    try:
        rt.submit(sess.start)
    except Exception as exc:
        raise HTTPException(500, f"打开登录窗口失败：{exc}")
    return {"ok": True, "message": "已打开登录窗口", "session": sess.snapshot()}


@app.get("/api/login/status")
def login_status(platform: str) -> dict[str, Any]:
    rt = get_rt()
    sess = rt.logins.get(platform)
    snap = sess.snapshot() if sess else {"platform": platform, "state": "idle", "running": False, "message": ""}
    snap["saved"] = _logged_in(rt).get(platform, False)
    return snap


@app.post("/api/login/save")
def login_save(platform: str = "boss") -> dict[str, Any]:
    """把当前浏览器的登录 cookie 持久化到 last_auth（登录成功后由前端调用）。

    读取期望职位走的是纯 HTTP（不经过浏览器），需要有一份可用的 cookie。
    """
    rt = get_rt()
    ctx = rt._ctx
    if ctx is None:
        return {"ok": False, "error": "浏览器尚未启动"}
    try:
        cookies = ctx.cookies()
    except Exception as exc:
        return {"ok": False, "error": f"读取 cookie 失败：{exc}"}
    picked = [c for c in cookies if "zhipin" in (c.get("domain") or "")] if platform == "boss" else cookies
    if not picked:
        return {"ok": False, "error": "没有拿到该平台的登录 cookie"}
    rt.store.set_config_json("last_auth", {"platform": platform, "cookies": picked, "ts": time.time()})
    rt.log("info", f"已保存 {platform} 登录 cookie {len(picked)} 条")
    return {"ok": True, "saved": len(picked)}


@app.post("/api/login/logout")
def login_logout(req: LoginReq) -> dict[str, Any]:
    """退出登录：清掉该平台的 cookie。"""
    from .browser import LOGIN_COOKIES

    rt = get_rt()
    plat = req.platform
    cleared = 0
    ctx = rt._ctx
    if ctx is not None:
        spec = LOGIN_COOKIES.get(plat)
        if spec:
            domain = spec[0]
            try:
                cookies = [c for c in ctx.cookies() if domain in (c.get("domain") or "").lstrip(".").lower()]
                for c in cookies:
                    try:
                        ctx.clear_cookies(name=c["name"], domain=c["domain"], path=c.get("path", "/"))
                        cleared += 1
                    except Exception:
                        pass
            except Exception:
                pass
    return {"ok": True, "message": f"已退出登录（清理 {cleared} 条 cookie）", "cleared": cleared}


@app.get("/api/jobs")
def jobs(status: str | None = None, platform: str | None = None,
         min_score: int | None = None, limit: int = 40, offset: int = 0,
         sort: str = "score", order: str = "desc",
         include_rejected: bool = True, rejected_only: bool = False) -> dict[str, Any]:
    rt = get_rt()
    limit = max(0, min(int(limit), 200))  # limit=0 表示只要总数（页签计数用）
    if limit == 0:
        total = rt.store.count_jobs(status, platform, min_score, include_rejected, rejected_only)
        return {"items": [], "total": total, "offset": offset, "limit": 0}
    items = rt.store.list_jobs(status, platform, min_score, limit, offset, sort, order,
                               include_rejected, rejected_only)
    total = rt.store.count_jobs(status, platform, min_score, include_rejected, rejected_only)
    return {"items": items, "total": total, "offset": offset, "limit": limit}


@app.post("/api/jobs/reject")
def reject_jobs(req: dict[str, Any]) -> dict[str, Any]:
    """不投递：只打标记（记录保留在列表里可见），之后重抓也不会再进入待投递流程。"""
    rt = get_rt()
    ids = [str(x) for x in (req.get("job_ids") or [])]
    if not ids:
        return {"ok": True, "updated": 0}
    names: list[str] = []
    for jid in ids:
        row = rt.store.get_job(jid)
        if row:
            names.append(f"{row.get('platform')} {row.get('company') or ''} | {row.get('title') or ''}")
    updated = rt.store.set_rejected(ids, True)
    for n in names:
        rt.store.log_decision(platform="", company_job=n, decision="不投递", reason="手动标记不投递")
    rt.log("info", f"已标记不投递 {updated} 条")
    return {"ok": True, "updated": updated}


@app.post("/api/jobs/deliver")
def mark_delivered(req: dict[str, Any]) -> dict[str, Any]:
    """标记已投递（置灰）。投递模块接入后由投递结果自动调用这个接口。"""
    rt = get_rt()
    ids = [str(x) for x in (req.get("job_ids") or [])]
    if not ids:
        return {"ok": True, "updated": 0}
    updated = rt.store.set_delivered(ids, True)
    rt.log("info", f"已标记 {updated} 条为已投递")
    return {"ok": True, "updated": updated}


@app.post("/api/jobs/allow")
def allow_jobs(req: dict[str, Any]) -> dict[str, Any]:
    """恢复允许投递：把状态从「不投递」改回未投递。"""
    rt = get_rt()
    ids = [str(x) for x in (req.get("job_ids") or [])]
    if not ids:
        return {"ok": True, "updated": 0}
    updated = rt.store.set_rejected(ids, False)
    rt.log("info", f"已恢复允许投递 {updated} 条")
    return {"ok": True, "updated": updated}


@app.on_event("startup")
def _sync_stored_on_startup() -> None:
    """启动时把已入库的岗位补进敏感库，避免历史数据被重复抓取。"""
    rt = get_rt()
    try:
        n = rt.store.sync_stored()
        if n:
            rt.log("info", f"已把 {n} 条历史入库岗位同步进敏感库（重复抓取会自动跳过）")
    except Exception as exc:
        rt.log("warn", f"同步历史入库岗位失败：{exc}")


@app.get("/api/sensitive")
def list_sensitive(
    platform: str | None = None,
    keyword: str | None = None,
    kind: str | None = None,
    limit: int = 500,
    offset: int = 0,
) -> dict[str, Any]:
    rt = get_rt()
    items = rt.store.list_sensitive(
        platform=platform, keyword=keyword, kind=kind, limit=limit, offset=offset
    )
    return {"items": items, "total": rt.store.count_sensitive(platform=platform)}


class SensitiveDeleteReq(BaseModel):
    ids: list[int] = []


@app.post("/api/sensitive/clear")
def sensitive_clear() -> dict[str, Any]:
    """清空敏感库（不影响已入库的岗位列表）。"""
    rt = get_rt()
    n = rt.store.clear_sensitive()
    rt.log("info", f"敏感库已清空（移出 {n} 条）")
    return {"ok": True, "deleted": n}


@app.post("/api/sensitive/delete")
def delete_sensitive(req: SensitiveDeleteReq) -> dict[str, Any]:
    rt = get_rt()
    n = rt.store.delete_sensitive(req.ids)
    return {"ok": True, "deleted": n}


@app.get("/api/logs")
def logs(since: float = 0.0) -> dict[str, Any]:
    rt = get_rt()
    return {"items": rt.recent_logs(since), "now": time.time()}


@app.get("/api/config")
def get_config() -> dict[str, Any]:
    rt = get_rt()
    key = rt.store.get_config("api_key") or ""
    return {
        "base_url": rt.store.get_config("base_url") or "",
        "model": rt.store.get_config("model") or "deepseek-chat",
        "api_key_masked": (key[:4] + "****" + key[-4:]) if len(key) > 8 else ("****" if key else ""),
        "has_key": bool(key),
        "resume_path": rt.store.get_config("resume_path") or "",
        "theme": rt.store.get_config("theme") or "light",
        "exclude_keywords": rt.store.get_config_json("exclude_keywords", DEFAULT_EXCLUDE_KEYWORDS),
        "exclude_rules": rt.exclude_rules().as_dict(),
        "max_jd": rt.max_jd or 0,
        "min_score": rt.min_score or 0,
        "exclude_rule_defaults": ExcludeRules().as_dict(),
        "profile": PROFILE,
    }


class ConfigReq(BaseModel):
    api_key: str | None = None
    base_url: str | None = None
    model: str | None = None
    resume_path: str | None = None
    theme: str | None = None
    exclude_keywords: list[str] | None = None
    exclude_rules: dict[str, Any] | None = None


@app.post("/api/config")
def set_config(req: ConfigReq) -> dict[str, Any]:
    rt = get_rt()
    if req.api_key is not None:
        rt.store.set_config("api_key", req.api_key.strip())
    if req.base_url is not None:
        rt.store.set_config("base_url", req.base_url.strip())
    if req.model is not None:
        rt.store.set_config("model", req.model.strip())
    if req.resume_path is not None:
        rt.store.set_config("resume_path", req.resume_path)
    if req.theme is not None:
        rt.store.set_config("theme", req.theme)
    if req.exclude_keywords is not None:
        rt.store.set_config_json("exclude_keywords", req.exclude_keywords)
    if req.exclude_rules is not None:
        rt.store.set_config_json("exclude_rules", req.exclude_rules)
    rt.rebuild_scorer()
    return {"ok": True, "llm_enabled": rt.scorer.llm is not None}


@app.post("/api/rescore")
def rescore() -> dict[str, Any]:
    """对所有未打分/需重算的岗位重新打分（切换模型后很有用）。"""
    rt = get_rt()
    rows = rt.store.unscored_jobs()
    n = 0
    for job in rows:
        try:
            res = rt.scorer.score(job)
            rt.store.update_job(
                job["job_id"], match_score=res.score,
                score_source=res.source, score_reason=res.reason,
                token_used=res.tokens,
            )
            if res.tokens:
                rt.progress["tokens"] = rt.progress.get("tokens", 0) + res.tokens
            n += 1
        except Exception:
            pass
    return {"ok": True, "rescored": n}


def pick_port(preferred: int = 0) -> int:
    """选一个空闲端口；preferred=0 时由系统分配。"""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", preferred))
        return s.getsockname()[1]


def main() -> None:
    import uvicorn

    port = int(os.environ.get("AJA_PORT", "0")) or pick_port()
    # Tauri 侧从 stdout 读这一行拿到端口
    print(f"AJA_BACKEND_PORT={port}", flush=True)
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="warning")


if __name__ == "__main__":
    main()
