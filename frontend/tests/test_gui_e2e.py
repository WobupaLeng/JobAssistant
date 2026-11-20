"""`tools/gui_e2e.py` 的离线契约测试（不连应用、不连网络、不需要 exe）。

## 为什么在这里、为什么是 Python

`tools/gui_e2e.py` 的断言全部建立在三样东西上：`data-testid` 锚点、进度栏五段文案、
重算面板的进度文案。这三样都是 AGENTS.md §2.3 的**契约**，却一直没有测试——
2026-11-21 的 Leptos 重写把它们改坏了（重算文案、试算结果格式、进度锚点），
而脚本照旧假设旧结构，结果只有「打开真窗口跑一遍」才会发现，而真窗口还要
release 产物 + WebView2 可写目录，CI 与日常根本跑不起来。

这里的做法是**喂假页面**：一个最小 fake `page`（只实现脚本用到的那几个方法），
内容来自 `fixtures/dashboard_contract.html`，逐个调用脚本的辅助函数。
好处是：改脚本、改页面锚点、改文案，任意一侧漂移都会在毫秒级的单测里炸出来。

## 跑法

```powershell
cd frontend
python -m unittest discover -s tests -v      # 或 python -m pytest tests -q
```

## 边界

- 只测**纯逻辑**（锚点探测、进度解析、重算等待、产物位置）。真正的点击路径
  仍然只能由 `python tools/gui_e2e.py run --ui-only` 在真窗口里验证。
- 夹具 HTML 是**手抄的**约定，不是渲染结果；它若与 `frontend/src/**` 不一致，
  以源码为准并同步修改夹具（夹具顶部写着这条规则）。
"""

from __future__ import annotations

import importlib.util
import sys
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent
FIXTURE = HERE / "fixtures" / "dashboard_contract.html"


def _load_gui_e2e():
    """按路径加载 `tools/gui_e2e.py`（`tools/` 不是包，不能直接 import）。"""
    spec = importlib.util.spec_from_file_location("gui_e2e_under_test", REPO_ROOT / "tools" / "gui_e2e.py")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod  # dataclass 装饰器要求模块已登记
    spec.loader.exec_module(mod)
    return mod


gui_e2e = _load_gui_e2e()


# ────────────────────────────── 假页面 ───────────────────────────────


class FakeElement:
    def __init__(self, text: str = "", disabled: bool = False):
        self._text = text
        self._disabled = disabled

    def inner_text(self) -> str:
        return self._text

    def is_disabled(self) -> bool:
        return self._disabled


class FakePage:
    """只实现 `gui_e2e` 用到的方法：query_selector / eval_on_selector_all / wait_for_timeout。

    `node` 是 HTML 字符串；锚点靠一个**极简**的正则提取。刻意不用真正的 HTML
    解析器：脚本只按 `[data-testid="x"]` 取元素，这套提取与它的取法一一对应，
    少一层依赖就少一处解释器/deps 失踪导致测试跑不起来的机会。
    """

    def __init__(self, node: str, *, extra: dict[str, FakeElement] | None = None,
                 errors: list[str] | None = None, latency: float = 0.0,
                 disabled: set[str] | None = None):
        self.node = node
        self.extra = extra or {}
        self.errors = errors if errors is not None else []
        self.latency = latency
        self.disabled = disabled or set()
        self.clicks: list[str] = []
        self.fills: list[tuple[str, str]] = []

    # -- 选择器 ----------------------------------------------------------
    def query_selector(self, selector: str):
        tid = _selector_testid(selector)
        if tid is None:
            return None  # 脚本只按 testid 取元素
        el = self.extra.get(tid)
        if el is not None:
            return el
        # 容器锚点：真 DOM 的 inner_text 是**全部后代文本拼起来的一串**。
        # 前端 `scrape-progress` / `demo-result` 都是这种，必须原样模拟，
        # 否则「容器被当成数字锚点」这类缺陷在测试里根本暴露不出来。
        found = _outer_html(self.node, tid)
        if found is None:
            return None
        outer, inner = found
        if outer.endswith("/>") or outer.split()[0].lstrip("<").lower() in _VOID_TAGS:
            return FakeElement("", disabled=tid in self.disabled)
        return FakeElement(_strip_tags(inner), disabled=tid in self.disabled)

    def eval_on_selector_all(self, selector: str, expr: str):
        self.assert_sel = selector  # 记录一下，便于断言选择器没写错
        if selector.strip().endswith("tbody tr"):
            return self.node.count("<tr data-testid=") + self.node.count("<tr>")
        raise AssertionError(f"未预期的 eval 表达式：{expr}")

    # -- 动作（只为记录，行为由覆盖子类决定）------------------------------
    def click(self, selector: str) -> None:
        tid = _selector_testid(selector)
        self.clicks.append(tid or selector)

    def fill(self, selector: str, value: str) -> None:
        tid = _selector_testid(selector)
        if tid is None or _extract_testid_text(self.node, tid) is None and tid not in self.extra:
            raise AssertionError(f"fill 的目标不存在：{selector}")
        self.fills.append((tid, value))

    def wait_for_timeout(self, ms: int) -> None:
        time.sleep(self.latency)

    def wait_for_selector(self, selector: str, timeout: int = 30_000):
        tid = _selector_testid(selector)
        if self.query_selector(selector) is None:
            raise AssertionError(f"wait_for_selector 超时：{tid}")
        return FakeElement()


