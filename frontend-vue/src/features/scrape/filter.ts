/**
 * 筛选草稿的**本地兜底**（编排层）。
 *
 * 纯逻辑（草稿结构、编解码、diff、标签常量）已经住在 `lib_ui/scrape-filter-model.ts`，
 * 展示组件只消费那一份；这里只留会碰宿主环境的两个动作：
 * `localStorage` 读写（浏览器 / WebView 才有）。
 *
 * C12 的顺序不变：后端 `scrape_defaults` 优先，`applyBackend()` 返回 0 时
 * 才回落到这里的本地草稿。**不要**把 `DRAFT_KEY` 改名——老版本写下的草稿要能被读出来。
 */
import { DRAFT_KEY, decode, encode, type FilterDraft } from '../../lib_ui/scrape-filter-model';

interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function storage(): StorageLike | null {
  try {
    const w = globalThis as { localStorage?: StorageLike };
    return w.localStorage ?? null;
  } catch {
    return null;
  }
}

/** 读本地草稿；没有或坏掉都返回 `null`（由调用方决定用不用默认值）。 */
export function loadDraft(): FilterDraft | null {
  const s = storage();
  if (!s) return null;
  try {
    const raw = s.getItem(DRAFT_KEY);
    return raw ? decode(raw) : null;
  } catch {
    return null;
  }
}

/** 写本地草稿。失败（隐私模式 / 配额满）静默——本地只是兜底，不值得打扰用户。 */
export function saveDraft(d: FilterDraft): void {
  const s = storage();
  if (!s) return;
  try {
    s.setItem(DRAFT_KEY, encode(d));
  } catch {
    /* 忽略：本地草稿是兜底，不是权威 */
  }
}