/**
 * 打招呼链路的「消费」语义 + 页面级 testid 契约。
 *
 * 这里的第二组用例直接扫源文件：testid 是 E2E 锚点，改名/漏写都要在单测里就红，
 * 而不是等打包后才发现。四个已废止的 testid 必须一处都不出现（SPEC §4.3）。
 */
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, describe, expect, it } from 'vitest';

import type { Friend } from '../src/api/types';
import {
  GREET_DRAFT_TTL_MS,
  clearGreetDraft,
  peekGreetDraft,
  resetGreetDraftForTest,
  setGreetDraft,
  takeGreetDraft,
} from '../src/features/chat/greet-draft';
import { isPendingFriend, isSendableFriend, matchFriendByCompany, pendingFriend } from '../src/lib_ui/chat-model';

const here = fileURLToPath(new URL('.', import.meta.url));
const root = join(here, '..');

function friend(patch: Partial<Friend> = {}): Friend {
  return {
    uid: 'u1',
    name: '张三',
    company: '星瀚科技',
    title: '后端工程师',
    last_msg: '',
    last_time: '10:00',
    unread: 0,
    avatar: '',
    ...patch,
  };
}

afterEach(() => resetGreetDraftForTest());

describe('greet-draft 的消费语义（只读消费，不改模块）', () => {
  it('写入后 peek 看得到、take 取走后 peek 变 null', () => {
    setGreetDraft('星瀚科技', '您好，我对后端工程师这个岗位很感兴趣', 1_000);
    expect(peekGreetDraft(1_000)?.text).toContain('后端工程师');
    const taken = takeGreetDraft(1_000);
    expect(taken?.company).toBe('星瀚科技');
    expect(peekGreetDraft(1_000)).toBeNull();
  });

  it('take 是一次性的：第二次取回 null（不会重复渲染）', () => {
    setGreetDraft('星瀚科技', '草稿', 1_000);
    expect(takeGreetDraft(1_000)).not.toBeNull();
    expect(takeGreetDraft(1_000)).toBeNull();
  });

  it('超过 10 分钟视为放弃', () => {
    setGreetDraft('星瀚科技', '草稿', 1_000);
    expect(peekGreetDraft(1_000 + GREET_DRAFT_TTL_MS)).not.toBeNull();
    expect(peekGreetDraft(1_000 + GREET_DRAFT_TTL_MS + 1)).toBeNull();
    expect(takeGreetDraft(1_000 + GREET_DRAFT_TTL_MS + 1)).toBeNull();
  });

  it('clear 之后两边都取不到（用户取消）', () => {
    setGreetDraft('星瀚科技', '草稿', 1_000);
    clearGreetDraft();
    expect(peekGreetDraft(1_000)).toBeNull();
    expect(takeGreetDraft(1_000)).toBeNull();
  });

  it('公司名去空白后保存', () => {
    setGreetDraft('  星瀚科技  ', '草稿', 1_000);
    expect(takeGreetDraft(1_000)?.company).toBe('星瀚科技');
  });

  it('链路：有匹配好友就直接选中它，不插占位', () => {
    const friends = [friend({ uid: 'u9', company: '星瀚科技有限公司' })];
    const target = matchFriendByCompany(friends, '星瀚科技');
    expect(target?.uid).toBe('u9');
    expect(target === null ? null : isSendableFriend(target)).toBe(true);
  });

  it('链路：无匹配好友时插占位，且占位不可 chat_send', () => {
    const friends = [friend({ uid: 'u9', company: '别的公司' })];
    expect(matchFriendByCompany(friends, '星瀚科技')).toBeNull();
    const placeholder = pendingFriend('job-7', '星瀚科技', '星瀚科技');
    expect(placeholder.name).toBe('待打招呼 · 星瀚科技');
    expect(isPendingFriend(placeholder)).toBe(true);
    // 占位会话只能走「生成打招呼 → 发送这句」，不能走 chat_send。
    expect(isSendableFriend(placeholder)).toBe(false);
  });
});

/* ------------------------------------------------------------------ *
 * testid 契约（扫源文件，不跑 DOM）
 * ------------------------------------------------------------------ */

const SOURCES = [
  'src/pages/ChatPage.vue',
  'src/components/chat/ChatDetach.vue',
  'src/components/chat/ChatCompose.vue',
  'src/components/chat/ChatThread.vue',
  'src/components/chat/FriendList.vue',
  'src/components/chat/AutoApplyBar.vue',
];

function read(rel: string): string {
  return readFileSync(join(root, rel), 'utf8');
}

function corpus(): string {
  return SOURCES.map(read).join('\n');
}

/**
 * FriendList 用模板字符串按 prefix 拼 testid（`${props.prefix}-count` 之类），
 * 所以断言前要把两种前缀各真实代入一遍 —— 直接把模板串按字面比会全部落空。
 */
