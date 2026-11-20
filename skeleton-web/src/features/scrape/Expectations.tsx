/**
 * 求职期望：读 Boss 账号里配置过的期望职位，一键填进抓取关键词。
 *
 * 为什么单独做一块而不是并进抓取面板：
 * - 它**依赖浏览器已启动且已登录**，抓取面板的按钮在未登录时也能点（会报错），
 *   两种前置条件的提示语完全不同，混在一起会让人误以为抓取坏了。
 * - 数据来源是账号配置，不属于抓取过程的进度，塞进抓取面板会让那块越来越长。
 *
 * 失败分两种，提示必须区分：
 * - sidecar 没起（没有会话）→ **自己拉起**（`session_open`），不要让用户先去抓取
 * - 会话在但没登录 → 让用户去扫码
 * 读取期望本身不需要抓取，所以这里先确保浏览器可用再读，而不是把
 * 「请先开始抓取」这种实现细节甩给用户。
 */
import { useState } from 'react';
import { api, errText, type Expectation } from '../../api';
import { Card, Chip } from '../../components/ui';

/** 判断错误是不是「sidecar 还没起来」，是的话值得先拉起再重试。 */
function needsSession(msg: string): boolean {
  return msg.includes('尚未启动') || msg.includes('没有会话');
}

export function Expectations(props: {
  /** 把期望职位名填进抓取关键词；父组件决定是覆盖还是追加。 */
  onPick: (names: string[]) => void;
  fail: (e: unknown) => void;
}) {
  const [items, setItems] = useState<Expectation[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState('');

  const load = async () => {
    setBusy(true);
    setNote('');
    try {
      let list: Expectation[];
      try {
        list = await api.listExpectations();
      } catch (e) {
        // 没会话就先把浏览器拉起来再试一次——读期望不需要抓取，不该逼用户先抓
        if (!needsSession(errText(e))) throw e;
        setNote('正在启动浏览器…');
        await api.sessionOpen();
        list = await api.listExpectations();
      }
      setItems(list);
      setNote(
        list.length
          ? `读到 ${list.length} 条期望职位`
          : '账号里没有配置期望职位，或登录已失效（可在 Boss 网页端「求职期望」里补一条）',
      );
    } catch (e) {
      // 这里不走全局错误条：读取失败是这块自己的事，报在卡片里更容易看懂
      setItems(null);
      setNote(errText(e));
      props.fail(e);
    } finally {
      setBusy(false);
    }
  };

  const names = (items ?? []).map((x) => x.name).filter(Boolean);

  return (
    <Card
      testId="expect-panel"
      title="求职期望"
      subtitle="读账号配置，可一键填进抓取关键词"
      actions={
        <button className="btn btn-sm" data-testid="expect-load-btn" onClick={load} disabled={busy}>
          {busy ? '读取中…' : items ? '刷新' : '读取'}
        </button>
      }
    >
      {items === null ? (
        <p className="hint" data-testid="expect-empty">
          {note || '点「读取」从 Boss 账号取期望职位。需要浏览器已启动且已登录（先点抓取里的「扫码登录」）。'}
        </p>
      ) : (
        <>
          <div className="row wrap" style={{ gap: 6 }}>
            {items.length === 0 ? (
              <span className="hint" data-testid="expect-empty">
                {note}
              </span>
            ) : (
              items.map((x) => (
                <Chip
                  key={`${x.code}-${x.name}`}
                  tone="accent"
                  title={[x.city, x.salary].filter(Boolean).join(' · ') || undefined}
                >
                  <span data-testid="expect-item">{x.name}</span>
                  {x.city ? <span className="dim"> · {x.city}</span> : null}
                </Chip>
              ))
            )}
          </div>
          {note && items.length > 0 ? <p className="hint">{note}</p> : null}
          {names.length > 0 ? (
            <div className="row wrap" style={{ marginTop: 10, gap: 8 }}>
              <button
                className="btn btn-primary btn-sm"
                data-testid="expect-apply-btn"
                onClick={() => props.onPick(names)}
              >
                填入抓取关键词
              </button>
              <span className="hint">会追加到现有关键词里，不覆盖你手填的</span>
            </div>
          ) : null}
        </>
      )}
    </Card>
  );
}
