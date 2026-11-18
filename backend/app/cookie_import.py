"""从本机 Chrome 的 Cookie 数据库直接读取登录态（不接管浏览器）。

为什么这么做：
Boss 直聘会检测 CDP / Playwright 接管（哪怕连接的是真实 Chrome），
在 1-3 秒内把页面重置为 about:blank。实测 --disable-blink-features、
CDP 接管、真实 Chrome 都被识破。

所以登录改走这条路：
1. 由程序用 subprocess 启动**完全干净的 Chrome**（只带 --remote-debugging-port，
   不带任何自动化注入），或直接让用户在自己常用的 Chrome 里登录；
2. 用户扫码登录、关掉窗口；
3. 程序从 Chrome 的 Cookies SQLite 库里读出该平台的 cookie，
   解密（Windows 上用 DPAPI）后存成 storage_state 供 Playwright 抓取时使用。

这样浏览器本体没有任何自动化痕迹，Boss 检测不到。
"""
from __future__ import annotations

import json
import os
import shutil
import sqlite3
import tempfile
import time
from pathlib import Path
from typing import Any

# Chrome/Edge 用户数据目录候选
_CHROME_PROFILE_DIRS = [
    Path(os.environ.get("LOCALAPPDATA", "")) / "Google/Chrome/User Data",
    Path(os.environ.get("LOCALAPPDATA", "")) / "Microsoft/Edge/User Data",
]

# Chrome 的 cookie 加密：Windows 上用 DPAPI，密钥在 Local State 里
_STATE_FILE = "Local State"


def _local_state_key(profile_root: Path) -> bytes | None:
    """取出 Chrome 的加密主密钥（Windows 下是 DPAPI 保护的 base64）。"""
    ls = profile_root / _STATE_FILE
    if not ls.is_file():
        return None
    try:
        data = json.loads(ls.read_text(encoding="utf-8"))
        enc = data.get("os_crypt", {}).get("encrypted_key")
        if not enc:
            return None
        import base64

        raw = base64.b64decode(enc)
        # 前面 5 字节是 "DPAPI" 标识
        if raw[:5] == b"DPAPI":
            raw = raw[5:]
        # 用 Windows DPAPI 解出明文主密钥
        import ctypes
        from ctypes import wintypes

        class DATA_BLOB(ctypes.Structure):
            _fields_ = [("cbData", wintypes.DWORD), ("pbData", ctypes.POINTER(ctypes.c_char))]

        def _blob(data: bytes) -> DATA_BLOB:
            buf = ctypes.create_string_buffer(data, len(data))
            return DATA_BLOB(len(data), ctypes.cast(buf, ctypes.POINTER(ctypes.c_char)))

        out = DATA_BLOB()
        crypt32 = ctypes.windll.crypt32
        kern = ctypes.windll.kernel32
        if not crypt32.CryptUnprotectData(
            ctypes.byref(_blob(raw)), None, None, None, None, 0, ctypes.byref(out)
        ):
            return None
        try:
            return ctypes.string_at(out.pbData, out.cbData)
        finally:
            kern.LocalFree(out.pbData)
    except Exception:
        return None


def _decrypt_value(enc: bytes, master_key: bytes | None) -> str | None:
    """解密一条 cookie 的 encrypted_value。

    Chrome Windows 的格式（v10）：
        b"v10" + nonce(12) + AES-256-GCM(ciphertext+tag)
    AES-GCM 的 AAD 为空，密钥是 DPAPI 解出来的 32 字节主密钥。

    解密后的明文在 Chrome 80+ 里带 **32 字节的 domain SHA256 前缀**，
    必须剥掉，否则会得到一堆乱码前缀（之前踩过这个坑：
    '\\x1cv\\x12V^B>SrypVp6t...' 这种值会被 Playwright 判为非法 cookie）。
    """
    try:
        if enc[:3] in (b"v10", b"v11"):
            if not master_key:
                return None
            from cryptography.hazmat.primitives.ciphers.aead import AESGCM

            nonce = enc[3:15]
            payload = enc[15:]
            plain = AESGCM(master_key).decrypt(nonce, payload, None)
            # 剥掉 32 字节 domain hash 前缀（Chrome 80+）
            if len(plain) > 32:
                tail = plain[32:]
                # 校验：前缀应当是 32 字节不可打印内容，尾部应可打印
                if tail and all(32 <= b < 127 or b in (9,) for b in tail[:64]):
                    return tail.decode("utf-8", "ignore")
            return plain.decode("utf-8", "ignore")
        if enc[:3] == b"v20":
            # App-Bound Encryption（Chrome 127+ 对部分 cookie 启用），暂不支持
            return None
        # 未加密
        return enc.decode("utf-8", "ignore")
    except Exception:
        return None


