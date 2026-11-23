# UI 规格：Liquid Glass 第二层与第三层（UI-SPEC-liquid-glass-l2-l3）

> 状态：`L2_SPEC_READY` / `L3_DECISION_READY` · 版本 1 · 2026-11-21
> 作者：Lane G【UI有稿设计】（**只读产出，未修改任何运行时代码**；全部实证都在 `.tmp-test/l23/`）
> 上游依据：`AGENTS.md` 6.2 第 2 项（"Liquid Glass 第二三层未做（唯一剩下的前端设计项）"）
> 参照文档：`frontend/.agents/skills/liquid-glass/SKILL.md`、`.../REFERENCE.md`、`.../scripts/generate_glass_map.js`
> 风格对齐：本仓库既有 `docs/ui/UI-SPEC-web-align.md`（666 行）。

本规格的每一句「能/不能」都标注了来源：`【实测】`= 本轮 headless Chrome 158 真跑并截图为证；`【读码】`= 只读源码得出结论；`【未验证】`= 施工时必须现场确认。第 6 节把三类集中列出，**施工时以第 6 节为准，不要凭印象**。

---

## 0. 阅读顺序与硬约束

1. **先读第 2 节参数表再动手**：第二层的数值不是「调到手感对」，每一条都在第 6 节有实测依据或明确标注未验证。
2. **`tokens.css` 只能追加、不许改既有值**：`--refract-edge` / `--refract-turbulence` 已在 `frontend/styles/tokens.css:48-49`（深）与 `:152-153`（浅）定义好，且注释写明了「浅底上形变看着像脏点，位移砍到 1/3」。第二层/第三层**消费**它们，不得在别的文件里另写一套像素数。
3. **CSS 文件里不许出现 `//!` 或 `///`**：本项目真被坑过——整份样式表被判非法并**静默丢弃**，现象是「变量生效、组件样式全无」。新增 CSS 一律 `/* … */`。
4. **第二层不许碰 `backdrop-filter` 的 `url()`**：见 2.2 的实测——Chromium 把 `backdrop-filter: url(#f) blur(18px)` 里的 `url()` **静默忽略**，只有模糊没有折射。第二层必须走「克隆层 + 普通 `filter: url()`」。
5. **第三层 canvas 必须 `pointer-events: none`**：`tools/gui_e2e.py` 用真实点击驱动（`ElementHandle.click()` 依赖命中测试），覆盖层吞事件会让 E2E 全红。
6. **不许把桩脚本放进 `frontend/src/`**：本规格的验证桩页一律在 `.tmp-test/l23/`，产物（PNG/PPM）同样留在那里。

---

## 1. 现状：第一层做到了什么

第一层 = 纯 CSS 玻璃（`backdrop-filter` + 渐变高光边 + 内外阴影）。**已完成，本规格不重做。**

### 1.1 第一层的四处「材质定义」

| 材质 | 文件:行 | 关键声明 | 观感职责 |
|---|---|---|---|
| 顶栏 | `frontend/styles/layout.css:34-35` | `backdrop-filter: blur(var(--blur-1)) saturate(var(--saturate-1))` | 24px 糊 + 1.6 饱和，**只在顶部一条** |
| 卡片 | `frontend/styles/layout.css:101-102` | `blur(var(--blur-2)) saturate(var(--saturate-2))` | 18px 糊，页面主体材质 |
| 弹层/抽屉 | `frontend/styles/drawer.css:48-49`、`frontend/styles/sensitive.css:50-51` | `blur(var(--blur-pop)) saturate(var(--saturate-2))` | 30px（浅色 36px）糊，最厚 |
| 遮罩 | `frontend/styles/drawer.css:18-19`、`frontend/styles/sensitive.css:24-25` | `blur(2px)` | 极薄，只压背景 |
| 表头（sticky） | `frontend/styles/jobs.css:483-484` | `blur(var(--blur-3)) saturate(var(--saturate-2))` | 12px，防穿透 |
| 自动投递控制条 | `frontend/styles/auto.css:23-24` | `blur(var(--blur-2)) saturate(var(--saturate-1))` | 18px |

### 1.2 第一层已经建好的「第二/三层接口」

| 令牌 | 深色值 | 浅色值 | 位置 | 语义（注释原文） |
|---|---|---|---|---|
| `--refract-edge` | `14px` | `5px` | `tokens.css:48` / `:152` | 「边缘最大像素位移。深色下可以给大一点（暗部容得下形变），浅色下超过 8px 就开始发晕」 |
| `--refract-turbulence` | `0.008` | `0.006` | `tokens.css:49` / `:153` | 湍流空间频率 |

`tokens.css:13-15` 已经把第三层的**架构**写死了（用户选定的方案 C）：

> · 亚克力层：`window-vibrancy` 由 Rust 侧施加，负责「透出桌面」——本文件不参与
> · CSS 层：本文件。`backdrop-filter` 模糊 + 渐变高光边框 + 内外阴影
> · WebGL 层：`glass.js`。只渲染**卡片之间**的真折射（抽屉盖列表、气泡盖背景），
>   因为系统合成层在前端 `readPixels` 里读不到，桌面本身折射不了

**第二层与第三层的分工因此是确定的**：第二层是**静态材质**（湍流微折射，随卡片一起出现/消失）；第三层是**动态透镜**（打开抽屉时，抽屉边缘对下面的列表做放大式折射）。

### 1.3 现状缺口（用 git grep 举证）

| 检查 | 命令 | 结果 |
|---|---|---|
| 有没有 `feTurbulence` 运行时引用 | `git grep -n feTurbulence -- frontend/src frontend/styles` | **零命中** |
| 有没有 `feDisplacementMap` | `git grep -n feDisplacementMap -- frontend/src frontend/styles` | **零命中** |
| 有没有 WebGL/canvas | `git grep -n -E 'webgl\|Canvas\|canvas\|HtmlCanvasElement' -- frontend/src` | **只命中** `frontend/src/lib_ui/motion.rs:35`（那是 `request_animation_frame`，与画布无关） |
| `web-sys` 有没有 canvas 能力 | `frontend/Cargo.toml:28-35` | 只有 `Window, Document, HtmlElement, Storage, Location, MediaQueryList` —— **没有 `HtmlCanvasElement`，第三层必须追加 features** |
| 抽屉打开用的是什么 | `frontend/src/lib_ui/motion.rs:44` `DRAWER_IN = "drawer-in-play"` + `drawer.rs:95` `play_next_frame(...)` | CSS `@keyframes drawer-in` 0.22s（`drawer.css:57-70`）；敏感库用 `is-open` + `transition: transform 220ms var(--ease-glass)`（`sensitive.css:53-60`） |
| 有没有 `transitionend` 可用 | `git grep -n -E 'transitionend\|animationend' -- frontend/src` | **零命中**（第三层不能依赖事件，见 3.6） |

**结论**：第二层与第三层是真正的**从零开始**，唯一的既有资产是 `tokens.css` 的两枚折射令牌、`generate_glass_map.js` 这个离线工具、以及 `REFERENCE.md` 里的光学数学。

---

## 2. 第二层规格：SVG 湍流折射（静态材质）

### 2.1 一句话定义

> 第二层 = **在玻璃元素内部叠一层「按 backdrop 反相平移的克隆层」，对克隆层施加 `filter: url(#…)`，滤镜链先做 `feTurbulence` 湍流位移、再做 `feDisplacementMap` 边缘斜面位移**。它对**静态画面**生效，不随指针移动而变化。

### 2.2 六问逐条回答

#### Q1：滤镜作用在哪些元素？

**只作用于三类「有边界、静止、面积小」的玻璃**，按优先级：

