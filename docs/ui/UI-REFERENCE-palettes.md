# UI-REFERENCE · 主题色板（7 套）

> 状态：参考稿，随实现同步。真源：`frontend-vue/src/lib_ui/palette.ts`。
> 适用：单窗外壳（`--surface-0` + 中性 `--shell-tint` + 主题色径向光斑）。主题色只改强调色与流光，不改外壳中性底色与几何。

## 1. 取色口径

- Figma 文件 / API 在当前环境不可读；色相骨架参考 Figma 品牌色、Figma UI3 与社区主流设计系统的常用档位，alpha 按当前外壳底色重新取值。
- 深色：`primary` 取 500–600 档，`primaryText` 取 300 档，保证深底上的小字对比。
- 浅色：`primary` 压到 600–800 档，避免主按钮发飘；`primaryText` 取 700–800 档。
- 主按钮前景按渐变两端的最差对比度选：亮底（`teal` / `moss` / `marigold` / `coral` 的深色）用 `#04120c`，暗底用 `#fff`。七套在两端都 ≥ 4.5:1。
- 外壳底层改用中性染色 `--shell-tint`：深色黑、浅色白，不跟主题色；`--palette-wash` 保留但当前外壳不消费。外壳之上再叠 `--primary` / `--accent` 光斑，光斑峰值取 `0.11`（原 `0.22` 的 50%），只画在背景层，不覆盖卡片内的 `good` / `warn` / `bad` 语义光带。
- 每套只给一个主色相 + 一个邻近辅助色；同一屏不引入第二套色相。

## 2. 七套色板

| id | 名称 | 深色 primary / strong / text | 浅色 primary / strong / text | 主按钮 fg 深 / 浅 | accent 深 / 浅 | wash 深 / 浅 |
| --- | --- | --- | --- | --- | --- | --- |
| `cobalt` | 钴蓝 | `#2563eb` / `#1d4ed8` / `#93c5fd` | `#2563eb` / `#1d4ed8` / `#1e40af` | `#fff` / `#fff` | `#06b6d4` / `#0891b2` | `0.22` / `0.14` |
| `teal` | 青碧 | `#14b8a6` / `#0d9488` / `#5eead4` | `#0f766e` / `#115e59` / `#115e59` | `#04120c` / `#fff` | `#38bdf8` / `#0284c7` | `0.20` / `0.13` |
| `moss` | 苔绿 | `#84cc16` / `#65a30d` / `#bef264` | `#4d7c0f` / `#3f6212` / `#3f6212` | `#04120c` / `#fff` | `#2dd4bf` / `#0f766e` | `0.20` / `0.12` |
| `marigold` | 金盏 | `#f59e0b` / `#d97706` / `#fcd34d` | `#b45309` / `#92400e` / `#92400e` | `#04120c` / `#fff` | `#fb7185` / `#e11d48` | `0.20` / `0.13` |
| `coral` | 珊瑚 | `#fb7185` / `#f43f5e` / `#fda4af` | `#e11d48` / `#be123c` / `#be123c` | `#04120c` / `#fff` | `#a855f7` / `#7e22ce` | `0.20` / `0.13` |
| `magenta` | 洋红 | `#c026d3` / `#a21caf` / `#f0abfc` | `#a21caf` / `#86198f` / `#86198f` | `#fff` / `#fff` | `#fb7185` / `#be123c` | `0.22` / `0.13` |
| `iris` | 鸢尾 | `#7c3aed` / `#6d28d9` / `#c4b5fd` | `#6d28d9` / `#5b21b6` / `#5b21b6` | `#fff` / `#fff` | `#d946ef` / `#a21caf` | `0.24` / `0.13` |

`marigold` 浅色取 700/800 档的暗琥珀，而不是 500 档：500 档配白字只有 3.19，压深后 5.02。`teal` / `coral` 深色用近黑字，因为 500 档对白字只有 2.49 / 2.69。

## 3. 外壳接线

- `applyPalette()` 把 `--primary*`、`--accent*`、`--palette-wash`、`--palette-swatch`、`--btn-primary-fg`、`--btn-primary-border` 写到 `<html>` 行内 style，深浅切换后重新调用一次。
- 当前 `.app-shell` 是「`--surface-0` 底 + 中性 `--shell-tint` 薄染 + 两层伪元素流光」：深色 `rgba(0,0,0,.5)`，浅色 `rgba(255,255,255,.7)`；`::before` 用 `--primary` / `--accent` / `--primary-strong` 画三块径向光斑（30s 漂移，`opacity .11`），`::after` 用 `--primary` 画一条 24s 斜向高光（`opacity .06`）；`prefers-reduced-motion: reduce` 时只保留静态渐变。流光画在外壳层，跨过顶栏与内容区，避免横向断层。
- `--palette-wash` 仍由 `applyPalette()` 写入，但当前外壳底层不消费；若后续恢复主题色玻璃底，再把同一层 wash 叠回玻璃底即可，色值不用重取。
- `.app-shell` 与 `.card`、`.chat-rail`、`.chat-detach` 一律不投影；岗位卡 `.job-card` 默认与悬停都不铺底，让外壳流光直接透上来，只保留 1px 高光边。`--surface-card` 仍按 `color-mix(primary 6%, surface-0)` 轻染，但只供页面/敏感库等非岗位卡使用；否则同一窗口会出现两套色相。
- `.job-card` 的已投递/不投递不再铺 `good-soft` / `bad-soft`，只把高光带切到对应语义色；选中态仍保留 `--primary-soft` 底，作为多选反馈。
- 调色板圆点取 `swatch`（可辨识的亮色，不一定是按钮主色）；`aria-pressed` 与 `data-testid="palette-<id>"` 保持不变。

## 4. 旧 id 迁移

- 旧 5 套（`emerald` / `ocean` / `violet` / `amber` / `rose`）与上一轮的 `indigo` / `lime` 已全部移除。
- `findPalette()` 对未知 id 回退到第一套 `cobalt`；`readPalette()` 返回 `cobalt`，下一次 `applyPalette()` 会把 `cobalt` 写回 `aja.palette`。不需要单独写迁移脚本。
- 若想保留旧 id，只能在 `PALETTES` 里加别名并保持 7 个圆点，不建议；旧 id 的色值与本轮新色不一致。

## 5. 增加第 8 套

1. `PaletteId` 追加 id。
2. `PALETTES` 追加 `dark` / `light` 两个完整 tone（9 个字段，含 `primaryFg`）。
3. `frontend-vue/tests/palette.test.ts` 的数量断言从 7 改 8。
4. 不需要改 CSS；`.app-shell` 的流光伪元素会自动消费新变量。

## 6. 滚动条

- 全局滚动条、岗位网格 `.job-grid`、运行日志 `.run-log-box`、投递记录 `.dl-rows` 与 `.log-rows` 统一消费 `--scroll-thumb` / `--scroll-thumb-hover` / `--scroll-track`。
- 三枚令牌由 `--primary` 经 `color-mix` 派生：深色 `62% / 86% / 14%`，浅色 `68% / 90% / 16%`；切换色板后自动跟随。
- 不再用 `--hairline-strong` / `--text-3` 作为滚动条颜色，避免切主题后仍是灰色。
