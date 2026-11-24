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

const VUE_CSS = ['styles/vue-dashboard.css', 'styles/vue-chat.css', 'styles/vue-pages.css', 'styles/vue-glass.css'] as const;

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
  it('右栏宽乘 scale：2560 → 300×1.333 = 400；3840 → 300×1.5 = 450', () => {
    expect(computeRailWidth(1440)).toBe(300);
    expect(computeRailWidth(1920)).toBe(300);
    expect(computeRailWidth(2560)).toBe(400);
    expect(computeRailWidth(3840)).toBe(450);
  });

  it('窄窗右栏基准 260，同样乘 scale', () => {
    expect(computeRailWidth(RAIL_NARROW_BELOW - 1)).toBe(260);
    expect(computeRailWidth(1440)).toBe(300);
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
      expect(read(rel)).not.toMatch(/`[^`\n]*\*\//);
    });
  }

  it('app.css 的裸 px 字号也归零', () => {
    expect(stripComments(read('src/app.css'))).not.toMatch(/font-size:\s*[\d.]+px/);
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
    expect(css).toContain('--rail-w: calc(300px * var(--ui-scale))');
    // A36 兜底的宿主必须是 :root：挂到更近的祖先（.app-body / 退役前的 .page-chat）
    // 会把 syncCanvas() 写在 <html> 行内的真实值遮掉，展开态对话框实测只剩 1px 描边。
    expect(css).toMatch(/(^|\n)\s*:root\s*\{[^}]*--detach-w: 0px;/);
    expect(css).not.toMatch(/\.app-body\s*\{[^}]*--detach-w/);
    // 兜底与原型 :root 一致为 0px（规格 A36）：聊天默认收起，expand 时由 canvas.ts 写行内值。
    expect(css).toContain('--detach-w: 0px');
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
