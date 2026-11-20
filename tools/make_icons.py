# -*- coding: utf-8 -*-
"""生成 Tauri 需要的应用图标（无外部素材依赖，只用 Pillow）。

用法：python tools/make_icons.py
产物：app/src-tauri/icons/{32x32,128x128,128x128@2x,256x256,512x512,1024x1024,icon}.png
      src-tauri/icons/icon.ico
"""
from __future__ import annotations

import sys
from pathlib import Path

try:
    from PIL import Image, ImageDraw
except ImportError:
    sys.exit("缺少 Pillow，请先执行：pip install pillow")

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "app" / "src-tauri" / "icons"
OUT.mkdir(parents=True, exist_ok=True)

S = 1024
PRIMARY = (16, 185, 129, 255)   # 翡翠绿
SECONDARY = (37, 99, 235, 255)  # 科技蓝
WHITE = (255, 255, 255, 255)


def build_icon() -> Image.Image:
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # 圆角底板
    d.rounded_rectangle([0, 0, S - 1, S - 1], radius=int(S * 0.22), fill=PRIMARY)

    # 简历/文档
    m = int(S * 0.24)
    d.rounded_rectangle([m, m, S - m, S - m], radius=int(S * 0.05), fill=WHITE)

    # 文本行
    lm = int(S * 0.36)
    lw = int(S * 0.055)
    for i in range(3):
        y = lm + i * int(S * 0.14)
        x2 = S - lm if i != 2 else S - lm - int(S * 0.14)
        d.rounded_rectangle([lm, y, x2, y + lw], radius=lw // 2, fill=SECONDARY)

    # 右下角对勾徽标
    cx, cy, rr = int(S * 0.72), int(S * 0.72), int(S * 0.16)
    d.ellipse([cx - rr, cy - rr, cx + rr, cy + rr], fill=PRIMARY)
    d.line(
        [
            (cx - int(rr * 0.5), cy),
            (cx - int(rr * 0.1), cy + int(rr * 0.45)),
            (cx + int(rr * 0.55), cy - int(rr * 0.4)),
        ],
        fill=WHITE,
        width=int(rr * 0.28),
        joint="curve",
    )
    return img


def main() -> None:
    img = build_icon()
    for size in (32, 128, 256, 512, 1024):
        img.resize((size, size), Image.LANCZOS).save(OUT / f"{size}x{size}.png")
    img.resize((256, 256), Image.LANCZOS).save(OUT / "128x128@2x.png")
    img.resize((256, 256), Image.LANCZOS).save(OUT / "icon.png")

    ico_sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    img.resize((256, 256), Image.LANCZOS).save(OUT / "icon.ico", format="ICO", sizes=ico_sizes)
    print(f"图标已生成到 {OUT}")


if __name__ == "__main__":
    main()
