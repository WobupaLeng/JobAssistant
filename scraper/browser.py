"""浏览器启动：patchright 持久化上下文（真实 Chrome + 反检测参数）。

从 `backend/app/browser.py` 平移，只保留 sidecar 真正需要的部分：
启动上下文、准备 profile 目录、探测本机浏览器。

**刻意删掉的东西**（sidecar 用不到，保留只会增加体积与故障面）：
- CDP 调试端口与第二个客户端（信使车道是现行版的功能，重写版没有）
- `context_alive` 探活（sidecar 单进程内同步调用，上下文死了直接抛异常即可）

为什么要这些参数，逐条都有踩坑记录，不要凭直觉改：
- `launch_persistent_context` 而非 `launch()`+`new_context()`：只有前者会把
  扫码登录态落到 profile 目录，下次启动免登录。
- `headless=False` 强制：无头模式必被 Boss 检测（UA 带 Headless → code=37 风控）。
- `channel="chrome"`：用本机真实 Chrome 而非 patchright 自带 Chromium。
- **不设置** viewport / user_agent / locale：锁视口是自动化特征；伪造 UA 会制造
  「UA 说 Mac、platform 说 Win32」这种自相矛盾，比不伪造更容易被识别。
"""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import Any, Callable

LogFn = Callable[[str, str], None]

# 各平台登录页
PLATFORM_HOME = {
    "boss": "https://www.zhipin.com/web/user/?ka=header-login",
}

# 登录成功的标志 cookie（域名关键字, cookie 名）
# Boss 的登录态藏在 wt2 / geek_zp_token 里；bst / zp_at 是辅助令牌。
# 注意 __zp_stoken__ **不算**：那是访客态风控令牌，未登录也会有
# （实测：只访问一次首页就会下发，有效期 3 天）。
LOGIN_COOKIES = {
    "boss": ("zhipin.com", ("bst", "zp_at", "wt2", "geek_zp_token")),
}

# 登录成功后停留的页面
PLATFORM_AFTER_LOGIN = {
    "boss": "https://www.zhipin.com/web/geek/job?query=Java&city=101020100",
}

# 求职期望接口（geek 侧）。现行版 backend/app/main.py:1327 用的是同一个地址。
EXPECT_LIST_URL = "https://www.zhipin.com/wapi/zpgeek/pc/recommend/expect/list.json"

DEFAULT_TIMEOUT_MS = 60_000


def _launch_args() -> list[str]:
    """启动参数，逐条对应现行版 PlaywrightManager 的注释。"""
    return [
        "--start-maximized",
        # patchright 会加 --disable-blink-features=AutomationControlled，
        # Chrome 因此弹「不受支持的命令行标记」黄条，用 --test-type 抑制
        "--test-type",
        # 异常退出后 Chrome 弹「要恢复页面吗？」模态框会挂起所有 CDP 操作
        "--disable-session-crashed-bubble",
        "--hide-crash-restore-bubble",
        "--no-first-run",
        "--no-default-browser-check",
        # 强制直连，不走系统代理（代理出口 IP 更容易被风控标记）
        "--no-proxy-server",
        "--proxy-bypass-list=*",
    ]


_CHROME_EXES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    os.path.expandvars(r"%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe"),
]
_EDGE_EXES = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]


def _default_browser_kind() -> str | None:
    """Windows 默认 https 浏览器：'chrome' / 'edge'，其它（含 Firefox）返回 None。

    只读注册表 UserChoice；读取失败一律返回 None，调用方回退到原有顺序。
    """
    try:
        import winreg

        key = r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice"
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key) as k:
            prog_id, _ = winreg.QueryValueEx(k, "ProgId")
    except Exception:
        return None
    if prog_id.startswith("ChromeHTML"):
        return "chrome"
    if prog_id.startswith("MSEdgeHTM"):
        return "edge"
    return None


def _find_system_browser() -> str | None:
    """本机 Chrome / Edge 的 exe 路径。

    优先取系统默认浏览器（Chrome 或 Edge）对应的 exe；默认是 Firefox 或检测失败时，
    保持原有顺序（先 Chrome 后 Edge），行为与改动前一致。
    """
    preferred = _EDGE_EXES if _default_browser_kind() == "edge" else _CHROME_EXES
    others = _CHROME_EXES if preferred is _EDGE_EXES else _EDGE_EXES
    for c in preferred + others:
        if os.path.isfile(c):
            return c
    return None


def sanitize_profile_exit_state(data_dir: str | Path) -> bool:
    """清理 Chrome 的「异常退出」标记。

    不清理的话，上次强制结束进程后 Chrome 会弹「要恢复页面吗？」模态框，
    它会**挡在所有 CDP 操作前面**，导致后续 goto/click 全部超时。
    返回 True 表示确实改写了 Preferences。
    """
    pref = Path(data_dir) / "Default" / "Preferences"
    if not pref.is_file():
        return False
    try:
        root = json.loads(pref.read_text(encoding="utf-8"))
    except Exception:
        return False
    prof = root.get("profile") or {}
    if not prof:
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


