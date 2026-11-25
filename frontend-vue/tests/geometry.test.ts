/**
 * 几何回归（用户裁决 K/L，v7/v8）：`--ui-scale`、5 档字号、三窗固定高、A54 方块。
 *
 * 这一轮把上一版「把字号换成裸 px」的做法整个翻回来：原型是「1920 基准 + 等比放大」，
 * 字号写死 px 会让 2560 / 3840 的大屏上字不跟着放大。所以下面三类断言缺一不可：
 *
 * 1. **纯函数**：`computeUiScale` 五档，边界与封顶；
 * 2. **令牌**：tokens.css 里恰好 5 档 `--fs-*`，且都乘 `--ui-scale`；
 * 3. **样式文件**：vue 系样式里裸 px 字号 = 0，三窗高度与 A54 公式在产物里命中。
 */
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  MODAL_BASE_HEIGHT,
  MODAL_VIEWPORT_GAP,
  RAIL_NARROW_BELOW,
  UI_SCALE_MAX,
  computeDetachWidth,
  computeRailWidth,
  computeUiScale,
} from '../src/lib_ui/canvas';

const FRONTEND = join(dirname(fileURLToPath(import.meta.url)), '..');

function read(rel: string): string {
  return readFileSync(join(FRONTEND, rel), 'utf8');
}

/** 去掉 CSS 注释，避免注释里的示例文字被当成真规则。 */
function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

const VUE_CSS = ['styles/vue-dashboard.css', 'styles/vue-chat.css', 'styles/vue-pages.css'] as const;

describe('computeUiScale：clamp(1, innerWidth / 1920, 1.5)', () => {
  it('五档：1200/1440/1920 → 1，2560 → 1.333，3840 → 1.5', () => {
    expect(computeUiScale(1200)).toBe(1);
    expect(computeUiScale(1440)).toBe(1);
    expect(computeUiScale(1920)).toBe(1);
    expect(computeUiScale(2560)).toBe(1.333);
    expect(computeUiScale(3840)).toBe(1.5);
  });

  it('只放大不缩小：≤1920 恒为 1（1280 / 1600 也是 1）', () => {
    for (const w of [800, 1024, 1280, 1600, 1919, 1920]) {
      expect(computeUiScale(w)).toBe(1);
    }
  });

  it('封顶 1.5：再宽也不放大（5120 → 1.5 而不是 2.667）', () => {
    expect(computeUiScale(5120)).toBe(UI_SCALE_MAX);
    expect(computeUiScale(7680)).toBe(UI_SCALE_MAX);
  });

  it('单调不减且落在 [1, 1.5]', () => {
    let prev = 1;
    for (let w = 320; w <= 4000; w += 40) {
      const s = computeUiScale(w);
      expect(s).toBeGreaterThanOrEqual(1);
      expect(s).toBeLessThanOrEqual(UI_SCALE_MAX);
      expect(s).toBeGreaterThanOrEqual(prev);
      prev = s;
    }
  });

  it('非法输入回落到 1（首帧 innerWidth 可能是 0 / NaN）', () => {
    expect(computeUiScale(0)).toBe(1);
    expect(computeUiScale(-100)).toBe(1);
    expect(computeUiScale(Number.NaN)).toBe(1);
    expect(computeUiScale(Number.POSITIVE_INFINITY)).toBe(1);
  });
});

describe('右栏宽 / 对话框宽', () => {
  it('右栏宽乘 scale：2560 → 360×1.333 = 480；3840 → 360×1.5 = 540', () => {
    expect(computeRailWidth(1440)).toBe(360);
    expect(computeRailWidth(1920)).toBe(360);
    expect(computeRailWidth(2560)).toBe(480);
    expect(computeRailWidth(3840)).toBe(540);
  });

  it('窄窗右栏基准 280，同样乘 scale', () => {
    expect(computeRailWidth(RAIL_NARROW_BELOW - 1)).toBe(280);
    expect(computeRailWidth(1440)).toBe(360);
  });

  it('对话框宽不乘 scale：仍是 clamp(600, 50vw, 80vw)', () => {
    expect(computeDetachWidth(1440)).toBe(720);
    expect(computeDetachWidth(2560)).toBe(1280);
    expect(computeDetachWidth(3840)).toBe(1920);
  });
});

