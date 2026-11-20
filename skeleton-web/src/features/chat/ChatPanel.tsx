/**
 * Boss 消息：左侧会话列表 + 右侧消息流 + 底部发送框。
 *
 * ## 为什么所有按钮都要「确认才发」
 *
 * `chat_send` / `chat_send_resume` / `greet_one` 是把内容**真的发给真人 HR**，
 * 发出去收不回（撤回也只对自己可见窗口有限）。所以：
 * - 打招呼默认只生成草稿，必须再点一次「发送」；
 * - 发简历走 `window.confirm`；
 * - 发送过程中禁用输入框，避免连点发出两条。
 *
 * ## 为什么响应慢也要扛住
 *
 * 每次调用要等 sidecar 打开会话、拉接口，正常 3~20 秒。期间必须显示
 * 「加载中」而不是空白——旧版最容易让人以为卡死的地方就在这里。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  api,
  errText,
  type ChatMessage,
  type Friend,
  type GreetReply,
  type ResumeItem,
} from '../../api';
import { Card, Chip, SkeletonRows } from '../../components/ui';

/** 未读徽标：0 与空都不显示，避免每行挂一个无意义的「0」。 */
function unreadBadge(n: number | null) {
  if (!n || n <= 0) return null;
  return (
    <span className="unread" title={`${n} 条未读`}>
      {n > 99 ? '99+' : n}
    </span>
  );
}

/** 一个会话在列表里的展示文本：优先最后一条消息，没有就退回岗位名。 */
function friendLine(f: Friend): string {
  const msg = (f.last_msg || '').trim();
  if (msg) return msg;
  return (f.title || '').trim() || '（暂无消息）';
}

