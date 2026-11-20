"""分层依赖校验：防止 `frontend/` 的模块结构随时间退化。

为什么要写这个脚本：分层规则（`pages → features → components → lib_ui`）
一旦写进文档就只能靠人自觉。而依赖方向错一次不会报错——只会让某个文件
偷偷依赖上层，半年后没人敢删任何东西。这里把规则变成可执行断言。

扫描的是 **Rust 源文件**（`.rs`），识别 `use crate::...` 与 `crate::...::` 路径。

规则（与 AGENTS.md 第 2.3 节对应）：
  R1  features/    不得依赖  pages/
  R2  components/  不得依赖  pages/ 或 features/
  R3  lib_ui/      不得依赖  其他任何层（含 api）
  R4  api/         不得依赖  pages/ 或 features/ 或 components/
  R5  pages/       不得依赖  api 之上的组装层（pages 是最上层，允许依赖下面所有层）
  R6  任何文件都不得出现已删除的 `styles.css`（已拆为 styles/ 目录）

`main.rs` / `lib.rs` 是根文件，不做层判定。

用法：
    python tools/check_layers.py                  # 检查 frontend/src
    python tools/check_layers.py --root <dir>     # 指定目录
退出码 0 = 通过，1 = 有违规。
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ROOT = REPO_ROOT / "frontend" / "src"

# `use crate::features::jobs::list;` / `crate::api::invoke(...)`
USE_RE = re.compile(r"""\bcrate::([a-z_][a-z0-9_]*)""")

# 层 -> 禁止依赖的层。pages 是最上层，允许依赖下面所有层。
RULES: dict[str, set[str]] = {
    "features": {"pages"},
    "components": {"pages", "features"},
    "lib_ui": {"api", "components", "features", "pages"},
    "api": {"components", "features", "pages"},
    "pages": set(),
}

LAYERS = set(RULES)


def layer_of(rel: Path) -> str | None:
    """文件相对 src 的第一段目录名即层名；`lib.rs` / `main.rs` 返回 None。"""
    parts = rel.parts
    if len(parts) < 2:
        return None
    return parts[0] if parts[0] in LAYERS else None


def check(root: Path) -> tuple[list[str], int]:
    problems: list[str] = []
    files = 0
    for f in sorted(root.rglob("*.rs")):
        if not f.is_file():
            continue
        files += 1
        rel = f.relative_to(root)
        src_layer = layer_of(rel)
        text = f.read_text(encoding="utf-8", errors="ignore")

        # R6：旧样式路径
        if "styles.css" in text:
            problems.append(f"{rel}: 引用了已删除的 styles.css，应改为 styles/index.css")

        if src_layer is None:
            continue
        banned = RULES[src_layer]

        for dst_layer in set(USE_RE.findall(text)):
            if dst_layer == src_layer or dst_layer not in LAYERS:
                continue
            if dst_layer in banned:
                problems.append(
                    f"{rel}: {src_layer}/ 不得依赖 {dst_layer}/"
                )
    return problems, files


def main() -> int:
    ap = argparse.ArgumentParser(description="校验 frontend 的分层依赖方向")
    ap.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    args = ap.parse_args()

    root: Path = args.root
    if not root.exists():
        print(f"[layers] 目录不存在：{root}")
        return 1

    problems, files = check(root)
    if problems:
        print(f"[layers] 失败：{len(problems)} 处违规（共检查 {files} 个 .rs 文件）")
        for p in problems:
            print("  -", p)
        return 1

    print(f"[layers] 通过：{files} 个 .rs 文件，依赖方向全部合规")
    return 0


if __name__ == "__main__":
    sys.exit(main())
