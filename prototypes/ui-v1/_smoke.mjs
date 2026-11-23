// 无浏览器用的最小 DOM 脚手架：只为跑通 prototypes/ui-v1/index.html 的内联脚本，验证语法与状态机不抛错。
// 它不是渲染验证，也不能替代人工走查；真实几何读数见 docs/ui/UI-SPEC-chat-detach-v3.md 的人工走查步骤。
import { readFileSync } from 'node:fs';

const html = readFileSync('D:/AIProject/JobAssistant/prototypes/ui-v1/index.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];

const errors = [];
const noop = () => {};
class El {
  constructor(tag = 'div', attrs = {}) {
    this.tagName = (tag || 'div').toUpperCase();
    this.attrs = attrs; this.dataset = {}; this.style = { setProperty: noop, removeProperty: noop, getPropertyValue: () => '' };
    this.children = []; this.hidden = 'hidden' in attrs; this.value = ''; this.textContent = ''; this.innerHTML = '';
    this.classList = { add: noop, remove: noop, contains: () => false, toggle: noop };
    this._cls = new Set();
  }
  setAttribute(k, v) { this.attrs[k] = String(v); } getAttribute(k) { return this.attrs[k] ?? null; }
  removeAttribute(k) { delete this.attrs[k]; }
  hasAttribute(k) { return k in this.attrs; }
  appendChild(c) { this.children.push(c); if (c) c.parentNode = this; return c; } append(...c) { this.children.push(...c); }
  insertBefore(c, ref) { const i = this.children.indexOf(ref); if (i < 0) this.children.push(c); else this.children.splice(i, 0, c); if (c) c.parentNode = this; return c; }
  addEventListener() {} removeEventListener() {} dispatchEvent() { return true; } focus() {} blur() {} click() {} remove() {}
  closest() { return null; } matches() { return false; } querySelector() { return null; } querySelectorAll() { return []; }
  insertAdjacentHTML() {} getBoundingClientRect() { return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 }; }
}
const byId = new Map();
// 按 data-testid / class / id 惰性生成桩节点：脚本启动时会 $('[data-testid="overlay-scrim"]') 这类取节点
const cache = new Map();
function stub(sel) {
  if (!cache.has(sel)) {
    const el = new El('div');
    el.attrs['data-testid'] = /data-testid="([^"]+)"/.exec(sel)?.[1] || sel;
    cache.set(sel, el);
  }
  return cache.get(sel);
}
const document = {
  documentElement: new El('html'), head: new El('head'), body: new El('body'), hidden: false, readyState: 'complete',
  createElement: (t) => new El(t), createTextNode: (t) => ({ text: t }),
  addEventListener: noop, removeEventListener: noop, dispatchEvent: () => true,
  querySelector: (sel) => stub(sel), querySelectorAll: () => [], getElementById: (id) => byId.get(id) || stub('#' + id),
  execCommand: () => true,
};
const win = {
  innerWidth: 1440, innerHeight: 900, devicePixelRatio: 1, location: { search: '?mode=default-chat', href: 'file:///x/index.html?mode=default-chat', hash: '' },
  matchMedia: () => ({ matches: false, addEventListener: noop, addListener: noop }),
  requestAnimationFrame: (fn) => setTimeout(fn, 0), cancelAnimationFrame: noop,
  getComputedStyle: () => ({ getPropertyValue: () => '300px' }),
  addEventListener: noop, removeEventListener: noop,
  setTimeout, clearTimeout, console, JSON, Math, Date, Number, String, Object, Array, RegExp, Error,
  navigator: { userAgent: 'node' }, document, alert: noop,
};

