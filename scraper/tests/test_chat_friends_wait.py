"""`ChatClient.friends()` / `messages()` 的等待语义（离线，不碰浏览器）。

真实故障（用户 m00001 的截图：「读取消息失败：ChatError: 没拿到 Boss 会话列表」）：

1. `friends()` 在 reload 之前没把 `getGeekFriendList` 的**解析 handler** 挂回去——
   `chat_page()` 里那次注册是「只记账不入队」的，于是 reload 回来的接口正文被丢掉，
   `_friends_order` 永远是空的，每次刷新都报「没拿到 Boss 会话列表」。
2. `_drained` 是**累计**计数：上一次（甚至上次进聊天页时）那一次响应会让
   `_wait_friends` / `_wait_messages` 在第一拍就以为「接口已经回来过」，立刻判失败——
   表现是「只有第一次能拉到，之后全报错 / 第二条会话消息恒为空」。

这两个语义都用替身页面钉住：接口在 reload / 点击时同步回包。
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import chat  # noqa: E402

FRIEND_URL = "https://www.zhipin.com/wapi/zprelation/friend/getGeekFriendList.json"
HISTORY_URL = "https://www.zhipin.com/wapi/zprelation/friend/historyMsg.json"

FRIENDS_PAYLOAD = json.dumps(
    {
        "code": 0,
        "zpData": {
            "result": [
                {"uid": "u1", "name": "HR-A", "brandName": "公司A", "lastTS": 1700000000000},
                {"uid": "u2", "name": "HR-B", "brandName": "公司B", "lastTS": 1700000001000},
            ]
        },
    }
)
HISTORY_PAYLOAD = json.dumps(
    {
        "code": 0,
        "zpData": {
            "messages": [
                {"mid": "m1", "body": {"text": "你好"}, "from": {"uid": "u1"}, "msgTime": 0}
            ]
        },
    }
)

ITEM_ROWS = [
    {"i": 0, "name": "HR-A", "company": "公司A", "title": "", "last_msg": "",
     "last_time": "", "unread": "", "txt": "HR-A 公司A"},
    {"i": 1, "name": "HR-B", "company": "公司B", "title": "", "last_msg": "",
     "last_time": "", "unread": "", "txt": "HR-B 公司B"},
]


class FakeResp:
    def __init__(self, url, payload):
        self.url = url
        self.status = 200
        self._payload = payload

    def text(self):
        return self._payload


class FakeLocator:
    def __init__(self, page):
        self.page = page

    def nth(self, i):
        self.page.clicked = i
        return self

    def click(self, timeout=None):
        # 点开会话 ⇒ 页面立刻请求 historyMsg（真实时序：0.3~1.5s 后回，无所谓，等待是轮询的）
        self.page.fire(HISTORY_URL, HISTORY_PAYLOAD)

    def first(self):
        return self


class FakePage:
    """够用的替身：监听器真的挂/摘，reload 与点击都同步回包。"""

    def __init__(self):
        self.url = chat.CHAT_URL
        self.listeners = []
        self.clicked = None

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
        self.url = url

    def wait_for_timeout(self, ms):
        pass

    def wait_for_selector(self, sel, timeout=None):
        return {"tag": "div"}

    def locator(self, sel):
        return FakeLocator(self)

    def evaluate(self, js):
        if "querySelectorAll('.user-list-content ul li')" in js:
            return ITEM_ROWS
        if "Boolean(" in js:
            return True
        return []

    # ---- 测试辅助
    def fire(self, url, payload):
        for fn in list(self.listeners):
            fn(FakeResp(url, payload))


def client_with(page):
    c = chat.ChatClient(lambda lvl, msg: None)
    c._page = page
    return c


def test_friends_parses_after_reload_every_time():
    page = FakePage()
    c = client_with(page)
    # chat_page() 的那次注册：只记账、不解析（reload 前必须被真正的 handler 换掉）
    c._catch(page, chat.KEY_FRIENDS, None)
    page.fire(FRIEND_URL, FRIENDS_PAYLOAD)  # 上次进聊天页时接口就回过一次了

    first = c.friends(None)
    assert [i["uid"] for i in first] == ["u1", "u2"]
    assert first[0]["company"] == "公司A"

    # 第二轮：累计计数必须已经清零，否则第一拍就判「没拿到」并抛错
    second = c.friends(None)
    assert [i["uid"] for i in second] == ["u1", "u2"]


def test_second_conversation_gets_its_own_messages():
    page = FakePage()
    c = client_with(page)
    c.friends(None)

    one = c.messages(None, "u1")
    assert [m["content"] for m in one] == ["你好"], "第一条会话读不到消息"

    two = c.messages(None, "u2")
    assert [m["content"] for m in two] == ["你好"], "第二条会话被上一次的计数判成空"


def test_stale_history_from_previous_click_is_dropped():
    """上一次点击留下的响应不许算进这一次的等待（handler 绑的是别的 uid）。"""
    page = FakePage()
    c = client_with(page)
    c.friends(None)
    # 模拟「点开 u2 之前，上一条会话的响应还躺在队列里、而且还没被读走」
    c._catch(page, chat.KEY_HISTORY, c._history_handler("u3"))
    page.fire(HISTORY_URL, HISTORY_PAYLOAD)
    assert len(c._signals.get(chat.KEY_HISTORY) or []) == 1  # 还没 drain

    got = c.messages(None, "u2")
    assert [m["content"] for m in got] == ["你好"]
    assert c._msgs_raw.get("u3") is None, "旧响应不该在这轮被读进别的 uid"
