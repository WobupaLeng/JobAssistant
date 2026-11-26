"""浏览器：起一个本机真实 Chrome，用 CDP 连上去。

**为什么不走 patchright 的 `launch_persistent_context`**：那条路会把 patchright 的
node driver 一起拖进分发包（单文件 exe 收不进去），而且 `headless` / 锁视口这些
参数本身就是可被判定的自动化特征。现在的做法只有两步——
`start_cdp_chrome()` 用 `--remote-debugging-port` 起真实 Chrome（profile 落在
`data/browser-profile`，扫码登录态下次免登录），`connect_cdp_context()` 用
`connect_over_cdp` 连它的默认上下文。

进程与端口归 `start_cdp_chrome` 管，连接后拿到的上下文行为与普通上下文一致，
`ensure_context` 探活、`reuse_or_new_page` 都不需要区分来源。
"""
from __future__ import annotations

import os
import sys
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

# 求职期望接口（geek 侧）
EXPECT_LIST_URL = "https://www.zhipin.com/wapi/zpgeek/pc/recommend/expect/list.json"

DEFAULT_TIMEOUT_MS = 60_000


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


def connect_cdp_context(pw: Any, port: int) -> Any:
    """连接到已由 start_cdp_chrome 启动的 CDP Chrome，返回它的默认 BrowserContext。

    只连接不启动：启动由 start_cdp_chrome 负责（进程与端口归它管）。
    返回的 context 与普通上下文一样支持 cookies()/pages/new_page()。
    """
    browser = pw.chromium.connect_over_cdp(f"http://127.0.0.1:{port}")
    if not browser.contexts:
        raise RuntimeError("CDP Chrome 没有可用的 BrowserContext")
    return browser.contexts[0]


BLANK_URLS = ("about:blank", "", "chrome://newtab/")


def start_cdp_chrome(profile_dir: str) -> tuple[Any, int]:
    """以 CDP 方式启动本机 Chrome，返回 (subprocess.Popen, 端口)。

    端口由系统动态分配（bind 到 0 取空闲口），仅绑定 127.0.0.1，且只在本进程
    存活期间存在：父进程（sidecar）退出时由 Popen 句柄随之释放，不常驻占口。
    这一步只负责起进程与分配端口，连接与标签页由调用方用 connect_over_cdp 完成。

    兜底：profile 已被别的 Chrome 占用时（上次异常退出留下的孤儿），新进程只会
    把启动请求移交过去、自己以退出码 0 退出，CDP 端口永远不监听。此时直接接管
    旧进程已有的调试端口，而不是让「点登录」变成弹出一个空白页。
    """
    import socket
    import subprocess

    adopted = _find_profile_cdp(profile_dir)
    if adopted is not None:
        pid, old_port = adopted
        print(f"[browser] 接管已在运行的 Chrome（pid={pid}, 127.0.0.1:{old_port}）", file=sys.stderr)
        return ReusedChrome(pid, old_port), old_port

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
        # 预检与真正 spawn 之间有竞态（另一个 Chrome 恰好在这几秒里抢到 profile）：
        # 再查一次，能接管就接管，做不到才报错。
        adopted = _find_profile_cdp(profile_dir)
        if adopted is not None:
            pid, old_port = adopted
            # 新起的这个进程只是「移交请求的壳」：它在把请求转给旧实例后自行退出，
            # 但若还活着也不能留着，否则下次它又占着 profile。
            try:
                proc.terminate()
                proc.wait(timeout=5)
            except Exception:
                pass
            print(f"[browser] 接管已在运行的 Chrome（pid={pid}, 127.0.0.1:{old_port}）", file=sys.stderr)
            return ReusedChrome(pid, old_port), old_port
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


def _tcp_alive(port: int) -> bool:
    """探测 127.0.0.1:port 是否还在监听（只做 connect，不发 HTTP）。"""
    import socket

    try:
        with socket.create_connection(("127.0.0.1", port), timeout=0.5):
            return True
    except OSError:
        return False


def _find_profile_cdp(profile_dir: str) -> tuple[int, int] | None:
    """找出正在占用该 profile 且开着 CDP 端口的 Chrome，返回 (pid, port)。

    Chrome 对 user-data-dir 有单例锁：同 profile 已有进程时，新的 chrome.exe 会把
    启动请求移交过去后自己退出，调试端口永不监听。这种孤儿进程多数是上次崩溃
    留下的——直接接管它，比报错或再弹一个空白窗口都好。

    只在 Windows 上查（应用只发 Windows 包）；查询失败一律当作「没有」。
    """
    # 空路径会让 `-like '*--user-data-dir=*'` 匹配到任意 Chrome（误接管别人的浏览器）
    if not profile_dir.strip() or sys.platform != "win32":
        return None
    import base64
    import subprocess

    ps = (
        "$p = Get-CimInstance Win32_Process -Filter \"Name='chrome.exe'\" | "
        f"Where-Object {{ $_.CommandLine -like '*--user-data-dir={profile_dir}*' "
        "-and $_.CommandLine -match '--remote-debugging-port=(\\d+)' } | "
        "Select-Object -First 1\n"
        "if ($p) { \"$($p.ProcessId) $([regex]::Match($p.CommandLine,'--remote-debugging-port=(\\d+)').Groups[1].Value)\" }"
    )
    try:
        encoded = base64.b64encode(ps.encode("utf-16-le")).decode("ascii")
        out = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        ).stdout.strip()
    except Exception:
        return None
    parts = out.split()
    if len(parts) != 2 or not all(p.isdigit() for p in parts):
        return None
    pid, port = int(parts[0]), int(parts[1])
    if port <= 0 or not _tcp_alive(port):
        return None
    return pid, port


class ReusedChrome:
    """接管一个已在运行的 Chrome：我们不是它的父进程，只能经 CDP 关。

    `ensure_context` 只用 `poll()` 判活、`_shutdown_cdp_chrome` 用 `wait()` /
    `terminate()`，这里按同样的三个方法实现，好让调用方不必分支。
    """

    def __init__(self, pid: int, port: int) -> None:
        self.pid = pid
        self.port = port

    def poll(self) -> int | None:
        # 只认端口：进程号可能已被复用，端口关了才算这个浏览器没了。
        return None if _tcp_alive(self.port) else 0

    def wait(self, timeout: float | None = None) -> int:
        import time

        deadline = time.monotonic() + (8.0 if timeout is None else timeout)
        while time.monotonic() < deadline:
            if not _tcp_alive(self.port):
                return 0
            time.sleep(0.2)
        raise TimeoutError("接管的 Chrome 未在超时内退出")

    def terminate(self) -> None:
        import subprocess

        try:
            subprocess.run(
                ["taskkill", "/PID", str(self.pid), "/T", "/F"],
                capture_output=True,
                timeout=10,
                check=False,
            )
        except Exception:
            pass


def reuse_or_new_page(ctx: Any) -> Any:
    """优先复用启动时留下的空白页，没有才新建。

    Chrome 从 `about:blank` 起来，直接 `ctx.new_page()` 会让用户先看到空白、
    再看到目标页。抓取启动、登录、聊天都走这一个入口。
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

    浏览器里可能**一个页面都没有**（用户关过窗口），
    此时 ctx.pages 为空，只能 new_page()。
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
    等价于「带着登录态发一次 GET」——不必去解 Chrome 的 cookie 文件，
    浏览器本来就开着且已登录。

    每项字段为 name / code / city / city_code / salary / source。
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
