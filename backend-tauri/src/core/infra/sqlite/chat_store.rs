//! Boss 聊天本地缓存：会话列表与消息的持久化读写。
//!
//! 设计要点：
//! - 远程拉取成功后才写库（[`SqliteStore::upsert_chat_friends`] / [`SqliteStore::upsert_chat_messages`]），
//!   失败的远程结果不会污染本地缓存。
//! - 写入全部走 `INSERT ... ON CONFLICT DO UPDATE`，同一 `(platform, uid)` / `(platform, uid, msg_id)`
//!   幂等，重复拉取即增量追加。
//! - 读取只查本地，不触发任何浏览器动作（供前端「先展示本地」使用）。

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
    /// 远程会话列表成功后整体 UPSERT。返回写入的行数。
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
            "SELECT uid, name, company, title, last_msg, last_time, unread, avatar
             FROM chat_friends WHERE platform = ?1 ORDER BY synced_at DESC, uid",
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
                (platform, uid, name, company, title, last_msg, last_time, unread, avatar, synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(platform, uid) DO UPDATE SET
                name = excluded.name,
                company = excluded.company,
                title = excluded.title,
                last_msg = excluded.last_msg,
                last_time = excluded.last_time,
                unread = excluded.unread,
                avatar = excluded.avatar,
                synced_at = excluded.synced_at",
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
                now
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
}