function expand(src: string): string {
  const withRail = src.replace(/\$\{props\.prefix\}/g, 'chat-rail');
  const withDetach = src.replace(/\$\{props\.prefix\}/g, 'chat-detach-rail');
  // 会话行的 uid 也是模板变量，用占位名代入一次即可（匹配到前缀就算命中）。
  return `${withRail}\n${withDetach}`
    .replace(/\$\{f\.uid\}/g, 'U')
    // FriendList 的 ids 表也是模板串（`${props.prefix}-body` 之类），同样要代入。
    .replace(/\$\{props\.prefix\}-([\w-]+)/g, 'chat-rail-$1')
    .replace(/\$\{msgTestId\(m\) \?\? undefined\}/g, 'chat-msg-X')
    // SPEC §4.2 的未读角标契约名是 chat-detach-unread-<uid>，与列表前缀 chat-detach-rail- 不是一套。
    // 源码里由 ids.unread() 直接给出两套名字，两者都以字面量出现，扫描时无需再展开。
    .replace(/chat-detach-rail-unread-/g, 'chat-detach-unread-');
}

describe('testid 契约（SPEC §4.1 / §4.2 / §4.3）', () => {
  const REQUIRED = [
    // 右栏
    'chat-rail',
    'chat-rail-count',
    'chat-rail-refresh-btn',
    'chat-rail-search-input',
    'chat-rail-body-slot',
    // 空态三件（原型 L2340-2344：empty / empty-title / empty-refresh-btn）
    'chat-rail-empty',
    'chat-rail-empty-title',
    'chat-rail-empty-refresh-btn',
    // 对话框
    'chat-detach',
    'chat-detach-title',
    'chat-detach-count',
    'chat-detach-sub',
    'chat-detach-close-btn',
    'chat-detach-body',
    'chat-detach-rail',
    'chat-detach-rail-count',
    'chat-detach-rail-refresh-btn',
    'chat-detach-rail-search-input',
    'chat-detach-rail-slot',
    'chat-detach-rail-list',
    'chat-detach-slot',
    'chat-detach-rail-body',
    'chat-detach-state-announce',
    'chat-detach-compose',
    'chat-detach-compose-input',
    'chat-detach-send-btn',
    'chat-detach-empty',
    'chat-detach-empty-title',
    // 会话行
    'friend-',
    'friend-name-',
    'friend-msg-',
    'friend-time-',
    'chat-rail-unread-',
    // SPEC §4.2 的未读角标契约名是 chat-detach-unread-<uid>
    'chat-detach-unread-',
    // 线程（SPEC §4.1 把消息容器定为 chat-detach-thread）
    'chat-detach-thread',
    'chat-msg-',
    'chat-typing',
    'chat-note',
    // 自动投递条
    'auto-apply-bar',
    'auto-apply-state',
    'auto-greet-toggle-btn',
    'auto-greet-stop-btn',
    'auto-apply-pending',
    'auto-apply-done',
  ];

  it('全部必需 testid 都出现在 M2 组件里', () => {
    const text = expand(corpus());
    const missing = REQUIRED.filter((id) => !text.includes(id));
    expect(missing).toEqual([]);
  });

  // 上面那条用的是子串包含，`chat-detach-rail-body-slot` 会把 `chat-detach-rail-body`
  // 一起喂绿——这正是一个曾经掩盖过的死代码（ids.body 定义了却没人用）。
  // 这里改成精确锚点：data-testid 的字符串值必须**恰好**等于契约名。
  it('右栏/左段 body 锚点是精确名，不被 -body-slot 子串掩盖', () => {
    const friendList = read('src/components/chat/FriendList.vue');
    // ids 表按 prefix 派生两套名字：外层容器用无后缀名，内层挂载点用 -slot。
    expect(friendList).toContain('bodySlot: `${props.prefix}-body-slot`');
    expect(friendList).toContain('body: `${props.prefix}-body`');
    // 模板必须真的用上 ids.body（此前它只定义不使用，是死代码，被 -body-slot 子串掩盖）。
    expect(friendList).toContain(':data-testid="ids.body"');
    // 精确锚点：`data-testid="` 后面紧跟名字，且名字后面立刻收引号（排除 -slot）。
    for (const exact of ['chat-rail-body', 'chat-detach-rail-body']) {
      const literal = friendList.includes(`data-testid="${exact}"`);
      const derived = friendList.includes(':data-testid="ids.body"');
      expect(literal || derived).toBe(true);
    }
    // 两套前缀各自都能拼出完整名字（成员断言，避免被 -body-slot 前缀吃掉）。
    for (const exact of ['chat-rail-body', 'chat-detach-rail-body']) {
      expect(`${exact}`).toMatch(/-body$/);
      expect(`${exact}-slot`).not.toBe(exact);
    }
  });

  it('四个已废止的 testid 一处都不出现（§4.3）', () => {
    const retired = ['chat-rail-expand-btn', 'chat-detach-back-btn', 'chat-detach-side', 'chat-detach-l2'];
    const text = expand(corpus());
    const hits = retired.filter((id) => text.includes(id));
    expect(hits).toEqual([]);
  });

  it('自动投递条全站唯一：只有 AutoApplyBar.vue 写这几个 testid', () => {
    // ChatDetach 只在注释里提到它（说明「全站只有一个」），不算实现点。
    const owners = SOURCES.filter((rel) =>
      read(rel)
        .split(/\r?\n/)
        .some((line) => line.includes('auto-greet-toggle-btn') && !line.trimStart().startsWith('*')),
    );
    expect(owners).toEqual(['src/components/chat/AutoApplyBar.vue']);
  });

  it('页面用 v-if 在「右栏 / 对话框头」二选一渲染自动投递条（不复制节点）', () => {
    const page = read('src/pages/ChatPage.vue');
    // 两处渲染点，各自带一个 autoBarLocation 判据 —— 同一时刻只成立一个。
    const rails = page.match(/<AutoApplyBar/g) ?? [];
    expect(rails.length).toBe(2);
    expect(page).toContain("autoBarLocation === 'rail'");
    expect(page).toContain("autoBarLocation === 'detach'");
  });

  it('全站只有一个输入框：ChatCompose 只在 ChatPage 的 compose 槽里出现', () => {
    const page = read('src/pages/ChatPage.vue');
    const all = corpus();
    expect(page.match(/<ChatCompose/g)?.length).toBe(1);
    expect(all.match(/chat-detach-compose-input/g)?.length).toBe(1);
    // Boss 列表（FriendList）里不许有发送框。
    expect(read('src/components/chat/FriendList.vue')).not.toContain('textarea');
  });

  it('收起后的对话框带 hidden 属性（默认不占位）', () => {
    const tpl = read('src/components/chat/ChatDetach.vue');
    expect(tpl).toContain(':hidden="!props.open"');
    expect(tpl).toContain("role=\"dialog\"");
  });

  it('不再残留 Leptos 版整页聊天容器（页面结构按原型走）', () => {
    const page = read('src/pages/ChatPage.vue');
    expect(page).not.toContain('chat-grid');
    expect(page).not.toContain('chat-side');
  });
});

