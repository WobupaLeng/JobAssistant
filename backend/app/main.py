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
from .greeting import DEFAULT_PROMPT, GreetingWriter, profile_with_skills
from .store import DB_NAME, Store
from scrapers.base import ScrapeConfig, StopFlag, split_keywords
from scrapers.boss import BossScraper

SCRAPERS = {"boss": BossScraper}


def dev_data_dir() -> Path:
    """开发态数据目录：项目根的 data/（AJA_DATA_DIR 不参与，始终算项目根）。"""
    return Path(__file__).resolve().parents[2] / "data"


def packaged_data_dir() -> Path:
    """打包态数据目录：Tauri 释放后端的 `%LOCALAPPDATA%\\ai-job-assistant\\data`。

    PyInstaller onefile 里 sys.executable 是释放到临时目录的 exe，
    所以优先用 Tauri 注入的 AJA_DATA_DIR。
    """
    env = os.environ.get("AJA_DATA_DIR")
    if env:
        return Path(env)
    base = os.environ.get("LOCALAPPDATA")
    if base:
        return Path(base) / "ai-job-assistant" / "data"
    return Path(sys.executable).parent / "data"


def prepared_data_dir() -> Path:
    """确认过的打包态数据目录（Tauri 下不一定建过这个目录，这里不报错）。"""
    try:
        return packaged_data_dir()
    except Exception:
        return dev_data_dir()


def app_data_dir() -> Path:
    """数据目录：打包后放在 AppData，开发时放项目根的 data/。

    优先用配置里记住的目录（见 Runtime.__init__ 的「自动识别已有登录态」），
    这样开发态跑也会沿用打包版登录过的浏览器 profile。
    """
    rt = RT
    if rt is not None and getattr(rt, "data_dir", None):
        p = Path(rt.data_dir)
    elif getattr(sys, "frozen", False):
        p = packaged_data_dir()
    else:
        p = dev_data_dir()
    p.mkdir(parents=True, exist_ok=True)
    return p


# ----------------------------------------------------------------------
# 全局运行时
# ----------------------------------------------------------------------

# ----------------------------------------------------------------------
# 运行日志落盘：%LOCALAPPDATA%\ai-job-assistant\logs\backend.log
# ----------------------------------------------------------------------

_LOG_LOCK = threading.Lock()
_LOG_STATE = {"path": None, "fh": None}


def log_file_path() -> Path:
    try:
        return Path(app_data_dir()) / "logs" / "backend.log"
    except Exception:
        base = os.environ.get("LOCALAPPDATA") or str(Path.home())
        return Path(base) / "ai-job-assistant" / "logs" / "backend.log"


def log_to_file(level: str, tag: str, message: str) -> None:
    """把日志同时写进文件。

    打包态后端是窗口子进程，没地方看 print 输出；「程序直接关掉」这类问题
    没有日志就完全无迹可寻。日志超过 4MB 时滚动一份 .1。
    """
    path = log_file_path()
    with _LOG_LOCK:
        try:
            fh = _LOG_STATE.get("fh")
            if fh is None or _LOG_STATE.get("path") != str(path):
                path.parent.mkdir(parents=True, exist_ok=True)
                if path.is_file() and path.stat().st_size > 4 * 1024 * 1024:
                    try:
                        os.replace(path, path.with_suffix(".log.1"))
                    except OSError:
                        pass
                fh = open(path, "a", encoding="utf-8", errors="replace")
                _LOG_STATE["fh"] = fh
                _LOG_STATE["path"] = str(path)
            ts = time.strftime("%Y-%m-%d %H:%M:%S")
            fh.write(f"{ts} [{level}] [{tag}] {message}\n")
            fh.flush()
        except Exception:
            # 写日志失败绝不能影响业务
            pass


_EXCEPT_HOOK_READY = False


def install_excepthooks() -> None:
    """把主线程/任意线程里漏出来的异常写进 backend.log 和界面日志。

    之前这些异常被 threading.excepthook 默认打印到早已失效的 stderr，
    直接从界面上消失，用户只看到「没反应 / 程序被关掉」。
    """
    global _EXCEPT_HOOK_READY
    if _EXCEPT_HOOK_READY:
        return
    _EXCEPT_HOOK_READY = True

    def _fmt(exc_type, exc, tb) -> str:
        return "".join(traceback.format_exception(exc_type, exc, tb)).strip()

    def _sys_hook(exc_type, exc, tb) -> None:
        log_to_file("error", "uncaught", _fmt(exc_type, exc, tb))

    def _thread_hook(args) -> None:
        if args.exc_type is SystemExit:
            return
        name = getattr(getattr(args, "thread", None), "name", "?")
        log_to_file("error", f"thread {name}", _fmt(args.exc_type, args.exc_value, args.exc_traceback))
        rt = RT
        if rt is not None:
            try:
                rt.log("error", f"后台线程 {name} 出错：{args.exc_value}")
            except Exception:
                pass

    sys.excepthook = _sys_hook
    try:
        threading.excepthook = _thread_hook
    except Exception:
        pass


