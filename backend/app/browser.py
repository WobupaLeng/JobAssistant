"""浏览器管理：patchright + 持久化上下文（移植自 Java 项目已验证方案）。

# 为什么必须是 patchright，不能用原版 Playwright

原版 Playwright 驱动 Chrome 打开 Boss 直聘，页面会在 1-3 秒内被重置成
``about:blank``，或直接跳滑块验证页 ``/web/passport/zp/verify.html``。
原因不只是 ``navigator.webdriver``：Playwright 在 CDP 层留下大量可探测痕迹，
加 ``--disable-blink-features=AutomationControlled`` 也骗不过去。

**patchright** 是 Playwright 的反检测补丁版（Java 项目用的就是它，
patchright-core 1.62.1，配合 playwright-java 1.62.0 驱动）。换成它之后
Boss 的风控不再触发，Java 版实测可稳定抓取并投递。

# 三个必须遵守的要点（全部来自 Java 项目的踩坑记录）

1. 必须用 ``launch_persistent_context``，不能用 ``launch()`` + ``new_context()``。
   patchright 官方明确要求；而且登录态直接落盘到 USER_DATA_DIR，
   **登录和抓取必须用同一个浏览器实例/同一个 profile**，跨浏览器搬 cookie
   会被判定为环境异常。
2. 必须 ``channel="chrome"`` + ``headless=False``。无头必被检测。
3. 必须 ``--no-proxy-server`` 强制直连。代理出口 IP 更容易被风控标记。

另外还要 ``sanitize_profile_exit_state()``：上次异常退出后 Chrome 会弹
"要恢复页面吗？"模态框，那个框会挂起所有 CDP 页面操作（navigate 永远不返回、
连超时都不触发）。Java 版就是被这个坑过。
"""
from __future__ import annotations

import json
import os
import sys
import threading
import time
from pathlib import Path
from typing import Any, Callable

# 各平台登录页
PLATFORM_HOME = {
    "boss": "https://www.zhipin.com/web/user/?ka=header-login",
    "zhilian": "https://passport.zhaopin.com/login",
    "51job": "https://login.51job.com/login.php",
}

# 登录成功的标志 cookie（域名关键字, cookie 名）
LOGIN_COOKIES = {
    "boss": ("zhipin.com", ("bst", "zp_at", "wt2", "geek_zp_token")),
    "zhilian": ("zhaopin.com", ("at", "rt", "x-zp-client-id", "fe_cookie")),
    "51job": ("51job.com", ("guid", "jobid", "51job_isLogin", "interversion")),
}

# 登录成功后停留的页面
PLATFORM_AFTER_LOGIN = {
    "boss": "https://www.zhipin.com/web/geek/job?query=Java&city=101020100",
    "zhilian": "https://sou.zhaopin.com/?jl=538&kw=Java",
    "51job": "https://we.51job.com/pc/search?jobArea=020000&keyword=Java",
}

# 用户数据目录名（项目 data/ 下）
PROFILE_DIR_NAME = "browser-profile"

DEFAULT_TIMEOUT_MS = 60_000


def profile_dir(data_dir: str | Path) -> Path:
    """持久化 profile 目录（登录态 + 所有 cookie 都在这里）。只返回路径，不创建目录。"""
    return Path(data_dir) / PROFILE_DIR_NAME


# Chrome 放 Cookies 库的位置（新版 <profile>/Default/Network/Cookies）
_PROFILE_COOKIE_DBS = (
    Path("Default") / "Network" / "Cookies",
    Path("Default") / "Cookies",
    Path("Network") / "Cookies",
    Path("Cookies"),
)


def latest_profile_cookie_db(profile: str | Path) -> Path | None:
    """返回该 profile 里最新修改过的 Cookies 库路径（没有返回 None）。

    只有存在 Cookies 库的目录才算「真正登录过」，用它来选择数据目录，
    比单看 Local State 靠谱（Local State 启动浏览器就会生成）。
    """
    root = Path(profile)
    best: Path | None = None
    for rel in _PROFILE_COOKIE_DBS:
        p = root / rel
        try:
            if p.is_file() and (best is None or p.stat().st_mtime > best.stat().st_mtime):
                best = p
        except OSError:
            continue
    return best


