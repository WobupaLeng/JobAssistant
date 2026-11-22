<script setup lang="ts">
/**
 * Boss 列表（`rail`）—— 收起态挂在右栏 `aside.chat-rail`，
 * 展开态挂在对话框左段 `.chat-detach-rail`。**同一套渲染，两处实例**。
 *
 * 两处的 `data-testid` 前缀不同（`chat-rail-*` / `chat-detach-rail-*`），
 * 所以不会撞车；SPEC §4.4 要求「全站唯一」的只有自动投递条，不在本组件里。
 */
import { computed, ref } from 'vue';

import type { Friend } from '../../api/types';
import {
  CHAT_SEARCH_PLACEHOLDER,
  friendLine,
  friendName,
  unreadText,
} from '../../lib_ui/chat-model';

const props = withDefaults(
  defineProps<{
    /** 面板前缀：`chat-rail`（右栏）或 `chat-detach-rail`（对话框左段）。 */
    prefix: 'chat-rail' | 'chat-detach-rail';
    title: string;
    friends: readonly Friend[];
    activeUid: string | null;
    loading?: boolean;
    error?: string;
    /** 搜索词由页面持有：两段列表共享同一个查询（用户看到的是一份会话）。 */
    search: string;
  }>(),
  { loading: false, error: '' },
);

const emit = defineEmits<{
  (e: 'pick', uid: string): void;
  (e: 'refresh'): void;
  (e: 'search', value: string): void;
  (e: 'retry'): void;
}>();

/**
 * 未读角标前缀：展开态按冻结规格（SPEC §4.2）叫 `chat-detach-unread-<uid>`，
 * 与列表前缀 `chat-detach-rail-` 不是一套，所以两种前缀各自映射。
 */
function unreadPrefix(prefix: 'chat-rail' | 'chat-detach-rail'): string {
  return prefix === 'chat-rail' ? 'chat-rail-unread' : 'chat-detach-unread';
}

const listEl = ref<HTMLElement | null>(null);

/**
 * 根节点的 `chat-rail` 由页面里的 `aside.chat-rail` 承担（全站恰好一处），
 * 本组件不再重复挂同一个 testid——重复会让 `[data-testid="chat-rail"]`
 * 一次匹配到两个元素，选择器类断言（E2E / QA）随之失效。
 */
const ids = computed(() => ({
  count: `${props.prefix}-count`,
  refreshBtn: `${props.prefix}-refresh-btn`,
  searchInput: `${props.prefix}-search-input`,
  bodySlot: `${props.prefix}-body-slot`,
  list: `${props.prefix}-list`,
  empty: `${props.prefix}-empty`,
  error: `${props.prefix}-error`,
  retry: `${props.prefix}-retry-btn`,
  guide: `${props.prefix}-guide`,
  // 未读角标：收起态 chat-rail-unread-<uid>，展开态按冻结规格（SPEC §4.2）为 chat-detach-unread-<uid>。
  unread: (uid: string) => `${unreadPrefix(props.prefix)}-${uid}`,
}));

const emptyText = computed(() =>
  props.search.trim() === '' ? '还没有会话' : '没有匹配的会话',
);

function onSearch(ev: Event): void {
  const el = ev.target as HTMLInputElement;
  emit('search', el.value);
}

defineExpose({ listEl });
</script>

<template>
  <div class="rail-panel" :data-prefix="props.prefix">
    <div class="rail-head">
      <span class="rail-title">{{ props.title }}</span>
      <span class="chip" :data-testid="ids.count">{{ props.friends.length }}</span>
      <span class="spacer" />
      <span class="rail-head-actions">
        <button
          class="btn btn-sm btn-ghost"
          type="button"
          :data-testid="ids.refreshBtn"
          @click="emit('refresh')"
        >
          刷新
        </button>
      </span>
    </div>

    <div class="rail-search">
      <input
        class="input"
        type="search"
        :placeholder="CHAT_SEARCH_PLACEHOLDER"
        :aria-label="CHAT_SEARCH_PLACEHOLDER"
        :data-testid="ids.searchInput"
        :value="props.search"
        @input="onSearch"
      />
    </div>

    <div class="rail-body-slot" :data-testid="ids.bodySlot">
      <!-- 左段（对话框里的 Boss 列表）额外暴露一个 chat-detach-rail-slot 锚点，
           与右栏的 chat-rail-body-slot 语义相同，名字按 SPEC §4.2 的 detach 口径。 -->
      <span
        v-if="props.prefix === 'chat-detach-rail'"
        class="sr-only"
        data-testid="chat-detach-rail-slot"
      >Boss 列表挂载点</span>
      <div
        v-if="props.error !== ''"
        class="rail-error"
        :data-testid="ids.error"
        role="alert"
      >
        <span class="rail-error-head">读取会话失败</span>
        <span class="rail-err-msg">{{ props.error }}</span>
        <button class="btn btn-sm btn-ghost" type="button" :data-testid="ids.retry" @click="emit('retry')">
          重试
        </button>
      </div>

      <div v-else-if="props.loading && props.friends.length === 0" class="rail-state">
        <span class="skel-line" />
        <span class="skel-line" />
        <span class="skel-line" />
      </div>

      <div v-else-if="props.friends.length === 0" class="rail-state" :data-testid="ids.empty">
        <span class="state-glyph">💬</span>
        <span class="state-title">{{ emptyText }}</span>
        <span v-if="props.search.trim() === ''" class="rail-guide" :data-testid="ids.guide">
          确认已登录 Boss；未登录可到「概览」页点「扫码登录」。
        </span>
      </div>

      <div
        v-else
        ref="listEl"
        class="chat-rail-list"
        role="listbox"
        aria-label="会话列表"
        :data-testid="ids.list"
      >
        <button
          v-for="f in props.friends"
          :key="f.uid"
          type="button"
          role="option"
          class="friend"
          :class="{ 'is-active': f.uid === props.activeUid }"
          :aria-selected="f.uid === props.activeUid ? 'true' : 'false'"
          :data-testid="`friend-${f.uid}`"
          :title="friendName(f)"
          @click="emit('pick', f.uid)"
        >
          <span class="avatar" aria-hidden="true">{{ friendName(f).slice(0, 1) }}</span>
          <span class="friend-main">
            <span class="friend-name" :data-testid="`friend-name-${f.uid}`">{{ friendName(f) }}</span>
            <span class="friend-msg" :data-testid="`friend-msg-${f.uid}`">{{ friendLine(f) }}</span>
          </span>
          <span class="friend-side">
            <span class="friend-time" :data-testid="`friend-time-${f.uid}`">{{ f.last_time || '—' }}</span>
            <span
              v-if="f.unread > 0"
              class="unread"
              :data-testid="ids.unread(f.uid)"
              :title="`${f.unread} 条未读`"
            >{{ unreadText(f.unread) }}</span>
          </span>
        </button>
      </div>
    </div>
  </div>
</template>