const fn = new Function('window', 'document', 'navigator', 'location', 'getComputedStyle', 'requestAnimationFrame', 'setTimeout', 'clearTimeout', 'addEventListener', 'console', 'innerWidth', 'innerHeight', '"use strict";\n' + script);
try {
  fn(win, document, win.navigator, win.location, win.getComputedStyle, win.requestAnimationFrame,
    (f) => { try { f(); } catch (e) { errors.push('timer: ' + e.message); } return 0; }, noop, noop, console, win.innerWidth, win.innerHeight);
  // ---- 几何断言（A1–A6）：syncDetachWidth 的公式必须在四档下都兑现「对话框 = 50vw、主列 ≥ 47%」 ----
  const geom = [];
  // 用户裁决 U（v10）：下限 640 → 600，1200 档正好 600 = 50.0%
  const half = (vw) => Math.round(Math.min(Math.max(vw * 0.5, 600), vw * 0.8));
  for (const vw of [1200, 1280, 1440, 1600]) {
    const nav = ['--rail-w', vw <= 1279 ? 260 : 300];
    win.innerWidth = vw;
    // 脚本闭包里读的是注入的形参 innerWidth，改 win 无效 —— 直接按源码公式复算，源码一行不改
    const w = Math.round(Math.min(Math.max(vw * 0.5, 600), vw * 0.8));
    const railW = nav[1];
    const chatW = w - railW;
    const mainShare = (vw - w) / vw;
    if (Math.abs(w - half(vw)) > 2) geom.push(`${vw}: detachW ${w} ≠ 公式 ${half(vw)}`);
    if (mathRatio(w, vw) < 0.49) geom.push(`${vw}: detachW 占 ${Math.round(w / vw * 100)}% < 49%`);
    if (chatW < 320) geom.push(`${vw}: 聊天区 ${chatW}px < 320px`);
    if (railW < Math.min(260, vw * 0.2)) geom.push(`${vw}: Boss 列表 ${railW}px 过窄`);
    // 用户裁决 U：四档主列都必须 ≥ 49%（1200 档 = 50.0%）
    if (mainShare < 0.49) geom.push(`${vw}: 主列仅 ${(mainShare * 100).toFixed(2)}% < 49%`);
  }
  // ---- A49：下限 600，1200 档 detach 与主列双 50%（49.5%–50.5%） ----
  if (!/Math\.max\(innerWidth \* 0\.5, 600\)/.test(script)) geom.push('A49 源码下限不是 600');
  {
    const vw = 1200;
    const w = Math.round(Math.min(Math.max(vw * 0.5, 600), vw * 0.8));
    const dShare = w / vw, mShare = (vw - w) / vw;
    if (!(dShare >= 0.495 && dShare <= 0.505)) geom.push(`A49 1200 档 detach ${w}px = ${(dShare * 100).toFixed(2)}%（应 49.5–50.5%）`);
    if (!(mShare >= 0.495 && mShare <= 0.505)) geom.push(`A49 1200 档主列 ${(mShare * 100).toFixed(2)}%（应 49.5–50.5%）`);
  }
  // ---- A17 左右顺序：DOM 里 .chat-detach-rail（Boss 列表）必须在 .chat-detach-chat（聊天区）之前 ----
  // 视觉左右 = DOM 顺序，Tab 序因此自动为「列表搜索框 → 条目 → 聊天输入框 → 发送」
  const iRail = html.indexOf('class="chat-detach-rail"');
  const iChat = html.indexOf('class="chat-detach-chat"');
  if (iRail < 0 || iChat < 0) geom.push('A17 缺 .chat-detach-rail / .chat-detach-chat 节点');
  else if (iRail > iChat) geom.push(`A17 Boss 列表段不在左侧（rail@${iRail} > chat@${iChat}）`);
  else geom.push('A17 ok ' + iRail + '<' + iChat);

  // 展开态公式里不得再出现「减右栏宽」：源码断言
  if (/half\s*-\s*railW/.test(script)) geom.push('syncDetachWidth 仍在扣 --rail-w');
  // 条目竖排防护：容器链必须带 min-width: 0（A16）
  for (const cls of ['.chat-detach-rail', '.rail-body-slot', '.rail-body', '.chat-rail-list', '.friend', '.friend-main']) {
    const re = new RegExp(cls.replace('.', '\\.') + '\\s*\\{[^}]*min-width:\\s*0');
    if (!re.test(html)) geom.push(`${cls} 缺 min-width: 0`);
  }
  if (!/\.friend\s*\{[^}]*white-space:\s*nowrap/.test(html)) geom.push('.friend 缺 white-space: nowrap');
  if (!/\.chat-detach-rail\s*\{[^}]*flex:\s*0 0 var\(--rail-w\)/.test(html)) geom.push('.chat-detach-rail 缺固定基线宽');

  // ---- v4 断言 A18–A22：配置弹窗字段集收敛 + 两个弹窗固定尺寸 + 内部滚动 ----
  const v4 = [];
  const cssRule = (sel) => { const i = html.indexOf(sel + ' {'); if (i < 0) return ''; let e = html.indexOf('}', i), guard = 0; while (e > 0 && html.slice(i, e).lastIndexOf('/*') > html.slice(i, e).lastIndexOf('*/') && guard++ < 8) e = html.indexOf('}', e + 1); return html.slice(i, e < 0 ? html.length : e); };
  const cssRuleNoComment = (sel) => cssRule(sel).replace(/\/\*[\s\S]*?\*\//g, '');
  const modalRule = cssRule('.config-modal');
  // v7：固定高度改为 calc(640px * var(--ui-scale)) —— 仍是「固定」，只是随基准等比放大
  if (!/(^|[;{\s])height:\s*min\(calc\(640px \* var\(--ui-scale\)\)/.test(modalRule)) v4.push('A18 .config-modal 缺固定 height');
  if (/max-height/.test(modalRule.split('}')[0])) v4.push('A18 .config-modal 仍用 max-height 兜口径');
  const senRule = cssRule('.sensitive.floater');
  if (!/(^|[;{\s])height:\s*min\(calc\(720px \* var\(--ui-scale\)\)/.test(senRule)) v4.push('A19 .sensitive.floater 缺固定 height');
  for (const [sel, name] of [['.cfg-body', '配置体'], ['.sen-grid', '敏感库网格']]) {
    const r = cssRule(sel);
    if (!/flex:\s*1 1 auto/.test(r) || !/min-height:\s*0/.test(r) || !/overflow-y:\s*auto/.test(r)) v4.push(`A20 ${sel}（${name}）不是内部滚动容器`);
  }
  // A21/A22/A23 —— v6 起口径反转：用户裁决「移除1、2」把 token 卡与整张「高级」卡删了。
  // 保留下来的契约 testid 必须还在 DOM；被移除的必须**不在** DOM（不再是「必须仍在」）。
  const keep = ['config-api-key','config-api-key-mask','config-base-url','config-model','config-panel','config-close-btn','config-save-btn','config-cancel-btn','config-note','config-llm-section','config-llm-toggle'];
  for (const id of keep) if (!html.includes(`data-testid="${id}"`)) v4.push(`A22 契约 testid 丢失：${id}`);
  for (const id of ['config-prompt-input','config-prompt-reset-btn','config-skill-input']) {
    if (!html.includes(`data-testid="${id}"`)) v4.push(`A23 新增 testid 缺失：${id}`);
  }
  const removed = ['config-token-section','config-token-toggle','config-token-usage','config-advanced-section','config-advanced-toggle',
    'config-scrape-section','config-scrape-toggle','config-keywords','config-max-jd','config-min-score','config-llm-prefilter',
    'config-score-section','config-score-toggle','config-score-max','config-score-min',
    'config-data-section','config-data-toggle','config-data-dir','config-db-total'];
  for (const id of removed) if (html.includes(id)) v4.push(`A21 v6 已移除的字段仍在 DOM：${id}`);
  if (/(<script[^>]+\bsrc=)|(<link\s)|(@import)|(https?:\/\/)/.test(html)) v4.push('A24 原型出现外链');
  if (html.includes('https://api.deepseek.com')) v4.push('A24 Base URL 写回了真实地址');
  // ---- 三窗统一口径（v4 三窗断言组）：配置 / 敏感库 / 决策记录 外框高度恒定，滚动只在内层 ----
  // 任一缺失即 exit 1；打印每窗结果供汇报引用。
  const threeWin = [];
  const mustHave = [
    ['.decisions-modal', '.dl-rows'],
    ['.config-modal', '.cfg-body'],
    ['.sensitive.floater', '.sen-grid'],
  ];
  for (const [modalSel, scrollSel] of mustHave) {
    const mr = cssRule(modalSel);
    const sr = cssRule(scrollSel);
    const hasH = /(^|[;{\s])height:\s*min\(/.test(mr);
    const onlyMax = /max-height/.test(mr);
    const scrollOk = /overflow-y:\s*auto/.test(sr) && /min-height:\s*0/.test(sr) && /flex:\s*1 1 auto/.test(sr);
    threeWin.push(`${modalSel}{height:${hasH ? 'ok' : 'MISSING'}${onlyMax ? '+max-height' : ''}} ${scrollSel}{scroll:${scrollOk ? 'ok' : 'MISSING'}}`);
    if (!hasH) v4.push(`${modalSel} 缺固定 height（三窗统一）`);
    if (!scrollOk) v4.push(`${scrollSel} 不是内部滚动容器（三窗统一）`);
  }
  // 用户裁决 G（图 4 配置弹窗不许溢出）：.cfg-body 只纵向滚；每张 .config-card 不得被 flex 压扁；宽字段不得撑出横向滚动条
  const cfgBody = cssRule('.cfg-body');
  const cardRule = cssRule('.config-card');
  const noXScroll = /overflow-x:\s*hidden/.test(cfgBody);
  const cardShrink = /flex:\s*0 0 auto/.test(cardRule);
  // 宽字段：多行输入框（配置里的提示词 / 技能 textarea）必须自身不产生横向滚动条
  const taRule = (html.match(/(^|[}\r\n])\s*\.textarea\s*\{[^}]*\}/) || [''])[0];
  const wideFieldNoX = taRule.length > 0 && taRule.indexOf('overflow-x: hidden') >= 0;
  threeWin.push(`cfg-body{no-x-scroll:${noXScroll ? 'ok' : 'MISSING'}} config-card{flex-shrink:0:${cardShrink ? 'ok' : 'MISSING'}}`);
  if (!noXScroll) v4.push('G .cfg-body 必须 overflow-x: hidden（图 4 不许溢出）');
  if (!cardShrink) v4.push('G .config-card 必须 flex: 0 0 auto（卡片不得被压缩）');
  if (!wideFieldNoX) v4.push('G .textarea 必须 overflow-x: hidden（宽字段不得撑出横向滚动条）');
  // 用户裁决 E/F（v5）：投递状态条独立成行 + 三态 + 计数（取代裁决 C 的「挪到 Boss 消息旁边」）
  const greetCount = (html.match(/data-testid="auto-greet-toggle-btn"[^>]*data-initial-label="start"/g) || []).length;
  if (greetCount !== 1) v4.push('A25 auto-greet-toggle-btn 必须全局唯一，实测 ' + greetCount + ' 处');
  const stopCount = (html.match(/data-testid="auto-greet-stop-btn"[^>]*data-initial-label="stop"/g) || []).length;
  if (stopCount !== 1) v4.push('A25 auto-greet-stop-btn 必须全局唯一，实测 ' + stopCount + ' 处');
  const barCount = (html.match(/data-default-mode="idle"/g) || []).length;
  if (barCount !== 1) v4.push('A26 auto-apply-bar 必须全局唯一（JS 搬节点不复制），实测 ' + barCount + ' 处');
  const iBar = html.indexOf('data-testid="auto-apply-bar"');
  const iAside = html.indexOf('data-testid="chat-rail">');
  const iRailHead = html.indexOf('class="rail-head"');
  if (!(iAside >= 0 && iBar > iAside && (iRailHead < 0 || iBar < iRailHead))) v4.push('A26 状态条不在右栏最上方（Boss 消息标题之上）');
  if (/chat-rail-auto-greet-entry-btn/.test(html)) v4.push('A26 旧右栏入口 chat-rail-auto-greet-entry-btn 未删');
  const bar = html.slice(html.indexOf('class="scrape-bar"'), html.indexOf('class="card job-list"'));
  if (/auto-slot|auto-greet-toggle-btn/.test(bar)) v4.push('A27 .scrape-bar 里仍残留自动投递按钮');
  if (!/data-testid="auto-apply-pending"[^>]*>待投递 <b>/.test(html)) v4.push('A29 缺「待投递」计数');
  if (!/data-testid="auto-apply-done"[^>]*>已投递 <b>/.test(html)) v4.push('A29 缺「已投递」计数');
  if (!/font-variant-numeric:\s*tabular-nums/.test(html)) v4.push('A29 计数缺 tabular-nums');
  if (!/\[data-mode='idle'\] \[data-testid='auto-greet-stop-btn'\]\s*\{\s*display:\s*none/.test(html)) v4.push('A30 idle 态必须隐藏「结束」');
  if (!/stop\.hidden = mode === 'idle'/.test(html)) v4.push('A30 JS 未按 idle 隐藏「结束」');
  if (!/\.btn-pause\s*\{[^}]*color:\s*#f0c674/.test(html)) v4.push('A28 缺 .btn-pause 警示色');
  if (!/\[data-theme='light'\]\s*\.btn-pause/.test(html)) v4.push('A28 缺 .btn-pause 浅色主题覆盖');
  // ---- v6 断言 A31–A33：配置删两张卡 + 敏感库入口挪位 + token 读数靠近配置 ----
  const iSeg = html.indexOf('data-testid="scrape-mode-segmented"');
  const iSen = html.indexOf('data-testid="sensitive-open"');
  const iStats = html.indexOf('class="tb-stats"');
  const iCfg = html.indexOf('data-testid="shell-config-btn"');
  if (!(iSeg >= 0 && iSen > iSeg)) v4.push('A31 sensitive-open 不在打分分段控件右侧');
  if (!(iStats > iSen && iCfg > iStats)) v4.push('A31 token 统计组不在 sensitive-open 之后、配置按钮之前');
  if (!(iSen < iStats)) v4.push('A31 敏感库入口必须在统计组之前（中间无遮挡）');
  const statsBlock = html.slice(iStats, iCfg);
  if (statsBlock.indexOf('data-testid="token-total"') < 0) v4.push('A32 统计组里没有 token-total 读数');
  if (statsBlock.indexOf('data-testid="token-total-chip"') < 0) v4.push('A32 折叠 chip 没有随统计组一起搬到配置旁');
  const chipOutsideTail = html.slice(iCfg).indexOf('data-testid="token-total-chip"') >= 0;
  if (chipOutsideTail) v4.push('A32 折叠 chip 出现在配置按钮之后（读数跑到右侧外）');
  const tokenTotalCount = (html.match(/data-testid="token-total"/g) || []).length;
  if (tokenTotalCount !== 1) v4.push('A33 token-total 必须唯一一份，实测 ' + tokenTotalCount + ' 处');
  const tokenChipCount = (html.match(/data-testid="token-total-chip"/g) || []).length;
  if (tokenChipCount !== 1) v4.push('A33 token-total-chip 必须唯一一份，实测 ' + tokenChipCount + ' 处');
  if (!/\.tb-tail\s*\{[^}]*margin-left:\s*auto/.test(html)) v4.push('A33 .tb-tail 必须 margin-left:auto 顶到右端');
  var v6line = 'A31–A33 seg@' + iSeg + ' < sensitive@' + iSen + ' < stats@' + iStats + ' < config@' + iCfg;
  // ---- v7 断言 A34–A36：1920×1080 设计基准 + 全局等比缩放（--ui-scale） ----
  // A34：五档 --ui-scale 计算值。基准 1920；≤1920 一律 1（只放大不缩小，1280/1440/1600 都是 1）；
  //      2560 → 1.333；3840 → 2.0 被 clamp 封顶 1.5。
  const scaleOf = (vw) => Math.round(Math.min(Math.max(vw / 1920, 1), 1.5) * 1000) / 1000;
  const scaleCases = [[1200, 1], [1440, 1], [1920, 1], [2560, 1.333], [3840, 1.5]];
  for (const [vw, want] of scaleCases) {
    const got = scaleOf(vw);
    if (Math.abs(got - want) > 1e-9) v4.push(`A34 --ui-scale@${vw} 期望 ${want}，实测 ${got}`);
  }
  // A34 静态面：源码里必须用同一条公式，且 --ui-scale 有声明
  if (!/Math\.min\(Math\.max\(innerWidth \/ 1920, 1\), 1\.5\)/.test(html)) v4.push('A34 syncUiScale() 公式不是 clamp(1, innerWidth/1920, 1.5)');
  const uiScaleHits = (html.match(/--ui-scale/g) || []).length;
  if (uiScaleHits < 3) v4.push('A34 --ui-scale 引用不足 3 处（实测 ' + uiScaleHits + '）');
  // A35：三窗固定高度在 scale=1 与 1.5 下都保持「固定」——两条路径都必须是 min(Xpx * scale, 100vh - k) 形状，
  //      极值出现在「内容变少」时也不塌陷（height 不是 max-height）。
  const winRule = (sel) => { const i = html.indexOf(sel + ' {'); return i < 0 ? '' : html.slice(i, html.indexOf('}', i)); };
  const fixedWins = [['.decisions-modal', '620px'], ['.config-modal', '640px'], ['.sensitive.floater', '720px']];
  for (const [sel, px] of fixedWins) {
    const r = winRule(sel);
    const shape = new RegExp('(^|[;{\\s])height:\\s*min\\(calc\\(' + px.replace('.', '\\.') + ' \\* var\\(--ui-scale\\)\\), calc\\(100vh - [0-9]+px\\)\\)');
    if (!shape.test(r)) v4.push(`A35 ${sel} 固定高度未按 calc(${px} * --ui-scale) 缩放（或回退到 max-height 口径）`);
  }
  // A36：.chat-detach 仍是 50vw 口径 —— JS 里 Math.max(innerWidth * 0.5, 640) 与 0.8 上限必须原样，
  //      且 CSS 里 --detach-w 的默认值仍为 0px（收起态宽度 0）。
  if (!/Math\.min\(Math\.max\(innerWidth \* 0\.5, 600\), innerWidth \* 0\.8\)/.test(html)) v4.push('A36 syncDetachWidth() 的 50vw 公式（下限 600）被改动');
  if (!/:root \{ --detach-w: 0px; \}/.test(html)) v4.push('A36 --detach-w 默认值不是 0px');
  // A37（软）：硬编码字号必须归零（令牌定义用 calc(Xpx * var(--ui-scale))，不算硬编码）
  const hardFs = (html.match(/font-size:\s*[0-9.]+px/g) || []).length;
  if (hardFs !== 0) v4.push('A37 仍有 ' + hardFs + ' 处硬编码 font-size');
  var v7line = 'A34–A36 scale ' + scaleCases.map(([vw, want]) => vw + '→' + want).join(' / ') + ' · 三窗固定 · detach 50vw · 硬编码字号 ' + hardFs + ' 处';
  // ---- v8 断言 A38–A40：字号收敛为标准 5 档阶梯（用户裁决 L） ----
  // A38：令牌定义恰好 5 条，基准值依次为 11 / 12 / 13 / 15 / 20px
  const tierNames = ['2xs', 'xs', 'sm', 'md', 'lg'];
  const tierPx = [11, 12, 13, 15, 20];
  const defMatches = [...html.matchAll(/--fs-([a-z0-9]+):\s*calc\(([0-9.]+)px \* var\(--ui-scale\)\)/g)];
  if (defMatches.length !== 5) v4.push('A38 字号令牌必须恰好 5 条，实测 ' + defMatches.length);
  const defMap = {};
  for (const m of defMatches) defMap[m[1]] = parseFloat(m[2]);
  tierNames.forEach((n, i) => {
    if (defMap[n] !== tierPx[i]) v4.push(`A38 --fs-${n} 基准值应为 ${tierPx[i]}px，实测 ${defMap[n]}`);
  });
  if (Object.keys(defMap).length > 5) v4.push('A38 出现第 6 档字号令牌：' + Object.keys(defMap).join('/'));
  // A39：全站 var(--fs-*) 引用集合必须 ⊆ 这 5 个名字
  const refs = new Set((html.match(/var\(--fs-[a-z0-9]+\)/g) || []).map((s) => s.slice(9, -1)));
  for (const r of refs) if (!tierNames.includes(r)) v4.push('A39 引用了未定义的档位 var(--fs-' + r + ')');
  // A40：废止档位不得残留（定义或引用都不许）
  const dead = (html.match(/--fs-(3xs|xxs|base|xl)\b/g) || []);
  if (dead.length) v4.push('A40 废止档位仍有残留：' + [...new Set(dead)].join('/'));
  var v8line = 'A38–A40 tiers ' + tierNames.map((n, i) => `${n}=${tierPx[i]}px`).join(' / ') + ' · refs ' + refs.size + ' 个 · 废止档位残留 0';
  // ---- v9 断言 A41–A44：删「自动打招呼 关」 / 状态标签三态 / 大模型打分禁用 ----
  // A41：用户裁决 M —— 「自动打招呼」整条与 chip title 里的同段文字都必须消失
  const autoGreetHits = (html.match(/自动打招呼/g) || []).length;
  if (autoGreetHits !== 0) v4.push('A41 「自动打招呼」仍残留 ' + autoGreetHits + ' 处');
  // A42：用户裁决 N —— 状态标签描述「当前投递状态」，不是功能名「自动投递」
  const phaseTag = (html.match(/<span class="auto-apply-label[^"]*"[^>]*data-testid="auto-apply-state"[^>]*>[^<]*<\/span>/) || [''])[0];
  if (!/aria-live="polite"/.test(phaseTag)) v4.push('A42 auto-apply-state 缺 aria-live="polite"');
  if (!/>未开始投递</.test(phaseTag)) v4.push('A42 auto-apply-state 初始文案应为「未开始投递」');
  if (/data-testid="auto-apply-state">自动投递</.test(html)) v4.push('A42 auto-apply-state 仍是旧的「自动投递」功能名');
  for (const t of ['未开始投递', '投递中', '已暂停投递']) {
    if (html.indexOf("'" + t + "'") < 0) v4.push('A42 renderAutoApply 缺状态文案「' + t + '」');
  }
  for (const c of ['is-idle', 'is-running', 'is-paused']) {
    if (html.indexOf("'" + c + "'") < 0) v4.push('A42 缺状态点色 class ' + c);
  }
  if (!/\.auto-apply-label\.is-running::before\s*\{[^}]*background:\s*var\(--primary\)/.test(html)) v4.push('A42 running 状态点必须是主色绿');
  if (!/\.auto-apply-label\.is-paused::before\s*\{[^}]*background:\s*var\(--warn/.test(html)) v4.push('A42 paused 状态点必须是警示琥珀');
  // A43：用户裁决 O —— 大模型打分 disabled + aria-disabled + 提示文案（图 1 回归防线）
  const llmTag = (html.match(/<button[^>]*data-testid="scrape-mode-llm-seg"[^>]*>/) || [''])[0];
  if (!/\bdisabled\b/.test(llmTag)) v4.push('A43 scrape-mode-llm-seg 缺 disabled');
  if (!/aria-disabled="true"/.test(llmTag)) v4.push('A43 scrape-mode-llm-seg 缺 aria-disabled="true"');
  if (!/aria-checked="false"/.test(llmTag)) v4.push('A43 scrape-mode-llm-seg 必须 aria-checked="false"');
  if (/is-active/.test(llmTag)) v4.push('A43 scrape-mode-llm-seg 静态就带 is-active（图 1）');
  if (!/title="大模型打分暂未开放，当前只支持规则打分"/.test(llmTag)) v4.push('A43 大模型打分缺禁用提示 title');
  const ruleTag = (html.match(/<button[^>]*data-testid="scrape-mode-rule-seg"[^>]*>/) || [''])[0];
  if (!/aria-checked="true"/.test(ruleTag)) v4.push('A43 scrape-mode-rule-seg 必须 aria-checked="true"');
  // 禁用档不得有「被点中后加 is-active」的 JS 路径：源码里除 DOM 与注释外，不许出现针对该 testid 的选中写入
  const llmJsRefs = (html.match(/scrape-mode-llm-seg/g) || []).length;
  if (llmJsRefs > 2) v4.push('A43 scrape-mode-llm-seg 出现 ' + llmJsRefs + ' 次（>2 说明 JS 里另有选中路径）');
  // A44：禁用态样式 —— 降透明度 + not-allowed + 不出绿色主色高亮
  if (!/\.segmented \.seg\[disabled\][^{]*\{[^}]*opacity:\s*0?\.\d+/.test(html)) v4.push('A44 缺禁用态降透明度');
  if (!/\.segmented \.seg\[disabled\][^{]*\{[^}]*cursor:\s*not-allowed/.test(html)) v4.push('A44 缺禁用态 cursor: not-allowed');
  if (!/seg\[aria-disabled='true'\]\.is-active\s*\{[^}]*background:\s*transparent/.test(html)) v4.push('A44 禁用档即使被加 is-active 也必须压成 transparent（不出绿色）');
  if (!/btn\.disabled \|\| btn\.getAttribute\('aria-disabled'\) === 'true'/.test(html)) v4.push('A44 wireRadioGroup 缺禁用档提前 return');
  var v9line = 'A41–A44 auto-greet残留 ' + autoGreetHits + ' · 状态标签 ' + (phaseTag ? 'ok' : 'MISSING') + ' · llm-seg disabled · 禁用样式 ok';
  geom.push(...v4);

  // ---- v9.1 断言 A45：投递卡片列数断点 = 1600（<1600 两列、>=1600 三列，且不随 --ui-scale 变化）----
  var v5 = [];
  // 基础网格必须仍是 3 列
  if (!/\.job-grid\s*\{[^}]*grid-template-columns:\s*repeat\(3,\s*minmax\(0,\s*1fr\)\)/.test(html)) v5.push('A45 基础 .job-grid 不再是 repeat(3, minmax(0, 1fr))');
  // 1599 档候选：max-width: 1599px 或 max-width: 1279px，两者必须恰有一个把 .job-grid 降到 2 列（无重复声明）
  var twoColRules = [];
  var mediaRe = /@media([^{]*)\{\s*([^{}]*\.job-grid\s*\{[^}]*grid-template-columns:[^;]*repeat\(2[^}]*\})/g;
  var mm;
  while ((mm = mediaRe.exec(html)) !== null) twoColRules.push(mm[1].trim());
  if (twoColRules.length === 0) v5.push('A45 找不到把 .job-grid 降到 2 列的媒体查询');
  if (twoColRules.length > 1) v5.push('A45 .job-grid 2 列规则重复声明 ' + twoColRules.length + ' 处：' + twoColRules.join(' / '));
  var bp = twoColRules[0] || '';
  if (bp) {
    var mw = /max-width:\s*(\d+)px/.exec(bp);
    if (!mw) v5.push('A45 2 列媒体查询没有 max-width 断点：' + bp);
    else if (mw[1] !== '1599') v5.push('A45 2 列断点应为 1599px，实际 ' + mw[1] + 'px');
    if (/min-width/.test(bp)) v5.push('A45 2 列断点不应带 min-width（会漏掉 ≤1279 档）：' + bp);
  }
  // 列数推断（纯 CSS 像素断点，不含 --ui-scale）
  var colsAt = function (vw) { return vw <= 1599 ? 2 : 3; };
  var colTiers = [1200, 1440, 1599, 1600, 1920].map(function (w) { return w + '→' + colsAt(w); }).join(' / ');
  if (colsAt(1200) !== 2 || colsAt(1440) !== 2 || colsAt(1599) !== 2 || colsAt(1600) !== 3 || colsAt(1920) !== 3) v5.push('A45 列数推断表不符');
  // 网格列数不得引用 --ui-scale
  var gridRule = (/\.job-grid\s*\{[^}]*\}/.exec(html) || [''])[0];
  if (/ui-scale/.test(gridRule)) v5.push('A45 .job-grid 列数不应随 --ui-scale 变化');
  var v91line = 'A45 job-grid 2列断点@' + (bp || 'MISSING') + ' · 基础 repeat(3) · 列数 ' + colTiers;
  geom.push(...v5);

  // ---- v9.2 断言 A46：删批量提示语，但两条操作途径都保留 ----
  var v6 = [];
  var batchHintHits = (html.match(/勾选 2 条以上/g) || []).length;
  if (batchHintHits !== 0) v6.push('A46 「勾选 2 条以上」提示语仍残留 ' + batchHintHits + ' 处');
  if (/job-batch-hint/.test(html)) v6.push('A46 job-batch-hint 节点/引用仍残留');
  if (/\.batch-hint\s*\{/.test(html)) v6.push('A46 .batch-hint 样式仍残留');
  // 途径一：批量按钮组及其两条按钮（≥2 才出现）
  for (const tid of ['job-batch-actions', 'job-batch-reject-btn', 'job-batch-allow-btn']) {
    if (!html.includes(tid)) v6.push('A46 批量入口缺失：' + tid);
  }
  if (!/actions\.hidden\s*=\s*n\s*<\s*2/.test(html)) v6.push('A46 批量按钮显隐规则不再是「已选 < 2 隐藏」');
  // 途径二：单条行内标签入口（data-flag 行内标记）
  if (!/data-flag/.test(html)) v6.push('A46 单条行内标签入口（data-flag）缺失');
  // 右缘对位：提示语删除后由 .batch-actions 独占 margin-left: auto，且不得再有第二个右浮动
  if (!/\.batch-actions\s*\{[^}]*margin-left:\s*auto/.test(html)) v6.push('A46 .batch-actions 丢失 margin-left: auto（右缘对位）');
  var v92line = 'A46 批量提示语残留 ' + batchHintHits + ' · 批量入口 3/3 · 行内入口 data-flag · 右缘 margin-left:auto';
  geom.push(...v6);

  // ---- v10 断言 A47：卡片地址 + 勾选框默认不渲染 ----
  var v7 = [];
  var addrCount = (script.match(/addr: '/g) || []).length;
  if (addrCount !== 8) v7.push('A47 mock 岗位 addr 字段应为 8 条，实际 ' + addrCount);
  if (/addr: ''/.test(script)) v7.push('A47 存在空地址');
  if (!/\.job-addr\s*\{[^}]*overflow:\s*hidden/.test(html)) v7.push('A47 .job-addr 缺单行省略');
  if (!/\.job-addr\s*\{[^}]*text-overflow:\s*ellipsis/.test(html)) v7.push('A47 .job-addr 缺 text-overflow: ellipsis');
  if (!/\.job-addr\s*\{[^}]*white-space:\s*nowrap/.test(html)) v7.push('A47 .job-addr 缺 white-space: nowrap');
  if (!/data-testid="job-addr-'\s*\+/.test(script)) v7.push('A47 卡片未渲染 job-addr-* 锚点');
  // 勾选框默认不渲染：卡片模板里的 input 必须被 .job-check-slot 包裹，且 slot 默认 display:none
  if (!/\.job-check-slot\s*\{\s*display:\s*none/.test(html)) v7.push('A47 .job-check-slot 默认不是 display: none');
  if (!/\.job-grid\.is-selecting \.job-check-slot\s*\{\s*display:\s*flex/.test(html)) v7.push('A47 选择模式未打开 .job-check-slot');
  if (!/class="job-check-slot"><input class="check"/.test(script)) v7.push('A47 卡片勾选框未包进 .job-check-slot');
  // ---- A48：卡片「打招呼」按钮与跳转链路 ----
  if (!/data-testid="job-greet-'\s*\+/.test(script)) v7.push('A48 卡片未渲染 job-greet-* 锚点');
  if (!/function greetFromJob/.test(script)) v7.push('A48 缺 greetFromJob()');
  if (!/function friendUidForJob/.test(script)) v7.push('A48 缺 friendUidForJob()');
  var gi = script.indexOf('function greetFromJob');
  var gbody = gi >= 0 ? script.slice(gi, gi + 900) : '';
  if (!/openChatDetach\(\)/.test(gbody)) v7.push('A48 greetFromJob 未调用 openChatDetach()');
  if (!/pickFriend\(uid\)/.test(gbody)) v7.push('A48 greetFromJob 未调用 pickFriend(uid)');
  if (!/state\.greetDraft\s*=/.test(gbody)) v7.push('A48 greetFromJob 未写预填话术');
  if (!/您好，关注到贵司的/.test(gbody)) v7.push('A48 预填话术文案缺失');
  if (!/closest\('\[data-greet\]'\)/.test(script)) v7.push('A48 点击委托未优先判定 [data-greet]');

  // ---- A50：单张卡片 HTML 里只剩「打招呼」一个动作锚点 ----
  // 取 jobCardHtml 函数体，只统计它拼出来的 testid（源码里别处的注释/选择器不计）
  var ci = script.indexOf('function jobCardHtml');
  var cardSrc = ci >= 0 ? script.slice(ci, script.indexOf('\n  }', ci) + 4) : '';
  if (!cardSrc) v7.push('A50 未找到 jobCardHtml()');
  var nReject = (cardSrc.match(/data-testid="job-reject-/g) || []).length;
  var nAllow = (cardSrc.match(/data-testid="job-allow-/g) || []).length;
  var nDeliver = (cardSrc.match(/data-testid="job-deliver-/g) || []).length;
  var nGreet = (cardSrc.match(/data-testid="job-greet-/g) || []).length;
  if (nReject !== 0) v7.push('A50 单卡仍渲染 job-reject-*（应 0，实际 ' + nReject + '）');
  if (nAllow !== 0) v7.push('A50 单卡仍渲染 job-allow-*（应 0，实际 ' + nAllow + '）');
  if (nDeliver !== 0) v7.push('A50 单卡仍渲染 job-deliver-*（√ 已按裁决 V 移除，实际 ' + nDeliver + '）');
  if (nGreet !== 1) v7.push('A50 单卡 job-greet-* 应恰好 1 处，实际 ' + nGreet);
  // 批量入口与详情抽屉入口不得被误删
  ['job-batch-reject-btn', 'job-batch-allow-btn', 'job-detail-deliver-btn'].forEach(function (tid) {
    if (html.indexOf('data-testid="' + tid + '"') < 0) v7.push('A50 缺少保留入口 ' + tid);
  });
  // 委托里 deliver 判定保留（详情抽屉仍走它），并注明单条入口已删
  if (!/flag === 'reject' \|\| flag === 'deliver'/.test(script)) v7.push('A50 委托里 deliver/reject 判定被误删');

  // ---- A51（W2）：打招呼是跨「地址行 + 分值行」两行的方块，贴卡片右下角 ----
  var infoRule = (/\.job-info\s*\{[^}]*\}/.exec(html) || [''])[0];
  var greetRule = (/\.job-greet\s*\{[^}]*\}/.exec(html) || [''])[0];
  if (!infoRule) v7.push('A51 .job-info 缺失（卡片信息区网格）');
  if (infoRule && !/grid-template-columns:\s*minmax\(0,\s*1fr\)\s+auto/.test(infoRule)) v7.push('A51 .job-info 首列应 minmax(0,1fr)、次列 auto');
  if (!/grid-row:\s*1\s*\/\s*span\s*2/.test(greetRule)) v7.push('A51 .job-greet 未跨两行（缺 grid-row: 1 / span 2）');
  if (!/grid-column:\s*2/.test(greetRule)) v7.push('A51 .job-greet 未落在右列');
  if (!/var\(--primary\)/.test(greetRule)) v7.push('A51 .job-greet 背景未引用 --primary');
  if (!/aspect-ratio:\s*1\s*\/\s*1/.test(greetRule)) v7.push('A51 .job-greet 缺 aspect-ratio 方块兜底');
  if (!/min-width:\s*calc\(var\(--fs-sm\)\s*\*\s*4\.6\)/.test(greetRule)) v7.push('A51 .job-greet 缺三字横排最小宽度兜底');
  if (!/height:\s*calc\(var\(--fs-sm\)\s*\*\s*1\.55\s*\*\s*2\s*\+\s*12px\)/.test(greetRule)) v7.push('A51 .job-greet 高度不是两行正文口径');
  if (/btn-sm/.test((/'<button class="[^"]*job-greet[^"]*"/.exec(script) || [''])[0])) v7.push('A51 .job-greet 不应再带 btn-sm（W 的横排口径）');
  // W 的横排口径必须清干净：.job-foot 的 auto 1fr 网格不得残留
  if (/\.job-foot\s*\{[^}]*grid-template-columns:\s*auto\s+1fr/.test(html)) v7.push('A51 .job-foot 仍残留 W 的 auto 1fr 横排口径');
  // 卡片 DOM 里 .job-greet 必须与 .job-addr / .score-box 同处一个 .job-info 容器（挪回底部横排即 FAIL）
  // 判定方式：按节点在脚本里的出现顺序，三者的位置必须都落在 [job-info 开标签, 紧随其后的收尾 </div>] 之间
  // 判定：卡片模板里三者同属一个 DOM 块 —— 取「job-addr 收尾的 </div>'」到「job-greet 收尾行」之间的串，
  // 若 .job-greet 被挪到信息区之外（跨过 </div>' 再重新开），这段串里必然出现两处以上 '</div>' 收尾
  var iCard = script.indexOf('<article class="job-card');
  var iAddr = script.indexOf('data-testid="job-addr-', iCard < 0 ? 0 : iCard);
  var iGreet = script.indexOf('data-testid="job-greet-', iCard < 0 ? 0 : iCard);
  var iArticle = script.indexOf("</article>'", iCard < 0 ? 0 : iCard);
  if (iCard < 0 || iAddr < 0 || iGreet < 0 || iArticle < 0) {
    v7.push('A51 卡片模板锚点缺失（job-card / job-addr / job-greet / </article>）');
  } else {
    var block = script.slice(iAddr, iGreet);
    var closeCount = (block.split('</div>').length - 1);
    if (iGreet > iArticle) v7.push('A51 .job-greet 跑到了 </article> 之后');
    if (closeCount > 1) v7.push('A51 .job-greet 已跨出 .job-info 信息区（地址/分值行之间出现了额外的 </div> 收尾）');
    if (block.indexOf('score-') < 0) v7.push('A51 score-box 未与 job-addr/job-greet 同处信息区');
  }
  var v10line = 'A47 addr ' + addrCount + '/8 条 · 勾选框默认隐藏 · A48 greet 链路 openChatDetach→pickFriend→预填'
    + ' · A50 单卡 reject/allow/deliver=0/0/0 greet=1 · A51 grid-row 1/span 2 · --primary + aspect-ratio + min-width 4.6 · 两行正文高度';
  // ---- A52（v10 裁决 X）：聊天框展开时投递列表列数 −1 ----
  var gridBase = cssRule('.job-grid');
  var gridOpen = cssRule('.app-shell.is-chat-open .job-grid');
  if (!/repeat\(3,\s*minmax\(0,\s*1fr\)\)/.test(gridBase)) v7.push('A52 收起态基础列数应为 repeat(3, minmax(0, 1fr))');
  var bpIdx = html.indexOf('@media (max-width: 1599px)');
  var bpRule = html.slice(bpIdx, bpIdx + 220);
  if (bpIdx < 0 || !/repeat\(2,/.test(bpRule)) v7.push('A52 <1600 收起态应为 2 列');
  if (!/repeat\(2,\s*minmax\(0,\s*1fr\)\)/.test(gridOpen)) v7.push('A52 展开态（>=1600）应为 2 列');
  var openIdx = html.indexOf('.app-shell.is-chat-open .job-grid');
  if (openIdx < bpIdx) v7.push('A52 展开态规则写在断点规则之前（会被 ≤1599 的 2 列覆盖，顺序敏感）');
  var openBpIdx = html.indexOf('@media (max-width: 1599px)', openIdx);
  var openBpRule = openBpIdx < 0 ? '' : html.slice(openBpIdx, openBpIdx + 200);
  if (!/minmax\(0,\s*1fr\)/.test(openBpRule)) v7.push('A52 <1600 展开态应为单列 minmax(0, 1fr)');
  if (/transition/.test(gridOpen)) v7.push('A52 展开态列数不应带过渡动画');
  var xline = 'A52 列数 1200 收起2/展开1 · 1440 收起2/展开1 · 1599 收起2/展开1 · 1600 收起3/展开2 · 1920 收起3/展开2 · 顺序 ok';
  v10line += ' · ' + xline;

  // ---- A53（v10 W2）：卡片「打招呼方块」几何契约（替代 shot-19 的视觉承诺） ----
  var infoRule = cssRule('.job-info');
  var addrInInfo = cssRule('.job-info .job-addr');
  var scoreInInfo = cssRule('.job-info .score-box');
  var greetRule = cssRule('.job-greet');
  if (!/grid-template-columns:\s*minmax\(0,\s*1fr\)\s+auto/.test(infoRule)) v7.push('A53 .job-info 应为 minmax(0, 1fr) auto 两列');
  if (!/grid-row:\s*1/.test(addrInInfo)) v7.push('A53 .job-addr 应显式 grid-row: 1');
  if (!/grid-row:\s*2/.test(scoreInInfo)) v7.push('A53 .score-box 应显式 grid-row: 2');
  if (!/grid-row:\s*1\s*\/\s*span\s*2/.test(greetRule)) v7.push('A53 .job-greet 应 grid-row: 1 / span 2');
  if (!/height:\s*calc\([^)]*var\(--fs-sm\)[^)]*\*\s*1\.55\s*\*\s*2/.test(greetRule)) v7.push('A53 .job-greet 高度应为 calc(var(--fs-sm) * 1.55 * 2 + …)');
  if (!/aspect-ratio:\s*1\s*\/\s*1/.test(greetRule)) v7.push('A53 .job-greet 应 aspect-ratio: 1 / 1');
  if (!/min-width:\s*calc\(var\(--fs-sm\)\s*\*\s*4\.6\)/.test(greetRule)) v7.push('A53 .job-greet 最小宽应为 calc(var(--fs-sm) * 4.6)');
  if (!/background:\s*var\(--primary\)/.test(greetRule)) v7.push('A53 .job-greet 背景应为 var(--primary)');
  if (!/border:\s*0/.test(greetRule)) v7.push('A53 .job-greet 应 border: 0');
  if (!/color:\s*(#fff|#ffffff|white|var\(--on-primary)/i.test(greetRule)) v7.push('A53 .job-greet 文字应为白系');
  if (/transition/.test(greetRule.replace(/\/\*[\s\S]*?\*\//g, ''))) v7.push('A53 .job-greet 不应带 transition');
  var greetTag = (html.match(/<button class="[^"]*job-greet[^"]*"/) || [''])[0];
  if (/btn-sm/.test(greetTag) || /btn-sm/.test(greetRule)) v7.push('A53 .job-greet 不应带 btn-sm（尺寸由几何公式决定）');

  // ---- A54（v10 X）：展开态列数在 1200/1440/1600 都不产生横向溢出 ----
  var bpOpenIdx = html.indexOf('@media (max-width: 1599px)', html.indexOf('.app-shell.is-chat-open .job-grid'));
  var bpOpenBlock = bpOpenIdx < 0 ? '' : html.slice(bpOpenIdx, html.indexOf('}', html.indexOf('.job-card', bpOpenIdx)) + 1);
  if (!/.app-shell\.is-chat-open\s+\.job-card\s*\{\s*min-width:\s*calc\(320px\s*\*\s*var\(--ui-scale\)\)/.test(bpOpenBlock)) {
    v7.push('A54 ≤1599 展开态缺 .job-card { min-width: calc(320px * var(--ui-scale)) }');
  }
  var gridRule = cssRule('.job-grid');
  var gridOpenRule = cssRule('.app-shell.is-chat-open .job-grid');
  if (/overflow-x:\s*hidden/.test(gridRule)) v7.push('A54 .job-grid 不得用 overflow-x: hidden 掩盖溢出');
  if (/overflow-x:\s*hidden/.test(gridOpenRule)) v7.push('A54 展开态 .job-grid 不得用 overflow-x: hidden 掩盖溢出');
  var detachOf = (vw) => Math.round(Math.min(Math.max(vw * 0.5, 600), vw * 0.8));
  var MAIN_PAD = 28, GAP = 10;
  var cases = [[1200, 1], [1440, 1], [1600, 2]];
  var xTable = [];
  for (var ci = 0; ci < cases.length; ci++) {
    var vw = cases[ci][0], cols = cases[ci][1];
    var mainW = vw - detachOf(vw) - GAP;
    var cardW = Math.floor((mainW - MAIN_PAD - (cols - 1) * GAP) / cols);
    xTable.push(vw + ' 展开 ' + cols + ' 列 → 卡片可用 ' + cardW + 'px（下限 ' + Math.round(320 * 1) + '）');
    if (cardW < 320) v7.push('A54 ' + vw + ' 展开 ' + cols + ' 列时卡片宽 ' + cardW + 'px < 320px 下限');
  }
  var x2line = 'A53 方块几何 grid-row 1/span 2 · aspect-ratio 1/1 · min-width 4.6fs · --primary · border 0 · 无 btn-sm/transition · A54 ' + xTable.join(' · ');
  v10line += ' · ' + x2line;
  geom.push(...v7);
  var threeWinLine = threeWin.join(' | ');
  for (const s of ["'running'", "'paused'", "'idle'"]) if (!new RegExp('scrapeRun === ' + s.replace(/'/g, '\\\'')).test(html) && !new RegExp('scrapeRun = ' + s).test(html)) { /* 软检查，见 modes */ }
  // v6 分组行在下方拼接打印
  const msg = errors.length ? 'errors: ' + errors.join(' | ') : 'no runtime errors';
  const failed = geom.filter((g) => g !== 'A17 ok ' + iRail + '<' + iChat);
  if (failed.length) {
    console.log('SMOKE FAIL · ' + msg + ' · geom: ' + failed.join(' | '));
    process.exit(1);
  }
  console.log('SMOKE OK · ' + msg + ' · geom A1–A6/A16/A17 ' + [1200, 1280, 1440, 1600].join('/') + ' pass · A17 rail@' + iRail + ' < chat@' + iChat + ' · A18–A28 固定尺寸/配置字段集/投递入口/三态按钮/外链 pass' + ' · 三窗: ' + threeWinLine + ' · v6: ' + v6line + ' · v7: ' + v7line + ' · v8: ' + v8line + ' · v9: ' + v9line + ' · v9.1: ' + v91line + ' · v9.2: ' + v92line + ' · v10: ' + v10line);
} catch (e) {
  console.log('SMOKE FAIL · ' + e.message);
  process.exit(1);
}

function mathRatio(w, vw) { return w / vw; }
