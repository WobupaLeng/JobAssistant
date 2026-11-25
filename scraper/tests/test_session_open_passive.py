"""`session.open` 的被动分支（查登录态）不许启动浏览器（用户 m01706）。

现场：用户把浏览器关掉后点界面上的「重试」，Chrome 窗口又自己弹了出来。
链路是 `login_status` → Rust 侧「不 spawn、不拉起」→ 但 sidecar 里这条
`session_open`（不带 `page`）照样调 `ensure_context`，而它见不到进程就
`start_cdp_chrome` —— 一个「重试」按钮把浏览器又打开了。

本文件钉死两条语义：
1. 浏览器没起来时，被动查询只回报 `state: unknown`，**不调** `ensure_context`。
2. 浏览器本来就在跑时，被动查询照旧给出真实登录态（不能为了不启动就把功能关掉）。
"""

import sys
from contextlib import contextmanager
from pathlib import Path

# 测试文件在 scraper/tests/，被测模块在 scraper/（与 sidecar.py 的 sys.path 姿势一致）
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import browser  # noqa: E402
import chat as chat_mod  # noqa: E402
import sidecar  # noqa: E402

_MISSING = object()


class Proc:
    """CDP Chrome 进程替身：`poll()` 返回 None 表示还活着。"""

    def __init__(self, alive: bool) -> None:
        self.alive = alive

    def poll(self):
        return None if self.alive else 1


@contextmanager
def patched(**attrs):
    """临时替换模块属性，退出时原样还原（不装 pytest，自己管现场）。"""
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


@contextmanager
def patched_chat(**attrs):
    old = {k: getattr(chat_mod, k, _MISSING) for k in attrs}
    for k, v in attrs.items():
        setattr(chat_mod, k, v)
    try:
        yield
    finally:
        for k, v in old.items():
            if v is _MISSING:
                delattr(chat_mod, k)
            else:
                setattr(chat_mod, k, v)


def test_browser_running_false_when_process_dead():
    """进程已退出就是没在跑：端口还通也不许当成「在跑」（关窗口后端口会短暂残留）。"""
    with patched(
        _cdp_proc=Proc(alive=False),
        _cdp_port=9222,
        _port_listening=lambda port: True,
    ):
        assert sidecar._browser_running() is False, "进程没了必须判「没在跑」"


def test_passive_check_without_browser_does_not_start_it():
    """被动查询（不带 page）在浏览器没起来时只回报 unknown，绝不拉起。"""
    out: list[dict] = []
    logs: list[tuple[str, str]] = []
    calls: list[str] = []

    with patched(
        _cdp_proc=None,
        _cdp_port=None,
        emit=out.append,
        log=lambda level, msg: logs.append((level, msg)),
        ensure_context=lambda log_fn: calls.append("ensure") or object(),
    ):
        sidecar._run_session_open({"id": "c1", "platform": "boss"})

    assert calls == [], "被动查询不许调 ensure_context（它会 start_cdp_chrome）"
    assert len(out) == 1, out
    frame = out[0]
    assert frame["id"] == "c1"
    assert frame["ok"] is True
    assert frame["data"]["state"] == "unknown"
    assert frame["data"]["logged_in"] is False
    assert any("不启动浏览器" in msg for _, msg in logs), logs


def test_passive_check_with_running_browser_still_reports_state():
    """浏览器本来就在跑时，被动查询照旧给出真实登录态。"""
    out: list[dict] = []
    calls: list[str] = []

    with patched(
        _cdp_proc=Proc(alive=True),
        _cdp_port=9222,
        _port_listening=lambda port: True,
        emit=out.append,
        log=lambda level, msg: None,
        ensure_context=lambda log_fn: calls.append("ensure") or object(),
    ):
        with patched_browser(detect_logged_in=lambda ctx, platform: True):
            sidecar._run_session_open({"id": "c2", "platform": "boss"})

    assert calls == ["ensure"], "浏览器在跑时仍要复用（只连接，不新开进程）"
    assert len(out) == 1, out
    assert out[0]["data"]["state"] == "logged_in"
    assert out[0]["data"]["logged_in"] is True