def find_cookie_dbs() -> list[Path]:
    """找出所有可读的 Chrome/Edge Cookies 数据库（含程序自己的专属 profile）。"""
    import os

    roots: list[Path] = []
    # 程序专属 profile（优先）
    own = os.environ.get("AJA_CHROME_PROFILE")
    if own and Path(own).is_dir():
        for rel in ("Default/Network/Cookies", "Default/Cookies", "Network/Cookies", "Cookies"):
            pp = Path(own) / rel
            if pp.is_file():
                roots.append(pp)
    # 系统已安装浏览器
    for root in _CHROME_PROFILE_DIRS:
        if not root.is_dir():
            continue
        for prof in root.iterdir():
            if not prof.is_dir():
                continue
            for rel in ("Network/Cookies", "Cookies"):
                p = prof / rel
                if p.is_file():
                    roots.append(p)
    return roots


def _ro_copy(db: Path) -> Path | None:
    """把被浏览器锁住的 cookie 库复制出来（支持 Chrome 运行中读取）。"""
    import shutil
    import tempfile

    tmp = Path(tempfile.gettempdir()) / f"aja-cookies-{time.time_ns()}.db"
    try:
        with open(db, "rb") as src, open(tmp, "wb") as dst:
            shutil.copyfileobj(src, dst)
        return tmp
    except OSError:
        # 文件被独占锁定时，退而用 SQLite 的只读 + immutable 模式直连
        try:
            if tmp.exists():
                tmp.unlink()
        except Exception:
            pass
        return None


def read_cookies_from_profile(profile_dir: str | Path, domain_key: str) -> list[dict[str, Any]]:
    """从指定的（程序专属）Chrome profile 目录读取某平台的 cookie。

    Chrome 运行时会以独占方式锁住 Cookies 库，所以这里用逐字节拷贝的方式
    拿一份快照再读，Windows 下也通常可行（Chrome 的锁是共享/独占混合语义）。
    读不到就返回空列表，调用方按"尚未登录"处理。
    """
    import shutil
    import sqlite3
    import tempfile
    import time

    profile = Path(profile_dir)
    if not profile.is_dir():
        return []

    # 新版 Chrome: <profile>/Default/Network/Cookies；也兼容旧布局
    candidates = [
        profile / "Default" / "Network" / "Cookies",
        profile / "Default" / "Cookies",
        profile / "Network" / "Cookies",
        profile / "Cookies",
    ]
    db = next((c for c in candidates if c.is_file()), None)
    if db is None:
        return []

    master = _local_state_key(profile)
    tmp: Path | None = None
    try:
        # Chrome 运行中会独占锁住 Cookies 文件（WinError 32）。
        # 优先级：① SQLite immutable 只读打开原库（不拿写锁，Chrome 可继续写）；
        #        ② 不行再拷一份读；③ 都不行返回空。
        def _open_ro(path: Path) -> sqlite3.Connection:
            # Windows 盘符冒号在 SQLite URI 里必须转义，否则报 unable to open
            uri = path.resolve().as_uri() + "?mode=ro&immutable=1"
            return sqlite3.connect(uri, uri=True)

        conn = None
        try:
            conn = _open_ro(db)
            conn.execute("SELECT count(*) FROM cookies").fetchone()
        except Exception:
            if conn is not None:
                try:
                    conn.close()
                except Exception:
                    pass
                conn = None
            tmp = Path(tempfile.gettempdir()) / f"aja-prof-{time.time_ns()}.db"
            try:
                shutil.copyfile(db, tmp)
                conn = _open_ro(tmp)
            except OSError:
                return []

        conn.row_factory = sqlite3.Row
        try:
            rows = conn.execute(
                "SELECT host_key, name, value, encrypted_value, path, expires_utc, "
                "is_secure, is_httponly, samesite FROM cookies WHERE host_key LIKE ?",
                (f"%{domain_key}%",),
            ).fetchall()
        finally:
            try:
                conn.close()
            except Exception:
                pass

        out: list[dict[str, Any]] = []
        for r in rows:
            val = r["value"]
            if not val and r["encrypted_value"]:
                val = _decrypt_value(r["encrypted_value"], master) or ""
            if not val:
                continue
            # Playwright 对 cookie 字段要求很严：
            # - expires 必须是**整数**秒（小数会被拒：Invalid cookie fields）
            # - sameSite=None 必须配 secure=true，否则一样被拒
            exp = r["expires_utc"]
            expires = int(exp / 1_000_000 - 11644473600) if exp else -1
            if expires < 0:
                expires = -1
            ss = {0: "None", 1: "Lax", 2: "Strict"}.get(r["samesite"], "Lax")
            secure = bool(r["is_secure"])
            if ss == "None" and not secure:
                ss = "Lax"
            out.append(
                {
                    "name": r["name"],
                    "value": val,
                    "domain": r["host_key"],
                    "path": r["path"] or "/",
                    "expires": expires,
                    "httpOnly": bool(r["is_httponly"]),
                    "secure": secure,
                    "sameSite": ss,
                }
            )
        return out
    except Exception:
        return []
    finally:
        try:
            if tmp != db and Path(tmp).exists():
                Path(tmp).unlink()
        except Exception:
            pass
