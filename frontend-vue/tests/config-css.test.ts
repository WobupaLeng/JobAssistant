import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

/**
 * 固定窗 CSS 断言（纯静态文件检查，不渲染 DOM）。
 * 真源：任务给定的窗高定值 + 冻结原型 §11 的滚动容器形状。
 */
const css = readFileSync(new URL('../styles/vue-pages.css', import.meta.url), 'utf8');

function blockOf(sel: string): string {
  const esc = sel.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const m = css.match(new RegExp('^' + esc + '\\s*\\{([^}]*)\\}', 'm'));
  return m ? m[1] : '';
}

describe('配置弹窗固定窗', () => {
  // 用户裁决 K（v7）：窗高随 --ui-scale 放大，但恒不顶破视口（外层 min 与 vh 差值守住）。
  it('.config-modal 高度恒为 min(calc(640px * var(--ui-scale)), calc(100vh - 96px))，不随内容变化', () => {
    expect(blockOf('.config-modal')).toContain(
      'height: min(calc(640px * var(--ui-scale)), calc(100vh - 96px))',
    );
  });
  it('.config-modal 宽度 min(760px, calc(100vw - 64px))，固定定位', () => {
    const b = blockOf('.config-modal');
    expect(b).toContain('width: min(760px, calc(100vw - 64px))');
    expect(b).toContain('position: fixed');
  });
  it('.is-open 才显示为 flex', () => {
    expect(blockOf('.config-modal.is-open')).toContain('display: flex');
  });
  it('.cfg-body 是 flex:1 1 auto + min-height:0 + overflow-y:auto 的滚动容器', () => {
    const b = blockOf('.cfg-body');
    expect(b).toContain('flex: 1 1 auto');
    expect(b).toContain('min-height: 0');
    expect(b).toContain('overflow-y: auto');
  });
  it('.cfg-head / .cfg-foot 不伸缩（flex:0 0 auto）', () => {
    expect(blockOf('.cfg-head')).toContain('flex: 0 0 auto');
    expect(blockOf('.cfg-foot')).toContain('flex: 0 0 auto');
  });
  it('弹窗内 config-card 用统一弹层底色（覆盖 pages.css 的 surface-1）', () => {
    expect(blockOf('.config-modal .config-card')).toContain('background: var(--popup-surface)');
  });
});

describe('CSS 红线', () => {
  it('禁 //! 与 /// 注释', () => {
    expect(css).not.toContain('//!');
    expect(css).not.toContain('///');
  });
  it('无硬编码十六进制颜色（一律走 tokens 变量）', () => {
    expect(/#[0-9a-fA-F]{3,8}/.test(css)).toBe(false);
  });
  it('弹层与分隔线消费 tokens 变量', () => {
    expect(css).toContain('var(--popup-surface)');
    expect(css).toContain('var(--hairline-strong)');
  });
});
