"""PyInstaller 引导入口。

为什么不直接把 ``app/main.py`` 交给 PyInstaller：

PyInstaller 会把入口脚本当作**顶层模块**执行（``__name__ == "__main__"``，
``__package__`` 为 None），于是 ``main.py`` 里所有 ``from .xxx import``
相对导入都会抛：

    ImportError: attempted relative import with no known parent package

Python 直接 ``python -m app.main`` 时会正确设置 ``__package__='app'``，所以
开发态没问题，只有打包后才炸。这个入口脚本用**绝对导入**绕开该问题，
同时保证 ``app`` 与 ``scrapers`` 两个包都能被正常解析。

构建：``pyinstaller aja-backend.spec``（spec 里 Analysis 指向本文件）。
"""
import multiprocessing
import sys
from pathlib import Path

# 打包后 sys.path[0] 是 PyInstaller 的临时解包目录，源码根目录也需要在路径里，
# 否则 `import app` / `import scrapers` 找不到。
_root = Path(__file__).resolve().parent
if str(_root) not in sys.path:
    sys.path.insert(0, str(_root))


def main() -> None:
    # 必须在 patchright 启动前执行：把 compute_driver_executable 指向
    # 我们随包收集的 node driver，否则打包后 sync_playwright().start()
    # 会抛 FileNotFoundError: [WinError 2]。
    try:
        from app.browser import prepare_driver

        prepare_driver()
    except Exception:
        pass

    from app.main import main as backend_main

    backend_main()


if __name__ == "__main__":
    # PyInstaller + 多进程时（playwright 的 driver 会起子进程）必须 freeze_support，
    # 否则子进程会重复执行入口脚本，无限递归开窗口。
    multiprocessing.freeze_support()
    main()
