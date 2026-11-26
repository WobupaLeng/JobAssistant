<script setup lang="ts">
/**
 * Boss 列表：收起态挂在右栏 `aside.chat-rail`，展开态挂在对话框左段
 * `.chat-detach-rail`，同一套渲染两处实例。testid 前缀不同（`chat-rail-*` /
 * `chat-detach-rail-*`），不会撞车。
 */
import { computed, ref } from 'vue';

import type { Friend } from '../../api/types';
import {
  CHAT_EMPTY_TITLE,
  CHAT_SEARCH_PLACEHOLDER,
  friendLine,
  friendMsgTone,
  friendName,
  unreadText,
  unreadTotal,
} from '../../lib_ui/chat-model';
import { isUnreadRow, type ReadMarks } from '../../lib_ui/chat-read';
import { needsSession } from '../../lib_ui/session';

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
    /** 「生成打招呼」进行中的 uid（按钮转「生成中…」）；其它行不受影响。 */
    greetingUid?: string;
    /** 可移除的行（手动打招呼新增的占位 HR）；判定在页面层，组件只渲染按钮。 */
    removableUids?: readonly string[];
    /**
     * 标题旁徽标的未读总数（数字挂在「Boss 列表」旁，与远端未读同步）。
     * 不传则退回组件内对 friends 的求和；页面传入时以页面口径为准。
     */
    unread?: number;
    /**
     * 会话已读水位（uid → 我读到的那条的时刻）。Boss 的 `unreadMsgCount` 实测恒为 0，
     * 未读由 `chat-read.ts` 派生：HR 最后发言且晚于水位即未读，打开会话后清除。
     */
    readMarks?: ReadMarks;
  }>(),
  { loading: false, error: '', greetingUid: '', removableUids: () => [], readMarks: () => ({}) },
);

const emit = defineEmits<{
  (e: 'pick', uid: string): void;
  (e: 'refresh'): void;
  (e: 'search', value: string): void;
  (e: 'retry'): void;
  /** 行内「打招呼」：先选中该行，再生成话术（由页面串起来）。 */
  (e: 'greet', uid: string): void;
  /** 行内「岗位详情」：页面按这一行的岗位 id 打开详情抽屉。 */
  (e: 'detail', uid: string): void;
  (e: 'remove', uid: string): void;
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
  // 外层滚动容器（SPEC §4.2 的 chat-rail-body / chat-detach-rail-body）：
  // 与 -body-slot 是同一节点上的两级语义，父级用无后缀名，内层挂载点用 -slot。
  body: `${props.prefix}-body`,
  list: `${props.prefix}-list`,
  empty: `${props.prefix}-empty`,
  emptyTitle: `${props.prefix}-empty-title`,
  emptyRefresh: `${props.prefix}-empty-refresh-btn`,
  error: `${props.prefix}-error`,
  retry: `${props.prefix}-retry-btn`,
  guide: `${props.prefix}-guide`,
  // 未读角标：收起态 chat-rail-unread-<uid>，展开态按冻结规格（SPEC §4.2）为 chat-detach-unread-<uid>。
  unread: (uid: string) => `${unreadPrefix(props.prefix)}-${uid}`,
}));

/* 空态标题走常量 CHAT_EMPTY_TITLE，带搜索词时才换成匹配失败口径。 */
const emptyText = computed(() =>
  props.search.trim() === '' ? CHAT_EMPTY_TITLE : '没有匹配的会话',
);

/**
 * 名字行后面的公司（原型口径 `李女士 · 星瀚科技`）。公司只是补充信息：缺失时整段不渲染，
 * 不留一个孤零零的分隔点；长度交给 `.friend-name` 的单行省略处理。
 */
function rowCompany(f: Friend): string {
  // 展示用原文，不走 `normCompany`（那份会把括号里的后缀剥掉，是匹配口径不是文案口径）。
  return String(f.company ?? '').trim();
}

/** 这一行算不算未读：Boss 报的条数优先，否则「HR 最后发言 + 我没读过」。 */
function rowUnread(f: Friend): boolean {
  return isUnreadRow(f, props.readMarks);
}

