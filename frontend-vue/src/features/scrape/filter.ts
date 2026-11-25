/**
 * 筛选草稿的**本地兜底**。纯逻辑在 `lib_ui/scrape-filter-model.ts`，这里只留碰宿主环境的
 * `localStorage` 读写。顺序不变：后端 `scrape_defaults` 优先，`applyBackend()` 返回 0 时
 * 才回落本地草稿。**不要**改 `DRAFT_KEY` 的名字——老版本写下的草稿要能被读出来。
 */
import {
  DRAFT_KEY,
  KEYWORDS_KEY,
  decode,
  decodeKeywords,
  encode,
  encodeKeywords,
  type FilterDraft,
} from '../../lib_ui/scrape-filter-model';

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

/**
 * 读本地搜索关键词。关键词不进后端 `scrape_defaults`，只能本地存；坏数据按空表处理。
 */
export function loadKeywords(): string[] {
  const s = storage();
  if (!s) return [];
  try {
    return decodeKeywords(s.getItem(KEYWORDS_KEY));
  } catch {
    return [];
  }
}

/** 写本地搜索关键词；失败静默，和草稿同一姿势。 */
export function saveKeywords(list: readonly string[]): void {
  const s = storage();
  if (!s) return;
  try {
    s.setItem(KEYWORDS_KEY, encodeKeywords(list));
  } catch {
    /* 忽略：本地存储只是让 F5 之后还能看见，不是权威 */
  }
}
