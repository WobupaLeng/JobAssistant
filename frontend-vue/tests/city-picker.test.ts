// @vitest-environment jsdom
/**
 * 城市选择器：状态机（纯逻辑）+ 组件（DOM）。
 *
 * 文件级切 jsdom 而不是改全局 `environment`：本仓库其余用例都是纯函数，
 * 在 node 环境跑得快，只有本文件需要 `document`。漏写这一行会报
 * `document is not defined`。
 */
import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import { PROVINCES, cityName } from '../src/lib_ui/data/cities';
import { CITY_DEFAULT } from '../src/lib_ui/options';
import CityPicker from '../src/components/CityPicker.vue';
import {
  browseSelection,
  cityListFor,
  closePicker,
  handleKey,
  initialPickerState,
  openPicker,
  pickerMode,
  setQuery,
  stepIndex,
} from '../src/lib_ui/city-picker';

const tid = (id: string) => `[data-testid="${id}"]`;

/**
 * 面板查询走 `document.body`，不走 wrapper。
 *
 * 面板是 `<Teleport to="body">` + `position: fixed`（`CityPicker.vue:229`）：
 * 它为了躲开筛选抽屉这个滚动容器（`overflow-y: auto` 会裁掉绝对定位浮层，
 * 且抽屉的 `backdrop-filter` 会新建包含块）被搬出了组件子树。
 * Teleport 之后 wrapper 的 `find/get/findAll` 只在组件自己那棵树里找，
 * 面板元素一个都看不见——判据变成 `document.body`。
 * 触发按钮仍在原地（`scrape-filter-city` 是面板外的 `<button>`），继续用 wrapper。
 */
const panel = () => document.body.querySelector(tid('city-picker-panel'));
const panelAll = (prefix: string) =>
  document.body.querySelectorAll(`[data-testid^="${prefix}"]`);

/** 面板内的元素必须存在：拿到才允许 `dispatchEvent` / 读属性。 */
function inPanel(id: string): HTMLElement {
  const el = document.body.querySelector<HTMLElement>(tid(id));
  expect(el, `面板里找不到 ${id}（Teleport 后的 DOM 里有吗）`).not.toBeNull();
  return el!;
}

/** 点面板内的元素：`@vue/test-utils` 的 `trigger` 只认 wrapper，这里直接派发冒泡事件。 */
async function clickInPanel(id: string): Promise<void> {
  inPanel(id).dispatchEvent(new MouseEvent('click', { bubbles: true }));
  await flush();
}

/** 给面板内的输入框赋值并派发 `input`（Vue 的 v-model 监听它）。 */
async function typeInPanel(id: string, value: string): Promise<void> {
  const el = inPanel(id) as HTMLInputElement;
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
  await flush();
}

/**
 * 挂载并把容器挂到 `document.body` 下面。
 *
 * **必须 attachTo**：不挂载时 `w.get(...)` 的元素不在文档里，
 * `element.focus()` 不生效、`document.activeElement` 恒为 `<body>`，
 * 「自动聚焦搜索框」「Esc 回焦触发按钮」两条断言会假红。
 */
function mountPicker(value = CITY_DEFAULT) {
  return mount(CityPicker, { props: { modelValue: value }, attachTo: document.body });
}

/** 等一次真实宏任务，让 `nextTick` 里的 focus 落地。 */
const flush = () => new Promise((r) => setTimeout(r, 0));

describe('城市选择器 · 状态机（纯逻辑）', () => {
  it('空查询是浏览态，非空查询切搜索态', () => {
    const s0 = initialPickerState();
    expect(pickerMode(s0)).toBe('browse');
    expect(pickerMode(setQuery(s0, '  '))).toBe('browse');
    expect(pickerMode(setQuery(s0, '深圳'))).toBe('search');
  });

  it('打开即清词并回到第一屏；关闭同样清词', () => {
    const dirty = { ...openPicker(initialPickerState()), query: 'sz', provinceIndex: 5 };
    expect(openPicker(dirty)).toMatchObject({ open: true, query: '', provinceIndex: 0, cityIndex: 0 });
    expect(closePicker(dirty)).toMatchObject({ open: false, query: '', provinceIndex: 0, cityIndex: 0 });
  });

  it('环形步进不越界', () => {
    expect(stepIndex(0, -1, 34)).toBe(33);
    expect(stepIndex(33, 1, 34)).toBe(0);
    expect(stepIndex(0, 1, 0)).toBe(0);
  });

  it('省列上下键切省并重置城市下标；右方向键把焦点交给城市列', () => {
    const s0 = openPicker(initialPickerState());
    const down = handleKey(s0, 'ArrowDown');
    expect(down.state.provinceIndex).toBe(1);
    expect(down.state.cityIndex).toBe(0);
    expect(down.state.column).toBe('province');

    const right = handleKey({ ...s0, provinceIndex: 3 }, 'ArrowRight');
    expect(right.state.column).toBe('city');
    const cityDown = handleKey(right.state, 'ArrowDown');
    expect(cityDown.state.column).toBe('city');
    expect(cityDown.state.cityIndex).toBe(1);
    expect(cityDown.state.provinceIndex).toBe(3);
  });

  it('搜索态上下键在结果间移动', () => {
    const s = setQuery(openPicker(initialPickerState()), 'shen');
    const down = handleKey(s, 'ArrowDown');
    expect(down.state.resultIndex).toBe(1);
    expect(down.state.query).toBe('shen');
  });

  it('Enter 提交当前选中并关闭面板', () => {
    const s = openPicker(initialPickerState());
    const hit = handleKey(s, 'Enter');
    expect(hit.commit).toBe(browseSelection(s));
    expect(hit.state.open).toBe(false);
  });
});

