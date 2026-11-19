// Boss 消息面板：左 1/4 会话列表 + 右 1/4 对话框。
// 数据全部来自后端驱动真实 Chrome 抓到的 Boss 会话（/api/chat/*）。
import { useCallback, useEffect, useRef, useState } from 'react'
import { apiClient, type ChatFriend, type ChatMessage } from './api'

/** 头像加载失败时的兜底底色 */
const AVATAR_COLORS = ['#10b981', '#3b82f6', '#f59e0b', '#8b5cf6', '#ef4444', '#06b6d4']

/**
 * Boss 用私用区（PUA）字体做反爬，个别昵称/公司名会带回孤立代理字符（如 \udcab），
 * 直接渲染会让 React 抛错并整页白屏，所以所有文本出口都先过一遍。
 */
const clean = (s: unknown): string =>
  typeof s === 'string' ? s.replace(/[\uD800-\uDFFF]/g, '').replace(/[\uE000-\uF8FF]/g, '') : ''

const colorFor = (key: string) => {
  let h = 0
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) % 997
  return AVATAR_COLORS[h % AVATAR_COLORS.length]
}

function Avatar({ f, size = 32 }: { f: ChatFriend; size?: number }) {
  const [bad, setBad] = useState(false)
  const label = (clean(f.name) || '?').slice(0, 1)
  if (f.avatar && !bad) {
    return (
      <img
        src={f.avatar}
        alt={clean(f.name)}
        onError={() => setBad(true)}
        style={{ width: size, height: size }}
        className="shrink-0 rounded-full object-cover"
      />
    )
  }
  return (
    <div
      style={{ width: size, height: size, background: colorFor(f.uid || f.name || '?') }}
      className="flex shrink-0 items-center justify-center rounded-full text-[13px] font-medium text-white"
    >
      {label}
    </div>
  )
}

export interface ChatPanelProps {
  /** 外部（点岗位卡片时）要求切到的 HR uid；变化即触发切换 */
  focusUid?: string | null
  /** 发消息成功后通知外面刷新投递列表 */
  onSent?: () => void
}

