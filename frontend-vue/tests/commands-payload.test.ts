/**
 * 载荷形状契约（C2）：48 条命令「包 args / 平铺 / 无参」逐条钉死。
 *
 * 用一个只记录调用的假 `window.__TAURI_INTERNALS__`（**只在测试里存在**，
 * `src/` 里没有任何假桩），断言真正发出去的 `(cmd, payload)`。
 * 这些错形状在类型检查里全绿、只在真 WebView 里报缺 key 或静默丢参数，
 * 所以必须靠这张表守住。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import * as auto from '../src/api/commands/auto';
import * as chat from '../src/api/commands/chat';
import * as config from '../src/api/commands/config';
import * as db from '../src/api/commands/db';
import * as greet from '../src/api/commands/greet';
import * as logs from '../src/api/commands/logs';
import * as scrape from '../src/api/commands/scrape';
import * as sensitive from '../src/api/commands/sensitive';
import * as shell from '../src/api/commands/shell';
import { emptyJob } from '../src/lib_ui/job';

type Call = { cmd: string; payload: unknown };

/** `backend-tauri/src/lib.rs:522-571` 的 `generate_handler!` 逐条抄写（唯一真源）。 */
const EXPECTED_COMMANDS = [
  'get_scoring_spec',
  'app_info',
  'window_minimize',
  'window_toggle_maximize',
  'window_close',
  'list_jobs',
  'get_job',
  'db_overview',
  'migrate_scores',
  'score_job',
  'passes_threshold',
  'update_score',
  'rescore_jobs',
  'llm_status',
  'job_tab_counts',
  'reject_jobs',
  'allow_jobs',
'mark_delivered',
    'unmark_delivered',
  'list_decisions',
  'count_decisions',
  'decision_counts',
  'sensitive_list',
  'sensitive_remove',
  'get_app_config',
  'set_app_config',
  'set_limit',
  'set_scrape_defaults',
  'auto_greet_next',
  'auto_greet_pause',
  'auto_greet_resume',
  'auto_greet_status',
  'auto_greet_reset',
  'scrape_start',
  'scrape_stop',
  'scrape_progress',
  'scrape_drain',
  'login_start',
  'login_status',
  'login_save',
  'logout',
  'login_ack',
  'scrape_close',
  'session_open',
  'list_logs',
  'clear_logs',
'chat_friends',
    'chat_friends_local',
    'chat_messages_local',
  'chat_open',
  'chat_messages',
  'chat_send',
  'chat_resumes',
  'chat_send_resume',
  'chat_recall',
  'greet_draft',
  'greet_one',
  'list_expectations',
] as const;

let calls: Call[] = [];

function fakeInvoke(cmd: string, payload?: unknown): Promise<unknown> {
  calls.push({ cmd, payload });
  switch (cmd) {
    case 'sensitive_remove':
      return Promise.resolve({ ok: true, deleted: Number((payload as { args: { ids: number[] } }).args.ids.length) });
    case 'get_job':
      return Promise.resolve(emptyJob());
    case 'list_jobs':
      // 这条命令的返回值会过一层 `normalizeJob`（后端 `Job` 字段是 `Option<T>`，
      // 序列化后可能为 null），所以桩必须回真数组，不能落到默认的 `{ ok: true }`。
      return Promise.resolve([]);
    case 'auto_greet_resume':
      return Promise.resolve(1_700_000_000_000);
    case 'greet_one':
      return Promise.resolve('您好');
    default:
      return Promise.resolve({ ok: true });
  }
}

