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
JOB_DETAIL_URL = "https://www.zhipin.com/job_detail/{job_id}.html"

# 页面元素。**不要凭直觉改**：这些是 2026-11 实测的选择器，
# Boss 换了 class 名会让它们整片失效，症状是「列表空 / 点不动」。
SEL_ITEM = ".user-list-content ul li"
SEL_CONV = ".chat-conversation"
SEL_INPUT = "#chat-input"
SEL_SEND = ".chat-op .btn-send, button.btn-send"
# 岗位详情页的「立即沟通」。实测是 <a class="btn btn-startchat" href="javascript:;">，
# 外面还套着 .btn-startchat-wrap / .btn-container（文本「感兴趣 立即沟通」），
# 只有这个 a 是唯一可见可点的，用 :has-text() 会同时命中外层 div。
SEL_START_CHAT = "a.btn-startchat"

# 接口关键字。命中即记下响应，正文留到主流程读。
KEY_FRIENDS = "getGeekFriendList"
KEY_HISTORY = "historyMsg"
KEY_ENTER = "geekEnter"
KEY_RESUME = "attachment/checkbox"

# 「立即沟通」之后等页面跳到聊天页的上限。实测 3 秒内完成。
ENSURE_WAIT_S = 10.0

# 会话列表：reload 之后等 `getGeekFriendList` 的上限。实测 reload 首帧约 0.7s，
# 8s 是给慢网络/慢接口留的余量；收到即 break，不会等满。
FRIENDS_RELOAD_WAIT_S = 8.0

# 历史消息：点开会话之后等 `historyMsg` 的上限，同样收到即 break。
MESSAGES_REOPEN_WAIT_S = 8.0

# 等待循环的节拍。**只用来让出线程给 playwright 事件泵**，不做任何 DOM 查询——
# 以前这里调 `_wait_items`（每轮 40 行 × 7 个 querySelector + innerText），
# 返回值被丢弃、纯属给每次刷新加开销，已删（用户 m00585）。
WAIT_TICK_MS = 200

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
        # 最后一条是谁发的：看 `lastMessageInfo.fromId`，不猜。
        # 实测（100 条真实会话）：`fromId == 我` 的 66 条、`fromId == 对方` 的 34 条，
        # 与 DOM 上「[送达]/[已读] 前缀只出现在我发的最后一条」完全吻合。
        "last_from_me": _last_from_me(raw, info),
        # 我发的最后一条的送达态：1=已送达、2=已读；0/缺省=对方发的（不显示）。
        # 实测 status ∈ {0,1,2,None}，与 DOM 前缀一一对应（1→[送达]、2→[已读]）。
        "last_status": _int_or_zero(info.get("status")),
        "encrypt_job_id": raw.get("encryptJobId") or "",
        "job_id": raw.get("jobId"),
    }


def _int_or_zero(v: Any) -> int:
    try:
        return int(v)
    except (TypeError, ValueError):
        return 0


def _on_chat_page(page: Any) -> bool:
    """页面当前是不是停在聊天页。

    `ensure()` 点完「立即沟通」后页面就在这条会话上，即便列表里还没有对应的一行，
    也能直接等 `historyMsg` 回来。用它区分「不用点」和「真的不在聊天页」。
    """
    try:
        return "/web/geek/chat" in (page.url or "")
    except Exception:
        return False


def _goto_landed(page: Any, url: str) -> bool:
    """导航报错，但页面其实已经到了要去的地方（页面自己跳过去了）。

    报错文本区分不出来「被别的文档顶掉」和「目标页已经打开」——两者都是
    `net::ERR_ABORTED`，只能看真实地址。但光看地址会把**浏览器已关**误判成
    成功：那时 `page.url` 不抛错，仍返回上一次的地址（离线实测踩过一次）。
    所以先看页面是否还活着，再**实际探一次**（读一个确实存在的节点），
    关着的页面在这里会抛错。
    """
    try:
        if page.is_closed() or not page.query_selector("body"):
            return False
        cur = page.url or ""
    except Exception:
        return False
    return _same_page(url, cur)