install_excepthooks()


class Runtime:
    def __init__(self) -> None:
        self.data_dir = self._resolve_data_dir()
        self.store = Store(Path(self.data_dir) / DB_NAME)
        self.scorer = self._build_scorer()
        self.stop = StopFlag()
        self.thread: threading.Thread | None = None
        self.running = False
        self.platforms: list[str] = []
        self.logs: list[dict[str, Any]] = []
        self.log_lock = threading.Lock()
        self.progress = {"collected": 0, "scored": 0, "filtered": 0, "total": 0, "tokens": 0}
        # 全局累计 token（打分 + 生成打招呼），持久化在 app_config，重启不清零
        self.tokens_total = 0
        self.tokens_greet = 0
        self.tokens_score = 0
        try:
            self.tokens_total = int(self.store.get_config("tokens_total", "0") or 0)
            self.tokens_greet = int(self.store.get_config("tokens_greet", "0") or 0)
            self.tokens_score = int(self.store.get_config("tokens_score", "0") or 0)
            if self.tokens_score <= 0 and self.tokens_total > self.tokens_greet:
                # 老数据没有分开记：剩下的都算打分消耗
                self.tokens_score = self.tokens_total - self.tokens_greet
        except Exception:
            pass
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
        # Boss 消息模块：进程内唯一，由采集线程独占驱动
        from scrapers.boss_chat import BossChat

        self.chat = BossChat(self.log)
        self.chat.set_filler(self._chat_filler)
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
        try:
            self.log(
                "info",
                f"运行时已就绪：数据目录={self.data_dir} 库={self.store.path if hasattr(self.store, 'path') else ''}",
            )
        except Exception:
            pass

    @staticmethod
    def _resolve_data_dir() -> Path:
        """决定用哪个 data 目录，并把结果记进配置。

        踩过的坑：淘宝/打包版刚跑过时，数据在 `%LOCALAPPDATA%\\ai-job-assistant\\data`；
        开发态（`python -m app.main`）默认用项目根 `data/`，于是提示「没有可用的登录
         cookie，请先登录 Boss直聘」——登录窗口其实打不开（程序没在运行），
        而真正的浏览器 profile 在另一个目录。现在：配置里记住上次用的目录，
        它的浏览器 profile 有数据就继续用它。
        """
        if getattr(sys, "frozen", False):
            default = packaged_data_dir()
        else:
            default = dev_data_dir()
        try:
            remembered = ""
            try:
                remembered = (Store(default / DB_NAME).get_config(DATA_DIR_KEY, "") or "").strip()
            except Exception:
                pass
            from .browser import latest_profile_cookie_db, profile_dir

            def _has_profile(p: Path) -> bool:
                try:
                    return latest_profile_cookie_db(profile_dir(p)) is not None
                except Exception:
                    return False

            chosen = default
            if remembered and _has_profile(Path(remembered)):
                chosen = Path(remembered)
            chosen.mkdir(parents=True, exist_ok=True)
            # 记住选中的目录（写进被选中的库，下次照样命中）
            try:
                Store(chosen / DB_NAME).set_config(DATA_DIR_KEY, str(chosen))
            except Exception:
                pass
            return chosen
        except Exception:
            default.mkdir(parents=True, exist_ok=True)
            return default

    # ---------- 采集线程（patchright 的唯一宿主）----------
    def _worker_loop(self) -> None:
        while True:
            task = self._task.get()
            if task is None:
                return
            fn, args = task
            try:
                self._worker_label = getattr(fn, "__wrapped_label__", None) or getattr(fn, "__name__", "任务")
                self._worker_t0 = time.time()
            except Exception:
                pass
            # run_in_worker 投递的是内部包装函数 _wrap，直接打名字只会得到
            # 「采集线程开始执行：_wrap」，看不出在干什么。这里用 label 说清楚
            # 具体在做什么，同时在后台记一条 DEBUG 级的来源，便于对照。
            label = getattr(fn, "__wrapped_label__", None) or getattr(fn, "__name__", "任务")
            if label != "_wrap":
                try:
                    self.log("info", f"开始：{label}")
                except Exception:
                    pass
            t0 = time.time()
            try:
                fn(*args)
            except BaseException as exc:  # 单条任务出任何问题都不能把采集线程打死
                try:
                    self.log("error", f"失败：{label} —— {exc}")
                except Exception:
                    pass
            else:
                cost = time.time() - t0
                if cost >= 3 and label != "_wrap":
                    try:
                        self.log("info", f"完成：{label}（{cost:.1f}s）")
                    except Exception:
                        pass
            finally:
                try:
                    self._worker_label = ""
                    self._worker_t0 = 0.0
                except Exception:
                    pass

    def worker_busy(self) -> tuple[str, float]:
        """采集线程当前在忙什么、忙了多久。空闲返回 ("", 0)。"""
        label = getattr(self, "_worker_label", "") or ""
        t0 = getattr(self, "_worker_t0", 0.0) or 0.0
        if not label or not t0:
            return ("", 0.0)
        return (label, time.time() - t0)

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

        _wrap.__wrapped_name__ = getattr(fn, "__name__", "task")
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

    def add_tokens(self, n: int, kind: str = "score") -> None:
        """累计 token 消耗：kind=score 打分 / greet 生成打招呼。"""
        n = int(n or 0)
        if n <= 0:
            return
        self.tokens_total += n
        if kind == "greet":
            self.tokens_greet += n
            try:
                self.store.set_config("tokens_greet", str(self.tokens_greet))
            except Exception:
                pass
        else:
            self.tokens_score += n
            try:
                self.store.set_config("tokens_score", str(self.tokens_score))
            except Exception:
                pass
        try:
            self.store.set_config("tokens_total", str(self.tokens_total))
        except Exception:
            pass

    def _filter_text(self) -> str:
        """把界面上的筛选条件拼成一句话，喂给大模型打分时用。"""
        parts: list[str] = []
        try:
            if self.salary_filter:
                parts.append("薪资区间 " + "、".join(self.salary_filter))
            if self.min_score:
                parts.append(f"低于 {self.min_score} 分不要")
            rules = self.exclude_rules()
            for name in ("keywords", "companies"):
                vals = [v for v in (getattr(rules, name, []) or []) if str(v).strip()]
                if vals:
                    parts.append(f"排除{'关键词' if name == 'keywords' else '公司'}：" + "、".join(map(str, vals)))
        except Exception:
            pass
        return "；".join(parts)

    def _build_scorer(self) -> Scorer:
        rules = self.exclude_rules()
        mode = (self.store.get_config("score_mode") or "rule").strip() or "rule"
        key = self.store.get_config("api_key") or ""
        try:
            profile = _greeting_profile(self)
        except Exception:
            profile = None
        try:
            filters = self._filter_text()
        except Exception:
            filters = ""
        if not key.strip():
            sc = Scorer(None, rules, use_llm=False)
        else:
            try:
                sc = Scorer(
                    LLMScorer(
                        api_key=key.strip(),
                        base_url=self.store.get_config("base_url") or None,
                        model=self.store.get_config("model") or "deepseek-chat",
                    ),
                    rules,
                    use_llm=(mode == "llm"),
                )
            except Exception:
                sc = Scorer(None, rules, use_llm=False)
        if profile:
            sc.profile = profile
        sc.filters = filters
        try:
            sc.llm_prefilter = int(self.store.get_config("llm_prefilter", "5") or 5)
        except (TypeError, ValueError):
            sc.llm_prefilter = 5
        return sc

    def rebuild_scorer(self) -> None:
        self.scorer = self._build_scorer()

    # ---- 日志 ----
    def log(self, level: str, message: str) -> None:
        entry = {"ts": time.time(), "level": level, "message": message}
        with self.log_lock:
            self.logs.append(entry)
            if len(self.logs) > 2000:
                del self.logs[:500]
        if level not in ("info", "warn", "error"):
            return
        # 落盘：打包态没有控制台，出了问题只能靠这份 backend.log
        try:
            log_to_file(level, "app", str(message))
        except Exception:
            pass
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

    # ---- 消息模块的数据库侧动作（由 BossChat 注入回调，避免 scrapers 反向依赖 app）----
    def _chat_filler(self, action: str, friend: dict[str, Any]) -> None:
        """action=mark_delivered：发过消息的岗位进「已投递」页签。

        Boss 的会话里能拿到 encryptJobId；岗位库的 job_id 就是 `boss:{encryptJobId}`。
        发消息的岗位如果还没入库（抓取时被过滤掉了），顺手补一条，这样「已投递」列表里
        不会出现空记录。
        """
        if action != "mark_delivered":
            return
        ejid = str(friend.get("encrypt_job_id") or "").strip()
        if not ejid:
            self.log("warn", "这条会话拿不到岗位 ID，无法自动标记为已投递")
            return
        job_id = f"boss:{ejid}"
        if not self.store.exists(job_id):
            try:
                self.store.enqueue_job({
                    "job_id": job_id,
                    "platform": "boss",
                    "title": friend.get("job_title") or None,
                    "company": friend.get("brand") or None,
                    "salary": None,
                    "job_url": f"https://www.zhipin.com/job_detail/{ejid}.html",
                    "hr_name": friend.get("name") or None,
                    "hr_position": friend.get("title") or None,
                })
            except Exception as exc:
                self.log("warn", f"补录岗位失败：{exc}")
        self.store.set_delivered([job_id], True)


