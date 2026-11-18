"""SQLite 存储层。

按 PRD 第 4 节定义四张表，并为 JD 补齐打分所需的公司侧字段：
company_scale / financing_stage / hiring_count / founded_years / hr_active_status 等。
"""
from __future__ import annotations

import json
import sqlite3
import threading
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator

DB_NAME = "job_assistant_pro.db"

SCHEMA = """
CREATE TABLE IF NOT EXISTS job_queue (
    job_id            TEXT PRIMARY KEY,
    platform          TEXT NOT NULL,
    title             TEXT,
    company           TEXT,
    salary            TEXT,
    status            TEXT NOT NULL DEFAULT 'pending',
    created_at        INTEGER NOT NULL,

    -- 以下为打分需要的 JD / 公司侧扩展字段
    job_url           TEXT,
    jd_text           TEXT,
    location          TEXT,
    experience        TEXT,
    degree            TEXT,
    hr_name           TEXT,
    hr_position       TEXT,
    hr_active_status  TEXT,
    company_industry  TEXT,
    company_scale     TEXT,
    company_stage     TEXT,
    company_address   TEXT,
    company_intro     TEXT,
    hiring_count      INTEGER,
    founded_years     INTEGER,
    match_score       INTEGER,
    rejected          INTEGER DEFAULT 0,
    score_source      TEXT,
    score_reason      TEXT,
    updated_at        INTEGER
);

CREATE TABLE IF NOT EXISTS jd_decisions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    platform    TEXT,
    company_job TEXT,
    decision    TEXT,
    reason      TEXT,
    created_at  INTEGER NOT NULL
);

-- 敏感库：所有被抓过、但判定"不投递"的岗位都留档，下一轮抓取前先查这里直接跳过
CREATE TABLE IF NOT EXISTS sensitive_jobs (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    platform       TEXT NOT NULL,          -- 网站来源：boss / zhilian / 51job
    job_id         TEXT,                   -- 平台内岗位 ID
    company        TEXT,
    title          TEXT,
    salary         TEXT,
    location       TEXT,
    reason         TEXT,                   -- 排除原因
    job_url        TEXT,
    kind           TEXT DEFAULT 'rejected',  -- rejected=被排除 / stored=已入库
    created_at     INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS uk_sensitive_jobs ON sensitive_jobs(platform, company, title);
CREATE INDEX IF NOT EXISTS idx_sensitive_jobs_at ON sensitive_jobs(created_at DESC);

CREATE TABLE IF NOT EXISTS app_config (
    key    TEXT PRIMARY KEY,
    value  TEXT
);

CREATE INDEX IF NOT EXISTS idx_job_queue_status ON job_queue(status);
CREATE INDEX IF NOT EXISTS idx_job_queue_score  ON job_queue(match_score DESC);
CREATE INDEX IF NOT EXISTS idx_job_queue_plat   ON job_queue(platform);
CREATE INDEX IF NOT EXISTS idx_jd_decisions_at  ON jd_decisions(created_at DESC);
"""

# 老库升级时按需补的列（SQLite 无 ADD COLUMN IF NOT EXISTS）
_UPGRADE_COLUMNS = {
    "job_url": "TEXT",
    "jd_text": "TEXT",
    "location": "TEXT",
    "experience": "TEXT",
    "degree": "TEXT",
    "hr_name": "TEXT",
    "hr_position": "TEXT",
    "hr_active_status": "TEXT",
    "company_industry": "TEXT",
    "company_scale": "TEXT",
    "company_stage": "TEXT",
    "company_address": "TEXT",
    "company_intro": "TEXT",
    "hiring_count": "INTEGER",
    "founded_year": "INTEGER",
    "founded_years": "INTEGER",
    "reg_capital": "TEXT",
    "hr_active_time": "INTEGER",
    "token_used": "INTEGER DEFAULT 0",
    "match_score": "INTEGER",
    "rejected": "INTEGER DEFAULT 0",
    "score_source": "TEXT",
    "score_reason": "TEXT",
    "updated_at": "INTEGER",
}