def _same_page(url: str, cur: str) -> bool:
    """两个地址是不是同一类页面（聊天页 / 岗位详情页）。"""
    if not url or not cur:
        return False
    for prefix in ("/web/geek/chat", "job_detail"):
        if prefix in url and prefix in cur:
            return True
    return False


def _browser_gone(exc: Exception) -> bool:
    """这条异常是不是「页面/浏览器已经没了」。"""
    text = str(exc)
    return "Target page, context or browser has been closed" in text or "has been closed" in text


def _query_param(url: str, name: str) -> str:
    """从 URL 查询串里取一个参数。取不到返回空串。

    「立即沟通」跳转后的地址是 `/web/geek/chat?id=…&jobId=…&securityId=…`，
    其中 `id` 就是这条会话的 uid（实测与 `getBossData?bossId=` 一致）。
    不用 `urllib.parse` 只为了一个参数，这里 split 足够且没有转义问题。
    """
    _, _, query = (url or "").partition("?")
    for pair in query.split("&"):
        key, _, value = pair.partition("=")
        if key == name:
            return value
    return ""


def _last_from_me(raw: dict[str, Any], info: dict[str, Any]) -> bool:
    """最后一条消息是不是我发的。

    `lastMessageInfo.fromId` 是数字 uid，`toId` 是对方；`fromId` 缺省时（8/100 条）
    退回 `lastMsg` 与对方名字比对——**不**拿 `status` 推方向，它是送达态不是方向。
    """
    from_id = info.get("fromId")
    if from_id is None:
        return False
    peer = raw.get("uid")
    try:
        return int(from_id) != int(peer)
    except (TypeError, ValueError):
        return str(from_id) != str(peer)


def _member_uid(v: Any) -> str:
    """historyMsg 里 `from` / `to` 是成员对象 `{uid, name, avatar, ...}`，不是数字。

    实测（三个不同会话）：`from={uid:60799293,name:'容女士'}`、
    `to={uid:520761101,name:'邱先生'}`。兼容直接给数字/字符串的老结构。
    """
    if isinstance(v, dict):
        u = v.get("uid")
        return str(u) if u is not None else ""
    return str(v) if v is not None else ""


