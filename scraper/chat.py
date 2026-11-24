"""Boss 直聘聊天：会话列表、历史消息、发消息、发简历、撤回。

## 与旧版 `backend/scrapers/boss_chat.py` 的根本差异

旧版用 **CDP 连同一台 Chrome 开第二条 Playwright 车道**，让「抓取」和「聊天」
互不阻塞。SPEC-002 已放弃 CDP，所以新版只有一条车道：**所有命令排队到同一个
工作线程**执行。这带来两个必须接受的事实：

1. 抓取进行中发起聊天，聊天要等当前这一轮抓取结束才轮得上（sidecar 是
   同步串行的，见 `sidecar.py::main`）。
2. patchright 的同步 API 绑定创建它的线程，**绝不允许跨线程**调用——
   历史上 `Cannot switch to a different thread` 就是这么来的。

排队是刻意的取舍：宁可让用户看到「排队中」，也不要为了并发去碰线程铁律。

## 监听器铁律（原样搬过来的踩坑记录，不要重写）

`page.on("response")` 的回调运行在 playwright 自己的调度栈里，**绝不能在里面
调用 `resp.text()` 这类阻塞 API**。一旦调用，driver 连接会被整个搞断：

    Response.text: Connection closed while reading from the driver

之后所有请求都变成 `Event loop is closed` / `Sync API inside the asyncio loop`。
所以监听器只把响应对象塞进队列，正文由主流程在 `_drain` 里读。
"""
from __future__ import annotations

import html
import re
import threading
import time
from typing import Any, Callable

LogFn = Callable[[str, str], None]

CHAT_URL = "https://www.zhipin.com/web/geek/chat"

# 页面元素。**不要凭直觉改**：这些是 2026-11 实测的选择器，
# Boss 换了 class 名会让它们整片失效，症状是「列表空 / 点不动」。
SEL_ITEM = ".user-list-content ul li"
SEL_CONV = ".chat-conversation"
SEL_INPUT = "#chat-input"
SEL_SEND = ".chat-op .btn-send, button.btn-send"

# 接口关键字。命中即记下响应，正文留到主流程读。
KEY_FRIENDS = "getGeekFriendList"
KEY_HISTORY = "historyMsg"
KEY_ENTER = "geekEnter"
KEY_RESUME = "attachment/checkbox"

# 正文里的 HTML 标签（如 <copy>备注</copy>）和卡片上的纯按钮文字
_TAG_RE = re.compile(r"<[^>]{0,40}>")
_NAV_TEXT_RE = re.compile(
    r"^(查看详细分析|点击预览附件简历|复制微信号|拒绝|同意|暂不考虑|可以接受|复制|发送)$"
)


class ChatError(RuntimeError):
    """聊天链路的可预期失败（未登录、找不到会话等），消息直接给用户看。"""


# ---------------------------------------------------------------- 归一化


def fmt_time(ts: Any) -> str:
    """把毫秒/秒时间戳格式化成「今天 14:20」「11-18 09:03」这种界面直接可用的形式。"""
    try:
        v = int(float(ts))
    except (TypeError, ValueError):
        return ""
    if v <= 0:
        return ""
    if v > 10**11:  # 毫秒
        v //= 1000
    lt = time.localtime(v)
    today = time.localtime()[:3]
    if lt[:3] == today:
        return time.strftime("%H:%M", lt)
    if lt[0] == today[0]:
        return time.strftime("%m-%d %H:%M", lt)
    return time.strftime("%Y-%m-%d %H:%M", lt)