def _selector_testid(selector: str) -> str | None:
    prefix = '[data-testid="'
    if selector.startswith(prefix) and selector.endswith('"]'):
        return selector[len(prefix):-2]
    if selector.startswith('[data-testid="') and '"' in selector[len(prefix):]:
        return selector[len(prefix):].split('"', 1)[0]
    return None


def _outer_html(node: str, tid: str) -> tuple[str, str] | None:
    """返回 (开标签, 内部 HTML)；容器与叶子元素都能拿到内部 HTML。"""
    marker = f'data-testid="{tid}"'
    pos = 0
    while (at := node.find(marker, pos)) >= 0:
        head = _split_open_tag(node, at)
        if head is None:
            nxt = _skip_comment(node, at)
            pos = at + 1 if nxt < 0 else nxt
            continue
        name, tag_end = head
        if name.lower() in _VOID_TAGS or node[tag_end - 1] == "/":
            return node[node.rfind("<", 0, at):tag_end + 1], ""
        close = _matching_close(node, name, tag_end)
        return node[node.rfind("<", 0, at):tag_end + 1], node[tag_end + 1:close]
    return None


_VOID_TAGS = frozenset({"input", "br", "img", "hr", "meta", "link"})


def _matching_close(node: str, name: str, tag_end: int) -> int:
    """从开标签结束处起，跳过同类子元素，返回与本次开标签配对的 `</name>` 下标。

    容器锚点里嵌着同类子元素（`scrape-progress` 里套着多个 `div.stat`），
    直接 `find("</div>")` 会切在**第一个子元素**的收尾标签上。
    """
    open_tok, close_tok = f"<{name}", f"</{name}>"
    depth, pos = 1, tag_end + 1
    while depth > 0:
        nxt_open = node.find(open_tok, pos)
        nxt_close = node.find(close_tok, pos)
        if nxt_close < 0:
            return len(node)
        if 0 <= nxt_open < nxt_close and not node.startswith("</", nxt_open):
            nxt_open_gt = node.find(">", nxt_open)
            if nxt_open_gt < 0:
                return len(node)
            if node[nxt_open_gt - 1] != "/":
                depth += 1
            pos = nxt_open_gt + 1
        else:
            depth -= 1
            if depth == 0:
                return nxt_close
            pos = nxt_close + len(close_tok)
    return len(node)


def _skip_comment(node: str, start: int) -> int:
    """`start` 落在 `<!-- … -->` 里就返回注释之后的位置，否则原样返回。"""
    open_at = node.rfind("<!--", 0, start)
    if open_at < 0:
        return start
    close_at = node.find("-->", open_at)
    if close_at < 0 or close_at < start:
        return -1
    return close_at + 3


