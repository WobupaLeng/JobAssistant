<script setup lang="ts">
/**
 * 城市选择器：省 → 市 两级联动 + 输入框搜索（单值 9 位编码）。
 *
 * 为什么要它：Leptos 版是 `features/scrape/filter.rs:1100-1115` 的 `<select>`，
 * 只有 8 个热门城市写死；BOSS 官方表有 34 省 / 373 市，`<select>` 铺不下。
 *
 * 契约（不可丢）：
 * 1. `data-testid="scrape-filter-city"` 必须留在**触发按钮**上——Leptos 版这个
 *    testid 挂在 `<select>` 上，Vue 版换成按钮后 id 字面量不变，否则下游 E2E 与
 *    `filter.rs` 的锚点全部失联；
 * 2. 城市是**单值 string**，本组件只 emit 一个 9 位编码，不碰多选；
 * 3. 交互状态机在 `city-picker.ts`（纯逻辑、可单测），本文件只做 DOM 与焦点。
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';

import { cityName } from '../lib_ui/data/cities';
import {
  browseSelection,
  cityListFor,
  closePicker,
  handleKey,
  initialPickerState,
  openPicker,
  pickerMode,
  provinceList,
  resultList,
  setQuery,
  type PickerState,
} from '../lib_ui/city-picker';

const props = defineProps<{
  /** 当前选中的城市编码（9 位）。未知编码由 `cityName` 原样回显。 */
  modelValue: string;
  /** 无障碍名称，默认「城市」。 */
  label?: string;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', code: string): void;
}>();

const state = ref<PickerState>(initialPickerState());
const rootEl = ref<HTMLElement | null>(null);
const triggerEl = ref<HTMLButtonElement | null>(null);
const searchEl = ref<HTMLInputElement | null>(null);

const label = computed(() => props.label ?? '城市');
const display = computed(() => cityName(props.modelValue));
const mode = computed(() => pickerMode(state.value));
const provinces = provinceList();
const cities = computed(() => cityListFor(state.value));
const results = computed(() => resultList(state.value));
const activeProvince = computed(() => provinces[state.value.provinceIndex]);
const cityColumnActive = computed(() => state.value.column === 'city');

/** 选中一个编码：emit → 关面板（清词）→ 焦点还给触发按钮。 */
function commit(code: string): void {
  emit('update:modelValue', code);
  state.value = closePicker(state.value);
  void nextTick(() => triggerEl.value?.focus());
}

function toggle(): void {
  state.value = state.value.open ? closePicker(state.value) : openPicker(state.value);
}

function close(): void {
  state.value = closePicker(state.value);
  triggerEl.value?.focus();
}

function onSearchInput(ev: Event): void {
  const target = ev.target as HTMLInputElement;
  state.value = setQuery(state.value, target.value);
}

function onSearchKeydown(ev: KeyboardEvent): void {
  if (ev.key === 'Escape') {
    ev.stopPropagation();
    close();
    return;
  }
  const next = handleKey(state.value, ev.key);
  // 方向键要吃掉默认滚动，否则页面跟着跳
  if (ev.key.startsWith('Arrow') || ev.key === 'Enter') ev.preventDefault();
  if (next.commit) {
    commit(next.commit);
    return;
  }
  state.value = next.state;
}

function onProvinceClick(index: number): void {
  state.value = { ...state.value, column: 'province', provinceIndex: index, cityIndex: 0 };
}

function onProvinceKeydown(ev: KeyboardEvent, index: number): void {
  if (ev.key === 'Enter' || ev.key === ' ') {
    ev.preventDefault();
    onProvinceClick(index);
    return;
  }
  if (ev.key.startsWith('Arrow')) {
    ev.preventDefault();
    state.value = handleKey(state.value, ev.key).state;
  }
}

function onCityKeydown(ev: KeyboardEvent): void {
  if (ev.key.startsWith('Arrow')) {
    ev.preventDefault();
    state.value = handleKey(state.value, ev.key).state;
  }
}

function onCityClick(code: string): void {
  commit(code);
}

function onOutsidePointer(ev: PointerEvent): void {
  if (!state.value.open) return;
  const root = rootEl.value;
  if (root && ev.target instanceof Node && root.contains(ev.target)) return;
  close();
}

