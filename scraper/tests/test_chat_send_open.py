"""`ChatClient.send()` 打开会话的方式（离线，不碰浏览器）。

真实故障（用户 m00417 的报错原文）：

    发送失败：TimeoutError: Locator.wait_for: Timeout 15000ms exceeded.
    Call log:
      - waiting for locator("#chat-input").first to be visibl…

`send()` 以前**丢掉** `_open_conversation()` 的返回值。DOM 会话列表是虚拟滚动的
（约 40 行，接口给 100 条），目标会话排到后面时它找不到可点的行、什么也不点，
页面还停在会话列表页；接着等 `#chat-input` 只能白等满 15 秒，把一句裸的
Playwright 超时甩给用户。

现在的语义：
1. 点中了行 ⇒ 正常发；
2. 没点中但输入框已经在（`ensure()` 刚建好的会话）⇒ 直接发，不打断页面；
3. 没点中且没有输入框 ⇒ 按 uid 直接打开这条会话再发；
4. 打开后仍然没有输入框 ⇒ 报一句中文错，不带 Playwright 的 Call log。
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import chat  # noqa: E402

ROW_A = {"i": 0, "name": "HR-A", "company": "公司A", "title": "", "last_msg": "",
         "last_time": "", "unread": "", "txt": "HR-A 公司A"}

FRIEND_URL = "https://www.zhipin.com/wapi/zprelation/friend/getGeekFriendList.json"
FRIENDS_PAYLOAD = json.dumps(
    {
        "code": 0,
        "zpData": {"result": [{"uid": "u1", "name": "HR-A", "brandName": "公司A",
                               "lastTS": 1700000000000}]},
    }
)


class FakeResp:
    def __init__(self, url, payload):
        self.url = url
        self.status = 200
        self._payload = payload

    def text(self):
        return self._payload


class FakeKeyboard:
    def __init__(self, page):
        self.page = page

    def press(self, key):
        if key == "Backspace":
            self.page.typed = ""

    def type(self, text, delay=None):
        self.page.typed = text


class FakeLocator:
    def __init__(self, page, sel):
        self.page = page
        self.sel = sel

    def first(self):
        return self

    def nth(self, i):
        self.page.clicked_row = i
        return self

    def is_visible(self):
        return self.page.input_visible

    def wait_for(self, state=None, timeout=None):
        if self.sel == chat.SEL_INPUT and not self.page.input_visible:
            raise TimeoutError(
                'Locator.wait_for: Timeout 15000ms exceeded.\n'
                'Call log:\n  - waiting for locator("#chat-input").first to be visible'
            )

    def click(self, timeout=None):
        if self.sel == chat.SEL_SEND:
            self.page.typed = ""  # 发送即清空输入框（前端已接单）
        elif self.sel == chat.SEL_ITEM:
            self.page.input_visible = True  # 点开一行 = 对话区出现


class FakePage:
    """够用的替身：只有「会话行」「输入框」「发送按钮」三件东西。"""

    def __init__(self, rows, open_by_uid=True):
        self.url = chat.CHAT_URL
        self.rows = rows
        self.open_by_uid = open_by_uid
        self.input_visible = False
        self.typed = ""
        self.clicked_row = None
        self.gotos = []
        self.listeners = []
        self.keyboard = FakeKeyboard(self)

    # ---- playwright 表面
    def is_closed(self):
        return False

    def query_selector(self, sel):
        return {"tag": "body"}

    def on(self, event, fn):
        self.listeners.append(fn)

    def remove_listener(self, event, fn):
        if fn in self.listeners:
            self.listeners.remove(fn)

    def reload(self, **kwargs):
        self.fire(FRIEND_URL, FRIENDS_PAYLOAD)

    def goto(self, url, **kwargs):
        self.gotos.append(url)
        self.url = url
        # 地址里带着会话 id ⇒ 页面真的打开了这条会话，输入框就在
        if self.open_by_uid and "?id=" in url:
            self.input_visible = True

    def wait_for_timeout(self, ms):
        pass

    def wait_for_selector(self, sel, timeout=None):
        return {"tag": "div"}

    def locator(self, sel):
        return FakeLocator(self, sel)

    def evaluate(self, js):
        if "querySelectorAll('.user-list-content ul li')" in js:
            return self.rows
        if "Boolean(" in js:
            return True
        if "#chat-input" in js:
            return self.typed
        return []

    # ---- 测试辅助
    def fire(self, url, payload):
        for fn in list(self.listeners):
            fn(FakeResp(url, payload))


def client_with(page):
    c = chat.ChatClient(lambda lvl, msg: None)
    c._page = page
    return c


def test_send_opens_conversation_by_uid_when_row_missing():
    """会话行没渲染出来（虚拟滚动漏了这条）时，必须按 uid 打开，而不是硬等输入框。"""
    page = FakePage(rows=[ROW_A])
    c = client_with(page)
    c._friends_raw["u9"] = {"uid": "u9", "name": "HR-Z", "brand": "公司Z"}

    out = c.send(None, "u9", "你好")

    assert out == "你好"
    assert page.clicked_row is None, "列表里没有这一行，不该点任何行"
    assert page.gotos == [f"{chat.CHAT_URL}?id=u9"], f"没有按 uid 打开会话：{page.gotos}"


def test_send_clicks_row_when_present():
    """行在列表里就点行，不额外导航（少一次整页刷新）。"""
    page = FakePage(rows=[ROW_A])
    c = client_with(page)
    c._friends_raw["u1"] = {"uid": "u1", "name": "HR-A", "brand": "公司A"}

    assert c.send(None, "u1", "在吗") == "在吗"
    assert page.clicked_row == 0
    assert page.gotos == [], f"行点中了却还导航了一次：{page.gotos}"


def test_send_keeps_open_conversation_without_navigating():
    """`ensure()` 刚建好的会话：行还没进列表，但页面就在这条会话上，别打断它。"""
    page = FakePage(rows=[])
    page.input_visible = True
    c = client_with(page)
    c._friends_raw.pop("u9", None)

    assert c.send(None, "u9", "你好") == "你好"
    assert page.gotos == [], f"会话本来就开着，不该再导航：{page.gotos}"


def test_send_reports_plain_chinese_error_when_input_never_shows():
    """按 uid 也打不开时，给一句中文错，别把 Playwright 的多行 Call log 弹给用户。"""
    page = FakePage(rows=[ROW_A], open_by_uid=False)
    c = client_with(page)
    c._friends_raw["u9"] = {"uid": "u9", "name": "HR-Z", "brand": "公司Z"}

    try:
        c.send(None, "u9", "你好")
    except chat.ChatError as exc:
        text = str(exc)
        assert "输入框" in text
        assert "TimeoutError" not in text and "Call log" not in text
    else:
        raise AssertionError("输入框始终没出现时应当报 ChatError")
    assert page.gotos == [f"{chat.CHAT_URL}?id=u9"]