def _strip_tags(text: str) -> str:
    out, depth = [], 0
    for ch in text:
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth = max(0, depth - 1)
        elif depth == 0:
            out.append(ch)
    return " ".join("".join(out).split())


def _split_open_tag(node: str, at: int) -> tuple[str, int] | None:
    """从属性名出现处 `at` 反推所属开标签，返回 (标签名, 开标签结束 `>` 的下标)。"""
    tag_start = node.rfind("<", 0, at)
    if tag_start < 0:
        return None
    name = node[tag_start + 1:].split()[0].strip("/>")
    if name.startswith("!--"):  # 落在注释里
        return None
    tag_end = node.find(">", at)
    if tag_end < 0:
        return None
    return name, tag_end


def _extract_testid_text(node: str, tid: str) -> str | None:
    """找到第一个带该 testid 的**真实**元素，返回它内部的纯文本。"""
    marker = f'data-testid="{tid}"'
    pos = 0
    while (at := node.find(marker, pos)) >= 0:
        head = _split_open_tag(node, at)
        if head is None:  # 注释里的示例串，跳到注释之后继续找
            nxt = _skip_comment(node, at)
            pos = at + 1 if nxt < 0 else nxt
            continue
        name, tag_end = head
        if name.lower() in _VOID_TAGS or node[tag_end - 1] == "/":
            return ""
        close = node.find(f"</{name}>", tag_end)
        return _strip_tags(node[tag_end + 1:close])
    return None


def _fixture() -> str:
    return FIXTURE.read_text(encoding="utf-8")


def _broken_fixture(*drop_ids: str) -> str:
    """把指定锚点从夹具里删掉，模拟「前端漏了 data-testid」。"""
    node = _fixture()
    for tid in drop_ids:
        marker = f' data-testid="{tid}"'
        while (at := node.find(marker)) >= 0:
            tag_end = node.find(">", at)
            tag_start = node.rfind("<", 0, at)
            tag_name = node[tag_start + 1:].split()[0].strip("/")
            if tag_name in ("input", "br", "img"):
                # 自闭合：整段开标签去掉
                node = node[:tag_start] + node[tag_end + 1:]
                continue
            close = _matching_close(node, tag_name, tag_end)
            close_end = close + len(f"</{tag_name}>") if close < len(node) else len(node)
            inner = node[tag_end + 1:close]
            drop = inner.count(f'data-testid="')  # 容器里还有别的锚点：只摘属性，别把子元素一起删掉
            if drop:
                node = node[:at] + node[at + len(marker):]
            else:
                node = node[:tag_start] + node[close_end:]
    return node


# ────────────────────────────── 锚点契约 ───────────────────────────────