| 优先级 | 元素 | 旧版 DOM 锚点 | 理由 |
|---|---|---|---|
| P0 | **抽屉面板**（岗位详情 `.drawer`、敏感库 `.sensitive-panel`） | `frontend/src/features/jobs/drawer.rs:199`、`frontend/src/features/sensitive/panel.rs:215-216` | 面积最大、边界最清晰，折射最容易被看见；且一次只开一个，成本可控 |
| P1 | **卡片**（`.card`，尤其 `.job-card`） | `frontend/src/features/jobs/list.rs:99`（`section.card.job-list`）、`frontend/styles/jobs.css:196-199`（`.job-card`） | 数量多，但每个面积小；作为材质统一感的来源 |
| **不做** | 顶栏 `.topbar`、遮罩 `.drawer-backdrop` / `.sensitive-backdrop`、`.table thead th`、`auto` 控制条 | `layout.css:34`、`drawer.css:18`、`sensitive.css:24`、`jobs.css:483`、`auto.css:23` | 顶栏是长条（湍流位移会让顶部「晃」得像没对齐）；遮罩只有 2px 模糊，加湍流纯浪费；sticky 表头每帧重绘，代价与收益倒挂 |

**为什么不做「全屏 backdrop」**：全屏湍流等于给整个窗口加了一层滤镜，它与 Rust 侧亚克力（`app/src-tauri/src/lib.rs:78 fn apply_blur` → `apply_acrylic`）叠加后会让**桌面透出的部分也被扭曲**，而桌面内容不在 DOM 里、克隆不了——得到的不是折射，是「脏」。**【实测依据见 6.1-②：克隆层必须自己画出 backdrop，画不出就只能不做】**

#### Q2：与第一层 `backdrop-filter: blur()` 如何叠加而不打架 + 性能

**分层顺序（从远到近，z 轴自下而上）**：

```
[系统合成层] 桌面 + 亚克力（Rust apply_acrylic）      ← 第一层「透桌面」
      ↓
[元素自身] backdrop-filter: blur(N) saturate(S)        ← 第一层「糊」
      ↓
[元素背景] background: var(--glass-N) + 渐变 tint       ← 第一层「材质色」
      ↓
[克隆层] position:absolute; inset:0; overflow:hidden    ← 第二层容器（不糊）
   └─ .glass-clone: inset:-P; filter: url(#…); transform: translate3d(-x,-y,0)
      ↓
[高光边] .card::before / box-shadow: inset …            ← 第一层「高光」
[内容]   文字 / 控件，z-index 最高，pointer-events 正常
```

**铁律三条（都是实测换来的）**：

1. **不要把 `url()` 拼进 `backdrop-filter`**。【实测】`backdrop-filter: url(#turb8) blur(18px) saturate(1.5)` 与「完全不写 backdrop-filter」的渲染结果几乎一致（`.tmp-test/l23/chain.html` 第 1 格 vs 第 4 格）——`url()` 被静默吃掉，只剩模糊。
2. **克隆层上只用普通 `filter`，不写 `backdrop-filter`**。克隆层自己画背景（见 Q5），所以它不需要再取一次 backdrop；两层 backdrop 叠在一起 = 取两次背景 = 双倍光栅化。
3. **克隆层必须落在「元素自己的 `overflow:hidden` + `border-radius`」里面**，否则圆角处会露出方形位移。已有 `.card` 没写 `overflow:hidden`（`layout.css:94-106`）——**第二层要求给承载克隆层的容器补 `overflow: hidden`**，这是一处必要的样式改动，见 4.2。

**性能**：`feTurbulence` 是**逐像素噪声生成**，代价与「被滤镜的面积 × octaves 数」成正比。【实测】`feTurbulence` + `feDisplacementMap` 的**样式重算**几乎免费（`MODE=none 0.017ms` / `MODE=filter 0.019ms` 每帧，`.tmp-test/l23/perf3.html`），开销全部在**光栅化**。因此预算按面积给：

| 元素 | 单块尺寸上限 | 同时存在的块数上限 | octaves |
|---|---|---|---|
| 抽屉 | 1040×900（敏感库最大） | 1 | 3（深）/ 2（浅） |
| 卡片 | 每张 ≤ 420×180 | **同屏可见 ≤ 12 张**，超出则只给前 12 张挂滤镜 | 3（深）/ 2（浅） |

> 「同屏可见 ≤ 12 张」由 `IntersectionObserver` 在 **第三层**（`lib_ui/glass.rs`）统一实现，见 3.4。第二层只提供 CSS 类，不自己数卡片。

#### Q3：`normalScale` / `baseFrequency` / `numOctaves` / `seed` 各管什么，取什么值

> **先纠正一个命名**：Liquid Glass 的 SVG 滤镜链里**没有 `normalScale`**。`generate_glass_map.js` 的选项名是 `refractiveIndex` / `bevelWidth` / `dispScale`（`frontend/.agents/skills/liquid-glass/scripts/generate_glass_map.js:79-87`）。`normalScale` 是 three.js/pixi 那类「法线贴图」管线里的词，与本方案无关。下表按真实存在的参数给值。

**A. `feTurbulence` 参数（决定「玻璃表面有多不平静」）**

| 参数 | 语义 | 深色主题 | 浅色主题 | 太大/太小会怎样 |
|---|---|---|---|---|
| `type` | 噪声类型 | `fractalNoise`（固定） | 同 | 用 `turbulence` 会出现明显的「大理石纹」黑疙瘩 |
| `baseFrequency` | 噪波**空间频率**（1/波长，单位 px⁻¹） | **`0.008`** | **`0.006`** | 越小越像「大尺度缓慢起伏」（板材厚）；越大越像「细碎磨砂」。>0.05 变芝麻点，<0.003 整块同向偏移（看不出湍流） |
| `numOctaves` | 叠加几层噪声 | **`3`** | **`2`** | 4 以上边缘变毛刺感、代价翻倍；1 太单调 |
| `seed` | 随机种子 | **`11`**（固定） | `11`（**必须相同**） | 深浅切主题时若 seed 不同，噪波图案会整体跳变一次，观感像「重载」 |
| `stitchTiles` | 平铺接缝 | 不设（默认 `noStitch`） | 同 | 我们不对贴图做平铺，设了没用 |

> 深色取 `0.008` 与 `tokens.css:49` 的 `--refract-turbulence` **完全一致**；浅色取 `0.006` 与 `tokens.css:153` 一致。**规格不许另起数值**——CSS 里通过 `var(--refract-turbulence)` 传给滤镜？不行（SVG 属性不吃 CSS 变量），所以数值在 Rust 侧由主题分支给，**但必须与这两枚令牌同源**：建议 `lib_ui/glass.rs` 里 `match theme { Dark => (0.008, 3, 10.0), Light => (0.006, 2, 5.0) }`，并加单测断言「与 tokens.css 里的字面量相等」（用 `include_str!` 读 CSS 做断言，见 5.4）。

**B. `feDisplacementMap` 参数（决定「背景被推歪多少」）**

| 参数 | 语义 | 深色 | 浅色 | 说明 |
|---|---|---|---|---|
| `in` | 被位移的图 | `SourceGraphic` | 同 | 即克隆层的 backdrop |
| `in2` | 位移用的贴图 | 湍流结果（`result="n"`）或玻璃贴图 | 同 | 见 C |
| `scale` | **最大位移像素数**（R/G 通道 0..255 映射到 ±scale/2） | **`10`** | **`5`** | **= `--refract-edge` 的 10/14 折**。深色 14px 令牌对应 10px 实测可见但不过分；浅色 5px 令牌直接取 5 |
| `xChannelSelector` | 横向位移读哪个通道 | `R` | 同 | **不可改**：`REFERENCE.md:79-86` 的像素编码规定 R=水平位移、G=垂直位移、B=高光、A=透镜内 |
| `yChannelSelector` | 纵向位移 | `G` | 同 | 同上 |
| `color-interpolation-filters`（在 `<filter>` 上） | 色彩空间 | **`sRGB`** | 同 | **必须写**。默认 `linearRGB` 会让对比发闷、边缘发暗（`SKILL.md:105-107` Trap 2） |

**C. 位移贴图（`generateGlassMap`）参数 —— 只有「懂透镜形状」的那种才叫折射**