describe('tokens.css：5 档字号令牌', () => {
  const tokens = read('styles/tokens.css');

  it('--ui-scale 存在且基准为 1', () => {
    expect(tokens).toContain('--ui-scale: 1;');
  });

  it('恰好 5 档 --fs-*，且每档都乘 --ui-scale', () => {
    const decls = [...tokens.matchAll(/--fs-([a-z0-9]+):\s*calc\((\d+)px\s*\*\s*var\(--ui-scale\)\);/g)];
    expect(decls.map((d) => d[1])).toEqual(['2xs', 'xs', 'sm', 'md', 'lg']);
    expect(decls.map((d) => Number(d[2]))).toEqual([11, 12, 13, 15, 20]);
  });

  it('没有第 6 档（新增档位视为破坏契约）', () => {
    const names = new Set([...tokens.matchAll(/--fs-([a-z0-9]+)\s*:/g)].map((m) => m[1]));
    expect([...names].sort()).toEqual(['2xs', 'lg', 'md', 'sm', 'xs']);
  });

  it('C5 未被破坏：body 仍是 background: transparent', () => {
    expect(tokens).toMatch(/body\s*\{[^}]*background:\s*transparent/);
  });

  it('C7 未被破坏：浅色块仍在', () => {
    expect(tokens).toContain("[data-theme='light']");
  });
});

