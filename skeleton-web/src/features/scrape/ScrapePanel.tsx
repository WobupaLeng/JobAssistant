/**
 * 抓取控制：关键词 / 入库门槛 / 起停 / 登录 / 实时事件流。
 *
 * 设计约束：
 * - 进度靠轮询 `scrape_progress`，事件靠 `scrape_drain` 拉增量（Rust 侧注释：
 *   崩溃重启的 `Exited` 只从 drain 冒泡，不能改到 progress 里轮询）；
 * - 门槛默认值来自 `score_min_default`，这里不写死数字；
 * - 停止后**不**立刻把 running 置 false，等 sidecar 自己回报退出。
 */
import { useEffect, useRef, useState } from 'react';
import { api, emptyProgress, type ProgressReply, type ScrapeUpdate } from '../../api';
import { Card, Chip, Stat, StatusDot } from '../../components/ui';

type FeedLine = { tag: string; text: string; tone: 'ok' | 'bad' | 'warn' | 'plain' };

function toFeedLine(u: ScrapeUpdate): FeedLine {
  switch (u.kind) {
    case 'job':
      // 入库行的 tag 必须是 ✔：tools/gui_e2e.py 用 `feed.count("✔")` 统计入库条数
      return u.accepted
        ? { tag: '✔', text: `${u.score ?? '—'} 分 · ${u.summary}`, tone: 'ok' }
        : { tag: '跳过', text: u.summary, tone: 'warn' };
    case 'log':
      return { tag: u.level, text: u.msg, tone: u.level === 'error' ? 'bad' : 'plain' };
    case 'login':
      return { tag: '登录', text: `${u.state} · ${u.msg}`, tone: u.state === 'ok' ? 'ok' : 'warn' };
    case 'malformed':
      return { tag: '解析', text: `无法解析：${u.line}`, tone: 'bad' };
    case 'exited':
      return { tag: '退出', text: `抓取进程退出（code=${u.code ?? '无'}）`, tone: 'warn' };
    case 'restarted':
      return { tag: '重启', text: `已自动重启抓取进程（pid=${u.pid}）`, tone: 'warn' };
  }
}

const LOGIN_LABEL: Record<string, string> = {
  ok: '已登录',
  unknown: '登录态未知',
  required: '需要扫码登录',
  expired: '登录已过期',
};