| 参数 | 语义 | 抽屉 | 卡片 | 取值理由 |
|---|---|---|---|---|
| `width` / `height` | 贴图尺寸（px） | 与透镜实际尺寸 1:1 | 与卡片 1:1 | 贴图按 `preserveAspectRatio="none"` 拉伸到 `feImage` 的 `width/height`，1:1 最省事也最准 |
| `radius` | 与元素 `border-radius` 一致 | `22`（`--radius-lg`） | `10`（`--radius-sm`） | 圆角对不上会出现「方角折射」 |
| `bevelWidth` | **斜面宽度**（= 玻璃厚度观感） | **`18`** | **`10`** | 6.1-⑦ 实测：18 在 420px 宽上折弯 1~2 条色带，是最像「厚玻璃」的值；10 更薄更克制 |
| `refractiveIndex` | 折射率 | **`1.5`**（浮法玻璃） | `1.5` | 1.85（蓝宝石）实测形变过大，只适合做「按钮级」小件（`SKILL.md:146-156`） |
| `dispScale` | **像素编码幅度**（不是像素位移） | **`2.0`** | **`2.0`** | 6.1-⑥ 实测：`1.0` 时 R/G 只覆盖 33..218（±90），边缘形变肉眼几乎看不出；`2.2` 时扩到 1..255（±127，吃满编码）。取 2.0 留一点余量防 clamp 削顶 |
| `lightDirection` | 高光光源方向 | 默认 `[-0.5,-0.7,0.7]`（左上） | 同 | 与 CSS `--spec-top`（迎光在上边）一致，不改 |

**D. 三条「滤镜链」模板（可直接照抄）**

1. **纯湍流链（P0 抽屉、P1 卡片都用它）**——`REFERENCE.md:175-195` 的 lean 单通道，Chromium 不需要三次位移：

```html
<filter id="lg-turb-dark" x="-8%" y="-8%" width="116%" height="116%"
        primitiveUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
  <feTurbulence type="fractalNoise" baseFrequency="0.008" numOctaves="3"
                seed="11" result="n"/>
  <feDisplacementMap in="SourceGraphic" in2="n" scale="10"
                     xChannelSelector="R" yChannelSelector="G" result="refracted"/>
  <!-- 可选哑光：需要时再接 feGaussianBlur stdDeviation="0.6" -->
</filter>
```

2. **真斜面链（只在抽屉用，可选开关）**——把 `in2` 换成 `generateGlassMap` 产出的贴图（`REFERENCE.md:134-173` 的 Chromium 多通道简化版）：

```html
<filter id="lg-lens-drawer" x="-2%" y="-2%" width="104%" height="104%"
        primitiveUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
  <feImage href="data:image/png;base64,…" x="0" y="0"
           width="1040" height="900" preserveAspectRatio="none" result="map"/>
  <feDisplacementMap in="SourceGraphic" in2="map" scale="22"
                     xChannelSelector="R" yChannelSelector="G" result="refracted"/>
  <feColorMatrix in="map" type="matrix"
                 values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 1 0 0" result="rim"/>
  <feBlend in="rim" in2="refracted" mode="screen"/>
</filter>
```

3. **哑光酸蚀链**（配置页「材质」下拉的第三档，可选）：在链 1 末尾接 `<feGaussianBlur stdDeviation="0.6"/>`（6.1-⑧ 实测 P3）。

#### Q4：深浅双主题是否两套参数、差在哪

**是，必须两套，且差在三个数**（其余全同）：

| 维度 | 深色（默认） | 浅色 | 差在哪·为什么 |
|---|---|---|---|
| `baseFrequency` | `0.008` | `0.006` | 浅底对比高，同频率下噪波更「扎眼」，频率压低 + 幅度砍半才不脏（`tokens.css:129` 注释：「浅色下高光给多了会显脏」） |
| `numOctaves` | `3` | `2` | 同上，少一层细节 |
| `feDisplacementMap scale` | `10` | `5` | 【实测 6.1-⑪】浅色下位移 >5px 时条纹边界开始出现「毛边/发晕」，与 `tokens.css:152` 的 `--refract-edge: 5px` 完全吻合 |
| `seed` | `11` | **`11`（相同）** | 切主题时图案不该跳变 |
| `refractiveIndex` / `bevelWidth` / `dispScale` | `1.5 / 18 / 2.0` | **相同** | 这是「玻璃的物理常数」，不随主题变 |

**切换机制**：主题是 `html[data-theme='light']`（`frontend/src/lib_ui/theme.rs` 的 `Theme::as_str()` 写进 `data-theme`）。因此**两套 `<filter>` 同时挂在 `<defs>` 里**，用 CSS 选择器决定谁被引用：

```css
/* 深色默认 */
.glass-l2 .glass-clone { filter: url(#lg-turb-dark); }
/* 浅色覆盖 */
[data-theme='light'] .glass-l2 .glass-clone { filter: url(#lg-turb-light); }
```

> 好处：切主题只触发一次样式重算，不需要 WASM 侧改 DOM 属性。**【未验证】** 见 6.2-③：`filter` 的 URL 引用在样式表里切换、且目标在两个 `<filter>` 之间切换，Chromium 是否会**复用旧的光栅缓存**（`SKILL.md:110-113` 的 Trap 3 说 Safari 会按 id 缓存）——施工时需在 WebView2 里切一次主题，确认没有「主题换了但折射图案没换」。

#### Q5：`<defs>` 挂 DOM 哪层、`filter: url(#id)` 由谁引用

**挂载点**：**`frontend/index.html` 的 `<body>` 末尾，静态写死一个 0×0 的 `<svg>`**。

```html
<!-- frontend/index.html：在 `<body>` 内、`<script>` 之前 -->
<svg width="0" height="0" style="position:absolute; left:-9999px; top:0" aria-hidden="true">
  <defs>
    <!-- 链 1：纯湍流（深/浅各一份，见 Q4） -->
    <filter id="lg-turb-dark"  …>…</filter>
    <filter id="lg-turb-light" …>…</filter>
    <!-- 链 2：真斜面（贴图由 WASM 启动后注入 href） -->
    <filter id="lg-lens-drawer" …>…</filter>
  </defs>
</svg>
```

**为什么放 `index.html` 而不是 Leptos 组件里**：
1. `filter: url(#id)` 的解析是**文档级全局**，一次解析即可被任意元素引用；
2. 放进组件里会随组件卸载而消失——`Show` 条件渲染的抽屉一关，正在做退出动画的元素会立刻丢失滤镜（观感是「关闭瞬间玻璃变平」）；
3. `index.html` 是 trunk 的静态入口，不经过 WASM，**首帧就有**，不会出现「先平后皱」。

**谁引用 filter**：**只由 CSS 引用**（`.glass-l2 .glass-clone { filter: url(#lg-turb-dark) }`），Rust 侧**不写 `style.filter`**。理由：主题切换、`prefers-reduced-transparency` 降级全部能在一处（CSS）收口；Rust 只负责往 `<feImage href>` 注入贴图 dataURL（见 3.2）。

**CSP 约束**：`app/src-tauri/tauri.conf.json:30` 的 `csp` 是 `default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'`。`feImage href="data:image/png;base64,…"` 走 `img-src`，`data:` **已放行**——【实测 6.1-⑤】Chrome 里 data: 贴图在 `feImage` 中正常生效。**`blob:` 没有放行**，所以 `SKILL.md:37-49` 的「MANDATORY FOR SAFARI 用 Blob URL」在本项目**不能用**（除非改 CSP，而改 CSP 属 `app/` 目录，本 Lane 无权、也不建议——WebView2 不是 Safari，没有那条限制）。

#### Q6：WebView2（非 Chrome）上是否成立

**【未验证】——这是本规格最大的一条风险，必须在施工第一步就验证**。已确证与待确证的边界：