def normalize_friend(raw: dict[str, Any]) -> dict[str, Any]:
    """把 getGeekFriendList 的一条整理成界面要用的字段。"""
    info = raw.get("lastMessageInfo") or {}
    return {
        "uid": str(raw.get("uid") or ""),
        "security_id": raw.get("securityId") or "",
        "name": raw.get("name") or "",
        "title": raw.get("title") or "",
        "brand": raw.get("brandName") or "",
        "avatar": raw.get("avatar") or "",
        "last_msg": raw.get("lastMsg") or info.get("showText") or "",
        "last_ts": raw.get("lastTS") or 0,
        "last_time": raw.get("lastTime") or "",
        "unread": raw.get("unreadMsgCount") or 0,
        "encrypt_job_id": raw.get("encryptJobId") or "",
        "job_id": raw.get("jobId"),
    }


def normalize_message(raw: dict[str, Any], my_id: str | None) -> dict[str, Any]:
    """把 historyMsg 的一条整理成 {sender, content, time, msg_id}。

    `sender` 只取 `me` / `hr` / `system` —— 前端的渲染分支只认这三个值。
    """
    body = raw.get("body") or {}
    mid = raw.get("mid") or raw.get("msgId") or 0
    received = raw.get("received")
    text = ""
    from_text = False
    for key in ("text", "content", "showText", "desc"):
        v = body.get(key)
        if isinstance(v, str) and v.strip():
            from_text = True
            text = " ".join(html.unescape(_TAG_RE.sub("", v)).split())
            text = _NAV_TEXT_RE.sub("", text).strip()
            break
    # body.type 是模板类型（正文里的 16 只是模板 id），外层 raw.type 才是消息类型
    try:
        btype = int(raw.get("type") or 0)
    except (TypeError, ValueError):
        btype = 0

    # 「是不是我发的」：received 偶尔缺，再依次看 fromId/from，最后留 None 交给 DOM 覆盖
    from_me: bool | None
    if received is not None:
        from_me = bool(received) is False
    else:
        src_id = body.get("fromId") if body.get("fromId") is not None else body.get("from")
        if isinstance(src_id, (int, str)) and my_id and str(src_id) == str(my_id):
            from_me = True
        elif src_id is not None:
            from_me = False
        else:
            from_me = None

    if not text:
        # 结构化消息（职位卡片、系统提示等）退化成一句说明
        if btype == 8:
            job = body.get("jobDesc") or {}
            name = job.get("jobName")
            text = "职位卡片" + (f"：{name}" if name else "")
        elif btype in (5, 6):
            text = "图片"
        elif btype in (3, 7):
            text = "附件简历"
        elif btype == 2:
            text = "语音"
        elif btype in (1, 4, 16, 17, 18, 19, 20, 21):
            text = ""  # 纯系统/时间线消息：不画气泡，只当时序处理
        else:
            text = "其他消息"

    ts = raw.get("msgTime") or raw.get("time") or body.get("msgTime") or 0
    ts_int = int(ts) if str(ts).isdigit() else 0
    # 真的抓到正文才是对话气泡；类型兜底出来的（「图片」「附件简历」）算系统行
    system = (not from_text) or (not text) or btype in (16, 17, 18, 19, 20, 21)
    if system:
        sender = "system"
    elif from_me is True:
        sender = "me"
    else:
        sender = "hr"
    return {
        "sender": sender,
        "content": text,
        "time": fmt_time(ts_int) or None,
        "msg_id": str(mid) if mid else None,
        "_from_me": from_me,  # 内部字段，merge_dom 用，对外会被丢掉
    }


