// @vitest-environment jsdom
/**
 * 岗位卡长按多选的真实交互（用户裁决 m00725）。
 *
 * 纯状态机在 `scrape-panel.test.ts` 里测；这里只钉组件接线上的三件事，
 * 它们都是「写错就静默坏掉」的边界：
 *
 * 1. 按住满 500ms 才发 `longpress`，且发出的 job_id 是按下那张卡；
 * 2. 中途移出 8px（滚动列表）或抬起，就不再触发；
 * 3. 长按触发后浏览器补的那个 click 必须被吞掉，否则「刚进多选」会立刻被
 *    自己的尾巴当成「再点一下取消勾选」。
 */
import { mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import JobCard from '../src/components/JobCard.vue';
import type { CardView } from '../src/lib_ui/job-list-model';

const VIEW: CardView = {
  tid: 't1',
  jobId: 'boss:1',
  job: { job_id: 'boss:1', title: '前端工程师', company: '某公司' } as CardView['job'],
  title: '前端工程师',
  company: '某公司',
  rawCompany: '某公司',
  salary: '20-30K',
  meta: ['3-5年', '本科'],
  metaText: '3-5年 / 本科',
  addr: '杭州',
  hasAddr: true,
  score: 80,
  scoreTier: 'is-high',
  status: { tone: 'default', label: '待投递' },
  delivered: false,
  rejected: false,
  time: '刚刚',
};

function openIds(w: ReturnType<typeof mountCard>): string[] {
  return (w.emitted('open') ?? []).map((args) => (args[0] as { job_id: string }).job_id);
}

function mountCard() {
  return mount(JobCard, { props: { view: VIEW }, attachTo: document.body });
}

/** jsdom 的 PointerEvent 支持不全，这里造一个带坐标的普通事件喂给处理器。 */
function pointer(el: Element, type: string, x = 10, y = 10): void {
  const ev = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 });
  el.dispatchEvent(ev);
}

describe('JobCard 长按进多选', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  it('按住满 500ms 发出 longpress，携带这张卡的 job_id', () => {
    const w = mountCard();
    const card = w.find('[data-card]').element;
    pointer(card, 'pointerdown');
    // 差一毫秒都不行：499ms 时还不该有动静。
    vi.advanceTimersByTime(499);
    expect(w.emitted('longpress')).toBeUndefined();
    vi.advanceTimersByTime(1);
    expect(w.emitted('longpress')).toEqual([['boss:1']]);
    w.unmount();
  });

  it('按住后移出 8px 就撤销（用户是在滚动列表，不是要选）', () => {
    const w = mountCard();
    const card = w.find('[data-card]').element;
    pointer(card, 'pointerdown', 10, 10);
    pointer(card, 'pointermove', 40, 10);
    vi.advanceTimersByTime(1000);
    expect(w.emitted('longpress')).toBeUndefined();
    // 撤销之后照常单击仍是「打开详情」。
    pointer(card, 'click', 10, 10);
    expect(openIds(w)).toEqual(['boss:1']);
    w.unmount();
  });

  it('抬起就撤销：短按不会误进多选，且仍然打开详情', () => {
    const w = mountCard();
    const card = w.find('[data-card]').element;
    pointer(card, 'pointerdown');
    vi.advanceTimersByTime(200);
    pointer(card, 'pointerup');
    vi.advanceTimersByTime(1000);
    expect(w.emitted('longpress')).toBeUndefined();
    pointer(card, 'click');
    expect(openIds(w)).toEqual(['boss:1']);
    w.unmount();
  });

  it('长按后的那个 click 被吞掉：不会再发一次 open', () => {
    const w = mountCard();
    const card = w.find('[data-card]').element;
    pointer(card, 'pointerdown');
    vi.advanceTimersByTime(500);
    pointer(card, 'pointerup');
    pointer(card, 'click');
    expect(w.emitted('longpress')).toHaveLength(1);
    expect(w.emitted('open')).toBeUndefined();
    // 吞掉一次之后恢复正常：下一次单击照常打开详情。
    pointer(card, 'click');
    expect(openIds(w)).toEqual(['boss:1']);
    w.unmount();
  });

  it('按钮上的长按不归多选管（要能正常点）', () => {
    const w = mount(JobCard, { props: { view: VIEW, tab: 'pending' }, attachTo: document.body });
    const btn = w.find('[data-greet]').element;
    pointer(btn, 'pointerdown');
    vi.advanceTimersByTime(1000);
    expect(w.emitted('longpress')).toBeUndefined();
    w.unmount();
  });
});
