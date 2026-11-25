//! Boss 聊天本地缓存：会话列表与消息的持久化读写。
//!
//! 设计要点：
//! - 远程拉取成功后才写库（[`SqliteStore::upsert_chat_friends`] / [`SqliteStore::upsert_chat_messages`]），
//!   失败的远程结果不会污染本地缓存。
//! - 写入全部走 `INSERT ... ON CONFLICT DO UPDATE`，同一 `(platform, uid)` / `(platform, uid, msg_id)`
//!   幂等，重复拉取即增量追加。
//! - 读取只查本地，不触发任何浏览器动作（供前端「先展示本地」使用）。
//! - **会话列表按「最后聊天时间」判新旧**（用户 m01539）：每次远程拉取都先比时间戳，
//!   只有比库里新时才覆盖。否则一次「Boss 那边加载慢、回来的是半截列表」就会把
//!   更新过的行冲回旧值——这正是「拉一次覆盖一次」写法最隐蔽的丢数据方式。

use rusqlite::{params, Connection};

use super::row::now_secs;
use super::{Result, SqliteStore};

/// 会话列表一行（与前端 `Friend` 一一对应的存储形态）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatFriendRow {
    pub uid: String,
    pub name: String,
    pub company: Option<String>,
    pub title: Option<String>,
    pub last_msg: Option<String>,
    pub last_time: Option<String>,
    pub unread: u32,
    pub avatar: Option<String>,
    /// 最后聊天时刻（毫秒）。判新旧**只认它**，不看界面文案 `last_time`。
    pub last_ts: Option<i64>,
    /// 最后一条是不是我发的。`None` = 不知道（DOM 兜底路径取不到），界面不加状态词。
    pub last_from_me: Option<bool>,
    /// 我发的最后一条的送达态：`1` 已送达、`2` 已读。
    pub last_status: Option<u32>,
}

/// 消息一行。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatMessageRow {
    pub msg_id: String,
    pub sender: String,
    pub content: String,
    pub time: Option<String>,
    pub recalled: bool,
}

impl SqliteStore {
    /// 远程会话列表成功后 UPSERT。返回**真正写入**的行数。
    ///
    /// 判新旧规则（用户 m01539「对大于数据时间的聊天列表做储存，不大于的不管」）：
    /// - 远程行的 `last_ts` 比库里**大** → 覆盖；
    /// - 比库里**小或相等** → 跳过（库里那份更新，或者一样新，没必要写）；
    /// - 远程行**没有**时间戳（DOM 兜底取不到时间）→ 仍写，但**保留库里的 `last_ts`**，
    ///   不让缺时间的行把已有时间抹掉——时间一旦被抹成 0，下一次真更新就判不出来了。
    pub fn upsert_chat_friends(&self, platform: &str, rows: &[ChatFriendRow]) -> Result<usize> {
        let mut conn = self.conn.lock().expect("存储锁中毒");
        upsert_friends_tx(&mut conn, platform, rows, now_secs())
    }

    /// 远程某会话的历史消息成功后 UPSERT（按 msg_id 去重，即增量追加）。
    pub fn upsert_chat_messages(
        &self,
        platform: &str,
        uid: &str,
        rows: &[ChatMessageRow],
    ) -> Result<usize> {
        let mut conn = self.conn.lock().expect("存储锁中毒");
        upsert_messages_tx(&mut conn, platform, uid, rows, now_secs())
    }