def normalize_message(raw: dict[str, Any], my_id: str | None) -> dict[str, Any]:
    """把 historyMsg 的一条整理成 {sender, content, time, msg_id}。

    `sender` 只取 `me` / `hr` / `system` —— 前端的渲染分支只认这三个值。
    """
    body = raw.get("body") or {}
    mid = raw.get("mid") or raw.get("msgId") or 0
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

    # 「是不是我发的」只看发件人：`from.uid` 等于自己的 uid 就是我发的。
    # **不能用 `received`**：实测 11 条消息 `received` 恒为 true（那是送达/已读
    # 状态），与发件人无关；按它判断会把「我发的」全判成 hr（用户实测左右颠倒）。
    # 拿不到发件人或还不知道自己是谁时留 None，交给 DOM 的 item-myself 覆盖。
    src_id = None
    for candidate in (raw.get("from"), body.get("fromId"), body.get("from")):
        if candidate is not None:
            src_id = candidate
            break
    src = _member_uid(src_id)
    from_me: bool | None
    if src and my_id:
        from_me = src == str(my_id)
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
        # key → 累计回过几次（含只记账、不入队的监听）。导航被打断时正文取不到，
        # 这是「接口确实已经回来过」的唯一凭据。
        self._drained: dict[str, int] = {}
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

        `handler` 为 `None` 时只记账（`_drained`），由调用方拿 `_drain` 的
        返回值为「接口已回过」作证——导航被打断时收不到正文，只能这样确认。
        """

        def _on_response(resp: Any) -> None:
            try:
                if key not in resp.url or resp.status != 200:
                    return
                with self._sig_lock:
                    self._drained[key] = self._drained.get(key, 0) + 1
                    if handler is not None:
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
            seen = self._drained.get(key, 0)
        n = 0
        for resp, handler in items:
            try:
                handler(resp.text())
                n += 1
            except Exception as exc:
                self.log("warn", f"读取 {key} 响应失败：{type(exc).__name__}: {exc}")
        # 正文读失败的也算「接口回来过」：返回 seen 让等待方不再空转到超时。
        return max(n, seen)

    # ---- 回调

    def _on_friends(self, raw: str) -> None:
        import json

        data = json.loads(raw)
        zp = data.get("zpData") or {}
        # 真实结构的列表键只有一个：zpData.result（实测 10:49:23，
        # {"code":0,"message":"Success","zpData":{"result":[…"securityId":…,"name":…]}}）
        rows = zp.get("result") or zp.get("geekFriendList") or zp.get("friendList") or []
        order: list[str] = []
        for r in rows:
            uid = str(r.get("uid") or "")
            if not uid:
                continue
            self._friends_raw[uid] = r
            order.append(uid)
        self._friends_order = order

    def _on_history(self, raw: str) -> None:
        """未绑定会话的兜底 handler：只认响应自带的 uid（实测没有），正常路径不用它。"""
        import json

        data = json.loads(raw)
        zp = data.get("zpData") or {}
        msgs = zp.get("messages") or zp.get("msgList") or []
        uid = str(zp.get("encryptUid") or zp.get("uid") or "")
        if uid:
            self._msgs_raw[uid] = [m for m in msgs if isinstance(m, dict)]

    def _history_handler(self, uid: str) -> Callable[[str], None]:
        """生成绑定到某个会话的 historyMsg handler。

        响应里**没有 uid**（实测 zpData 只有 hasMore/messages/type/minMsgId），
        靠响应自带的 uid 会让 `_msgs_raw` 永远存不进去——uid 只能由调用方给。
        """

        def handler(raw: str) -> None:
            import json

            data = json.loads(raw)
            zp = data.get("zpData") or {}
            msgs = zp.get("messages") or zp.get("msgList") or []
            self._msgs_raw[uid] = [m for m in msgs if isinstance(m, dict)]

        return handler

    def _on_enter(self, raw: str) -> None:
        import json

        data = json.loads(raw)
        zp = data.get("zpData") or {}
        me = zp.get("myId") or zp.get("uid") or zp.get("userId")
        if me is not None:
            self._my_id = str(me)

    def _note_me(self, msgs: list[dict[str, Any]], peer_uid: str) -> None:
        """从 historyMsg 反推「我是谁」。geekEnter 不返回 myId（实测 zp_keys 里没有）。

        `from` / `to` 是成员对象，`peer_uid` 是这条会话的对端（调用方给的，必然正确）：
        发件人不是对端 → 我是收件人 `to.uid`。取到一次就够用。

        **不能用 `received`**：实测 11 条消息它恒为 true（送达/已读状态），
        按它取 `to.uid` 会在「我发的」那条上把对端当成自己。
        """
        if self._my_id:
            return
        peer = str(peer_uid or "")
        if not peer:
            return
        for m in msgs:
            if not isinstance(m, dict):
                continue
            if _member_uid(m.get("from")) == peer:
                me = _member_uid(m.get("to"))
                if me:
                    self._my_id = me
                    return

    def _goto(self, page: Any, url: str, what: str) -> None:
        """导航到 url，并把导航失败翻译成能照做的提示。

        真实故障：加载慢时页面里**自己**的脚本先跳走，Playwright 报
        `Page.goto: net::ERR_ABORTED at <url>`。这条原始文案占了半个界面还不说
        该怎么办，用户看到的就是「程序无响应」。导航一旦被新文档顶掉，旧文档的
        请求全部作废，重试一次是唯一有效的动作（只重试一次，第二次失败照实报）。
        """
        last: Exception | None = None
        for _ in range(2):
            try:
                page.goto(url, wait_until="domcontentloaded", timeout=45_000)
                return
            except Exception as exc:
                if _browser_gone(exc):
                    raise ChatError(
                        f"{what} 打不开：Boss 浏览器已关闭或连接断了。"
                        "先点右上角「扫码登录」把浏览器拉起来，再点「刷新」。"
                    ) from exc
                # 页面已经跳到目标页（被自己的脚本顶掉那一次，地址就已经对了）
                # 不算失败。地址还停在旧页才重试，重试仍失败照实报。
                if _goto_landed(page, url):
                    return
                last = exc
                if "ERR_ABORTED" not in str(exc):
                    break
                self.log("warn", f"{what} 导航被页面自身跳转打断，重试一次")
                page.wait_for_timeout(WAIT_TICK_MS)
        detail = str(last)
        if "ERR_ABORTED" in detail:
            raise ChatError(
                f"{what} 打不开：页面在加载途中自己跳转走了（网络慢或被风控拦了一次），"
                "重试一次仍失败，请稍后再点「刷新」"
            ) from last
        # 消息必须**只有一行**：前端把 errText 整段塞进提示条，Playwright 的
        # 多行 Call log（navigating to … waiting until …）会把界面撑成两大块。
        first = detail.strip().splitlines()[0] if detail.strip() else "未知原因"
        raise ChatError(f"{what} 打不开：{first}") from last

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
            self._goto(page, CHAT_URL, "Boss 聊天页")
        self._page = page
        # KEY_FRIENDS 只记账不入队：`friends()` 会在 reload 之后重新挂上真正解析
        # 正文的监听；这里若也入队，reload 后旧文档的响应只能读出 `net::ERR_ABORTED`，
        # 白白往日志里灌一条「读取 getGeekFriendList 响应失败」。
        self._catch(page, KEY_FRIENDS, None)
        self._catch(page, KEY_HISTORY, self._on_history)
        self._catch(page, KEY_ENTER, self._on_enter)
        return page

    def _item_rows(self, page: Any) -> list[tuple[int, str, str]]:
        """读出会话列表每一行的 (索引, HR 名字, 整行文本)。

        用于「按名字/公司定位该点哪一行」（`_open_conversation`）。要拿字段请用
        `_item_details`——这里只有名字，公司/岗位/时间都在 `innerText` 里没拆开。
        """
        return [(r["i"], r["name"], r["txt"]) for r in self._item_details(page)]

    def _item_details(self, page: Any) -> list[dict[str, Any]]:
        """逐行解析会话列表，尽量补齐字段。

        只有 `_item_rows`（定位该点哪一行）在用；列表数据一律走接口
        `zpData.result`。DOM 行是虚拟滚动渲染的（40 行 vs 接口 100 条），
        且拿不到 uid，撑不起会话列表。

        选择器名称以实测 DOM 为准（`.name-text` / `.last-msg-text` / `.time`）：
        旧写法 `.name` / `.msg-text` 全部落空，`name` 恒为空串，导致
        `_open_conversation` 只能靠公司名子串猜行、猜不中就报「找不到这条会话」。
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
                            name: pick(el, ['.name-text', '.name-box', '.name', '.geek-name']),
                            company: pick(el, ['.company-name', '.brand-name', '.company']),
                            title: pick(el, ['.job-name', '.position', '.geek-job']),
                            last_msg: pick(el, ['.last-msg-text', '.last-msg', '.msg-text', '.lastMessage']),
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

    def _open_conversation(self, page: Any, friend: dict[str, Any], uid: str = "") -> bool:
        """点开会话。会话项文本在深层子节点里，locator.filter(has_text=) 会超时，
        所以先一次性 evaluate 读出所有条目，再按索引点。

        `uid` 用于把 historyMsg 绑定到这条会话（响应里没有 uid，见 `_history_handler`）。

        返回**是否真的点了行**。列表里没有这一行时返回 `False` 而不抛错——
        调用方据此判断「是不是根本不用点」（`ensure()` 刚建好的会话，页面本来就停在
        这条会话上，而 `getGeekFriendList` 有延迟、列表里可能还没有它）。
        `friend` 为空时也返回 `False`，由调用方决定要不要报错。
        """
        if not friend:
            return False
        name = (friend or {}).get("name") or ""
        brand = (friend or {}).get("brandName") or (friend or {}).get("brand") or ""
        self._catch(page, KEY_HISTORY, self._history_handler(uid) if uid else self._on_history)
        self._catch(page, KEY_ENTER, self._on_enter)
        rows = self._item_rows(page)
        if not rows:
            # 列表还没渲染（页面在加载）：等选择器，不要空转轮询 DOM。
            try:
                page.wait_for_selector(SEL_ITEM, timeout=10_000)
            except Exception:
                pass
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
            return False
        page.locator(SEL_ITEM).nth(idx).click(timeout=15_000)
        try:
            page.wait_for_selector(SEL_CONV, timeout=15_000)
        except Exception as exc:
            # 不吞：对话区没出来时后面一定读不到消息，留日志便于排查
            self.log("warn", f"点开会话后 {SEL_CONV} 未出现：{type(exc).__name__}")
        # 等对话区真正渲染出来。点完立刻读会拿到空列表——前端还在请求历史消息
        self._poll(
            page,
            "document.querySelectorAll('.chat-conversation li').length > 0",
            timeout_ms=3000,
        )
        return True

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
        """会话列表：reload 一次触发 `getGeekFriendList`，等它回来（收到即 break）。

        为什么不「先挂着等推送」：实测（2026-11 探针）不 reload 时确实偶尔能收到
        推送触发的接口，但**推不推不可控**——没有新消息时可能一次都不来，那就变成
        每次刷新白等 3 秒。用户判定为「毫无用处、纯加开销的兜底」（m00585），已删。

        reload 本身并不慢（首帧实测 ~0.7s），所以直接刷就是常态最快路径。
        """
        page = self.chat_page(ctx)
        self._friends_raw.clear()
        self._friends_order = []
        try:
            page.reload(wait_until="domcontentloaded", timeout=45_000)
        except Exception as exc:
            # 导航被页面自身跳转打断时，等待循环立刻用 `_drained` 记账结束，
            # 再给一次机会（reload 本来就要重来一遍，空口报错比慢更糟）。
            page.wait_for_timeout(WAIT_TICK_MS)
            try:
                page.reload(wait_until="domcontentloaded", timeout=45_000)
            except Exception as exc2:
                self.log("warn", f"刷新聊天页失败：{str(exc2).splitlines()[0]}")
                _ = exc
        got = self._wait_friends(page, FRIENDS_RELOAD_WAIT_S)
        if not got:
            self.log("warn", "刷新后仍未拿到会话列表接口")
        return self._friends_out(page)

    def _wait_friends(self, page: Any, timeout_s: float) -> bool:
        """等 `getGeekFriendList` 填进 `_friends_order`。返回是否拿到。

        **响应已处理就立刻返回，不等满超时**：`_drain` 已经跑过 handler，
        `_friends_order` 仍为空说明解析不出列表（字段名变了等），继续空转到
        超时只是白白多等几秒——实测接口 1s 就回、这里空转到 8.2s（用户报卡住）。
        """
        t0 = time.time()
        _n_drain = 0
        while time.time() - t0 < timeout_s:
            _n_drain += self._drain(KEY_FRIENDS)
            if self._friends_order:
                return True
            if _n_drain > 0:
                return False
            page.wait_for_timeout(WAIT_TICK_MS)
        return bool(self._friends_order)

    def _friends_out(self, page: Any) -> list[dict[str, Any]]:
        if not self._friends_order:
            raise ChatError(
                "没拿到 Boss 会话列表：可能未登录，或 Boss 接口结构已变化（详见日志）"
            )
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
                # 最后一条的归属与送达态：前端据此给副行上色（用户 m01427）。
                "last_from_me": f["last_from_me"],
                "last_status": f["last_status"],
                "avatar": f["avatar"] or None,
            }
            for f in items
        ]

    def ensure(self, ctx: Any, job_id: str) -> dict[str, Any]:
        """在「还没打过招呼」的岗位上建立会话，返回这条会话的真实 uid。

        走的是什么：打开岗位详情页 → 点「立即沟通」→ Boss 服务端建会话，页面**自己**
        跳到 `/web/geek/chat`。跳转后的 URL 里带着这次新建的会话标识：

            /web/geek/chat?id=<HR 的加密 uid>&jobId=<岗位>&securityId=…

        这个 `id` 与页面随即发出的 `getBossData?bossId=<同一个值>` / `historyMsg?bossId=<同一个值>`
        完全一致，就是 `friends()` 里每条的 `uid`。所以**点完就地读 URL** 就够，
        不需要前后拉两次列表求差集（用户 m01981 明确否掉：那是屎山）。

        只建会话，不发任何消息：话术仍由调用方随后走 `send()`。
        """
        job = str(job_id or "").strip()
        # 库里的 job_id 形如 `boss:577f3c…`，详情页只要后半段。
        if job.startswith("boss:"):
            job = job[len("boss:"):]
        if not job:
            raise ChatError("缺少 jobId，无法建立会话")

        page = self.chat_page(ctx)
        url = JOB_DETAIL_URL.format(job_id=job)
        self.log("info", f"打开岗位详情页建立会话：{job}")
        self._goto(page, url, "岗位详情页")
        # `domcontentloaded` 时按钮还没被前端框架挂上（实测要等页面渲染完）：
        # 直接 click 会在 10 秒里一直等不到元素。先等选择器出现。
        try:
            page.wait_for_selector(SEL_START_CHAT, timeout=15_000)
        except Exception as exc:
            # 岗位已关闭/已投递时页面确实没有这个按钮，这不是脚本故障。
            raise ChatError(
                "岗位页没有「立即沟通」按钮（职位可能已关闭，或你已与该 Boss 建立过会话）"
            ) from exc
        page.locator(SEL_START_CHAT).first.click(timeout=10_000)

        # 等页面自己跳到聊天页（实测 3 秒内）。不轮询 DOM，只看 URL。
        t0 = time.time()
        while time.time() - t0 < ENSURE_WAIT_S:
            if "/web/geek/chat" in (page.url or ""):
                break
            page.wait_for_timeout(WAIT_TICK_MS)
        cur = page.url or ""
        if "/web/geek/chat" not in cur:
            raise ChatError("点了「立即沟通」但页面没有进入聊天页（可能触发了验证或风控）")

        uid = _query_param(cur, "id")
        if not uid:
            raise ChatError("聊天页地址里没有会话 id，无法定位刚建立的会话")
        # 刚建好会话的这条页就是聊天页本身，登记下来供后续 messages()/send() 复用。
        self._page = page
        self._catch(page, KEY_FRIENDS, self._on_friends)
        self._catch(page, KEY_HISTORY, self._on_history)
        self._catch(page, KEY_ENTER, self._on_enter)
        self.log("info", f"会话已建立 uid={uid}")
        return {"uid": uid, "job_id": job, "url": cur}

    def messages(self, ctx: Any, uid: str) -> list[dict[str, Any]]:
        """与某个 HR 的历史消息：点开会话触发 `historyMsg`，等它回来（收到即 break）。

        同 `friends()` 的理由：点开前先挂着等推送是不可控的等待，没推送就白等
        （用户 m00585 已删）。直接点开是确定的动作，且点开本身本来就要做。
        """
        page = self.chat_page(ctx)
        friend = self._friends_raw.get(uid)
        if friend is None:
            self.friends(ctx)
            friend = self._friends_raw.get(uid)
        if friend is None:
            # 查不到不等于错：刚由 `ensure()` 建好的会话还没进 `getGeekFriendList`。
            # 交给 `_open_conversation` 判——列表里没有可点的行时它会返回 False，
            # 那时页面本来就已经停在这条会话上，不需要再点。
            friend = {}
        self._msgs_raw.pop(uid, None)
        # 挂信号与点开都在 `_open_conversation` 里做：historyMsg 的 handler 必须绑定
        # 本次的 uid（响应本身不带 uid）。
        #
        # 刚用 `ensure()` 建好的会话可能还没进 `_friends_raw`（`getGeekFriendList`
        # 有延迟）。这时页面已经停在聊天页，对话区本来就是这条会话，**不用再点一次**——
        # 直接挂 historyMsg 监听等它回来即可。
        if not self._open_conversation(page, friend, uid):
            if not _on_chat_page(page):
                raise ChatError(
                    "在 Boss 会话列表里找不到这条会话（会话列表可能未加载成功，点「刷新」重试）"
                )
            self._catch(page, KEY_HISTORY, self._history_handler(uid))
            self._catch(page, KEY_ENTER, self._on_enter)
        got = self._wait_messages(page, uid, MESSAGES_REOPEN_WAIT_S)
        raw = self._msgs_raw.get(uid)
        if raw is None:
            self.log("warn", f"历史消息接口未返回（got={got}），该会话消息为空")
            return []
        self._note_me(raw, uid)
        msgs = [normalize_message(m, self._my_id) for m in raw]
        msgs = merge_dom_status(msgs, self._dom_messages(page))
        return strip_internal(msgs)

    def _wait_messages(self, page: Any, uid: str, timeout_s: float) -> bool:
        """等 `historyMsg` 填进 `_msgs_raw[uid]`。返回是否拿到。

        同 `_wait_friends`：`historyMsg` 已处理过但仍没写进这个 uid，说明响应
        回来了却解析不出消息，继续空转到超时只是白等。

        **提前返回只看 `KEY_HISTORY`，不能把 `geekEnter` 算进去**：实测点开会话
        时 `geekEnter` 先回、`historyMsg` 约 1.5s 后才回，把 enter 计入
        `_n_drain` 会让本函数在 0.08s 就判「没有消息」，用户看到的就是空会话。
        """
        t0 = time.time()
        _n_drain = 0
        while time.time() - t0 < timeout_s:
            _n_drain += self._drain(KEY_HISTORY)
            # enter 只负责补 myId，与「本条会话的消息到没到」无关
            self._drain(KEY_ENTER)
            if uid in self._msgs_raw:
                return True
            if _n_drain > 0:
                return False
            page.wait_for_timeout(WAIT_TICK_MS)
        return uid in self._msgs_raw

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
            # 同 `messages()`：可能是 `ensure()` 刚建好、列表还没刷出来。页面若已在
            # 聊天页就直接发；否则说明这条会话真不存在。
            if not _on_chat_page(page):
                raise ChatError(
                    "在 Boss 会话列表里找不到这条会话（会话列表可能未加载成功，点「刷新」重试）"
                )
            friend = {}
        self._open_conversation(page, friend, uid)
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
                self._open_conversation(page, self._friends_raw[uid], uid)
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
            raise ChatError("在 Boss 会话列表里找不到这条会话（尚未与该 Boss 打过招呼）")
        self._open_conversation(page, friend, uid)
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
            raise ChatError("在 Boss 会话列表里找不到这条会话（尚未与该 Boss 打过招呼）")
        self._open_conversation(page, friend, uid)
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