describe('城市选择器 · 组件', () => {
  it('默认显示 cityName(CITY_DEFAULT)，testid 仍在触发按钮上', () => {
    const w = mountPicker();
    const trigger = w.get(tid('scrape-filter-city'));
    expect(trigger.element.tagName).toBe('BUTTON');
    expect(trigger.text()).toContain(cityName(CITY_DEFAULT));
    expect(trigger.attributes('aria-haspopup')).toBe('listbox');
    expect(trigger.attributes('aria-expanded')).toBe('false');
    expect(w.find(tid('city-picker-panel')).exists()).toBe(false);
    w.unmount();
  });

  it('点开面板后省份数 = PROVINCES.length，且搜索框自动聚焦', async () => {
    const w = mountPicker();
    await w.get(tid('scrape-filter-city')).trigger('click');
    await flush();
    expect(w.get(tid('scrape-filter-city')).attributes('aria-expanded')).toBe('true');
    expect(panel()).not.toBeNull();
    expect(panelAll('city-picker-province-').length).toBe(PROVINCES.length);
    expect(document.activeElement?.getAttribute('data-testid')).toBe('city-picker-search');
    w.unmount();
  });

  it('点省 → 右列 = 该省 cities.length', async () => {
    const w = mountPicker();
    await w.get(tid('scrape-filter-city')).trigger('click');
    await flush();
    const target = PROVINCES[3];
    await clickInPanel(`city-picker-province-${target.code}`);
    expect(panelAll('city-picker-city-').length).toBe(target.cities.length);
    w.unmount();
  });

  it('点市 → emit 正确编码、面板关闭、搜索词清空', async () => {
    const w = mountPicker();
    await w.get(tid('scrape-filter-city')).trigger('click');
    await flush();
    const p = PROVINCES[0];
    const c = p.cities[0];
    await clickInPanel(`city-picker-province-${p.code}`);
    await clickInPanel(`city-picker-city-${c.code}`);
    expect(w.emitted('update:modelValue')?.[0]).toEqual([c.code]);
    expect(panel()).toBeNull();
    expect(w.get(tid('scrape-filter-city')).attributes('aria-expanded')).toBe('false');
    // 面板已卸载，搜索框 DOM 不存在：重开一次确认搜索词被清空（状态机 closePicker 的契约）
    await w.get(tid('scrape-filter-city')).trigger('click');
    await flush();
    expect((inPanel('city-picker-search') as HTMLInputElement).value).toBe('');
    w.unmount();
  });

  it('搜索「深圳」「shenzhen」「sz」「101280600」都能命中深圳', async () => {
    for (const q of ['深圳', 'shenzhen', 'sz', '101280600']) {
      const w = mountPicker();
      await w.get(tid('scrape-filter-city')).trigger('click');
      await flush();
      await typeInPanel('city-picker-search', q);
      const hit = document.body.querySelector(tid('city-picker-result-101280600'));
      expect(hit, `查询「${q}」未命中深圳`).not.toBeNull();
      expect(hit!.textContent).toContain('深圳');
      expect(panelAll('city-picker-province').length).toBe(0);
      await clickInPanel('city-picker-result-101280600');
      expect(w.emitted('update:modelValue')?.[0]).toEqual(['101280600']);
      w.unmount();
    }
  });

  it('无结果时显示「没有匹配的城市」', async () => {
    const w = mountPicker();
    await w.get(tid('scrape-filter-city')).trigger('click');
    await flush();
    await typeInPanel('city-picker-search', 'zzzzzzzz');
    expect(panelAll('city-picker-result-').length).toBe(0);
    expect(inPanel('city-picker-empty').textContent?.trim()).toBe('没有匹配的城市');
    w.unmount();
  });

  it('未知编码 999999999 原样回显', () => {
    const w = mountPicker('999999999');
    expect(w.get(tid('scrape-filter-city')).text()).toContain('999999999');
    w.unmount();
  });

  it('Esc 关闭面板并把焦点还给触发按钮', async () => {
    const w = mountPicker();
    await w.get(tid('scrape-filter-city')).trigger('click');
    await flush();
    expect(panel()).not.toBeNull();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await w.vm.$nextTick();
    await flush();
    expect(panel()).toBeNull();
    expect(document.activeElement?.getAttribute('data-testid')).toBe('scrape-filter-city');
    w.unmount();
  });

  it('城市列数量与 cityListFor 一致（省级条目口径）', () => {
    const s = { ...openPicker(initialPickerState()), provinceIndex: 8 };
    expect(cityListFor(s).length).toBe(PROVINCES[8].cities.length);
  });
});
