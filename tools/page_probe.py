"""只验证「翻页后 DOM 通道还能不能取到卡片」——不拉详情，一条命令内跑完。

存在的理由：`scrape_probe.py` 是完整抓取（含详情页，每条 3~6 秒），第 1 页 15 条详情
就把预算耗光了，永远走不到第 2 页。而 `scraper/boss.py` 删掉接口通道后，翻页是否
仍然可用正是最需要证明的命题——它直接决定 DOM-only 改造是否成立。

做法：复用 boss.py 自己的 DOM 解析函数，只开搜索页 + 点下一页，不碰详情页。

    python tools/page_probe.py --keyword Java --pages 3
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scraper"))

import boss as boss_mod  # noqa: E402
import browser  # noqa: E402
import sidecar  # noqa: E402

LOGIN_COOKIES = ("bst", "zp_at", "wt2", "geek_zp_token")

def _dump_pagers(page) -> None:
    """找不到下一页按钮时，把页面上所有可能的分页容器打出来，区分两种原因：
    ① 页面确实只有 1 页；② 选择器过期了。"""
    for sel in (".options-pages", ".pagination", ".page", "[class*=page]"):
        try:
            loc = page.locator(sel)
            if loc.count() == 0:
                continue
            print(f"  [{sel}] count={loc.count()}")
            for i in range(min(loc.count(), 3)):
                txt = (loc.nth(i).inner_text() or "").strip().replace("\n", " ")
                print(f"    #{i} {txt[:120]!r}")
        except Exception:  # noqa: BLE001
            continue


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="只验证翻页后 DOM 卡片是否可取（不拉详情）")
    ap.add_argument("--keyword", default="Java")
    ap.add_argument("--pages", type=int, default=3)
    ap.add_argument("--budget", type=int, default=90, help="总预算（秒），到点即停")
    args = ap.parse_args(argv)

    started = time.time()
    from patchright.sync_api import sync_playwright

    with sync_playwright() as p:
        ctx = browser.launch_context(p, sidecar.data_dir(), lambda lv, m: print(f"[{lv}] {m}"))
        try:
            names = {c.get("name") for c in ctx.cookies("https://www.zhipin.com")}
            print(f"[登录 cookie] {sorted(names & set(LOGIN_COOKIES))}")
            if not (names & set(LOGIN_COOKIES)):
                print("[FAIL] 未登录，翻页测试无意义")
                return 1

            page = browser.find_page_for(ctx, "boss") or ctx.new_page()
            url = f"{boss_mod.SEARCH_BASE}?query={args.keyword}&city=101020100"
            page.goto(url, wait_until="domcontentloaded", timeout=30000)
            # 分页条通常比卡片晚渲染：先等卡片出来，再多等一会儿，否则会把
            # 「还没渲染」误判成「没有第 2 页」——这正是 2026-11-21 第一版脚本的错。
            page.wait_for_selector("ul.rec-job-list", timeout=20000)
            page.wait_for_timeout(6000)

            # 复用 boss.py 的 DOM 解析，避免测试用另一套选择器（那样测不出真实缺陷）
            scraper = boss_mod.BossScraper(
                cfg=None,
                log=lambda lv, m: None,
                should_stop=lambda: False,
                sleep=lambda s: False,
                on_job=lambda item: None,
            )

            seen: set[str] = set()
            for page_no in range(1, args.pages + 1):
                if time.time() - started > args.budget:
                    print(f"[预算耗尽] {args.budget}s")
                    break
                cards = scraper._parse_dom_cards(page)
                ids = {c.get("job_id") for c in cards if c.get("job_id")}
                new = ids - seen
                seen |= ids
                sample = (cards[0].get("title") if cards else "")
                print(f"[第 {page_no} 页] 卡片 {len(ids)} 条，新增 {len(new)} 条，"
                      f"累计 {len(seen)}，首条「{sample}」")
                if page_no == args.pages:
                    break
                # 复用 boss.py 的真实翻页实现（无限滚动），不另写一套，
                # 否则测的就不是产品代码了。
                before = len(seen)
                if not scraper._load_more_by_scroll(page, len(ids)):
                    print("[翻页] 滚动后没有新卡片，结束")
                    _dump_pagers(page)
                    break
                print(f"[翻页] 滚动加载成功（此前累计 {before}）")
        finally:
            ctx.close()

    print(f"[结果] 共取到 {len(seen)} 条不同岗位 / {time.time() - started:.1f}s")
    return 0 if len(seen) > 15 else 1


if __name__ == "__main__":
    sys.exit(main())