def connect_cdp_context(pw: Any, port: int) -> Any:
    """连接到已由 start_cdp_chrome 启动的 CDP Chrome，返回它的默认 BrowserContext。

    只连接不启动：启动由 start_cdp_chrome 负责（进程与端口归它管）。
    返回的 context 与 launch_context 一样可用 cookies()/pages/new_page()，
    调用方（ensure_context 的探活、reuse_or_new_page）无需改动。
    """
    browser = pw.chromium.connect_over_cdp(f"http://127.0.0.1:{port}")
    if not browser.contexts:
        raise RuntimeError("CDP Chrome 没有可用的 BrowserContext")
    return browser.contexts[0]


def launch_context(pw: Any, data_dir: str | Path, log: LogFn):
    """启动 patchright 持久化上下文，返回 BrowserContext。"""
    sanitize_profile_exit_state(data_dir)
    pdir = Path(data_dir)
    pdir.mkdir(parents=True, exist_ok=True)
    log("info", f"启动浏览器（持久化目录 {pdir}）")

    # 打包成单文件 exe 后 patchright 的 node driver 不会被收进去，
    # `compute_driver_executable()` 返回的路径不存在 → 起 node 子进程时
    # `[WinError 2] 系统找不到指定的文件`。frozen 时直接走 executable_path。
    kwargs: dict[str, Any] = {}
    driver_ok = False
    if not getattr(sys, "frozen", False):
        try:
            from patchright._impl._driver import compute_driver_executable

            node_exe, cli_js = compute_driver_executable()
            driver_ok = Path(node_exe).is_file() and Path(cli_js).is_file()
        except Exception:
            driver_ok = False

    if driver_ok:
        kwargs["channel"] = "chrome"
    else:
        exe = _find_system_browser()
        if exe:
            kwargs["executable_path"] = exe
            log("warn", f"patchright driver 不可用，改用本机浏览器: {exe}")
        else:
            log("warn", "patchright driver 不可用且未找到本机 Chrome/Edge，尝试默认通道")

    args = _launch_args()
    try:
        ctx = pw.chromium.launch_persistent_context(
            str(pdir),
            headless=False,          # 无头必被检测
            args=args,
            slow_mo=50,              # 放慢操作，降低行为检测风险
            viewport=None,           # 不锁视口
            chromium_sandbox=True,   # 不加 --no-sandbox：它本身就是自动化特征
            no_viewport=True,
            **kwargs,
        )
    except Exception as exc:
        # channel="chrome" 依赖注册表探测，个别机器报 WinError 2；兜底直连 exe
        if "channel" in kwargs:
            exe = _find_system_browser()
            if not exe:
                raise
            log("warn", f"channel=chrome 启动失败（{exc}），改用 {exe}")
            kwargs.pop("channel", None)
            kwargs["executable_path"] = exe
            ctx = pw.chromium.launch_persistent_context(
                str(pdir),
                headless=False,
                args=args,
                slow_mo=50,
                viewport=None,
                chromium_sandbox=True,
                no_viewport=True,
                **kwargs,
            )
        else:
            raise
    except Exception:
        # 把 traceback 写进 stderr：上层只把 str(exc) 回给界面，而
        # `TargetClosedError: Target page, context or browser has been closed`
        # 对这一行**毫无信息量**。真凶通常在更早的 patchright 内部日志里
        # （比如 driver 起不来、profile 被占）。Rust 侧会保留 stderr 尾部，
        # 因此这是唯一能让下一个人看到根因的地方。
        import traceback as _tb

        print("[browser] launch_persistent_context 失败：", file=__import__("sys").stderr)
        _tb.print_exc()
        raise
    log("info", "浏览器已启动（真实 Chrome + 持久化上下文）")
    return ctx


BLANK_URLS = ("about:blank", "", "chrome://newtab/")


def start_cdp_chrome(profile_dir: str) -> tuple[Any, int]:
    """以 CDP 方式启动本机 Chrome，返回 (subprocess.Popen, 端口)。

    端口由系统动态分配（bind 到 0 取空闲口），仅绑定 127.0.0.1，且只在本进程
    存活期间存在：父进程（sidecar）退出时由 Popen 句柄随之释放，不常驻占口。
    这一步只负责起进程与分配端口，连接与标签页由调用方用 connect_over_cdp 完成。
    """
    import socket
    import subprocess

    exe = _find_system_browser()
    if not exe:
        raise RuntimeError("未找到本机 Chrome，无法以 CDP 方式启动")
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    args = [
        exe,
        f"--remote-debugging-port={port}",
        "--remote-debugging-address=127.0.0.1",
        f"--user-data-dir={profile_dir}",
        "--no-first-run",
        "--no-default-browser-check",
        "--no-proxy-server",
        "--proxy-bypass-list=*",
        "about:blank",
    ]
    proc = subprocess.Popen(args)
    # 必须等调试端口真的监听上再返回：Chrome 从 spawn 到开 CDP 端口要 0.5~2 秒，
    # 立刻返回会让调用方的 _port_listening() 探空 → 再起一个 Chrome → 窗口无限打开。
    ready_port = _wait_cdp_ready(proc, port, log=None)
    if ready_port is None:
        # 端口没起来：进程要么已经死了，要么卡在被别的会话接管。
        # 不回收的话它会在调用方抛错后变成孤儿，占着 profile 让下一次启动必败。
        try:
            proc.terminate()
            proc.wait(timeout=5)
        except Exception:
            pass
        raise RuntimeError(
            f"CDP Chrome 调试端口 {port} 在 15 秒内未就绪（进程退出码 {proc.returncode}）"
        )
    print(f"[browser] CDP Chrome 已启动（127.0.0.1:{port}）", file=__import__("sys").stderr)
    return proc, port