def merge_dom_status(
    msgs: list[dict[str, Any]], dom: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    """用 DOM 校正接口消息里「是不是我发的」。

    接口的 `received` 并非每条都有，缺的时候 `from_me` 会是 None（也可能误判），
    而 DOM 的 li 一定有 `item-myself` / `item-friend`，所以按位置对齐后以 DOM 为准。
    """
    if not dom or not msgs:
        return msgs
    if len(dom) >= len(msgs):
        for m, d in zip(msgs, dom[-len(msgs):]):
            if m.get("_from_me") is None and d.get("from_me") is not None:
                m["_from_me"] = bool(d["from_me"])
    for m in msgs:
        if m.get("_from_me") is True:
            m["sender"] = "me"
        elif m.get("_from_me") is False and m["sender"] != "system":
            m["sender"] = "hr"
    return msgs  # 由调用方在返回前剥掉 _from_me


def strip_internal(msgs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """去掉 `_from_me` 这类只在本模块内部用的键，别漏到协议里。"""
    return [{k: v for k, v in m.items() if not k.startswith("_")} for m in msgs]


# ---------------------------------------------------------------- 浏览器交互


class ChatClient:
    """一条车道上的聊天操作。**必须在拥有 context 的同一个线程里用。**

    所有方法都是同步阻塞的：调用方（sidecar 的工作线程）本身就在排队，
    这里再加一层并发只会撞上 patchright 的线程铁律。
    """

    def __init__(self, log: LogFn) -> None:
        self.log = log
        # 响应信号队列：(key → [(resp, handler), ...])。
        # 监听器只入队，不出队读正文——见模块 docstring 的铁律。
        self._signals: dict[str, list[tuple[Any, Callable[[str], None]]]] = {}
        self._sig_lock = threading.Lock()
        self._listeners: dict[tuple[int, str], Any] = {}
        self._friends_raw: dict[str, dict[str, Any]] = {}
        self._friends_order: list[str] = []
        self._msgs_raw: dict[str, list[dict[str, Any]]] = {}
        self._my_id: str | None = None
        self._page: Any = None

    # ---- 信号队列

    def _catch(self, page: Any, key: str, handler: Callable[[str], None]) -> None:
        """给 page 挂一个响应监听：URL 含 key 时**只记下响应对象**。

        监听器运行在 playwright 的调度栈里，调用 `resp.text()` 会弄断整个
        driver 连接（详见模块 docstring）。正文一律走 `_drain` 在主流程读。
        """

        def _on_response(resp: Any) -> None:
            try:
                if key not in resp.url or resp.status != 200:
                    return
                with self._sig_lock:
                    self._signals.setdefault(key, []).append((resp, handler))
            except Exception:
                # 监听器里绝不能抛异常：会被调度器吞掉并连带弄坏连接
                pass

        # 同一个 key 在同一个 page 上只挂一次：重复挂会让回调栈越积越深
        reg = (id(page), key)
        old = self._listeners.get(reg)
        if old is not None:
            try:
                page.remove_listener("response", old)
            except Exception:
                pass
        self._listeners[reg] = _on_response
        page.on("response", _on_response)

    def _drain(self, key: str) -> int:
        """在主流程里把监听器攒下的响应正文读出来交给 handler。返回处理条数。"""
        with self._sig_lock:
            items = self._signals.pop(key, [])
        n = 0
        for resp, handler in items:
            try:
                handler(resp.text())
                n += 1
            except Exception as exc:
                self.log("warn", f"读取 {key} 响应失败：{type(exc).__name__}: {exc}")
        return n

    # ---- 回调

    def _on_friends(self, raw: str) -> None:
        import json

        data = json.loads(raw)
        zp = data.get("zpData") or {}
        rows = zp.get("geekFriendList") or zp.get("friendList") or []
        order: list[str] = []
        for r in rows:
            uid = str(r.get("uid") or "")
            if not uid:
                continue
            self._friends_raw[uid] = r
            order.append(uid)
        self._friends_order = order

    def _on_history(self, raw: str) -> None:
        import json

        data = json.loads(raw)
        zp = data.get("zpData") or {}
        msgs = zp.get("messages") or zp.get("msgList") or []
        uid = str(zp.get("encryptUid") or zp.get("uid") or "")
        if uid:
            self._msgs_raw[uid] = [m for m in msgs if isinstance(m, dict)]

    def _on_enter(self, raw: str) -> None:
        import json

        data = json.loads(raw)
        zp = data.get("zpData") or {}
        me = zp.get("myId") or zp.get("uid") or zp.get("userId")
        if me is not None:
            self._my_id = str(me)

    # ---- 页面

    def chat_page(self, ctx: Any) -> Any:
        """拿到聊天页并发起一次会话列表拉取。"""
        if self._page is not None:
            try:
                if not self._page.is_closed():
                    return self._page
            except Exception:
                pass
        page = None
        for p in ctx.pages:
            if "/web/geek/chat" in (p.url or ""):
                page = p
                break
        if page is None:
            # 与抓取/登录共用同一套空白页复用规则，避免各写一份导致行为不一致
            import browser as browser_mod
            page = browser_mod.reuse_or_new_page(ctx)
        if "/web/geek/chat" not in (page.url or ""):
            page.goto(CHAT_URL, wait_until="domcontentloaded", timeout=45_000)
        self._page = page
        self._catch(page, KEY_FRIENDS, self._on_friends)
        self._catch(page, KEY_HISTORY, self._on_history)
        self._catch(page, KEY_ENTER, self._on_enter)
        return page

    def _item_rows(self, page: Any) -> list[tuple[int, str, str]]:
        """读出会话列表每一行的 (索引, HR 名字, 整行文本)。

        只用于「列表渲染出来没有」这类计数场景（`_wait_items`）。要拿字段请用
        `_item_details`——这里只有名字，公司/岗位/时间都在 `innerText` 里没拆开。
        """
        return [(r["i"], r["name"], r["txt"]) for r in self._item_details(page)]

    def _item_details(self, page: Any) -> list[dict[str, Any]]:
        """逐行解析会话列表，尽量补齐字段。

        接口（`getGeekFriendList`）没回来时才走这里，所以能拿多少算多少：
        名字、最近一条消息、时间、公司、岗位都从行内 DOM 直接取，
        取不到的留空字符串（由调用方转成 `None`），**绝不硬写成 None**——
        那会让每一行都显示成「—」，看起来像列表构建失败。
        """
        try:
            rows = page.evaluate(
                r"""() => {
                    const pick = (el, sels) => {
                        for (const s of sels) {
                            const n = el.querySelector(s);
                            if (n && n.textContent && n.textContent.trim()) {
                                return n.textContent.trim();
                            }
                        }
                        return '';
                    };
                    return [...document.querySelectorAll('.user-list-content ul li')].map((el, i) => {
                        const txt = (el.innerText || '').replace(/\s+/g, ' ').trim();
                        return {
                            i,
                            name: pick(el, ['.title', '.name', '.geek-name']),
                            company: pick(el, ['.company-name', '.brand-name', '.company']),
                            title: pick(el, ['.job-name', '.position', '.geek-job']),
                            last_msg: pick(el, ['.last-msg', '.msg-text', '.lastMessage']),
                            last_time: pick(el, ['.time', '.last-time', '.msg-time']),
                            unread: pick(el, ['.unread-count', '.badge', '.num']),
                            txt,
                        };
                    });
                }"""
            )
        except Exception:
            return []
        out: list[dict[str, Any]] = []
        for r in rows or []:
            if not isinstance(r, dict):
                continue
            out.append(
                {
                    "i": int(r.get("i") or 0),
                    "name": str(r.get("name") or ""),
                    "company": str(r.get("company") or ""),
                    "title": str(r.get("title") or ""),
                    "last_msg": str(r.get("last_msg") or ""),
                    "last_time": str(r.get("last_time") or ""),
                    "unread": str(r.get("unread") or ""),
                    "txt": str(r.get("txt") or ""),
                }
            )
        return out

    def _wait_items(self, page: Any, timeout_s: float = 12.0) -> int:
        t0 = time.time()
        while time.time() - t0 < timeout_s:
            n = len(self._item_rows(page))
            if n:
                return n
            page.wait_for_timeout(300)
        return 0

    @staticmethod
    def _int_or_zero(text: Any) -> int:
        """从「3」「99+」这类角标文字里取整数；取不到按 0 算。

        列表兜底路径专用：未读数只是为了显示角标，解析失败不该让整行报废。
        """
        digits = re.sub(r"\D", "", str(text or ""))
        return int(digits) if digits else 0

    def _poll(self, page: Any, expr: str, timeout_ms: int = 1500, step_ms: int = 60) -> bool:
        """轮询一段 JS 表达式直到为真。比 wait_for_selector 灵活：条件常是「值变了」。"""
        t0 = time.time()
        while (time.time() - t0) * 1000 < timeout_ms:
            try:
                if page.evaluate(f"() => Boolean({expr})"):
                    return True
            except Exception:
                pass
            page.wait_for_timeout(step_ms)
        return False

    def _open_conversation(self, page: Any, friend: dict[str, Any]) -> None:
        """点开会话。会话项文本在深层子节点里，locator.filter(has_text=) 会超时，
        所以先一次性 evaluate 读出所有条目，再按索引点。"""
        name = (friend or {}).get("name") or ""
        brand = (friend or {}).get("brandName") or (friend or {}).get("brand") or ""
        self._catch(page, KEY_HISTORY, self._on_history)
        self._catch(page, KEY_ENTER, self._on_enter)
        rows = self._item_rows(page)
        if not rows:
            self._wait_items(page, timeout_s=10)
            rows = self._item_rows(page)
        if not rows:
            raise ChatError("Boss 会话列表没有渲染出来（页面在加载或登录已失效），请先点「扫码登录」")
        idx = None
        if name:
            idx = next((i for i, nm, _ in rows if nm == name), None)
            if idx is None:
                idx = next(
                    (i for i, _, txt in rows if name in txt and (not brand or brand in txt)),
                    None,
                )
        if idx is None and brand:
            idx = next((i for i, _, txt in rows if brand in txt), None)
        if idx is None:
            raise ChatError(f"在 Boss 会话列表里找不到「{name or brand or '该 HR'}」，可能还没打过招呼")
        page.locator(SEL_ITEM).nth(idx).click(timeout=15_000)
        try:
            page.wait_for_selector(SEL_CONV, timeout=15_000)
        except Exception:
            pass
        # 等对话区真正渲染出来。点完立刻读会拿到空列表——前端还在请求历史消息
        self._poll(
            page,
            "document.querySelectorAll('.chat-conversation li').length > 0",
            timeout_ms=3000,
        )

    def _input_text(self, page: Any) -> str:
        try:
            return page.evaluate(
                "() => { const el = document.querySelector('#chat-input');"
                " return el ? el.innerText : ''; }"
            )
        except Exception:
            return ""

    def _dom_messages(self, page: Any) -> list[dict[str, Any]]:
        """读对话区 DOM 的每条气泡，用于校正「谁发的」。"""
        try:
            rows = page.evaluate(
                r"""() => [...document.querySelectorAll('.chat-conversation li')].map((el) => {
                    const cls = el.className || '';
                    const mine = /item-myself/.test(cls) ? true
                               : /item-friend/.test(cls) ? false : null;
                    return { from_me: mine };
                })"""
            )
        except Exception:
            return []
        return [{"from_me": r.get("from_me")} for r in (rows or [])]

    # ---- 对外接口

    def friends(self, ctx: Any) -> list[dict[str, Any]]:
        """会话列表。每次调用都重新去接口拉一次，避免显示过期数据。"""
        page = self.chat_page(ctx)
        self._friends_raw.clear()
        self._friends_order = []
        # reload 才能真正触发一次 getGeekFriendList；单次点击可能拿不到
        try:
            page.reload(wait_until="domcontentloaded", timeout=45_000)
        except Exception as exc:
            self.log("warn", f"刷新聊天页失败：{exc}")
        t0 = time.time()
        while time.time() - t0 < 25.0:
            self._drain(KEY_FRIENDS)
            if self._friends_order:
                break
            self._wait_items(page, timeout_s=1.0)
        if not self._friends_order:
            # 接口没回来时退回 DOM：至少让用户看到有哪些人，别整块空白
            rows = self._item_details(page)
            if not rows:
                raise ChatError("没拿到会话列表：可能未登录，或 Boss 页面结构已变化")
            self.log("warn", "会话列表接口未返回，改用页面 DOM 兜底（部分字段缺失）")
            out = []
            for r in rows:
                # 整行文本兜底「最近一条消息」：Boss 的行 dom 结构常变，
                # 选择器全落空时至少还有整行文字，不至于副行全空。
                last = r["last_msg"] or r["txt"] or ""
                out.append(
                    {
                        "uid": f"dom:{r['i']}",
                        "name": r["name"] or r["txt"][:20],
                        "title": r["title"] or None,
                        "company": r["company"] or None,
                        "last_msg": last or None,
                        "last_time": r["last_time"] or None,
                        # DOM 兜底取不到可信时间戳：留空，让库里的时间不被抹掉。
                        "last_ts": None,
                        "unread": self._int_or_zero(r["unread"]),
                        "avatar": None,
                    }
                )
            return out
        items = [normalize_friend(self._friends_raw[u]) for u in self._friends_order]
        return [
            {
                "uid": f["uid"],
                "name": f["name"],
                "company": f["brand"] or None,
                "title": f["title"] or None,
                "last_msg": f["last_msg"] or None,
                "last_time": f["last_time"] or None,
                # 接口路径带 `lastTS`（毫秒）：会话列表靠它判新旧（用户 m01539）。
                "last_ts": self._int_or_zero(f["last_ts"]) or None,
                "unread": f["unread"],
                "avatar": f["avatar"] or None,
            }
            for f in items
        ]

    def messages(self, ctx: Any, uid: str) -> list[dict[str, Any]]:
        """与某个 HR 的历史消息。"""
        page = self.chat_page(ctx)
        friend = self._friends_raw.get(uid)
        if friend is None:
            self.friends(ctx)
            friend = self._friends_raw.get(uid)
        if friend is None:
            raise ChatError("在 Boss 会话列表里找不到这条会话，可能还没打过招呼")
        self._msgs_raw.pop(uid, None)
        self._open_conversation(page, friend)
        t0 = time.time()
        while time.time() - t0 < 12.0:
            self._drain(KEY_HISTORY)
            self._drain(KEY_ENTER)
            if uid in self._msgs_raw:
                break
            page.wait_for_timeout(200)
        raw = self._msgs_raw.get(uid)
        if raw is None:
            self.log("warn", "历史消息接口未返回，改用页面 DOM 兜底")
            return []
        msgs = [normalize_message(m, self._my_id) for m in raw]
        msgs = merge_dom_status(msgs, self._dom_messages(page))
        return strip_internal(msgs)

    def send(self, ctx: Any, uid: str, text: str) -> str:
        """给某个 HR 发一条消息（DOM 输入 + 点发送，和真人操作一致）。返回实际发出的文本。"""
        text = (text or "").strip()
        if not text:
            raise ChatError("消息内容为空")
        if len(text) > 1000:
            text = text[:1000]
        page = self.chat_page(ctx)
        friend = self._friends_raw.get(uid)
        if friend is None:
            self.friends(ctx)
            friend = self._friends_raw.get(uid)
        if friend is None:
            raise ChatError("在 Boss 会话列表里找不到这条会话，可能还没打过招呼")
        self._open_conversation(page, friend)
        # 输入框是 contenteditable div：先点聚焦，再用键盘输入（受控组件只有真实按键
        # 才会置空发送按钮的 disabled 态），最后清掉可能残留的草稿。
        box = page.locator(SEL_INPUT).first
        box.wait_for(state="visible", timeout=15_000)
        box.click()
        page.wait_for_timeout(120)
        page.keyboard.press("Control+A")
        page.keyboard.press("Backspace")
        page.keyboard.type(text, delay=15)
        page.wait_for_timeout(300)
        if not self._input_text(page).strip():
            raise ChatError("消息没有输入到聊天框（Boss 输入框可能没加载好），请重试")
        try:
            page.locator(SEL_SEND).first.click(timeout=8_000)
        except Exception:
            page.keyboard.press("Enter")  # 兜底：回车发送
        # 等输入框被清空（= 前端已接单）
        self._poll(
            page,
            "!document.querySelector('#chat-input')"
            " || !document.querySelector('#chat-input').innerText.trim()",
            timeout_ms=1500,
        )
        if self._input_text(page).strip():
            raise ChatError("消息没有发出去（输入框内容还在），可能是账号被限制或网络异常")
        self._msgs_raw.pop(uid, None)
        name = (friend or {}).get("name") or uid
        self.log("info", f"已发送消息给 {name}：{text[:40]}")
        return text

    def resumes(self, ctx: Any, uid: str | None = None) -> list[dict[str, Any]]:
        """当前账号可发送的简历列表。"""
        page = self.chat_page(ctx)
        rows: list[dict[str, Any]] = []

        def _on_resume(raw: str) -> None:
            import json

            data = json.loads(raw)
            zp = data.get("zpData") or {}
            for r in zp.get("resumeList") or zp.get("list") or []:
                rows.append(
                    {
                        "resume_id": str(r.get("resumeId") or r.get("id") or ""),
                        "name": r.get("resumeName") or r.get("name") or "我的简历",
                        "updated_at": r.get("updateTime") or r.get("modifiedTime"),
                    }
                )

        self._catch(page, KEY_RESUME, _on_resume)
        # 打开简历面板才会触发这个接口：先进任意会话，再点「发送简历」
        if uid and uid in self._friends_raw:
            try:
                self._open_conversation(page, self._friends_raw[uid])
                page.locator(".btn-send-resume, .toolbar-resume, [class*='resume']").first.click(
                    timeout=5_000
                )
            except Exception:
                pass
        t0 = time.time()
        while time.time() - t0 < 8.0:
            self._drain(KEY_RESUME)
            if rows:
                break
            page.wait_for_timeout(200)
        return rows

    def send_resume(self, ctx: Any, uid: str, resume_id: str) -> bool:
        """发送一份简历。走 DOM 选择列表里的对应项，与真人操作一致。"""
        page = self.chat_page(ctx)
        friend = self._friends_raw.get(uid)
        if friend is None:
            self.friends(ctx)
            friend = self._friends_raw.get(uid)
        if friend is None:
            raise ChatError("在 Boss 会话列表里找不到这条会话")
        self._open_conversation(page, friend)
        try:
            page.locator(".btn-send-resume, .toolbar-resume, [class*='resume']").first.click(
                timeout=8_000
            )
            page.wait_for_timeout(400)
            page.locator(f"[data-resume-id='{resume_id}'], [data-id='{resume_id}']").first.click(
                timeout=8_000
            )
        except Exception as exc:
            raise ChatError(f"发送简历失败：{type(exc).__name__}") from exc
        self.log("info", f"已发送简历给 {(friend or {}).get('name') or uid}")
        return True

    def recall(self, ctx: Any, uid: str) -> bool:
        """撤回最近一条自己发的消息：右键气泡 → 撤回。"""
        page = self.chat_page(ctx)
        friend = self._friends_raw.get(uid)
        if friend is None:
            self.friends(ctx)
            friend = self._friends_raw.get(uid)
        if friend is None:
            raise ChatError("在 Boss 会话列表里找不到这条会话")
        self._open_conversation(page, friend)
        try:
            mine = page.locator(".chat-conversation li.item-myself, .chat-conversation li.item-mine")
            last = mine.last
            last.click(button="right", timeout=8_000)
            page.wait_for_timeout(300)
            page.locator("text=撤回").first.click(timeout=5_000)
        except Exception as exc:
            raise ChatError(f"撤回失败：{type(exc).__name__}（可能已超过撤回时限）") from exc
        self.log("info", "已撤回最近一条消息")
        return True
