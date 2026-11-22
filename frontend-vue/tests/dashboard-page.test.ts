// @vitest-environment jsdom
/**
 * 概览页：抓取筛选栏 + 城市槽位接线。
 *
 * 本轮只验「城市接进去了」：容器 testid 由 `pageTestId('dashboard')` 派生、
 * 筛选栏存在、`CityPicker` 在栏内且显示默认城市、选中后落到页面持有的筛选状态上。
 * 薪资 / 经验 等其余槽位与 `scrape_start` 属后续阶段，这里断言它们**还没有**，
 * 防止有人提前把它们塞回来却没人接线。
 *
 * 文件级切 jsdom 而不是改全局 `environment`：本仓库其余用例都是纯函数，漏写这一行
 * 会报 `document is not defined`。
 */
import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';

import DashboardPage from '../src/pages/DashboardPage.vue';
import CityPicker from '../src/components/CityPicker.vue';
import { createFilter, filterPayload, isDefaultFilter } from '../src/features/scrape/filter';
import { CITY_DEFAULT } from '../src/lib_ui/options';
import { cityName } from '../src/lib_ui/data/cities';
import { NAV_ALL, pageTestId } from '../src/lib_ui/nav';

const tid = (id: string) => `[data-testid="${id}"]`;

describe('概览页 · 页面容器', () => {
  it('根容器 testid 由 pageTestId 派生，不手写字符串', () => {
    const w = mount(DashboardPage);
    const root = w.get(tid('page-dashboard'));
    expect(root.element.tagName).toBe('SECTION');
    expect(pageTestId('dashboard')).toBe('page-dashboard');
    expect(w.find(tid('scrape-filter-bar')).exists()).toBe(true);
    w.unmount();
  });

  it('没有新增页面别名 testid：页面级锚点只认 page-<slug> 规则', () => {
    const w = mount(DashboardPage);
    const ids = w.findAll('[data-testid]').map((n) => n.attributes('data-testid') ?? '');
    const pageAnchors = ids.filter((id) => id.startsWith('page-'));
    expect(pageAnchors).toEqual(['page-dashboard']);
    for (const id of pageAnchors) {
      expect(NAV_ALL.map((n) => pageTestId(n))).toContain(id);
    }
    w.unmount();
  });
});

describe('概览页 · 城市槽位', () => {
  it('CityPicker 挂在筛选栏里，显示默认城市', () => {
    const w = mount(DashboardPage);
    const bar = w.get(tid('scrape-filter-bar'));
    const picker = bar.getComponent(CityPicker);
    expect(picker.vm.$el.tagName).toBe('DIV');
    const trigger = bar.get(tid('scrape-filter-city'));
    expect(trigger.element.tagName).toBe('BUTTON');
    expect(trigger.text()).toContain(cityName(CITY_DEFAULT));
    w.unmount();
  });

  it('点省点市 → 选中结果落到筛选状态上（而不是消失在 emit 里）', async () => {
    const w = mount(DashboardPage);
    const bar = w.get(tid('scrape-filter-bar'));
    await bar.get(tid('scrape-filter-city')).trigger('click');
    const firstProvince = bar.get('[data-testid^="city-picker-province-"]');
    await firstProvince.trigger('click');
    const firstCity = bar.get('[data-testid^="city-picker-city-"]');
    const code = firstCity.attributes('data-testid')!.replace('city-picker-city-', '');
    await firstCity.trigger('click');

    // 面板关闭、触发按钮回显新城市 → 说明 v-model 真的写回了页面状态
    expect(bar.find(tid('city-picker-panel')).exists()).toBe(false);
    expect(bar.get(tid('scrape-filter-city')).text()).toContain(cityName(code));
    w.unmount();
  });

  it('筛选栏里本轮只有城市一个槽位', () => {
    const w = mount(DashboardPage);
    const bar = w.get(tid('scrape-filter-bar'));
    expect(bar.findAll('.scrape-filter-field').length).toBe(1);
    expect(bar.getComponent(CityPicker).vm.$el.classList.contains('city-picker')).toBe(true);
    w.unmount();
  });
});

describe('概览页 · 筛选状态骨架（纯逻辑）', () => {
  it('初始城市为 CITY_DEFAULT，且判定为默认筛选', () => {
    const f = createFilter();
    expect(f.city).toBe(CITY_DEFAULT);
    expect(isDefaultFilter(f)).toBe(true);
  });

  it('createFilter 每次返回新对象', () => {
    const a = createFilter();
    const b = createFilter();
    expect(a).not.toBe(b);
    a.city = '101010100';
    expect(b.city).toBe(CITY_DEFAULT);
  });

  it('载荷只带城市，改城市后不再是默认筛选', () => {
    const f = { ...createFilter(), city: '101010100' };
    expect(isDefaultFilter(f)).toBe(false);
    expect(filterPayload(f)).toEqual({ city: '101010100' });
    expect(Object.keys(filterPayload(f))).toEqual(['city']);
  });
});