def _wait_cdp_ready(proc: Any, port: int, log: Any = None, timeout_s: float = 15.0) -> int | None:
    """轮询等 `port` 上的 CDP 端口就绪。就绪返回端口，进程先死 / 超时返回 None。

    只做 TCP connect 探测（不消耗 HTTP 并发槽，也不触发 devtools 限流）。
    进程提前退出时立刻放弃：继续等只会让用户多看着一个空窗口。
    """
    import socket
    import time

    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            return None
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=1):
                return port
        except OSError:
            time.sleep(0.2)
    return None


def reuse_or_new_page(ctx: Any) -> Any:
    """优先复用启动时留下的空白页，没有才新建。

    `launch_persistent_context` 会留下一个 about:blank 标签；直接 `ctx.new_page()`
    会让用户先看到空白、再看到目标页。抓取启动、登录、聊天都走这一个入口。
    """
    for pg in ctx.pages:
        try:
            if (pg.url or "") in BLANK_URLS:
                return pg
        except Exception:
            continue
    return ctx.new_page()


def find_page_for(ctx: Any, platform: str, log: LogFn | None = None) -> Any:
    """在该平台域名下找已存在的标签页，没有就新建并打开首页。

    `launch_persistent_context` 之后浏览器里可能**一个页面都没有**
    （用户关过窗口），此时 ctx.pages 为空，只能 new_page()。
    """
    warn = log or (lambda *_: None)
    domain = LOGIN_COOKIES.get(platform, ("", ()))[0]
    for pg in ctx.pages:
        try:
            if domain and domain in pg.url:
                return pg
        except Exception:
            continue
    # 优先复用空白页，否则才新建：反复登录时不堆一堆空标签页
    page = reuse_or_new_page(ctx)
    page.set_default_timeout(DEFAULT_TIMEOUT_MS)
    try:
        page.goto(PLATFORM_HOME[platform],
                  wait_until="domcontentloaded", timeout=DEFAULT_TIMEOUT_MS)
    except Exception as exc:
        warn("warn", f"[{platform}] 打开登录页超时/失败：{exc}")
    return page


def detect_logged_in(ctx: Any, platform: str) -> bool:
    """按 cookie 判断是否已登录该平台。

    只读 cookie，**不导航、不新开页面**：登录轮询每 1.5 秒一次，
    每次刷新页面就是用户看到的「登录页一直闪烁」。
    """
    spec = LOGIN_COOKIES.get(platform)
    if not spec:
        return False
    domain, names = spec
    try:
        cookies = ctx.cookies()
    except Exception:
        return False
    for c in cookies:
        d = (c.get("domain") or "").lstrip(".").lower()
        if domain in d and c.get("name") in names:
            return True
    return False


def fetch_expectations(ctx: Any) -> list[dict[str, Any]]:
    """读取 Boss 账号里的「求职期望」职位。

    **不新开页面、不发导航**：走 `ctx.request`，它复用上下文里已有的 cookie，
    等价于「带着登录态发一次 GET」。现行版是从 Chrome profile 里解 cookie 再
    urllib 发请求（`backend/app/main.py:1241`），新版放弃 CDP 后没有必要再碰
    cookie 文件——浏览器本来就开着且已登录。

    返回字段与现行版保持一致（name / code / city / city_code / salary / source），
    前端不用区分数据来源。
    """
    if not detect_logged_in(ctx, "boss"):
        raise RuntimeError("尚未登录 Boss，请先扫码登录")

    resp = ctx.request.get(
        EXPECT_LIST_URL,
        headers={
            "Referer": "https://www.zhipin.com/web/geek/job",
            "Accept": "application/json, text/plain, */*",
        },
        timeout=15_000,
    )
    if not resp.ok:
        raise RuntimeError(f"读取期望职位失败：HTTP {resp.status}")
    body = resp.json()
    if not isinstance(body, dict):
        raise RuntimeError("读取期望职位失败：返回的不是 JSON 对象")
    # Boss 的风控拒绝也是 200 + code!=0，只看 HTTP 状态会把「登录失效」当成「没有期望」
    code = body.get("code")
    if code not in (None, 0):
        raise RuntimeError(f"读取期望职位失败：{body.get('message') or f'code={code}'}")

    out: list[dict[str, Any]] = []
    for e in (body.get("zpData") or {}).get("expectList") or []:
        if not isinstance(e, dict):
            continue
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
