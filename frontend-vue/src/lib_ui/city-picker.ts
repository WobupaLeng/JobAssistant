/**
 * 城市选择器的状态机与键盘规则（纯逻辑，不碰 DOM 与框架）。
 *
 * **为什么放在 `lib_ui/` 而不是 `features/scrape/`**：它是纯状态机（只吃
 * `PROVINCES` / `searchCities`，不碰命令、不碰业务规则），而消费方是
 * `components/CityPicker.vue`。放在 `features/` 会让 `components → features`
 * 成为向上引用，与 `frontend-vue/AGENTS.md` §2 的
 * `pages → features → components → lib_ui` 方向相反。
 *
 * `components/CityPicker.vue` 只负责把这里算出的字段铺到模板上；所有
 * 「什么时候开 / 关」「上下键走到哪一条」「回车选中哪一条」都在本文件里，
 * 因此可以脱离组件单测（`frontend-vue/AGENTS.md` §6：vitest 只给纯逻辑写单测）。
 *
 * 契约：城市始终是**单值** 9 位编码 `string`，本文件不理解也不改写编码语义，
 * `cityName()` 的兜底（未知编码原样回显）由数据层保证。
 */

import {
  PROVINCES,
  searchCities,
  type CityHit,
  type ProvinceNode,
} from './data/cities';

/** 搜索框为空时的两列模式；非空时切到结果列表。 */
export type PickerMode = 'browse' | 'search';

/**
 * 浏览态里 ↑ / ↓ 作用在哪一列。
 *
 * 规格要求「省份列 ↑↓ 切省、→ 进城市列、Enter 选中市」，所以上下键必须先知道
 * 焦点在哪一列；把这一位放进状态机而不是留在组件的 ref 里，是为了让键盘规则
 * 可以脱离组件单测。
 */
export type PickerColumn = 'province' | 'city';

/** 面板当前态。`open` 与 `mode` 分开存，是为了「关闭后重开」能回到浏览态。 */
export interface PickerState {
  open: boolean;
  query: string;
  /** 浏览态上下键的作用列；搜索态忽略。 */
  column: PickerColumn;
  /** `browse` 态：左列选中省的**下标**（不是编码，省份编码可能重复出现在别处）。 */
  provinceIndex: number;
  /** `browse` 态：右列选中的城市下标；跨省切换时必须重置，否则会指到别人家的城市。 */
  cityIndex: number;
  /** `search` 态：结果列表选中下标。 */
  resultIndex: number;
}

/** 初始态：面板关着、浏览态、选第一个省的第一个市。 */
export function initialPickerState(): PickerState {
  return { open: false, query: '', column: 'province', provinceIndex: 0, cityIndex: 0, resultIndex: 0 };
}

/** 当前模式只由搜索词决定（空串 → 浏览，非空 → 搜索）。 */
export function pickerMode(state: PickerState): PickerMode {
  return state.query.trim() ? 'search' : 'browse';
}

/** 左列的省份列表（数据层不可变，直接透出，不做拷贝）。 */
export function provinceList(): readonly ProvinceNode[] {
  return PROVINCES;
}

/** 右列：当前省的城市列表。 */
export function cityListFor(state: PickerState): readonly CityHit[] {
  const p = PROVINCES[state.provinceIndex];
  if (!p) return [];
  return p.cities.map((c) => ({
    code: c.code,
    name: c.name,
    pinyin: c.pinyin,
    province: p.name,
  }));
}

/** 搜索态结果（数据层已去重、已限流，直接透出）。 */
export function resultList(state: PickerState): readonly CityHit[] {
  return searchCities(state.query);
}

/** 打开面板：保留搜索词清空与下标归零，保证每次打开都是干净的第一屏。 */
export function openPicker(state: PickerState): PickerState {
  void state;
  return { ...initialPickerState(), open: true };
}

/**
 * 关闭面板：搜索词必须清空（用户裁决第 4 条：任何关闭路径都要清词）。
 * 选中值不在这里处理——它由调用方 emit 出去。
 */
export function closePicker(state: PickerState): PickerState {
  void state;
  return initialPickerState();
}

/** 写搜索词：切模式时把结果下标归零，避免沿用上一轮的位置。 */
export function setQuery(state: PickerState, query: string): PickerState {
  return { ...state, query, resultIndex: 0, column: 'province' };
}

/** 上下键的环形移动（-1 / +1），越界回到另一端。 */
export function stepIndex(index: number, delta: number, length: number): number {
  if (length <= 0) return 0;
  return (index + delta + length) % length;
}

/** 浏览态 ↑ / ↓：移动省份，并把右列下标归零。 */
export function moveProvince(state: PickerState, delta: number): PickerState {
  const next = stepIndex(state.provinceIndex, delta, PROVINCES.length);
  return { ...state, column: 'province', provinceIndex: next, cityIndex: 0 };
}

/** 浏览态：省份列 ← / 城市列 → 切换右列的上下移动目标。 */
export function moveCity(state: PickerState, delta: number): PickerState {
  const next = stepIndex(state.cityIndex, delta, cityListFor(state).length);
  return { ...state, column: 'city', cityIndex: next };
}

/** 搜索态 ↑ / ↓。 */
export function moveResult(state: PickerState, delta: number): PickerState {
  const next = stepIndex(state.resultIndex, delta, resultList(state).length);
  return { ...state, resultIndex: next };
}

/** 浏览态回车 / 点城市 → 该省的 `cityIndex` 城市编码。 */
export function browseSelection(state: PickerState): string | null {
  const p = PROVINCES[state.provinceIndex];
  const c = p?.cities[state.cityIndex];
  return c ? c.code : null;
}

/** 搜索态回车 → 结果里 `resultIndex` 的编码。 */
export function searchSelection(state: PickerState): string | null {
  const hit = searchCities(state.query)[state.resultIndex];
  return hit ? hit.code : null;
}

/**
 * 键盘总调度：返回 `{ state, commit }`。
 *
 * - `commit` 非空表示「这一下要选中该编码并关闭面板」，由组件负责 emit；
 * - `null` 表示只是移动 / 切模式 / 关闭，组件只需把 `state` 写回。
 *
 * `Esc` 的关闭由组件处理（它还要把焦点还给触发按钮），这里只服务列表导航。
 */
export function handleKey(
  state: PickerState,
  key: string,
): { state: PickerState; commit: string | null } {
  const mode = pickerMode(state);
  const delta = key === 'ArrowDown' ? 1 : key === 'ArrowUp' ? -1 : 0;
  if (delta !== 0) {
    if (mode === 'search') return { state: moveResult(state, delta), commit: null };
    // 焦点列决定上下键改哪一边：省份列切省、城市列切市
    return { state: state.column === 'city' ? moveCity(state, delta) : moveProvince(state, delta), commit: null };
  }
  if (mode === 'browse' && key === 'ArrowRight') {
    return { state: { ...state, column: 'city' }, commit: null };
  }
  if (mode === 'browse' && key === 'ArrowLeft') {
    return { state: { ...state, column: 'province' }, commit: null };
  }
  if (key === 'Enter') {
    const code = mode === 'search' ? searchSelection(state) : browseSelection(state);
    return { state: closePicker(state), commit: code };
  }
  return { state, commit: null };
}
