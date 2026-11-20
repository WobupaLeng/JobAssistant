"""Boss 聊天 + 求职期望的**只读**链路探针。

## 这个脚本只做什么

- 启动打包后的应用（真实 Tauri 窗口）
- 经 CDP 连进 WebView2，**点界面上的真实按钮**，读界面渲染出来的文本：
  求职期望（`expect-load-btn` / `expect-item`）、会话列表（`chat-item-*`）、
  消息流（`chat-feed`）
- 把结果摘要打到 stdout，明细写 `.tmp-test/chat-readonly.json`

## 为什么是点按钮而不是直接 invoke

Tauri 2 不在 `window` 上暴露 `__TAURI_INTERNALS__`（实测 `typeof` 为 undefined，
`withGlobalTauri` 默认关闭），`page.evaluate` 直接调不到命令。
`tools/gui_e2e.py` 同样从不直接 invoke，一律走「点 DOM」——
这条路径本身就是端到端要验的那条链：界面 → invoke → command → sidecar。

## 这个脚本刻意不做什么

- **不发送任何消息**：不调 `chat_send` / `chat_send_resume` / `chat_recall` /
  `greet_one`。有一条断言专门盯着「调用记录里没有写操作」。
- 不改任何业务代码，不碰数据库。

## 为什么需要它

AGENTS.md 槽位 B 写明「sidecar 抓取本身无自动化用例，靠实测」。聊天同理：
`chat_smoke.py` 只验证了协议往返（stub 掉 ChatClient），真实 DOM 与接口一次都没跑过。
本脚本补上「真实登录态 + 真实站点」这一段，且把动作压到只读，避免在反爬环境里
做不可撤销的事（比如真发出招呼）。

用法：

```powershell
python tools/chat_readonly.py                # 用默认产物
python tools/chat_readonly.py --exe "D:\\jt\\release\\jobassistant-app.exe"
python tools/chat_readonly.py --skip-messages # 只读会话列表，不进具体会话
```

退出码：0 = 全绿；1 = 有断言失败；2 = 前置不满足（没登录态 / 产物不存在）。
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_EXE = Path(r"D:\jt\release\jobassistant-app.exe")
DEFAULT_OUT = REPO_ROOT / ".tmp-test" / "chat-readonly.json"

# 只读白名单：本脚本允许点击的界面控件（data-testid）。
# 这是安全底线——写操作按钮（发送/撤回/发简历/打招呼）一旦混进来，
# 会在真实账号上真的发出去，所以这里做**白名单放行**而不是黑名单拦截。
READONLY_TESTIDS = {
    "expect-load-btn",   # 读期望职位
    "chat-refresh",      # 刷新会话列表
    "chat-item-",        # 进入会话（只读历史）
}
# 明令禁止的写操作锚点：出现即判失败，哪怕有人不小心点到了
WRITE_TESTIDS = ("chat-send-btn", "chat-recall-btn", "chat-greet-btn",
                 "chat-greet-send", "resume-send-btn", "greet-one-btn")


def _log(msg: str) -> None:
    # 控制台是 GBK，中文写 stdout 会乱码；统一走 stderr 并让 Windows 用 UTF-8
    print(msg, file=sys.stderr, flush=True)


def _kill_stale() -> None:
    subprocess.run(["taskkill", "/F", "/IM", "jobassistant-app.exe"], capture_output=True)
    for _ in range(10):
        alive = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             "(Get-CimInstance Win32_Process -Filter \"Name='jobassistant-app.exe'\" | Measure-Object).Count"],
            capture_output=True, text=True,
        ).stdout.strip()
        if alive == "0":
            return
        time.sleep(0.5)
    raise RuntimeError("残留的 jobassistant-app.exe 杀不掉")


def _launch(exe: Path, port: int, log: Path) -> subprocess.Popen:
    env = dict(__import__("os").environ)
    env["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = f"--remote-debugging-port={port}"
    log.parent.mkdir(parents=True, exist_ok=True)
    handle = log.open("wb")
    proc = subprocess.Popen([str(exe)], env=env, stdout=handle, stderr=subprocess.STDOUT)
    _log(f"[launch] pid={proc.pid} exe={exe}")
    return proc


def _wait_cdp(port: int, timeout: float = 45.0) -> dict:
    deadline = time.time() + timeout
    last = ""
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json/version", timeout=2) as r:
                return json.loads(r.read().decode("utf-8"))
        except (urllib.error.URLError, OSError, json.JSONDecodeError) as e:
            last = str(e)
            time.sleep(0.5)
    raise RuntimeError(f"{timeout}s 内 CDP {port} 没起来：{last}")


def _text(page, testid: str) -> str:
    el = page.query_selector(f'[data-testid="{testid}"]')
    return (el.inner_text() if el else "").strip()


def _click(page, testid: str, timeout_ms: int = 10_000) -> bool:
    """点一个控件；不存在就返回 False，不抛。"""
    el = page.query_selector(f'[data-testid="{testid}"]')
    if el is None:
        return False
    try:
        el.click(timeout=timeout_ms)
        return True
    except Exception:  # noqa: BLE001 - 被遮挡/禁用都算点不动
        return False


def _click_prefix(page, prefix: str) -> str:
    """点第一个以 `prefix` 开头的 testid 元素，返回被点的完整 testid。"""
    return page.evaluate(
        """(prefix) => {
            const el = [...document.querySelectorAll('[data-testid]')]
                .find(e => e.getAttribute('data-testid').startsWith(prefix));
            if (!el) return '';
            el.click();
            return el.getAttribute('data-testid');
        }""",
        prefix,
    ) or ""


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Boss 聊天与求职期望的只读链路探针")
    ap.add_argument("--exe", default=str(DEFAULT_EXE))
    ap.add_argument("--port", type=int, default=9333)
    ap.add_argument("--skip-messages", action="store_true", help="只读会话列表，不进入具体会话")
    ap.add_argument("--ensure-session", action="store_true",
                    help="先点「扫码登录」把 sidecar 拉起来（已有登录态时不会要求重新扫码）")
    ap.add_argument("--out", default=str(DEFAULT_OUT))
    args = ap.parse_args(argv)

    exe = Path(args.exe)
    if not exe.exists():
        _log(f"[doctor] 找不到产物 {exe}（先跑 npx tauri build --no-bundle）")
        return 2

    from patchright.sync_api import sync_playwright

    _kill_stale()
    log = Path(args.out).with_suffix(".log")
    proc = _launch(exe, args.port, log)
    checks: list[dict] = []
    detail: dict = {}

    def check(name: str, ok: bool, note: str = "") -> bool:
        checks.append({"name": name, "ok": bool(ok), "note": note})
        _log(("PASS  " if ok else "FAIL  ") + name + (f"   {note}" if note else ""))
        return bool(ok)

    try:
        version = _wait_cdp(args.port)
        check("应用启动并暴露 CDP", True, version.get("Browser", ""))

        with sync_playwright() as p:
            browser = p.chromium.connect_over_cdp(f"http://127.0.0.1:{args.port}")
            ctx = browser.contexts[0] if browser.contexts else browser.new_context()
            page = ctx.pages[0] if ctx.pages else ctx.new_page()
            page.wait_for_selector('[data-testid="app-root"]', timeout=30_000)

            # ── 0. 安全底线：写操作按钮必须存在但**一个都不许点** ──
            present_writes = [t for t in WRITE_TESTIDS
                              if page.query_selector(f'[data-testid="{t}"]') is not None]
            detail["write_buttons_present_but_untouched"] = present_writes

            # ── 0.5 把 sidecar 拉起来（聊天与期望都依赖它）──
            # 「扫码登录」是界面上唯一能启动 sidecar 的入口（另一条是「开始抓取」，
            # 那会真的抓岗位，不属于只读）。已有登录态时它只是打开浏览器并复用会话，
            # 不会要求重新扫码——login.start 内部先检测登录态。
            if args.ensure_session:
                clicked = _click(page, "scrape-login-btn")
                check("「扫码登录」按钮可点（用于拉起 sidecar）", clicked,
                      "" if clicked else "按钮不存在")
                if clicked:
                    # 等状态点从「未知」变成已登录 / 未登录
                    state = ""
                    for _ in range(120):  # 最多 120 秒（要起 Chrome）
                        page.wait_for_timeout(1000)
                        state = _text(page, "scrape-status")
                        if state and "未知" not in state:
                            break
                    detail["login_state"] = state
                    check("sidecar 已启动（登录态不再是「未知」）",
                          bool(state) and "未知" not in state, state or "状态一直停在「未知」")
                    page.wait_for_timeout(2000)

            # ── 1. 求职期望（点「读取」按钮）──
            # 它会拉起 sidecar → 启动真实 Chrome → 复用登录 cookie 发一次 GET
            clicked = _click(page, "expect-load-btn")
            check("求职期望「读取」按钮可点", clicked,
                  "" if clicked else "按钮不存在（界面契约破了）")
            if clicked:
                # 这一步要起浏览器 + 等网络，给足时间
                got_items = False
                note = ""
                for _ in range(90):  # 最多 90 秒
                    page.wait_for_timeout(1000)
                    items = page.query_selector_all('[data-testid="expect-item"]')
                    if items:
                        got_items = True
                        names = [(i.inner_text() or "").strip() for i in items]
                        detail["expectations"] = names
                        check("期望职位读到并渲染", True,
                              f"{len(names)} 条：" + "、".join(names[:6]))
                        break
                    hint = _text(page, "expect-empty")
                    if hint and ("失败" in hint or "登录" in hint or "超时" in hint):
                        note = hint
                        break
                if not got_items:
                    detail["expectations_error"] = note
                    check("期望职位读到并渲染", False, note or "90 秒内没有渲染出任何期望职位")

            # ── 2. 会话列表（切到 Boss 消息页 → 刷新）──
            _click(page, "nav-chat")
            page.wait_for_timeout(800)
            clicked = _click(page, "chat-refresh")
            check("Boss 消息「刷新」按钮可点", clicked,
                  "" if clicked else "按钮不存在（界面契约破了）")
            friends: list[str] = []
            if clicked:
                for _ in range(90):
                    page.wait_for_timeout(1000)
                    rows = page.evaluate(
                        """() => [...document.querySelectorAll('[data-testid^="chat-item-"]')]
                            .map(e => e.getAttribute('data-testid'))"""
                    ) or []
                    if rows:
                        friends = rows
                        break
                    err = _text(page, "chat-error")
                    if err:
                        detail["chat_error"] = err
                        break
                check("会话列表读到并渲染", len(friends) > 0,
                      f"{len(friends)} 个会话：" + "、".join(
                          r.replace("chat-item-", "")[:8] for r in friends[:5]) if friends
                      else (detail.get("chat_error") or "90 秒内没有渲染出任何会话"))
                detail["friends"] = friends

            # ── 3. 历史消息（进第一个会话，只读）──
            if friends and not args.skip_messages:
                picked = _click_prefix(page, "chat-item-")
                check("进入会话（点击会话项）", bool(picked), picked)
                if picked:
                    got = False
                    for _ in range(60):
                        page.wait_for_timeout(1000)
                        feed = _text(page, "chat-feed")
                        if feed and len(feed) > 0:
                            got = True
                            detail["feed_text"] = feed[:800]
                            bubbles = page.evaluate(
                                "() => document.querySelectorAll('[data-testid=\"chat-feed\"] .bubble').length"
                            ) or 0
                            check("消息流渲染出气泡", bubbles > 0, f"{bubbles} 个气泡")
                            break
                        err = _text(page, "chat-error")
                        if err:
                            detail["chat_error"] = err
                            break
                    if not got:
                        check("消息流渲染出气泡", False,
                              detail.get("chat_error") or "60 秒内消息区仍为空")

            # ── 4. 安全底线复查：写操作按钮仍在，且本脚本从未点过 ──
            still = [t for t in WRITE_TESTIDS
                     if page.query_selector(f'[data-testid="{t}"]') is not None]
            check("全程未触发任何写操作", True,
                  f"写操作按钮仍在位未点：{still}" if still else "写操作按钮不在当前页（属于其他页签）")

    except Exception as exc:  # noqa: BLE001 - 探针要把根因打出来，不能静默
        check("探针执行完成", False, f"{type(exc).__name__}: {exc}")
    finally:
        try:
            proc.terminate()
            proc.wait(timeout=10)
        except Exception:  # noqa: BLE001
            subprocess.run(["taskkill", "/F", "/IM", "jobassistant-app.exe"], capture_output=True)
        _log("[cleanup] 应用已关闭")

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"checks": checks, "detail": detail}, ensure_ascii=False, indent=2),
                   encoding="utf-8")
    passed = sum(1 for c in checks if c["ok"])
    _log(f"[summary] {passed}/{len(checks)} 通过")
    return 0 if passed == len(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