| 事项 | 状态 | 证据 |
|---|---|---|
| 本机 WebView2 Runtime 版本 | 【实测】`157.0.4341.60` | 注册表 `HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` 的 `pv` |
| headless Chrome 版本 | 【实测】`158.0.0.0` | `--dump-dom` 里的 `data-probe` / `data-res` 的 `ua` |
| 同代内核 | 【读码】157 与 158 是同一条 Chromium 分支的相邻小版本 | 两者都是 Blink 158/157 线 |
| `filter: url(#svg)` 在 Chrome 158 成立 | 【实测】 | `chain.html`、`clone.html`、`edge.html`、`edgeE.html`、`edgeF.html` |
| `feTurbulence`+`feDisplacementMap` 在 Chrome 158 成立 | 【实测】 | `params.html`、`params-chrome.png` |
| `feImage`（data: PNG）在 Chrome 158 成立 | 【实测】 | `edgeE-chrome.png`（真 `generateGlassMap` 贴图） |
| **WebView2 157 是否同样成立** | **【未验证】** | 需按 5.1-① 在真窗口里跑一次探针 |

**为什么不能只靠「同代内核」推断**：WebView2 Runtime 可能被企业策略/精简安装裁剪 GPU 光栅路径；本项目还要叠 `window-vibrancy` 的亚克力（`app/src-tauri/src/lib.rs:78`），DWM 合成路径与纯 Chrome 不同。**验证方法见 5.1-①**：真窗口 + CDP + 一段注入脚本，把 `CSS.supports` 结果与「滤镜前后像素差」都打印出来。

**降级路径（必须有）**：若 WebView2 上 `filter: url()` 无效，则**整层静默关闭**——CSS 里用 `@supports` 兜底不行（`CSS.supports('filter','url(#x)')` 实测返回 `true`，但真正渲染可能失效），必须由 Rust 侧的运行时探针决定：注入一段「1×1 像素 diff」测试，失败则给 `<html>` 加 `data-l2="off"`，CSS 里 `.glass-l2` 相关规则全部按 `[data-l2='off']` 关掉，退回纯第一层。

### 2.3 第二层的 DOM 结构（施工时照抄）

```html
<!-- 以卡片为例；抽屉同理，只改类名 -->
<section class="card job-list glass-l2" data-testid="job-list">
  <!-- 新增：克隆层容器（必须在 .card 内部，且 .card 需 overflow:hidden） -->
  <div class="glass-clone-wrap" aria-hidden="true">
    <div class="glass-clone" data-glass-clone></div>
  </div>

  <!-- 原有内容全部保留，只是需要 z-index:1（见 4.2 的 CSS） -->
  <header class="card-head">…</header>
  <div class="card-body">…</div>
</section>
```

**三条不可省的约束**：

1. `aria-hidden="true"`——克隆层是纯装饰，读屏器不许念；
2. `.glass-clone` 的 `background` **必须由 Rust 侧按元素的真实屏幕坐标反向平移**（`translate3d(-x, -y, 0)`，其中 `x/y` = 元素 `getBoundingClientRect()` 相对视口的位置，加上 `.dashboard` 的滚动偏移）。这就是 `SKILL.md:18-26` 的 "Refract a Cloned Backdrop, Never the Live DOM" 契约；
3. `.glass-clone-wrap` 必须 `position:absolute; inset:0; overflow:hidden; border-radius:inherit; pointer-events:none; z-index:0`，且**父级 `.card` 要补 `overflow:hidden`**（现状没有，见 1.3）。

> **写清一个诚实的天花板**：克隆层只能画出「我们自己知道背景长什么样」的那部分（页面内容、CSS 渐变）。**桌面壁纸在 DOM 外，克隆不到**——所以第二层在「窗口内背景」上成立，在「透出的桌面」上不成立。这与第一层的分工正好互补：桌面靠亚克力（Rust），窗内靠第二层。

### 2.4 第二层的 CSS（新增 `frontend/styles/glass.css`）

```css
/* frontend/styles/glass.css —— Liquid Glass 第二层（SVG 湍流折射）
 * 只消费 tokens.css 的令牌；不定义任何新的像素常量。
 * 必须在 index.html 里追加 link，位置见 4.1。 */

/* ---------- 克隆层容器 ---------- */
.glass-l2 { position: relative; overflow: hidden; }   /* 圆角裁剪，缺了会露方角 */
.glass-clone-wrap {
  position: absolute; inset: 0; overflow: hidden;
  border-radius: inherit; pointer-events: none; z-index: 0;
}
.glass-clone {
  position: absolute; inset: -8%;        /* 留 8% 余量，位移不露边 */
  background: inherit;                    /* 由 Rust 侧写真实背景层 */
  filter: url(#lg-turb-dark);             /* 深色默认 */
  will-change: transform;
}
[data-theme='light'] .glass-clone { filter: url(#lg-turb-light); }

/* 内容压在克隆层之上 */
.glass-l2 > .card-head,
.glass-l2 > .card-body { position: relative; z-index: 1; }

/* ---------- 关闭开关（运行时探针判定后由 Rust 打属性） ---------- */
[data-l2='off'] .glass-clone { filter: none !important; }

/* ---------- 无障碍 ---------- */
@media (prefers-reduced-transparency: reduce) {
  .glass-clone { filter: none !important; opacity: 0 !important; }
}
@media (forced-colors: active) {
  .glass-clone { display: none !important; }
}
```

### 2.5 第二层的成本

| 项 | 量级 | 说明 |
|---|---|---|
| DOM 节点 | 每块玻璃 +2（wrap + clone） | 概览页 P1 最多 12 张卡 = +24 节点 |
| WASM 代码 | **0 B**（第二层全在 CSS） | 只多一个 `<filter>` 块 |
| CSS | 约 60 行 / 1.5 KB | 新增 `glass.css` |
| 首帧成本 | `feTurbulence` 每块一次光栅化 | 12 张卡 × 3 octaves，实测在 Chrome 上是「一次性」，滚动不重算（`motion.css:8` 已明确只动 transform/opacity） |

---

## 3. 第三层规格：WebGL 卡片间真折射（方案 C）

### 3.1 技术选型：裸 WebGL2 + 极小封装

| 选项 | 体积 | 依赖风险 | 结论 |
|---|---|---|---|
| **裸 WebGL2 + `web-sys`** | 0 KB JS，新增 wasm 约 **6~10 KB**（估算见下） | 无第三方依赖，无许可证，不会被上游 break | ✅ **选它** |
| `wgpu` | release wasm 常 +1.5~3 MB | 需要额外 feature 组合，`opt-level="s"` 下 tree-shaking 仍会带进 backend 选择 | ❌ 与「界面小工具」的体量不匹配 |
| three.js / pixi | +150~600 KB JS，或 +几百 KB 的 wasm 包装 | 需要 `js-sys` 桥、`wasm-bindgen` 版本对齐；升级跟随上游 | ❌ 只用它的 0.5% 功能 |
| 官方 `leptos_webgl` 之类 | 生态不成熟 | 维护者不确定 | ❌ |

**体积估算依据**：`frontend/dist/aja-frontend-*.wasm` 现在是 **93,941,672 B（89.6 MiB，dev 未裁剪）**；`Cargo.toml` 的 release profile 是 `codegen-units=1 / lto=true / opt-level="s"`。第三层新增的代码是「一个顶点/片元着色器字符串 + 一次 draw call + 一个 rAF 环」≈ 300~400 行 Rust，其中真正进 wasm 的常量占大头（着色器文本 ~1.5 KB）。**施工时以实测为准**（`trunk build --release` 前后对比 wasm 字节数，与 `frontend/dist` 现有产物对比），若增量 > 40 KB 则复查是否误引了大依赖。

**需要追加的 `web-sys` features**（`frontend/Cargo.toml:28-35`）：

```toml
"HtmlCanvasElement", "CanvasRenderingContext2d", "WebGl2RenderingContext",
"WebGlProgram", "WebGlShader", "WebGlBuffer", "WebGlUniformLocation",
"WebGlTexture", "WebGlVertexArrayObject", "WebGlFramebuffer",
"Element", "DomRect", "ResizeObserver", "IntersectionObserver",
```

### 3.2 【核心】「WebGL 拿不到 DOM 合成结果」这个矛盾，怎么解