beforeEach(() => {
  calls = [];
  vi.stubGlobal('window', { __TAURI_INTERNALS__: { invoke: fakeInvoke } });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

/**
 * 反查：`src/api/commands/*.ts` 里出现的命令名必须与 `generate_handler!` 逐条对齐。
 *
 * 这是 C1 的静态守卫。`invokeArgs('...')` 的第一个实参如果拼错，类型检查、
 * 单测、构建**全绿**，只有真机点下去才报 `command not found`。
 * 直接读源文件做文本扫描，不做 AST：48 条命令名都在同一行。
 */
describe('命令名覆盖：与 generate_handler! 逐条对齐（C1）', () => {
  it('58 条命令恰好覆盖一次，不多不少', () => {
    const dir = fileURLToPath(new URL('../src/api/commands/', import.meta.url));
    const used = new Set<string>();
    for (const name of readdirSync(dir).filter((f: string) => f.endsWith('.ts'))) {
      const src = readFileSync(join(dir, name), 'utf8');
      // 直接调用：invokeArgs<T>('cmd', ...) / invokeFlat<T>('cmd', ...) / invokeNone<T>('cmd')
      for (const m of src.matchAll(/invoke(?:Args|Flat|None)(?:<[^>]*>)?\(\s*'([a-z_]+)'/g)) {
        used.add(m[1]!);
      }
      // 动态命令名：setJobFlag('reject_jobs', jobIds) —— 三条 job-flag 命令共用一条路径
      for (const m of src.matchAll(/setJobFlag\(\s*'([a-z_]+)'/g)) {
        used.add(m[1]!);
      }
    }
    expect(used.size).toBe(58);
    expect(EXPECTED_COMMANDS.filter((c) => !used.has(c))).toEqual([]);
    const known: readonly string[] = EXPECTED_COMMANDS;
    expect([...used].filter((c) => !known.includes(c))).toEqual([]);
  });
});

/** 只取最后一次调用（多数用例只发一次）。 */
function last(): Call {
  expect(calls).toHaveLength(1);
  return calls[calls.length - 1]!;
}

describe('commands_db · 包 args 的一类', () => {
  it('list_jobs：args 层 + 三个布尔恒发 + 可选键被丢掉', async () => {
    await db.listJobs({ tab: 'pending', limit: 20 });
    const { cmd, payload } = last();
    expect(cmd).toBe('list_jobs');
    expect(payload).toEqual({
      args: {
        tab: 'pending',
        limit: 20,
        include_rejected: false,
        rejected_only: false,
        ascending: false,
      },
    });
  });

  it('list_jobs：空参数也带 args 层与三个布尔', async () => {
    await db.listJobs();
    expect(last().payload).toEqual({
      args: { include_rejected: false, rejected_only: false, ascending: false },
    });
  });

  it('reject_jobs / allow_jobs / mark_delivered：args.jobIds', async () => {
    await db.rejectJobs(['a', 'b']);
    expect(last()).toEqual({ cmd: 'reject_jobs', payload: { args: { jobIds: ['a', 'b'] } } });
    calls = [];
    await db.allowJobs(['c']);
    expect(last()).toEqual({ cmd: 'allow_jobs', payload: { args: { jobIds: ['c'] } } });
    calls = [];
    await db.markDelivered(['d']);
    expect(last()).toEqual({ cmd: 'mark_delivered', payload: { args: { jobIds: ['d'] } } });
  });

  it('set_job_flag：空数组短路，不发 IPC 并回 0', async () => {
    await expect(db.rejectJobs([])).resolves.toBe(0);
    await expect(db.markDelivered([])).resolves.toBe(0);
    expect(calls).toHaveLength(0);
  });

  it('list_decisions：limit / afterId 驼峰，缺省才丢键', async () => {
    await db.listDecisions(50, 1234);
    expect(last()).toEqual({ cmd: 'list_decisions', payload: { args: { limit: 50, afterId: 1234 } } });
    calls = [];
    await db.listDecisions();
    expect(last()).toEqual({ cmd: 'list_decisions', payload: { args: {} } });
  });

  it('list_decisions：beforeId 向更旧翻页', async () => {
    await db.listDecisions(10, null, 99);
    expect(last()).toEqual({
      cmd: 'list_decisions',
      payload: { args: { limit: 10, afterId: null, beforeId: 99 } },
    });
  });

  it('rescore_jobs：三键显式 null，布尔恒发，filters 缺省被丢', async () => {
    await db.rescoreJobs();
    expect(last()).toEqual({
      cmd: 'rescore_jobs',
      payload: {
        args: { afterJobId: null, limit: null, useLlm: false, llmPrefilter: null, logChanges: false },
      },
    });
    calls = [];
    await db.rescoreJobs({ after_job_id: 'j1', limit: 10, use_llm: true, llm_prefilter: 2, log_changes: true });
    expect(last().payload).toEqual({
      args: { afterJobId: 'j1', limit: 10, useLlm: true, llmPrefilter: 2, logChanges: true },
    });
  });
});

describe('commands_db · 平铺的一类', () => {
  it('get_job：{ jobId }', async () => {
    await db.getJob('abc');
    expect(last()).toEqual({ cmd: 'get_job', payload: { jobId: 'abc' } });
  });

  it('update_score：键名与参数名同名（驼峰 jobId）', async () => {
    await db.updateScore('abc', 72, 'manual', '人工改分');
    expect(last()).toEqual({
      cmd: 'update_score',
      payload: { jobId: 'abc', score: 72, source: 'manual', reason: '人工改分' },
    });
  });

  // tauri 2 的命令参数 key 恒为参数的 lowerCamelCase 名：
  //   fn passes_threshold(args: ScoreArgs, min_score: u8)
  // → 两个参数都平铺在顶层；结构体字段照 `rename_all = "snake_case"` 原样平铺，
  //   标量 `min_score` 的 key 是 `minScore`（不是 `min_score`）。
  it('passes_threshold：ScoreArgs 字段平铺 + minScore（无 args 层，门槛键为驼峰）', async () => {
    await db.passesThreshold({ company: 'A', title: 'B', salary: 'C', experience: 'D', jd: 'E' }, 70);
    expect(last()).toEqual({
      cmd: 'passes_threshold',
      payload: { company: 'A', title: 'B', salary: 'C', experience: 'D', jd: 'E', minScore: 70 },
    });
    expect('args' in (last().payload as Record<string, unknown>)).toBe(false);
    // 蛇形 min_score 会被 serde 当未知字段静默忽略 → 门槛变 0，必须钉死不能回退。
    expect('min_score' in (last().payload as Record<string, unknown>)).toBe(false);
  });

  // fn score_job(llm: State<..>, args: ScoreArgs) —— `args` 是结构体参数，
  // tauri 要求 top-level 存在 `args` 键，所以必须包一层（此前平铺实现是错的）。
  it('score_job：必须包一层 { args: ScoreArgs }（结构体参数）', async () => {
    const args = { company: 'A', title: 'B', salary: 'C', experience: 'D', jd: 'E', use_llm: true };
    await db.scoreJob(args);
    expect(last()).toEqual({ cmd: 'score_job', payload: { args } });
  });
});

describe('commands_db · 无参一类', () => {
  it('全部发 {} 且只发一次', async () => {
    const pairs: Array<[() => Promise<unknown>, string]> = [
      [db.dbOverview, 'db_overview'],
      [db.migrateScores, 'migrate_scores'],
      [db.llmStatus, 'llm_status'],
      [db.jobTabCounts, 'job_tab_counts'],
    ];
    for (const [fn, cmd] of pairs) {
      calls = [];
      await fn();
      expect(last()).toEqual({ cmd, payload: {} });
    }
  });
});

describe('commands_scrape', () => {
  it('scrape_start 必须包 args（Leptos 版平铺是错的）', async () => {
    await scrape.scrapeStart({ platform: 'boss', keywords: ['Java', 'Go'], maxJd: 30, minScore: 70 });
    expect(last()).toEqual({
      cmd: 'scrape_start',
      payload: { args: { platform: 'boss', keywords: ['Java', 'Go'], maxJd: 30, minScore: 70 } },
    });
  });

  it('scrape_drain：平铺 { limit }', async () => {
    await scrape.scrapeDrain(200);
    expect(last()).toEqual({ cmd: 'scrape_drain', payload: { limit: 200 } });
  });

  it('session_open：page 有值才发', async () => {
    await scrape.sessionOpen('boss', 'chat');
    expect(last()).toEqual({ cmd: 'session_open', payload: { platform: 'boss', page: 'chat' } });
    calls = [];
    await scrape.sessionOpen('boss');
    expect(last()).toEqual({ cmd: 'session_open', payload: { platform: 'boss' } });
  });

  it('登录态四连：全部平铺 { platform }', async () => {
    await scrape.loginStart('boss');
    expect(last()).toEqual({ cmd: 'login_start', payload: { platform: 'boss' } });
    calls = [];
    await scrape.loginStatus('boss');
    expect(last()).toEqual({ cmd: 'login_status', payload: { platform: 'boss' } });
    calls = [];
    await scrape.loginSave('boss');
    expect(last()).toEqual({ cmd: 'login_save', payload: { platform: 'boss' } });
    calls = [];
    await scrape.logout('boss');
    expect(last()).toEqual({ cmd: 'logout', payload: { platform: 'boss' } });
  });

  it('无参三条', async () => {
    for (const [fn, cmd] of [
      [scrape.scrapeStop, 'scrape_stop'],
      [scrape.scrapeClose, 'scrape_close'],
      [scrape.scrapeProgress, 'scrape_progress'],
    ] as Array<[() => Promise<unknown>, string]>) {
      calls = [];
      await fn();
      expect(last()).toEqual({ cmd, payload: {} });
    }
  });
});

describe('commands_chat', () => {
  // 平铺参数的键名**是驼峰**：Tauri 的 `#[tauri::command]` 默认 `rename_all = "camelCase"`，
  // Rust `job_id` 对应前端 `jobId`。发蛇形时必填参数直接报 `missing required key`，
  // `Option<String>` 参数则静默变 `None`（「聊过就算投过」会无声失效）。
  it('chat_send：jobId 空/空白时丢掉该键', async () => {
    await chat.chatSend('u1', '你好', '');
    expect(last()).toEqual({ cmd: 'chat_send', payload: { uid: 'u1', text: '你好' } });
    calls = [];
    await chat.chatSend('u1', '你好', '   ');
    expect(last()).toEqual({ cmd: 'chat_send', payload: { uid: 'u1', text: '你好' } });
    calls = [];
    await chat.chatSend('u1', '你好', 'job-1');
    expect(last()).toEqual({ cmd: 'chat_send', payload: { uid: 'u1', text: '你好', jobId: 'job-1' } });
  });

  it('chat_send_resume：驼峰 resumeId', async () => {
    await chat.chatSendResume('u1', 'r1');
    expect(last()).toEqual({ cmd: 'chat_send_resume', payload: { uid: 'u1', resumeId: 'r1' } });
  });

  it('chat_open：驼峰 jobId', async () => {
    await chat.chatOpen('boss:abc');
    expect(last()).toEqual({ cmd: 'chat_open', payload: { jobId: 'boss:abc' } });
  });

  it('chat_messages / chat_recall：平铺 { uid }', async () => {
    await chat.chatMessages('u1');
    expect(last()).toEqual({ cmd: 'chat_messages', payload: { uid: 'u1' } });
    calls = [];
    await chat.chatRecall('u2');
    expect(last()).toEqual({ cmd: 'chat_recall', payload: { uid: 'u2' } });
  });

  it('无参三条', async () => {
    for (const [fn, cmd] of [
      [chat.chatFriends, 'chat_friends'],
      [chat.chatResumes, 'chat_resumes'],
      [chat.listExpectations, 'list_expectations'],
    ] as Array<[() => Promise<unknown>, string]>) {
      calls = [];
      await fn();
      expect(last()).toEqual({ cmd, payload: {} });
    }
  });
});

describe('commands_config', () => {
  it('set_app_config：平铺 { patch }', async () => {
    await config.setAppConfig({ model: 'gpt-x', exclude_keywords: '外包' });
    expect(last()).toEqual({
      cmd: 'set_app_config',
      payload: { patch: { model: 'gpt-x', exclude_keywords: '外包' } },
    });
  });

  it('set_app_config：min_score 永远不进 patch（只能走 set_limit）', async () => {
    // 含脏键的 patch 走真封装，但网络层是假的，所以这里断言的是**真正发出去**的载荷。
    const dirty: Record<string, unknown> = {};
    dirty.model = 'gpt-x';
    dirty.min_score = 70;
    dirty.api_key_hint = '****abcd';
    await config.setAppConfig(dirty);
    const patch = (last().payload as { patch: Record<string, unknown> }).patch;
    expect(Object.keys(patch)).not.toContain('min_score');
    expect(Object.keys(patch)).not.toContain('api_key_hint');
    expect(patch).toEqual({ model: 'gpt-x' });
  });

  it('sanitizeConfigPatch 不改入参、剔禁用键与 undefined', () => {
    const input = { a: 1, min_score: 70, api_key_hint: '****abcd', b: undefined };
    const out = config.sanitizeConfigPatch(input);
    expect(out).toEqual({ a: 1 });
    expect(input.min_score).toBe(70);
  });

  it('set_limit：args 层 + 驼峰，缺席表示不动', async () => {
    await config.setLimit(30, 70);
    expect(last()).toEqual({ cmd: 'set_limit', payload: { args: { maxJd: 30, minScore: 70 } } });
    calls = [];
    await config.setLimit(null, 65);
    expect(last()).toEqual({ cmd: 'set_limit', payload: { args: { minScore: 65 } } });
  });

  it('set_scrape_defaults：args 层，只带变化字段', async () => {
    await config.setScrapeDefaults({ city: '101020100', maxPages: 5 });
    expect(last()).toEqual({
      cmd: 'set_scrape_defaults',
      payload: { args: { city: '101020100', maxPages: 5 } },
    });
  });

  it('get_app_config：无参', async () => {
    await config.getAppConfig();
    expect(last()).toEqual({ cmd: 'get_app_config', payload: {} });
  });
});

describe('commands_auto（C2 明文的形状例外）', () => {
  it('auto_greet_next：包 args；无 jobId 时是 { args: {} }', async () => {
    await auto.autoGreetNext();
    expect(last()).toEqual({ cmd: 'auto_greet_next', payload: { args: {} } });
    calls = [];
    await auto.autoGreetNext('job-1');
    expect(last()).toEqual({ cmd: 'auto_greet_next', payload: { args: { jobId: 'job-1' } } });
  });

  it('auto_greet_pause：平铺，reason 有值才发', async () => {
    await auto.autoGreetPause();
    expect(last()).toEqual({ cmd: 'auto_greet_pause', payload: {} });
    calls = [];
    await auto.autoGreetPause('用户点了暂停');
    expect(last()).toEqual({ cmd: 'auto_greet_pause', payload: { reason: '用户点了暂停' } });
  });

  it('resume / status / reset：无参', async () => {
    for (const [fn, cmd] of [
      [auto.autoGreetResume, 'auto_greet_resume'],
      [auto.autoGreetStatus, 'auto_greet_status'],
      [auto.autoGreetReset, 'auto_greet_reset'],
    ] as Array<[() => Promise<unknown>, string]>) {
      calls = [];
      await fn();
      expect(last()).toEqual({ cmd, payload: {} });
    }
  });
});

describe('commands_sensitive', () => {
  it('sensitive_list：args 层，五个键有值才加', async () => {
    await sensitive.sensitiveList({ platform: 'boss', kind: 'stored' });
    expect(last()).toEqual({
      cmd: 'sensitive_list',
      payload: { args: { platform: 'boss', kind: 'stored' } },
    });
    calls = [];
    await sensitive.sensitiveList();
    expect(last()).toEqual({ cmd: 'sensitive_list', payload: { args: {} } });
  });

  it('sensitive_remove：args.ids，空数组短路不发 IPC', async () => {
    await expect(sensitive.sensitiveRemove([1, 2])).resolves.toBe(2);
    expect(last()).toEqual({ cmd: 'sensitive_remove', payload: { args: { ids: [1, 2] } } });
    calls = [];
    await expect(sensitive.sensitiveRemove([])).resolves.toBe(0);
    expect(calls).toHaveLength(0);
  });
});

describe('commands_logs', () => {
  it('list_logs：since 有值才发（0 也算有值）', async () => {
    await logs.listLogs(1_700_000_000_000);
    expect(last()).toEqual({ cmd: 'list_logs', payload: { since: 1_700_000_000_000 } });
    calls = [];
    await logs.listLogs();
    expect(last()).toEqual({ cmd: 'list_logs', payload: {} });
    calls = [];
    await logs.listLogs(0);
    expect(last()).toEqual({ cmd: 'list_logs', payload: { since: 0 } });
  });

  it('clear_logs：无参', async () => {
    await logs.clearLogs();
    expect(last()).toEqual({ cmd: 'clear_logs', payload: {} });
  });
});

describe('commands_greet', () => {
  it('greet_draft：包 args，useLlm 恒 true，缺省补空串', async () => {
    await greet.greetDraft({
      jobId: 'j1',
      title: '后端',
      company: 'A',
      salary: '20-30K',
      experience: '3-5年',
      jd: 'JD',
    });
    expect(last()).toEqual({
      cmd: 'greet_draft',
      payload: {
        args: {
          jobId: 'j1',
          title: '后端',
          company: 'A',
          salary: '20-30K',
          experience: '3-5年',
          jd: 'JD',
          findByCompany: '',
          reply: '',
          useLlm: true,
        },
      },
    });
  });

  it('greet_draft：聊天页带 findByCompany 与 reply（重新生成 / 生成回复）', async () => {
    await greet.greetDraft({
      jobId: '',
      title: '',
      company: '星瀚科技',
      salary: '',
      experience: '',
      jd: '',
      findByCompany: '星瀚科技',
      reply: '你的 Redis 经验说一下',
    });
    const args = (last().payload as { args: Record<string, unknown> }).args;
    expect(args.findByCompany).toBe('星瀚科技');
    expect(args.reply).toBe('你的 Redis 经验说一下');
    expect(args.useLlm).toBe(true);
  });

  it('greet_one：平铺 { uid, text }', async () => {
    await greet.greetOne('u1', '您好');
    expect(last()).toEqual({ cmd: 'greet_one', payload: { uid: 'u1', text: '您好' } });
  });

  it('greet_one：带岗位时发驼峰 jobId（后端据此标已投递）', async () => {
    // 键名必须是驼峰：蛇形 `job_id` 会被当成未知键，而它是 `Option<String>`，
    // 静默变 None —— 消息照发，岗位永远不进已投递，且没有任何报错。
    await greet.greetOne('u1', '您好', 'boss:1');
    expect(last()).toEqual({
      cmd: 'greet_one',
      payload: { uid: 'u1', text: '您好', jobId: 'boss:1' },
    });
  });
});