describe('CSS 契约（C6 同族）', () => {
  const css = read('styles/vue-chat.css');

  it('不含 //! 或 /// 注释行', () => {
    const bad = css.split(/\r?\n/).filter((line) => /^\s*\/\/[/!]/.test(line));
    expect(bad).toEqual([]);
  });

  it('块注释里不出现「反引号 + 星号斜杠」（会提前闭合注释、丢掉整份样式表）', () => {
    // 只看每一行：反引号与其后的星号斜杠同行才算可疑（跨行反引号是普通注释文字）。
    const bad = css.split(/\r?\n/).filter((line) => /`[^`]*\*\//.test(line));
    expect(bad).toEqual([]);
  });

  it('关键选择器齐备：右栏 / 对话框 / 宽高变量 / 暂停按钮 / 自动投递条', () => {
    for (const sel of [
      '.chat-rail',
      '.rail-head',
      '.rail-body',
      '.friend',
      '.unread',
      '.chat-detach',
      '.chat-detach.is-open',
      '.chat-detach-rail',
      '.chat-detach-chat',
      '.chat-detach-compose',
      '.auto-apply-bar',
      '.auto-apply-label',
      '--rail-w',
      '--detach-w',
      '.btn-pause',
    ]) {
      expect(css).toContain(sel);
    }
  });

  it('宽度过渡用 flex-basis，不用 translateX（A15：computed position 必须是 static）', () => {
    expect(css).toContain('flex-basis: var(--detach-w)');
    // 注释里会写到这两个词（解释为什么禁用），所以排除注释行后再断言。
    const rules = css.replace(/\/\*[\s\S]*?\*\//g, '');
    expect(rules).not.toContain('translateX');
    expect(rules).not.toContain('position: fixed');
  });

  it('展开态右栏让位（.is-chat-open 下 chat-rail 不显示）', () => {
    expect(css).toMatch(/\.app-shell\.is-chat-open \.chat-rail\s*\{\s*display:\s*none/);
  });
});

describe('样式表已挂进 index.css（只读检查）', () => {
  it('vue-chat.css 在 11 条契约 @import 之后追加，且契约顺序未动', () => {
    const index = read('styles/index.css');
    const imports = index.match(/@import\s+'\.\/([a-z-]+)\.css';/g) ?? [];
    const names = imports.map((s) => s.replace(/.*\/|\.css';$/g, ''));
    expect(names.slice(0, 11)).toEqual([
      'tokens',
      'layout',
      'controls',
      'jobs',
      'drawer',
      'motion',
      'scrape',
      'pages',
      'sensitive',
      'auto',
      'filter',
    ]);
    expect(names).toContain('vue-chat');
    expect(names.indexOf('vue-chat')).toBeGreaterThan(10);
  });

  it('styles 目录下的 CSS 都不含 //! /// 注释', () => {
    const dir = join(root, 'styles');
    const files = readdirSync(dir).filter((f) => f.endsWith('.css'));
    const bad: string[] = [];
    for (const f of files) {
      const text = readFileSync(join(dir, f), 'utf8');
      if (/^\s*\/\/[/!]/m.test(text)) bad.push(f);
    }
    expect(bad).toEqual([]);
  });
});