describe('vue 系样式：裸 px 字号归零', () => {
  for (const rel of VUE_CSS) {
    it(`${rel} 不出现 font-size: <数字>px`, () => {
      const css = stripComments(read(rel));
      expect(css).not.toMatch(/font-size:\s*[\d.]+px/);
    });

    it(`${rel} 不出现 //! 或 /// 注释`, () => {
      expect(read(rel)).not.toMatch(/^\s*\/\/[/!]/m);
    });

    it(`${rel} 不出现 hex / rgb 字面色值`, () => {
      expect(stripComments(read(rel))).not.toMatch(/#[0-9a-fA-F]{3,8}\b|\brgba?\(/);
    });

    it(`${rel} 注释里不出现反引号包住星号斜杠的写法`, () => {
      // 浏览器把 `*/` 当成块注释的**结束**，所以「反引号 … `*/`（收尾）」这种写法
      // 会让整份样式表被静默丢弃。判定只在注释区里找，并且只认真正的收尾
      // （`*/` 之后是行尾或空白），不会把 `styles/*.css` 这类通配路径误报成违规。
      const comments = read(rel).match(/\/\*[\s\S]*?\*\//g) ?? [];
      for (const block of comments) {
        expect(block).not.toMatch(/`[^`\n]*\*\/(?=\s|$)/);
      }
    });
  }

  it('app.css 的裸 px 字号也归零', () => {
    expect(stripComments(read('src/app.css'))).not.toMatch(/font-size:\s*[\d.]+px/);
  });
});

/**
 * 归属守卫：**同一元素的自身几何与材质只允许一个样式文件定义**。
 *
 * 起因是一次真实回归：`.sensitive-panel` 的底色 / 磨砂 / pointer-events 被
 * sensitive.css、layout.css、motion.css 三处各写一份，谁改一处都会在别处冒出来，
 * 表现为「点关闭没反应」「被遮挡的部分变透明」「列表点不动」这类与改动毫不相干的故障。
 *
 * 边界：动效文件（motion.css）只许写 transition / animation / transform，
 * 不许碰 background / backdrop-filter / pointer-events。
 */
const PANEL_OWNER: Readonly<Record<string, string>> = {
  '.sensitive-panel': 'styles/sensitive.css',
  '.drawer': 'styles/drawer.css',
  '.config-modal': 'styles/vue-pages.css',
  '.decisions-modal': 'styles/vue-pages.css',
};

const MOTION_ONLY = 'styles/motion.css';

const ALL_CSS = [
  'styles/tokens.css',
  'styles/layout.css',
  'styles/controls.css',
  'styles/jobs.css',
  'styles/drawer.css',
  'styles/motion.css',
  'styles/scrape.css',
  'styles/pages.css',
  'styles/sensitive.css',
  'styles/auto.css',
  'styles/filter.css',
  'styles/vue-dashboard.css',
  'styles/vue-chat.css',
  'styles/vue-pages.css',
  'src/app.css',
] as const;

/** 把样式表拆成「选择器 → 声明体」的块，注释先剥掉。 */
function ruleBlocks(css: string): { selector: string; body: string }[] {
  const out: { selector: string; body: string }[] = [];
  const text = stripComments(css);
  const re = /([^{}]+)\{([^{}]*)\}/g;
  let m: RegExpExecArray | null = null;
  while ((m = re.exec(text)) !== null) {
    out.push({ selector: m[1].replace(/\s+/g, ' ').trim(), body: m[2] });
  }
  return out;
}

describe('日志卡：scrape-feed 包裹层不抢 .log-rows 的滚动高度', () => {
  it('.log-feed 撑满卡体余高，且允许子项收缩（否则整块被裁、不能滚）', () => {
    const block = ruleBlocks(read('styles/vue-dashboard.css')).find((r) => r.selector === '.log-feed');
    expect(block?.body).toContain('display: flex');
    expect(block?.body).toContain('flex: 1 1 auto');
    expect(block?.body).toContain('min-height: 0');
    expect(block?.body).toContain('overflow: hidden');
  });
});

describe('归属：面板的自身几何与材质只有一个写者', () => {
  for (const [sel, owner] of Object.entries(PANEL_OWNER)) {
    it(`${sel} 的背景 / 磨砂只在 ${owner} 声明`, () => {
      const offenders: string[] = [];
      for (const file of ALL_CSS) {
        if (file === owner) continue;
        for (const { selector, body } of ruleBlocks(read(file))) {
          if (!selector.includes(sel)) continue;
          if (/(^|[\s;{])(background|backdrop-filter|-webkit-backdrop-filter)\s*:/.test(body)) {
            offenders.push(`${file} → ${selector}`);
          }
        }
      }
      expect(offenders).toEqual([]);
    });
  }

  it('motion.css 只写动效：面板选择器里不出现 background / backdrop-filter / pointer-events', () => {
    const offenders: string[] = [];
    for (const { selector, body } of ruleBlocks(read(MOTION_ONLY))) {
      if (!Object.keys(PANEL_OWNER).some((sel) => selector.includes(sel))) continue;
      if (/(background|backdrop-filter|pointer-events)\s*:/.test(body)) offenders.push(selector);
    }
    expect(offenders).toEqual([]);
  });

  it('layout.css 不再跨文件改写上述面板的背景与磨砂', () => {
    const offenders: string[] = [];
    for (const { selector, body } of ruleBlocks(read('styles/layout.css'))) {
      if (!Object.keys(PANEL_OWNER).some((sel) => selector.includes(sel))) continue;
      if (/(^|[\s;{])(background|backdrop-filter|-webkit-backdrop-filter)\s*:/.test(body)) {
        offenders.push(selector);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('减少动态效果的降级不再用通配符 + !important 压全局', () => {
    const css = stripComments(read('styles/controls.css'));
    const m = /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([\s\S]*?)\n\}/.exec(css);
    expect(m).not.toBeNull();
    const body = m === null ? '' : m[1];
    // 通配符选择器（`*`、`*::before`、`*::after`）在这里一律不允许。
    expect(body).not.toMatch(/(^|[\s,{])\*(::[a-z-]+)?\s*[,{]/);
  });
});

describe('三窗固定高 / 敏感库 720 / A54 方块', () => {
  it('敏感库高 = min(calc(720px * scale), calc(100vh - 56px))', () => {
    expect(MODAL_BASE_HEIGHT.sensitive).toBe(720);
    // 几何已收编回契约样式表本体（REMAINING#4）：vue-pages.css 不再做特异性覆写，
    // 所以这里读 styles/sensitive.css，否则会一直读一份「已经不存在的规则」。
    const css = stripComments(read('styles/sensitive.css'));
    expect(css).toContain(
      `height: min(calc(${MODAL_BASE_HEIGHT.sensitive}px * var(--ui-scale)), calc(100vh - ${MODAL_VIEWPORT_GAP.sensitive}px))`,
    );
  });

  it('配置窗 640 / 决策窗 620 同样乘 scale', () => {
    const css = stripComments(read('styles/vue-pages.css'));
    expect(css).toContain(
      `height: min(calc(${MODAL_BASE_HEIGHT.config}px * var(--ui-scale)), calc(100vh - ${MODAL_VIEWPORT_GAP.config}px))`,
    );
    expect(css).toContain(
      `height: min(calc(${MODAL_BASE_HEIGHT.decisions}px * var(--ui-scale)), calc(100vh - ${MODAL_VIEWPORT_GAP.decisions}px))`,
    );
  });

  it('A53：打招呼方块缩小为紧凑按钮 min-width = --fs-sm × 3.6，高度 30px', () => {
    const css = stripComments(read('styles/vue-dashboard.css'));
    expect(css).toContain('min-width: calc(var(--fs-sm) * 3.6)');
    // 用户裁决：打招呼缩小（m01710），按钮定高 30px。
    expect(css).toContain('height: 30px');
    expect(css).not.toContain('aspect-ratio: 1 / 1');
  });

  // A54（states.md:809）：≤1599 展开态 `.job-card { min-width: calc(320px * var(--ui-scale)) }`。
  // 曾经是「占位待裁决」的反向守卫，用户裁决「补齐实现·规格不动」后已反转为正向断言。
  it('A54：≤1599 展开态 .job-card 有 320×scale 下限，且网格不靠 overflow-x 掩盖', () => {
    const css = stripComments(read('styles/vue-dashboard.css'));
    expect(css).toContain('min-width: calc(320px * var(--ui-scale))');
    // 选择器必须挂在展开态、且落在 ≤1599 的媒体查询里（不是全局撒一条）。
    const opened = css.indexOf('.app-shell.is-chat-open .job-card');
    expect(opened).toBeGreaterThan(-1);
    const mediaBefore = css.lastIndexOf('@media (max-width: 1599px)', opened);
    expect(mediaBefore).toBeGreaterThan(-1);

    // `.job-grid` 与展开态 `.job-grid` 都不得用 overflow-x: hidden 掩盖溢出。
    const gridBlocks = [
      ...css.matchAll(/\.app-shell\.is-chat-open \.job-grid\s*\{([^}]*)\}/g),
      ...css.matchAll(/(?:^|\n)\.job-grid\s*\{([^}]*)\}/g),
    ].map((m) => m[1]);
    expect(gridBlocks.length).toBeGreaterThan(0);
    for (const block of gridBlocks) expect(block).not.toContain('overflow-x: hidden');

    // 打印并断言推断表：三档下「1 列 / 2 列」可用宽度都 ≥ 320px，下限只是兜底。
    const table: Array<{ w: number; mode: string; cols: number; card: number }> = [];
    for (const w of [1200, 1440, 1600]) {
      const detach = Math.round(Math.min(Math.max(w * 0.5, 600), w * 0.8));
      const main = w - detach; // 展开态右栏让位，主列 = 窗宽 − 对话框
      const cols = w < 1600 ? 1 : 2;
      const card = Math.floor((main - 10 * (cols - 1) - 28) / cols); // 10px gap + 14px×2 内边距
      table.push({ w, mode: `${detach} 对话框 / ${main} 主列`, cols, card });
      expect(card).toBeGreaterThanOrEqual(320);
    }
    // 顺手把推断值钉住（若将来断点/内边距变化会立刻报警）。
    expect(table.map((r) => r.cols)).toEqual([1, 1, 2]);
    expect(table.every((r) => r.card >= 320)).toBe(true);
  });

  it('右栏宽兜底与对话框兜底都在样式里，且右栏乘 scale', () => {
    const css = stripComments(read('styles/vue-chat.css'));
    expect(css).toContain('--rail-w: calc(360px * var(--ui-scale))');
    // A36 兜底的宿主必须是 :root：挂到更近的祖先（.app-body / 退役前的 .page-chat）
    // 会把 syncCanvas() 写在 <html> 行内的真实值遮掉，展开态对话框实测只剩 1px 描边。
    expect(css).toMatch(/(^|\n)\s*:root\s*\{[^}]*--detach-w: 0px;/);
    expect(css).not.toMatch(/\.app-body\s*\{[^}]*--detach-w/);
    // 兜底与原型 :root 一致为 0px（规格 A36）：聊天默认收起，expand 时由 canvas.ts 写行内值。
    expect(css).toContain('--detach-w: 0px');
  });

it('岗位卡行高锁定 142px、三行占满 1440×900 窗口（用户 m03968 裁决）', () => {
  const css = stripComments(read('styles/vue-dashboard.css'));
  const grid = css.match(/(?:^|\n)\.job-grid\s*\{([^}]*)\}/);
  expect(grid).not.toBeNull();
  const block = grid![1];

  // 行高是单一像素值，不是 minmax(下限, 1fr)：1fr 会让行高随窗口无限放大
  // （1000 高时到 172px），卡内内容被拉开，看起来「没占满也没对齐」。
  expect(block).toContain('--job-card-h: 142px');
  expect(block).toContain('grid-template-rows: repeat(3, var(--job-card-h))');
  expect(block).toContain('grid-auto-rows: var(--job-card-h)');
  // 142px 的来源（CDP 实测，不是估算）：900 − 14(外壳底边距) − 46(顶栏) = 840；
  // − 42(抓取条) − 20(两处页面 gap) − 220(日志卡) = 558（列表卡可用高）；
  // − 40(页签行) − 40(批量条 32+8) = 478（网格盒高）；
  // − 24(网格内边) − 28(两处行 gap) = 426，除以 3 正好 142。
  // 列宽仍用 1fr（自适应），只有行高锁死；这条断言只针对行声明。
  expect(block).not.toMatch(/grid-template-rows:\s*repeat\(3,\s*minmax\(/);
  expect(block).not.toMatch(/grid-auto-rows:\s*minmax\(/);
  // 行高定死后余量不能摊进行里：从顶部排列，多出来的高度落在底部。
  expect(block).toContain('align-content: start');

  // 加载哨兵是网格的最后一个子项，会占一个隐式行。它必须从行份额里脱身，
  // 否则第 4 个行槽会把三行卡片挤矮。
  const sentinel = css.match(/(?:^|\n)\.job-load-sentinel\s*\{([^}]*)\}/);
  expect(sentinel).not.toBeNull();
  expect(sentinel![1]).toContain('position: absolute');
});

it('窄窗不出现整窗滚动条：body 两个方向都裁死，宽度交给各列（用户 m03014 裁决）', () => {
  const dash = stripComments(read('styles/vue-dashboard.css'));
  const body = dash.match(/(?:^|\n)body\s*\{([^}]*)\}/);
  expect(body).not.toBeNull();
  // 早先这里放开 overflow-x: auto，窗口 minWidth 1200 后被 body 自己当成可滚内容，
  // 整窗多出一条横向滚动条（用户 m03014「最窄窗口大小底部出现了滚动条」）。
  // 现在两个方向都裁死，横向裁切交给 .app-shell 的 overflow:hidden。
  expect(body![1]).not.toContain('overflow-x: auto');
  expect(body![1]).toContain('overflow-x: hidden');
  expect(body![1]).toContain('overflow-y: hidden');
    // 1200 基线宿主仍是 .app-shell；契约宿主 tokens.css 的 body overflow:hidden 不改。
    expect(dash).toContain('min-width: 1200px');
    expect(read('styles/tokens.css')).toMatch(/body\s*\{[^}]*overflow:\s*hidden/);
  });
});

describe('产物 CSS（dist/）包含几何契约', () => {
  const assets = join(FRONTEND, 'dist', 'assets');
  const built = existsSync(assets);

  it.skipIf(!built)('dist CSS 含 --ui-scale、5 档 --fs-*、敏感库 720 与 A53 方块尺寸', () => {
    const files = readFileSync(join(FRONTEND, 'dist', 'index.html'), 'utf8').match(/assets\/[^"]+\.css/g) ?? [];
    expect(files.length).toBeGreaterThan(0);
    const css = files.map((f) => readFileSync(join(FRONTEND, 'dist', f), 'utf8')).join('\n');

    expect(css).toContain('--ui-scale');
    for (const step of ['2xs', 'xs', 'sm', 'md', 'lg']) {
      expect(css).toContain(`--fs-${step}`);
    }
    expect(css).toContain('720px');
    expect(css).toContain('calc(var(--fs-sm) * 3.6)');
  });
});

describe('接线：AppShell 调 syncCanvas，顺序先 scale 后 detach', () => {
  const shell = read('src/components/AppShell.vue');
  const canvas = read('src/lib_ui/canvas.ts');

  it('外壳使用 useCanvasScale()', () => {
    expect(shell).toContain("import { useCanvasScale } from '../lib_ui/use-canvas-scale';");
    expect(shell).toContain('const canvas = useCanvasScale();');
  });

  it('syncCanvas 里 --ui-scale 先于 --rail-w / --detach-w 写入', () => {
    const iScale = canvas.indexOf("setProperty('--ui-scale'");
    const iRail = canvas.indexOf("setProperty('--rail-w'");
    const iDetach = canvas.indexOf("setProperty('--detach-w'");
    expect(iScale).toBeGreaterThan(0);
    expect(iScale).toBeLessThan(iRail);
    expect(iScale).toBeLessThan(iDetach);
  });
});
