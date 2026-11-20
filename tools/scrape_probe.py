"""直接驱动 sidecar 跑一轮抓取，产出字段完整度报告——不经 GUI、不用 CDP。

存在的理由：GUI E2E（`tools/gui_e2e.py`）只断言「链路通」，抓几条就够；
判断「DOM-only 通道是否丢了字段、能不能连续翻页」需要看**每一条**的 21 个字段。

跑法（必须用独立脚本，**不要** `| Select-Object -First N` 接长任务——会提前关管道杀掉子进程）：

    python tools/scrape_probe.py --keyword Java --pages 2 --out .tmp-test/scrape-probe.json
"""

from __future__ import annotations

import argparse
import json
import queue
import subprocess
import sys
import threading
import time
from collections import Counter
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SIDECAR = REPO_ROOT / "scraper" / "sidecar.py"

# ScrapedJob 的 21 个字段（core/src/sidecar/protocol.rs）里，DOM 通道应当能拿到的
EXPECT_FROM_CARD = ["job_id", "platform", "job_url", "title", "salary", "company", "location"]
EXPECT_FROM_DETAIL = ["jd_text", "experience", "degree", "company_industry",
                      "company_scale", "company_stage", "founded_years"]
# 卡片与详情页都没有来源的字段：出现缺失属预期，只做记录不判失败
KNOWN_GAPS = {"hr_active_status", "hr_name", "hr_position", "matched_keyword"}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="直接驱动 sidecar 抓取并统计字段完整度")
    ap.add_argument("--keyword", default="Java")
    ap.add_argument("--pages", type=int, default=2)
    ap.add_argument("--limit", type=int, default=8, help="最多回传多少条（详情页很慢）")
    ap.add_argument("--budget", type=int, default=300,
                    help="总观察窗口（秒）。**硬截止**：到点即发 scrape.stop 退出，不受单个详情页卡住影响")
    ap.add_argument("--out", type=Path, default=REPO_ROOT / ".tmp-test" / "scrape-probe.json")
    args = ap.parse_args(argv)

    # stderr 落文件而不是 PIPE：PIPE 只写不读会让管道写满、子进程阻塞在写 stderr 上
    err_path = args.out.with_suffix(".stderr.log")
    err_path.parent.mkdir(parents=True, exist_ok=True)
    err_file = err_path.open("w+", encoding="utf-8", errors="replace")
    proc = subprocess.Popen(
        [sys.executable, "-u", str(SIDECAR)],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err_file,
        text=True, encoding="utf-8", bufsize=1,
    )
    jobs: list[dict] = []
    logs: list[str] = []
    started = time.time()

    def send(obj: dict) -> None:
        try:
            proc.stdin.write(json.dumps(obj, ensure_ascii=False) + "\n")
            proc.stdin.flush()
        except (BrokenPipeError, ValueError):
            pass  # 子进程已退出

    # stdout 必须用读线程转成队列：`readline()` 是阻塞的，一旦某个详情页卡住，
    # 主线程就永远等在那里，`while time.time() < budget` 这层判断根本轮不到执行
    # （2026-11-21 实测：--budget 88 的命令跑到 104 秒还在等）。队列让超时真正生效。
    lines: queue.Queue[str | None] = queue.Queue()

    def pump() -> None:
        try:
            for raw in proc.stdout:
                lines.put(raw)
        finally:
            lines.put(None)  # 哨兵：stdout 关闭

    threading.Thread(target=pump, daemon=True).start()

    try:
        # ── ready 握手 ───────────────────────────────────────────
        # 必须从队列拿，**不能**再调 proc.stdout.readline()：读线程已经把这一行取走了，
        # 主线程再去读原始流会永久阻塞在第二行上——表现为「预算不生效、命令一直挂住」
        # （2026-11-21 实测：--budget 45 实跑 108s）。
        try:
            line = lines.get(timeout=30.0)
        except queue.Empty:
            print("[FAIL] sidecar 30 秒内没有发出 ready")
            return 1
        if line is None:
            print("[FAIL] sidecar 未握手就关闭了 stdout")
            return 1
        evt = json.loads(line)
        assert evt.get("event") == "ready", f"第一行不是 ready：{line[:200]}"
        print(f"[ready] pid={evt['data'].get('pid')}，握手 {time.time() - started:.1f}s")

        send({"id": "c1", "cmd": "scrape.start", "platform": "boss",
              "keywords": [args.keyword], "max_pages": args.pages, "min_score": 0})

        while True:
            remaining = args.budget - (time.time() - started)
            if remaining <= 0:
                logs.append(f"[预算耗尽] 已达 {args.budget}s，主动停止")
                print(f"[预算耗尽] 已达 {args.budget}s，主动停止")
                send({"id": "c2", "cmd": "scrape.stop"})
                break
            try:
                line = lines.get(timeout=min(remaining, 30.0))
            except queue.Empty:
                continue  # 回主循环重算剩余预算
            if line is None:
                break
            try:
                evt = json.loads(line)
            except json.JSONDecodeError:
                logs.append(f"[非 JSON] {line[:200]}")
                continue
            name = evt.get("event")
            if name == "job.scraped":
                jobs.append(evt["data"])
                j = evt["data"]
                print(f"[job {len(jobs)}] {j.get('title')} | {j.get('company')} | "
                      f"{j.get('salary')} | JD {len(j.get('jd_text') or '')} 字")
                if len(jobs) >= args.limit:
                    break
            elif name == "log":
                msg = evt["data"].get("msg", "")
                logs.append(f"[{evt['data'].get('level')}] {msg}")
                print(f"[log] {msg}")
            else:
                logs.append(f"[{name}] {json.dumps(evt, ensure_ascii=False)[:300]}")
    finally:
        # 收尾必须是「scrape.stop → 关 stdin」，缺一不可：
        # `scrape.stop` 只置位停止标志、结束抓取线程，主循环仍在 `for raw in sys.stdin`
        # 等输入，**进程不会自己退出**（这是设计，Rust 侧靠 scrape_close 关 stdin 收工）。
        # 早先这里只发 stop 就 `proc.wait(20)`，于是每次都等满 20 秒再 kill——
        # 表现为「命令卡住不出结果」（2026-11-21 实测）。
        try:
            send({"id": "c9", "cmd": "scrape.stop"})
        except Exception:  # noqa: BLE001 — 进程可能已经没了
            pass
        try:
            proc.stdin.close()  # 等价于 Rust 的 scrape_close：主循环读到 EOF 才会退出
        except Exception:  # noqa: BLE001
            pass
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
        try:
            err_file.seek(0)
            err = err_file.read()
            err_file.close()
        except Exception:  # noqa: BLE001
            err = ""
        if err and err.strip():
            logs.append(f"[stderr] {err[-2000:]}")

    # ── 统计 ─────────────────────────────────────────────────────
    elapsed = round(time.time() - started, 1)

    def has(j: dict, k: str) -> bool:
        """字段有没有「被回传」。

        注意不能用真值判断：`founded_years = 0` 表示「今年才成立」，是合法数据，
        若按真值算会被误报成缺失。这里只看键在不在、值非 None。
        """
        return k in j and j[k] is not None and j[k] != ""

    present = Counter()
    for j in jobs:
        for k in j:
            if has(j, k):
                present[k] += 1

    n = len(jobs) or 1
    missing_card = {k: n - present[k] for k in EXPECT_FROM_CARD if present[k] < n}
    missing_detail = {k: n - present[k] for k in EXPECT_FROM_DETAIL if present[k] < n}
    jd_lens = [len(j.get("jd_text") or "") for j in jobs]

    report = {
        "elapsed_s": elapsed,
        "keyword": args.keyword,
        "pages_requested": args.pages,
        "count": len(jobs),
        "present": dict(present),
        "missing_card_fields": missing_card,
        "missing_detail_fields": missing_detail,
        "jd_chars": {"min": min(jd_lens) if jd_lens else 0,
                     "max": max(jd_lens) if jd_lens else 0},
        "locations": sorted({j.get("location") for j in jobs if j.get("location")}),
        "jobs": jobs,
        "logs": logs,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")

    print(f"\n[结果] {len(jobs)} 条 / {elapsed}s，JD {report['jd_chars']} 字")
    print(f"[卡片字段缺失] {missing_card or '无'}")
    print(f"[详情字段缺失] {missing_detail or '无'}")
    print(f"[城市口径] {report['locations']}")
    print(f"[报告] {args.out}")

    if not jobs:
        print("[FAIL] 一条都没抓到")
        return 1
    return 0 if not missing_card and not missing_detail else 1


if __name__ == "__main__":
    sys.exit(main())
