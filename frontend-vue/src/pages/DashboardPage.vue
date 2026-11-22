<script setup lang="ts">
/**
 * 概览页最小壳：一条抓取筛选栏，城市槽位由 `CityPicker` 承担。
 *
 * 本轮的边界（用户裁决）：**只接城市**。薪资 / 经验 / 学历 / 规模 / 阶段 /
 * 职位类型 / 行业 / 关键词与 `scrape_start` 都留给后续阶段，所以这里没有按钮、
 * 没有请求、没有 mock 数据——只有一个持有筛选状态的对象。
 *
 * 分层：本页属 `pages` 层，可以消费 `features`（`createFilter`）与 `components`
 * （`CityPicker`）；反过来不许。城市选择器的状态机住在 `lib_ui/city-picker.ts`，
 * 所以组件本身不构成 `components → features` 的向上引用。
 *
 * 容器 testid 由 `pageTestId('dashboard')` 派生（= `page-dashboard`），不手写字符串：
 * E2E 依赖 `page-<slug>` 这个规则，手写一份就多一处会失联的锚点。
 */
import { ref } from 'vue';

import CityPicker from '../components/CityPicker.vue';
import { createFilter } from '../features/scrape/filter';
import { pageTestId } from '../lib_ui/nav';

/** 筛选状态：本轮唯一的字段是城市编码。 */
const filter = ref(createFilter());

/** 页面容器 testid，规则见 `lib_ui/nav`。 */
const rootTestId = pageTestId('dashboard');
</script>

<template>
  <section class="page page-dashboard" :data-testid="rootTestId">
    <div class="card scrape-filter-bar" data-testid="scrape-filter-bar">
      <div class="scrape-filter-field">
        <CityPicker v-model="filter.city" />
      </div>
    </div>
  </section>
</template>
