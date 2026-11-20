// Boss 消息面板：左 1/4 会话列表 + 右 1/4 对话框。
// 数据全部来自后端驱动真实 Chrome 抓到的 Boss 会话（/api/chat/*）。
import { useCallback, useEffect, useRef, useState } from 'react'
import { apiClient, type ChatFriend, type ChatMessage, type ResumeItem } from './api'

/** 统一配色：不在会话列表里给每个人配不同颜色 */
const UNIFORM = '#64748b'

const hueOf = (_key: string) => UNIFORM

/**
 * Boss 用私用区（PUA）字体做反爬，个别昵称/公司名会带回孤立代理字符（如 \udcab），
 * 直接渲染会让 React 抛错并整页白屏，所以所有文本出口都先过一遍。
 */
const clean = (s: unknown): string =>
  typeof s === 'string' ? s.replace(/[\uD800-\uDFFF]/g, '').replace(/[\uE000-\uF8FF]/g, '') : ''

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
      style={{ width: size, height: size, background: hueOf(String(f.uid || f.name || '?')) }}
      className="flex shrink-0 items-center justify-center rounded-full text-[13px] font-medium text-white"
    >
      {label}
    </div>
  )
}

export interface ChatPanelProps {
  /** 外部（点岗位卡片时）要求切到的 HR uid；变化即触发切换并自动展开 */
  focusUid?: string | null
  /** 发消息成功后通知外面刷新投递列表 */
  onSent?: () => void
  /** 生成打招呼成功后的回调，用于把累计 token 同步给顶部/配置窗 */
  onTokens?: (tokens: { total: number; greet: number }) => void
  /** 抽屉展开状态变化（外面据此决定投递列表排几列） */
  onOpenChange?: (open: boolean) => void
}

/** 回执只在自己发的气泡上显示，用颜色区分已读 / 送达 / 未读 */
const RECEIPT_TEXT: Record<string, string> = { read: '已读', delivered: '已送达', unread: '未读' }
const RECEIPT_CLS: Record<string, string> = {
  read: 'text-emerald-600 dark:text-emerald-400',
  delivered: 'text-slate-400',
  unread: 'text-amber-600 dark:text-amber-400',
}

/** 这些 kind 是 Boss 的系统事件（附件简历/图片/职位卡片/语音），不画成对话气泡 */
const SYS_KINDS = new Set(['2', '3', '5', '6', '7', '8'])

const receiptOf = (m: ChatMessage): string =>
  m.from_me && m.status && RECEIPT_TEXT[m.status] ? m.status : ''

