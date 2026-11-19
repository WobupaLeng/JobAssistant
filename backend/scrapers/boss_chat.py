"""Boss 直聘消息模块：会话列表、历史消息、发送消息。

思路：不做纯 HTTP 转发（Boss 的 wapi 带 zp_token/wt 签名，硬算会触发风控），
而是驱动**同一个** patchright 持久化上下文里的聊天页，用接口响应解析数据、
用 DOM 完成发送。这样 cookie / 指纹 / 风控环境都和用户手动操作一模一样。

所有方法都必须在「采集线程」里调用（patchright sync API 绑定线程），
对外由 main.py 的 Runtime.run_in_worker 投递。

线上实测到的结构（2026-11）：
- 会话列表 GET  /wapi/zprelation/friend/getGeekFriendList.json
    返回 zpData.result 是**数组**，每条含 securityId / encryptBossId / uid /
    encryptJobId / name / title / brandName / lastMsg / lastTime / unreadMsgCount ...
- 历史消息 GET  /wapi/zpchat/geek/historyMsg?bossId=&securityId=&maxMsgId=&c=20&page=1&src=0
    返回 zpData.messages（数组，received=true 表示对方发的）
- 进入会话     POST /wapi/zpchat/session/geekEnter
- DOM：会话项 `.user-list-content ul li`；对话区 `.chat-conversation`；
    输入框 `textarea.input`；发送按钮 `.btn-send`。
"""
from __future__ import annotations

import html
import json
import threading
import time
from typing import Any, Callable

CHAT_URL = "https://www.zhipin.com/web/geek/chat"

# 会话列表项 / 对话区 / 输入框 / 发送按钮
#
# 注意：输入框是 contenteditable 的 div#chat-input，不是 textarea。
# 页面里确实有一个 <textarea class="input">，但它是 0×0 的隐藏镜像（用于
# 移动端/兜底），点它永远等不到 visible，会白等 30 秒。
SEL_ITEM = ".user-list-content ul li"
SEL_CONV = ".chat-conversation"
SEL_INPUT = "#chat-input"
SEL_SEND = ".chat-op .btn-send, button.btn-send"


class ChatError(RuntimeError):
    """消息模块的业务错误（界面可以直接展示 message）。"""


def _short(v: Any, n: int = 60) -> str:
    s = str(v if v is not None else "")
    return s if len(s) <= n else s[:n] + "…"


def normalize_friend(raw: dict[str, Any]) -> dict[str, Any]:
    """把 getGeekFriendList 的一条整理成界面要用的字段。"""
    info = raw.get("lastMessageInfo") or {}
    return {
        "uid": raw.get("uid"),
        "security_id": raw.get("securityId") or "",
        "encrypt_boss_id": raw.get("encryptBossId") or "",
        "encrypt_uid": raw.get("encryptUid") or "",
        "name": raw.get("name") or "",
        "title": raw.get("title") or "",
        "brand": raw.get("brandName") or "",
        "avatar": raw.get("avatar") or "",
        "last_msg": raw.get("lastMsg") or info.get("showText") or "",
        "last_time": raw.get("lastTime") or "",
        "last_ts": raw.get("lastTS") or 0,
        "unread": raw.get("unreadMsgCount") or 0,
        "encrypt_job_id": raw.get("encryptJobId") or "",
        "job_id": raw.get("jobId"),
        "from_id": info.get("fromId"),
        "to_id": info.get("toId"),
    }


def _fmt_time(ts: Any) -> str:
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