RT: Runtime | None = None

# 记住数据目录的配置键：开发态/打包态谁是真正的 profile 由它决定，
# 见 Runtime.__init__ 里的「自动识别已有登录态」。
DATA_DIR_KEY = "data_dir"


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
        rt.add_tokens(result.tokens, "score")

    # 3.5) 分值门槛：低于设定分值的 JD 不入库
    gate = int(getattr(rt, "min_score", 0) or 0)
    if gate > 1 and result.score < gate:
        no_jd = not str(job.get("jd_text") or "").strip()
        tail = "（详情页没拿到 JD，分数被压低了，重新抓一次通常就好了）" if no_jd else ""
        return _exclude(rt, job, label, f"匹配度 {result.score}/10 低于设定门槛 {gate} 分{tail}")

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
    - 失败时回退到浏览器 profile 的 Cookies 库：先当前数据目录，
      再开发态/打包态另一个目录，最后本机 Chrome、Edge 的 profile
      （只在没登录过时才报「请先登录」）
    """
    import urllib.request

    rt = get_rt()

    def _collect_cookies(domain: str) -> tuple[dict[str, str], list[str]]:
        """按优先级收集该平台 cookie；返回 (cookie 字典, 试过的来源)。"""
        from .browser import latest_profile_cookie_db, profile_dir
        from .cookie_import import find_cookie_dbs, read_cookies_from_profile

        cookies: dict[str, str] = {}

        def _merge(items: list[dict[str, Any]] | None) -> None:
            for c in items or []:
                try:
                    if domain in (c.get("domain") or ""):
                        cookies.setdefault(c.get("name", ""), c.get("value", ""))
                except Exception:
                    continue

        # ① last_auth：登录成功时前端调 /api/login/save 存下来的
        raw = rt.store.get_config_json("last_auth") or {}
        _merge(raw.get("cookies"))
        if cookies:
            return cookies, ["last_auth"]

        # ② 浏览器 profile 的 Cookies 库（按修改时间由新到旧）
        tried: list[str] = []
        try:
            from .browser import PROFILE_DIR_NAME

            candidates = [app_data_dir(), dev_data_dir(), prepared_data_dir()]
            seen: set[str] = set()
            found: list[tuple[float, Path]] = []
            for d in candidates:
                p = profile_dir(d)
                try:
                    key = str(p.resolve()).lower()
                except Exception:
                    key = str(p).lower()
                if key in seen:
                    continue
                seen.add(key)
                db = latest_profile_cookie_db(p)
                if db is not None:
                    found.append((db.stat().st_mtime, p))
            for _, p in sorted(found, reverse=True):
                _merge(read_cookies_from_profile(p, domain))
                tried.append(f"{PROFILE_DIR_NAME}（{p.parent}）")
                if cookies:
                    return cookies, tried
        except Exception:
            pass

        # ③ 本机用户自己日常用的 Chrome / Edge（先在程序里登录过就不会走到这）
        try:
            for db in find_cookie_dbs():
                if cookies:
                    break
                _merge(read_cookies_from_profile(db.parent, domain))
                tried.append(str(db.parent))
        except Exception:
            pass
        return cookies, tried

    def _fetch() -> list[dict[str, Any]]:
        from .browser import LOGIN_COOKIES

        domain, names = LOGIN_COOKIES.get("boss", ("zhipin.com", ()))
        cookies, tried = _collect_cookies(domain)
        if not cookies:
            where = "、".join(tried) if tried else "任何浏览器 profile"
            raise RuntimeError(
                f"没有可用的登录 cookie，请先登录 Boss直聘（已查找 {where}，都没有 Boss 登录态）"
            )

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
        "tokens_total": rt.tokens_total,
        "tokens_greet": rt.tokens_greet,
        "tokens_score": rt.tokens_score,
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


# ----------------------------------------------------------------------
# Boss 消息：会话列表 / 历史消息 / 发送 / 撤回
# ----------------------------------------------------------------------
#
# 这些接口都要驱动真实 Chrome，只能投递到采集线程执行（patchright sync API
# 绑定线程）；前端每 3 秒轮询一次列表，所以接口内部做了 10 秒缓存，避免
# 轮询把浏览器拖住。

_CHAT_STATE: dict[str, Any] = {"ok": None, "error": "", "ts": 0.0}


def _run_chat(fn, timeout: float = 45.0):
    """把聊天任务投递到采集线程，按「有没有进展」而不是「总耗时」判超时。

    patchright 的 sync API 只能在采集线程里用，而 BOSS 聊天页经常一卡就是几十秒
    （点开会话后列表重渲染、切会话时接口回包慢）。固定 timeout 会把正常但要等
    的操作误杀成「执行超时」，所以改成看门狗：
      - 聊天任务在等锁（另一个请求正占着浏览器）→ 继续等；
      - 已经在浏览器里操作，但最后活动时间超过它自己的预算 → 判定卡死；
      - 正常推进时每完成一步都会刷新时间戳。
    """
    rt = get_rt()
    box: dict[str, Any] = {}
    done = threading.Event()
    # 日志里要能看出这一步到底在干什么（_wrap 本身的名字没有信息量）
    label = getattr(fn, "__chat_label__", None) or "Boss 消息"

    def _wrap():
        try:
            box["res"] = fn()
        except Exception as exc:  # noqa: BLE001 - 原样带回调用方
            box["err"] = exc
        finally:
            done.set()

    _wrap.__wrapped_label__ = label
    rt.submit(_wrap)
    deadline = time.time() + timeout
    while not done.wait(0.4):
        last, budget = rt.chat.activity()
        limit = budget if budget > 0 else 30.0
        # 浏览器在同一时刻只能给一个任务用。抓取/打分正在用时消息任务是在排队，
        # 这不是卡死，只能提示「等一下」，绝不能报「浏览器可能卡住了」。
        busy_label, busy_for = rt.worker_busy()
        if busy_label and not last:
            if busy_for > 90:
                raise RuntimeError(
                    f"浏览器正被「{busy_label}」占用（已 {int(busy_for)} 秒），"
                    "消息操作排在它后面。等它跑完再试，或先点「停止抓取」。"
                )
            if time.time() > deadline:
                raise RuntimeError(
                    f"浏览器正被「{busy_label}」占用，消息操作还没轮到。"
                    "等它跑完再试，或先点「停止抓取」。"
                )
            continue
        if last and time.time() - last > limit + 15:
            raise RuntimeError(
                "Boss 消息页没有响应（浏览器可能卡住了），请稍后重试或重启程序"
            )
        if time.time() > deadline:
            raise RuntimeError("Boss 消息页响应太慢（可能需要重新登录），请稍后重试")
    if "err" in box:
        raise box["err"]
    return box.get("res")


@app.get("/api/chat/friends")
def chat_friends(force: bool = False) -> dict[str, Any]:
    """Boss 会话列表（= 所有聊过的 HR）。界面左 1/4 的消息列表用它。"""
    rt = get_rt()
    try:
        def _do():
            return rt.chat.friends(rt.ensure_browser(), force=force)

        _do.__chat_label__ = "读取 Boss 会话列表" + ("（强制刷新）" if force else "")
        items = _run_chat(_do)
        _CHAT_STATE.update({"ok": True, "error": "", "ts": time.time()})
        return {"ok": True, "items": items}
    except Exception as exc:
        msg = str(exc) or exc.__class__.__name__
        _CHAT_STATE.update({"ok": False, "error": msg, "ts": time.time()})
        rt.log("warn", f"拉取 Boss 会话失败：{msg}")
        return {"ok": False, "items": [], "error": msg}


@app.get("/api/chat/messages")
def chat_messages(uid: str, force: bool = False) -> dict[str, Any]:
    """某个会话的历史消息（点开会话时才拉，带缓存）。"""
    rt = get_rt()
    try:
        def _do():
            return rt.chat.messages(rt.ensure_browser(), uid, force=force)

        _do.__chat_label__ = f"打开会话 {uid} 读取聊天记录" + ("（强制刷新）" if force else "")
        items = _run_chat(_do)
        return {"ok": True, "uid": uid, "items": items}
    except Exception as exc:
        msg = str(exc) or exc.__class__.__name__
        rt.log("warn", f"拉取会话消息失败：{msg}")
        return {"ok": False, "uid": uid, "items": [], "error": msg}


class ChatSendReq(BaseModel):
    uid: str
    text: str
    # 前端已知的岗位 ID（可选）：发成功但会话里没 encryptJobId 时用它兜底标记已投递
    job_id: str | None = None


@app.post("/api/chat/send")
def chat_send(req: ChatSendReq) -> dict[str, Any]:
    """给 HR 发消息；发出即把对应岗位标记为「已投递」。"""
    rt = get_rt()

    def _do():
        ctx = rt.ensure_browser()
        res = rt.chat.send(ctx, req.uid, req.text)
        if req.job_id and rt.store.exists(req.job_id):
            rt.store.set_delivered([req.job_id], True)
        return res

    _do.__chat_label__ = f"给 HR {req.uid} 发送消息"

    try:
        res = _run_chat(_do, timeout=90.0)
        return {"ok": True, **res}
    except Exception as exc:
        msg = str(exc) or exc.__class__.__name__
        rt.log("warn", f"发送 Boss 消息失败：{msg}")
        return {"ok": False, "error": msg}


class AutoGreetReq(BaseModel):
    job_id: str


@app.get("/api/auto-greet/next")
def auto_greet_next() -> dict[str, Any]:
    """自动打招呼的下一条候选：待投递里还没聊过 HR 的岗位（按匹配分高→低）。"""
    rt = get_rt()
    rows = rt.store.list_jobs(
        status="pending", platform="boss", include_rejected=False,
        sort="score", order="desc", limit=50,
    )
    items = []
    for r in rows:
        jid = str(r.get("job_id") or "")
        if not jid.startswith("boss:"):
            continue
        items.append({
            "job_id": jid,
            "title": r.get("title") or "",
            "company": r.get("company") or "",
            "salary": r.get("salary") or "",
            "score": r.get("match_score"),
            "encrypt_job_id": jid[5:],
            "hr_name": r.get("hr_name") or "",
            "job_url": r.get("job_url") or "",
        })
    return {"ok": True, "items": items, "tokens_total": rt.tokens_total, "tokens_greet": rt.tokens_greet, "tokens_score": rt.tokens_score}


@app.post("/api/auto-greet/one")
def auto_greet_one(req: AutoGreetReq) -> dict[str, Any]:
    """自动打招呼的单步：找到 HR → 按提示词+JD+技能生成话术 → 发出去。

    刻意做成「一次只处理一条」：浏览器同一时刻只能给一个任务用，
    长流程会把抓取和聊天操作全堵死，由前端按间隔逐条驱动最稳。
    """
    rt = get_rt()
    job = rt.store.get_job(req.job_id)
    if job is None:
        return {"ok": False, "stage": "job", "error": "岗位不在库里了", "done": True}
    if str(job.get("status") or "") == "delivered":
        return {"ok": False, "stage": "skip", "error": "已经标记为已投递，跳过", "skip": True}

    ejid = str(job.get("job_id") or "")[5:] if str(job.get("job_id") or "").startswith("boss:") else ""

    # 1) 找到这个岗位对应的 HR 会话
    def _find():
        ctx = rt.ensure_browser()
        return rt.chat.friends(ctx, force=False)

    _find.__chat_label__ = "自动打招呼：查找 HR 会话"
    try:
        friends = _run_chat(_find, timeout=90.0)
    except Exception as exc:
        return {"ok": False, "stage": "find", "error": str(exc) or exc.__class__.__name__}

    target = None
    for f in friends or []:
        if ejid and str(f.get("encrypt_job_id") or "") == ejid:
            target = f
            break
    if target is None:
        # 没打过招呼的岗位在 Boss 会话列表里根本不存在。既然用户开了「自动打招呼」，
        # 就是要主动发起：直接去岗位详情页点「立即沟通」把这个会话建出来。
        if ejid:
            def _new():
                return rt.chat.start_conversation(rt.ensure_browser(), {"encrypt_job_id": ejid}, ejid)
            _new.__chat_label__ = "自动打招呼：主动建立会话（岗位详情页「立即沟通」）"
            try:
                res = _run_chat(_new, timeout=120.0)
                target = res.get("friend") or None
            except Exception as exc:
                return {"ok": False, "stage": "find",
                        "error": f"没能主动发起会话：{exc}"}
        if target is None:
            brand = str(job.get("company") or "").strip()
            same = [f for f in friends or [] if brand and brand in str(f.get("brand") or "")]
            if len(same) == 1:
                target = same[0]
            else:
                why = (f"这家公司有 {len(same)} 位 HR，无法确定发给谁" if same
                       else "Boss 会话列表里没有这家公司的 HR")
                return {"ok": False, "stage": "find", "error": why, "skip": True}

    # 2) 生成话术（消耗的 token 在 _write_greeting 里累加）
    writer = GreetingWriter(rt.scorer.llm if rt.scorer else None)
    g = _write_greeting(rt, writer, job)
    if not g.text:
        return {"ok": False, "stage": "greet", "error": "生成话术为空", "tokens": g.tokens}

    # 3) 发出去
    uid = str(target.get("uid"))

    def _send():
        return rt.chat.send(rt.ensure_browser(), uid, g.text)

    _send.__chat_label__ = f"自动打招呼：给 HR {uid} 发送消息"
    try:
        _run_chat(_send, timeout=90.0)
    except Exception as exc:
        return {"ok": False, "stage": "send", "error": str(exc) or exc.__class__.__name__, "tokens": g.tokens}

    if rt.store.exists(req.job_id):
        rt.store.set_delivered([req.job_id], True)
    rt.log("info", f"自动打招呼：{job.get('company') or ''} · {job.get('title') or ''}（{g.tokens} token）")
    return {
        "ok": True,
        "job_id": req.job_id,
        "uid": uid,
        "text": g.text,
        "tokens": g.tokens,
        "source": g.source,
        "note": g.note,
        "company": job.get("company") or "",
        "title": job.get("title") or "",
        "tokens_total": rt.tokens_total,
        "tokens_greet": rt.tokens_greet,
        "tokens_score": rt.tokens_score,
    }


class ChatRecallReq(BaseModel):
    uid: str


class GreetReq(BaseModel):
    job_id: str | None = None
    uid: str | None = None  # 也可以只按会话 uid 生成（会话里能拿到岗位）
    encrypt_job_id: str | None = None


@app.post("/api/greet")
def greet(req: GreetReq) -> dict[str, Any]:
    """根据（提示词 + 岗位 JD + 我的技能）生成一句打招呼，返回正文与本次 token。

    只生成文本，不发送；前端填进输入框由用户确认后再走 /api/chat/send。
    """
    rt = get_rt()
    job: dict[str, Any] | None = None
    ejid = req.encrypt_job_id or ""
    if req.job_id:
        job = rt.store.get_job(req.job_id)
        if not ejid and req.job_id.startswith("boss:"):
            ejid = req.job_id[5:]
    if job is None and ejid:
        job = rt.store.get_job(f"boss:{ejid}")
    if job is None:
        # 会话里没匹配到已入库岗位时，至少用会话信息拼一个最简岗位
        title = company = None
        if req.uid:
            try:
                cache = getattr(rt.chat, "_friends", None) or []
                for f in cache:
                    if str(f.get("uid")) == str(req.uid):
                        title = f.get("job_title") or f.get("source_title")
                        company = f.get("brand")
                        break
            except Exception:
                pass
        job = {"title": title or "", "company": company or "", "jd_text": None, "salary": None}
    writer = GreetingWriter(rt.scorer.llm if rt.scorer else None)
    g = _write_greeting(rt, writer, job)
    if g.tokens:
        rt.add_tokens(g.tokens, "greet")
        rt.log("info", f"生成打招呼（{g.tokens} token）：{g.text[:40]}…")
    else:
        rt.log("warn", f"生成打招呼（未消耗 token，来源={g.source}）：{g.text[:40]}…")
    return {
        "ok": True,
        "text": g.text,
        "tokens": g.tokens,
        "source": g.source,
        "note": g.note,
        "tokens_total": rt.tokens_total,
        "tokens_greet": rt.tokens_greet,
        "tokens_score": rt.tokens_score,
    }


def _greeting_profile(rt) -> dict[str, Any]:
    """画像 + 「配置」里手填的技能（留空就用简历自带的技能）。"""
    return profile_with_skills(PROFILE, rt.store.get_config("skills"))


def _write_greeting(rt, writer, job: dict[str, Any]):
    """按「提示词 + JD + 技能」写一条招呼语，顺手把 token 记进累计值。"""
    g = writer.write(rt.store.get_config("greet_prompt"), job, _greeting_profile(rt))
    if g.tokens:
        rt.add_tokens(g.tokens, "greet")
    return g


@app.get("/api/chat/resumes")
def chat_resumes(force: bool = False) -> dict[str, Any]:
    """Boss 上已上传的附件简历列表（点「发简历」时给用户选一份）。"""
    rt = get_rt()

    def _run():
        ctx = rt.ensure_browser()
        return rt.chat.resumes(ctx, force=force)

    _run.__chat_label__ = "读取 Boss 附件简历列表"
    try:
        items = _run_chat(_run, timeout=90.0)
        return {"ok": True, "items": items}
    except Exception as exc:
        return {"ok": False, "items": [], "error": str(exc)}


class ChatResumeReq(BaseModel):
    uid: str
    resume_id: str | None = None


@app.post("/api/chat/resume")
def chat_send_resume(req: ChatResumeReq) -> dict[str, Any]:
    """给某位 HR 发送附件简历（Boss 会弹确认框，这里自动确认）。"""
    rt = get_rt()

    def _run():
        ctx = rt.ensure_browser()
        return rt.chat.send_resume(ctx, req.uid, req.resume_id or "")

    _run.__chat_label__ = f"给 HR {req.uid} 发送附件简历"
    try:
        return _run_chat(_run, timeout=90.0)
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


@app.post("/api/chat/recall")
def chat_recall(req: ChatRecallReq) -> dict[str, Any]:
    """撤回自己最后发的消息。

    注意：Boss 求职端网页版没有撤回入口（见 scrapers/boss_chat.py 的 recall），
    这里会把明确原因返回给前端，不再假装成功。
    """
    rt = get_rt()
    try:
        def _do():
            return rt.chat.recall(rt.ensure_browser(), req.uid)

        _do.__chat_label__ = f"撤回消息 {req.uid}"
        res = _run_chat(_do, timeout=90.0)
        return {"ok": True, **res}
    except Exception as exc:
        msg = str(exc) or exc.__class__.__name__
        rt.log("warn", f"撤回失败：{msg}")
        return {"ok": False, "error": msg}


class ChatOpenReq(BaseModel):
    uid: str | None = None
    encrypt_job_id: str | None = None
    brand: str | None = None
    # True = 用户点了「打招呼」，明确要主动发起：没有会话就去详情页点「立即沟通」建一个
    force_new: bool = False


@app.post("/api/chat/open")
def chat_open(req: ChatOpenReq) -> dict[str, Any]:
    """按岗位找 HR 会话（未投递卡片点开时用）。

    Boss 只有在「打过招呼」之后才会为这个岗位建立会话，没聊过的岗位在会话列表里
    根本不存在。所以找不到时**不再报「还没聊过」**，而是：
      1) 按公司名挑出该公司的 HR（可能有多位），交给界面让人选；
      2) 一位都没有就直接返回「可选 HR 列表」，界面展开消息面板让人挑一位开始聊。
    """
    rt = get_rt()
    try:
        def _do():
            ctx = rt.ensure_browser()
            return rt.chat.find_friend(
                ctx,
                uid=req.uid,
                encrypt_job_id=req.encrypt_job_id or "",
                brand=req.brand or "",
            ), rt.chat.friends(ctx, force=False)

        _do.__chat_label__ = f"查找 {req.brand or ''} 的 HR 会话".strip()
        friend, friends = _run_chat(_do)
        if friend:
            return {"ok": True, "found": True, "friend": friend}

        # 用户主动点「打招呼」：没有会话就自己去建一个（岗位详情页「立即沟通」），
        # 建完已经进了聊天页并选中这位 HR，直接返回，不需要再让用户挑
        if req.force_new and (req.encrypt_job_id or "").strip():
            ejid = (req.encrypt_job_id or "").strip()

            def _start():
                ctx2 = rt.ensure_browser()
                seed: dict[str, Any] = {"encrypt_job_id": ejid, "brand": req.brand or ""}
                if req.uid:
                    seed["uid"] = req.uid
                return rt.chat.start_conversation(ctx2, seed, ejid)

            _start.__chat_label__ = f"主动联系 {req.brand or '该岗位'} 的 HR 并建立会话"
            res = _run_chat(_start, timeout=120.0)
            return {
                "ok": True,
                "found": True,
                "friend": res.get("friend") or {},
                "created": bool(res.get("created")),
            }

        # 兜底一：同一家公司的 HR（会话名里带公司名）
        brand = (req.brand or "").strip()
        same = [f for f in friends or [] if brand and brand in str(f.get("brand") or "")]
        if len(same) == 1:
            return {"ok": True, "found": True, "friend": same[0], "fallback": "brand"}
        return {
            "ok": False,
            "found": False,
            "candidates": [
                {"uid": f.get("uid"), "name": f.get("name"), "title": f.get("title"),
                 "brand": f.get("brand"), "avatar": f.get("avatar")}
                for f in same
            ],
            "error": (
                "这个岗位还没和 HR 建立会话，右侧列出的都可以直接聊"
                if not same else
                f"这家公司有 {len(same)} 位 HR，右侧已列出，挑一位开始聊"
            ),
        }
    except Exception as exc:
        msg = str(exc) or exc.__class__.__name__
        rt.log("warn", f"查找 HR 会话失败：{msg}")
        return {"ok": False, "found": False, "error": msg}


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
        "greet_prompt": rt.store.get_config("greet_prompt") or DEFAULT_PROMPT,
        "greet_prompt_default": DEFAULT_PROMPT,
        "skills": rt.store.get_config("skills") or "",
        "score_mode": (rt.store.get_config("score_mode") or "rule"),
        "llm_prefilter": getattr(rt.scorer, "llm_prefilter", 5),
        "tokens_total": rt.tokens_total,
        "tokens_greet": rt.tokens_greet,
        "tokens_score": rt.tokens_score,
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
    greet_prompt: str | None = None
    skills: str | None = None
    score_mode: str | None = None
    llm_prefilter: int | None = None
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
    if req.greet_prompt is not None:
        rt.store.set_config("greet_prompt", req.greet_prompt)
    if req.skills is not None:
        rt.store.set_config("skills", req.skills)
    if req.score_mode is not None:
        mode = "llm" if req.score_mode.strip() == "llm" else "rule"
        rt.store.set_config("score_mode", mode)
    if req.llm_prefilter is not None:
        rt.store.set_config("llm_prefilter", str(max(0, min(10, int(req.llm_prefilter)))))
    if req.theme is not None:
        rt.store.set_config("theme", req.theme)
    if req.exclude_keywords is not None:
        rt.store.set_config_json("exclude_keywords", req.exclude_keywords)
    if req.exclude_rules is not None:
        rt.store.set_config_json("exclude_rules", req.exclude_rules)
    rt.rebuild_scorer()
    return {
        "ok": True,
        "llm_enabled": rt.scorer.llm is not None,
        "score_mode": "llm" if getattr(rt.scorer, "use_llm", False) else "rule",
    }


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
                rt.add_tokens(res.tokens, "score")
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

    # 启动横幅先落盘：打包态后端没有控制台，「用户点了开始然后程序没了」
    # 这类问题只能靠这份日志倒推死在哪一步。
    try:
        log_to_file(
            "info",
            "boot",
            f"后端启动 pid={os.getpid()} frozen={getattr(sys, 'frozen', False)} "
            f"AJA_DATA_DIR={os.environ.get('AJA_DATA_DIR', '')}",
        )
        log_to_file("info", "boot", f"数据目录={app_data_dir()} 日志={log_file_path()}")
    except Exception:
        pass
    # 注意：这里**不**实例化 Runtime。打包态下它可能在做浏览器/profile 探测，
    # 启动阶段任何毛病都不该拖慢或拖挂 uvicorn 起来，按需再建。
    port = int(os.environ.get("AJA_PORT", "0")) or pick_port()
    # Tauri 侧从 stdout 读这一行拿到端口
    print(f"AJA_BACKEND_PORT={port}", flush=True)
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="warning")


if __name__ == "__main__":
    main()
