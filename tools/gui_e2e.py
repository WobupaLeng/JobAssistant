"""GUI 端到端验收驱动：CDP 连进 Tauri 的 WebView2，真点界面，真跑 sidecar。

## 为什么是这条路

交接清单第 5 项的 E2E 二选一，选定「**测真实 Tauri 窗口**」（而非只测 WebView 页面）：

- 只测页面的方案（`npm run dev` + 浏览器开 `localhost:5184`）里没有 `__TAURI_INTERNALS__`，
  `invoke()` 全部失败，等于什么都没测到。
- 本方案连的是**打包后应用里真实渲染的那个 WebView2**，走完
  「界面 → invoke → Tauri command → core → SQLite / python sidecar」整条链。
- 不引入 Node 侧 Playwright：复用 sidecar 已有的 patchright，且**不改一行业务代码**——
  WebView2 远程调试是 Chromium 原生能力，靠环境变量 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` 打开。

## 用法

前置（三条都是硬要求，见 AGENTS.md 槽位 A 与 DEVELOPMENT.md 2026-11-21 踩坑）：

1. 产物必须在**工作区外**：`$env:CARGO_TARGET_DIR="D:\\jt"; cd app; npx tauri build --no-bundle`
   —— 工作区内的 exe 继承 `Mandatory Label\\Low Mandatory Level`，进程被降为 Low IL，起不来。
2. 验收前先确保没有残留实例：`taskkill` 返回成功不代表进程没了，要用
   `Get-CimInstance Win32_Process -Filter "Name='jobassistant-app.exe'"` 复核为 0。
3. 已登录 Boss（登录态在 `%LOCALAPPDATA%\\ai-job-assistant`，sidecar 与壳共用）。

```powershell
# 1. 先看有没有登录态（不需要开应用，5 秒）
python tools/gui_e2e.py doctor

# 2. 跑一遍完整点击路径（会启动应用、真抓取、真落库、最后自己关掉应用）
python tools/gui_e2e.py run --exe "D:\\jt\\release\\jobassistant-app.exe"

# 只验界面契约，不碰网络（CI 友好）
python tools/gui_e2e.py run --ui-only
```

退出码：0 = 全绿；1 = 有断言失败（明细在 `--out` 指定的 JSON 里）；2 = 环境不满足前置条件。
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = REPO_ROOT / ".tmp-test" / "gui-e2e.json"

# 登录态目录：与sidecar 与壳共用，见 scraper/browser.py::data_dir()
LOGIN_DATA_DIR = Path(os.environ.get("AJA_DATA_DIR") or (Path(os.environ["LOCALAPPDATA"]) / "ai-job-assistant"))

# 界面契约：AGENTS.md 2.3 要求所有可交互控件带 data-testid，缺一个就算契约破损。
#
# 2026-11-21（Leptos 重写后）对齐说明：
# - `scrape-progress` 在 Leptos 版里是**容器**（里面 5 个数字各挂
#   `scrape-scraped / -inserted / -duplicates / -below / -rejected`），
#   所以进度读数优先按元素取，取不到再退回解析容器文本（`_parse_progress`）。
# - `demo-title` 是「打分试算」卡片标题的锚点；Leptos 版把它复用成了
#   **文本输入框**（`demo-title-input` 另有其锚点），脚本点名的元素是按钮，
#   所以填表那段显式用 `demo-title-input`。
# 每一项都可以写成「元组」表示别名：任一命中即算有（向后/向前兼容窗口）。
REQUIRED_TESTIDS: list[str | tuple[str, ...]] = [
    "app-root",
    "score-spec",
    "job-refresh",
    "scrape-status",
    "scrape-login-btn",
    "scrape-keywords",
    "scrape-min-score",
    "scrape-start-btn",
    "scrape-stop-btn",
    "scrape-progress",
    "scrape-feed",
    # SPEC-002 第 3.3 节：全量重算入口必须可见可点
    "rescore-panel",
    "rescore-btn",
    "rescore-llm-toggle",
    "rescore-note",
    "demo-score-btn",
]

# 试算表单的输入锚点，与 REQUIRED_TESTIDS 同等地位（漏一个整段试算就点不动）
DEMO_FORM_TESTIDS = (
    ("demo-title", "demo-title-input"),
    "demo-salary",
    "demo-exp",
    "demo-jd",
    "demo-result",
)

# 概览页「表格行」的选择器：确认过 DOM 仍是 `<tbody> <tr>`（features/jobs/list.rs）
ROW_SELECTOR = '[data-testid="app-root"] tbody tr'


def _ids(spec: str | tuple[str, ...]) -> tuple[str, ...]:
    return (spec,) if isinstance(spec, str) else spec


def _first_present(page, spec: str | tuple[str, ...]) -> str | None:
    """返回该契约项实际命中的 testid（别名优先），全都没有则 None。"""
    for t in _ids(spec):
        if page.query_selector(f'[data-testid="{t}"]') is not None:
            return t
    return None


def _missing_testids(page, specs) -> list[str]:
    """逐个探测，**不短路**：一项缺失不影响其它项的报告。"""
    out = []
    for spec in specs:
        if _first_present(page, spec) is None:
            out.append(" | ".join(_ids(spec)))
    return out

PROGRESS_RE = re.compile(r"已抓 (\d+).*?入库 (\d+).*?重复 (\d+).*?低于门槛 (\d+).*?规则否决 (\d+)", re.S)

# 全量重算的进度文案（frontend/ 的 rescore-note）。
#
# 契约（`features/jobs/rescore.rs`）：`已扫描 N 条，改写 M 条[，K 条失败]`；
# 进行中只更新这一行，**没有**「重算完成」这样的终态文案，所以「跑完了」只能用
# 「按钮重新可点」判定（见 `_wait_rescore_done`）。两个正则都容忍旧文案「扫描」。
RESCORE_SCANNED_RE = re.compile(r"(?:已扫描|扫描) (\d+) 条")
RESCORE_UPDATED_RE = re.compile(r"改写 (\d+) 条")
RESCORE_FAILED_RE = re.compile(r"(\d+) 条失败")


# ────────────────────────────── 结果收集 ───────────────────────────────


@dataclass
class Check:
    name: str
    ok: bool
    detail: str = ""


@dataclass
class Report:
    mode: str
    started_at: float = field(default_factory=time.time)
    checks: list[Check] = field(default_factory=list)
    steps: list[dict] = field(default_factory=list)
    progress: dict = field(default_factory=dict)
    feed_tail: str = ""

    def add(self, name: str, ok: bool, detail: str = "") -> bool:
        self.checks.append(Check(name, ok, detail))
        mark = "PASS" if ok else "FAIL"
        print(f"[{mark}] {name}" + (f" — {detail}" if detail else ""))
        return ok

    def step(self, name: str, **kw) -> None:
        self.steps.append({"step": name, "t": round(time.time() - self.started_at, 2), **kw})
        print(f"[step] {name} {json.dumps(kw, ensure_ascii=False)[:300]}")

    @property
    def failed(self) -> list[Check]:
        return [c for c in self.checks if not c.ok]

    def dump(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "mode": self.mode,
            "elapsed_s": round(time.time() - self.started_at, 2),
            "failed": len(self.failed),
            "checks": [c.__dict__ for c in self.checks],
            "steps": self.steps,
            "progress": self.progress,
            "feed_tail": self.feed_tail,
        }
        path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"[report] {path}")


# ────────────────────────────── 环境前置 ───────────────────────────────


def doctor() -> int:
    """不开应用，只检查跑 E2E 的前置条件。"""
    ok = True
    print(f"仓库根：{REPO_ROOT}")
    print(f"登录态目录：{LOGIN_DATA_DIR}  {'存在' if LOGIN_DATA_DIR.exists() else '不存在'}")

    # 候选路径按「sidecar 实际用的」排在前面：<data_dir>/data/browser-profile。
    # 顶层 Default/ 是 Chrome 自己写的，不含 Boss 会话，只看不改。
    candidates = [
        LOGIN_DATA_DIR / "data" / "browser-profile" / "Default" / "Network" / "Cookies",
        LOGIN_DATA_DIR / "browser-profile" / "Default" / "Network" / "Cookies",
    ]
    existing = [c for c in candidates if c.exists()]
    if not existing:
        print(f"  ! 没找到 cookie 库（找过 {[str(c) for c in candidates]}）——从未登录过 Boss")
        ok = False
    else:
        best = max(existing, key=lambda c: len(_login_cookie_names(c)))
        names = _login_cookie_names(best)
        print(f"  cookie 库：{best}（{best.stat().st_size} 字节）")
        print(f"  登录 cookie：{sorted(names) if names else '无'}")
        if not names:
            ok = False

    try:
        import patchright  # noqa: F401

        print("patchright：已安装")
    except ImportError:
        print("patchright：未安装（E2E 驱动需要它，与 sidecar 共用）")
        ok = False

    return 0 if ok else 2


def _login_cookie_names(cookie_db: Path) -> set[str]:
    """读 cookie 名。整个函数不得向外抛：doctor 是诊断工具，读不到就是读不到。"""
    try:
        return _login_cookie_names_inner(cookie_db)
    except Exception as e:  # noqa: BLE001 — 沙箱里 tempfile/权限异常形态很多，见下
        print(f"  ! 读 cookie 库异常（{type(e).__name__}: {e}），按「无登录态」处理")
        return set()


def _login_cookie_names_inner(cookie_db: Path) -> set[str]:
    """直接用 sqlite3 读 cookie 名，不解密——只看有没有，不碰敏感值（AGENTS.md 2.4）。"""
    import shutil
    import sqlite3
    import tempfile

    # 浏览器/Chrome 可能正锁着库，拷一份到临时目录读。
    # `ignore_cleanup_errors` 是必需的：沙箱里 tempfile 的 rmtree 会在 chmod 上
    # 报 `PermissionError: [WinError 5]`，把 doctor 的退出码从 3 变成 1——
    # 医生脚本自身崩溃会盖掉真正的诊断结论（2026-11-21 实测）。
    try:
        td_ctx = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        td = td_ctx.name
    except OSError as e:  # 建临时目录本身就没权限：直接原地读
        print(f"  ! 建临时目录失败：{e}，改为原地只读打开")
        td_ctx = None
        td = None
    try:
        copy = Path(td) / "Cookies" if td else cookie_db
        try:
            shutil.copy2(cookie_db, copy)
        except OSError as e:
            # 沙箱常把上级临时目录的写权限掐掉（WinError 13/5）。退一步：原地只读打开。
            print(f"  ! 拷贝 cookie 库失败：{e}，改为原地只读打开")
            copy = cookie_db
        try:
            con = sqlite3.connect(f"file:{copy}?mode=ro", uri=True)
            try:
                rows = con.execute("SELECT name FROM cookies").fetchall()
            finally:
                con.close()
        except sqlite3.Error as e:
            print(f"  ! 读取 cookie 库失败：{e}")
            return set()
    finally:
        if td_ctx is not None:
            td_ctx.cleanup()
    wanted = {"bst", "zp_at", "wt2", "geek_zp_token"}  # scraper/browser.py::LOGIN_COOKIES
    return {r[0] for r in rows if r[0] in wanted}


# ─────────────────────── 静态契约（不启动应用）─────────────────────────

# 每个锚点必须出现在哪个前端文件里。file 为 None 表示「任意源文件出现即可」。
# 这是**双向对齐的落点**：Leptos 版改了结构，改这张表；前端漏了锚点，这里会点名文件。
TESTID_HOMES: dict[str, str | None] = {
    "app-root": "src/components/shell.rs",
    "score-spec": "src/components/shell.rs",
    "job-refresh": "src/features/jobs/list.rs",
    "scrape-status": "src/features/scrape/panel.rs",
    "scrape-login-btn": "src/features/scrape/panel.rs",
    "scrape-keywords": "src/features/scrape/panel.rs",
    "scrape-min-score": "src/features/scrape/panel.rs",
    "scrape-start-btn": "src/features/scrape/panel.rs",
    "scrape-stop-btn": "src/features/scrape/panel.rs",
    "scrape-progress": "src/features/scrape/panel.rs",
    "scrape-feed": "src/features/scrape/panel.rs",
    "rescore-panel": "src/features/jobs/rescore.rs",
    "rescore-btn": "src/features/jobs/rescore.rs",
    "rescore-llm-toggle": "src/features/jobs/rescore.rs",
    "rescore-note": "src/features/jobs/rescore.rs",
    "demo-score-btn": "src/features/score/demo.rs",
}

# 五个进度 Stat 的锚点以**组件属性**写法下发：`<Stat … test_id="scrape-scraped"/>`。
# 只在这里点名 test_id 字面量，否则「属性名改掉但值仍在」的漏写查不出来。
PROP_TESTIDS: dict[str, str] = {
    "scrape-scraped": "test_id=",
    "scrape-inserted": "test_id=",
    "scrape-duplicates": "test_id=",
    "scrape-below": "test_id=",
    "scrape-rejected": "test_id=",
}

# 文案契约（AGENTS.md §2.3）：元素在 Rust 里是 `data-testid="x"`，属性写法可能是
# `data-testid="..."` 字面量、也可能是 `attr:data-testid=` / `test_id="..."` 宏写法，
# 所以这里只查**文案片段**，查不到就算契约破了。
TEXT_CONTRACTS: list[tuple[str, str]] = [
    ("五段进度文案", "已抓 {} · 入库 {} · 重复 {} · 低于门槛 {} · 规则否决 {}"),
    ("入库行 tag", "✔"),
    ("抓取启动回执", "已启动抓取进程"),
    ("重算进度文案", "已扫描"),
    ("重算改写文案", "改写"),
]


def _rust_sources() -> dict[str, str]:
    root = REPO_ROOT / "frontend"
    out: dict[str, str] = {}
    for p in sorted((root / "src").rglob("*.rs")):
        out[str(p.relative_to(root)).replace("\\", "/")] = p.read_text(encoding="utf-8", errors="replace")
    return out


def _anchor_home(tid: str, src: dict[str, str]) -> str | None:
    """该锚点在前端源码里由哪个文件定义（找不到返回 None）。"""
    attr = PROP_TESTIDS.get(tid, "data-testid=")
    needle = f'{attr}"{tid}"'
    hits = [f for f, body in src.items() if needle in body]
    if not hits:
        # Leptos 里 `data-testid=test_id` 与 `data-testid="字面量"` 两种都合法，回退到裸名
        hits = [f for f, body in src.items() if f'"{tid}"' in body]
    return hits[0] if hits else None


def contract_check() -> int:
    """不开应用，静态核对 `frontend/src/**` 是否满足本脚本的 E2E 契约。

    存在理由：`run --ui-only` 要能启动真实 exe（需要工作区外的产物 + 可写的
    `%LOCALAPPDATA%`）。这两条在本机/沙箱里常常一条都不满足，此时「界面契约」
    这一层仍然**可以静态验证**：缺哪个锚点、哪个文案被改了，这里直接点名到文件。
    返回 0 = 契约齐；1 = 有缺口（逐条打印）。
    """
    src = _rust_sources()
    blade = "\n".join(src.values())
    missing: list[str] = []

    for tid in _ids(REQUIRED_TESTIDS):
        home = TESTID_HOMES.get(tid)
        where = f"（应出现在 {home}）" if home else ""
        found = _anchor_home(tid, src)
        if found is None:
            missing.append(f"data-testid={tid!r} 在前端源码里找不到{where}")
        elif home and found != home:
            # 在别处出现了：不算缺，只提示归位（避免把「重命名文件」误判成契约破坏）
            print(f"[note] {tid} 不在 {home}，而在 {found}（契约仍满足）")

    demo_alias = [a for a in DEMO_FORM_TESTIDS[0] if a in blade]
    if not demo_alias:
        missing.append(f"试算表单标题锚点全缺：{DEMO_FORM_TESTIDS[0]}")

    for label, snippet in TEXT_CONTRACTS:
        if snippet not in blade:
            missing.append(f"文案契约「{label}」片段找不到：{snippet!r}")

    for m in missing:
        print(f"[FAIL] {m}")
    total = len(_ids(REQUIRED_TESTIDS)) + 1 + len(TEXT_CONTRACTS)
    print(f"[contract] {total - len(missing)}/{total} 项契约成立（扫描 {len(src)} 个 .rs）")
    return 0 if not missing else 1


def _kill_stale_app() -> None:
    """清掉残留实例。taskkill 返回成功不代表进程真没了，必须复核。"""
    def alive_count() -> str | None:
        """返回存活数；**探测本身失败时返回 None**（不能当成 0）。"""
        try:
            r = subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 "(Get-CimInstance Win32_Process -Filter \"Name='jobassistant-app.exe'\" "
                 "| Measure-Object).Count"],
                capture_output=True, text=True, timeout=30,
            )
        except subprocess.TimeoutExpired:
            return None
        if r.returncode != 0:
            return None
        out = (r.stdout or "").strip()
        return out if out.isdigit() else None

    subprocess.run(["taskkill", "/F", "/IM", "jobassistant-app.exe"], capture_output=True)
    for _ in range(10):
        alive = alive_count()
        if alive is None:
            # WMI 被沙箱/权限挡住（本机实测：`拒绝访问`）。降级为 tasklist 只读复核，
            # 再不行就**放弃复核**而不是假定 0——把「看不到」当成「已清干净」
            # 会让下一轮连到上一轮的窗口，测出来的东西不可信。
            alive = _tasklist_count()
            if alive is None:
                print("[warn] 无法复核残留实例（WMI 与 tasklist 都不可用），跳过复核")
                return
        if alive == "0":
            return
        time.sleep(0.5)
    raise RuntimeError("残留的 jobassistant-app.exe 杀不掉，先手工处理再跑")


def _tasklist_count() -> str | None:
    try:
        r = subprocess.run(["tasklist", "/FI", "IMAGENAME eq jobassistant-app.exe", "/NH"],
                           capture_output=True, text=True, timeout=30)
    except subprocess.TimeoutExpired:
        return None
    if r.returncode != 0:
        return None
    return str((r.stdout or "").lower().count("jobassistant-app.exe"))


class LaunchError(RuntimeError):
    """应用起来了但立刻死了（前置检查失败之类）。属于「环境不满足」，退出码 2。"""


_current: dict = {}  # 当前这一次的 proc / log，供 `_wait_cdp` 判活


def _launch(exe: Path, port: int, log: Path) -> subprocess.Popen:
    env = dict(os.environ)
    env["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = f"--remote-debugging-port={port}"
    log.parent.mkdir(parents=True, exist_ok=True)
    handle = log.open("wb")
    handle.write(f"# exe={exe}\n# CDP port={port}\n".encode("utf-8"))
    handle.flush()
    proc = subprocess.Popen([str(exe)], env=env, stdout=handle, stderr=subprocess.STDOUT)
    _current.update(proc=proc, log=log)
    print(f"[launch] pid={proc.pid} exe={exe}")
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
            alive = _current.get("proc")
            if alive is not None and alive.poll() is not None:
                raise LaunchError(
                    f"应用启动后立刻退出（exit code {alive.returncode}），CDP 不可能起来。"
                    f"前置检查多半没过，看日志 {_current.get('log')}。最后一次探测错误：{last}")
            time.sleep(0.5)
    raise RuntimeError(f"{timeout}s 内 CDP {port} 没起来（应用可能起不来，先看日志）：{last}")


# ────────────────────────────── 界面操作 ───────────────────────────────


def _text(page, testid: str) -> str:
    el = page.query_selector(f'[data-testid="{testid}"]')
    return (el.inner_text() if el else "").strip()


def _num_of(page, testid: str) -> int | None:
    """读一个纯数字锚点（`Stat` 渲染的就是纯数字）。

    只认**整个文本就是数字**：`Stat` 里是 `"3"`，而容器锚点（例如前端的
    `scrape-progress`，其文本是「已抓 3 入库 1 …」）不会被误读成 3——
    否则「已抓 3 · 入库 1 · …」会把 scraped 读成 3 而 inserted 也读成 3。
    """
    raw = _text(page, testid)
    return int(raw) if raw.isdigit() else None


# 进度五项：语义键 → (独立锚点, 五段文案里的标签)
PROGRESS_KEYS: dict[str, tuple[str, str]] = {
    "scraped": ("scrape-scraped", "已抓"),
    "inserted": ("scrape-inserted", "入库"),
    "duplicates": ("scrape-duplicates", "重复"),
    "below_threshold": ("scrape-below", "低于门槛"),
    "rejected": ("scrape-rejected", "规则否决"),
}


def _parse_progress(page) -> dict:
    """读五路计数。

    优先走 Leptos 版给的独立数字锚点（`scrape-scraped` …）；
    锚点缺失时退回解析 `scrape-progress` 的容器文本——那条正则
    （`已抓 N · 入库 N · 重复 N · 低于门槛 N · 规则否决 N`）仍然是契约的一部分：
    它在抓取面板里由 `sr-only` 的摘要行渲染，容器文本兜底时能命中。
    """
    out: dict = {}
    for key, (testid, label) in PROGRESS_KEYS.items():
        v = _num_of(page, testid)
        if v is not None:
            out[key] = v
    if len(out) == len(PROGRESS_KEYS):
        return out

    raw = _text(page, "scrape-progress")
    m = PROGRESS_RE.search(raw)
    if m:
        for key, val in zip(PROGRESS_KEYS, m.groups()):
            out.setdefault(key, int(val))
    out["raw"] = raw
    return out


def _expect_progress(page, key: str, op: str, threshold: int, timeout: float) -> tuple[bool, int]:
    """轮询进度栏直到条件成立，返回 (是否成立, 最后一次的值)。"""
    deadline = time.time() + timeout
    last = -1
    while time.time() < deadline:
        page.wait_for_timeout(500)
        val = _parse_progress(page).get(key, last)
        if isinstance(val, int):
            last = val
            if (op == ">=" and val >= threshold) or (op == "<=" and val <= threshold):
                return True, val
    return False, last


def _rescore_count(page) -> int | None:
    """从 `rescore-note` 里取「扫描 N 条」的 N；还没跑或文案没到那一步时返回 None。"""
    m = RESCORE_SCANNED_RE.search(_text(page, "rescore-note"))
    return int(m.group(1)) if m else None


def _wait_rescore_done(page, timeout: float = 120.0) -> tuple[bool, str]:
    """点完「全量重算」后等它跑完，返回 (是否完成, 最终文案)。

    判据是**按钮重新可点**，不是文案：`rescore.rs` 的按钮在重算期间同时显示
    「重算中…」并 `disabled`，跑完后恢复文案、解除禁用；而 `rescore-note`
    全程只有「已扫描 N 条，改写 M 条」一行，没有「重算完成」这种终态串。
    如果哪天真加了终态文案，这里也会立刻返回（两条判据取先到者）。
    """
    deadline = time.time() + timeout
    note = ""
    while time.time() < deadline:
        note = _text(page, "rescore-note")
        if note.startswith("重算完成") or note.startswith("重算中断"):
            return True, note
        btn = page.query_selector('[data-testid="rescore-btn"]')
        if btn is not None and not btn.is_disabled():
            # 至少见过一次进度行才算跑过；否则是「点击没生效」而不是「跑完了」
            if _rescore_count(page) is not None:
                return True, note
        page.wait_for_timeout(400)
    return False, note


def run_exe(report: Report, exe: Path, *, ui_only: bool, keywords: str, min_score: int,
            budget: int, port: int) -> int:
    from patchright.sync_api import sync_playwright

    if not exe.exists():
        raise RuntimeError(f"找不到产物 {exe}（先跑 npx tauri build --no-bundle）")
    if exe.is_relative_to(REPO_ROOT):
        print(f"[warn] 产物在仓库工作区内：{exe}。它会继承 Low 完整性标签而起不来，"
              f"请用 $env:CARGO_TARGET_DIR=D:\\jt 构建后指向工作区外。")

    _kill_stale_app()
    log = DEFAULT_OUT.with_suffix(".log")
    proc = _launch(exe, port, log)
    try:
        version = _wait_cdp(port)
        report.step("cdp_up", browser=version.get("Browser"))
        report.add("应用启动并暴露 CDP", True, version.get("Browser", ""))

        with sync_playwright() as p:
            browser = p.chromium.connect_over_cdp(f"http://127.0.0.1:{port}")
            ctx = browser.contexts[0] if browser.contexts else browser.new_context()
            page = ctx.pages[0] if ctx.pages else ctx.new_page()
            page.set_default_timeout(30_000)

            page.wait_for_selector('[data-testid="app-root"]')
            report.step("ui_loaded", url=page.url, title=page.title())

            # ── 1. 界面契约：data-testid 一个不能少 ────────────────────
            # 缺锚点只记「本次」失败，绝不短路：后面的读库 / 重算 / 试算照跑，
            # 这样一份报告就能看出「缺哪几个锚点」和「其余路径是否健康」。
            missing = _missing_testids(page, REQUIRED_TESTIDS)
            report.add("界面契约：data-testid 齐全", not missing,
                       f"缺 {missing}" if missing else f"{len(REQUIRED_TESTIDS)} 项全中")
            missing_form = _missing_testids(page, DEMO_FORM_TESTIDS)
            report.add("试算表单 input 锚点齐全", not missing_form,
                       f"缺 {missing_form}" if missing_form else "")

            spec = _text(page, "score-spec")
            report.add("打分口径由 Rust 下发（前端不硬编码）", bool(spec), spec)

            # ── 2. 读库路径：刷新 → 表格行数 → 快照图（不是冒烟断言）──
            rows_before = 0
            if _first_present(page, "job-refresh"):
                page.click('[data-testid="job-refresh"]')
                page.wait_for_timeout(1200)
                rows_before = page.eval_on_selector_all(ROW_SELECTOR, "e => e.length")
            else:
                report.add("读库路径：刷新列表", False, "缺锚点 job-refresh，跳过刷新")
            report.step("refresh", rows=rows_before)

            # ── 2.5 全量重算（SPEC-002 第 3.3 节）：点一次，跑两遍 ──────
            # 第一遍验证「能改写」；第二遍验证「幂等」——同一口径重算第二遍
            # 不该再改任何一条，否则每次启动分数都会漂移。
            btn = page.query_selector('[data-testid="rescore-btn"]')
            if btn is None:
                report.add("全量重算可执行", False, "缺锚点 rescore-btn")
            elif btn.is_disabled():
                # 按钮在 `total == 0` 时禁用：空库是正常状态，不是契约破损
                report.add("全量重算可执行", True,
                           "库里没有岗位，按钮按设计禁用（跳过两遍重算）")
            else:
                page.click('[data-testid="rescore-btn"]')
                done1, note1 = _wait_rescore_done(page)
                scanned = _rescore_count(page)
                report.step("rescore_pass1", note=note1)
                report.add("全量重算跑完并报告扫描条数", done1 and bool(scanned) and scanned > 0, note1)
                if scanned and scanned > 0:
                    report.add("重算结果无失败条目",
                               "条失败" not in note1, note1)
                page.wait_for_timeout(600)
                btn = page.query_selector('[data-testid="rescore-btn"]')
                if btn is not None and not btn.is_disabled():
                    page.click('[data-testid="rescore-btn"]')
                    done2, note2 = _wait_rescore_done(page)
                    updated2 = RESCORE_UPDATED_RE.search(note2)
                    report.step("rescore_pass2", note=note2)
                    report.add(
                        "重算幂等：同一口径第二遍不再改写",
                        done2 and updated2 is not None and int(updated2.group(1)) == 0,
                        note2,
                    )
                else:
                    report.add("重算幂等：同一口径第二遍不再改写", False, "第二遍按钮不可点")

            if not ui_only:
                # 快照图用 JD 里塞一个随机标记，既不引大模型、也不依赖网络
                token = f"AJA-E2E-{random.randrange(10**6):06d}"
                demo_jd = ("负责后端服务开发，熟悉 Java、Spring Boot、MySQL、Redis。"
                           f"这里只是一个可检索的标记：{token}。")
                # 标题输入框：`demo-title` 在 Leptos 版里是卡片标题的锚点名，
                # 输入框是 `demo-title-input`；两者都试，命中哪个填哪个。
                title_id = _first_present(page, ("demo-title-input", "demo-title"))
                if missing_form or title_id is None:
                    report.add("打分试算返回百分制分数", False,
                               f"试算表单锚点缺失，跳过（缺 {missing_form}）")
                    score = None
                else:
                    page.fill(f'[data-testid="{title_id}"]', f"E2E 后端开发 {token}")
                    page.fill('[data-testid="demo-salary"]', "20-30K")
                    page.fill('[data-testid="demo-exp"]', "3-5年")
                    page.fill('[data-testid="demo-jd"]', demo_jd)
                    page.click('[data-testid="demo-score-btn"]')
                    # 试算结果只在返回后才渲染，等锚点比等固定时长可靠
                    try:
                        page.wait_for_selector('[data-testid="demo-result"]', timeout=20_000)
                    except Exception:  # noqa: BLE001 — 失败由下面的断言报告
                        pass
                    result = _text(page, "demo-result")
                    # 渲染格式：「78 [规则] [N tokens] [无 JD，判不入库] 理由」——
                    # Leptos 版分数是**首个独立整数**（不再有 `/ 100` 后缀）。
                    matched = re.match(r"\s*(\d{1,3})(?!\d)", result)
                    score = int(matched.group(1)) if matched else None
                    report.step("demo_score", score=score, text=result[:120])
                    report.add("打分试算返回百分制分数",
                               score is not None and 0 <= score <= 100,
                               f"score={score} text={result[:80]!r}")
                # 快照图只负责「人在浏览器里点得到、读得懂」，不做数值断言
                snap = REPO_ROOT / ".tmp-test" / "gui-e2e-home.png"
                page.screenshot(path=str(snap), full_page=True)
                report.add("首页快照可读（不是白屏）", snap.exists() and snap.stat().st_size > 20_000,
                           str(snap))
            if ui_only:
                report.progress = {"ui_only": True}
                report.feed_tail = _text(page, "scrape-feed")
                # 缺锚点会在这里被计数成失败，但其余检查已经全部跑完
                report.add("界面契约无缺失（ui-only 汇合门禁）", not missing and not missing_form,
                           f"缺 {missing + missing_form}" if (missing or missing_form) else "")
                return 0 if not report.failed else 1

            # ── 3. 登录态：未登录会话里第一个动作就该是 login.start ──
            start_feed = _text(page, "scrape-feed")
            report.progress.update({"rows_before": rows_before})

            # ── 4. 真抓取：门槛设成 100，这一轮断言「低于门槛」在涨 ────
            #    故意用极值门槛：既验证「界面参数真的传到了 Rust」（曾经的静默丢弃 bug），
            #    又不需要关心本轮 LLM 打分波动——低于门槛的计数必然随抓取上升。
            page.fill('[data-testid="scrape-keywords"]', keywords)
            page.fill('[data-testid="scrape-min-score"]', str(min_score))
            page.click('[data-testid="scrape-start-btn"]')
            page.wait_for_timeout(800)

            banner = _text(page, "scrape-feed")
            reply = next((ln for ln in banner.splitlines() if "已启动抓取进程" in ln), "")
            report.step("scrape_start_reply", line=reply[:200])
            m = re.search(r"门槛 (\d+) 分", reply)
            # 只断言「本轮新增」的事件里没有解析失败。`banner` 是事件流全文，
            # 里面可能残留上一次会话的历史行——那样会把好结果判成失败。
            started_new = banner[len(start_feed):] if banner.startswith(start_feed) else banner
            report.add("scrape_start 回执无「无法解析」",
                       "无法解析" not in started_new, reply[:160] or started_new[:160])
            report.add("界面设的门槛真的传到了 Rust（不是默认 70）",
                       bool(m) and int(m.group(1)) == min_score,
                       f"回执门槛={m.group(1) if m else '?'}，界面设={min_score}")

            report.add("状态进入「抓取中」", "抓取中" in _text(page, "scrape-status"),
                       _text(page, "scrape-status"))

            if "尚未登录" in banner:
                report.step("blocked_by_login", feed=banner[:300])
                report.add("登录态可用（%LOCALAPPDATA%\\ai-job-assistant）", False,
                           "sidecar 报未登录：先 python tools/gui_e2e.py doctor 核查")
                report.feed_tail = banner[-2000:]
                return 1

            # ── 5. 观察窗口：进度读数必须真的在动，且「无法解析」恒为 0 ──
            since_start = lambda: _text(page, "scrape-feed")[len(start_feed):]  # noqa: E731
            moved, scraped = _expect_progress(page, "scraped", ">=", 1, budget)
            report.add("抓取线程真的在工作（已抓 ≥ 1）", moved, f"已抓 {scraped}")

            if moved:
                # 门槛 100 ⇒ 分数 ≤ 100 但实际几乎不可能到 100，低于门槛数应随抓取上升
                gt, below = _expect_progress(page, "below_threshold", ">=", 2, max(60, budget // 2))
                report.add("门槛 100 生效（低于门槛计数上升）", gt, f"低于门槛 {below}")
                report.progress.update(_parse_progress(page))

                # 事件流里同一批岗位不能既算重复又算入库，且计数与事件条数对得上
                feed_new = since_start()
                report.add("事件流无「无法解析」",
                           "无法解析" not in feed_new,
                           f"命中 {feed_new.count('无法解析')} 次")
                inserted_lines = feed_new.count("✔")
                parsed = report.progress
                if "inserted" in parsed:
                    report.add("事件流与进度栏口径一致（入库数 ≤ ✔ 行数）",
                               parsed["inserted"] <= inserted_lines,
                               f"进度栏入库 {parsed['inserted']}，事件流 ✔ {inserted_lines} 行")

            # ── 6. 停止：状态必须立刻离开「抓取中」，不能等到 Exited ──
            page.click('[data-testid="scrape-stop-btn"]')
            stopped = False
            for _ in range(20):
                page.wait_for_timeout(500)
                if "抓取中" not in _text(page, "scrape-status"):
                    stopped = True
                    break
            status = _text(page, "scrape-status")
            report.step("stopped", status=status)
            report.add("点停止后 10s 内离开「抓取中」", stopped, status)

            # ── 7. 落库：刷新后行数不减少，且快照图那条岗位在库里 ──────
            page.click('[data-testid="job-refresh"]')
            page.wait_for_timeout(1500)
            rows_after = page.eval_on_selector_all(ROW_SELECTOR, "e => e.length")
            report.step("refresh_after", rows=rows_after)
            report.add("刷新后列表行数不减少", rows_after >= rows_before,
                       f"{rows_before} → {rows_after}")

            report.feed_tail = _text(page, "scrape-feed")[-2000:]
            return 0 if not report.failed else 1
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
        _kill_stale_app()
        print(f"[cleanup] 应用已关闭，日志 {log}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="GUI 端到端验收驱动（CDP 连真实 Tauri 窗口）")
    sub = ap.add_subparsers(dest="cmd", required=True)

    sub.add_parser("doctor", help="不开应用，检查登录态与 patchright 是否就绪")

    sub.add_parser("contract", help="不开应用，只静态核对 Leptos 源码里的界面契约锚点与文案")

    r = sub.add_parser("run", help="启动应用、真点界面、跑完点击路径后关闭应用")
    r.add_argument("--exe", type=Path, default=Path(os.environ.get("AJA_E2E_EXE", "D:/jt/release/jobassistant-app.exe")))
    r.add_argument("--ui-only", action="store_true", help="只验界面契约与读库路径，不碰网络")
    r.add_argument("--keywords", default="Java")
    r.add_argument("--min-score", type=int, default=100)
    r.add_argument("--budget", type=int, default=180, help="抓取观察窗口（秒）")
    r.add_argument("--port", type=int, default=9222, help="WebView2 远程调试端口")
    r.add_argument("--out", type=Path, default=DEFAULT_OUT)

    args = ap.parse_args(argv)
    if args.cmd == "doctor":
        return doctor()
    if args.cmd == "contract":
        return contract_check()

    report = Report(mode="ui-only" if args.ui_only else "full")
    try:
        code = run_exe(report, args.exe.resolve(), ui_only=args.ui_only, keywords=args.keywords,
                       min_score=args.min_score, budget=args.budget, port=args.port)
    except LaunchError as e:
        # 环境不满足前置条件：写 2，与「断言失败」的 1 区分开
        report.add("应用启动并暴露 CDP", False, str(e))
        code = 2
    except Exception as e:  # noqa: BLE001 — 驱动层的任何异常都要落进报告，不能只剩栈
        report.add("驱动执行完成", False, f"{type(e).__name__}: {e}")
        code = 2
    report.dump(args.out)
    total = len(report.checks)
    passed = total - len(report.failed)
    print(f"[summary] {passed}/{total} 通过 · 退出码 {code}")
    if report.failed:
        print("[failed]")
        for c in report.failed:
            print(f"  - {c.name}：{c.detail}")
    return code


if __name__ == "__main__":
    sys.exit(main())