    /// 读本地会话列表（按最近同步时间倒序，便于界面先展示）。
    pub fn list_chat_friends(&self, platform: &str) -> Result<Vec<ChatFriendRow>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let mut stmt = conn.prepare(
            "SELECT uid, name, company, title, last_msg, last_time, unread, avatar, last_ts,
                    last_from_me, last_status
             FROM chat_friends WHERE platform = ?1
             ORDER BY COALESCE(last_ts, 0) DESC, uid",
        )?;
        let rows = stmt.query_map(params![platform], |r| {
            Ok(ChatFriendRow {
                uid: r.get(0)?,
                name: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                company: r.get(2)?,
                title: r.get(3)?,
                last_msg: r.get(4)?,
                last_time: r.get(5)?,
                unread: r.get::<_, Option<i64>>(6)?.unwrap_or(0).max(0) as u32,
                avatar: r.get(7)?,
                last_ts: r.get(8)?,
                last_from_me: r.get::<_, Option<i64>>(9)?.map(|v| v != 0),
                last_status: r.get::<_, Option<i64>>(10)?.map(|v| v.max(0) as u32),
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 读某会话的本地消息（按 time 文本再按 msg_id 升序，保证稳定顺序）。
    pub fn list_chat_messages(&self, platform: &str, uid: &str) -> Result<Vec<ChatMessageRow>> {
        let conn = self.conn.lock().expect("存储锁中毒");
        let mut stmt = conn.prepare(
            "SELECT msg_id, sender, content, time, recalled
             FROM chat_messages WHERE platform = ?1 AND uid = ?2
             ORDER BY time, msg_id",
        )?;
        let rows = stmt.query_map(params![platform, uid], |r| {
            Ok(ChatMessageRow {
                msg_id: r.get(0)?,
                sender: r.get(1)?,
                content: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                time: r.get(3)?,
                recalled: r.get::<_, Option<i64>>(4)?.unwrap_or(0) != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

/// 会话列表写入：单事务，整体成功或整体回滚（避免列表半新半旧）。
///
/// 时间比较放在 SQL 的 `WHERE` 里而不是先 SELECT 再比：一行语句内完成判断与写入，
/// 不受「读和写之间被另一条命令插进来」的影响，也省掉一次往返。
fn upsert_friends_tx(
    conn: &mut Connection,
    platform: &str,
    rows: &[ChatFriendRow],
    now: i64,
) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0usize;
    for r in rows {
        if r.uid.is_empty() {
            continue;
        }
        n += tx.execute(
            "INSERT INTO chat_friends
                (platform, uid, name, company, title, last_msg, last_time, unread, avatar,
                 synced_at, last_ts, last_from_me, last_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(platform, uid) DO UPDATE SET
                name = excluded.name,
                company = excluded.company,
                title = excluded.title,
                last_msg = excluded.last_msg,
                last_time = excluded.last_time,
                unread = excluded.unread,
                avatar = excluded.avatar,
                synced_at = excluded.synced_at,
                -- 远程行没带时间戳时保留库里已有的，别把它抹成 NULL
                last_ts = COALESCE(excluded.last_ts, chat_friends.last_ts),
                -- 同理：这次没拿到送达态就留旧值，别把「已读」抹回空
                last_from_me = COALESCE(excluded.last_from_me, chat_friends.last_from_me),
                last_status = COALESCE(excluded.last_status, chat_friends.last_status)
             -- 只有远程时间「更大」才覆盖；相等说明库里那份一样新，不必重写
             WHERE excluded.last_ts IS NULL
                OR excluded.last_ts > COALESCE(chat_friends.last_ts, 0)",
            params![
                platform,
                r.uid,
                r.name,
                r.company,
                r.title,
                r.last_msg,
                r.last_time,
                r.unread as i64,
                r.avatar,
                now,
                r.last_ts,
                r.last_from_me.map(|v| v as i64),
                r.last_status.map(|v| v as i64)
            ],
        )?;
    }
    tx.commit()?;
    Ok(n)
}

/// 消息写入：单事务，按 `(platform, uid, msg_id)` 增量 UPSERT。
fn upsert_messages_tx(
    conn: &mut Connection,
    platform: &str,
    uid: &str,
    rows: &[ChatMessageRow],
    now: i64,
) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut n = 0usize;
    for r in rows {
        if r.msg_id.is_empty() {
            continue;
        }
        n += tx.execute(
            "INSERT INTO chat_messages
                (platform, uid, msg_id, sender, content, time, recalled, synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(platform, uid, msg_id) DO UPDATE SET
                sender = excluded.sender,
                content = excluded.content,
                time = excluded.time,
                recalled = excluded.recalled,
                synced_at = excluded.synced_at",
            params![
                platform,
                uid,
                r.msg_id,
                r.sender,
                r.content,
                r.time,
                r.recalled as i64,
                now
            ],
        )?;
    }
    tx.commit()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn friend(uid: &str, unread: u32) -> ChatFriendRow {
        ChatFriendRow {
            uid: uid.into(),
            name: format!("hr-{uid}"),
            unread,
            ..Default::default()
        }
    }

    fn msg(id: &str, content: &str) -> ChatMessageRow {
        ChatMessageRow {
            msg_id: id.into(),
            sender: "hr".into(),
            content: content.into(),
            ..Default::default()
        }
    }

    #[test]
    fn friends_upsert_is_idempotent_and_updates_unread() {
        let s = SqliteStore::open_in_memory().unwrap();
        s.upsert_chat_friends("boss", &[friend("u1", 0)]).unwrap();
        s.upsert_chat_friends("boss", &[friend("u1", 3)]).unwrap();
        let rows = s.list_chat_friends("boss").unwrap();
        assert_eq!(rows.len(), 1, "同一 uid 应只保留一行");
        assert_eq!(rows[0].unread, 3, "远程拉取后未读数应更新");
    }

    #[test]
    fn messages_append_incrementally_by_msg_id() {
        let s = SqliteStore::open_in_memory().unwrap();
        s.upsert_chat_messages("boss", "u1", &[msg("m1", "你好")]).unwrap();
        s.upsert_chat_messages("boss", "u1", &[msg("m1", "你好"), msg("m2", "在吗")])
            .unwrap();
        let rows = s.list_chat_messages("boss", "u1").unwrap();
        assert_eq!(rows.len(), 2, "已存在的 m1 不重复，新增 m2 追加");
    }

    #[test]
    fn messages_are_scoped_by_uid() {
        let s = SqliteStore::open_in_memory().unwrap();
        s.upsert_chat_messages("boss", "u1", &[msg("m1", "a")]).unwrap();
        s.upsert_chat_messages("boss", "u2", &[msg("m1", "b")]).unwrap();
        assert_eq!(s.list_chat_messages("boss", "u1").unwrap().len(), 1);
        assert_eq!(s.list_chat_messages("boss", "u2").unwrap().len(), 1);
    }

    #[test]
    fn empty_uid_or_msg_id_is_skipped() {
        let s = SqliteStore::open_in_memory().unwrap();
        s.upsert_chat_friends("boss", &[friend("", 0)]).unwrap();
        s.upsert_chat_messages("boss", "u1", &[msg("", "x")]).unwrap();
        assert!(s.list_chat_friends("boss").unwrap().is_empty());
        assert!(s.list_chat_messages("boss", "u1").unwrap().is_empty());
    }

    fn timed(uid: &str, last_msg: &str, last_ts: Option<i64>) -> ChatFriendRow {
        ChatFriendRow {
            uid: uid.into(),
            name: format!("hr-{uid}"),
            last_msg: Some(last_msg.into()),
            last_ts,
            ..Default::default()
        }
    }

    #[test]
    fn friends_newer_timestamp_overwrites() {
        let s = SqliteStore::open_in_memory().unwrap();
        s.upsert_chat_friends("boss", &[timed("u1", "旧消息", Some(1_000))])
            .unwrap();
        let n = s
            .upsert_chat_friends("boss", &[timed("u1", "新消息", Some(2_000))])
            .unwrap();
        assert_eq!(n, 1, "时间更大应真的写进去");
        let rows = s.list_chat_friends("boss").unwrap();
        assert_eq!(rows[0].last_msg.as_deref(), Some("新消息"));
        assert_eq!(rows[0].last_ts, Some(2_000));
    }

    #[test]
    fn friends_older_or_equal_timestamp_is_ignored() {
        let s = SqliteStore::open_in_memory().unwrap();
        s.upsert_chat_friends("boss", &[timed("u1", "新消息", Some(2_000))])
            .unwrap();
        // 半截列表（Boss 那边还在加载）带回来的旧行：不许冲掉已有的新值。
        let older = s
            .upsert_chat_friends("boss", &[timed("u1", "旧消息", Some(1_000))])
            .unwrap();
        assert_eq!(older, 0, "时间更小不写");
        let equal = s
            .upsert_chat_friends("boss", &[timed("u1", "同刻消息", Some(2_000))])
            .unwrap();
        assert_eq!(equal, 0, "时间相等也不写（库里那份一样新）");
        let rows = s.list_chat_friends("boss").unwrap();
        assert_eq!(rows[0].last_msg.as_deref(), Some("新消息"));
        assert_eq!(rows[0].last_ts, Some(2_000));
    }

    #[test]
    fn friends_without_timestamp_keep_stored_time() {
        let s = SqliteStore::open_in_memory().unwrap();
        s.upsert_chat_friends("boss", &[timed("u1", "有时间的", Some(5_000))])
            .unwrap();
        // DOM 兜底路径取不到时间：仍写（消息文案是新的），但时间保留库里的。
        s.upsert_chat_friends("boss", &[timed("u1", "无时间的", None)])
            .unwrap();
        let rows = s.list_chat_friends("boss").unwrap();
        assert_eq!(rows[0].last_msg.as_deref(), Some("无时间的"));
        assert_eq!(rows[0].last_ts, Some(5_000), "缺时间戳不得把已有时间抹掉");
    }

    #[test]
    fn friends_list_is_ordered_by_last_ts_desc() {
        let s = SqliteStore::open_in_memory().unwrap();
        s.upsert_chat_friends(
            "boss",
            &[
                timed("old", "a", Some(1_000)),
                timed("new", "b", Some(9_000)),
                timed("mid", "c", Some(5_000)),
                timed("none", "d", None),
            ],
        )
        .unwrap();
        let uids: Vec<String> = s
            .list_chat_friends("boss")
            .unwrap()
            .into_iter()
            .map(|r| r.uid)
            .collect();
        assert_eq!(uids, vec!["new", "mid", "old", "none"], "最近聊天在前，无时间的垫底");
    }
}
