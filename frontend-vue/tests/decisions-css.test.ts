import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

/**
 * 决策记录弹窗固定窗 CSS 断言（纯静态文件检查，不渲染 DOM）。
 * 真源：任务给定的窗高定值 + 冻结原型 §11b 的 dl-row 四列网格。
 */
const css = readFileSync(new URL('../styles/vue-pages.css', import.meta.url), 'utf8');

function blockOf(sel: string): string {
  const esc = sel.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const m = css.match(new RegExp('^' + esc + '\\s*\\{([^}]*)\\}', 'm'));
  return m ? m[1] : '';
}

describe('决策记录弹窗固定窗', () => {
  it('.decisions-modal 高度恒为 min(620px, calc(100vh - 96px))', () => {
    expect(blockOf('.decisions-modal')).toContain('height: min(620px, calc(100vh - 96px))');
  });
  it('.decisions-modal 宽度 min(860px, calc(100vw - 64px))', () => {
    expect(blockOf('.decisions-modal')).toContain('width: min(860px, calc(100vw - 64px))');
  });
  it('.dl-rows 是 flex:1 1 auto + min-height:0 + overflow-y:auto 的滚动容器', () => {
    const b = blockOf('.dl-rows');
    expect(b).toContain('flex: 1 1 auto');
    expect(b).toContain('min-height: 0');
    expect(b).toContain('overflow-y: auto');
  });
  it('.dl-head 只定义一次（不重复）', () => {
    expect((css.match(/^\.dl-head\s*\{/gm) || []).length).toBe(1);
  });
  it('.dl-head 不伸缩（flex:0 0 auto）', () => {
    expect(blockOf('.dl-head')).toContain('flex: 0 0 auto');
  });
  it('.dl-row 四列网格 62px 66px 100px minmax(0,1fr)', () => {
    expect(blockOf('.dl-row')).toContain('grid-template-columns: 62px 66px 100px minmax(0, 1fr)');
  });
  it('入库绿 / 排除红走 tokens', () => {
    expect(blockOf('.dl-row.is-in > .dl-verdict')).toContain('color: var(--good)');
    expect(blockOf('.dl-row.is-out > .dl-verdict')).toContain('color: var(--bad)');
  });
  it('运行日志喂进固定窗时取消 58vh 限制', () => {
    const b = blockOf('.decisions-modal .run-log-box');
    expect(b).toContain('flex: 1 1 auto');
    expect(b).toContain('max-height: none');
  });
});

describe('CSS 红线', () => {
  it('禁 //! 与 /// 注释', () => {
    expect(css).not.toContain('//!');
    expect(css).not.toContain('///');
  });
  it('无硬编码十六进制颜色', () => {
    expect(/#[0-9a-fA-F]{3,8}/.test(css)).toBe(false);
  });
});