export function ChatPanel(props: {
  /** 切到本页时递增，用来触发一次刷新 */
  refreshKey: number;
  /** 打招呼草稿生成成功后，父组件可以把结果带到别处展示（当前未用，留口） */
  onGreetDraft?: (g: GreetReply) => void;
}) {
  const [friends, setFriends] = useState<Friend[]>([]);
  const [query, setQuery] = useState('');
  const [activeUid, setActiveUid] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [resumes, setResumes] = useState<ResumeItem[]>([]);
  const [draft, setDraft] = useState('');
  /** 打招呼草稿：生成后仍要用户确认才会发 */
  const [greet, setGreet] = useState<GreetReply | null>(null);

  const [loadingFriends, setLoadingFriends] = useState(false);
  const [loadingMsgs, setLoadingMsgs] = useState(false);
  const [sending, setSending] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [hint, setHint] = useState('');
  const [error, setError] = useState('');

  const feedRef = useRef<HTMLDivElement | null>(null);

  const active = useMemo(
    () => friends.find((f) => f.uid === activeUid) ?? null,
    [friends, activeUid],
  );

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return friends;
    return friends.filter((f) =>
      [f.name, f.company, f.title, f.last_msg]
        .map((x) => (x || '').toLowerCase())
        .some((x) => x.includes(q)),
    );
  }, [friends, query]);

  const loadFriends = useCallback(async () => {
    setLoadingFriends(true);
    setError('');
    try {
      let r;
      try {
        r = await api.chatFriends();
      } catch (e) {
        // 没会话就先把浏览器拉起来再试一次：看一眼消息不该被迫先跑一次抓取。
        // 判据用文案而不是错误码——命令层把「尚未启动」拼进返回的字符串里。
        const msg = errText(e);
        if (!msg.includes('尚未启动') && !msg.includes('没有会话')) throw e;
        setHint('正在启动浏览器…');
        // 带 page='chat'：只启动不导航时浏览器停在空白页，用户点了「Boss 消息」
        // 会看到一片白，得等 chatFriends 自己 goto 才有内容。
        await api.sessionOpen('boss', 'chat');
        r = await api.chatFriends();
      }
      setFriends(r.items);
      setHint(
        r.items.length === 0
          ? '会话列表是空的。确认已登录 Boss；未登录可到「概览」页点「扫码登录」。'
          : '',
      );
    } catch (e) {
      setError(errText(e));
      setFriends([]);
    } finally {
      setLoadingFriends(false);
    }
  }, []);

  const loadMessages = useCallback(async (uid: string) => {
    setLoadingMsgs(true);
    setError('');
    try {
      const r = await api.chatMessages(uid);
      setMessages(r.items);
    } catch (e) {
      setError(errText(e));
      setMessages([]);
    } finally {
      setLoadingMsgs(false);
    }
  }, []);

  // 首次进入与切换页签时拉一次会话列表
  useEffect(() => {
    void loadFriends();
  }, [loadFriends, props.refreshKey]);

  // 选中会话时拉历史消息
  useEffect(() => {
    if (!activeUid) {
      setMessages([]);
      setGreet(null);
      return;
    }
    void loadMessages(activeUid);
    setGreet(null);
    setDraft('');
  }, [activeUid, loadMessages]);

  // 新消息进来后滚到底：不滚的话用户以为消息没发出去
  useEffect(() => {
    const el = feedRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  const doSend = async () => {
    const text = draft.trim();
    if (!activeUid || !text || sending) return;
    setSending(true);
    setError('');
    try {
      const r = await api.chatSend(activeUid, text);
      // 用回执里的文本入列表：超长内容会被侧车截断，显示真正发出去的那段
      setMessages((prev) => [
        ...prev,
        {
          sender: 'me',
          content: r.text || text,
          time: null,
          msg_id: null,
          recalled: false,
        },
      ]);
      setDraft('');
      setHint('');
    } catch (e) {
      setError(errText(e));
    } finally {
      setSending(false);
    }
  };

  const doGenerateGreet = async () => {
    if (!activeUid || generating) return;
    setGenerating(true);
    setError('');
    try {
      // 没有岗位 id：会话列表里只带 title/company，够写一句话术了
      const r = await api.greetDraft({
        title: active?.title ?? undefined,
        company: active?.company ?? undefined,
        useLlm: true,
      });
      setGreet(r);
      props.onGreetDraft?.(r);
      setHint(r.note);
    } catch (e) {
      setError(errText(e));
    } finally {
      setGenerating(false);
    }
  };

  const doSendGreet = async () => {
    if (!activeUid || !greet || sending) return;
    setSending(true);
    setError('');
    try {
      const sent = await api.greetOne(activeUid, greet.text);
      setMessages((prev) => [
        ...prev,
        { sender: 'me', content: sent || greet.text, time: null, msg_id: null, recalled: false },
      ]);
      setGreet(null);
      setHint('');
    } catch (e) {
      setError(errText(e));
    } finally {
      setSending(false);
    }
  };

  const loadResumes = async () => {
    try {
      const r = await api.chatResumes();
      setResumes(r.items);
      if (r.items.length === 0) setHint('没读到简历列表。确认 Boss 账号里已上传附件简历。');
    } catch (e) {
      setError(errText(e));
    }
  };

  const doSendResume = async (resume: ResumeItem) => {
    if (!activeUid || sending) return;
    const name = resume.name || resume.resume_id;
    if (!window.confirm(`确定把简历「${name}」发给「${active?.name || activeUid}」？发出去之后无法收回。`)) {
      return;
    }
    setSending(true);
    setError('');
    try {
      await api.chatSendResume(activeUid, resume.resume_id);
      setMessages((prev) => [
        ...prev,
        { sender: 'me', content: `[附件简历] ${name}`, time: null, msg_id: null, recalled: false },
      ]);
    } catch (e) {
      setError(errText(e));
    } finally {
      setSending(false);
    }
  };

  const doRecall = async () => {
    if (!activeUid || sending) return;
    if (!window.confirm('确定撤回最近一条自己发的消息？')) return;
    setSending(true);
    setError('');
    try {
      await api.chatRecall(activeUid);
      // 撤回后整段重拉，比在前端猜哪条变成「已撤回」可靠
      await loadMessages(activeUid);
    } catch (e) {
      setError(errText(e));
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="chat-wrap" data-testid="chat-panel">
      <Card
        title="Boss 消息"
        subtitle={
          <span>
            {loadingFriends ? '正在读取会话…' : `${friends.length} 个会话`}
            {active ? ` · 当前：${active.name || active.uid}` : ''}
          </span>
        }
        actions={
          <button
            className="btn btn-sm"
            data-testid="chat-refresh"
            onClick={() => void loadFriends()}
            disabled={loadingFriends}
          >
            {loadingFriends ? '刷新中…' : '刷新会话'}
          </button>
        }
      >
        <div className="chat-grid">
          <aside className="chat-side">
            <input
              className="input"
              data-testid="chat-search"
              placeholder="搜索 HR / 公司 / 消息"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            {loadingFriends ? (
              <div className="chat-list">
                <SkeletonRows cols={2} rows={6} />
              </div>
            ) : shown.length === 0 ? (
              <div className="empty" data-testid="chat-empty">
                {friends.length === 0 ? '还没有会话' : '没有匹配的会话'}
              </div>
            ) : (
              <div className="chat-list" role="listbox" aria-label="会话列表">
                {shown.map((f) => (
                  <button
                    key={f.uid}
                    className={`chat-item${f.uid === activeUid ? ' is-active' : ''}`}
                    data-testid={`chat-item-${f.uid}`}
                    onClick={() => setActiveUid(f.uid)}
                  >
                    <span className="chat-item-top">
                      <span className="chat-item-name">{f.name || f.uid}</span>
                      <span className="chat-item-time">{f.last_time || ''}</span>
                    </span>
                    <span className="chat-item-sub">{friendLine(f)}</span>
                    <span className="chat-item-meta">
                      {f.company ? <span>{f.company}</span> : null}
                      {unreadBadge(f.unread)}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </aside>

          <section className="chat-main">
            {!activeUid ? (
              <div className="empty" data-testid="chat-hint">
                左侧选一位 HR 开始。
              </div>
            ) : (
              <>
                <div className="chat-head">
                  <div>
                    <div className="chat-head-name">{active?.name || activeUid}</div>
                    <div className="chat-head-sub">
                      {[active?.company, active?.title].filter(Boolean).join(' · ') || '—'}
                    </div>
                  </div>
                  <div className="chat-head-actions">
                    <button
                      className="btn btn-sm"
                      data-testid="chat-greet-btn"
                      onClick={() => void doGenerateGreet()}
                      disabled={generating || sending}
                    >
                      {generating ? '生成中…' : '生成打招呼'}
                    </button>
                    <button className="btn btn-sm" onClick={() => void loadResumes()} disabled={sending}>
                      选简历
                    </button>
                    <button
                      className="btn btn-sm btn-ghost"
                      data-testid="chat-recall-btn"
                      onClick={() => void doRecall()}
                      disabled={sending}
                    >
                      撤回上一条
                    </button>
                  </div>
                </div>

                <div className="chat-feed" data-testid="chat-feed" ref={feedRef}>
                  {loadingMsgs ? (
                    <SkeletonRows cols={1} rows={5} />
                  ) : messages.length === 0 ? (
                    <div className="empty">还没有消息记录</div>
                  ) : (
                    messages.map((m, i) => (
                      <div
                        key={`${m.msg_id || i}`}
                        className={`bubble-row is-${m.sender}`}
                      >
                        <div className="bubble">
                          <div className="bubble-text">
                            {m.recalled ? <span className="bubble-recall">该消息已撤回</span> : m.content}
                          </div>
                          {m.time ? <div className="bubble-time">{m.time}</div> : null}
                        </div>
                      </div>
                    ))
                  )}
                </div>

                {greet ? (
                  <div className="greet-box" data-testid="chat-greet-box">
                    <div className="greet-head">
                      <span className="section-label">打招呼草稿</span>
                      <Chip tone={greet.source === 'llm' ? 'primary' : 'warn'}>
                        {greet.source === 'llm' ? `大模型 · ${greet.tokens} tokens` : '规则话术'}
                      </Chip>
                    </div>
                    <div className="greet-text" data-testid="chat-greet-text">
                      {greet.text}
                    </div>
                    {greet.note ? <div className="cfg-hint">{greet.note}</div> : null}
                    <div className="greet-actions">
                      <button
                        className="btn btn-sm btn-primary"
                        data-testid="chat-greet-send"
                        onClick={() => void doSendGreet()}
                        disabled={sending}
                      >
                        {sending ? '发送中…' : '发送这句'}
                      </button>
                      <button
                        className="btn btn-sm btn-ghost"
                        onClick={() => setDraft(greet.text)}
                        disabled={sending}
                      >
                        放到输入框再改
                      </button>
                      <button className="btn btn-sm btn-ghost" onClick={() => setGreet(null)}>
                        丢弃
                      </button>
                    </div>
                  </div>
                ) : null}

                {resumes.length > 0 ? (
                  <div className="resume-bar">
                    <span className="section-label">简历</span>
                    {resumes.map((r) => (
                      <button
                        key={r.resume_id}
                        className="chip chip-accent as-btn"
                        onClick={() => void doSendResume(r)}
                        disabled={sending}
                        title={r.updated_at || ''}
                      >
                        {r.name || r.resume_id}
                      </button>
                    ))}
                    <button className="btn btn-sm btn-ghost" onClick={() => setResumes([])}>
                      收起
                    </button>
                  </div>
                ) : null}

                <div className="chat-send">
                  <textarea
                    className="textarea chat-input"
                    data-testid="chat-input"
                    placeholder="输入消息，Ctrl+Enter 发送"
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                        e.preventDefault();
                        void doSend();
                      }
                    }}
                    rows={2}
                    disabled={sending}
                  />
                  <button
                    className="btn btn-primary"
                    data-testid="chat-send-btn"
                    onClick={() => void doSend()}
                    disabled={sending || !draft.trim()}
                  >
                    {sending ? '发送中…' : '发送'}
                  </button>
                </div>
              </>
            )}
          </section>
        </div>

        {error ? (
          <div className="error-bar" data-testid="chat-error">
            {error}
            <button className="btn btn-sm btn-ghost" onClick={() => setError('')}>
              忽略
            </button>
          </div>
        ) : null}
        {hint ? <div className="cfg-hint" data-testid="chat-note">{hint}</div> : null}
      </Card>
    </div>
  );
}