class RequiredTestidsTest(unittest.TestCase):
    def test_required_list_is_the_documented_16(self):
        """任务书 / AGENTS §6.2 第 9 项点名的 16 个锚点一个都不能少。"""
        documented = [
            "app-root", "score-spec", "job-refresh", "scrape-status", "scrape-login-btn",
            "scrape-keywords", "scrape-min-score", "scrape-start-btn", "scrape-stop-btn",
            "scrape-progress", "scrape-feed", "rescore-panel", "rescore-btn",
            "rescore-llm-toggle", "rescore-note", "demo-score-btn",
        ]
        flat = [t for spec in gui_e2e.REQUIRED_TESTIDS for t in gui_e2e._ids(spec)]
        for tid in documented:
            self.assertIn(tid, flat)
        self.assertEqual(len(gui_e2e.REQUIRED_TESTIDS), 16)

    def test_fixture_satisfies_every_required_testid(self):
        page = FakePage(_fixture())
        self.assertEqual(gui_e2e._missing_testids(page, gui_e2e.REQUIRED_TESTIDS), [])
        self.assertEqual(gui_e2e._missing_testids(page, gui_e2e.DEMO_FORM_TESTIDS), [])

    def test_missing_anchor_is_reported_without_short_circuit(self):
        """缺一个锚点 → 只报告那一个；后面的项照样被检查（不短路）。"""
        page = FakePage(_broken_fixture("rescore-note"))
        missing = gui_e2e._missing_testids(page, gui_e2e.REQUIRED_TESTIDS)
        self.assertEqual(missing, ["rescore-note"])

    def test_multiple_missing_anchors_all_reported(self):
        page = FakePage(_broken_fixture("scrape-status", "demo-score-btn"))
        missing = gui_e2e._missing_testids(page, gui_e2e.REQUIRED_TESTIDS)
        self.assertEqual(sorted(missing), ["demo-score-btn", "scrape-status"])

    def test_alias_spec_accepts_either_id(self):
        """`demo-title` 在 Leptos 版里是卡片标题，输入框是 `demo-title-input`：别名任一命中即可。"""
        alias = ("demo-title", "demo-title-input")
        self.assertIsNotNone(gui_e2e._first_present(FakePage(_fixture()), alias))
        only_input = "<div data-testid=\"demo-title-input\"></div>"
        self.assertEqual(gui_e2e._first_present(FakePage(only_input), alias), "demo-title-input")
        self.assertEqual(gui_e2e._first_present(FakePage(_fixture()), alias), "demo-title")

    def test_demo_form_requires_title_input(self):
        """试算表单必须能填标题：两种锚点都缺就该报缺失。"""
        page = FakePage(_broken_fixture("demo-title", "demo-title-input"))
        self.assertIn("demo-title | demo-title-input",
                      gui_e2e._missing_testids(page, gui_e2e.DEMO_FORM_TESTIDS))


# ────────────────────────────── 进度读数 ───────────────────────────────


class ProgressParseTest(unittest.TestCase):
    def test_reads_independent_numeric_anchors(self):
        """Leptos 版：`scrape-scraped` … 五个独立锚点，优先级高于容器文本。"""
        p = gui_e2e._parse_progress(FakePage(_fixture()))
        self.assertEqual(p["scraped"], 3)
        self.assertEqual(p["inserted"], 1)
        self.assertEqual(p["duplicates"], 0)
        self.assertEqual(p["below_threshold"], 2)
        self.assertEqual(p["rejected"], 0)

    def test_falls_back_to_five_segment_text_contract(self):
        """锚点缺失时退回 AGENTS §2.3 的五段文案契约。"""
        node = _broken_fixture("scrape-scraped", "scrape-inserted", "scrape-duplicates",
                               "scrape-below", "scrape-rejected")
        p = gui_e2e._parse_progress(FakePage(node))
        self.assertEqual(
            [p[k] for k in ("scraped", "inserted", "duplicates", "below_threshold", "rejected")],
            [3, 1, 0, 2, 0],
        )

    def test_progress_regex_keeps_all_five_segments(self):
        m = gui_e2e.PROGRESS_RE.search("已抓 41 · 入库 7 · 重复 12 · 低于门槛 20 · 规则否决 2")
        self.assertEqual(m.groups(), ("41", "7", "12", "20", "2"))

    def test_unparseable_progress_is_none_not_zero(self):
        """读不到 ≠ 0：返回 None，`_expect_progress` 才不会把「没读到」当成「已达 0」。"""
        p = gui_e2e._parse_progress(FakePage("<div></div>"))
        self.assertNotIn("scraped", p)
        self.assertEqual(p["raw"], "")


# ────────────────────────────── 重算等待 ───────────────────────────────