def ensure_profile_dir(data_dir: str | Path) -> Path:
    """创建 profile 目录（启动浏览器前才需要）。"""
    p = profile_dir(data_dir)
    p.mkdir(parents=True, exist_ok=True)
    return p


# ---------------- profile 卫生 ----------------

def sanitize_profile_exit_state(data_dir: str | Path) -> bool:
    """把 profile 里"上次崩溃退出"的标记改回正常。

    Java 版踩过的坑：应用被强杀后，Chrome 下次启动会弹"要恢复页面吗？"
    模态框，该框会挂起所有 CDP 页面操作，navigate 永不返回且超时不触发。
    """
    pref = profile_dir(data_dir) / "Default" / "Preferences"
    if not pref.is_file():
        return False
    try:
        root = json.loads(pref.read_text(encoding="utf-8"))
    except Exception:
        return False
    prof = root.get("profile")
    if not isinstance(prof, dict):
        return False
    if prof.get("exit_type") == "Normal" and prof.get("exited_cleanly") is True:
        return False
    prof["exit_type"] = "Normal"
    prof["exited_cleanly"] = True
    try:
        pref.write_text(json.dumps(root, ensure_ascii=False), encoding="utf-8")
        return True
    except Exception:
        return False


def _launch_args() -> list[str]:
    """启动参数，逐条对应 Java 版 PlaywrightManager 的注释。"""
    return [
        "--start-maximized",
        # patchright 会加 --disable-blink-features=AutomationControlled，
        # Chrome 因此弹"不受支持的命令行标记"黄条，用 --test-type 抑制
        "--test-type",
        # 异常退出后 Chrome 弹"要恢复页面吗？"模态框会挂起所有 CDP 操作
        "--disable-session-crashed-bubble",
        "--hide-crash-restore-bubble",
        "--no-first-run",
        "--no-default-browser-check",
        # 强制直连，不走系统代理（代理出口 IP 更容易被风控标记）
        "--no-proxy-server",
        "--proxy-bypass-list=*",
    ]


def _driver_dir() -> Path | None:
    """patchright 的 node driver 目录（含 node.exe 与 package/cli.js）。

    两种来源，按优先级：
    1. PyInstaller 打包时用 datas 收集进来的 `<解包目录>/patchright-driver`
    2. 开发态 site-packages 里的 `patchright/driver`
    """
    cands: list[Path] = []
    mei = getattr(sys, "_MEIPASS", None)
    if mei:
        cands.append(Path(mei) / "patchright-driver")
    try:
        import patchright

        cands.append(Path(patchright.__file__).resolve().parent / "driver")
    except Exception:
        pass
    for d in cands:
        if (d / "node.exe").is_file() and (d / "package" / "cli.js").is_file():
            return d
    return None


def prepare_driver() -> None:
    """让 patchright 用我们自己的 driver 路径。

    打包后 compute_driver_executable() 返回的路径不在解包目录里，
    `sync_playwright().start()` 会在起 node 子进程时抛：
        FileNotFoundError: [WinError 2] 系统找不到指定的文件。
    这里直接把 `compute_driver_executable` 换成返回收集进来的驱动路径，
    patchright 其余逻辑无需改动。
    """
    d = _driver_dir()
    if not d:
        return
    node_exe = d / "node.exe"
    cli_js = d / "package" / "cli.js"
    try:
        from patchright._impl import _driver as _drv

        _drv.compute_driver_executable = lambda: (str(node_exe), str(cli_js))  # type: ignore[assignment]
        # _transport 里是 `from ._driver import compute_driver_executable`，
        # 需要把已经绑定到 _transport 的那个名字一起替换。
        from patchright._impl import _transport as _tr

        _tr.compute_driver_executable = lambda: (str(node_exe), str(cli_js))  # type: ignore[assignment]
    except Exception:
        pass


def _find_system_browser() -> str | None:
    """本机 Chrome / Edge 的 exe 路径（driver 不可用时的兜底）。"""
    candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        os.path.expandvars(r"%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe"),
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ]
    for c in candidates:
        if os.path.isfile(c):
            return c
    return None


