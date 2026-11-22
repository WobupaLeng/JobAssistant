/**
 * 几何收口（REMAINING #1 / #3 / #4）：契约 CSS 字号、`--detach-w` 单一来源、敏感库归属。
 *
 * 前两轮把字号分成了两套：
 *   - `styles/*.css`（11 份契约 + 3 份 vue）里的字号必须一律走 `var(--fs-*)`，
 *     否则 2560 / 3840 的大屏上只有部分控件跟着 `--ui-scale` 放大；
 *   - `--detach-w` / `--rail-w` 只能由 `canvas.ts:syncCanvas()` 写（单一来源），
 *     页面组件里再写一份就会在 resize 时互相打架。
 *
 * 这里用源码扫描把它们钉死：不是为了抓当前 bug，而是防回归。
 */
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { basename, dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

const FRONTEND = join(dirname(fileURLToPath(import.meta.url)), '..');

function read(rel: string): string {
  return readFileSync(join(FRONTEND, rel), 'utf8');
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

/** 去掉注释后，逐行找 font-size: <数字>px。 */
function rawFontSizes(css: string): string[] {
  return stripComments(css)
    .split('\n')
    .filter((line) => /font-size:\s*[\d.]+px/.test(line))
    .map((line) => line.trim());
}

const STYLE_FILES = readdirSync(join(FRONTEND, 'styles'))
  .filter((f) => f.endsWith('.css'))
  // index.css 只做 @import 串联，本身没有字号。
  .filter((f) => f !== 'index.css')
  .sort();

describe('契约样式表：裸 font-size px 归零', () => {
  it('styles/ 下每份 .css 都没有 font-size: <数字>px', () => {
    const offenders: Array<{ file: string; lines: string[] }> = [];
    for (const f of STYLE_FILES) {
      const lines = rawFontSizes(read(`styles/${f}`));
      if (lines.length > 0) offenders.push({ file: f, lines });
    }
    expect(offenders).toEqual([]);
  });

  it('字号只允许出现在 5 档之内（不出现 --fs-3xs 之类不存在的档位）', () => {
    const known = new Set(['2xs', 'xs', 'sm', 'md', 'lg']);
    const bad: Array<{ file: string; name: string }> = [];
    for (const f of STYLE_FILES) {
      const css = stripComments(read(`styles/${f}`));
      for (const m of css.matchAll(/var\(--fs-([a-z0-9-]+)/g)) {
        if (!known.has(m[1])) bad.push({ file: f, name: m[1] });
      }
    }
    expect(bad).toEqual([]);
  });

  it('light 主题没有把字号重新写回 px（主题只换颜色）', () => {
    for (const f of STYLE_FILES) {
      const css = stripComments(read(`styles/${f}`));
      for (const block of css.matchAll(/\[data-theme\s*=\s*['"]?light['"]?\][^{]*\{([^}]*)\}/g)) {
        expect(block[1]).not.toMatch(/font-size:/);
      }
    }
  });

  it('index.css 仍是 14 条 @import，顺序不动', () => {
    const imports = [...read('styles/index.css').matchAll(/@import\s+'\.\/([\w-]+)\.css';/g)].map(
      (m) => m[1],
    );
    expect(imports).toEqual([
      'tokens',
      'layout',
      'controls',
      'jobs',
      'drawer',
      'motion',
      'scrape',
      'pages',
      'sensitive',
      'auto',
      'filter',
      'vue-dashboard',
      'vue-chat',
      'vue-pages',
    ]);
  });
});

describe('--detach-w / --rail-w：单一来源是 canvas.ts', () => {
  const page = read('src/pages/ChatPage.vue');
  const shell = read('src/components/AppShell.vue');
  const canvas = read('src/lib_ui/canvas.ts');
  const css = read('styles/vue-chat.css');

  it('ChatPage 不再写行内 --detach-w / --rail-w', () => {
    expect(page).not.toMatch(/['"]--detach-w['"]\s*:/);
    expect(page).not.toMatch(/['"]--rail-w['"]\s*:/);
    expect(page).not.toContain(':style');
  });

  it('只有 canvas.ts 真正 setProperty 这两个变量', () => {
    expect(canvas).toContain("setProperty('--detach-w'");
    expect(canvas).toContain("setProperty('--rail-w'");
    // 任何 .vue 组件都不许直接写这两个自定义属性。
    for (const rel of ['src/pages/ChatPage.vue', 'src/components/AppShell.vue']) {
      expect(read(rel)).not.toContain("setProperty('--detach-w'");
      expect(read(rel)).not.toContain("setProperty('--rail-w'");
    }
  });

  it('AppShell 负责接线，样式表保留静态兜底（首帧不塌成 0 宽）', () => {
    expect(shell).toContain('useCanvasScale()');
    expect(css).toContain('flex-basis: var(--detach-w)');
    expect(css).toContain('--detach-w: 720px');
  });
});

describe('敏感库：几何回归 sensitive.css 本体，不再靠 vue-pages.css 覆写', () => {
  const sensitive = stripComments(read('styles/sensitive.css'));
  const pageCss = stripComments(read('styles/vue-pages.css'));

  it('sensitive.css 自带居中窗 + 乘 --ui-scale 的宽高', () => {
    expect(sensitive).toContain('width: min(calc(1120px * var(--ui-scale)), calc(100vw - 40px))');
    expect(sensitive).toContain('height: min(calc(720px * var(--ui-scale)), calc(100vh - 56px))');
    expect(sensitive).toContain('max-height: none');
    expect(sensitive).toContain('transform: translate(-50%, -50%)');
  });

  it('打开态归位到居中（不再用 translateX 滑入）', () => {
    const open = sensitive.match(/\.sensitive-panel\.is-open\s*\{([^}]*)\}/);
    expect(open).not.toBeNull();
    expect(open?.[1]).toContain('translate(-50%, -50%)');
    expect(open?.[1]).not.toContain('translateX');
  });

  it('vue-pages.css 里不再出现 .sensitive-panel 规则', () => {
    expect(pageCss).not.toMatch(/\.sensitive-panel\b/);
  });

  it('滚动语义仍是 body 单容器（不是整窗滚动）', () => {
    // 原文写的是 overflow: auto（等价于 overflow-x/y: auto），
    // 关键是「面板本身不滚、正文滚动容器撑满剩余高度」。
    const body = sensitive.match(/\.sensitive-body\s*\{([^}]*)\}/);
    expect(body).not.toBeNull();
    expect(body?.[1]).toMatch(/overflow(-y)?:\s*auto/);
    expect(body?.[1]).toContain('min-height: 0');
  });
});
