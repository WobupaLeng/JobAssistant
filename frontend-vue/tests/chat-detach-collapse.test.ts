// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import ChatDetach from '../src/components/chat/ChatDetach.vue';

/** Boss 列表底部吸底的收起按钮：展开时出现，点了走与右上角 » 同一个出口，收起后不再渲染。 */
describe('对话框底部收起按钮', () => {
  const base = { friend: null, count: 0 };

  it('展开时渲染在左段里，点击抛出 close', async () => {
    const w = mount(ChatDetach, { props: { ...base, open: true } });
    const rail = w.find('[data-testid="chat-detach-rail"]');
    const btn = rail.find('[data-testid="chat-detach-rail-collapse"]');
    expect(btn.exists()).toBe(true);
    expect(btn.text()).toContain('收起');
    await btn.trigger('click');
    expect(w.emitted('close')).toHaveLength(1);
    w.unmount();
  });

  it('收起后不再显示', () => {
    const w = mount(ChatDetach, { props: { ...base, open: false } });
    expect(w.find('[data-testid="chat-detach-rail-collapse"]').exists()).toBe(false);
    w.unmount();
  });
});