# UI-REFERENCE · 主题色板（7 套）

> 状态：参考稿，随实现同步。真源：`frontend-vue/src/lib_ui/palette.ts`。
> 适用：单窗外壳（半透明中性染色 `--shell-tint` + 主题色径向光斑 + 系统磨砂）。主题色只进流光、按钮、链接、滚动条与选中态，不改外壳底色与几何。

## 1. 取色口径

- Figma 文件 / API 在当前环境不可读；色相骨架参考 Figma 品牌色、Figma UI3 与社区主流设计系统的常用档位，alpha 按当前外壳底色重新取值。
- 深色：`primary` 取 500–600 档，`primaryText` 取 300 档，保证深底上的小字对比。
- 浅色：`primary` 压到 600–800 档，避免主按钮发飘；`primaryText` 取 700–800 档。
- 主按钮前景按渐变两端的最差对比度选：亮底（`teal` / `moss` / `marigold` / `coral` 的深色）用 `#04120c`，暗底用 `#fff`。七套在两端都 ≥ 4.5:1。
- 外壳底层固定用中性 `--shell-tint`（深色 `rgba(0,0,0,.5)` / 浅色 `rgba(255,255,255,.70)`），**不跟随主题色**；主题色只在上面叠 `--primary` / `--accent` 光斑，光斑峰值取 `0.022`（原 `0.22` 的 10%），只画在背景层，不覆盖卡片内的 `good` / `warn` / `bad` 语义光带。
- 每套只给一个主色相 + 一个邻近辅助色；同一屏不引入第二套色相。

## 2. 七套色板

| id | 名称 | 深色 primary / strong / text | 浅色 primary / strong / text | 主按钮 fg 深 / 浅 | accent 深 / 浅 | wash 深 / 浅 |
| --- | --- | --- | --- | --- | --- | --- |
| `cobalt` | 钴蓝 | `#2563eb` / `#1d4ed8` / `#93c5fd` | `#2563eb` / `#1d4ed8` / `#1e3a8a` | `#fff` / `#fff` | `#06b6d4` / `#0891b2` | `0.22` / `0.14` |
| `teal` | 青碧 | `#14b8a6` / `#0d9488` / `#5eead4` | `#0f766e` / `#115e59` / `#0d3f3b` | `#04120c` / `#fff` | `#38bdf8` / `#0284c7` | `0.20` / `0.13` |
| `moss` | 苔绿 | `#84cc16` / `#65a30d` / `#bef264` | `#4d7c0f` / `#3f6212` / `#2c470d` | `#04120c` / `#fff` | `#2dd4bf` / `#0f766e` | `0.20` / `0.12` |
| `marigold` | 金盏 | `#f59e0b` / `#d97706` / `#fcd34d` | `#b45309` / `#92400e` / `#6b2f0a` | `#04120c` / `#fff` | `#fb7185` / `#e11d48` | `0.20` / `0.13` |
| `coral` | 珊瑚 | `#fb7185` / `#f43f5e` / `#fda4af` | `#e11d48` / `#be123c` / `#7f0e2d` | `#04120c` / `#fff` | `#a855f7` / `#7e22ce` | `0.20` / `0.13` |
| `magenta` | 洋红 | `#c026d3` / `#a21caf` / `#f0abfc` | `#a21caf` / `#86198f` / `#6b166b` | `#fff` / `#fff` | `#fb7185` / `#be123c` | `0.22` / `0.13` |
| `iris` | 鸢尾 | `#7c3aed` / `#6d28d9` / `#c4b5fd` | `#6d28d9` / `#5b21b6` / `#4c1d95` | `#fff` / `#fff` | `#d946ef` / `#a21caf` | `0.24` / `0.13` |

`marigold` 浅色取 700/800 档的暗琥珀，而不是 500 档：500 档配白字只有 3.19，压深后 5.02。`teal` / `coral` 深色用近黑字，因为 500 档对白字只有 2.49 / 2.69。

## 3. 外壳接线