**矛盾的准确表述**：WebGL 的 `gl.readPixels()` 只能读**自己这个 canvas 的帧缓冲**；`html2canvas` 那类「DOM 重绘成图」的方式拿不到 `backdrop-filter` 之后的真实合成结果，也拿不到 Rust 侧的亚克力桌面。所以「想让玻璃折射它背后的东西」时，**背后的东西根本不在 GPU 里可读**。

**唯一选择（不做并列选项）：放弃「读背景」，改成「玻璃边缘法线偏移 UV 的假折射」，并把折射源限定为「我们自己渲染的那一层」。**

具体三条：

1. **不读 DOM，不 `readPixels` 背景**。第三层**自己画一份「被折射对象」**：抽屉打开时，把**列表卡片的几何（矩形 + 圆角 + 底色 + 1px 高光边）**作为**实例化四边形**传给 GPU。这些几何是我们自己的数据（`features/jobs/state.rs` 里的 `jobs` 列表 + `getBoundingClientRect()`），不需要任何像素读取。
2. **折射 = 法线偏移 UV**。片元着色器对每个目标像素 `p`，用 `generateGlassMap` 同一条数学在 GLSL 里就地算法线 `n`（`snellDeflection` + `squircleSDF`，见 `REFERENCE.md:34-88`），然后 `uv = p + n.xy * edgeFactor * uScale`，再从「我们自己渲染的那张卡片层纹理」采样。**这就是真折射的几何，只是折射源是自绘纹理而不是 DOM 合成图**。
3. **玻璃覆盖关系由 z 序决定，不由像素决定**。抽屉盖住列表 → 只给「被抽屉覆盖区域的卡片层」上折射；气泡盖背景 → 只给「气泡下方的卡片层」上折射。**桌面永远不折射**（`tokens.css:13-15` 已写死这条）。

**为什么是它（而不是另外三条）**：

- **预渲染背景快照**：需要在每次布局变化后把整个页面「画成图」——WebView2 里没有离屏渲染 DOM 的 API，html2canvas 那条路会把 `backdrop-filter` 和亚克力全部丢掉，等于折射一张错的图。
- **只对玻璃自身内容折射**：抽屉里主要是文字，折射文字会糊掉可读性，而且不算「真折射」。
- **html2canvas 类**：体积（~200 KB）+ 保真度（不支持 `backdrop-filter`）+ 每次打开抽屉要重绘整页，三样都不可接受。
- **法线偏移 UV**：唯一的代价是「折射源是自绘的卡片层」而不是真实 DOM 像素；但观感上，用户看到的是「抽屉边缘把下面的卡片推弯了」——**这一层正好是方案 C 需要的**，因为真正被折射的也就是卡片。

**一句话对照表**（便于 code review 时一眼看出边界）：

| 视觉层 | 谁负责 | 折射源 | 是否可读 DOM 像素 |
|---|---|---|---|
| 桌面壁纸 | Rust 亚克力（`apply_acrylic`） | 系统 DWM | **不需要**（DWM 自己糊） |
| 页面背景 + 卡片 | 第一层 CSS + 第三层自绘 | 我们自己上传的卡片几何 | 否——几何是我们自己的数据 |
| 卡片边缘的湍流 | 第二层 SVG | 克隆层（按坐标反向平移） | 否——克隆层自己画 |
| 抽屉边缘的放大折射 | **第三层 WebGL** | **自绘卡片层纹理** | **否** |

### 3.3 第三层的渲染管线（可直接照抄的骨架）

```
每帧（仅在 active 时）:
  1. 读状态：drawerProgress(x,y,w,h) / 列表滚动 offset / 指针位置
  2. 若 progress 没变且没有动画 → 跳过整帧（return，不 draw）
  3. 清屏 → 画「背景层」：一个全屏 quad，底色 = body 背景色（不是 DOM，是 CSS 变量值）
  4. 画「卡片层」到 FBO：实例化绘制 N 个圆角矩形（每实例 = mat3 变换 + 4 个圆角半径 + 3 个颜色）
  5. 画「透镜」：以抽屉矩形为边界，片元里算 squircle SDF →
       edge = clamp(1 - sdf/bevelWidth, 0, 1)
       n    = snell(deflectionAt(edge))
       uv   = screenUV + n.xy * edge * uRefractEdge
       col  = texture(cardFBO, uv)
       再按 uTint 混 --glass-3、按 (n·L)^16 叠高光
  6. 用 premultiplied alpha 与 DOM 合成（canvas 的 CSS opacity 由 JS 线性插值到 drawerProgress）
```

**为什么自己画卡片层而不是直接折射 DOM**：见 3.2。**为什么还要画一遍卡片**（看起来像重复劳动）：因为只有这样 GPU 里才有「可读的折射源」；卡片本来就在 DOM 里，canvas 只是**在抽屉的覆盖区域内**叠一层「折射后的卡片」，覆盖区外的卡片完全由 DOM 渲染，两者在边界处通过 `drawerProgress` 对齐（3.6）。

### 3.4 性能预算

| 项 | 约定 | 判据 |
|---|---|---|
| 渲染开关 | **只在 `drawerProgress` 处于 (0, 1) 区间 或 「静止但抽屉开着」的 1 帧内渲染** | 关闭态 `progress == 0` → **完全停止 rAF**，不 burn CPU |
| 静止后的降频 | 抽屉完全静止 **500ms** 后停 rAF，只保留一次 `draw` 的最终画面 | 用 `requestIdleCallback` 不行（WebKit 系不支持），用 `setTimeout(500)` |
| 帧预算 | 单帧 GPU 时间 ≤ **4ms**（16.7ms 的 1/4）；超出则把 `bevelWidth` 或卡片实例数降一档 | 用 `EXT_disjoint_timer_query_webgl2`（若不可用则跳过，见 6.2-⑤） |
| 分辨率 | canvas 的 `width/height` = 覆盖区 × `devicePixelRatio`，但 **上限 2.0** | 4K 屏上 3x 会把 1040×900 的抽屉变成 3120×2700，必爆 |
| 实例数 | 卡片实例 ≤ **64**（概览页一屏最多 12 张卡，留 5× 余量） | 超了只画前 64 个最近矩形 |
| `prefers-reduced-motion: reduce` | **完全不建 canvas**，`detect_reduced_motion()` 为真时直接 return；抽屉退回纯 CSS 过渡 | 与 `motion.css:86-93` 的既有降级一致 |
| `prefers-reduced-transparency: reduce` | 同样不建 canvas（与 2.4 的 CSS 降级同源） | `SKILL.md:114-126` |
| 后台/失焦 | `document.visibilityState === 'hidden'` → 停 rAF | 避免最小化时偷跑 GPU |
| 设备不支持 | `webgl2` 上下文创建失败 → 打 `data-l3="off"`，第二层照常 | 见 5.1-① 的探针 |

### 3.5 第三层与 DOM 的层级与事件

| 约束 | 值 | 理由 |
|---|---|---|
| canvas 位置 | `position: fixed; inset: 0; z-index: 40`（**在抽屉下层**：DOM 抽屉 z-index 是 41，见 `drawer.css:41`、`sensitive.css:41` 附近） | 透镜必须在抽屉**下面**，抽屉的不透明玻璃盖在上面才有「透过玻璃看折射」的层次 |
| `pointer-events` | **`none`** | E2E 的 `ElementHandle.click()` 必须能命中 DOM |
| `data-testid` | `glass-l3-canvas` | 新增，供 E2E 断言存在性 |
| DOM 位置 | 挂在 `frontend/src/components/shell.rs:153` 的 `div.app-shell` **内部末尾**（`data-testid="app-root"` 之内） | 跟随窗口尺寸变化，不需要额外监听 resize 窗口事件 |

### 3.6 与抽屉 `transform: translateX()` 过渡同步不撕裂

**问题**：抽屉的平移动画是 CSS 合成层在跑（`sensitive.css:53-60`：`transition: transform 220ms var(--ease-glass)`，从 `translateX(100%)` 到 `0`；岗位详情抽屉是 `drawer.css:57-70` 的 `@keyframes drawer-in` 0.22s）。WebGL 的透镜位置必须**逐帧跟上**，否则会出现「玻璃边缘和折射层错开」的撕裂。

