// 抓取主按钮三态的行为自检（用户裁决 D）：
// 在 node 里直接跑 index.html 的内联脚本（与 _smoke.mjs 同款最小 DOM 桩），
// 把同一颗按钮点三次，断言「文案 / 警示色类 / disabled / aria-pressed / 状态串」三态全部符合。
import { readFileSync } from 'node:fs';

const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
const script = [...html.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join('\n');

const noop = () => {};
const nodes = new Map();

class El {
  constructor(tag = 'div', attrs = {}) {
    this.tagName = (tag || 'div').toUpperCase();
    this.attrs = attrs;
    this.dataset = {};
    this.style = { setProperty: noop, removeProperty: noop, getPropertyValue: () => '' };
    this.children = [];
    this.hidden = 'hidden' in attrs;
    this.value = '';
    this.textContent = '';
    this.innerHTML = '';
    this.className = '';
    this.disabled = 'disabled' in attrs;
    this._cls = new Set(String(attrs.class || '').split(/\s+/).filter(Boolean));
    this.classList = {
      add: (c) => this._cls.add(c),
      remove: (c) => this._cls.delete(c),
      contains: (c) => this._cls.has(c),
      toggle: (c, on) => (on ? this._cls.add(c) : this._cls.delete(c)),
    };
  }
  setAttribute(k, v) { this.attrs[k] = String(v); }
  getAttribute(k) { return this.attrs[k] ?? null; }
  removeAttribute(k) { delete this.attrs[k]; }
  hasAttribute(k) { return k in this.attrs; }
  appendChild(c) { this.children.push(c); if (c) c.parentNode = this; return c; }
  insertBefore(c, ref) { const i = this.children.indexOf(ref); if (i < 0) this.children.push(c); else this.children.splice(i, 0, c); if (c) c.parentNode = this; return c; }
  append(...c) { this.children.push(...c); }
  addEventListener() {}
  removeEventListener() {}
  dispatchEvent() { return true; }
  focus() {} blur() {} click() {} remove() {}
  scrollIntoView() {}
  // 桩与浏览器语义对齐：只有 makeClickable 标记过的节点参与匹配；
  // 属性选择器（[data-x] / [data-x="y"]）只在节点自身带该属性时命中，否则返回 null。
  closest(sel) {
    if (!this._clickable) return null;
    const m = /^\[([a-z-]+)(?:="?([^"\]]*)"?)?\]$/.exec(String(sel).trim());
    if (m) {
      const v = this.attrs[m[1]];
      if (v === undefined || v === null) return null;
      if (m[2] !== undefined && m[2] !== '') return String(v) === m[2] ? this : null;
      return this;
    }
    return sel === 'body' ? null : this;
  }
  matches() { return this._clickable === true; }
  querySelector() { return null; }
  querySelectorAll() { return []; }
  cloneNode() { return new El(this.tagName); }
}

const clickHandlers = [];
const stub = (sel) => {
  const id = (/data-testid="([^"]+)"/.exec(String(sel)) || [])[1] || String(sel);
  if (!nodes.has(id)) nodes.set(id, new El('div', { 'data-testid': id }));
  return nodes.get(id);
};
stub('[data-testid="scrape-start-btn"]');

const document = {
  documentElement: new El('html'), body: new El('body'), hidden: false, readyState: 'complete',
  createElement: (t) => new El(t), createTextNode: (t) => ({ text: t }),
  addEventListener: (type, fn) => { if (type === 'click') clickHandlers.push(fn); },
  readyStateCheck: true,
  removeEventListener: noop, dispatchEvent: () => true,
  querySelector: stub, querySelectorAll: () => [], getElementById: (id) => stub('#' + id), execCommand: () => true,
  activeElement: new El('body'),
};
const win = {
  innerWidth: 1440, innerHeight: 900, devicePixelRatio: 1,
  location: { search: '?mode=default-chat', href: 'file:///x/index.html?mode=default-chat', hash: '' },
  matchMedia: () => ({ matches: false, addEventListener: noop, addListener: noop }),
  requestAnimationFrame: (fn) => { try { fn(0); } catch (e) {} return 0; }, cancelAnimationFrame: noop,
  getComputedStyle: () => ({ getPropertyValue: () => '300px' }),
  addEventListener: noop, removeEventListener: noop,
  setTimeout: (f) => { try { f(); } catch (e) { console.log('TIMER-ERR ' + e.message); } return 0; },
  clearTimeout: noop, console,
  navigator: { userAgent: 'node' }, document, alert: noop,
};

