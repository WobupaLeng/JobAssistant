// ?mode= 直达表编译期校验（无需浏览器）：独立复刻 index.html applyModeQuery 的解析顺序，
// 断言每个 mode 都落到预期 flag 组合。
import { readFileSync } from 'node:fs';
import { runStates } from './_states.mjs';

const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
const script = [...html.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join('\n');

function parse(q) {
  q = String(q || '').replace(/^\?/, '');
  const com = /(?:^|-)(chat-open-(f\d+))$/.exec(q);
  const out = {};
  // 与 index.html 同序：filters → sensitive → chatOpen → chat → decisions → config，逐轮从尾部剥后缀
  const order = [
    ['filters', /-?filters$/],
    ['sensitive', /-?sensitive$/],
    ['chatOpen', /-?chat-open-f\d+$/],
    ['chat', /-?chat$/],
    ['decisions', /-?decisions$/],
    ['config', /-?config$/],
  ];
  let core = q;
  for (const [k, re] of order) {
    if (re.test(core)) { out[k] = true; core = core.replace(new RegExp('-?' + k.replace(/([A-Z])/g, '$1') + '$'), ''); }
    else out[k] = false;
  }
  out.rest = core;
  out.uid = com ? com[2] : null;
  return out;
}

const cases = [
  ['', { chatOpen: false, uid: null }],
  ['default-chat', { chat: true, chatOpen: false }],
  ['default-chat-open-f1', { chatOpen: true, uid: 'f1' }],
  ['dark-rails-empty-chat-open-f2', { chatOpen: true, uid: 'f2' }],
  ['light-empty-error', { chatOpen: false }],
  ['light-empty-error-filters', { filters: true }],
  ['*-sensitive', { sensitive: true }],
  ['dark-decisions', { decisions: true }],
  ['light-config', { config: true }],
];

let bad = 0;
for (const [mode, want] of cases) {
  const got = parse(mode);
  for (const [k, v] of Object.entries(want)) {
    if (k === 'uid' ? got.uid !== v : got[k] !== v) {
      console.log(`FAIL  ?mode=${mode}  ${k}: want ${v}, got ${k === 'uid' ? got.uid : got[k]}`);
      bad++;
    }
  }
}

// 已废止的 mode 必须不再被识别为独立分支（只能退化到 name-filters 一类，绝不出现 detach 语义）
for (const dead of ['default-detach', 'default-detachfull-f1']) {
  const got = parse(dead);
  if (got.chatOpen || (got.uid && dead.includes('detachfull'))) {
    console.log(`FAIL  ?mode=${dead} 仍被识别为展开态`);
    bad++;
  }
}

if (!/default-chat-open-f1/.test(script)) { console.log('FAIL  index.html 注释表缺 default-chat-open-f1'); bad++; }
if (/default-detach(full)?-f\d+/.test(script)) { console.log('FAIL  index.html 注释表仍含 default-detach*'); bad++; }

// ---- 行为三态（裁决 D 抓取 / 裁决 E+F 自动投递）：与 _tri.mjs 同款最小 DOM 桩，点同一颗按钮逐态核对 ----
if (!/__PROTO__/.test(script)) { console.log("FAIL  index.html 缺 __PROTO__ 探针"); bad++; }
const tri = [];
try {
  const st = runStates(script);
  for (const f of st.fails) { console.log("FAIL  " + f); bad++; }
  tri.push(...st.tri);
} catch (e) { console.log("FAIL  三态自检异常：" + e.message); bad++; }

console.log(bad === 0 ? `MODES OK · ${cases.length} cases + 2 dead modes · 三态: ${tri.join(" → ")}` : `MODES FAILED · ${bad}`);
process.exit(bad === 0 ? 0 : 1);
