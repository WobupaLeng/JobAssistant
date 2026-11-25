"""接管已在运行的 Chrome（profile 被旧进程占用时的兜底）。

真实场景：上次异常退出留下一个占着 `--user-data-dir` 的 Chrome，新起的
chrome.exe 会把启动请求移交过去后自己以 0 退出，CDP 端口永远不监听。
`start_cdp_chrome` 必须先找到那个旧进程并接管它的调试端口。

这里只测不依赖真实浏览器的部分：端口探活、接管句柄的 poll/terminate、
以及「查不到就返回 None」。
"""
from __future__ import annotations

import socket
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import browser  # noqa: E402


def test_tcp_alive_reports_closed_port() -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    assert browser._tcp_alive(port) is False


def test_reused_chrome_poll_is_dead_when_port_closed() -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    proc = browser.ReusedChrome(pid=1, port=port)
    assert proc.poll() == 0
    assert proc.wait(timeout=0.5) == 0


def test_find_profile_cdp_without_match_returns_none() -> None:
    # 不存在的 profile 路径：不可能有进程占用它
    assert browser._find_profile_cdp(r"C:\definitely-not-a-profile-目录") is None


def test_find_profile_cdp_never_raises_on_bad_input() -> None:
    # 空串 / 引号之类的脏值只能返回 None，不能把调用方带崩
    assert browser._find_profile_cdp("") is None
    assert browser._find_profile_cdp("'") is None


def main() -> int:
    failed = 0
    for name, fn in sorted(globals().items()):
        if not name.startswith("test_") or not callable(fn):
            continue
        try:
            fn()
            print(f"PASS {name}")
        except Exception as exc:  # noqa: BLE001
            failed += 1
            print(f"FAIL {name}: {type(exc).__name__}: {exc}")
    print(f"{failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())