def launch_context(
    pw: Any,
    data_dir: str | Path,
    log: Callable[[str, str], None],
):
    """启动 patchright 持久化上下文（全平台共享，登录态自动落盘）。

    刻意不设置 viewport / user_agent / locale：
    - viewport=None 用窗口实际大小（锁视口是自动化特征）
    - UA 伪造只会制造 "UA 说 Mac、platform 说 Win32" 这种致命矛盾
    """
    sanitize_profile_exit_state(data_dir)
    pdir = ensure_profile_dir(data_dir)
    log("info", f"启动 patchright（持久化目录 {pdir}）")

    # 打包成单文件 exe 后，patchright 的 node driver（node.exe + package/cli.js）
    # 没有被 PyInstaller 自动收集，此时内部会抛：
    #   [WinError 2] 系统找不到指定的文件。
    # 判定：driver 文件不存在 → 显式传 executable_path 用本机真实 Chrome
    # （patchright 在 executable_path 模式下不走 node driver，直接 CDP 驱动），
    # 找不到本机 Chrome 再退回 Edge。
    # 打包（frozen）后一律直接用本机 Chrome 的 exe：
    # PyInstaller 不会把 patchright 的 node driver（node.exe + package/cli.js）
    # 收进 onefile，此时无论 compute_driver_executable() 返回什么，
    # launch_persistent_context 内部都会抛
    #   [WinError 2] 系统找不到指定的文件。
    # 所以 frozen 时直接走 executable_path 分支，不再尝试探测 driver。
    kwargs: dict[str, Any] = {}
    driver_ok = False
    if not getattr(sys, "frozen", False):
        try:
            from patchright._impl._driver import compute_driver_executable

            node_exe, cli_js = compute_driver_executable()
            driver_ok = Path(node_exe).is_file() and Path(cli_js).is_file()
        except Exception:
            driver_ok = False

    if not driver_ok:
        exe = _find_system_browser()
        if exe:
            kwargs["executable_path"] = exe
            log("warn", f"patchright driver 不可用，改用本机浏览器: {exe}")
        else:
            log("warn", "patchright driver 不可用且未找到本机 Chrome/Edge")
    else:
        # 用本机真实 Chrome，而不是 Chromium
        kwargs["channel"] = "chrome"

    try:
        # 默认屏蔽自动化特征（patchright 的核心：去掉 navigator.webdriver、
        # Chrome 内部 UA 里的 "HeadlessChrome" 标记等）。
        # Boss 会校验 UA，UA 带 Headless 会直接返回 code=37 风控。
        launch_kwargs: dict[str, Any] = {}
        if not os.environ.get("AJA_NO_STEALTH"):
            launch_kwargs["channel"] = kwargs.get("channel", "chrome")
            launch_kwargs.pop("executable_path", None)
        else:
            launch_kwargs = dict(kwargs)
        ctx = pw.chromium.launch_persistent_context(
            str(pdir),
            headless=False,          # 无头必被检测
            args=_launch_args(),
            slow_mo=50,              # 放慢操作，降低行为检测风险
            viewport=None,           # 不锁视口
            chromium_sandbox=True,   # 不加 --no-sandbox：它本身就是自动化特征
            no_viewport=True,
            **kwargs,
        )
    except Exception as exc:
        # channel="chrome" 依赖注册表探测，个别机器会报 WinError 2；兜底直连 exe
        if "channel" in kwargs:
            exe = _find_system_browser()
            if exe:
                log("warn", f"channel=chrome 启动失败（{exc}），改用 {exe}")
                kwargs.pop("channel", None)
                kwargs["executable_path"] = exe
                ctx = pw.chromium.launch_persistent_context(
                    str(pdir),
                    headless=False,
                    args=_launch_args(),
                    slow_mo=50,
                    viewport=None,
                    chromium_sandbox=True,
                    no_viewport=True,
                    **kwargs,
                )
            else:
                raise
        else:
            raise
    log("info", "patchright 已启动（真实 Chrome + 持久化上下文）")
    return ctx