function onDocumentKeydown(ev: KeyboardEvent): void {
  if (!state.value.open) return;
  if (ev.key === 'Escape') {
    // 只关一层：模态的 Esc 由各自组件负责，这里只在面板开着时响应
    ev.stopPropagation();
    close();
  }
}

watch(
  () => state.value.open,
  (open) => {
    if (open) void nextTick(() => searchEl.value?.focus());
  },
);

if (typeof document !== 'undefined') {
  document.addEventListener('pointerdown', onOutsidePointer, true);
  document.addEventListener('keydown', onDocumentKeydown, true);
}
onBeforeUnmount(() => {
  if (typeof document !== 'undefined') {
    document.removeEventListener('pointerdown', onOutsidePointer, true);
    document.removeEventListener('keydown', onDocumentKeydown, true);
  }
});
</script>

<template>
  <div ref="rootEl" class="city-picker" data-testid="city-picker">
    <button
      ref="triggerEl"
      type="button"
      class="input city-picker-trigger"
      data-testid="scrape-filter-city"
      :aria-label="label"
      aria-haspopup="listbox"
      :aria-expanded="state.open ? 'true' : 'false'"
      aria-controls="city-picker-panel"
      @click="toggle"
    >
      <span class="city-picker-value" data-testid="city-picker-value">{{ display }}</span>
      <span class="city-picker-caret" aria-hidden="true">▾</span>
    </button>

    <div
      v-if="state.open"
      id="city-picker-panel"
      class="city-picker-panel"
      data-testid="city-picker-panel"
    >
      <input
        ref="searchEl"
        class="input city-picker-search"
        data-testid="city-picker-search"
        type="text"
        role="combobox"
        aria-autocomplete="list"
        aria-controls="city-picker-results"
        :aria-expanded="mode === 'search' ? 'true' : 'false'"
        :value="state.query"
        placeholder="搜索城市 / 拼音 / 首字母"
        @input="onSearchInput"
        @keydown="onSearchKeydown"
      />

      <div v-if="mode === 'browse'" class="city-picker-cols">
        <ul
          class="city-picker-col city-picker-province"
          :class="{ 'is-focus': !cityColumnActive }"
          data-testid="city-picker-province"
          role="listbox"
          aria-label="省份"
          aria-controls="city-picker-city"
        >
          <li
            v-for="(p, i) in provinces"
            :key="p.code"
            class="city-picker-item"
            :class="{ 'is-active': i === state.provinceIndex }"
            role="option"
            :data-testid="`city-picker-province-${p.code}`"
            :aria-selected="i === state.provinceIndex ? 'true' : 'false'"
            tabindex="-1"
            @click="onProvinceClick(i)"
            @keydown="onProvinceKeydown($event, i)"
          >{{ p.name }}</li>
        </ul>
        <ul
          id="city-picker-city"
          class="city-picker-col city-picker-city"
          :class="{ 'is-focus': cityColumnActive }"
          data-testid="city-picker-city"
          role="listbox"
          :aria-label="activeProvince ? `${activeProvince.name}的城市` : '城市'"
        >
          <li
            v-for="(c, i) in cities"
            :key="c.code"
            class="city-picker-item"
            :class="{ 'is-active': i === state.cityIndex }"
            role="option"
            :data-testid="`city-picker-city-${c.code}`"
            :aria-selected="c.code === props.modelValue ? 'true' : 'false'"
            tabindex="-1"
            @click="onCityClick(c.code)"
            @keydown="onCityKeydown"
          >{{ c.name }}</li>
        </ul>
      </div>

      <ul
        v-else
        id="city-picker-results"
        class="city-picker-col city-picker-results"
        data-testid="city-picker-results"
        role="listbox"
        aria-label="搜索结果"
      >
          <li
            v-for="(hit, i) in results"
            :key="hit.code"
            class="city-picker-item"
            :class="{ 'is-active': i === state.resultIndex }"
            role="option"
            :data-testid="`city-picker-result-${hit.code}`"
            :aria-selected="i === state.resultIndex ? 'true' : 'false'"
            tabindex="-1"
            @click="onCityClick(hit.code)"
          >{{ hit.province }} · {{ hit.name }}</li>
        <li v-if="results.length === 0" class="city-picker-empty" data-testid="city-picker-empty">
          没有匹配的城市
        </li>
      </ul>
    </div>
  </div>
</template>
