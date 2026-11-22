<script setup lang="ts">
/**
 * 配置弹窗里的可折叠卡（对照 Leptos features/config/form.rs 的 ConfigSection）。
 * 纯展示组件：只收 props / 发 update:open，不 import features（AGENTS §2.5）。
 * 折叠标题按钮带 {section}-toggle 派生 testid（补上 Leptos C8 的唯一缺口）。
 */
withDefaults(
  defineProps<{
    title: string;
    subtitle?: string;
    open?: boolean;
    sectionTestId: string;
    toggleTestId: string;
  }>(),
  { subtitle: '', open: true },
);

const emit = defineEmits<{ (e: 'update:open', value: boolean): void }>();
</script>

<template>
  <div class="config-card" :data-testid="sectionTestId">
    <button
      type="button"
      class="config-head"
      :aria-expanded="open"
      :data-testid="toggleTestId"
      @click="emit('update:open', !open)"
    >
      <span class="chevron" :class="{ 'is-open': open }"></span>
      <span class="config-title">{{ title }}</span>
      <span v-if="subtitle" class="card-sub">{{ subtitle }}</span>
    </button>
    <div v-show="open" class="config-body">
      <slot />
    </div>
  </div>
</template>