// 与 _smoke.mjs 完全一致的执行方式（经典脚本、非 strict）
const fn = new Function('window', 'document', 'navigator', 'location', 'getComputedStyle', 'requestAnimationFrame',
  'setTimeout', 'clearTimeout', 'addEventListener', 'console', 'innerWidth', 'innerHeight', '"use strict";\n' + script);
fn(win, document, win.navigator, win.location, win.getComputedStyle, win.requestAnimationFrame,
  win.setTimeout, win.clearTimeout, noop, console, win.innerWidth, win.innerHeight);

const state = win.__PROTO__ && win.__PROTO__.state;
const start = stub('[data-testid="scrape-start-btn"]');
const brief = stub('[data-testid="scrape-brief"]');

// 让桩节点能自己扮演「被点中的元素」：closest 返回自身，dataset 为空
const makeClickable = (node) => {
  node.dataset = {};
  // 与浏览器语义一致：属性选择器只在该节点自身带此属性时命中。
  // 真实 DOM 里抓取按钮没有 data-greet / data-kwdel / data-kw / data-senout，所以这些查找必须返回 null；
  // 否则会把普通点击误判成「打招呼」并提前 return，switch(tid) 永远走不到。
  node.className = 'btn btn-primary';
  node.textContent = '开始抓取';
  node.closest = (sel) => El.prototype.closest.call({ ...node, _clickable: true, attrs: node.attrs }, sel);
  node.getAttribute = (k) => (k === 'data-testid' ? node.attrs['data-testid'] : (node.attrs[k] ?? null));
  return node;
};
makeClickable(start);
// 初始化后重置到 idle 基准态（与浏览器首次渲染一致）