def _merge_status(msgs: list[dict[str, Any]], dom: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """把 DOM 上读到的回执（已读/送达）按 mid 合并进接口消息。"""
    if not dom or not msgs:
        return msgs
    by_mid = {str(d.get("mid") or d.get("id") or ""): d.get("status") or "" for d in dom}
    for m in msgs:
        if m.get("status"):
            continue
        k = str(m.get("mid") or m.get("id") or "")
        st = by_mid.get(k)
        if st:
            m["status"] = st
    return msgs


def _status_from_text(txt: str, cls: str = "") -> str:
    """把 Boss 气泡上的回执读成 read / delivered / unread。

    实测 2026-11：自己发的气泡里有
        <i class="message-status status-read">已读</i>
        <i class="message-status status-delivery"> 送达 </i>
    historyMsg 接口里没有可靠已读位（flag 恒为 0），所以回执只能从 DOM 取。
    """
    t = (txt or "").strip()
    c = cls or ""
    if "status-read" in c or t == "已读":
        return "read"
    if "status-delivery" in c or "送达" in t:
        return "delivered"
    if "未读" in t:
        return "unread"
    return ""


def normalize_message(raw: dict[str, Any], my_id: int | None) -> dict[str, Any]:
    """把 historyMsg 的一条整理成 {id, from_me, text, time, time_text, kind}。"""
    body = raw.get("body") or {}
    mid = raw.get("mid") or raw.get("msgId") or 0
    received = raw.get("received")
    from_me = (received is False) if received is not None else (
        bool(my_id) and body.get("fromId") == my_id
    )
    text = ""
    for key in ("text", "content", "showText", "desc"):
        v = body.get(key)
        if isinstance(v, str) and v.strip():
            # 卡片消息的 text 里常带换行，压成一行更好读
            text = " ".join(html.unescape(v).split())
            break
    # 注意：body.type 是模板类型（如正文里的 16 只是模板 id），外层 raw.type 才是消息类型
    try:
        btype = int(raw.get("type") or 0)
    except (TypeError, ValueError):
        btype = 0
    if not text:
        # 结构化消息（职位卡片、系统提示等）退化成一句说明
        if btype == 8:
            job = body.get("jobDesc") or {}
            text = "职位卡片" + (f"：{job.get('jobName')}" if job.get("jobName") else "")
        elif btype in (5, 6):
            text = "图片"
        elif btype in (3, 7):
            text = "附件简历"
        elif btype == 2:
            text = "语音"
        elif btype in (1, 4, 16, 17, 18, 19, 20, 21):
            text = ""          # 纯系统/时间线消息：不画气泡，只当时序处理
        else:
            text = "其他消息"
    ts = raw.get("msgTime") or raw.get("time") or body.get("msgTime") or 0
    ts_int = int(ts) if str(ts).isdigit() else 0
    # 没有正文的类型一律按系统行（居中细字），有正文的才是真正的对话气泡；
    # 正文只是类型兜底（「图片」「其他消息」这类）时也按系统行，避免误画成对方说的话
    from_text = any(isinstance(body.get(k), str) and body.get(k).strip() for k in ("text", "content", "showText", "desc"))
    system = (not from_text) or btype in (16, 17, 18, 19, 20, 21)
    return {
        "id": str(mid),
        "mid": mid,
        "from_me": bool(from_me),
        "text": text,
        "time": ts_int,
        "time_text": _fmt_time(ts_int),
        "kind": btype,
        "system": system,
        # 回执只能从 DOM 补，接口里没有
        "status": "",
    }


class _Activity:
    """记录「采集线程正在驱动浏览器」的时间戳 + 这次动作允许的最长耗时。

    外部看门狗（main.py 的 /api/chat/* 接口）用它区分三种情况：
      - 在等锁（另一个请求正在用浏览器）→ 继续等，不算卡死；
      - 在浏览器里操作但已经超过 budget → 判定卡死，让用户重启或重试；
      - 刚操作完 → 超时后不会误杀。
    """

    def __init__(self, chat: "BossChat", timeout: float) -> None:
        self._chat = chat
        self._timeout = timeout
        self._prev = 0.0

    def __enter__(self):
        self._prev = self._chat._budget
        self._chat._budget = self._timeout
        self._chat._touch()
        return self

    def __exit__(self, *exc) -> bool:
        self._chat._budget = self._prev
        self._chat._touch()
        return False


class BossChat:
    """在采集线程里驱动 Boss 聊天页。一个实例全局共用（Runtime 持有）。"""
    def __init__(self, log: Callable[[str, str], None]) -> None:
        self.log = log
        # 接口响应缓存：只要页面还在，就不用每次重新拉
        self._friends: list[dict[str, Any]] = []
        self._friends_at = 0.0
        self._msgs: dict[str, tuple[float, list[dict[str, Any]]]] = {}
        # Boss 上已上传的附件简历（点「发简历」才会拉一次）
        self._resumes: list[dict[str, Any]] = []
        self._resumes_at = 0.0
        self._my_id: int | None = None
        self._page = None
        # 会话列表先按 reload 前的页面读 —— reload 之后列表可能要好几秒才渲染，
        # 所以把「列表什么时候是空的」也记下来。
        self._items_seen_at = 0.0
        # 当前打开的会话 uid：historyMsg 响应不一定带 bossId，缺了就回填它，
        # 但不能凭空写进空键（以前会串到别的会话上，表现为点 A 显示 B 的记录）。
        self._open_uid = ""
        self._filler: Callable[[str, dict[str, Any]], Any] | None = None
        # 可重入 + 只记「最后一次活动时间」：外部看门狗据此判断是否卡死
        self._lock = threading.RLock()
        self._act_lock = threading.Lock()
        self._last_act = 0.0
        self._budget = 0.0

    def _touch(self) -> None:
        with self._act_lock:
            self._last_act = time.time()

    def activity(self) -> tuple[float, float]:
        """(最后一次活动时间, 当前动作允许的耗时上限)。"""
        with self._act_lock:
            return self._last_act, self._budget

    def _activity(self, timeout: float = 0.0):
        """上下文管理器：标记「正在干活」，让看门狗区分『卡在浏览器里』和『在等锁』。

        timeout 是这条命令自己允许的最长耗时（秒），0 表示看门狗按默认值判断。
        """
        return _Activity(self, timeout)

    # ---------------- 内部：页面与拦截 ----------------

    def set_filler(self, fn: Callable[[str, dict[str, Any]], Any] | None) -> None:
        """外挂能力：bump_job(uid, friend) / mark_delivered(friend)。

        这两件事要改数据库，数据库在 app 层，scrapers 不能反向 import，
        所以由 main.py 注入回调。
        """
        self._filler = fn

    def _chat_page(self, ctx):
        """返回聊天页；必要时从现有页面导航过去。"""
        if self._page is not None:
            try:
                if not self._page.is_closed():
                    return self._page
            except Exception:
                pass
            self._page = None
        page = None
        try:
            for p in ctx.pages:
                if "/web/geek/chat" in (p.url or ""):
                    page = p
                    break
        except Exception:
            page = None
        if page is None:
            try:
                page = ctx.pages[0] if ctx.pages else ctx.new_page()
            except Exception:
                page = ctx.new_page()
        self._page = page
        if "/web/geek/chat" not in (page.url or ""):
            self._goto_chat(page, ctx)
        return page

    def _goto_chat(self, page, ctx, reason: str = "") -> None:
        if reason:
            self.log("info", f"打开 Boss 消息页（{reason}）")
        self._catch(page, "getGeekFriendList", self._on_friends, "chat_init")
        self._catch(page, "historyMsg", self._on_history, "chat_init")
        try:
            page.goto(CHAT_URL, wait_until="domcontentloaded", timeout=45_000)
        except Exception as exc:
            raise ChatError(f"打开 Boss 消息页失败：{exc}") from exc
        self._wait_friends(page, ctx)
        # 拿自己的 uid：历史消息里 received 字段不完全可靠时用它兜底判断「我发的」
        try:
            me = page.evaluate(
                "() => (window.__VUE_DEVTOOLS_GLOBAL_HOOK__ && 0) || "
                "(() => { const t = document.body.innerText || ''; return t; })()"
            )
            del me
        except Exception:
            pass
        self._guess_my_id(page)

    def _guess_my_id(self, page) -> None:
        """从页面 cookie/接口里认领自己的 uid（失败也不影响主流程）。"""
        if self._my_id:
            return
        try:
            self._catch(page, "getUserInfo", self._on_me, "chat_init")
        except Exception:
            pass

    def _on_me(self, raw: str) -> None:
        try:
            data = json.loads(raw)
        except Exception:
            return
        zp = data.get("zpData") or {}
        uid = zp.get("userId") or zp.get("geekId") or zp.get("uid")
        if uid:
            try:
                self._my_id = int(uid)
            except (TypeError, ValueError):
                pass

    def _on_friends(self, raw: str) -> None:
        try:
            data = json.loads(raw)
        except Exception:
            return
        res = (data.get("zpData") or {}).get("result")
        if not isinstance(res, list):
            # 有的灰度返回 {result: {...}} 或直接 friendList
            res = (data.get("zpData") or {}).get("friendList") or []
        res = [f for f in res if isinstance(f, dict)]
        # 同一个会话可能因为「全部/N 个新招呼/新职位」分组重复出现，按 uid 去重
        uniq: dict[Any, dict[str, Any]] = {}
        for f in res:
            uniq.setdefault(f.get("uid"), f)
        self._friends = list(uniq.values())
        self._friends_at = time.time()

    def _on_history(self, raw: str) -> None:
        try:
            data = json.loads(raw)
        except Exception:
            return
        zp = data.get("zpData") or {}
        msgs = zp.get("messages") or zp.get("list") or []
        if not isinstance(msgs, list):
            return
        uid = str(zp.get("bossId") or zp.get("encryptBossId") or zp.get("uid") or "")
        if not uid:
            uid = self._open_uid
        if not uid:
            return
        self._msgs[uid] = (time.time(), msgs)

    @staticmethod
    def _catch(page, key: str, handler: Callable[[str], None], tag: str,
               on_fail: Callable[[str], None] | None = None) -> None:
        """给 page 挂一个响应监听：URL 含 key 时把响应体交给 handler。

        handler 里只做解析和存内存，不做任何等待，避免阻塞 patchright 事件流。
        """

        def _on_response(resp):
            try:
                if key not in resp.url:
                    return
                if resp.status != 200:
                    if on_fail:
                        on_fail(f"HTTP {resp.status} {resp.url}")
                    return
                handler(resp.text())
            except Exception as exc:
                if on_fail:
                    on_fail(f"{type(exc).__name__}: {exc}")
                self.log("warn", f"{tag} 响应解析失败：{type(exc).__name__}: {exc}") if hasattr(self, "log") else None

        page.on("response", _on_response)
        return _on_response

    def _wait_friends(self, page, ctx, timeout_s: float = 25.0) -> None:
        t0 = time.time()
        while time.time() - t0 < timeout_s:
            if self._friends:
                return
            try:
                if "/login" in (page.url or ""):
                    raise ChatError("Boss 未登录或登录已失效，请先点左上角「登录」")
            except ChatError:
                raise
            except Exception:
                pass
            try:
                # 页面加载完还没数据就点一下「全部」标签，触发列表刷新
                if time.time() - t0 > 12 and page.locator(SEL_ITEM).count() == 0:
                    page.reload(wait_until="domcontentloaded")
            except Exception:
                pass
            page.wait_for_timeout(500)
        raise ChatError("没有取到 Boss 会话列表（可能需要重新登录，或在设置里关闭了消息同步）")

    def _item_rows(self, page) -> list[tuple[int, str, str]]:
        """读会话列表 DOM：[(索引, 名字, 整块文本)]。一次性 evaluate，避免几十次 CDP 往返。"""
        try:
            rows = page.evaluate(
                """() => [...document.querySelectorAll('.user-list-content ul li')].map((el, i) => ({
                    i,
                    name: (el.querySelector('.name-text') || {}).innerText || '',
                    text: el.innerText || '',
                }))"""
            )
        except Exception:
            return []
        return [(r["i"], (r.get("name") or "").strip(), r.get("text") or "") for r in rows]

    def _wait_items(self, page, need: bool = True, timeout_s: float = 12.0) -> int:
        """等会话列表渲染出条目（点开会话后列表要重新出现才能继续点）。"""
        t0 = time.time()
        while True:
            n = 0
            try:
                n = page.locator(SEL_ITEM).count()
            except Exception:
                n = 0
            if (n > 0) == need or time.time() - t0 > timeout_s:
                return n
            page.wait_for_timeout(300)

    # ---------------- 对外 API ----------------

    def friends(self, ctx, force: bool = False) -> list[dict[str, Any]]:
        """会话列表（Boss 上所有聊过的 HR）。"""
        with self._lock, self._activity(45.0):
            page = self._chat_page(ctx)
            fresh = time.time() - self._friends_at < 10
            if force or not self._friends or not fresh:
                self._friends = []
                self._friends_at = 0.0
                self._catch(page, "getGeekFriendList", self._on_friends, "friends")
                try:
                    page.evaluate("() => { const l = document.querySelector('.label-list li'); if (l) l.click(); }")
                except Exception:
                    pass
                try:
                    page.reload(wait_until="domcontentloaded", timeout=45_000)
                except Exception:
                    pass
                self._wait_friends(page, ctx)
                # reload 后列表渲染需要时间，点会话前先等它出现
                self._wait_items(page, need=True, timeout_s=12)
            return [normalize_friend(f) for f in self._friends]

    def find_friend(self, ctx, uid: Any = None, encrypt_job_id: str = "",
                    brand: str = "") -> dict[str, Any] | None:
        """按 uid / 岗位 / 公司名找会话。未投递岗位点开时用。"""
        friends = self.friends(ctx, force=False)
        suid = str(uid or "")
        if suid:
            for f in friends:
                if str(f.get("uid")) == suid:
                    return f
        if encrypt_job_id:
            for f in friends:
                if f.get("encrypt_job_id") == encrypt_job_id:
                    return f
        if brand:
            hit = [f for f in friends if f.get("brand") and f.get("brand") in brand]
            if len(hit) == 1:
                return hit[0]
        return None

    def messages(self, ctx, uid: Any, force: bool = False) -> list[dict[str, Any]]:
        """某个会话的历史消息。先点开会话（DOM 会顺带发起 historyMsg 请求）。"""
        key = str(uid)
        with self._lock, self._activity(40.0):
            page = self._chat_page(ctx)
            cached = self._msgs.get(key)
            if cached and not force and time.time() - cached[0] < 8 and key != "":
                return [normalize_message(m, self._my_id) for m in cached[1]]
            friend = self._raw_friend(key)
            if friend is None:
                # 冷启动 / 新会话：先确保会话列表是新的
                self.friends(ctx, force=True)
                friend = self._raw_friend(key)
            before = self._msgs.get(key, (0.0, []))[0]
            self._open_conversation(page, friend)
            # 优先 DOM：点开会话后气泡就渲染好了，比等 historyMsg 回包快得多。
            # 接口数据到手就换成接口的（带毫秒时间戳和消息类型），两边都拿不到才返回空。
            t0 = time.time()
            dom: list[dict[str, Any]] = []
            dom_at = 0.0
            while time.time() - t0 < 10:
                cur = self._msgs.get(key)
                if cur and cur[0] > before:
                    out = [normalize_message(m, self._my_id) for m in cur[1]]
                    # 回执在 DOM 上：拿到就把已读/送达补进去
                    if not dom_at or time.time() - dom_at > 2:
                        dom = self._messages_from_dom(page) or dom
                        dom_at = time.time()
                    return _merge_status(out, dom)
                if not dom:
                    dom = self._messages_from_dom(page)
                    if dom:
                        dom_at = time.time()
                        return dom
                page.wait_for_timeout(80)
            return dom

    def _messages_from_dom(self, page) -> list[dict[str, Any]]:
        """从对话区 DOM 抓消息。

        实测结构（2026-11）：
            <li class="message-item item-friend|item-myself" data-mid="...">
              <div class="item-time"><span class="time">昨天 14:20</span></div>
              <div class="message-content"> ... <span class="text-content">正文</span>
                  或 <div class="message-card-*">名片/系统卡片</div>
        时间只在「时间分隔」时出现，没有 item-time 的条目沿用上一条的时间。
        """
        try:
            rows = page.evaluate(
                """(sel) => {
                    const conv = document.querySelector(sel);
                    if (!conv) return [];
                    return [...conv.querySelectorAll('.message-item')].map((el, i) => {
                        const body = el.querySelector('.text-content');
                        const card = el.querySelector('[class*="message-card"]');
                        const tm = el.querySelector('.item-time .time');
                        const st = el.querySelector('.message-status');
                        return {
                            i,
                            status_text: st ? (st.innerText || '').trim() : '',
                            status_cls: st ? (st.className || '') : '',
                            cls: el.className || '',
                            mid: el.getAttribute('data-mid') || '',
                            time: tm ? (tm.innerText || '').trim() : '',
                            text: body ? (body.innerText || '').trim()
                                  : (card ? (card.innerText || '').trim() : (el.innerText || '').trim()),
                            is_card: card ? 1 : 0,
                        };
                    });
                }""",
                SEL_CONV,
            )
        except Exception:
            return []
        out = []
        last_time = ""
        for r in rows:
            if r.get("time"):
                last_time = r["time"]
            text = (r.get("text") or "").strip()
            if not text:
                continue
            cls = r.get("cls") or ""
            out.append({
                "id": r.get("mid") or f"dom-{r['i']}",
                "mid": r.get("mid") or "",
                "from_me": "item-myself" in cls,
                "text": text,
                "time": 0,
                "time_text": last_time,
                "kind": None,
                "status": _status_from_text(r.get("status_text") or "", r.get("status_cls") or ""),
                # 兜底路径只在接口没数据时用；只有真正的系统行才标 system
                "system": "item-system" in cls,
            })
        return out

    def _raw_friend(self, uid: str) -> dict[str, Any] | None:
        for f in self._friends:
            if str(f.get("uid")) == uid:
                return f
        return None

    @staticmethod
    def _poll(page, cond, timeout_ms: int = 1500, step_ms: int = 60) -> bool:
        """条件一成立就立刻返回（不再无脑固定等待）。

        条件可以是 JS 表达式字符串，也可以是无参可调用对象。
        """
        deadline = time.time() + timeout_ms / 1000.0
        while time.time() < deadline:
            try:
                ok = page.evaluate(cond) if isinstance(cond, str) else cond()
            except Exception:
                ok = False
            if ok:
                return True
            page.wait_for_timeout(step_ms)
        return False

    def _open_conversation(self, page, friend: dict[str, Any] | None) -> None:
        """点开会话。会话项文本在深层子节点里，locator.filter(has_text=) 会超时，
        所以先一次性 evaluate 读出所有条目，再按索引点。"""
        name = (friend or {}).get("name") or ""
        brand = (friend or {}).get("brandName") or ""
        # 重新挂钩子（页面切会话时数据可能已经缓存过，需要一次性监听）
        self._catch(page, "historyMsg", self._on_history, "open")
        rows = self._item_rows(page)
        if not rows:
            # 刚 reload 完还没渲染：等一下子再读一次
            self._wait_items(page, need=True, timeout_s=10)
            rows = self._item_rows(page)
        if not rows:
            raise ChatError("Boss 会话列表没有渲染出来（页面在加载或登录已失效），请重试或点左上角「登录」")
        idx = None
        for i, nm, txt in rows:
            if name and nm == name:
                idx = i
                break
        if idx is None and name:
            for i, nm, txt in rows:
                if name in txt and (not brand or brand in txt):
                    idx = i
                    break
        if idx is None and brand:
            for i, nm, txt in rows:
                if brand in txt:
                    idx = i
                    break
        if idx is None:
            raise ChatError(f"在 Boss 会话列表里找不到「{name or brand or '该 HR'}」，可能还没打过招呼")
        target_uid = str((friend or {}).get("uid") or "")
        items = page.locator(SEL_ITEM)
        items.nth(idx).click(timeout=15_000)
        try:
            page.wait_for_selector(SEL_CONV, timeout=15_000)
        except Exception:
            pass
        # 点完等对话区真正渲染出来（标题对得上且有消息气泡），命中就立刻返回
        name_js = json.dumps(name or '', ensure_ascii=False)
        self._poll(
            page,
            "(function(){"
            "  var t=document.querySelector('.chat-conversation .name-text');"
            "  var n=" + name_js + ";"
            "  var okTitle=!n||!t||t.innerText.trim()===n;"
            "  var hasItem=!!document.querySelector('.chat-conversation .message-item');"
            "  return okTitle&&hasItem;"
            "})()",
            timeout_ms=4000,
        )
        self._open_uid = target_uid
        # 点开后核对标题，点偏了（同名/列表重排）就重试一次
        try:
            title = page.locator(f"{SEL_CONV} .name-text").first.inner_text(timeout=3000).strip()
        except Exception:
            title = ""
        if title and name and title != name:
            raise ChatError(f"Boss 会话列表点偏了：想打开「{name}」，实际打开的是「{title}」，请重试")

    @staticmethod
    def _input_text(page) -> str:
        """contenteditable 输入框里的当前文本。"""
        try:
            return page.evaluate(
                """(sel) => {
                    const el = document.querySelector(sel);
                    return el ? (el.innerText || '') : '';
                }""",
                SEL_INPUT,
            ) or ""
        except Exception:
            return ""

    def send(self, ctx, uid: Any, text: str) -> dict[str, Any]:
        """给某个 HR 发一条消息（DOM 输入 + 点发送，和真人操作一致）。"""
        text = (text or "").strip()
        if not text:
            raise ChatError("消息内容为空")
        if len(text) > 1000:
            text = text[:1000]
        key = str(uid)
        with self._lock, self._activity(60.0):
            page = self._chat_page(ctx)
            friend = self._raw_friend(key)
            if friend is None:
                self.friends(ctx, force=True)
                friend = self._raw_friend(key)
            if friend is None:
                raise ChatError("在 Boss 会话列表里找不到这条会话，可能还没打过招呼")
            self._open_conversation(page, friend)
            # 输入框是 contenteditable div：先点聚焦，再用键盘输入（受控组件只有真实按键才会
            # 置空发送按钮的 disabled 态），最后清一下可能残留的草稿。
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
            btn = page.locator(SEL_SEND).first
            try:
                btn.click(timeout=8_000)
            except Exception:
                # 兜底：回车发送
                page.keyboard.press("Enter")
            # 等输入框被清空（= 前端已接单），最多 1.5s，清空就立刻继续
            self._poll(page, "!document.querySelector('#chat-input') || !document.querySelector('#chat-input').innerText.trim()", timeout_ms=1500)
            if self._input_text(page).strip():
                raise ChatError("消息没有发出去（输入框内容还在），可能是账号被限制或网络异常")
        self._msgs.pop(key, None)
        self._friends_at = 0.0
        self.log("info", f"已发送消息给 { _short((friend or {}).get('name'), 20)}：{_short(text, 40)}")
        # 发过消息 = 已投递：交给外挂回调改数据库
        info = normalize_friend(friend or {"uid": uid})
        if self._filler:
            try:
                self._filler("mark_delivered", info)
            except Exception as exc:
                self.log("warn", f"标记已投递失败：{exc}")
        return {"ok": True, "uid": uid, "text": text}

    # ---------------- 附件简历 ----------------
    def resumes(self, ctx, force: bool = False) -> list[dict[str, Any]]:
        """Boss 上已上传的附件简历列表。

        数据来自 `/wapi/zpgeek/resume/attachment/checkbox.json?from=2`，
        它只在点「发简历」时才会被前端请求，所以这里打开任一会话点一次按钮、
        拦下响应解析（不点确认，不会真的发出去）。
        """
        if not force and self._resumes and time.time() - self._resumes_at < 120:
            return self._resumes
        page = self._chat_page(ctx)
        self._resumes = []
        self._resumes_at = 0.0
        errs: list[str] = []
        # 临时监听：抓完立刻摘掉，否则常驻监听会参与后续所有会话导航
        on_resp = self._catch(page, "resume", self._on_resumes, "简历列表",
                              on_fail=lambda e: errs.append(e))
        try:
            return self._read_resumes(page, errs)
        finally:
            try:
                page.remove_listener("response", on_resp)
            except Exception:
                pass

    def _read_resumes(self, page, errs: list[str]) -> list[dict[str, Any]]:
        items = page.locator(SEL_ITEM)
        if items.count() == 0:
            self._wait_items(page)
        if page.locator(SEL_ITEM).count() == 0:
            raise ChatError("还没有任何 Boss 会话，无法读取简历列表")
        page.locator(SEL_ITEM).first.click(timeout=15000)
        self._poll(page, "!!document.querySelector('.chat-controls')", timeout_ms=2500)
        btn = self._resume_btn(page)
        if btn is None:
            raise ChatError("Boss 聊天页没有「发简历」按钮（可能账号未上传附件简历）")
        btn.click(timeout=15000)
        deadline = time.time() + 12
        while time.time() < deadline and not self._resumes_at:
            page.wait_for_timeout(200)
        # 关掉 Boss 的「确定发送简历吗」浮层，避免误点确认
        try:
            page.get_by_text("取消", exact=True).first.click(timeout=1500)
        except Exception:
            pass
        if not self._resumes_at and errs:
            raise ChatError(f"读取 Boss 附件简历失败：{errs[-1]}")
        if not self._resumes_at:
            raise ChatError("没有取到 Boss 的附件简历列表（可能还没上传过简历）")
        return self._resumes

    @staticmethod
    def _resume_btn(page):
        """「发简历」按钮：优先 .chat-controls 里的 toolbar-btn，避免命中别处的同名文本。"""
        for sel in (".chat-controls .toolbar-btn:has-text('发简历')",
                    ".chat-controls >> text=发简历"):
            loc = page.locator(sel)
            if loc.count():
                return loc.first
        loc = page.get_by_text("发简历", exact=True)
        return loc.first if loc.count() else None

    def _on_resumes(self, raw: str) -> None:
        try:
            data = json.loads(raw)
        except Exception:
            return
        obj = data.get("zpData") or {}
        lst = obj.get("resumeList") if isinstance(obj, dict) else None
        if not isinstance(lst, list):
            return
        out: list[dict[str, Any]] = []
        for r in lst:
            if not isinstance(r, dict):
                continue
            out.append({
                "id": str(r.get("resumeId") or ""),
                "name": str(r.get("showName") or ""),
                "size": str(r.get("resumeSizeDesc") or ""),
                "upload_time": str(r.get("uploadTime") or ""),
                "suffix": str(r.get("suffixName") or ""),
                "preview": r.get("previewType"),
            })
        self._resumes = out
        self._resumes_at = time.time()
        self.log("info", f"读取到 {len(out)} 份附件简历")

    def send_resume(self, ctx, uid: Any, resume_id: str = "") -> dict[str, Any]:
        """给某位 HR 发附件简历。

        Boss 的流程是「点发简历 → 选一份 → 确认框 → 确定」；若账号只有一份简历，
        点按钮后直接弹确认框。这里按 resume_id 选（列表里只有一份时不用选）。
        """
        with self._lock, self._activity(60):
            page = self._chat_page(ctx)
            friend = self._raw_friend(str(uid))
            if friend is None:
                self.friends(ctx, force=False)
                friend = self._raw_friend(str(uid))
            if friend is None:
                raise ChatError("在 Boss 会话列表里找不到该 HR，可能还没打过招呼")
            self._open_conversation(page, friend)
            self._poll(page, "!!document.querySelector('.chat-controls')", timeout_ms=1500)
            btn = page.get_by_text("发简历", exact=True)
            if btn.count() == 0:
                raise ChatError("Boss 聊天页没有「发简历」按钮（可能账号未上传附件简历）")
            btn.first.click(timeout=15000)
            self._poll(page, "!!document.querySelector('.panel-resume, .sentence-popover')", timeout_ms=2500)
            # 如果有简历选择面板，按 resume_id 或名字点选；只有一份则直接进入确认框
            if resume_id:
                try:
                    page.get_by_text(resume_id, exact=True).first.click(timeout=1500)
                except Exception:
                    pass
            sure = page.get_by_text("确定", exact=True)
            if sure.count() == 0:
                raise ChatError("没有出现「确定向 Boss 发送简历吗」确认框，可能已发送或简历已失效")
            sure.last.click(timeout=15000)
            self._poll(page, "(function(){var b=[...document.querySelectorAll('button')].filter(e=>e.offsetParent!==null&&e.innerText.trim()==='确定');return b.length===0;})()", timeout_ms=2000)
            self.log("info", f"已给 HR {friend.get('name') or uid} 发送附件简历")
            self._msgs.pop(str(uid), None)
            return {"ok": True, "uid": uid}

    def recall(self, ctx, uid: Any) -> dict[str, Any]:
        """撤回：Boss 求职端网页版没有撤回入口。

        2026-11 实测：会话里自己发的气泡悬停/点击/右键都不会出现「撤回」，
        对话区「更多」菜单里只有 置顶/备注/不感兴趣/黑名单/删除/举报，
        站点打包产物里的 `withdrawMessage` 接口在求职端也没被监听使用。
        所以这里不猜 DOM，直接给一句明确的提示，避免误点别的菜单项。
        """
        raise ChatError(
            "Boss 求职端网页版不支持撤回已发送的消息，"
            "请到 Boss直聘 App 里操作（发送后约 10 秒内可撤回）"
        )