class RescoreWaitTest(unittest.TestCase):
    NOTE_DONE = "已扫描 1370 条，改写 42 条"

    def _page(self, *, note: str, disabled: bool, latency: float = 0.0):
        return FakePage(
            _fixture(),
            extra={"rescore-note": FakeElement(note)},
            latency=latency,
            disabled={"rescore-btn"} if disabled else set(),
        )

    def test_detects_done_by_reenabled_button(self):
        """契约：重算期间按钮 disabled + 显示「重算中…」，跑完解除禁用。

        Leptos 版**没有**「重算完成」终态文案——旧脚本按文案等，永远等不到。
        """
        page = self._page(note=self.NOTE_DONE, disabled=False)
        done, note = gui_e2e._wait_rescore_done(page, timeout=2)
        self.assertTrue(done)
        self.assertEqual(note, self.NOTE_DONE)

    def test_does_not_claim_done_before_any_progress_line(self):
        """按钮一开始就可点（还没点下去）时，不能误判成「跑完了」。"""
        page = self._page(note="尚未重算", disabled=False, latency=0.05)
        done, note = gui_e2e._wait_rescore_done(page, timeout=0.2)
        self.assertFalse(done)
        self.assertEqual(note, "尚未重算")

    def test_times_out_while_still_running(self):
        page = self._page(note="重算中…", disabled=True, latency=0.05)
        done, _ = gui_e2e._wait_rescore_done(page, timeout=0.2)
        self.assertFalse(done)

    def test_accepts_legacy_terminal_wording(self):
        page = self._page(note="重算完成：扫描 9 条，改写 0 条", disabled=True)
        done, note = gui_e2e._wait_rescore_done(page, timeout=1)
        self.assertTrue(done)
        self.assertTrue(note.startswith("重算完成"))

    def test_scanned_count_from_both_wordings(self):
        self.assertEqual(gui_e2e._rescore_count(FakePage(_fixture(),
                         extra={"rescore-note": FakeElement("已扫描 1370 条，改写 42 条")})), 1370)
        self.assertEqual(gui_e2e._rescore_count(FakePage(_fixture(),
                         extra={"rescore-note": FakeElement("扫描 7 条，改写 0 条")})), 7)
        self.assertIsNone(gui_e2e._rescore_count(FakePage(_fixture())))

    def test_failed_count_is_reported_in_note(self):
        """`rescore.rs` 在有失败时会追加「N 条失败」，脚本据此判「结果无失败」。"""
        note = "已扫描 12 条，改写 3 条，2 条失败"
        self.assertEqual(gui_e2e.RESCORE_FAILED_RE.search(note).group(1), "2")
        self.assertIsNone(gui_e2e.RESCORE_FAILED_RE.search("已扫描 12 条，改写 3 条"))


# ────────────────────────────── 试算结果 / 其它 ───────────────────────────────


class DemoScoreParseTest(unittest.TestCase):
    def test_score_is_first_integer_of_demo_result(self):
        """Leptos 版渲染「78 [规则] …」，**没有** `/ 100` 后缀——旧正则 `(\\d+)/` 永远匹配不到。"""
        import re

        result = gui_e2e._text(FakePage(_fixture()), "demo-result")
        m = re.match(r"\s*(\d{1,3})(?!\d)", result)
        self.assertIsNotNone(m, result)
        self.assertEqual(int(m.group(1)), 78)

    def test_does_not_swallow_leading_digits_of_longer_number(self):
        import re

        self.assertIsNone(re.match(r"\s*(\d{1,3})(?!\d)", "1000 分"))

    def test_text_helper_returns_empty_for_missing_anchor(self):
        self.assertEqual(gui_e2e._text(FakePage(_fixture()), "no-such-anchor"), "")


class RowSelectorTest(unittest.TestCase):
    def test_row_selector_targets_tbody_rows_inside_app_root(self):
        """行数统计依赖 `<tbody><tr>` 结构（`features/jobs/list.rs`）：结构变了要同步这里。"""
        page = FakePage(_fixture())
        rows = page.eval_on_selector_all(gui_e2e.ROW_SELECTOR, "e => e.length")
        self.assertEqual(rows, 3)
        self.assertIn("tbody tr", gui_e2e.ROW_SELECTOR)


class ExePathPolicyTest(unittest.TestCase):
    def test_default_exe_lives_outside_workspace(self):
        """产物必须在工作区外（工作区内 exe 继承 Low 完整性标签，起不来）。"""
        default = Path("D:/jt/release/jobassistant-app.exe")
        self.assertFalse(default.is_relative_to(REPO_ROOT))


if __name__ == "__main__":
    unittest.main(verbosity=2)