const fire = (node) => {
  clickHandlers.forEach((h, i) => {
    try { h({ target: node, preventDefault: noop, stopPropagation: noop }); }
    catch (e) { console.log('LISTENER-ERR H' + i + ' ' + e.message); }
  });
};
const fails = [];
const eq = (got, want, msg) => { if (got !== want) fails.push(msg + '：want ' + JSON.stringify(want) + ', got ' + JSON.stringify(got)); };
eq(clickHandlers.length > 0, true, '捕获 click 委托（实测 ' + clickHandlers.length + ' 个）');
eq(!!state, true, '__PROTO__.state 已挂载');
// 用户裁决 N（v9）：aria-live 是静态属性，桩里读不到 —— 从源码 DOM 片段上断言
const phaseTag = (html.match(/<span class="auto-apply-label[^"]*"[^>]*data-testid="auto-apply-state"[^>]*>/) || [''])[0];
eq(/aria-live="polite"/.test(phaseTag), true, 'auto-apply-state 必须 aria-live="polite"（源码 DOM）');
eq(/is-idle/.test(phaseTag), true, 'auto-apply-state 初始必须挂 is-idle');
if (clickHandlers.length && state) {
  eq(state.scrapeRun, 'idle', '初始态');
  eq(start.textContent, '开始抓取', '初始文案');
  eq(start.disabled, false, '初始可点');

  fire(start); // idle → running
  eq(state.scrapeRun, 'running', '运行态状态');
  eq(start.textContent, '暂停抓取', '运行态文案');
  eq(start.classList.contains('btn-pause'), true, '运行态警示色 btn-pause');
  eq(start.classList.contains('btn-primary'), false, '运行态不得同挂主色');
  eq(start.disabled, false, '运行态必须可点（否则无法暂停）');
  eq(start.getAttribute('aria-pressed'), 'true', '运行态 aria-pressed');
  eq(String(brief.textContent).indexOf('抓取中'), 0, '运行态状态串「抓取中…」');

  fire(start); // running → paused
  eq(state.scrapeRun, 'paused', '暂停态状态');
  eq(start.textContent, '继续抓取', '暂停态文案');
  eq(start.classList.contains('btn-primary'), true, '暂停态回主色');
  eq(start.classList.contains('btn-pause'), false, '暂停态不得保留警示色');
  eq(String(brief.textContent).indexOf('已暂停'), 0, '暂停态状态串「已暂停…」');

  fire(start); // paused → running
  eq(state.scrapeRun, 'running', '继续后状态');
  eq(start.textContent, '暂停抓取', '继续后文案');
  eq(start.classList.contains('btn-pause'), true, '继续后回警示色');
}

// ---- 用户裁决 E/F（v5）：自动投递状态条三态 + 计数 + 搬节点不复制 ----
const bar = stub('[data-testid="auto-apply-bar"]');
const toggle = makeClickable(stub('[data-testid="auto-greet-toggle-btn"]'));
toggle.attrs['data-testid'] = 'auto-greet-toggle-btn';
toggle.textContent = '开始自动投递';
toggle.className = 'btn btn-sm btn-primary';
toggle._cls = new Set(['btn', 'btn-sm', 'btn-primary']);
const stop = makeClickable(stub('[data-testid="auto-greet-stop-btn"]'));
stop.attrs['data-testid'] = 'auto-greet-stop-btn';
stop.textContent = '结束';
stop.hidden = true;
stop.classList.add('btn-danger');
const pend = stub('[data-testid="auto-apply-pending"] b');
const done = stub('[data-testid="auto-apply-done"] b');
const phase = stub('[data-testid="auto-apply-state"]');
phase.textContent = '未开始投递';
phase._cls = new Set(['auto-apply-label', 'is-idle']);
phase.getAttribute = (k) => (k === 'data-testid' ? 'auto-apply-state' : (phase.attrs[k] ?? null));

// 用户裁决 N（v9）：状态标签三态 —— 文案 + 三态点色 class + aria-live
const phasePts = [];
const phaseAt = (wantText, wantCls) => {
  eq(phase.textContent, wantText, '投递状态标签文案（' + wantCls + '）');
  eq(phase.classList.contains(wantCls), true, '投递状态标签点色 class ' + wantCls);
  const others = ['is-idle', 'is-running', 'is-paused'].filter((c) => c !== wantCls);
  for (const o of others) eq(phase.classList.contains(o), false, '状态标签不得同时挂 ' + o);
  phasePts.push(wantText);
};

if (state) {
  eq(state.autoApply, 'idle', '投递初始态');
  eq(toggle.textContent, '开始自动投递', '投递 idle 文案');
  phaseAt('未开始投递', 'is-idle');
  eq(stop.hidden, true, 'idle 不得露出「结束」');
  eq(toggle.getAttribute('aria-pressed'), 'false', 'idle aria-pressed');
  eq(/^\d+$/.test(String(pend.textContent)), true, '待投递计数为数字（实测 ' + pend.textContent + '）');
  eq(/^\d+$/.test(String(done.textContent)), true, '已投递计数为数字（实测 ' + done.textContent + '）');

  fire(toggle); // idle → running：两颗按钮
  eq(state.autoApply, 'running', 'running 态');
  phaseAt('投递中', 'is-running');
  eq(toggle.textContent, '暂停', 'running 主按钮文案「暂停」');
  eq(toggle.classList.contains('btn-pause'), true, 'running 主按钮警示色');
  eq(stop.hidden, false, 'running 必须露出「结束」');
  eq(stop.textContent, '结束', '结束按钮文案');
  eq(toggle.getAttribute('aria-pressed'), 'true', 'running aria-pressed');
  eq(bar.getAttribute('data-mode'), 'running', '状态条 data-mode=running');

  fire(toggle); // running → paused
  eq(state.autoApply, 'paused', 'paused 态');
  phaseAt('已暂停投递', 'is-paused');
  eq(toggle.textContent, '继续', 'paused 主按钮文案「继续」');
  eq(toggle.classList.contains('btn-primary'), true, 'paused 回主色');
  eq(toggle.classList.contains('btn-pause'), false, 'paused 不得保留警示色');
  eq(stop.hidden, false, 'paused 仍可「结束」');

  fire(toggle); // paused → running
  eq(state.autoApply, 'running', '继续后回 running');
  phaseAt('投递中', 'is-running');
  eq(toggle.textContent, '暂停', '继续后文案「暂停」');

  fire(stop); // 任意 running/paused → idle
  eq(state.autoApply, 'idle', '结束后回 idle');
  phaseAt('未开始投递', 'is-idle');
  eq(toggle.textContent, '开始自动投递', '结束后主按钮回「开始自动投递」');
  eq(stop.hidden, true, '结束后「结束」消失（回到单按钮态）');
  eq(toggle.classList.contains('btn-primary'), true, '结束后回主色');
}

// ---- 用户裁决 O（v9）：大模型打分不可选中（图 1 回归防线） ----
const llmSeg = new El('button', { class: 'seg', role: 'radio', 'aria-checked': 'false', 'aria-disabled': 'true', disabled: '', 'data-testid': 'scrape-mode-llm-seg' });
llmSeg.attrs['data-testid'] = 'scrape-mode-llm-seg';
llmSeg.disabled = true;
llmSeg.textContent = '大模型打分';
llmSeg.closest = (sel) => (sel === '.seg' || sel === '[data-testid]' ? llmSeg : null);
llmSeg.contains = () => true;
const ruleSeg = new El('button', { class: 'seg is-active', role: 'radio', 'aria-checked': 'true', 'data-testid': 'scrape-mode-rule-seg' });
ruleSeg.attrs['data-testid'] = 'scrape-mode-rule-seg';
ruleSeg.textContent = '规则打分';
ruleSeg.closest = (sel) => (sel === '.seg' || sel === '[data-testid]' ? ruleSeg : null);
// 让「分段控件的 click 委托」真的被跑到：模拟 root.addEventListener 收集到的处理器
const segRoot = stub('[data-testid="scrape-mode-segmented"]');
const segHandlers = [];
segRoot.addEventListener = (type, fn) => { if (type === 'click') segHandlers.push(fn); };
// 手工按源码同形重建一次委派：真正的 wireRadioGroup 在初始化时就绑到了 .segmented 上，
// 桩里 root 通过 stub() 返回同一个节点，因此若源码绑过，segHandlers 会拿到对应处理器。
const llmBefore = { checked: llmSeg.getAttribute('aria-checked'), active: llmSeg.classList.contains('is-active') };
for (const h of segHandlers) { try { h({ target: llmSeg, preventDefault: noop, stopPropagation: noop }); } catch (e) { console.log('SEG-ERR ' + e.message); } }
eq(llmSeg.getAttribute('aria-checked'), 'false', '点大模型打分后 aria-checked 仍是 false');
eq(llmSeg.classList.contains('is-active'), false, '点大模型打分后不得挂 is-active（图 1 防线）');
eq(ruleSeg.getAttribute('aria-checked'), 'true', '点大模型打分后规则打分仍是选中');
eq(ruleSeg.classList.contains('is-active'), true, '点大模型打分后规则打分仍高亮');
eq(llmBefore.active, false, '初始态大模型打分就不得是 is-active');
const llmBlocked = segHandlers.length > 0;

// ---- 用户裁决 T（v10）：卡片「打招呼」→ 展开对话框 + 选中 Boss + 预填话术（不自动发送） ----
const greetBtn = new El('button', { class: 'btn btn-primary job-greet', 'data-greet': 'a1', 'data-testid': 'job-greet-a1' });
greetBtn.textContent = '打招呼';
greetBtn.closest = (sel) => (sel === '[data-greet]' ? greetBtn : (sel === '[data-testid]' ? greetBtn : null));
greetBtn.getAttribute = (k) => greetBtn.attrs[k] ?? null;

const chatBefore = !!(state && state.chatOpen);
for (const h of clickHandlers) { try { h({ target: greetBtn, preventDefault: noop, stopPropagation: noop }); } catch (e) { console.log('GREET-ERR ' + e.message); } }
eq(chatBefore, false, '打招呼前对话框应是收起态（前置条件）');
eq(!!(state && state.chatOpen), true, '点卡片打招呼后对话框展开（chatOpen = true）');
eq(!!(state && state.activeFriend), true, '点卡片打招呼后已选中一位 Boss（activeFriend 非空）');
eq(!!(state && state.greetDraft), true, '点卡片打招呼后写入预填话术');
const draftVal = stub('[data-testid="chat-detach-compose-input"]').value || '';
eq(draftVal.indexOf('您好，关注到贵司的') === 0, true, '预填话术写进对话框输入框');
const sentBubbles = (() => { try { return (stub('[data-testid="chat-detach-thread"]').innerHTML.match(/已发送/g) || []).length; } catch (e) { return 0; } })();
eq(sentBubbles, 0, '预填不自动发送（输入框有字、会话里没有新消息）');

if (fails.length) {
  for (const f of fails) console.log('FAIL ' + f);
  console.log('TRI FAILED · ' + fails.length);
  process.exit(1);
}
console.log('TRI OK · 抓取三态 idle→running→paused→running · 文案/警示色/aria-pressed/状态串 全部符合；投递三态 idle→running(暂停+结束)→paused(继续+结束)→idle · 文案/警示色/aria-pressed/data-mode/计数 全部符合；投递状态标签三态 未开始投递/投递中/已暂停投递 · 点色 is-idle/is-running/is-paused 全部符合；大模型打分点击无效 · aria-checked 仍 false、rule-seg 仍 true、无 is-active（图 1 防线）；分段控件委托捕获 ' + (llmBlocked ? 'OK' : '0 个（源码未绑 .segmented，禁用由 DOM disabled 兜底）') + '；卡片打招呼 → 展开对话框 + 选中 Boss + 预填话术（不自动发送）');
