"""登录入口的三条语义（用户 m0182x：点登录不跳转 / F5 未登录时浏览器自己弹回来）。

现场两个症状同一个根因：
1. 上一轮登录线程还活着（扫码最长等 15 分钟）时，新的一次 `login.start` 被静默
   忽略——用户点了「扫码登录」，页面上什么也没发生。
2. 那个旧线程发现浏览器被关会**自动重开**（最多 3 次）。F5 不再关 sidecar 之后，
   刷新一次就会看到 Chrome 自己弹回来。

本文件钉死：
1. 复用到的标签页停在岗位列表时，登录必须把它拨回登录页（不能「点了没反应」）。
2. 登录窗口被关掉 → 结束本次登录，**不自动重开**。
3. 再次点登录会抢占仍在等待的登录线程；抓取占着时仍然拒绝。
"""

import sys
import threading
from contextlib import contextmanager
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import browser  # noqa: E402
import sidecar  # noqa: E402

_MISSING = object()


@contextmanager
def patched(**attrs):
    old = {k: getattr(sidecar, k, _MISSING) for k in attrs}
    for k, v in attrs.items():
        setattr(sidecar, k, v)
    try:
        yield
    finally:
        for k, v in old.items():
            if v is _MISSING:
                delattr(sidecar, k)
            else:
                setattr(sidecar, k, v)


@contextmanager
def patched_browser(**attrs):
    old = {k: getattr(browser, k, _MISSING) for k in attrs}
    for k, v in attrs.items():
        setattr(browser, k, v)
    try:
        yield
    finally:
        for k, v in old.items():
            if v is _MISSING:
                delattr(browser, k)
            else:
                setattr(browser, k, v)


class FakePage:
    def __init__(self, url: str) -> None:
        self.url = url
        self.gotos: list[str] = []

    def bring_to_front(self) -> None:
        pass

    def goto(self, url, wait_until=None, timeout=None):
        self.gotos.append(url)
        self.url = url


class FakeCtx:
    def __init__(self, cookies=None, cookies_raise: bool = False) -> None:
        self._cookies = cookies or []
        self._raise = cookies_raise

    def cookies(self):
        if self._raise:
            raise RuntimeError("Browser has been closed")
        return self._cookies


def test_login_navigates_to_login_page_when_reusing_job_list_tab():
    """复用到的标签页停在岗位列表：登录必须导航到登录页。"""
    page = FakePage("https://www.zhipin.com/web/geek/job?query=Java")
    ev = threading.Event()
    calls = {"n": 0}

    def detect(ctx, platform):
        calls["n"] += 1
        if calls["n"] >= 2:  # 第一次决定要不要导航，第二次让等待循环退出
            ev.set()
        return False

    with (
        patched(ensure_context=lambda _log: FakeCtx(), emit=lambda _f: None,
                log=lambda _l, _m: None, _stop=ev),
        patched_browser(find_page_for=lambda _ctx, _p, _log: page, detect_logged_in=detect),
    ):
        sidecar._run_login({"platform": "boss", "timeout_sec": 5})

    assert page.gotos == [browser.PLATFORM_HOME["boss"]], page.gotos


def test_closed_login_window_ends_flow_without_reopen():
    """关掉登录窗口就结束本次登录：不重建 context、不重开 Chrome。"""
    page = FakePage("about:blank")
    ctx = FakeCtx(cookies_raise=True)
    ensures: list[int] = []
    out: list[dict] = []

    def ensure(_log):
        ensures.append(1)
        return ctx

    with (
        patched(ensure_context=ensure, emit=out.append, log=lambda _l, _m: None),
        patched_browser(
            find_page_for=lambda _ctx, _p, _log: page,
            detect_logged_in=lambda _ctx, _p: False,
        ),
    ):
        sidecar._run_login({"platform": "boss", "timeout_sec": 5})

    assert len(ensures) == 1, f"窗口被关后不该再拉浏览器，ensure_context 调了 {len(ensures)} 次"
    assert out[-1]["event"] == "login.state"
    assert out[-1]["data"]["state"] == "closed"
    assert "登录窗口已关闭" in out[-1]["data"]["msg"]


class _FakeFut:
    def done(self) -> bool:
        return False


class _FakeQ:
    def __init__(self) -> None:
        self.submitted: list[tuple[object, tuple]] = []

    def submit(self, fn, *args):
        self.submitted.append((fn, args))
        return _FakeFut()


class _OldWorker:
    """上一轮登录：停止信号一到就算退出。"""

    def __init__(self, ev: threading.Event) -> None:
        self._ev = ev

    def is_alive(self) -> bool:
        return not self._ev.is_set()


def test_second_login_click_takes_over_waiting_login():
    """上一轮登录还在等扫码时再点登录：先结束旧的，再开新的一轮。"""
    ev = threading.Event()
    q = _FakeQ()
    logs: list[str] = []

    with patched(
        _worker=_OldWorker(ev), _occupant="login", _stop=ev, _browser_q=q,
        log=lambda _l, m: logs.append(m),
    ):
        sidecar.start_login({"platform": "boss"})
        assert ev.is_set() is False, "开了新一轮必须清掉停止信号"
        assert sidecar._occupant == "login"

    assert len(q.submitted) == 1, q.submitted
    assert q.submitted[0][0] is sidecar._run_login
    assert any("上一轮登录" in m for m in logs), logs


def test_login_click_while_scraping_still_rejects():
    """抓取占着浏览器时点登录仍然拒绝——浏览器只能有一个主人。"""
    ev = threading.Event()
    q = _FakeQ()

    with patched(
        _worker=_OldWorker(ev), _occupant="scrape", _stop=ev, _browser_q=q,
        log=lambda _l, _m: None,
    ):
        sidecar.start_login({"platform": "boss"})

    assert q.submitted == [], "抓取中不许再提交登录任务"


if __name__ == "__main__":
    failed = 0
    for name in sorted(n for n in dir() if n.startswith("test_")):
        fn = globals()[name]
        try:
            fn()
            print("PASS", name)
        except Exception as exc:  # noqa: BLE001 - 自建 runner，报出全部失败
            failed += 1
            print("FAIL", name, f"{type(exc).__name__}: {exc}")
    print(f"{failed} failed")
    sys.exit(1 if failed else 0)