def test_active_open_still_launches_browser():
    """带 page 的「打开 Boss 消息」是主动动作，仍要拉起浏览器。"""
    out: list[dict] = []
    calls: list[str] = []

    with patched(
        _cdp_proc=None,
        _cdp_port=None,
        emit=out.append,
        log=lambda level, msg: None,
        ensure_context=lambda log_fn: calls.append("ensure") or object(),
    ):
        with patched_browser(detect_logged_in=lambda ctx, platform: True):
            # 真 ChatClient 要真页面；这里只验证「主动路径仍然会拉起浏览器」
            with patched_chat(ChatClient=lambda log: _FakeChatClient()):
                sidecar._run_session_open(
                    {"id": "c3", "platform": "boss", "page": "chat"}
                )

    assert calls == ["ensure"], "主动打开消息页必须拉起浏览器"
    assert out[0]["data"]["state"] == "logged_in"


class _FakeChatClient:
    def chat_page(self, ctx):
        return ctx


class _FakeProbeQ:
    """探活车道的替身：只记下投递进来的任务，result() 立即返回。"""

    def __init__(self, value: bool = True) -> None:
        self.value = value
        self.submitted: list = []

    def submit(self, fn, *args):
        self.submitted.append((fn, args))
        # 同步执行：替身的意义是「投递就到了」，真 executor 也是唯一线程串行跑。
        fn(*args)

        class _Done:
            def result(self, timeout=None):
                return self.value

        return _Done()


def test_passive_check_while_scraping_answers_via_chat_lane():
    """抓取占着浏览器车道时，被动查询必须走聊天车道答话，不许被「正在抓取」挡回。"""
    out: list[dict] = []
    q = _FakeProbeQ(True)

    with patched(
        _busy_now=lambda action: f"正在抓取，请先点「停止」再{action}",
        _browser_running=lambda: True,
        _probe_logged_in_via_chat=lambda platform: True,
        _probe_q=q,
        emit=out.append,
        log=lambda level, msg: None,
    ):
        sidecar.handle({"id": "c4", "cmd": "session.open", "platform": "boss"})

    assert len(q.submitted) == 1, q.submitted
    assert q.submitted[0][0] is sidecar._run_login_probe
    assert q.submitted[0][1][1] == "boss"
    assert out == [
        {"id": "c4", "ok": True, "data": {"logged_in": True, "state": "logged_in"}}
    ], out


def test_active_open_while_scraping_still_rejects():
    """带 page 的主动开页仍要独占浏览器：抓取中照旧挡回，不能降级成只读。"""
    out: list[dict] = []
    q = _FakeProbeQ(True)

    with patched(
        _busy_now=lambda action: f"正在抓取，请先点「停止」再{action}",
        _probe_q=q,
        emit=out.append,
        log=lambda level, msg: None,
    ):
        sidecar.handle(
            {"id": "c5", "cmd": "session.open", "platform": "boss", "page": "chat"}
        )

    assert q.submitted == [], "主动开页不许走只读探活"
    assert out[0]["ok"] is False
    assert "正在抓取" in out[0]["error"]["msg"]
    assert out[0]["error"]["code"] == "SESSION_FAILED"


def test_probe_timeout_reports_unknown_not_no_login():
    """探活超时只能答 unknown：把「查不到」报成「没登录」会让已登录用户看见「请先登录」。"""
    out: list[dict] = []

    def _timeout(platform: str) -> bool:
        raise TimeoutError("timed out")

    with patched(
        _browser_running=lambda: True,
        _probe_logged_in_via_chat=_timeout,
        emit=out.append,
        log=lambda level, msg: None,
    ):
        sidecar._run_login_probe({"id": "c6"}, "boss")

    assert out == [
        {"id": "c6", "ok": True, "data": {"logged_in": False, "state": "unknown"}}
    ], out


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