def context_alive(ctx: Any) -> bool:
    """探活：连接断了 / 浏览器被关掉时返回 False。"""
    if ctx is None:
        return False
    try:
        _ = ctx.pages
    except Exception:
        return False
    # 注意：浏览器被外部关掉后 ctx.pages 仍可能返回旧列表，
    # 所以这里必须真正问一次浏览器（否则会在死上下文上抓取报 TargetClosedError）。
    # 但**绝不能** new_page()+close() 探活：登录流程每秒调一次，
    # 在真实 Chrome 里就是标签页每秒闪现一次（用户看到的「登录页一直闪烁」），
    # 还会把当前活动标签页抢走、打出满屏的「采集线程开始执行：_wrap」。
    # 用带超时的 cookies() 代替：不新建页面，上下文已死时会抛 WrongTarget/Timeout。
    try:
        ctx.cookies()
        return True
    except Exception:
        return False


def find_page_for(ctx: Any, platform: str, log: Callable[[str, str], None] | None = None) -> Any:
    """在该平台域名下找已存在的标签页，没有就新建并打开首页。

    注意：launch_persistent_context 之后浏览器里可能**一个页面都没有**
    （用户把窗口关掉过），此时 ctx.pages 为空，只能 new_page()。
    """
    warn = log or (lambda *_: None)
    domain = LOGIN_COOKIES.get(platform, ("", ()))[0]
    for pg in ctx.pages:
        try:
            if domain and domain in pg.url:
                return pg
        except Exception:
            continue
    page = None
    # 优先复用已有的空白页（about:blank），否则才新建：反复登录时不堆一堆空标签页
    for pg in ctx.pages:
        try:
            if (pg.url or "") in ("about:blank", "", "chrome://newtab/"):
                page = pg
                break
        except Exception:
            continue
    if page is None:
        page = ctx.new_page()
    page.set_default_timeout(DEFAULT_TIMEOUT_MS)
    try:
        page.goto(PLATFORM_HOME[platform], wait_until="domcontentloaded", timeout=DEFAULT_TIMEOUT_MS)
    except Exception as exc:
        warn("warn", f"[{platform}] 打开登录页超时/失败：{exc}")
    return page


def detect_logged_in_from_page(page: Any, platform: str) -> bool:
    """按 cookie 判断是否已登录该平台。"""
    spec = LOGIN_COOKIES.get(platform)
    if not spec:
        return False
    domain, names = spec
    try:
        cookies = page.context.cookies()
    except Exception:
        return False
    for c in cookies:
        d = (c.get("domain") or "").lstrip(".").lower()
        if domain in d and c.get("name") in names:
            return True
    return False


