# `prototypes/ui-v1/` — 界面设计原型（UI_READY）

单文件、自包含的可运行原型。**不是产品代码**，`frontend-vue/` 落地时只从这里抄结构、令牌与交互口径，不 import 任何文件。

```powershell
# 直接双击打开即可；也可带查询参数进入指定自检状态
#   index.html?l=error&r=loading&c=connect-lost
Start-Process prototypes\ui-v1\index.html
```

## 1. 文件

| 文件 | 作用 |
|---|---|
| `index.html` | 唯一产物：全部样式、脚本、mock 数据内联；**外链 0**（无 `<script src>` / `<link>` / `@import` / `http(s)://`） |
| `states.md` | 状态机与逐条断言的文字口径（§12 各条裁决 + 走查步骤） |
| `_smoke.mjs` | 静态断言 A1–A54 + 几何推断（1200/1280/1440/1600 四档） |
| `_modes.mjs` | 页面模式矩阵 9 例 + 2 个死模式 |
| `_tri.mjs` | 交互三态：抓取 / 投递 / 投递状态标签 / 大模型打分禁用 / 卡片打招呼链路 |
| `_states.mjs` | 状态快照导出 |
| ~~`shots/`~~ | **已整体删除**（用户裁决）：旧图全停留在 v9 之前形态，与当前实现冲突，补拍又不可能，见 §3 |

## 2. 自检

```powershell
node prototypes\ui-v1\_smoke.mjs    # SMOKE OK · A1–A54
node prototypes\ui-v1\_modes.mjs    # MODES OK
node prototypes\ui-v1\_tri.mjs      # TRI OK
```

三个脚本**都不依赖浏览器**，只读 `index.html` 源码并做字符串/位置断言，因此能在无 GUI 环境下跑。

## 3. 截图已放弃（不要重试）

`shots/` 目录**已删除**。v10 起未再产出截图，本机 headless 浏览器在沙箱下直接崩溃：

- Edge：`network_sandbox.cc:591 拒绝访问 (0x5)`
- Chrome：`crashpad_client_win.cc:431 OpenProcess: 拒绝访问 (0x5)` + `platform_channel.cc:112 FATAL Check failed: . : 拒绝访问 (0x5)`

换 `$env:TEMP` 下的独立 `--user-data-dir`、加 `--no-sandbox --disable-features=Crashpad` 均无效。**经用户裁决转为源码级断言兜底**：A53（卡片打招呼方块几何契约）、A54（展开态列数不溢出）。视觉走查按 `states.md §12A11.5` 的 5 步人工清单执行。

## 4. 权威设计文档

原型只是**实现**，口径以 `docs/ui/` 为准：

| 文档 | 内容 |
|---|---|
| [`docs/ui/UI-SPEC-chat-detach-v3.md`](../../docs/ui/UI-SPEC-chat-detach-v3.md) | **当前有效**：聊天抽屉布局与交互（v3 → v10 累积裁决 R/S/T/U/V/W2/X） |
| [`docs/ui/UI-SPEC-legacy-baseline.md`](../../docs/ui/UI-SPEC-legacy-baseline.md) | 旧版基线盘点 + 已被 v3 废止的部分（三级展开、§9 Q-2 等） |
| [`docs/ui/UI-REFERENCE-legacy-notes.md`](../../docs/ui/UI-REFERENCE-legacy-notes.md) | 11 张裁切图的逐条说明 |

## 5. 修改约定

1. **改 `index.html` 必须同时更新 `states.md` 与对应断言**，否则 `_smoke.mjs` 会在下一轮变成假绿。
2. **每条新断言都要做反向验证**：故意改坏 → 必须 FAIL → 还原 → 必须 OK。改坏后仍通过说明断言写错了。
3. 控件 `data-testid` 命名 `<模块>-<功能>-<控件类型>`，动态 id 走 `tidSafe()`。
4. 禁止外链与硬编码 `font-size: NNpx`（字号只走 `--fs-*` 五档，已含 `--ui-scale`）。
5. 本目录**不进**产品构建；`frontend-vue/` 的 Vite 配置不引用它。