export default function ChatPanel({ focusUid, onSent, onTokens, onOpenChange }: ChatPanelProps) {
  // 收起时只留最右边一条竖栏；单击竖栏或任一岗位的「展开」按钮时向左滑出
  const [open, setOpen] = useState(false)
  const cbRef = useRef(onOpenChange)
  cbRef.current = onOpenChange
  useEffect(() => {
    cbRef.current?.(open)
  }, [open])
  const [genning, setGenning] = useState(false)
  // 附件简历选择面板
  const [resumeOpen, setResumeOpen] = useState(false)
  const [resumes, setResumes] = useState<ResumeItem[]>([])
  const [resumesLoading, setResumesLoading] = useState(false)
  const [resumesErr, setResumesErr] = useState('')
  const [resumeSending, setResumeSending] = useState('')
  const [genNote, setGenNote] = useState('')
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
        // 抓取 JD 时后端直接给上次缓存，标注 stale，别让用户以为列表卡住了
        if (r.stale) flash(`正在${r.busy || '忙别的任务'}，下面显示的是上次拉到的列表`)
      }
    } catch (e: any) {
      setFriendsErr(String(e?.message || e))
      if (_friendCache) setFriends(_friendCache.items)
    } finally {
      setFriendsLoading(false)
    }
  }, [])

  // 列表常显，挂载即拉一次（模块级缓存命中则不发请求）
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
        if (r.stale) flash(`正在${r.busy || '忙别的任务'}，聊天记录是上次拉到的`)
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
      setOpen(true)
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
    setOpen(true)
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

  // ---------- 生成回复（AI 按 提示词 + 岗位 JD + 我的技能 写一句打招呼） ----------
  const doGenerate = async () => {
    if (!active || genning) return
    setGenning(true)
    setGenNote('')
    try {
      const r = await apiClient.greet({
        uid: String(active.uid),
        encrypt_job_id: active.encrypt_job_id || undefined,
      })
      setDraft(r.text)
      if (r.note) setGenNote(r.note)
      else if (r.tokens) setGenNote(`已生成（消耗 ${r.tokens} token）`)
      else setGenNote('已生成')
      onTokens?.({ total: r.tokens_total, greet: r.tokens_greet })
    } catch (e: any) {
      setGenNote(`生成失败：${String(e?.message || e)}`)
    } finally {
      setGenning(false)
    }
  }

  // ---------- 发送附件简历 ----------
  // 先展开「选择哪份简历」的面板，等用户点某一份时再真正去 Boss 读取并发送
  const toggleResumes = () => {
    if (!active) return
    setResumesErr('')
    setResumeOpen((v) => !v)
  }

  /** 读取 Boss 上的附件简历列表（只在真正要发的时候才拉） */
  const loadResumes = async (): Promise<ResumeItem[]> => {
    setResumesLoading(true)
    setResumesErr('')
    try {
      const r = await apiClient.chatResumes(false)
      if (!r.ok) {
        setResumesErr(r.error || '读取简历列表失败')
        return []
      }
      const items = r.items || []
      setResumes(items)
      if (items.length === 0) setResumesErr('Boss 上还没有附件简历，请先在 Boss 网页端上传')
      return items
    } catch (e: any) {
      setResumesErr(String(e?.message || e))
      return []
    } finally {
      setResumesLoading(false)
    }
  }

  const sendResume = async (r: ResumeItem) => {
    if (!active || resumeSending) return
    // 列表还没读出来就点「发送」：先拉一次（面板里渲染的是占位那一份）
    let target = r
    if (!resumes.length) {
      const items = await loadResumes()
      if (items.length === 0) return
      target = items[0]
    }
    setResumeSending(target.id)
    setResumesErr('')
    try {
      const res = await apiClient.chatSendResume(String(active.uid), target.id)
      if (!res.ok) {
        setResumesErr(res.error || '发送简历失败')
        return
      }
      setResumeOpen(false)
      flash(`已发送简历「${target.name}」，该岗位已加入「已投递」`)
      await loadMessages(String(active.uid))
      onSent?.()
    } catch (e: any) {
      setResumesErr(String(e?.message || e))
    } finally {
      setResumeSending('')
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
    <div className="flex min-h-0 flex-1 gap-3">
      {/* ========== Boss 消息列表：始终显示 ========== */}
      <div className={`flex min-w-0 flex-col overflow-hidden ${open ? 'flex-[1.2]' : 'flex-1'}`}>
        <div className="flex shrink-0 items-center gap-2 px-1 py-2">
          <h2 className="shrink-0 whitespace-nowrap text-xs font-semibold">Boss 消息</h2>
          <span className="shrink-0 text-[10px] text-slate-400">{friends.length}</span>
          <button
            className="ml-auto shrink-0 whitespace-nowrap btn-ghost !px-1.5 !py-0.5 text-[11px]"
            title="从 Boss 重新拉取会话列表"
            onClick={() => loadFriends(true)}
            disabled={friendsLoading}
          >
            {friendsLoading ? '读取中…' : '刷新'}
          </button>
          <button
            className="shrink-0 whitespace-nowrap btn-ghost !px-1.5 !py-0.5 text-[11px]"
            title={open ? '收起对话框，列表保留' : '展开对话框'}
            onClick={() => setOpen((v) => !v)}
          >
            {open ? '收起对话框 ▶' : '◀ 对话框'}
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
                className={`mb-1 flex w-full items-center gap-2.5 rounded-lg border-l-[3px] px-2 py-2.5 text-left transition ${
                  on
                    ? 'border-primary bg-primary/10 font-semibold shadow-sm ring-1 ring-primary/30 dark:bg-primary/15'
                    : 'border-transparent hover:bg-slate-100/80 dark:hover:bg-slate-800/40'
                }`}
              >
                <Avatar f={f} size={on ? 38 : 34} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline gap-1.5">
                    <span className={`truncate text-[12px] ${on ? 'font-semibold' : 'font-medium'}`}>
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

      {/* ========== 对话框：可收起，收起后列表独占 ========== */}
      {open && (
      <div className="flex min-w-0 flex-[2] flex-col overflow-hidden">
        {/* 顶栏固定：标题 + 在招岗位，不是卡片，只是清楚的分区 */}
        <div className="flex shrink-0 items-center gap-2 px-1 py-2">
          <h2 className="text-xs font-semibold">对话框</h2>
          {active && (
            <>
              <span className="text-[10px] text-slate-400">
                <span className="font-medium">
                  {clean(active.name)}
                </span>{' '}
                · {clean(active.brand)}
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
                // 附件简历 / 图片 / 职位卡片本身就是系统事件，无论接口怎么标都画成居中胶囊
                if (SYS_KINDS.has(String(m.kind ?? '')) || /^附件简历$|^图片$|^职位卡片/.test(clean(m.text))) {
                  return (
                    <div key={m.id || i} className="flex justify-center">
                      <span className="rounded-full bg-slate-100/80 px-2 py-0.5 text-[10px] text-slate-500 dark:bg-slate-800/60 dark:text-slate-400">
                        {clean(m.text)}
                      </span>
                    </div>
                  )
                }
                if (m.system) {
                  return (
                    <div key={m.id || i} className="flex justify-center">
                      <span className="rounded-full bg-slate-100/80 px-2 py-0.5 text-[10px] text-slate-500 dark:bg-slate-800/60 dark:text-slate-400">
                        {clean(m.text)}
                      </span>
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
                      <div className={`flex max-w-[80%] flex-col gap-0.5 ${m.from_me ? 'items-end' : 'items-start'}`}>
                        <div
                          className={`whitespace-pre-wrap break-words rounded-xl px-3 py-1.5 text-[12px] leading-5 shadow-sm ${
                            m.from_me
                              ? 'rounded-tr-sm bg-[#95ec69] text-[#1f2d3d] dark:bg-[#3a5a3c] dark:text-slate-100'
                              : 'rounded-tl-sm border border-slate-200 bg-white text-slate-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200'
                          }`}
                        >
                          {clean(m.text)}
                        </div>
                        {receiptOf(m) && (
                          <span
                            className={`text-[9px] leading-3 ${RECEIPT_CLS[receiptOf(m)]}`}
                            title={
                              receiptOf(m) === 'read'
                                ? '对方已读'
                                : receiptOf(m) === 'delivered'
                                  ? '已送达，对方还没读'
                                  : '对方还没看'
                            }
                          >
                            {RECEIPT_TEXT[receiptOf(m)]}
                          </span>
                        )}
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>

            {/* 输入区 */}
            <div className="shrink-0 border-t border-slate-100 p-2 dark:border-dark-border">
              {genNote && !sendErr && (
                <div className="mb-1 rounded bg-sky-50 px-2 py-1 text-[10px] leading-4 text-sky-700 dark:bg-sky-950/30 dark:text-sky-300">
                  {genNote}
                </div>
              )}
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
              <div className="mb-1 flex items-center gap-2">
                <button
                  className="btn-primary !px-2 !py-0.5 text-[10px]"
                  onClick={doGenerate}
                  disabled={genning}
                  title="按「模型配置」里的提示词，结合岗位 JD 与我的技能生成一句打招呼，填进下面的输入框"
                >
                  {genning ? '生成中…' : '生成回复'}
                </button>
                <button
                  className="btn-ghost !px-2 !py-0.5 text-[10px]"
                  onClick={toggleResumes}
                  title="选择一份附件简历发给这位 HR"
                >
                  {resumeOpen ? '收起简历' : '发送简历'}
                </button>
                <span className="truncate text-[10px] text-slate-400">
                  按提示词 + 岗位 JD + 我的技能生成，可直接改再发送
                </span>
              </div>
              {resumeOpen && (
                <div className="mb-1 rounded-lg border border-slate-200 bg-white p-1.5 dark:border-dark-border dark:bg-slate-900/60">
                  {resumesErr && (
                    <div className="mb-1 rounded bg-rose-50 px-1.5 py-1 text-[10px] leading-4 text-rose-600 dark:bg-rose-950/30">
                      {resumesErr}
                    </div>
                  )}
                  {resumesLoading && (
                    <div className="px-1 py-2 text-[10px] text-slate-400">
                      正在从 Boss 读取你的附件简历…
                    </div>
                  )}
                  {!resumesLoading && resumes.length === 0 && (
                    <button
                      className="btn-ghost mb-1 w-full !justify-start !px-1.5 !py-1 text-[11px]"
                      onClick={() => loadResumes()}
                    >
                      📄 读取 Boss 上的附件简历并发送
                    </button>
                  )}
                  {resumes.map((r) => (
                    <button
                      key={r.id}
                      className="flex w-full items-center gap-2 rounded px-1.5 py-1 text-left hover:bg-slate-100 dark:hover:bg-slate-800/60"
                      onClick={() => sendResume(r)}
                      disabled={!!resumeSending}
                    >
                      <span className="text-[12px]">📄</span>
                      <span className="min-w-0 flex-1 truncate text-[11px]">{r.name}</span>
                      <span className="shrink-0 text-[10px] text-slate-400">{r.size}</span>
                      <span className="shrink-0 text-[10px] text-slate-400">
                        {resumeSending === r.id ? '发送中…' : '发送'}
                      </span>
                    </button>
                  ))}
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
      )}

    </div>
  )
}