**做法（唯一方案）：每帧读真实几何，不猜动画进度。**

```
rAF 循环里：
  const r = drawerEl.getBoundingClientRect();      // 真实合成后的矩形（含 transform）
  uniform uLensRect = [r.x, r.y, r.width, r.height];
  uniform uProgress = clamp(1 - r.right / window.innerWidth, 0, 1);
```

理由与实测支撑：

1. **`getBoundingClientRect()` 返回的是变换后的矩形**——这是 CSS Transforms 规范保证的，不是猜测。【实测 6.1-⑨】`sync.html` 里对「plate 层」做 `translate3d(-i px)` 并让克隆层跟随，在 60 帧的动画中**色带边界与真实位置始终对齐**（左格有滤镜、右格无滤镜，边界位置一致）。
2. **不依赖 `transitionend`/`animationend`**：`git grep` 实测前端零命中，而且事件会漏（快速开关时 `transitionend` 可能不触发）。
3. **不用 CSS 变量插值**：`--dur` 只存在于 `tokens.css`，硬编码 220ms 会在 `prefers-reduced-motion` 降级（`transition: none`）时对不上——而 `getBoundingClientRect()` 天然对上。
4. **同一帧内读 + 画**：先 `getBoundingClientRect()` 再 `gl.drawElements()`，两者在同一 rAF 回调里，合成器看到的永远是同一帧的几何。**若在 rAF 之外由 `IntersectionObserver` 触发绘制，必须再次读矩形**。

**【未验证】6.2-④**：WebView2 在「CSS 合成层动画」进行期间，`getBoundingClientRect()` 是否仍返回**当帧**的合成位置（Chrome 会，WebView2 理论上相同但需实测）。验收方法见 5.3-③。

### 3.7 第三层的代码落点（**只列路径，本 Lane 不写代码**）

| 文件 | 动作 | 分层依据（`frontend/AGENTS.md`） |
|---|---|---|
| `frontend/src/lib_ui/glass.rs` | **新建**：`detect()` 能力探针、`ThemeParams` 常量、`normal_offset()` 纯函数（可在宿主目标单测） | `lib_ui` 不依赖其它层，纯函数放这里最合适 |
| `frontend/src/components/glass_canvas.rs` | **新建**：Leptos 组件，持 `HtmlCanvasElement`，建 `WebGl2RenderingContext`、编译着色器、rAF 环、`Drop` 里 `delete_program` | `components` 只依赖 `lib_ui` |
| `frontend/src/components/shell.rs:153` | 改 1 处：在 `div.app-shell` 末尾插 `<GlassCanvas/>` | 已有组件，改动最小 |
| `frontend/Cargo.toml:28-35` | `web-sys` features 追加（见 3.1） | — |
| `frontend/styles/glass.css` | **新建**：第二层 CSS（2.4）+ 第三层 canvas 的 `#glass-l3-canvas { position:fixed; inset:0; pointer-events:none; z-index:40 }` | — |
| `frontend/index.html` | 追加 1 条 `<link data-trunk rel="css" href="styles/glass.css" />` + 静态 `<svg><defs>` 块 | — |

---

## 4. 施工顺序（每步有验收判据）

> 每批 ≤ 2 个文件簇。**第 1 批是风险闸门**：WebView2 验证不过就停在这里，不写一行第二层代码。

### 批次 L2-0 · 能力闸门（不改任何正式代码）

| 步骤 | 做什么 | 文件簇 | 验收判据 |
|---|---|---|---|
| 0.1 | 在 `.tmp-test/l23/` 写好探针页（`probe-webview2.html`：报告 `CSS.supports` ×4、`webgl2` 是否可用、`feTurbulence` 前后像素差） | `.tmp-test/l23/**`（**不进 src**） | 页在 Chrome 里跑通，4 个布尔值 + 1 个像素差数值都能打印 |
| 0.2 | 用 CDP 接进真窗口：`$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=PORT"` 起 `D:\jt\debug\jobassistant-app.exe`，`Page.navigate` 到 `http://localhost:PORT/…`（或 `Runtime.evaluate` 注入探针脚本） | 无 | 拿到 WebView2 157 上的同一组布尔值 + 像素差 |
| **闸门判定** | `filter:url()` 有效 且 像素差 > 0 | — | **通过** → 进 L2-1；**不通过** → 停，改走 2.2-Q6 的降级（`data-l2="off"`），并把结论回写 `AGENTS.md` 6.2 |

### 批次 L2-1 · 静态 `<defs>` + CSS 类

| 步骤 | 做什么 | 文件簇 | 验收判据 |
|---|---|---|---|
| 1.1 | `frontend/index.html` 加 `<svg><defs>`（深/浅两条湍流滤镜，参数照 2.2-Q3） | `frontend/index.html` | 打开 DevTools，`document.getElementById('lg-turb-dark')` 非 null；改 `baseFrequency` 后截图肉眼可见变化 |
| 1.2 | 新建 `frontend/styles/glass.css`（2.4 全文）+ `index.html` 加 link | `frontend/styles/glass.css` | `data-trunk` link 位于 `filter.css` 之后；`trunk build` 成功；**CSS 里 `//!` 零命中**（`Select-String -Pattern '^\s*//[!/]'`） |

### 批次 L2-2 · 抽屉接上第二层（先做 P0）

| 步骤 | 做什么 | 文件簇 | 验收判据 |
|---|---|---|---|
| 2.1 | `frontend/src/features/sensitive/panel.rs`、`frontend/src/features/jobs/drawer.rs` 各插一个 `.glass-clone-wrap > .glass-clone` | 这两个 `.rs` | 抽屉打开时新增节点 `[data-glass-clone]` 存在且 `pointer-events:none` |
| 2.2 | Rust 侧按 `getBoundingClientRect()` + `.dashboard.scrollTop` 写 `--glass-x/--glass-y`，CSS 用 `translate3d(calc(var(--glass-x) * -1), …)` | 同上 + `glass.css` | 缓慢滚动列表，克隆层背景与真实背景**不相对滑动**（目视 + 两张相隔 200px 的截图对比） |
| 2.3 | 主题切换复测 | 无 | 深→浅→深，折射图案**不跳变、不残留**（见 6.2-③ 待验证点） |

### 批次 L2-3 · 卡片接上第二层（P1）

| 步骤 | 做什么 | 文件簇 | 验收判据 |
|---|---|---|---|
| 3.1 | `frontend/src/features/jobs/list.rs` / `cards.rs` 给 `.job-card` 加 `.glass-l2` + 克隆层 | `cards.rs` | 12 张卡同时挂滤镜，`performance.now()` 测「首屏到可交互」增量 ≤ 50ms |
| 3.2 | `IntersectionObserver` 限制同屏 ≤ 12 张（逻辑放 `lib_ui/glass.rs`） | `lib_ui/glass.rs` | 滚到第 13 张时，第 1 张的 `filter` 被移除（DevTools 可查） |

### 批次 L3-1 · 能力与骨架（无视觉效果）

| 步骤 | 做什么 | 文件簇 | 验收判据 |
|---|---|---|---|
| 1.1 | `frontend/Cargo.toml` 追加 features | `frontend/Cargo.toml` | `cargo check --target wasm32-unknown-unknown` 通过 |
| 1.2 | `lib_ui/glass.rs` 写 `detect()` + `ThemeParams` + `normal_offset()` 纯函数 + 单测（断言与 `tokens.css` 字面量一致） | `lib_ui/glass.rs` | `cargo test`（**宿主目标**，因为 wasm 目标缺 runner，见 6.1-⑩）新增用例全绿 |
| 1.3 | `components/glass_canvas.rs` 建 canvas + 清屏（**先画纯色，不做折射**） | `components/glass_canvas.rs` | 窗口里出现一层透明 canvas，`data-testid="glass-l3-canvas"` 存在，DOM 交互不受影响（点按钮仍可点） |

