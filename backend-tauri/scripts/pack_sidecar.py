"""把 `scraper/` 打成免安装的 sidecar 可执行文件（PyInstaller onedir）。

产物：`backend-tauri/sidecar-dist/jobassistant-sidecar.exe`（约 130 MB 整目录）。
`tauri.conf.json` 的 `bundle.resources` 会把这个目录放进安装包，安装后位于 exe 同级的
`sidecar-dist/`；Rust 侧 `locate_bundled_sidecar` 命中它就不再调用本机 python。

用法（仓库根或任意目录均可）：

    python backend-tauri/scripts/pack_sidecar.py

前置：`pip install pyinstaller`（当前验证版本 6.22.3）。onedir 而不是 onefile：onefile
每次启动要解压上百 MB 到临时目录，而 sidecar 会反复重启（抓取/登录/聊天各一次）。
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRAPER = ROOT / "scraper"
ENTRY = SCRAPER / "sidecar.py"
PACK_ROOT = ROOT / "backend-tauri" / ".pack-sidecar"
OUT_DIR = ROOT / "backend-tauri" / "sidecar-dist"
APP_NAME = "jobassistant-sidecar"


def main() -> int:
    if not ENTRY.is_file():
        print(f"[pack] 找不到入口 {ENTRY}", file=sys.stderr)
        return 2

    for path in (PACK_ROOT, OUT_DIR):
        shutil.rmtree(path, ignore_errors=True)
    PACK_ROOT.mkdir(parents=True, exist_ok=True)

    cmd = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        "--log-level",
        "WARN",
        "--name",
        APP_NAME,
        "--onedir",
        "--paths",
        str(SCRAPER),
        # patchright 的 node driver 是数据文件，PyInstaller 不会自己收
        "--collect-all",
        "patchright",
        "--distpath",
        str(PACK_ROOT / "out"),
        "--workpath",
        str(PACK_ROOT / "build"),
        "--specpath",
        str(PACK_ROOT),
        str(ENTRY),
    ]
    print("[pack] " + " ".join(cmd))
    result = subprocess.run(cmd, cwd=str(ROOT))
    if result.returncode != 0:
        print(f"[pack] PyInstaller 失败：exit={result.returncode}", file=sys.stderr)
        return result.returncode

    built = PACK_ROOT / "out" / APP_NAME
    if not built.is_dir():
        print(f"[pack] 产物缺失：{built}", file=sys.stderr)
        return 3
    shutil.move(str(built), str(OUT_DIR))

    size_mb = sum(p.stat().st_size for p in OUT_DIR.rglob("*") if p.is_file()) / (1024 * 1024)
    exe = OUT_DIR / ("jobassistant-sidecar.exe" if sys.platform == "win32" else APP_NAME)
    print(f"[pack] 完成：{exe}（整目录 {size_mb:.1f} MB）")
    if not exe.is_file():
        print("[pack] 未找到主可执行文件", file=sys.stderr)
        return 4
    return 0


if __name__ == "__main__":
    raise SystemExit(main())