export default function ChatPanel({ focusUid, onSent }: ChatPanelProps) {
  const [friends, setFriends] = useState<ChatFriend[]>([])
  const [friendsErr, setFriendsErr] = useState('')
  const [friendsLoading, setFriendsLoading] = useState(false)
  const [kw, setKw] = useState('')
  const [activeUid, setActiveUid] = useState<string | null>(null)
  const [msgs, setMsgs] = useState<ChatMessage[]>([])
  const [msgsLoading, setMsgsLoading] = useState(false)
  const [msgsErr, setMsgsErr] = useState('')
  const [draft, setDraft] = useState('')
  const [sending, setSending] = useState(false)
  const [sendErr, setSendErr] = useState('')
  const [toast, setToast] = useState('')
  const msgBoxRef = useRef<HTMLDivElement>(null)

  const active = friends.find((f) => String(f.uid) === String(activeUid)) || null

  const flash = (text: string) => {
    setToast(text)
    window.setTimeout(() => setToast(''), 2600)
  }

  // 模块级缓存：ChatPanel 被隐藏/重建（例如打开敏感库）后回来不用重新
  // 驱动浏览器拉一遍会话列表，否则每次都要等 5~20 秒。
  let _friendCache: { at: number; items: ChatFriend[] } | null = null
  const FRIEND_TTL = 5 * 60_000

  // ---------- 会话列表 ----------
  const loadFriends = useCallback(async (force = false) => {
    if (!force && _friendCache && Date.now() - _friendCache.at < FRIEND_TTL) {
      setFriends(_friendCache.items)
      return
    }
    setFriendsLoading(true)
    setFriendsErr('')
    try {
      const r = await apiClient.chatFriends(force)
      if (!r.ok) {
        setFriendsErr(r.error || '读取 Boss 会话列表失败')
        if (!_friendCache) setFriends([])
      } else {
        const items = r.items || []
        _friendCache = { at: Date.now(), items }
        setFriends(items)
      }
    } catch (e: any) {
      setFriendsErr(String(e?.message || e))
      if (_friendCache) setFriends(_friendCache.items)
    } finally {
      setFriendsLoading(false)
    }
  }, [])

  useEffect(() => {
    loadFriends(false)
  }, [loadFriends])

  // ---------- 拉某个会话的消息 ----------
  const loadMessages = useCallback(async (uid: string, force = false) => {
    setMsgsLoading(true)
    setMsgsErr('')
    try {
      const r = await apiClient.chatMessages(uid, force)
      if (!r.ok) {
        setMsgsErr(r.error || '读取聊天记录失败')
        setMsgs([])
      } else {
        setMsgs(r.items || [])
      }
    } catch (e: any) {
      setMsgsErr(String(e?.message || e))
      setMsgs([])
    } finally {
      setMsgsLoading(false)
    }
  }, [])

  const openFriend = useCallback(
    (f: ChatFriend) => {
      const uid = String(f.uid)
      setActiveUid(uid)
      setDraft('')
      setSendErr('')
      loadMessages(uid)
    },
    [loadMessages],
  )

  // 外部要求切到某个 HR（点未投递岗位卡片）
  useEffect(() => {
    if (!focusUid) return
    const uid = String(focusUid)
    setActiveUid(uid)
    setSendErr('')
    loadMessages(uid)
  }, [focusUid, loadMessages])

  // 消息变化后滚到底
  useEffect(() => {
    const el = msgBoxRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [msgs, activeUid])

  // ---------- 发送 ----------
  const doSend = async () => {
    const text = draft.trim()
    if (!text || !active || sending) return
    setSending(true)
    setSendErr('')
    try {
      const r = await apiClient.chatSend(String(active.uid), text)
      if (!r.ok) {
        setSendErr(r.error || '发送失败')
        setDraft(text)
        return
      }
      setDraft('')
      flash('已发送，该岗位已加入「已投递」')
      await loadMessages(String(active.uid))
      onSent?.()
    } catch (e: any) {
      setSendErr(String(e?.message || e))
      setDraft(text)
    } finally {
      setSending(false)
    }
  }

  // ---------- 撤回（网页端不支持，提示去 App 操作） ----------
  const doRecall = async () => {
    if (!active) return
    try {
      const r = await apiClient.chatRecall(String(active.uid))
      if (!r.ok) setSendErr(r.error || '撤回失败')
      else flash('已撤回')
    } catch (e: any) {
      setSendErr(String(e?.message || e))
    }
  }

  const shown = friends.filter((f) => {
    if (!kw.trim()) return true
    const q = kw.trim().toLowerCase()
    return (
      clean(f.name).toLowerCase().includes(q) ||
      clean(f.brand).toLowerCase().includes(q) ||
      clean(f.title).toLowerCase().includes(q) ||
      clean(f.last_msg).toLowerCase().includes(q)
    )
  })

  return (
    <div className="flex min-h-0 flex-1 gap-4 bg-slate-50/60 dark:bg-slate-900/30">
      {/* ========== 左 1/4：会话列表（选中项高亮） ========== */}
      <div className="flex min-w-0 flex-[1] flex-col overflow-hidden">
        <div className="flex shrink-0 items-center gap-2 px-1 py-2">
          <h2 className="text-xs font-semibold">Boss 消息</h2>
          <span className="text-[10px] text-slate-400">{friends.length}</span>
          <button
            className="ml-auto btn-ghost !px-1.5 !py-0.5 text-[11px]"
            title="从 Boss 重新拉取会话列表"
            onClick={() => loadFriends(true)}
            disabled={friendsLoading}
          >
            {friendsLoading ? '读取中…' : '刷新'}
          </button>
        </div>
        <div className="shrink-0 px-2 py-1.5">
          <input
            className="input !py-1 !text-[11px]"
            placeholder="搜索 HR / 公司 / 消息"
            value={kw}
            onChange={(e) => setKw(e.target.value)}
          />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {friendsErr && (
            <div className="px-2 py-3 text-[11px] leading-5 text-rose-500">
              {friendsErr}
              <div className="mt-1 text-slate-400">确认左上角已登录 Boss 后点「刷新」。</div>
            </div>
          )}
          {!friendsErr && friendsLoading && friends.length === 0 && (
            <div className="px-2 py-3 text-[11px] text-slate-400">正在通过浏览器读取 Boss 会话，首次约需 5–20 秒…</div>
          )}
          {!friendsErr && !friendsLoading && shown.length === 0 && (
            <div className="px-2 py-3 text-[11px] text-slate-400">没有会话</div>
          )}
          {shown.map((f) => {
            const on = String(f.uid) === String(activeUid)
            return (
              <button
                key={f.uid}
                onClick={() => openFriend(f)}
                className={`mb-1.5 flex w-full items-center gap-2.5 rounded-lg px-2 py-2.5 text-left transition ${
                  on
                    ? 'bg-primary/15 font-semibold ring-1 ring-primary/35'
                    : 'hover:bg-slate-100/80 dark:hover:bg-slate-800/50'
                }`}
              >
                <Avatar f={f} size={on ? 38 : 34} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline gap-1.5">
                    <span
                      className={`truncate text-[12px] ${
                        on ? 'font-semibold text-primary' : 'font-medium'
                      }`}
                    >
                      {clean(f.name) || '-'}
                    </span>
                    <span className="shrink-0 text-[10px] text-slate-400">{clean(f.title)}</span>
                    <span className="ml-auto shrink-0 text-[10px] text-slate-400">
                      {clean(f.last_time)}
                    </span>
                  </div>
                  <div className="mt-1 flex items-center gap-1">
                    <span className="truncate text-[11px] text-slate-500 dark:text-slate-400">
                      {clean(f.brand)}
                      {f.last_msg ? ` · ${clean(f.last_msg)}` : ''}
                    </span>
                    {!!f.unread && (
                      <span className="ml-auto shrink-0 rounded-full bg-rose-500 px-1 text-[9px] text-white">
                        {f.unread}
                      </span>
                    )}
                  </div>
                </div>
              </button>
            )
          })}
        </div>
      </div>

      {/* ========== 右 1/4：对话框 ========== */}
      <div className="flex min-w-0 flex-[3] flex-col overflow-hidden">
        {/* 顶栏固定：标题 + 在招岗位，不是卡片，只是清楚的分区 */}
        <div className="flex shrink-0 items-center gap-2 px-1 py-2">
          <h2 className="text-xs font-semibold">对话框</h2>
          {active && (
            <>
              <span className="text-[10px] text-slate-400">
                {clean(active.name)} · {clean(active.brand)}
              </span>
              <button
                className="ml-auto btn-ghost !px-1.5 !py-0.5 text-[10px]"
                onClick={() => loadMessages(String(active.uid), true)}
                disabled={msgsLoading}
                title="重新拉取这个会话的聊天记录"
              >
                {msgsLoading ? '读取中…' : '刷新记录'}
              </button>
              {active.encrypt_job_id && (
                <a
                  className="text-[10px] text-primary hover:underline"
                  href={`https://www.zhipin.com/job_detail/${active.encrypt_job_id}.html`}
                  target="_blank"
                  rel="noreferrer"
                >
                  查看岗位
                </a>
              )}
            </>
          )}
        </div>

        {!active ? (
          <div className="flex min-h-0 flex-1 flex-col items-center justify-center p-6 text-center">
            <div className="mb-2 h-10 w-10 rounded-full bg-primary/15" />
            <h2 className="text-sm font-semibold text-slate-600 dark:text-slate-300">选择一位 HR</h2>
            <p className="mt-1 max-w-[220px] text-[11px] leading-5 text-slate-400">
              左边点选会话即可查看聊天记录；
              <br />
              在「未投递」列表点开某个岗位，也会自动切到对应的 HR。
            </p>
          </div>
        ) : (
          <>
            {/* 消息区 */}
            <div ref={msgBoxRef} className="min-h-0 flex-1 space-y-2 overflow-y-auto px-2 py-2">
              {msgsErr && (
                <div className="rounded border border-rose-200 bg-rose-50/60 px-2 py-1.5 text-[11px] text-rose-600 dark:border-rose-900 dark:bg-rose-950/30">
                  {msgsErr}
                </div>
              )}
              {msgsLoading && msgs.length === 0 && (
                <div className="py-3 text-center text-[11px] text-slate-400">正在打开这个会话读取聊天记录…</div>
              )}
              {!msgsLoading && !msgsErr && msgs.length === 0 && (
                <div className="py-3 text-center text-[11px] text-slate-400">还没有聊天记录</div>
              )}
              {msgs.map((m, i) => {
                if (m.system) {
                  return (
                    <div key={m.id || i} className="text-center text-[10px] text-slate-400">
                      {clean(m.text)}
                    </div>
                  )
                }
                const prev = msgs[i - 1]
                const showTime = !prev || Math.abs((m.time || 0) - (prev.time || 0)) > 5 * 60_000
                return (
                  <div key={m.id || i}>
                    {showTime && m.time_text && (
                      <div className="mb-1 text-center text-[10px] text-slate-400">{clean(m.time_text)}</div>
                    )}
                    <div className={`flex ${m.from_me ? 'justify-end' : 'justify-start'}`}>
                      <div
                        className={`max-w-[85%] whitespace-pre-wrap break-words rounded-lg px-2 py-1 text-[12px] leading-5 ${
                          m.from_me
                            ? 'bg-primary/12 text-slate-700 dark:bg-primary/20 dark:text-slate-100'
                            : 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-200'
                        }`}
                      >
                        {clean(m.text)}
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>

            {/* 输入区 */}
            <div className="shrink-0 border-t border-slate-100 p-2 dark:border-dark-border">
              {(sendErr || toast) && (
                <div
                  className={`mb-1 rounded px-2 py-1 text-[10px] leading-4 ${
                    sendErr
                      ? 'bg-rose-50 text-rose-600 dark:bg-rose-950/30'
                      : 'bg-emerald-50 text-emerald-600 dark:bg-emerald-950/30'
                  }`}
                >
                  {sendErr || toast}
                </div>
              )}
              <textarea
                className="input min-h-[52px] resize-none !text-[12px]"
                placeholder={`发消息给 ${clean(active.name) || 'HR'}…（Enter 发送，Shift+Enter 换行）`}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault()
                    doSend()
                  }
                }}
              />
              <div className="mt-1 flex items-center gap-2">
                <span className="text-[10px] text-slate-400">
                  发出后该岗位自动进入「已投递」
                </span>
                <button
                  className="ml-auto btn-ghost !px-1.5 !py-0.5 text-[10px]"
                  onClick={doRecall}
                  title="Boss 求职端网页版不支持撤回，需到 App 操作"
                >
                  撤回
                </button>
                <button className="btn-primary !px-3 !py-1 text-[11px]" onClick={doSend} disabled={sending || !draft.trim()}>
                  {sending ? '发送中…' : '发送'}
                </button>
              </div>
            </div>
          </>
        )}
      </div>

    </div>
  )
}