class LoginSession:
    """在**同一个** patchright 持久化浏览器里打开登录页，等用户扫码登录。

    登录态直接落在 profile 目录里，抓取时复用同一个 profile ——
    这正是 Java 项目能稳定工作的关键（不跨浏览器搬 cookie）。
    """

    def __init__(
        self,
        platform: str,
        data_dir: str | Path,
        log: Callable[[str, str], None],
        get_context: Callable[[], Any] | None = None,
        start_browser: Callable[[], Any] | None = None,
        step_runner: Callable[..., Any] | None = None,
    ) -> None:
        self.platform = platform
        self.data_dir = data_dir
        self.log = log
        # 由 Runtime 注入：复用已在运行的持久化上下文，避免开两个 Chrome 抢占 profile
        self._get_context = get_context
        # 同样由 Runtime 注入：把浏览器投递到采集线程里启动并返回上下文。
        # patchright 的 sync API 绑定了创建它的 event loop，HTTP 线程里启动的
        # 浏览器不能给采集/登录线程用（会报 no running event loop）。
        self._start_browser = start_browser
        # 把每个「碰浏览器」的步骤投递到采集线程执行（patchright 只能在创建它的线程里用）
        self._step_runner = step_runner
        self.thread: threading.Thread | None = None
        self._stop = threading.Event()
        self.state = "idle"  # idle|opening|waiting|logged_in|closed|error
        self.message = ""

    def start(self) -> None:
        if self.thread and self.thread.is_alive():
            raise RuntimeError("登录窗口已在运行")
        self._stop.clear()
        self.state = "opening"
        self.message = ""
        self.thread = threading.Thread(
            target=self._run, name=f"login-{self.platform}", daemon=True
        )
        self.thread.start()

    def stop(self) -> None:
        self._stop.set()

    def snapshot(self) -> dict[str, Any]:
        alive = bool(self.thread and self.thread.is_alive())
        return {
            "platform": self.platform,
            "state": self.state,
            "running": alive,
            "message": self.message,
        }

    def _open_browser(self) -> Any:
        """优先走 start_browser（采集线程里启动），其次退回 get_context。"""
        if self._start_browser is not None:
            return self._start_browser()
        return self._get_context() if self._get_context else None

    def _step(self, fn: Callable[..., Any], *args: Any) -> Any:
        """兜底：没有注入 step_runner 时直接在本线程执行（仅在采集线程里调用才安全）。"""
        return fn(*args)

    def _steps(self):
        """登录流程的 generator：每 yield 一个「要在采集线程里执行的 (fn, *args)」。"""
        if self._start_browser is None and self._get_context is None:
            self.state = "error"
            self.message = "内部错误：未注入浏览器上下文"
            return

        ctx = self._open_browser()
        if ctx is None:
            self.state = "error"
            self.message = "浏览器启动失败（详见运行日志）"
            return
        page = yield (find_page_for, ctx, self.platform, self.log)
        self.state = "waiting"
        self.message = "请在弹出的窗口里完成登录（可手动处理验证码）"
        self.log("info", f"[{self.platform}] 登录窗口已打开，等待扫码")

        deadline = time.time() + 900  # 最多等 15 分钟
        while time.time() < deadline and not self._stop.is_set():
            alive = yield (context_alive, ctx)
            if not alive:
                self.log("warn", f"[{self.platform}] 登录窗口被关闭，重新打开…")
                self.state = "opening"
                self.message = "登录窗口已关闭，正在重新打开…"
                time.sleep(1.0)
                if self._stop.is_set():
                    break
                ctx = yield (self._open_browser,)
                if ctx is None:
                    self.state = "error"
                    self.message = "浏览器启动失败（详见运行日志）"
                    return
                page = yield (find_page_for, ctx, self.platform, self.log)
                self.state = "waiting"
                self.message = "请在弹出的窗口里完成登录（可手动处理验证码）"
                continue

            if self._stop.is_set():
                break

            logged = yield (self._check_logged_in, page)
            if logged:
                self.state = "logged_in"
                self.message = "登录成功"
                self.log("info", f"[{self.platform}] 登录成功")
                after = PLATFORM_AFTER_LOGIN.get(self.platform)
                if after:
                    yield (self._goto_quiet, page, after)
                return

            time.sleep(1.5)

        if self._stop.is_set():
            self.state = "closed"
            self.message = "已取消登录"
        else:
            self.state = "closed"
            self.message = "登录超时（15 分钟）"
            self.log("warn", f"[{self.platform}] {self.message}")

    # --- 以下动作都在采集线程里执行 ---
    def _check_logged_in(self, page: Any) -> bool:
        """在采集线程里执行：只读 cookie，不新建/刷新页面。"""
        try:
            return bool(detect_logged_in_from_page(page, self.platform))
        except Exception:
            return False

    def _goto_quiet(self, page: Any, url: str) -> None:
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=30_000)
        except Exception:
            pass

    def _run(self) -> None:
        """调度线程：逐步驱动 _steps()，每一步都投递到采集线程执行（绝不跨线程碰浏览器）。"""
        try:
            gen = self._steps()
            step = self._step_runner or self._step
            try:
                action = next(gen)
                while True:
                    result = step(*action)
                    action = gen.send(result)
            except StopIteration:
                return
        except Exception as exc:
            self.state = "error"
            self.message = str(exc)
            import traceback as _tb

            detail = _tb.format_exc()
            self.log("error", f"[{self.platform}] 登录窗口异常：{exc}\n{detail}")


