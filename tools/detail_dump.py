"""打开一条详情页，把决定字段完整度的 DOM 原文打出来。

用途：`scrape_probe.py` 报「某字段缺失」时，判断是**页面本身没有**（数据缺口，
接受）还是**选择器没覆盖到**（代码缺陷，要修）。两者处置完全不同，不能靠猜。

    python tools/detail_dump.py boss:78952ac10e88740e0nx-3Nm_ElNZ
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scraper"))

import browser  # noqa: E402
import sidecar  # noqa: E402


def _save_root_text(page) -> str:
    return (page.locator("body").inner_text() or "").strip()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("job_ids", nargs="+", help="job_id，形如 boss:<encryptJobId>")
    args = ap.parse_args(argv)

    from patchright.sync_api import sync_playwright

    with sync_playwright() as p:
        ctx = browser.launch_context(p, sidecar.data_dir(), lambda level, msg: print(f"[{level}] {msg}"))
        page = ctx.new_page()
        try:
            for job_id in args.job_ids:
                jid = job_id.split(":", 1)[-1]
                url = f"https://www.zhipin.com/job_detail/{jid}.html"
                print(f"\n{'=' * 70}\n{url}")
                page.goto(url, wait_until="domcontentloaded", timeout=30000)
                page.wait_for_timeout(2500)

                for sel in (".sider-company", ".business-info-box"):
                    loc = page.locator(sel)
                    if loc.count() == 0:
                        print(f"\n[{sel}] 选择器没匹配到任何元素")
                        continue
                    print(f"\n[{sel}]\n" + (loc.first.inner_text() or "").strip()[:900])

                for sel in ("li.res-time", "li.company-fund", "li.company-name", "li.company-type"):
                    loc = page.locator(sel)
                    print(f"  {sel}: count={loc.count()}"
                          + (f" text={loc.first.inner_text()!r}" if loc.count() else ""))

                # 兜底证据：整个 body 里有没有「成立」「融资」「轮」这些字样。
                # 有字但选择器没命中 = 选择器要修；连字都没有 = 页面本身缺数据。
                body = _save_root_text(page)
                print(f"\n[body 含「成立」] {'成立' in body}")
                idx = body.find("成立")
                if idx >= 0:
                    print(f"[body 片段] {body[max(0, idx - 120):idx + 120]!r}")
                print(f"[body 含「轮」] {'轮' in body}  [含「融资」] {'融资' in body}")

                # HR 活跃度取证（exclude_hr_active_days 规则依赖它）
                print("\n[HR 活跃度]")
                for sel in (
                    ".job-boss-info",
                    ".boss-info",
                    ".boss-active-time",
                    ".job-boss-info .boss-active-time",
                    ".job-boss-info .name",
                    ".job-boss-info .boss-info-time",
                    ".info-public em",
                    ".job-detail-box .boss-info-attr",
                ):
                    loc = page.locator(sel)
                    print(f"  {sel}: count={loc.count()}"
                          + (f" text={loc.first.inner_text()!r}" if loc.count() else ""))
                for word in ("活跃", "刚刚活跃", "今日活跃", "本周活跃", "在线", "分钟前", "小时前", "天前"):
                    print(f"  body 含 {word!r}: {word in body}")
                idx = body.find("活跃")
                if idx >= 0:
                    print(f"  [活跃 片段] {body[max(0, idx - 100):idx + 100]!r}")

        finally:
            page.close()
            ctx.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
