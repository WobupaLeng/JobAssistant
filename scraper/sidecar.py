"""抓取 sidecar：JSON Lines over stdin/stdout。

SPEC-002 第 2.1 节。**约束**：
- 不使用 HTTP、不占端口，只读写标准输入输出；
- 无状态：不打分、不落库、不读配置——业务判定全部归 Rust；
- 单行上限 1 MB，超限截断，不静默丢弃。

进程模型：
  Rust 主进程 --spawn--> python sidecar.py
  Rust --stdin--> {"id":"c1","cmd":"scrape.start",...}
  sidecar --stdout--> {"event":"log",...} / {"event":"job.scraped",...} / {"id":"c1","ok":true}

抓取跑在**独立线程**里，主线程只做 stdin 读循环。这样「停止」命令能在抓取
进行中被立即处理——如果抓取占着主线程，用户在界面上点停止要等到整页抓完
才生效，实际体验是「点了没反应」。
"""
from __future__ import annotations

import json
import os
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

MAX_LINE_BYTES = 1024 * 1024

# 浏览器常驻单车道（scraper/AGENTS.md §4.10：碰浏览器的命令必须排队到同一个
# 工作线程）。patchright 的 sync API 绑死创建它的线程：旧实现每条命令各起一个
# 新线程，首条命令的线程一退出，_pw/_ctx 就成了尸体——第二条命令探活必报
# 「Playwright connection closed」，随后误判「浏览器已失效」去 teardown 重启，
# 撞上仍活着的 Chrome 单例窗（「正在现有的浏览器会话中打开」→ TargetClosedError）。
# 改为常驻单线程 executor：所有浏览器命令在同一条线程上排队，连接与进程同寿。
_browser_q = ThreadPoolExecutor(max_workers=1, thread_name_prefix="browser")


class _WorkerHandle:
    """把长任务 Future 包装成旧 _worker 线程的接口（is_alive）。"""

    def __init__(self, fut: Any) -> None:
        self._fut = fut

    def is_alive(self) -> bool:
        return not self._fut.done()


# 抓取运行态。_stop 是「请求停止」，_sleep 期间每 0.2s 检查一次，
# 保证点停止后最多 0.2 秒就退出等待。
_stop = threading.Event()
_worker: _WorkerHandle | None = None
_worker_lock = threading.Lock()
# 谁占着浏览器：`"scrape"` / `"login"` / `None`。只用于把「占用中」的
# 错误提示说清楚——登录期间没有「停止」按钮可按，照抄抓取的措辞会把
# 用户引到一条走不通的路上（见 busy_reason）。
_occupant: str | None = None

# 登录态查询：Rust 侧轮询 login.start 后用。
# greenlet 约束：patchright 的 sync API 绑死创建它的线程，所以浏览器只在
# _browser_q 的常驻工作线程里碰——所有浏览器命令排队，不并发。
_pw: Any = None          # 当前 sync_playwright 实例
_ctx: Any = None         # 当前 CDP 连接得到的 BrowserContext
_cdp_proc: Any = None    # 本模块拉起的 CDP Chrome 进程（Popen），进程寿命与应用一致
_cdp_port: int | None = None  # 其动态分配的 127.0.0.1 端口
_pw_lock = threading.RLock()  # 可重入：聊天车道在持锁时会再调用 ensure_context


def emit(payload: dict) -> None:
    """写一行 JSON 到 stdout 并立即 flush。

    必须 flush：父进程按行读取，缓冲会导致 Rust 侧一直阻塞等不到回执。
    """
    line = json.dumps(payload, ensure_ascii=False)
    if len(line.encode("utf-8")) > MAX_LINE_BYTES:
        payload = {"event": "log", "data": {"level": "error", "msg": "单条消息超过 1MB 上限，已丢弃"}}
        line = json.dumps(payload, ensure_ascii=False)
    sys.stdout.write(line + "\n")
    sys.stdout.flush()


def log(level: str, msg: str) -> None:
    emit({"event": "log", "data": {"level": level, "msg": clamp_text(msg, 4000)}})


