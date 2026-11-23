// 行为三态自检（裁决 D 抓取三态 + 裁决 E/F 自动投递三态），供 _modes.mjs import。
// 与 _tri.mjs 同款最小 DOM 桩：不是渲染验证，只核对状态机写出来的可见结果（文案 / 类名 / aria-pressed / 状态串 / 计数）。
export function runStates(script) {
  const noop = () => {};
  // '[data-testid="x"] b' 这类带后代选择器的取值，桩里折成同一个节点 id（'x b'），与 _smoke.mjs 同款约定
  const attrsOf = (sel) => {
    const s = String(sel);
    const m = /data-testid="([^"]+)"/.exec(s);
    const desc = /\](?:\s+|>)([a-z][\w-]*)\s*$/.exec(s);
    if (m) return m[1] + (desc ? ' ' + desc[1] : '');
    return s;
  };
  const nodes = new Map();
  const mk = (id) => {
    const n = { testid: id, textContent: '', disabled: false, hidden: false, attrs: {}, dataset: {}, parentNode: null, children: [], _s: new Set(),
      style: { setProperty: noop, removeProperty: noop, getPropertyValue: () => '' },
      setAttribute(k, v) { this.attrs[k] = String(v); }, getAttribute(k) { return this.attrs[k] ?? null; },
      removeAttribute(k) { delete this.attrs[k]; }, hasAttribute(k) { return k in this.attrs; },
      addEventListener: noop, removeEventListener: noop, dispatchEvent: () => true,
      appendChild(c) { this.children.push(c); if (c) c.parentNode = this; return c; },
      insertBefore(c, ref) { const i = this.children.indexOf(ref); if (i < 0) this.children.push(c); else this.children.splice(i, 0, c); if (c) c.parentNode = this; return c; },
      scrollIntoView: noop, focus: noop, blur: noop, click: noop, remove: noop, removeProperty: noop, setProperty: noop,
      querySelector: () => null, querySelectorAll: () => [], matches: () => false, closest: () => null,
      getBoundingClientRect: () => ({ x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 }) };
    n.classList = {
      add: (c) => n._s.add(c), remove: (c) => n._s.delete(c), contains: (c) => n._s.has(c),
      toggle: (c, on) => (on === undefined ? (n._s.has(c) ? n._s.delete(c) : n._s.add(c)) : (on ? n._s.add(c) : n._s.delete(c))),
    };
    return n;
  };
  const node = (id) => { if (!nodes.has(id)) nodes.set(id, mk(id)); return nodes.get(id); };

  // 被点中的元素：closest 与浏览器语义对齐 ——
  // 属性选择器（[data-x] / [data-x="y"]）只在该节点自身带此属性时才命中，其余选择器命中自身。
  // 真实 DOM 里抓取/投递按钮没有 data-greet / data-kwdel / data-kw / data-senout，
  // 桩若一律命中就会把普通点击误判成「打招呼」并提前 return，switch(tid) 永远走不到（见 HANDOFF）。
  const clickable = (id, label, cls) => {
    const n = node(id);
    n.textContent = label;
    n.attrs['data-testid'] = id;
    n._s = new Set((cls || '').split(/\s+/).filter(Boolean));
    n.closest = (sel) => {
      const m = /^\[([a-z-]+)(?:="?([^"\]]*)"?)?\]$/.exec(String(sel).trim());
      if (m) {
        const v = n.attrs[m[1]];
        if (v === undefined || v === null) return null;
        if (m[2] !== undefined && m[2] !== '') return String(v) === m[2] ? n : null;
        return n;
      }
      return sel === 'body' ? null : n;
    };
    return n;
  };

  clickable('scrape-start-btn', '开始抓取', 'btn btn-primary');
  clickable('auto-greet-toggle-btn', '开始自动投递', 'btn btn-sm btn-primary');
  const stop = clickable('auto-greet-stop-btn', '结束', 'btn btn-sm btn-danger');
  stop.hidden = true;
  node('scrape-stop-btn').disabled = true;
  const bar = node('auto-apply-bar');
  bar.attrs['data-mode'] = 'idle';

  const api = {};                 // 暴露给 _modes.mjs 的钩子（测试专用，不在原型里）
  const clickHandlers = [];
  let clickHandler = null;
  const document = {
    documentElement: mk('html'), body: mk('body'), head: mk('head'), hidden: false, readyState: 'complete',
    createElement: (t) => mk(t || 'div'), createTextNode: (t) => ({ text: t }),
    // 与 _tri.mjs 一致：收集成数组，任一监听器报错不影响其它（也照出原型里注册了几份）
    addEventListener: (type, fn) => { if (type === 'click') clickHandlers.push(fn); }, removeEventListener: noop,
    dispatchEvent: () => true, querySelector: (sel) => node(attrsOf(sel)), querySelectorAll: () => [],
    getElementById: (id) => node('#' + id), execCommand: () => true, activeElement: mk('body'),
  };
  const win = {
    innerWidth: 1440, innerHeight: 900, devicePixelRatio: 1,
    location: { search: '?mode=default-chat', href: 'file:///x/index.html?mode=default-chat', hash: '' },
    matchMedia: () => ({ matches: false, addEventListener: noop, addListener: noop, removeEventListener: noop }),
    requestAnimationFrame: (f) => { try { f(0); } catch (e) {} return 0; }, cancelAnimationFrame: noop,
    getComputedStyle: () => ({ getPropertyValue: () => '300px' }),
    addEventListener: noop, removeEventListener: noop,
    setTimeout: (f) => { try { f(); } catch (e) {} return 0; }, clearTimeout: noop,
  };

  new Function('window', 'document', 'navigator', 'location', 'getComputedStyle', 'requestAnimationFrame',
    'setTimeout', 'clearTimeout', 'addEventListener', 'console', 'innerWidth', 'innerHeight',
    '"use strict";\n' + script)(win, document, win.navigator, win.location, win.getComputedStyle, win.requestAnimationFrame,
    win.setTimeout, win.clearTimeout, noop, console, win.innerWidth, win.innerHeight);

  const fails = [];
  const tri = [];
  const eq = (got, want, msg) => { if (got !== want) fails.push(msg + '：want ' + JSON.stringify(want) + ', got ' + JSON.stringify(got)); };
  const state = win.__PROTO__ && win.__PROTO__.state;
  const origSet = win.__PROTO__ && win.__PROTO__.setScrapeRun;
  if (!clickHandlers.length || !state) { fails.push('未捕获 click 委托或 __PROTO__.state 未挂载'); return { fails, tri }; }

  // 桩节点上「被点中的元素」：closest 绑定桩自身，data-testid 读桩的 id
  const fire = (n) => {
    const target = { dataset: {}, href: '', keepalive: false, flag: undefined, job: undefined, uid: undefined,
      kw: undefined, kwdel: undefined, senout: undefined,
      get closest() { return (sel) => n.closest(sel); }, get id() { return n.testid; }, get isConnected() { return true; }, get nodeType() { return 1; },
      get textContent() { return n.textContent; }, set textContent(v) { n.textContent = v; },
      get disabled() { return n.disabled; }, set disabled(v) { n.disabled = v; },
      get hidden() { return n.hidden; }, set hidden(v) { n.hidden = v; },
      classList: n.classList, getAttribute: (k) => (k === 'data-testid' ? n.testid : (n.attrs[k] ?? null)),
      setAttribute: (k, v) => n.setAttribute(k, v),
      querySelector: () => null, querySelectorAll: () => [] };
    for (const h of clickHandlers) {
      try { h({ target: target, preventDefault: noop, stopPropagation: noop }); }
      catch (e) { fails.push('click 处理抛错（' + n.testid + '）：' + e.message); }
    }
  };

  // ---- 裁决 D：抓取主按钮三态 ----
  const start = node('scrape-start-btn');
  const brief = node('scrape-brief');
  eq(state.scrapeRun, 'idle', '抓取初始态');
  eq(start.textContent, '开始抓取', '抓取初始文案');
  fire(start);
  eq(state.scrapeRun, 'running', '抓取运行态 state');
  eq(start.textContent, '暂停抓取', '抓取运行态文案');
  eq(start.classList.contains('btn-pause'), true, '抓取运行态 btn-pause');
  eq(start.disabled, false, '抓取运行态可点');
  eq(start.getAttribute('aria-pressed'), 'true', '抓取运行态 aria-pressed');
  eq(String(brief.textContent).indexOf('抓取中'), 0, '抓取运行态状态串「抓取中…」');
  fire(start);
  eq(state.scrapeRun, 'paused', '抓取暂停态 state');
  eq(start.textContent, '继续抓取', '抓取暂停态文案');
  eq(start.classList.contains('btn-primary'), true, '抓取暂停态回主色');
  eq(start.classList.contains('btn-pause'), false, '抓取暂停态不得保留警示色');
  eq(String(brief.textContent).indexOf('已暂停'), 0, '抓取暂停态状态串「已暂停…」');
  fire(start);
  eq(start.textContent, '暂停抓取', '抓取继续后文案');
  eq(start.classList.contains('btn-pause'), true, '抓取继续后回警示色');
  tri.push('抓取 idle→running→paused→running');

  // ---- 裁决 E/F：自动投递状态条三态 + 计数 ----
  const toggle = node('auto-greet-toggle-btn');
  const pend = node('auto-apply-pending b');
  const done = node('auto-apply-done b');
  eq(state.autoApply, 'idle', '投递初始态');
  eq(toggle.textContent, '开始自动投递', '投递 idle 文案');
  eq(stop.hidden, true, 'idle 不得露出「结束」');
  eq(toggle.getAttribute('aria-pressed'), 'false', '投递 idle aria-pressed');
  eq(/^\d+$/.test(String(pend.textContent)), true, '待投递计数为数字（' + pend.textContent + '）');
  eq(/^\d+$/.test(String(done.textContent)), true, '已投递计数为数字（' + done.textContent + '）');
  fire(toggle);
  eq(state.autoApply, 'running', '投递 running 态');
  eq(toggle.textContent, '暂停', '投递 running 主按钮「暂停」');
  eq(toggle.classList.contains('btn-pause'), true, '投递 running 警示色');
  eq(stop.hidden, false, '投递 running 露出「结束」');
  eq(stop.textContent, '结束', '结束按钮文案');
  eq(toggle.getAttribute('aria-pressed'), 'true', '投递 running aria-pressed');
  eq(bar.attrs['data-mode'], 'running', '状态条 data-mode=running');
  fire(toggle);
  eq(state.autoApply, 'paused', '投递 paused 态');
  eq(toggle.textContent, '继续', '投递 paused 主按钮「继续」');
  eq(toggle.classList.contains('btn-primary'), true, '投递 paused 回主色');
  eq(toggle.classList.contains('btn-pause'), false, '投递 paused 不得保留警示色');
  eq(stop.hidden, false, '投递 paused 仍可「结束」');
  fire(toggle);
  eq(state.autoApply, 'running', '投递继续后回 running');
  eq(toggle.textContent, '暂停', '投递继续后文案');
  fire(stop);
  eq(state.autoApply, 'idle', '投递结束后回 idle');
  eq(toggle.textContent, '开始自动投递', '结束后主按钮回「开始自动投递」');
  eq(stop.hidden, true, '结束后「结束」消失（回单按钮态）');
  eq(toggle.classList.contains('btn-primary'), true, '结束后回主色');
  tri.push('投递 idle→running(暂停+结束)→paused(继续+结束)→idle');

  return { fails, tri };
}