### 批次 L3-2 · 折射本体

| 步骤 | 做什么 | 文件簇 | 验收判据 |
|---|---|---|---|
| 2.1 | 片元着色器实现 squircle SDF + Snell + UV 偏移，折射源先用「程序化条纹」验证几何正确 | `glass_canvas.rs` | 抽屉滑动时，条纹被「推弯」的方向随边缘法线变化（左边缘向左推、右边缘向右推） |
| 2.2 | 换成自绘卡片层（实例化矩形） | `glass_canvas.rs` | 关掉 DOM 卡片（DevTools 隐藏 `.job-grid`）后，canvas 里仍能看到卡片形状 → 证明是自己画的 |
| 2.3 | `getBoundingClientRect()` 逐帧同步 | `glass_canvas.rs` | 快速开合 5 次，抽屉边缘与折射边界**不错位**（录屏逐帧看） |

### 批次 L3-3 · 预算与降级

| 步骤 | 做什么 | 文件簇 | 验收判据 |
|---|---|---|---|
| 3.1 | rAF 开关/静止降频/可见性 | `glass_canvas.rs` | 抽屉关着时 `performance` 面板里 **0 次 draw** |
| 3.2 | `reduced-motion` / `reduced-transparency` / `data-l3="off"` | `glass_canvas.rs` + `glass.css` | 系统开「减少动画」后，canvas 根本不被创建 |

---

## 5. 验收清单（可勾选，每项写明「怎么算过」）

### 5.1 能力与兼容

- [ ] **①【最高优先】WebView2 157 上 `filter: url(#f)` 生效** — 怎么算过：真窗口里注入探针，`CSS.supports('filter','url(#f)')` 为 `true` **且** 对同一元素加/去滤镜后的 `canvas` 采样像素差 > 0（**不能只看 supports**，`backdrop-filter: url()` 就是 supports 为真而实际失效的反例）。
- [ ] **② WebView2 上 `feImage` 吃 `data:` 有效** — 怎么算过：注入 `feImage href="data:image/png;base64,…"` 的滤镜，边缘像素与不加时不同。依据：`tauri.conf.json:30` 的 `img-src` 已含 `data:`，Chrome 侧实测见 6.1-⑤。
- [ ] **③ WebGL2 可用** — 怎么算过：`document.createElement('canvas').getContext('webgl2')` 非 null，且 `getParameter(VERSION)` 打印出来。
- [ ] **④ `devicePixelRatio` 上限 2 生效** — 怎么算过：把系统缩放调到 300% 起窗口，canvas 的 `width` 不超过 `cssWidth * 2`。

### 5.2 第二层观感

- [ ] **⑤ 深色下能看到边缘折弯、中心不变形** — 怎么算过：`.tmp-test/l23/edgeF-chrome.png` 式的放大对比（同一块玻璃，有/无滤镜各截一张，边界处色带弯曲 1~2 条，中心色带与背景对齐）。
- [ ] **⑥ 浅色下不「脏」** — 怎么算过：浅色主题下截图，与深色同位置对比，边缘**不出现暗边/毛刺**；若出现则 `scale` 从 5 降到 3 复测。
- [ ] **⑦ 滚动不相对滑动** — 怎么算过：`.dashboard` 滚 200px 前后各截一张，克隆层里的背景图案与页面真实背景**位移一致**（可用像素坐标对齐检查）。
- [ ] **⑧ 主题切换不跳变、不残留** — 怎么算过：深→浅→深各截一张，噪波图案的「大块走向」三次一致（seed 相同），且没有「切了主题但图案还是旧的」。
- [ ] **⑨ 关掉第二层不留痕** — 怎么算过：给 `<html>` 打 `data-l2="off"`，截图与改动前**逐像素一致**（这证明第二层是纯附加层）。

### 5.3 第三层行为

- [ ] **⑩ 不吞事件** — 怎么算过：`python tools/gui_e2e.py run --ui-only` 全绿（canvas 覆盖全屏，这是最直接的回归）。
- [ ] **⑪ 关闭态零开销** — 怎么算过：抽屉关闭静置 30s，chrome devtools `Performance` 录制里 `drawElements` 调用数为 **0**。
- [ ] **⑫ 抽屉动画同步不撕裂** — 怎么算过：0.25× 慢放录屏，抽屉左边缘与折射区左边界**逐帧重合**（偏差 ≤ 1px）。
- [ ] **⑬ 降级路径可达** — 怎么算过：手动打 `data-l3="off"`，界面**无报错、无白块**，仅少折射。

### 5.4 工程质量

- [ ] **⑭ 新增 CSS 无 Rust 注释** — 怎么算过：`Select-String -Path frontend/styles/*.css -Pattern '^\s*//[!/]'` **零命中**（当前基线已有 5 行历史残留，见 `controls.css:2-3`、`layout.css:2-4`，不在本规格处理范围但**不许新增**）。
- [ ] **⑮ 分层不破** — 怎么算过：`python tools/check_layers.py` 不新增违规（现状已有 2 处历史违规）。
- [ ] **⑯ 参数与令牌同源** — 怎么算过：`lib_ui/glass.rs` 的单测用 `include_str!("../../styles/tokens.css")` 断言 `0.008` / `0.006` / `14px` / `5px` 四个字面量仍存在；改令牌而不改常量会红。
- [ ] **⑰ wasm 增量受控** — 怎么算过：`trunk build --release` 前后 `frontend/dist/*.wasm` 字节差 ≤ 40 KB。
- [ ] **⑱ 无桩代码入库** — 怎么算过：`git grep -n -E 'probe|__STUB|tmp-test' -- frontend/src` 零命中。
- [ ] **⑲ 前端单测全绿** — 怎么算过：`cd frontend && cargo test`（宿主目标）通过，用例数从 85 上升。
- [ ] **⑳ E2E 契约不破** — 怎么算过：`tools/gui_e2e.py` 的 16 项 `REQUIRED_TESTIDS` 全部仍能命中。

---

## 6. 已实测 vs 未验证（施工时以本节为准）

### 6.1 已实测可行（本轮在 headless Chrome 158.0.0.0 上真跑，产物在 `.tmp-test/l23/`）