class Store:
    """线程安全的轻量 SQLite 封装（每线程独立连接）。"""

    def __init__(self, db_path: Path | str) -> None:
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._local = threading.local()
        with self.conn() as c:
            c.executescript(SCHEMA)
            self._upgrade(c)

    # ---------- 连接 ----------
    def conn(self) -> sqlite3.Connection:
        c = getattr(self._local, "conn", None)
        if c is None:
            c = sqlite3.connect(self.db_path, timeout=30, check_same_thread=False)
            c.row_factory = sqlite3.Row
            c.execute("PRAGMA journal_mode=WAL")
            c.execute("PRAGMA synchronous=NORMAL")
            self._local.conn = c
        return c

    @contextmanager
    def tx(self) -> Iterator[sqlite3.Connection]:
        c = self.conn()
        try:
            yield c
            c.commit()
        except Exception:
            c.rollback()
            raise

    def _upgrade(self, c: sqlite3.Connection) -> None:
        cols = {r["name"] for r in c.execute("PRAGMA table_info(job_queue)")}
        for name, decl in _UPGRADE_COLUMNS.items():
            if name not in cols:
                c.execute(f"ALTER TABLE job_queue ADD COLUMN {name} {decl}")
        # 敏感库新增 kind 列（老库补列）：rejected=被排除 / stored=已入库（避免重复抓取）
        scols = {r["name"] for r in c.execute("PRAGMA table_info(sensitive_jobs)")}
        if "kind" not in scols:
            try:
                c.execute("ALTER TABLE sensitive_jobs ADD COLUMN kind TEXT DEFAULT 'rejected'")
            except Exception:
                pass
        c.commit()

    def close(self) -> None:
        c = getattr(self._local, "conn", None)
        if c is not None:
            c.close()
            self._local.conn = None

    # ---------- job_queue ----------
    def enqueue_job(self, job: dict[str, Any]) -> bool:
        """插入岗位；已存在则跳过（同一平台同一 job_id 视为已处理）。

        返回 True 表示新插入，False 表示重复跳过。
        """
        now = int(time.time())
        job.setdefault("created_at", now)
        job["updated_at"] = now
        cols = [k for k in job if k in self._job_columns()]
        placeholders = ",".join("?" * len(cols))
        sql = (
            f"INSERT INTO job_queue ({','.join(cols)}) VALUES ({placeholders}) "
            "ON CONFLICT(job_id) DO NOTHING"
        )
        with self.tx() as c:
            cur = c.execute(sql, [job[k] for k in cols])
            return cur.rowcount > 0

    def _job_columns(self) -> set[str]:
        if not hasattr(self, "_job_cols_cache"):
            rows = self.conn().execute("PRAGMA table_info(job_queue)").fetchall()
            self._job_cols_cache = {r["name"] for r in rows}
        return self._job_cols_cache

    def exists(self, job_id: str) -> bool:
        row = self.conn().execute(
            "SELECT 1 FROM job_queue WHERE job_id = ? LIMIT 1", (job_id,)
        ).fetchone()
        return row is not None

    def update_job(self, job_id: str, **fields: Any) -> None:
        if not fields:
            return
        fields["updated_at"] = int(time.time())
        cols = [k for k in fields if k in self._job_columns()]
        assigns = ",".join(f"{c} = ?" for c in cols)
        with self.tx() as c:
            c.execute(
                f"UPDATE job_queue SET {assigns} WHERE job_id = ?",
                [fields[c] for c in cols] + [job_id],
            )

    def get_job(self, job_id: str) -> dict[str, Any] | None:
        row = self.conn().execute(
            "SELECT * FROM job_queue WHERE job_id = ?", (job_id,)
        ).fetchone()
        return dict(row) if row else None

    def list_jobs(
        self,
        status: str | None = None,
        platform: str | None = None,
        min_score: int | None = None,
        limit: int = 100,
        offset: int = 0,
        sort: str = "score",
        order: str = "desc",
        include_rejected: bool = True,
        rejected_only: bool = False,
    ) -> list[dict[str, Any]]:
        sql = "SELECT * FROM job_queue WHERE 1=1"
        args: list[Any] = []
        if status:
            sql += " AND status = ?"
            args.append(status)
        if platform:
            sql += " AND platform = ?"
            args.append(platform)
        if min_score is not None:
            sql += " AND COALESCE(match_score, 0) >= ?"
            args.append(min_score)
        if rejected_only:
            sql += " AND COALESCE(rejected, 0) = 1"
        elif not include_rejected:
            sql += " AND COALESCE(rejected, 0) = 0"
        # 「不投递」只在未投递里才算一种独立状态；已投递的岗位不计入不投递页签
        if rejected_only and (status or "pending") == "pending":
            sql += " AND COALESCE(status, 'pending') = 'pending'"
        # 排序：只允许白名单列，避免 SQL 注入。
        # salary 列存的是 "20-35K" / "35-65K·15薪" / "2.5-4万" 这类文本，
        # 旧实现用 substr 取前 3 位，导致 "35-65K" 被当成 35、"1-2万" 被当成 1，排序错乱。
        # 这里用正则把薪资上下限抽成数值再排序（单位统一成 K）。
        salary_k = (
            "CASE "
            "WHEN salary IS NULL OR TRIM(salary) = '' OR salary LIKE '%面议%' THEN -1 "
            "WHEN salary LIKE '%万%' THEN "
            "  CAST(COALESCE(SUBSTR(salary, 1, INSTR(salary, '万') - 1), '0') AS REAL) * 10 "
            "ELSE "
            "  CAST(COALESCE(SUBSTR(TRIM(salary), 1, INSTR(TRIM(salary) || '-', '-') - 1), '0') AS REAL) "
            "END"
        )
        cols = {
            "score": "COALESCE(match_score, 0)",
            "time": "created_at",
            # 主排序按薪资下限，同下限时按字符串兜底，保证稳定
            "salary": salary_k,
        }
        col = cols.get(sort, cols["score"])
        direction = "ASC" if str(order).lower() == "asc" else "DESC"
        sql += f" ORDER BY {col} {direction}, created_at DESC LIMIT ? OFFSET ?"
        args.extend([int(limit), int(max(0, offset))])
        return [dict(r) for r in self.conn().execute(sql, args).fetchall()]

    def count_jobs(
        self,
        status: str | None = None,
        platform: str | None = None,
        min_score: int | None = None,
        include_rejected: bool = True,
        rejected_only: bool = False,
    ) -> int:
        sql = "SELECT COUNT(*) AS n FROM job_queue WHERE 1=1"
        args: list[Any] = []
        if status:
            sql += " AND status = ?"
            args.append(status)
        if platform:
            sql += " AND platform = ?"
            args.append(platform)
        if min_score is not None:
            sql += " AND COALESCE(match_score, 0) >= ?"
            args.append(min_score)
        if rejected_only:
            sql += " AND COALESCE(rejected, 0) = 1"
        elif not include_rejected:
            sql += " AND COALESCE(rejected, 0) = 0"
        if rejected_only and (status or "pending") == "pending":
            sql += " AND COALESCE(status, 'pending') = 'pending'"
        row = self.conn().execute(sql, args).fetchone()
        return int(row["n"]) if row else 0

    def set_rejected(self, job_ids: list[str], rejected: bool = True) -> int:
        """标记不投递 / 恢复允许投递。

        不投递只是打标记（rejected=1），记录仍保留在列表里可见，
        之后重新抓到也不会重新进入待投递流程（与「已投递」同等处理）。
        """
        if not job_ids:
            return 0
        marks = ",".join("?" for _ in job_ids)
        with self.tx() as c:
            cur = c.execute(
                f"UPDATE job_queue SET rejected = ?, updated_at = ? WHERE job_id IN ({marks})",
                [1 if rejected else 0, int(time.time())] + list(job_ids),
            )
            return cur.rowcount

    def set_delivered(self, job_ids: list[str], delivered: bool = True) -> int:
        """标记已投递 / 撤销已投递。已投递的记录在列表里置灰，且不再参与投递。"""
        if not job_ids:
            return 0
        marks = ",".join("?" for _ in job_ids)
        now = int(time.time())
        with self.tx() as c:
            cur = c.execute(
                f"UPDATE job_queue SET status = ?, updated_at = ? WHERE job_id IN ({marks})",
                ["delivered" if delivered else "pending", now] + list(job_ids),
            )
            return cur.rowcount

    def unscored_jobs(self) -> list[dict[str, Any]]:
        return [
            dict(r)
            for r in self.conn().execute(
                "SELECT * FROM job_queue WHERE match_score IS NULL"
            ).fetchall()
        ]

    def stats(self) -> dict[str, Any]:
        c = self.conn()
        total = c.execute("SELECT COUNT(*) n FROM job_queue").fetchone()["n"]
        by_status = {
            r["status"]: r["n"]
            for r in c.execute(
                "SELECT status, COUNT(*) n FROM job_queue GROUP BY status"
            ).fetchall()
        }
        by_platform = {
            r["platform"]: r["n"]
            for r in c.execute(
                "SELECT platform, COUNT(*) n FROM job_queue GROUP BY platform"
            ).fetchall()
        }
        scored = c.execute(
            "SELECT COUNT(*) n FROM job_queue WHERE match_score IS NOT NULL"
        ).fetchone()["n"]
        avg = c.execute(
            "SELECT AVG(match_score) v FROM job_queue WHERE match_score IS NOT NULL"
        ).fetchone()["v"]
        return {
            "total": total,
            "scored": scored,
            "avg_score": round(avg, 2) if avg is not None else None,
            "by_status": by_status,
            "by_platform": by_platform,
        }

    # ---------- sensitive_jobs（敏感库 / 已抓取库） ----------
    def clear_sensitive(self) -> int:
        """把敏感库清空（界面上「清空」按钮用）。"""
        with self._lock:
            cur = self._conn().execute("DELETE FROM sensitive_jobs")
            self._conn().commit()
            return cur.rowcount or 0

    def add_sensitive(
        self,
        platform: str,
        company: str | None,
        title: str | None,
        reason: str | None,
        job_id: str | None = None,
        salary: str | None = None,
        location: str | None = None,
        job_url: str | None = None,
        kind: str = "rejected",
    ) -> bool:
        """记录一条岗位（kind: rejected=被排除 / stored=已入库）。

        同一平台 + 公司 + 岗位只留一条；重复时把 kind 升级为 stored（已入库优先）。
        """
        now = int(time.time())
        with self.tx() as c:
            cur = c.execute(
                "INSERT OR IGNORE INTO sensitive_jobs"
                " (platform, job_id, company, title, salary, location, reason, job_url, kind, created_at)"
                " VALUES (?,?,?,?,?,?,?,?,?,?)",
                (platform, job_id, company, title, salary, location, reason, job_url, kind, now),
            )
            if cur.rowcount == 0 and kind == "stored":
                c.execute(
                    "UPDATE sensitive_jobs SET kind='stored' WHERE platform=? AND company=? AND title=?",
                    (platform, company, title),
                )
            return cur.rowcount > 0

    def sync_stored(self) -> int:
        """把 job_queue 里已有的岗位补进敏感库（kind='stored'），返回补入条数。"""
        now = int(time.time())
        n = 0
        with self.tx() as c:
            rows = c.execute(
                "SELECT platform, job_id, company, title, salary, location, job_url FROM job_queue"
            ).fetchall()
            for r in rows:
                cur = c.execute(
                    "INSERT OR IGNORE INTO sensitive_jobs"
                    " (platform, job_id, company, title, salary, location, reason, job_url, kind, created_at)"
                    " VALUES (?,?,?,?,?,?,?,?,?,?)",
                    (r["platform"], r["job_id"], r["company"], r["title"], r["salary"],
                     r["location"], "已入库（历史数据）", r["job_url"], "stored", now),
                )
                if cur.rowcount == 0:
                    c.execute(
                        "UPDATE sensitive_jobs SET kind='stored' WHERE platform=? AND company=? AND title=?",
                        (r["platform"], r["company"], r["title"]),
                    )
                else:
                    n += 1
        return n

    def sensitive_hit(
        self, platform: str, company: str | None, title: str | None
    ) -> dict[str, Any] | None:
        """精确查重：同平台 + 同公司 + 同岗位名。"""
        if not company or not title:
            return None
        with self.conn() as c:
            row = c.execute(
                "SELECT * FROM sensitive_jobs WHERE platform = ? AND company = ? AND title = ?"
                " ORDER BY id DESC LIMIT 1",
                (platform, company, title),
            ).fetchone()
            return dict(row) if row else None

    def sensitive_company_hit(self, platform: str, company: str | None) -> dict[str, Any] | None:
        """粗查重：同一公司之前已经有岗位被排除过（JD 大多是同一套模板）。"""
        if not company or len(company.strip()) < 4:
            return None
        with self.conn() as c:
            row = c.execute(
                "SELECT * FROM sensitive_jobs WHERE platform = ? AND company = ?"
                " ORDER BY id DESC LIMIT 1",
                (platform, company.strip()),
            ).fetchone()
            return dict(row) if row else None

    def list_sensitive(
        self,
        platform: str | None = None,
        keyword: str | None = None,
        limit: int = 500,
        offset: int = 0,
        kind: str | None = None,
    ) -> list[dict[str, Any]]:
        sql = "SELECT * FROM sensitive_jobs WHERE 1=1"
        args: list[Any] = []
        if platform:
            sql += " AND platform = ?"
            args.append(platform)
        if kind:
            sql += " AND kind = ?"
            args.append(kind)
        if keyword:
            sql += " AND (company LIKE ? OR title LIKE ? OR reason LIKE ?)"
            kw = f"%{keyword}%"
            args.extend([kw, kw, kw])
        sql += " ORDER BY id DESC LIMIT ? OFFSET ?"
        args.extend([max(0, limit), max(0, offset)])
        with self.conn() as c:
            return [dict(r) for r in c.execute(sql, args).fetchall()]

    def count_sensitive(self, platform: str | None = None) -> int:
        sql = "SELECT COUNT(*) AS n FROM sensitive_jobs"
        args: list[Any] = []
        if platform:
            sql += " WHERE platform = ?"
            args.append(platform)
        with self.conn() as c:
            return int(c.execute(sql, args).fetchone()["n"])

    def delete_sensitive(self, ids: list[int]) -> int:
        if not ids:
            return 0
        marks = ",".join("?" * len(ids))
        with self.tx() as c:
            cur = c.execute(f"DELETE FROM sensitive_jobs WHERE id IN ({marks})", list(ids))
            return cur.rowcount

    # ---------- jd_decisions ----------
    def log_decision(
        self, platform: str, company_job: str, decision: str, reason: str
    ) -> dict[str, Any]:
        now = int(time.time())
        with self.tx() as c:
            cur = c.execute(
                "INSERT INTO jd_decisions (platform, company_job, decision, reason, created_at)"
                " VALUES (?,?,?,?,?)",
                (platform, company_job, decision, reason, now),
            )
            rid = cur.lastrowid
        return {
            "id": rid,
            "platform": platform,
            "company_job": company_job,
            "decision": decision,
            "reason": reason,
            "created_at": now,
        }

    def list_decisions(self, limit: int = 200, after_id: int = 0) -> list[dict[str, Any]]:
        return [
            dict(r)
            for r in self.conn().execute(
                "SELECT * FROM jd_decisions WHERE id > ? ORDER BY id DESC LIMIT ?",
                (int(after_id), int(limit)),
            ).fetchall()
        ]

    # ---------- app_config ----------
    def get_config(self, key: str, default: str | None = None) -> str | None:
        row = self.conn().execute(
            "SELECT value FROM app_config WHERE key = ?", (key,)
        ).fetchone()
        return row["value"] if row else default

    def set_config(self, key: str, value: str | None) -> None:
        with self.tx() as c:
            c.execute(
                "INSERT INTO app_config (key, value) VALUES (?,?)"
                " ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (key, value if value is not None else ""),
            )

    def get_config_json(self, key: str, default: Any = None) -> Any:
        raw = self.get_config(key)
        if raw in (None, ""):
            return default
        try:
            return json.loads(raw)
        except (json.JSONDecodeError, TypeError):
            return default

    def set_config_json(self, key: str, value: Any) -> None:
        self.set_config(key, json.dumps(value, ensure_ascii=False))