- `applyPalette()` 把 `--primary*`、`--accent*`、`--palette-swatch`、`--btn-primary-fg`、`--btn-primary-border` 写到 `<html>` 行内 style，深浅切换后重新调用一次；**不写 `--shell-tint`**。
- 当前 `.app-shell` 是「半透明中性底 + 两层伪元素流光」：底色固定取 `tokens.css` 的 `rgba(0,0,0,.5)` / `rgba(255,255,255,.70)`，不跟随色板。不再在下面叠不透明 `--surface-0`，桌面由 `haze` 的 `HostBackdropBrush` 在窗口内容下方做圆角模糊。`::before` 用 `--primary` / `--accent` / `--primary-strong` 画三块径向光斑（30s 漂移，`opacity .022`），`::after` 用 `--primary` 画一条 24s 斜向高光（`opacity .012`）；`prefers-reduced-motion: reduce` 时只保留静态渐变。流光画在外壳层，跨过顶栏与内容区，避免横向断层。
- 展开态 `.chat-detach` 只铺 4% 的 `--text-1` 极轻压暗/提亮；**浅色不再单独开 `backdrop-filter`**：Tauri 透明窗 + WebView2 里该层会合成到白色兜底面上，导致聊天区整块发白、把系统磨砂盖掉。磨砂由窗口底层 Haze 提供；深色保留原有 backdrop 行为。不再叠第二层 `--shell-tint`，也不铺 `--surface-0` 白色渐层。
- 浅色次级文字：`--text-2` 提到 `0.76`、`--text-3` 提到 `0.74`；语义红压到 `#7a1a1a`（错误条正文同值），已读绿压到 `#0f3d24`，警告文字压到 `#5a3b05`，避免亮红/亮绿/亮黄在粉底、灰底上糊成一片。
- 浅色 `primaryText` 统一再压深一档（上表浅色 text 列），在 `.70` 外壳、最深黑桌面下最差 ≥4.6；深色 `primaryText` 不动。
- 系统磨砂由 `backend-tauri` 的 `haze 0.3` 提供：`Haze::attach_hwnd` 在窗口内容下挂 `HostBackdropBrush`，半径 8px、额外模糊 20px、无 tint；必须由 `HazeState` 持有句柄。旧 `SetWindowCompositionAttribute` / Mica 链只在 `haze` 不支持或挂载失败时回退，不能再和 Haze 同时调用，否则会覆盖 `ACCENT_ENABLE_HOSTBACKDROP`。Windows「透明效果」仍建议开启；Haze 自身不依赖该开关的模糊结果。
- `--palette-wash` 当前不由 `applyPalette()` 写入，外壳也不消费；主题色只通过 `--primary` / `--accent` 进流光。
- `.app-shell` 与 `.card`、`.chat-rail`、`.chat-detach` 一律不投影；岗位卡 `.job-card` 默认与悬停都不铺底，让外壳流光直接透上来，只保留 1px 高光边。`--surface-card` 取中性 `--surface-0`，不再混入主题色；非岗位卡的页面/敏感库也保持中性底，避免同一窗口出现两套色相。
- `.job-card` 的已投递/不投递不再铺 `good-soft` / `bad-soft`，只把高光带切到对应语义色；选中态仍保留 `--primary-soft` 底，作为多选反馈。
- 调色板圆点取 `swatch`（可辨识的亮色，不一定是按钮主色）；`aria-pressed` 与 `data-testid="palette-<id>"` 保持不变。
- 滚动条只保留 thumb 的主题色：深浅两态的 `--scroll-track` 都是 `transparent`，`scrollbar-color` 第二值、全局与局部 `::-webkit-scrollbar-track` 都不再铺色。

## 4. 旧 id 迁移

- 旧 5 套（`emerald` / `ocean` / `violet` / `amber` / `rose`）与上一轮的 `indigo` / `lime` 已全部移除。
- `findPalette()` 对未知 id 回退到第一套 `cobalt`；`readPalette()` 返回 `cobalt`，下一次 `applyPalette()` 会把 `cobalt` 写回 `aja.palette`。不需要单独写迁移脚本。
- 若想保留旧 id，只能在 `PALETTES` 里加别名并保持 7 个圆点，不建议；旧 id 的色值与本轮新色不一致。

## 5. 增加第 8 套

1. `PaletteId` 追加 id。
2. `PALETTES` 追加 `dark` / `light` 两个完整 tone（8 个字段，含 `primaryFg`）。
3. `frontend-vue/tests/palette.test.ts` 的数量断言从 7 改 8。
4. 不需要改 CSS；`.app-shell` 的流光伪元素会自动消费新变量。

## 6. 滚动条

- 全局滚动条、岗位网格 `.job-grid`、运行日志 `.run-log-box`、投递记录 `.dl-rows` 与 `.log-rows` 统一消费 `--scroll-thumb` / `--scroll-thumb-hover` / `--scroll-track`。
- 三枚令牌由 `--primary` 经 `color-mix` 派生：深色 `62% / 86% / 14%`，浅色 `68% / 90% / 16%`；切换色板后自动跟随。
- 不再用 `--hairline-strong` / `--text-3` 作为滚动条颜色，避免切主题后仍是灰色。