export function ScrapePanel(props: {
  keywords: string;
  onKeywords: (v: string) => void;
  minScore: number | null;
  onMinScore: (v: number | null) => void;
  scoreMax: number | null;
  minScoreDefault: number | null;
  onInserted: () => void;
  notify: (msg: string) => void;
  fail: (e: unknown) => void;
}) {
  const [progress, setProgress] = useState<ProgressReply>(emptyProgress);
  const [feed, setFeed] = useState<FeedLine[]>([]);
  const [starting, setStarting] = useState(false);
  const [loginBusy, setLoginBusy] = useState(false);
  const feedRef = useRef<HTMLDivElement | null>(null);
  const timer = useRef<number | null>(null);
  // 每入库一条就要刷新列表，但刷新是父组件的事，用 ref 避免重建轮询
  const onInserted = useRef(props.onInserted);
  onInserted.current = props.onInserted;

  const push = (lines: FeedLine[]) => {
    if (lines.length === 0) return;
    setFeed((prev) => [...prev, ...lines].slice(-300));
    if (lines.some((l) => l.tag === '✔')) onInserted.current();
  };

  const pull = async () => {
    try {
      const [p, updates] = await Promise.all([api.scrapeProgress(), api.scrapeDrain(50)]);
      setProgress(p);
      push(updates.map(toFeedLine));
    } catch (e) {
      props.fail(e);
    }
  };
  const pullRef = useRef(pull);
  pullRef.current = pull;

  // 首次进来先拉一次，显示上次会话残留的进度与登录态
  useEffect(() => {
    void pullRef.current();
  }, []);

  const running = progress.running;
  useEffect(() => {
    if (!running) {
      if (timer.current !== null) {
        window.clearInterval(timer.current);
        timer.current = null;
      }
      return;
    }
    timer.current = window.setInterval(() => void pullRef.current(), 800);
    return () => {
      if (timer.current !== null) {
        window.clearInterval(timer.current);
        timer.current = null;
      }
    };
  }, [running]);

  // 事件流永远滚到底部，长任务也不会卡在旧位置
  useEffect(() => {
    const el = feedRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [feed]);

  const start = async () => {
    const kw = props.keywords
      .split(/[,，\s]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    setStarting(true);
    try {
      const r = await api.scrapeStart({
        keywords: kw.length > 0 ? kw : undefined,
        minScore: props.minScore ?? undefined,
      });
      // 「已启动抓取进程」与「门槛 N 分」是 tools/gui_e2e.py 的断言锚点，措辞不可改
      push([
        {
          tag: '开始',
          text: `已启动抓取进程 pid=${r.pid} · 关键词 ${r.keywords.join(' / ')} · 门槛 ${r.min_score} 分`,
          tone: 'ok',
        },
      ]);
      props.notify(`已启动抓取（pid ${r.pid}）`);
      await pull();
    } catch (e) {
      props.fail(e);
    } finally {
      setStarting(false);
    }
  };

  const stop = async () => {
    try {
      await api.scrapeStop();
      await pull();
    } catch (e) {
      props.fail(e);
    }
  };

  const login = async () => {
    setLoginBusy(true);
    try {
      const pid = await api.loginStart('boss');
      push([{ tag: '登录', text: `已请求登录（pid=${pid}），请在弹出的浏览器窗口里扫码`, tone: 'warn' }]);
      await pull();
    } catch (e) {
      props.fail(e);
    } finally {
      setLoginBusy(false);
    }
  };

  const loginState = LOGIN_LABEL[progress.login_state] ?? progress.login_state;
  const loginTone = progress.login_state === 'ok' ? 'primary' : progress.login_state === 'unknown' ? 'default' : 'warn';

  return (
    <Card
      testId="scrape-panel"
      title="抓取控制"
      subtitle={
        <span data-testid="scrape-status">
          <StatusDot state={running ? 'running' : 'idle'} label={running ? '抓取中' : '未在抓取'} />
        </span>
      }
      actions={
        <>
          <Chip tone={loginTone} title="sidecar 回报的登录态">
            {loginState}
          </Chip>
          <button
            className="btn btn-sm"
            data-testid="scrape-login-btn"
            onClick={login}
            disabled={loginBusy || running}
            title={running ? '抓取中无法同时启动登录浏览器' : '拉一个浏览器窗口扫码登录 Boss 直聘'}
          >
            扫码登录
          </button>
        </>
      }
    >
      <div className="scrape-controls">
        <label className="field">
          <span className="field-label">关键词（逗号分隔）</span>
          <input
            className="input"
            data-testid="scrape-keywords"
            value={props.keywords}
            placeholder="Java, Spring Cloud, 后端开发"
            onChange={(e) => props.onKeywords(e.target.value)}
          />
        </label>
        <label className="field">
          <span className="field-label">入库门槛</span>
          <input
            className="input input-num"
            data-testid="scrape-min-score"
            type="number"
            min={0}
            max={props.scoreMax ?? undefined}
            value={props.minScore ?? ''}
            placeholder={props.minScoreDefault !== null ? String(props.minScoreDefault) : ''}
            onChange={(e) => props.onMinScore(e.target.value === '' ? null : Number(e.target.value))}
          />
        </label>
        <button
          className="btn btn-primary"
          data-testid="scrape-start-btn"
          onClick={start}
          disabled={starting || running}
          title={running ? '先点「停止」结束本轮再重启' : undefined}
        >
          {starting ? '启动中…' : '开始抓取'}
        </button>
        <button className="btn btn-danger" data-testid="scrape-stop-btn" onClick={stop} disabled={!running}>
          停止
        </button>
      </div>

      {/* 五个计数既有可读的卡片，又有一行无样式摘要：
          tools/gui_e2e.py 的 PROGRESS_RE 按「已抓 N…入库 N…重复 N…低于门槛 N…规则否决 N」
          的顺序解析这段文本，改顺序或改措辞都会让进度断言静默失效。 */}
      <div className="stats" data-testid="scrape-progress">
        <Stat label="已抓" value={progress.scraped} testId="scrape-scraped" />
        <Stat label="入库" value={progress.inserted} tone="good" testId="scrape-inserted" />
        <Stat label="重复" value={progress.duplicates} testId="scrape-duplicates" />
        <Stat label="低于门槛" value={progress.below_threshold} tone="warn" testId="scrape-below" />
        <Stat label="规则否决" value={progress.rejected} tone="bad" testId="scrape-rejected" />
        <span className="sr-only">
          {`已抓 ${progress.scraped} · 入库 ${progress.inserted} · 重复 ${progress.duplicates}` +
            ` · 低于门槛 ${progress.below_threshold} · 规则否决 ${progress.rejected}`}
        </span>
      </div>

      <div className="row wrap" style={{ marginTop: 10, fontSize: 12, color: 'var(--text-3)' }}>
        <span>
          最近：<span style={{ color: 'var(--text-2)' }}>{progress.last_title ?? '—'}</span>
        </span>
        {progress.finished_reason && <span>· 结束原因：{progress.finished_reason}</span>}
        {progress.last_log && (
          <span className="cell-ellipsis" style={{ minWidth: 200, flex: 1 }} title={progress.last_log}>
            · {progress.last_log}
          </span>
        )}
      </div>

      <div className="feed" data-testid="scrape-feed" ref={feedRef}>
        {feed.length === 0 ? (
          <span style={{ color: 'var(--text-3)' }}>
            尚无事件。点击「扫码登录」完成一次登录，再点「开始抓取」，这里会滚动显示每条岗位的判定与打分。
          </span>
        ) : (
          feed.map((line, i) => (
            <div key={i} className={`feed-line is-${line.tone}`}>
              <span className="tag">{line.tag}</span>
              <span>{line.text}</span>
            </div>
          ))
        )}
      </div>
    </Card>
  );
}