/** 标题旁的计数是**未读消息条数**，不是会话数。0 时不渲染。 */
const unreadSum = computed(() => unreadTotal(props.friends));

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
      <!-- 未读消息条数，不是会话数；一条未读都没有时不显示这个徽标。 -->
      <span v-if="unreadSum > 0" class="chip" :data-testid="ids.count">{{ unreadText(unreadSum) }}</span>
      <!-- 显式传入的未读数优先；组件内部不再自行统计，避免与页面口径出现两个数。 -->
      <span v-else-if="(props.unread ?? 0) > 0" class="chip" :data-testid="ids.count">{{ unreadText(props.unread ?? 0) }}</span>
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

    <!-- 说明条（如「已是最新消息」）独占一行：塞进上面的 44px 标题行会被标题与
         刷新按钮一起挤成两行，一句话断在中途。 -->
    <div v-if="$slots['rail-head']" class="rail-fresh-row">
      <slot name="rail-head" />
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

    <div class="rail-body-slot" :data-testid="ids.bodySlot" :data-rail-body="ids.body">
      <!-- 外层滚动容器同时挂 SPEC §4.2 的精确锚点（chat-rail-body / chat-detach-rail-body）：
           该锚点在 <template> 上无法逐次解析 v-if 分支，挂一个零尺寸的 sr-only 标记最省事，
           且不给 E2E 增加任何可见节点。 -->
      <span class="sr-only" :data-testid="ids.body" aria-hidden="true" />
      <!-- 左段（对话框里的 Boss 列表）额外暴露一个 chat-detach-rail-slot 锚点，
           与右栏的 chat-rail-body-slot 语义相同，名字按 SPEC §4.2 的 detach 口径。 -->
      <span
        v-if="props.prefix === 'chat-detach-rail'"
        class="sr-only"
        data-testid="chat-detach-rail-slot"
      >Boss 列表挂载点</span>
      <div
        v-if="props.error !== '' && needsSession(props.error) && props.friends.length === 0"
        class="rail-state"
        :data-testid="ids.guide"
        role="status"
      >
        <span class="state-glyph" aria-hidden="true">●</span>
        <span class="state-title">浏览器未连接</span>
        <span class="state-sub">请先登录后重试</span>
        <span class="state-actions">
          <button class="btn btn-sm" type="button" :data-testid="ids.retry" @click="emit('retry')">
            重试
          </button>
        </span>
      </div>

      <div
        v-else-if="props.error !== '' && props.friends.length === 0"
        class="rail-error"
        :data-testid="ids.error"
        role="alert"
      >
        <span class="rail-error-head">读取消息失败：<code class="rail-err-msg">{{ props.error }}</code></span>
        <span class="rail-guide" :data-testid="ids.guide">确认左上角已登录 Boss 直聘</span>
        <button class="btn btn-sm" type="button" :data-testid="ids.retry" @click="emit('retry')">
          重试
        </button>
      </div>

      <div v-else-if="props.loading && props.friends.length === 0" class="rail-state">
        <span class="skel-line" />
        <span class="skel-line" />
        <span class="skel-line" />
      </div>

      <div v-else-if="props.friends.length === 0" class="rail-state" :data-testid="ids.empty">
        <span class="state-glyph" aria-hidden="true"><svg width="20" height="20" viewBox="0 0 16 16" fill="none"><path d="M2.6 4.2h10.8v6.4H7.4L4.2 13v-2.4H2.6z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/></svg></span>
        <span class="state-title" :data-testid="ids.emptyTitle">{{ emptyText }}</span>
        <span class="state-sub">点「刷新」或在 Boss 直聘回一条消息</span>
        <span class="state-actions">
          <button class="btn btn-sm" type="button" :data-testid="ids.emptyRefresh" @click="emit('refresh')">
            刷新
          </button>
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
          class="friend hl-surface"
          :class="{ 'is-active': f.uid === props.activeUid }"
          :aria-selected="f.uid === props.activeUid ? 'true' : 'false'"
          :data-testid="`friend-${f.uid}`"
          :title="friendName(f)"
          @click="emit('pick', f.uid)"
        >
          <!-- 行内操作列：「对谁打招呼」由被点的那一行决定，不靠选中态隐式决定。
               @click.stop 防止同时触发整行的 pick。 -->
          <span class="avatar" aria-hidden="true">{{ friendName(f).slice(0, 1) }}</span>
          <span class="friend-main">
            <span class="friend-name" :data-testid="`friend-name-${f.uid}`">{{ friendName(f) }}<span
              v-if="rowCompany(f) !== ''"
              class="friend-company"
              :data-testid="`friend-company-${f.uid}`"
            > · {{ rowCompany(f) }}</span></span>
            <!-- 副行按「未读 / 已读 / 已送达」上色。对方发的最后一条只标未读，已读/送达说的是我发的。 -->
            <span
              class="friend-msg"
              :class="`is-${friendMsgTone(f, rowUnread(f))}`"
              :data-testid="`friend-msg-${f.uid}`"
            >
              <span
                v-if="friendMsgTone(f, rowUnread(f)) === 'unread'"
                class="friend-msg-tag"
                :data-testid="`friend-msg-tag-${f.uid}`"
              >[未读]</span>
              <span
                v-else-if="friendMsgTone(f, rowUnread(f)) === 'read'"
                class="friend-msg-tag"
                :data-testid="`friend-msg-tag-${f.uid}`"
              >[已读]</span>
              <span
                v-else-if="friendMsgTone(f, rowUnread(f)) === 'sent'"
                class="friend-msg-tag"
                :data-testid="`friend-msg-tag-${f.uid}`"
              >[送达]</span>
              {{ friendLine(f) }}
            </span>
          </span>
          <span class="friend-side">
            <span class="friend-time" :data-testid="`friend-time-${f.uid}`">{{ f.last_time || '—' }}</span>
            <span
              v-if="rowUnread(f)"
              class="unread"
              :class="{ 'is-dot': (f.unread ?? 0) <= 0 }"
              :data-testid="ids.unread(f.uid)"
              :title="(f.unread ?? 0) > 0 ? `${f.unread} 条未读` : 'HR 有新回复'"
            >{{ (f.unread ?? 0) > 0 ? unreadText(f.unread) : '' }}</span>
          </span>
          <span class="friend-acts">
            <!-- 操作列竖排（打招呼在上、岗位详情在下），两枚压到 15px 高，合计约等于头像高度，
                 行高不变；文字标签在悬停 / 键盘聚焦时浮在按钮左侧。 -->
            <span
              role="button"
              tabindex="0"
              class="friend-act"
              :data-testid="`friend-greet-${f.uid}`"
              :aria-label="props.greetingUid === f.uid ? '生成中…' : '打招呼'"
              :aria-disabled="props.greetingUid === f.uid ? 'true' : 'false'"
              @click.stop="emit('greet', f.uid)"
              @keydown.enter.stop.prevent="emit('greet', f.uid)"
              @keydown.space.stop.prevent="emit('greet', f.uid)"
            >
              <svg class="friend-act-svg" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M2.6 3.4h10.8v7H7.4L4.2 13.4v-3H2.6z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/></svg>
              <span class="friend-act-label" aria-hidden="true">{{ props.greetingUid === f.uid ? '生成中…' : '打招呼' }}</span>
            </span>
            <span
              role="button"
              tabindex="0"
              class="friend-act friend-act-detail"
              :data-testid="`friend-detail-${f.uid}`"
              aria-label="岗位详情"
              @click.stop="emit('detail', f.uid)"
              @keydown.enter.stop.prevent="emit('detail', f.uid)"
              @keydown.space.stop.prevent="emit('detail', f.uid)"
            >
              <svg class="friend-act-svg" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M3.6 2.6h5.6l3.2 3.2v7.6H3.6z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M5.8 7.8h4.4M5.8 10.1h4.4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>
              <span class="friend-act-label" aria-hidden="true">岗位详情</span>
            </span>
            <span
              v-if="props.removableUids.includes(f.uid)"
              role="button"
              tabindex="0"
              class="friend-act friend-act-remove"
              :data-testid="`friend-remove-${f.uid}`"
              aria-label="移除"
              @click.stop="emit('remove', f.uid)"
              @keydown.enter.stop.prevent="emit('remove', f.uid)"
              @keydown.space.stop.prevent="emit('remove', f.uid)"
            >
              <svg class="friend-act-svg" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
              <span class="friend-act-label" aria-hidden="true">移除</span>
            </span>
          </span>
        </button>
      </div>
    </div>
  </div>
</template>
