/**
 * 应用外壳：顶栏 + 错误条 + 提示条 + 页面分发。
 *
 * **本文件不包含任何业务动作**。它只做三件事：
 * 1. 顶栏（身份 / 口径 / 大模型 / 主题）；
 * 2. 全局错误条与提示条（跨页共享，所以必须留在这里）；
 * 3. 按 nav 渲染 `pages/` 里的一个页面。
 *
 * 结构性约束（SPEC-002 第 3.2 节 + AGENTS.md）：
 * - 满分与门槛**只能**来自 `get_scoring_spec`，本文件不出现硬编码分数；
 * - 整页背景保持透明，毛玻璃由 Rust 侧系统合成，任何不透明底色都会吃掉它；
 * - **概览是默认页**，E2E 依赖的控件都在那一页，见 `pages/DashboardPage.tsx`。
 */

import { useCallback, useEffect, useState } from 'react';
import { api } from './api';
import { NAV, NAV_ENTER, type Nav } from './lib/nav';
import { useBootstrap } from './app/useBootstrap';
import { useTheme } from './app/useTheme';
import { Banner, Chip, ErrorBar } from './components/ui';
import { Page } from './pages';

export default function App() {
  const { info, spec, llm, error, dismissError, refreshLlm, setError } = useBootstrap();
  const { theme, toggle } = useTheme();
  const [nav, setNav] = useState<Nav>('dashboard');
  const [notice, setNotice] = useState('');
  // 进入页面时递增，驱动该页重拉（logs / chat 需要新鲜数据）
  const [pageTick, setPageTick] = useState(0);

  const fail = useCallback((e: unknown) => setError(String(e instanceof Error ? e.message : e)), [setError]);

  useEffect(() => {
    if (!notice) return;
    const t = window.setTimeout(() => setNotice(''), 4000);
    return () => window.clearTimeout(t);
  }, [notice]);

  const go = (k: Nav) => {
    setNav(k);
    const enter = NAV_ENTER[k];
    if (enter === 'none') return;
    setPageTick((n) => n + 1);
    if (enter === 'refresh-chat') {
      // 顺手把浏览器带到聊天页。sidecar 可能早在别处（抓取/登录/期望）就起来了，
      // 但它当时停在别的页面；用户点「Boss 消息」就该看到消息，不是一片空白。
      // 失败不打扰：没登录或 sidecar 没起时，ChatPanel 自己会拉起并重试。
      void api.sessionOpen('boss', 'chat').catch(() => {});
    }
  };

  return (
    <div className="app-shell" data-testid="app-root">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark">职</span>
          AI 求职助手
        </div>
        <span className="build-tag" data-testid="app-build">
          {info ? `${info.shell} · v${info.version} · ${info.stage}` : '连接中…'}
        </span>
        <nav className="tabs nav-tabs" role="tablist" aria-label="主导航">
          {NAV.map((t) => (
            <button
              key={t.key}
              role="tab"
              aria-selected={nav === t.key}
              className={`tab ${nav === t.key ? 'is-active' : ''}`}
              data-testid={`nav-${t.key}`}
              onClick={() => go(t.key)}
            >
              {t.name}
            </button>
          ))}
        </nav>
        <span className="spacer" />
        <span className="card-sub" data-testid="score-spec">
          满分 {spec?.score_max ?? '…'} / 默认门槛 {spec?.score_min_default ?? '…'}
        </span>
        <Chip
          tone={llm?.configured ? 'primary' : 'default'}
          title={llm?.configured ? `密钥 ${llm.key_hint ?? ''} · ${llm.model ?? ''}` : '未配置 API 密钥'}
        >
          {llm?.configured ? `大模型 ${llm.model ?? '已配置'}` : '大模型未配置'}
        </Chip>
        <button
          className="btn btn-sm btn-ghost"
          data-testid="theme-toggle"
          onClick={toggle}
          title="切换明暗主题"
        >
          {theme === 'dark' ? '☾ 深色' : '☀ 浅色'}
        </button>
      </header>

      <div className="dashboard" data-testid={`page-${nav}`}>
        {error && <ErrorBar message={error} onDismiss={dismissError} testId="app-error" />}
        {notice && <Banner testId="app-notice">{notice}</Banner>}

        <Page
          nav={nav}
          spec={spec}
          llm={llm}
          refreshKey={pageTick}
          notify={setNotice}
          fail={fail}
          onConfigSaved={refreshLlm}
        />
      </div>
    </div>
  );
}
