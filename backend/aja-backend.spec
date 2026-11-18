# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller 打包配置：把 Python 后端打成单个 exe，供 Tauri 作为 sidecar 调用。

产物：dist/aja-backend.exe
用法：
    cd backend
    pip install pyinstaller
    pyinstaller aja-backend.spec --noconfirm
"""

block_cipher = None

# ---------------------------------------------------------------------------
# 收集 patchright 的 node driver（node.exe + package/cli.js）。
# PyInstaller 不会自动收它，缺了它打包后一开浏览器就报：
#   FileNotFoundError: [WinError 2] 系统找不到指定的文件。
# 运行时由 app.browser.prepare_driver() 把 compute_driver_executable
# 指向 <解包目录>/patchright-driver。
# ---------------------------------------------------------------------------
import os
import sys as _sys

import patchright as _pr

_driver_src = os.path.join(os.path.dirname(_pr.__file__), 'driver')
_driver_datas = []
if os.path.isfile(os.path.join(_driver_src, 'node.exe')):
    _driver_datas = [(_driver_src, 'patchright-driver')]
    print(f'[spec] 收集 patchright driver: {_driver_src}')
else:
    print('[spec] 警告：没找到 patchright driver，打包后将无法启动浏览器')

a = Analysis(
    # 入口必须是 entry.py 而不是 app/main.py：
    # PyInstaller 把入口当顶层模块执行，app/main.py 里的
    # `from .browser import ...` 会报
    # "attempted relative import with no known parent package"。
    ['entry.py'],
    pathex=['.'],
    binaries=[],
    datas=_driver_datas,
    hiddenimports=[
        'uvicorn.logging',
        'uvicorn.loops',
        'uvicorn.loops.auto',
        'uvicorn.protocols',
        'uvicorn.protocols.http',
        'uvicorn.protocols.http.auto',
        'uvicorn.protocols.websockets',
        'uvicorn.protocols.websockets.auto',
        'uvicorn.lifespan',
        'uvicorn.lifespan.on',
        'app.scorer',
        'app.store',
        'app.browser',
        # patchright 是 Playwright 的反检测补丁版（Boss 风控绕过的关键），
        # 它内部动态导入 driver，必须显式声明，否则打包后运行时报
        # "playwright.cli 找不到" / "asyncio subprocess 不可用"
        'patchright',
        'patchright.sync_api',
        'patchright.async_api',
        'patchright._impl',
        'patchright._impl._api_structures',
        'patchright._impl._browser',
        'patchright._impl._browser_context',
        'patchright._impl._page',
        'patchright._impl._driver',
        'patchright._impl._transport',
        'scrapers.base',
        'scrapers.boss',
        'scrapers.zhilian',
        'scrapers.job51',
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        'tkinter', 'matplotlib', 'numpy', 'pandas', 'scipy',
        'PySide6', 'PyQt5', 'PyQt6',
        # 只保留 patchright，别把原版 playwright 也打进去（体积 + 混淆）
        'playwright',
    ],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)
pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.zipfiles,
    a.datas,
    [],
    name='aja-backend',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,          # 不弹黑框；日志走 stdout 由 Tauri 接管
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
