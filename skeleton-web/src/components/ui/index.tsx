/**
 * 通用展示组件（设计系统层）。
 *
 * 铁律：**这里只放纯展示**，不得出现任何业务判定、`invoke` 调用或状态管理。
 * 一旦某个组件开始知道「什么是门槛」「什么是抓取」，它就该移到 `features/` 下。
 */

import type { ReactNode } from 'react';

/** 毛玻璃卡片，带可选标题栏。 */
export function Card(props: {
  title?: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  testId?: string;
  className?: string;
}) {
  return (
    <section className={`card ${props.className ?? ''}`} data-testid={props.testId}>
      {(props.title || props.actions) && (
        <header className="card-head">
          {props.title && <h2 className="card-title">{props.title}</h2>}
          {props.subtitle && <span className="card-sub">{props.subtitle}</span>}
          <span className="spacer" />
          {props.actions}
        </header>
      )}
      <div className="card-body">{props.children}</div>
    </section>
  );
}

/** 统计胶囊：抓取进度的「已抓 / 入库 / 重复 / 低于门槛 / 否决」。 */
export function Stat(props: {
  label: ReactNode;
  value: ReactNode;
  tone?: 'good' | 'bad' | 'warn';
  testId?: string;
}) {
  return (
    <div className="stat" data-testid={props.testId}>
      <div className="stat-label">{props.label}</div>
      <div className={`stat-value ${props.tone ? `is-${props.tone}` : ''}`}>{props.value}</div>
    </div>
  );
}

export type ChipTone = 'default' | 'primary' | 'accent' | 'warn' | 'bad';

export function Chip(props: { children: ReactNode; tone?: ChipTone; title?: string }) {
  const tone = props.tone && props.tone !== 'default' ? `chip-${props.tone}` : '';
  return (
    <span className={`chip ${tone}`} title={props.title}>
      {props.children}
    </span>
  );
}

/** 运行状态点 + 文案。 */
export function StatusDot(props: { state: 'running' | 'idle' | 'warn' | 'bad'; label: ReactNode }) {
  return (
    <span className="row" style={{ gap: 6, fontSize: 12.5, color: 'var(--text-2)' }}>
      <span className={`dot is-${props.state}`} />
      {props.label}
    </span>
  );
}

/** 列表加载中的骨架行，避免「先闪空态再出数据」。 */
export function SkeletonRows(props: { cols: number; rows?: number }) {
  return (
    <>
      {Array.from({ length: props.rows ?? 6 }).map((_, r) => (
        <tr key={r} className="skeleton-row">
          {Array.from({ length: props.cols }).map((__, c) => (
            <td key={c}>
              <div className="skeleton-bar" style={{ width: `${[36, 62, 44, 30, 24][c % 5]}%` }} />
            </td>
          ))}
        </tr>
      ))}
    </>
  );
}

/** 错误条：一行说明 + 「忽略」。`onDismiss` 为空则不给按钮。 */
export function ErrorBar(props: { message: string; onDismiss?: () => void; testId?: string }) {
  return (
    <div className="error-bar" data-testid={props.testId}>
      <b style={{ flexShrink: 0 }}>错误</b>
      <span style={{ flex: 1, wordBreak: 'break-word' }}>{props.message}</span>
      {props.onDismiss && (
        <button className="btn btn-sm btn-ghost" data-testid="app-error-dismiss" onClick={props.onDismiss}>
          忽略
        </button>
      )}
    </div>
  );
}

/** 提示横幅：比错误条温和，4 秒后自行消失（由调用方控制）。 */
export function Banner(props: { children: ReactNode; action?: ReactNode; testId?: string }) {
  return (
    <div className="banner" data-testid={props.testId}>
      <span style={{ flex: 1 }}>{props.children}</span>
      {props.action}
    </div>
  );
}

/** 表单字段：标签 + 输入。`hint` 是标签右侧的灰字说明。 */
export function Field(props: {
  label: ReactNode;
  hint?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`field ${props.className ?? ''}`}>
      <div className="label">
        {props.label}
        {props.hint && <span className="field-hint">{props.hint}</span>}
      </div>
      {props.children}
    </div>
  );
}

/** 折叠分组：标题整行可点（`.section-label` 的样式依赖这个结构）。 */
export function Collapsible(props: {
  title: ReactNode;
  subtitle?: ReactNode;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
  testId?: string;
}) {
  return (
    <div className="config-group" data-testid={props.testId}>
      <button className="section-label" type="button" onClick={props.onToggle} aria-expanded={props.open}>
        <span className={`caret ${props.open ? 'is-open' : ''}`} />
        {props.title}
        {props.subtitle && <span className="card-sub">{props.subtitle}</span>}
      </button>
      {props.open && <div className="config-body">{props.children}</div>}
    </div>
  );
}

/** 空态：一行说明，可选一个动作按钮。 */
export function EmptyState(props: { children: ReactNode; action?: ReactNode; testId?: string }) {
  return (
    <div className="empty-state" data-testid={props.testId}>
      <span>{props.children}</span>
      {props.action}
    </div>
  );
}
