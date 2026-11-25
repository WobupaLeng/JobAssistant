"""`chat.ChatClient._goto` 的离线回归（不碰浏览器、不联网）。

真实故障（用户 m02288）：Boss 聊天页加载慢时页面自己跳走，Playwright 抛
`Page.goto: net::ERR_ABORTED`，多行 Call log 原样贴进右栏，占掉半个界面。

用替身页面固定三条语义：
1. 报错时页面如果**已经**跳到目标页，算成功（页面自己跳过去了，不该再报失败）。
2. 一直没到目标页就重试一次，仍失败则给出**单行**中文提示。
3. 浏览器已关时不许判成功（离线实测踩过：`page.url` 不抛错，仍返回旧地址）。
"""

import sys
from pathlib import Path

# 测试文件在 scraper/tests/，被测模块在 scraper/（与 sidecar.py 的 sys.path 姿势一致）
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import chat  # noqa: E402

CHAT_URL = chat.CHAT_URL


class FakePage:
    """`goto` 先写 url 再抛错 —— 与真实失败时的状态一致（页面停在报错那一页）。"""

    def __init__(self, fail_times, url_after=CHAT_URL, closed=False):
        self.n = 0
        self.fail_times = fail_times
        self.url = ""
        self.after = url_after
        self.closed = closed

    def is_closed(self):
        return self.closed

    def query_selector(self, sel):
        if self.closed:
            raise RuntimeError("Target page, context or browser has been closed")
        return {"tag": "body"}

    def goto(self, url, wait_until=None, timeout=None):
        self.n += 1
        self.url = url
        if self.n <= self.fail_times:
            raise RuntimeError(
                'Page.goto: net::ERR_ABORTED at %s\nCall log:\n'
                '  - navigating to "%s", waiting until "domcontentloaded"' % (url, url)
            )
        self.url = self.after

    def wait_for_timeout(self, ms):
        pass


class RefusedPage(FakePage):
    def goto(self, url, wait_until=None, timeout=None):
        raise RuntimeError("net::ERR_CONNECTION_REFUSED at x\nCall log:\n  - navigating")


class GonePage(FakePage):
    """页面还"在"（is_closed 为假）但地址停在旧页 —— 浏览器没了，读不到真因。"""

    def query_selector(self, sel):
        raise RuntimeError("Target page, context or browser has been closed")


def client():
    return chat.ChatClient(lambda lvl, msg: None)


def test_abort_but_landed_counts_as_success():
    # goto 先写 url 再抛错：地址已是目标页 = 页面自己跳过去了，这时不该再报失败
    page = FakePage(1)
    client()._goto(page, CHAT_URL, "Boss 聊天页")
    assert page.n == 1, "已落到目标页就不该再重试"
    assert page.url == CHAT_URL


def test_persistent_abort_retries_once_then_single_line_error():
    class AbortPage(FakePage):
        """每次都 ABORTED，且地址始终停在旧页（页面没有跳去目标页）。"""

        def goto(self, url, wait_until=None, timeout=None):
            self.n += 1
            raise RuntimeError(
                'Page.goto: net::ERR_ABORTED at %s\nCall log:\n  - navigating' % url
            )

    page = AbortPage(9)
    page.url = "about:blank"
    try:
        client()._goto(page, CHAT_URL, "Boss 聊天页")
    except chat.ChatError as exc:
        msg = str(exc)
    else:
        raise AssertionError("两次都失败必须抛 ChatError")
    assert page.n == 2, "第一次被打断、地址仍在旧页，就该再试一次"
    assert len(msg.splitlines()) == 1, "多行 Call log 会把提示条撑成两块"
    assert "ERR_ABORTED" not in msg, "原始技术文案不许贴到界面上"
    assert "刷新" in msg, "提示必须说清下一步"


def test_other_error_keeps_first_line_only():
    try:
        client()._goto(RefusedPage(0), CHAT_URL, "Boss 聊天页")
    except chat.ChatError as exc:
        msg = str(exc)
    else:
        raise AssertionError("连接被拒必须抛 ChatError")
    assert msg.splitlines() == [msg]
    assert "ERR_CONNECTION_REFUSED" in msg


def test_url_left_on_target_is_not_reported_as_success():
    """`goto` 没报错之外的另一种形态：地址写着目标页，但页面根本探不到。

    真实踩坑：浏览器关掉后 `page.url` 不抛错，仍返回上一次的地址，
    光看地址会把「浏览器没了」咽掉。
    """

    class StalePage(FakePage):
        """url 停在目标页，但页面已不可用（不抛 goto 错，只探不到节点）。"""

        def query_selector(self, sel):
            raise RuntimeError("Target page, context or browser has been closed")

    gone = StalePage(0)
    gone.url = CHAT_URL
    assert chat._goto_landed(gone, CHAT_URL) is False, "页面探不到时不许判成功"
    assert chat._goto_landed(FakePage(0), CHAT_URL) is False, "地址为空也不许判成功"

    alive = FakePage(0)
    alive.url = CHAT_URL
    assert chat._goto_landed(alive, CHAT_URL) is True, "页面活着且地址对 = 已经落地"


def test_closed_browser_says_how_to_fix():
    """浏览器已关：给「扫码登录」这一步，而不是一句技术错误。"""

    class ClosedPage(FakePage):
        def goto(self, url, wait_until=None, timeout=None):
            raise RuntimeError(
                "Page.goto: Target page, context or browser has been closed"
            )

    try:
        client()._goto(ClosedPage(0), CHAT_URL, "Boss 聊天页")
    except chat.ChatError as exc:
        msg = str(exc)
    else:
        raise AssertionError("浏览器已关必须抛 ChatError")
    assert "扫码登录" in msg and len(msg.splitlines()) == 1


def test_bookkeeping_only_listener_ends_the_wait():
    """`_catch(..., None)` 只记账：导航被打断时正文取不到，也要让等待方停止空转。"""
    c = client()
    fns = []

    class Resp:
        url = "https://x/wapi/zprelation/friend/getGeekFriendList.json"
        status = 200

    class Pg:
        def on(self, ev, fn):
            fns.append(fn)

        def remove_listener(self, ev, fn):
            pass

    c._catch(Pg(), chat.KEY_FRIENDS, None)
    fns[-1](Resp())
    assert c._signals.get(chat.KEY_FRIENDS) is None, "只记账的监听不该往信号队列塞东西"
    assert c._drain(chat.KEY_FRIENDS) == 1, "记账数要让等待循环立刻结束"