| # | 结论 | 证据文件 | 复现命令 |
|---|---|---|---|
| ① | `CSS.supports` 四连：`backdrop-filter:url()`=**true**、`-webkit-backdrop-filter:url()`=**false**、`filter:url()`=**true**、`backdrop-filter:blur()`=**true** | `probe.html` + `probe-dom.html` 的 `data-probe` | `chrome --headless=new --dump-dom …`（**必须 `2>&1 \| Out-File` 才能读到 DOM**） |
| ② | **`backdrop-filter: url(#turb8) blur(18px)` 的 `url()` 被静默忽略**（第 1 格与第 4 格几乎一致），**不能这样叠第二层** | `chain.html` / `chain-chrome.png` | 同上，`--screenshot` |
| ③ | `backdrop-filter: url(#f)` **单独**用是真折射（强波纹） | `chain.html` 第 2 格 | 同上 |
| ④ | 克隆层 + `filter: url(#feTurbulence+feDisplacementMap)` 成立；但纯渐变色块会被打碎成波浪条带（**所以要做「真贴图」链，不能只挂湍流当玻璃**） | `clone.html` / `clone-chrome.png` | 同上 |
| ⑤ | `feImage`（`data:image/png;base64` 与 `data:image/svg+xml;base64` 两种）在 `feDisplacementMap` 里**全部生效**，且不随滤镜数量/元素尺寸失效；`primitiveUnits="userSpaceOnUse"` 与 `filterUnits` 组合均可 | `edge.html` / `edge-chrome.png`、`edgeE.html` / `edgeE-chrome.png`、`edgeF.html` / `edgeF-chrome.png` | 同上 |
| ⑥ | `generateGlassMap` 可在 node 直接跑（UMD 零依赖），中心=中性 `[128,128,0]`、越界 `A=0`；`dispScale=1.0` 时 R/G 仅覆盖 33..218，**`2.2` 时扩到 1..255**（吃满编码）；`toDataURL()` 在 node 里抛 `toCanvas requires a DOM environment` | `genmap.cjs`、`range.cjs`、`glassmap-x3.png` | `node .tmp-test/l23/genmap.cjs` |
| ⑦ | 四组材质参数在真贴图链下的观感：**A(bevel18/disp1.0) 几乎无感**、B(disp2.2) 与 C(bevel10) 边缘可见折弯、D(蓝宝石 bevel32/n1.85/scale36) 形变最大 | `maps.html` / `maps-chrome.png` | `chrome --screenshot` |
| ⑧ | `feTurbulence` 三组参数观感：P1(bf .018/oct3/scale18) 中等波纹、P2(bf .006/oct2/scale34) 大尺度起伏、P3(bf .030/oct4/scale9+blur .6) 细碎哑光 | `params.html` / `params-chrome.png` | 同上 |
| ⑨ | **克隆层逐帧反向平移不撕裂**：被滤镜的 plate 层 `translate3d(-i px)` 时，克隆层与真实位置在 60 帧里始终同步 | `sync.html` / `sync-chrome.png` | 同上 |
| ⑩ | 样式重算成本与滤镜无关（`none 0.017ms` / `blur 0.020ms` / `filter 0.019ms` 每帧）——**成本在光栅化，不在样式** | `perf3.html` / `perf3-*.txt` | `chrome --headless=new --dump-dom …?m=filter` 读 `<title>` |
| ⑪ | 深浅折射量：深色 `scale 10` 可见而不脏；浅色 `scale 5` 合适、`scale 10` 开始发晕（与 `tokens.css:152` 的 5px 令牌一致） | `theme5.html` / `theme5.png` | `chrome --screenshot` |
| ⑫ | headless Chrome 的 WebGL：`webgl:true`、`webgl2:true`、renderer = `ANGLE (Intel, Intel(R) UHD Graphics …, D3D11)`；三种启动参数结果一致 | `gpu.html` 的 `data-res` | `chrome --headless=new --dump-dom …` |
| ⑬ | 本机 WebView2 Runtime = **157.0.4341.60**；headless Chrome = **158.0.0.0** | 注册表 `pv` + `ua` | — |
| ⑭ | 前端基线：`cargo check --target wasm32-unknown-unknown` 通过（24.3s，45 warnings）；`cargo test --target wasm32-unknown-unknown` **失败**（`os error 193`，无 wasm runner）；`cargo test`（宿主）**85 passed** | 终端 | `cd frontend; cargo test` |
| ⑮ | 现状缺口：`feTurbulence` / `feDisplacementMap` / WebGL 在 `frontend/src` 与 `frontend/styles` **零命中**；`transitionend`/`animationend` 零命中；`Cargo.toml` 无 canvas features | `git grep` 输出 | 见 1.3 |

### 6.2 未验证——施工时必须现场确认

| # | 待确认项 | 风险等级 | 怎么确认 | 不成立的后果 / 对策 |
|---|---|---|---|---|
| ① | **WebView2 157 上 `filter: url(#svg)` 是否真的生效**（不是 supports，是像素） | **高** | 5.1-① 探针 | 第二层整层关闭（`data-l2="off"`），退回第一层；把结论回写 `AGENTS.md` 6.2 |
| ② | WebView2 上 `feImage` + `data:` 是否受 `img-src` 放行**且**被滤镜认 | 中 | 5.1-② | 改用「程序化贴图」（`feTurbulence` 直喂）降级，观感略差但不报错 |
| ③ | **切主题后滤镜 URL 切换是否有光栅缓存残留**（Safari 按 id 缓存的 Trap 3 是否也发生在 WebView2） | 中 | 5.2-⑧ | 切换时**换一个新 id**（`lg-turb-light-{n}`）强制重建 |
| ④ | WebView2 在 CSS 合成层动画期间 `getBoundingClientRect()` 是否给当帧值 | 中 | 5.3-⑫ 慢放录屏 | 改用 `drawerProgress` 由 CSS 变量 `--drawer-p` 插值并读 `getComputedStyle` |
| ⑤ | `EXT_disjoint_timer_query_webgl2` 是否可用（帧预算测量） | 低 | 5.3-③ | 不可用则用「帧计数 / 时间窗」粗测 |
| ⑥ | 亚克力（`apply_acrylic`）与 WebGL canvas 叠加时，DWM 合成是否出现色偏/闪烁 | 中 | 真窗口里开抽屉，目视 + 录屏 | 改 canvas 的 `alpha:false` + 手动混为 `rgba` |
| ⑦ | 12 张卡同时挂 `feTurbulence` 在 WebView2 上的首屏开销 | 中 | 5.2 的 3.1 | 降到 8 张，或卡片改用「无湍流、只留高光」 |
| ⑧ | `prefers-reduced-transparency` 在 WebView2 是否被支持 | 低 | 5.3-⑬ | `@media` 不生效则加 Rust 侧读取（`match_media`） |
| ⑨ | 90 MB dev wasm 之外的 **release wasm 增量**（体积估算 6~10 KB 未实测） | 低 | 5.4-⑰ | 超 40 KB 复查依赖 |
| ⑩ | `cargo test --target wasm32-unknown-unknown` 缺 runner（现状已红），本规格的新单测只能跑宿主目标 | 低（已知） | — | 保持宿主目标 + `#[cfg(not(target_arch = "wasm32"))]` 门控 |

### 6.3 明确「不做」的清单（避免施工时被理解成遗漏）

- ❌ 不给 `backdrop-filter` 追加 `url()`（实测无效，6.1-②）。
- ❌ 不做全屏/顶栏/遮罩的第二层（2.2-Q1 的「不做」表）。
- ❌ 不用 `blob:` URL 作贴图（CSP `img-src` 没放行，`tauri.conf.json:30`）。
- ❌ 不引 three.js / pixi / wgpu（3.1）。
- ❌ 不让 WebGL 折射桌面（3.2，`tokens.css:13-15` 已定）。
- ❌ 不改 `tokens.css` 的既有令牌值。
- ❌ 不把探针/桩代码放进 `frontend/src/`。

---

## 附：本规格引用的实测产物索引（全部在 `.tmp-test/l23/`，不进版本库）

| 文件 | 内容 |
|---|---|
| `probe.html` / `probe-dom.html` | `CSS.supports` 四连探测 |
| `chain.html` / `chain-chrome.png` | backdrop-filter 三种写法对照（证明 url() 被忽略） |
| `clone.html` / `clone-chrome.png` | 克隆层 + 湍流（证明「水面波纹」不是折射） |
| `dbg.html` / `dbg-chrome.png` | 排除「过滤器太多导致失效」的误判 |
| `edge.html` / `edge-chrome.png` | `feImage` 放 data:SVG 贴图 |
| `edgeE.html` / `edgeE-chrome.png` | 真 `generateGlassMap` 贴图（node 导 raw → 浏览器 canvas → dataURL） |
| `edgeF.html` / `edgeF-chrome.png` | 100×40 小贴图 + `scale(4)` 放大镜验证 |
| `maps.html` / `maps-chrome.png` | 四组材质参数对照 |
| `params.html` / `params-chrome.png` | 三组 `feTurbulence` 参数对照 |
| `sync.html` / `sync-chrome.png` | 逐帧反向平移同步验证 |
| `perf.html` / `perf3.html` / `perf3-*.txt` | 样式重算成本 |
| `gpu.html` | headless WebGL/WebGL2 能力 |
| `theme5.html` / `theme5.png` | 深浅折射量对照（另有 theme/2/3/4 三次失败取景，lesson：背景要有 40px 级中等频率条纹） |
| `genmap.cjs` / `range.cjs` / `dumpraw.cjs` / `maps.cjs` / `gen2.cjs` / `genppm.cjs` | `generateGlassMap` 的 node 侧取数与贴图导出 |
| `glassmap-x3.png` | `generateGlassMap` 输出放大 3 倍（看编码形态：中心中性、边缘渐偏、角上透明） |