def clamp_text(text: str, limit: int = MAX_LINE_BYTES // 2) -> str:
    """按字符边界截断超长文本，避免把 UTF-8 多字节字符切一半。"""
    if len(text.encode("utf-8")) <= limit:
        return text
    out = text.encode("utf-8")[:limit].decode("utf-8", errors="ignore")
    return out + "…[截断]"


def interruptible_sleep(seconds: float) -> bool:
    """睡眠；期间被要求停止则提前返回 True。

    不能用 time.sleep：用户点「停止」后最多等 0.2 秒就要生效，
    否则随机反爬等待（最长 9.8 秒）会让界面看起来卡死。
    """
    end = time.time() + seconds
    while True:
        if _stop.is_set():
            return True
        left = end - time.time()
        if left <= 0:
            return False
        _stop.wait(min(left, 0.2))


def data_dir() -> Path:
    """浏览器持久化目录。

    必须与现行版 Python 应用**共用**同一个 profile——否则用户要重新扫码登录，
    而且新 profile 更容易被 Boss 判定为陌生设备。
    """
    env = os.environ.get("AJA_DATA_DIR")
    if env:
        return Path(env)
    local = os.environ.get("LOCALAPPDATA") or str(Path.home() / "AppData" / "Local")
    return Path(local) / "ai-job-assistant"


def profile_dir() -> Path:
    """Chrome 的持久化 profile 目录（`<data_dir>/data/browser-profile`）。

    与 `data_dir()` 的区别不是风格问题：`data_dir()` 是**应用根目录**，
    里面还有 `data/job_assistant.db`。把根目录当 `--user-data-dir` 传给
    Chrome 会有两个后果——Chrome 把 `Default/`、`Local State` 等洒在应用数据
    根下，且一旦该目录被别的 Chrome 会话认可（实测报「正在现有的浏览器会话中
    打开」），`launch_persistent_context` 会立刻 `Target page, context or
    browser has been closed`，表现为聊天与求职期望全部失败。

    路径必须与现行版 `backend/app/browser.py` 一致：`data/browser-profile`。

    按系统默认浏览器分目录：Chrome 沿用原路径（已有登录态不受影响）；Edge 用
    `browser-profile-edge`。cookie 由各浏览器自己的密钥加密，互不可读，混用会让
    登录静默失效，所以必须分开。
    """
    from browser import _default_browser_kind

    if _default_browser_kind() == "edge":
        return data_dir() / "data" / "browser-profile-edge"
    return data_dir() / "data" / "browser-profile"


def ensure_context(log_fn: Any) -> Any:
    """确保浏览器上下文已启动并返回它。**必须在工作线程里调用**。"""
    global _pw, _ctx
    if _ctx is not None:
        # 探活：浏览器被用户手动关掉后 ctx.pages 仍可能返回旧列表，
        # 所以必须真正问一次浏览器。用 cookies() 而非 new_page()+close()
        # ——后者会让登录流程每秒闪一个新标签页。
        # 单次探活失败不等于浏览器没了（可能是瞬时的 driver 抖动或窗口刚被手动关），
        # 直接 teardown 会 `_pw.stop()` 连带关掉应用拉起的 Chrome。先重试一次，
        # 两次都失败才认定失效，且只丢引用、不在存活的 driver 上 stop。
        for _attempt in range(2):
            try:
                _ctx.cookies()
                return _ctx
            except Exception:
                time.sleep(0.5)
        log_fn("warn", "浏览器上下文已失效，重新连接")
        _ctx = None
        _teardown_playwright()
    import browser as browser_mod

    global _cdp_proc, _cdp_port
    with _pw_lock:
        # 已有可连的 CDP Chrome（上一轮遗留或本模块拉起）就直接复用，不再起新的。
        # 判据必须**先看进程**：同一 profile 只能有一个 Chrome 持锁，新起的进程
        # 会把请求移交给旧进程后自己退出，而用户看到的是**又一个浏览器窗口**。
        # 只有确认进程已死（poll 有退出码）才允许再起；否则只做端口探活。
        if _cdp_proc is None or _cdp_proc.poll() is not None or _cdp_port is None:
            _cdp_proc, _cdp_port = browser_mod.start_cdp_chrome(str(profile_dir()))
        elif not _port_listening(_cdp_port):
            # 进程活着但端口短暂不响应：等一轮再判定，别急着再 spawn 一个 Chrome
            for _wait in range(10):
                time.sleep(0.3)
                if _port_listening(_cdp_port) or _cdp_proc.poll() is not None:
                    break
            if _cdp_proc.poll() is not None:
                _cdp_proc, _cdp_port = browser_mod.start_cdp_chrome(str(profile_dir()))
        if _pw is None:
            from patchright.sync_api import sync_playwright

            _pw = sync_playwright().start()
        try:
            _ctx = browser_mod.connect_cdp_context(_pw, _cdp_port)
        except Exception:
            # 连接失败可能是 driver 断了：清掉死 _pw，下次重新拉起
            _teardown_playwright()
            raise
    return _ctx


def _teardown_playwright() -> None:
    """丢弃已失效的 Playwright 连接，让下次 `ensure_context` 重新拉起。

    只清 `_ctx` 是不够的：`Playwright connection closed` 说明 **driver 进程
    本身**断了（`_pw` 已死）。此时 `_pw` 非空，`ensure_context` 会跳过
    `sync_playwright().start()`，直接把死连接的 `_ctx` 拿去用，于是每条聊天
    命令都报同一句话——死因看起来像「浏览器没开」，其实是被复用了的尸体。

    stop 失败也要把引用清掉：调用方要的是「下次能重新起来」。
    """
    global _pw, _ctx
    _ctx = None
    if _pw is not None:
        try:
            _pw.stop()
        except Exception:
            pass  # 连接已断时 stop 必抛，这正是我们要丢掉的
    _pw = None


def _port_listening(port: int) -> bool:
    """探测 127.0.0.1:port 是否有 CDP 监听（不发请求，只判断端口是否通）。"""
    import socket

    try:
        with socket.create_connection(("127.0.0.1", port), timeout=1):
            return True
    except OSError:
        return False


def _browser_running() -> bool:
    """浏览器进程**现在**是否还在跑。只问进程与端口，不碰 playwright。

    被动查询登录态（`session_open` 不带 `page`，界面上的「重试」就走这条）必须
    先问这一句：`ensure_context` 见到没有进程就 `start_cdp_chrome`，于是写着
    「重试」的按钮变成了启动按钮——用户关掉浏览器后点重试，窗口又自己弹回来
    （用户 m01706：重试只能检查状态，不能打开浏览器）。

    不探 `_ctx.cookies()`：patchright 的 sync API 绑死创建它的线程，这里可能
    在别的线程被调用；进程与端口是跨线程安全的判据。
    """
    proc = _cdp_proc
    if proc is None or proc.poll() is not None:
        return False
    port = _cdp_port
    return bool(port) and _port_listening(int(port))


def _shutdown_cdp_chrome() -> None:
    """优雅关闭本模块拉起的 CDP Chrome（应用退出时调用）。

    先经 CDP 的 Browser.close 让 Chrome 正常落盘 cookie，再兜底 terminate。
    直接 terminate 会丢 cookie（之前实测过：硬杀后 Cookies 库 0 行）。
    """
    global _cdp_proc, _cdp_port
    if _cdp_proc is None:
        return
    try:
        if _cdp_port:
            from patchright.sync_api import sync_playwright

            with sync_playwright() as pw:
                b = pw.chromium.connect_over_cdp(f"http://127.0.0.1:{_cdp_port}")
                b.close()
    except Exception:
        pass
    try:
        _cdp_proc.wait(timeout=8)
    except Exception:
        _cdp_proc.terminate()
    _cdp_proc = None
    _cdp_port = None


def _job_to_event(item: dict[str, Any]) -> dict[str, Any]:
    """把抓取到的原始 dict 转成 job.scraped 事件。"""
    keep = (
        "job_id", "platform", "title", "company", "salary", "experience", "degree",
        "location", "job_url", "jd_text", "company_industry", "company_scale",
        "company_stage", "company_intro", "company_address", "hr_name",
        "hr_position", "hr_active_status", "founded_years", "reg_capital",
        "matched_keyword",
    )
    data = {k: item.get(k) for k in keep if item.get(k) not in (None, "")}
    if "jd_text" in data:
        data["jd_text"] = clamp_text(str(data["jd_text"]))
    if "company_intro" in data:
        data["company_intro"] = clamp_text(str(data["company_intro"]), 4000)
    return {"event": "job.scraped", "data": data}


def _run_scrape(payload: dict[str, Any]) -> None:
    """真正跑一次抓取。异常一律转成 log 事件，绝不让线程静默死掉。"""
    global _occupant
    # 无论哪条路径收尾（自然完成 / 早退 / 异常 / 用户停止），finally 都必须
    # 发 scrape.finished：Rust 侧靠它清 running，缺了界面永停「抓取中」。
    count = 0
    err = ""
    try:
        import base as base_mod
        import boss as boss_mod
        import browser as browser_mod

        cfg = base_mod.ScrapeConfig(
            keywords=list(payload.get("keywords") or []),
            city=payload.get("city") or "101020100",
            salary=list(payload.get("salary") or []),
            experience=list(payload.get("experience") or []),
            degree=list(payload.get("degree") or []),
            scale=list(payload.get("scale") or []),
            stage=list(payload.get("stage") or []),
            industry=list(payload.get("industry") or []),
            # 协议新增键（与 protocol.rs Command::ScrapeStart 逐字对齐）：
            # 1901 全职 / 1902 兼职 / 1903 实习；缺省空串 = 不限。
            job_type=payload.get("job_type") or "",
            max_pages=int(payload.get("max_pages") or 5),
            max_cards=int(payload.get("max_cards") or 100),
            daily_limit=int(payload.get("daily_limit") or 100),
        )
        if not cfg.keywords:
            err = "没有关键词，无法搜索"
            log("error", err)
            return

        log("info", f"数据目录：{data_dir()}")
        ctx = ensure_context(log)

        # 开抓前先确认登录态：未登录时 Boss 会把搜索页重定向到
        # /web/user/，空跑几分钟才报「没有拿到任何岗位」，用户不知道要干什么
        if not browser_mod.detect_logged_in(ctx, "boss"):
            err = "尚未登录 Boss，请先执行登录（login.start）并扫码"
            log("error", err)
            return

        page = browser_mod.reuse_or_new_page(ctx)
        try:
            page.bring_to_front()
        except Exception:
            pass
        page.set_default_timeout(60_000)

        scraper = boss_mod.BossScraper(
            cfg=cfg,
            log=log,
            should_stop=lambda: _stop.is_set(),
            sleep=interruptible_sleep,
            on_job=lambda item: emit(_job_to_event(item)),
        )
        try:
            count = scraper.run(page)
            log("info", f"抓取结束，共 {count} 条")
            if scraper.risk_reason:
                # 检测即停的事件帧：Rust 侧据此停止 running 并把原因交给界面提示恢复
                emit({
                    "event": "scrape.risk",
                    "data": {"platform": "boss", "reason": scraper.risk_reason, "url": page.url},
                })
        finally:
            try:
                page.close()
            except Exception:
                pass
    except Exception as exc:
        err = f"{type(exc).__name__}: {exc}"
        log("error", f"抓取异常：{err}")
    finally:
        # 放手时清空占用标记，否则「正在抓取」会一直挂在提示语里
        _occupant = None
        # 完成帧（additive 事件，与 protocol.rs Event::ScrapeFinished 逐字对齐）：
        # count 是 sidecar 自报数（权威入库计数在 Rust）；stopped 区分用户停止；
        # error 非空 = 异常收尾。stdout 可能已被父进程关闭，发不出去就算了。
        try:
            emit({
                "event": "scrape.finished",
                "data": {"count": count, "stopped": _stop.is_set(), "error": err},
            })
        except Exception:
            pass


def _run_login(payload: dict[str, Any]) -> None:
    """打开登录页并轮询登录态，直到成功 / 超时 / 被停止。

    为什么不复用现行版那套「generator + 投递到采集线程」的写法：
    那套复杂度是为了跨线程调用 patchright，而 sidecar 里浏览器本来就只
    在这一个工作线程上碰，不需要跨线程投递。
    """
    global _ctx
    try:
        import browser as browser_mod

        platform = payload.get("platform") or "boss"
        timeout_s = int(payload.get("timeout_sec") or 900)

        log("info", f"准备打开 {platform} 登录页")
        ctx = ensure_context(log)
        page = browser_mod.find_page_for(ctx, platform, log)
        try:
            page.bring_to_front()
        except Exception:
            pass
        # `find_page_for` 会复用任意该平台域的标签页（比如上一轮停在的岗位列表），
        # 那样「点登录」看起来毫无反应——用户点的是登录，必须看到登录页。
        if not browser_mod.detect_logged_in(ctx, platform):
            try:
                page.goto(
                    browser_mod.PLATFORM_HOME[platform],
                    wait_until="domcontentloaded",
                    timeout=30_000,
                )
            except Exception as exc:
                log("warn", f"打开登录页失败：{type(exc).__name__}: {exc}")

        emit({"event": "login.state", "data": {
            "platform": platform, "state": "waiting",
            "msg": "请在弹出的窗口里扫码登录（可手动处理验证码）",
        }})
        log("info", "登录窗口已打开，等待扫码")

        deadline = time.time() + timeout_s
        while time.time() < deadline and not _stop.is_set():
            if browser_mod.detect_logged_in(ctx, platform):
                emit({"event": "login.state", "data": {
                    "platform": platform, "state": "logged_in", "msg": "登录成功",
                }})
                log("info", "登录成功")
                after = browser_mod.PLATFORM_AFTER_LOGIN.get(platform)
                if after:
                    try:
                        page.goto(after, wait_until="domcontentloaded", timeout=30_000)
                    except Exception:
                        pass
                return
            # 用户关掉登录窗口 = 放弃这次登录：**不自动重开**。早先「关一个弹一个」
            # 的重开兜底在 F5 不再关 sidecar 之后会持续冒窗。
            try:
                ctx.cookies()
            except Exception:
                log("info", "登录窗口已关闭，结束本次登录")
                emit({"event": "login.state", "data": {
                    "platform": platform, "state": "closed",
                    "msg": "登录窗口已关闭，想登录请再点「扫码登录」",
                }})
                return
            _stop.wait(1.5)

        if _stop.is_set():
            state, msg = "closed", "已取消登录"
        else:
            state, msg = "closed", f"登录超时（{timeout_s // 60} 分钟）"
        emit({"event": "login.state", "data": {
            "platform": platform, "state": state, "msg": msg,
        }})
        log("warn", msg)
    except Exception as exc:
        log("error", f"登录异常：{type(exc).__name__}: {exc}")
        emit({"event": "login.state", "data": {
            "platform": payload.get("platform") or "boss",
            "state": "error", "msg": f"{type(exc).__name__}: {exc}",
        }})
    finally:
        # 登录结束（成功/超时/取消/异常）后立刻放手，聊天与期望不必再等
        _occupant = None


def busy_reason(action: str) -> str:
    """浏览器被别的任务占着时，给一句**可执行**的说明。

    早先所有占用场景都回「正在抓取或登录，请先停止再操作聊天」。问题是登录
    期间根本停不下来：`stop_scrape` 会让登录循环误报「已取消登录」，而登录
    正是用户想做的事。教训——提示语必须指向一条真能走通的路。
    """
    if _occupant == "login":
        return f"正在登录：请在弹出的浏览器窗口里完成扫码，登录结束后再{action}"
    if _occupant == "scrape":
        return f"正在抓取，请先点「停止」再{action}"
    return f"浏览器正忙，请稍后再{action}"


def _busy_now(action: str) -> str | None:
    """主线程预检：长任务占着浏览器时立即回 busy，不让命令排到几分钟之后。"""
    with _worker_lock:
        if _worker is not None and _worker.is_alive():
            return busy_reason(action)
    return None


def start_scrape(payload: dict[str, Any]) -> None:
    """在后台线程启动抓取。已在跑就拒绝，避免两个浏览器抢同一个 profile。"""
    global _worker
    global _occupant
    # 上一轮已请求停止、还在收尾（每轮检查点 0.2 s）时，「继续抓取」若直接判 busy
    # 会被静默丢弃：界面显示「已启动」，实际一条都没抓。先等它真的退出去。
    if _stop.is_set():
        _wait_worker_done(20.0)
    with _worker_lock:
        if _worker is not None and _worker.is_alive():
            log("warn", "已有抓取在进行，忽略本次开始请求")
            return
        _stop.clear()
        _occupant = "scrape"
        _worker = _WorkerHandle(_browser_q.submit(_run_scrape, payload))


def _wait_worker_done(timeout_s: float) -> None:
    """等上一轮抓取线程退出。**不持锁等**：持锁会让收尾线程与自己互锁。"""
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        with _worker_lock:
            alive = _worker is not None and _worker.is_alive()
        if not alive:
            return
        time.sleep(0.05)
    log("warn", "上一轮抓取迟迟没有退出，本次开始请求按忙处理")


def start_login(payload: dict[str, Any]) -> None:
    """在后台线程打开登录页。抓取在跑就拒绝；上一轮登录还在等则抢占重开。"""
    global _worker
    global _occupant
    with _worker_lock:
        if _worker is not None and _worker.is_alive():
            if _occupant != "login":
                log("warn", "已有抓取任务在进行，忽略本次登录请求")
                return
            # 上一轮登录最长会等 15 分钟（扫码超时）。用户又点了登录说明他要重来，
            # 不抢占的话这次点击会被静默丢弃——界面上就是「点了没反应」。
            log("info", "上一轮登录仍在等待，先结束它再重新打开登录页")
            _stop.set()
            deadline = time.time() + 3.0
            while _worker is not None and _worker.is_alive() and time.time() < deadline:
                time.sleep(0.05)
            if _worker is not None and _worker.is_alive():
                log("warn", "上一轮登录还没退出，请稍后再点「扫码登录」")
                return
        _stop.clear()
        _occupant = "login"
        _worker = _WorkerHandle(_browser_q.submit(_run_login, payload))


def stop_scrape() -> None:
    """请求停止。只是置位，真正退出由抓取线程在下一次检查点完成。"""
    _stop.set()
    log("info", "已请求停止抓取")


# ---------------------------------------------------------------- 聊天
#
# 聊天命令与抓取命令走**同一把 _worker_lock**：patchright 的 sync API 绑死
# 创建它的线程，浏览器只能有一个主人。抓取进行中发起的聊天会等抓取那一轮
# 结束才执行——这是刻意的，宁可排队也不能碰线程铁律
# （AGENTS.md 2.3「严禁跨线程复用 Playwright 对象」）。

_chat: Any = None      # 延迟创建的 ChatClient
_chat_busy = threading.Lock()   # 同一时刻只允许一条聊天命令在跑

# 车道 B（聊天）专属线程。patchright 的 sync API 绑死创建它的线程，所以聊天必须
# 有自己的 sync_playwright 连接与线程，不能与抓取车道 A（_browser_q）共用。
# 两车道各自 connect_over_cdp 到同一个 CDP Chrome 端口，互不阻塞。
_chat_q = ThreadPoolExecutor(max_workers=1, thread_name_prefix="chat")
_chat_pw: Any = None     # 聊天线程专属的 sync_playwright 实例
_chat_ctx: Any = None    # 聊天线程专属的 BrowserContext（经 CDP 连接）

# 车道 C（只读探测）：登录态查询不能排在抓取后面，也不必抢占聊天车道的主线程。
# 它只把任务转投 `_chat_q` 等待结果（patchright 对象绑死线程），自己不碰浏览器。
_probe_q = ThreadPoolExecutor(max_workers=1, thread_name_prefix="probe")


def _chat_client() -> Any:
    global _chat
    if _chat is None:
        import chat as chat_mod

        _chat = chat_mod.ChatClient(log)
    return _chat


def _with_chat(fn: Any) -> Any:
    """在**持有浏览器的工作线程**里执行一个聊天动作。

    串行执行而不是并发：`_chat_busy` 保证同一时刻只有一条聊天命令在动浏览器，
    `_worker_lock` 保证它不会和抓取/登录抢同一个 context。
    """
    # 车道 B：不抢 _worker_lock，因此抓取进行中也能执行聊天（这就是双车道的意义）
    with _chat_busy:
        ctx = _ensure_chat_context()
        return fn(ctx)


def _ensure_chat_context() -> Any:
    """聊天车道专属的 BrowserContext：经 CDP 连到已拉起的同一端口，与抓取各自独立。

    必须在聊天线程（_chat_q）里调用。端口由 ensure_context 首次拉起 Chrome 后记录在 _cdp_port。
    """
    global _chat_pw, _chat_ctx
    import browser as browser_mod

    if _chat_ctx is not None:
        try:
            _chat_ctx.cookies()
            return _chat_ctx
        except Exception:
            _chat_ctx = None
            try:
                if _chat_pw is not None:
                    _chat_pw.stop()
            except Exception:
                pass
            _chat_pw = None
    # 双车道可能同时首次进来：必须与 ensure_context 共用 _pw_lock 串行拉起 Chrome，
    # 否则两边都判定「未拉起」，各起一个 Chrome，端口互相覆盖（实测 ECONNREFUSED）。
    with _pw_lock:
        if _cdp_proc is None or _cdp_proc.poll() is not None or _cdp_port is None:
            ensure_context(log)
    from patchright.sync_api import sync_playwright

    if _chat_pw is None:
        _chat_pw = sync_playwright().start()
    _chat_ctx = browser_mod.connect_cdp_context(_chat_pw, _cdp_port)
    return _chat_ctx


def _probe_logged_in_via_chat(platform: str) -> bool:
    """在聊天车道探测登录态：车道 A 被抓取/登录占着时也能答话。

    必须投递到 `_chat_q`：patchright 的 sync 对象绑死创建它的线程，`_chat_ctx`
    只能由聊天线程创建与使用。聊天线程排着长命令时 `result(timeout)` 会抛
    TimeoutError，由调用方降级成 unknown——不能降级成 no_login，那会把
    「查不到」误报成「没登录」。
    """
    def _probe() -> bool:
        import browser as browser_mod

        return browser_mod.detect_logged_in(_ensure_chat_context(), platform)

    return bool(_chat_q.submit(_probe).result(timeout=8))


def _run_login_probe(cmd: dict[str, Any], platform: str) -> None:
    """`login.status` 与被动 `session.open` 的共用回执：只读 cookie，不抢浏览器。"""
    cid = cmd.get("id")
    if not _browser_running():
        emit({"id": cid, "ok": True, "data": {"logged_in": False, "state": "unknown"}})
        return
    try:
        logged = _probe_logged_in_via_chat(platform)
        state = "logged_in" if logged else "no_login"
    except Exception as exc:
        # 超时 / 聊天车道报错：答「查不到」，不是「没登录」。
        msg = f"{type(exc).__name__}: {exc}"
        log("warn", f"探测登录态失败：{msg}")
        logged, state = False, "unknown"
    emit({"id": cid, "ok": True, "data": {"logged_in": logged, "state": state}})


def _run_chat(cmd: dict[str, Any]) -> None:
    """执行一条聊天命令并把结果作为回执 emit 回去。"""
    cid = cmd.get("id")
    name = cmd.get("cmd") or ""
    client = _chat_client()
    try:
        if name == "chat.friends":
            items = _with_chat(lambda ctx: client.friends(ctx))
            data: dict[str, Any] = {"items": items}
        elif name == "chat.messages":
            uid = str(cmd.get("uid") or "")
            if not uid:
                raise ValueError("缺少 uid")
            items = _with_chat(lambda ctx: client.messages(ctx, uid))
            data = {"uid": uid, "items": items}
        elif name == "chat.send":
            uid = str(cmd.get("uid") or "")
            text = str(cmd.get("text") or "")
            if not uid:
                raise ValueError("缺少 uid")
            sent = _with_chat(lambda ctx: client.send(ctx, uid, text))
            data = {"uid": uid, "ok": True, "text": sent}
        elif name == "chat.resumes":
            uid = str(cmd.get("uid") or "") or None
            items = _with_chat(lambda ctx: client.resumes(ctx, uid))
            data = {"items": items}
        elif name == "chat.send_resume":
            uid = str(cmd.get("uid") or "")
            rid = str(cmd.get("resume_id") or "")
            if not uid or not rid:
                raise ValueError("缺少 uid 或 resume_id")
            _with_chat(lambda ctx: client.send_resume(ctx, uid, rid))
            data = {"uid": uid, "ok": True}
        elif name == "chat.recall":
            uid = str(cmd.get("uid") or "")
            if not uid:
                raise ValueError("缺少 uid")
            _with_chat(lambda ctx: client.recall(ctx, uid))
            data = {"uid": uid, "ok": True}
        elif name == "chat.open":
            # 按岗位在 Boss 上建立会话（「还没打过招呼」的岗位走这里）。
            # 旧版是按公司/HR 名在已有会话里找，对没聊过的岗位永远找不到；
            # 现在改成真去岗位页点「立即沟通」，拿到服务端分配的会话 uid。
            job_id = str(cmd.get("job_id") or "")
            if not job_id:
                raise ValueError("缺少 job_id")
            data = _with_chat(lambda ctx: client.ensure(ctx, job_id))
        else:
            raise ValueError(f"未知聊天命令：{name}")
        emit({"id": cid, "ok": True, "data": data})
    except Exception as exc:
        # 聊天失败是可预期的（未登录、找不到会话、输入框没渲染）：
        # 把原因原样回给前端，不要把 traceback 灌进事件流
        msg = f"{type(exc).__name__}: {exc}"
        log("error", f"聊天命令失败（{name}）：{msg}")
        emit({"id": cid, "ok": False, "error": {"code": "CHAT_FAILED", "msg": msg}})


def _run_expect(cmd: dict[str, Any]) -> None:
    """读取账号的求职期望职位（expectation.list）。

    与聊天同样放进后台线程：它要等一次网络往返，主线程必须继续读 stdin，
    否则读取期间点「停止抓取」会没人接。
    """
    cid = cmd.get("id")
    try:
        with _chat_busy, _worker_lock:
            if _worker is not None and _worker.is_alive():
                raise RuntimeError(busy_reason("读取期望职位"))
            import browser as browser_mod

            ctx = ensure_context(log)
            items = browser_mod.fetch_expectations(ctx)
        emit({"id": cid, "ok": True, "data": {"items": items}})
    except Exception as exc:
        # traceback 只进 stderr（Rust 侧保留尾部），同时给界面留一句可读原因。
        # `TargetClosedError: ... has been closed` 这种话对用户没意义，但少了
        # traceback 就没人知道是 launch 失败还是 cookies 探测失败。
        import traceback as _tb

        _tb.print_exc(file=__import__("sys").stderr)
        msg = f"{type(exc).__name__}: {exc}"
        log("warn", f"读取期望职位失败：{msg}")
        emit({"id": cid, "ok": False, "error": {"code": "EXPECT_FAILED", "msg": msg}})


def _run_session_open(cmd: dict[str, Any]) -> None:
    """把浏览器拉起来并报登录态（session.open），可选导航到指定页面。

    为什么需要这条命令：聊天与「求职期望」都要一个活着的 sidecar，但此前唯一的
    启动入口是「开始抓取」或「扫码登录」。用户想看一眼消息，却被迫先跑一次抓取
    ——那是把实现顺序当成了操作顺序。更糟的是「扫码登录」会占住 _worker 直到
    登录成功或超时，期间聊天一律被「正在抓取或登录」挡回去。

    `page` 参数是补出来的：只启动不导航时，浏览器停在 about:blank，用户点了
    「Boss 消息」看到的是一片空白窗口，得等 `chat.friends` 自己 goto 才有内容。
    现在由调用方说清楚要去哪一页，打开即有东西可看。

    并发姿态与聊天一致：跟 _chat_busy 一起抢，避免同一时刻两个线程动浏览器。
    """
    cid = cmd.get("id")
    platform = cmd.get("platform") or "boss"
    # 可选目标页：`chat` = Boss 聊天页。省略时不导航（保持向后兼容）。
    target = cmd.get("page") or ""
    try:
        # 被动查询登录态（不带 page，界面上的「重试」按钮走的就是这条）：浏览器
        # 没起来就只回报「没连上」，**绝不拉起**。以前这里直接 `ensure_context`，
        # 而它见不到进程就 `start_cdp_chrome`——一个「重试」按钮把浏览器又打开了
        # （用户 m01706）。
        if not target and not _browser_running():
            emit({
                "id": cid,
                "ok": True,
                "data": {"logged_in": False, "state": "unknown"},
            })
            log("info", "浏览器未连接：只检查状态，不启动浏览器")
            return
        with _chat_busy, _worker_lock:
            if _worker is not None and _worker.is_alive():
                raise RuntimeError(busy_reason("打开浏览器"))
            import browser as browser_mod

            ctx = ensure_context(log)
            logged = browser_mod.detect_logged_in(ctx, platform)
            if target == "chat":
                # 未登录时导航到聊天页只会被重定向到登录墙，不如让用户先扫码
                if not logged:
                    raise RuntimeError("尚未登录 Boss，请先扫码登录后再打开消息页")
                import chat as chat_mod

                # ChatClient.chat_page 会复用已开的 /web/geek/chat 页，
                # 没有就新开一个并 goto——它就是唯一真源，别在这里另写一份 goto
                chat_mod.ChatClient(log).chat_page(ctx)
            elif not target:
                # 无目标页 = **被动查询登录态**，只探测 cookie，不做任何导航。
                #
                # 这条分支曾经会 `reuse_or_new_page()` + `goto(首页)`，于是前端 5 秒
                # tick 里那次 `login_status` 每轮都把浏览器导航一次；首页导航完就不再
                # 是 about:blank，下一轮找不到空白页就 `ctx.new_page()` 再开一个
                # ——用户看到的就是「浏览器窗口无限打开」。
                #
                # 首页导航属于「打开浏览器」这个主动动作（`session_open`），
                # 查询登录态不该改浏览器状态：`login_status` 的 Rust 注释也写明
                # 「不 spawn、不 session.open、不拉起 Chrome」。
                pass
        emit({"id": cid, "ok": True, "data": {
            "logged_in": bool(logged),
            "state": "logged_in" if logged else "no_login",
        }})
        log("info", f"浏览器已就绪，登录态：{'已登录' if logged else '未登录'}")
    except Exception as exc:
        msg = f"{type(exc).__name__}: {exc}"
        log("warn", f"启动浏览器失败：{msg}")
        emit({"id": cid, "ok": False, "error": {"code": "SESSION_FAILED", "msg": msg}})


def handle(cmd: dict) -> None:
    """分发一条命令。"""
    cid = cmd.get("id")
    name = cmd.get("cmd")

    if name == "ping":
        emit({"id": cid, "ok": True, "data": {"pong": True}})
        return

    if name == "scrape.start":
        start_scrape(cmd)
        emit({"id": cid, "ok": True, "data": {"accepted": True}})
        return

    if name == "scrape.stop":
        stop_scrape()
        emit({"id": cid, "ok": True, "data": {"stopped": True}})
        return

    if name == "login.start":
        start_login(cmd)
        emit({"id": cid, "ok": True, "data": {"accepted": True}})
        return

    if name == "login.status":
        # 探活**不排进车道 A**：抓取是分钟级长任务，排队的探活会 5 秒超时并被
        # 当成「未登录」，于是用户明明已登录却看到「浏览器未连接：请先登录」。
        # 改由车道 C 转投聊天车道（只读 cookie，不导航、不抢浏览器）。
        platform = str(cmd.get("platform") or "boss")
        _probe_q.submit(_run_login_probe, cmd, platform)
        return

    if isinstance(name, str) and name.startswith("chat."):
        # 聊天是**请求/应答**：执行完必须用同一个 id 回执。
        # 车道 B：投递到聊天专属线程 _chat_q，不再因抓取占用而拒绝（双车道的核心）。
        _chat_q.submit(_run_chat, cmd)
        return

    if name == "expectation.list":
        # 同样是请求/应答，且要等网络往返
        busy = _busy_now("读取期望职位")
        if busy:
            msg = f"RuntimeError: {busy}"
            log("warn", f"读取期望职位失败：{msg}")
            emit({"id": cid, "ok": False, "error": {"code": "EXPECT_FAILED", "msg": msg}})
        else:
            _browser_q.submit(_run_expect, cmd)
        return

    if name == "session.open":
        # 只把浏览器拉起来并报登录态。放后台线程是因为启动 Chrome 要几秒，
        # 主线程必须继续读 stdin。
        busy = _busy_now("打开浏览器")
        if busy:
            if cmd.get("page"):
                # 带目标页 = 主动开页，必须独占浏览器，照旧挡回。
                msg = f"RuntimeError: {busy}"
                log("warn", f"启动浏览器失败：{msg}")
                emit({"id": cid, "ok": False, "error": {"code": "SESSION_FAILED", "msg": msg}})
            else:
                # 被动查询（界面上的「重试」）：抓取占着车道 A 时只读 cookie，
                # 走聊天车道答话，别让重试按钮被「正在抓取」挡回去。
                log("info", "抓取进行中：登录态改由聊天车道探测")
                _probe_q.submit(_run_login_probe, cmd, str(cmd.get("platform") or "boss"))
        else:
            _browser_q.submit(_run_session_open, cmd)
        return

    emit({"id": cid, "ok": False, "error": {"code": "UNKNOWN_CMD", "msg": f"未知命令：{name}"}})


def main() -> None:
    # Windows 上 Python 默认用 cp936/GBK 写 stdout，而协议约定 UTF-8。
    # 不强制重配置会让**所有中文 JD 变成乱码**——这是实测到的真实缺陷
    # （`{"msg": "已启动浏览器"}` 输出成 `{"msg": "������..."}`）。
    # errors="replace" 而非 "strict"：宁可个别字符坏掉，也不能让整条消息抛异常中断抓取。
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stdin.reconfigure(encoding="utf-8", errors="replace")

    # scraper 目录要在 sys.path 里，才能 `import boss` / `import browser`
    here = str(Path(__file__).resolve().parent)
    if here not in sys.path:
        sys.path.insert(0, here)

    emit({"event": "ready", "data": {"pid": os.getpid()}})
    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            cmd = json.loads(raw)
        except json.JSONDecodeError as exc:
            log("error", f"JSON 解析失败：{exc}")
            continue
        handle(cmd)

    # stdin 关闭 = 父进程退出，收尾
    _stop.set()
    # 兜底：显式丢弃 playwright 连接（关 driver / 浏览器）。Rust 侧 KILL_ON_JOB_CLOSE
    # 作业是硬保证；这里只在无长任务占用时尝试，且必须投递到浏览器线程执行
    # ——跨线程 stop 必报 greenlet 错，等于没关。
    try:
        if _worker is None or not _worker.is_alive():
            _browser_q.submit(_teardown_playwright).result(timeout=3)
    except Exception:
        pass
    # 最后优雅关闭本模块拉起的 CDP Chrome（cookie 落盘后再结束进程）
    try:
        _browser_q.submit(_shutdown_cdp_chrome).result(timeout=15)
    except Exception:
        pass


if __name__ == "__main__":
    main()
