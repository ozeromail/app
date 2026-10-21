//! `Store` on SQLite, encrypted with SQLCipher.

mod schema;

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use ozero_domain::{
    Account, AccountId, AccountKind, Action, Address, Attachment, Body, Category, Changes,
    Checkpoint, Correspondents, Draft, DraftId, Flags, Folder, FolderId, FolderRole, ListQuery,
    ListScope, MessageId, MessageLocation, MessageRow, NewAccount, NewFolder, NewMessage, Op, OpId,
    OutboxId, Outgoing, SavedDraft, Sender, Server, Store, StoreError, Unsent, with_quote,
};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{
    CachedStatement, Connection, ErrorCode, OptionalExtension, Row, Transaction, params,
};

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SqliteStore")
    }
}

impl SqliteStore {
    /// Opens or creates the database at `path` with a 256-bit key from the platform keystore.
    pub fn open(path: &Path, key: &[u8; 32]) -> Result<Self, StoreError> {
        Self::init(Connection::open(path).map_err(storage)?, key)
    }

    #[cfg(test)]
    fn open_in_memory(key: &[u8; 32]) -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory().map_err(storage)?, key)
    }

    fn init(conn: Connection, key: &[u8; 32]) -> Result<Self, StoreError> {
        // Errors reach us as results; SQLCipher's own log is off.
        conn.execute_batch("PRAGMA cipher_log_level = NONE;")
            .map_err(storage)?;
        apply_key(&conn, key)?;
        // The first read fails if the key does not match the file.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
            .map_err(|e| match e.sqlite_error_code() {
                Some(ErrorCode::NotADatabase) => StoreError::WrongKey,
                _ => storage(e),
            })?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )
        .map_err(storage)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock cannot leave a half-written transaction, so reuse the connection.
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Passes the raw key (no slow password derivation) and wipes our copy of the statement.
fn apply_key(conn: &Connection, key: &[u8; 32]) -> Result<(), StoreError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut pragma = String::with_capacity(80);
    pragma.push_str("PRAGMA key = \"x'");
    for b in key {
        pragma.push(char::from(HEX[usize::from(b >> 4)]));
        pragma.push(char::from(HEX[usize::from(b & 0xf)]));
    }
    pragma.push_str("'\";");
    let result = conn.execute_batch(&pragma).map_err(storage);
    let mut bytes = pragma.into_bytes();
    bytes.fill(0);
    std::hint::black_box(&bytes);
    result
}

fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let current: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(storage)?;
    let latest = schema::MIGRATIONS.len() as u32;
    if current > latest {
        return Err(StoreError::TooNew { version: current });
    }
    for (i, sql) in schema::MIGRATIONS.iter().enumerate().skip(current as usize) {
        // Rolls back on drop if any step fails.
        let tx = conn.unchecked_transaction().map_err(storage)?;
        tx.execute_batch(sql).map_err(storage)?;
        tx.pragma_update(None, "user_version", i as u32 + 1)
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
    }
    Ok(())
}

impl Store for SqliteStore {
    fn add_account(&self, account: &NewAccount) -> Result<AccountId, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let (server, smtp) = (account.server.as_ref(), account.smtp.as_ref());
        // The lowest colour no account has, so a picked one is not repeated soon.
        let id: i64 = tx
            .query_row(
                "INSERT INTO account (id, kind, email, display_name, server_host, server_port,
                                      server_user, smtp_host, smtp_port, smtp_user, color)
                 VALUES ((SELECT last + 1 FROM account_seq), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                         (SELECT min(c) FROM (SELECT 0 AS c UNION SELECT color + 1 FROM account)
                          WHERE c NOT IN (SELECT color FROM account)))
                 RETURNING id",
                params![
                    kind_str(account.kind),
                    account.email,
                    account.display_name,
                    server.map(|s| &s.host),
                    server.map(|s| s.port),
                    server.map(|s| &s.user),
                    smtp.map(|s| &s.host),
                    smtp.map(|s| s.port),
                    smtp.map(|s| &s.user),
                ],
                |r| r.get(0),
            )
            .map_err(storage)?;
        tx.execute("UPDATE account_seq SET last = ?1", [id])
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(AccountId(id))
    }

    fn accounts(&self) -> Result<Vec<Account>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, kind, email, display_name, server_host, server_port, server_user,
                        smtp_host, smtp_port, smtp_user, color, label, keep_days, needs_sign_in
                 FROM account ORDER BY id",
            )
            .map_err(storage)?;
        let rows = stmt.query_map([], |r| {
            let server = |at: usize| -> rusqlite::Result<Option<Server>> {
                Ok(match (r.get(at)?, r.get(at + 1)?, r.get(at + 2)?) {
                    (Some(host), Some(port), Some(user)) => Some(Server { host, port, user }),
                    _ => None,
                })
            };
            Ok(Account {
                id: AccountId(r.get(0)?),
                kind: r.get::<_, Kind>(1)?.0,
                email: r.get(2)?,
                display_name: r.get(3)?,
                server: server(4)?,
                smtp: server(7)?,
                color: r.get(10)?,
                label: r.get(11)?,
                keep_days: r.get(12)?,
                needs_sign_in: r.get(13)?,
            })
        });
        rows.and_then(|rows| rows.collect()).map_err(storage)
    }

    fn remove_account(&self, account: AccountId) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute("DELETE FROM account WHERE id = ?1", [account.0])
            .map_err(storage)?;
        found(changed)
    }

    fn set_account_color(&self, account: AccountId, color: u32) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE account SET color = ?2 WHERE id = ?1",
                params![account.0, color],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn set_account_label(&self, account: AccountId, label: &str) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE account SET label = ?2 WHERE id = ?1",
                params![account.0, label.trim()],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn set_account_keep(&self, account: AccountId, days: u32) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let changed = tx
            .execute(
                "UPDATE account SET keep_days = ?2 WHERE id = ?1 AND keep_days != ?2",
                params![account.0, days],
            )
            .map_err(storage)?;
        // Kept longer, a folder that ended at the old limit may have older mail again.
        if changed > 0 {
            tx.execute(
                "UPDATE folder SET older_done = 0 WHERE account_id = ?1",
                [account.0],
            )
            .map_err(storage)?;
        } else {
            require(&tx, "SELECT 1 FROM account WHERE id = ?1", account.0)?;
        }
        tx.commit().map_err(storage)
    }

    fn prune(&self, account: AccountId, before: i64) -> Result<(), StoreError> {
        self.conn()
            .execute(
                "DELETE FROM message
                 WHERE folder_id IN (SELECT id FROM folder WHERE account_id = ?1) AND date < ?2
                   AND found = 0
                   AND NOT EXISTS (SELECT 1 FROM op WHERE op.message_id = message.id)
                   AND NOT EXISTS (SELECT 1 FROM outbox WHERE outbox.reply_to = message.id)",
                params![account.0, before],
            )
            .map_err(storage)?;
        Ok(())
    }

    fn set_needs_sign_in(&self, account: AccountId, needs: bool) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE account SET needs_sign_in = ?2 WHERE id = ?1",
                params![account.0, needs],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn save_folders(
        &self,
        account: AccountId,
        folders: &[NewFolder],
    ) -> Result<Vec<Folder>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        require(&tx, "SELECT 1 FROM account WHERE id = ?1", account.0)?;
        let saved = {
            let mut upsert = tx
                .prepare_cached(
                    "INSERT INTO folder (account_id, remote_id, name, role) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (account_id, remote_id) DO UPDATE SET name = excluded.name, role = excluded.role
                     RETURNING id",
                )
                .map_err(storage)?;
            folders
                .iter()
                .map(|f| {
                    let id = upsert
                        .query_row(
                            params![account.0, f.remote_id, f.name, role_str(f.role)],
                            |r| r.get(0),
                        )
                        .map_err(storage)?;
                    Ok(Folder {
                        id: FolderId(id),
                        account_id: account,
                        remote_id: f.remote_id.clone(),
                        name: f.name.clone(),
                        role: f.role,
                    })
                })
                .collect::<Result<Vec<_>, StoreError>>()?
        };
        tx.commit().map_err(storage)?;
        Ok(saved)
    }

    fn folders(&self, account: AccountId) -> Result<Vec<Folder>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, remote_id, name, role FROM folder WHERE account_id = ?1 ORDER BY id",
            )
            .map_err(storage)?;
        let rows = stmt.query_map([account.0], |r| {
            Ok(Folder {
                id: FolderId(r.get(0)?),
                account_id: account,
                remote_id: r.get(1)?,
                name: r.get(2)?,
                role: r.get::<_, Role>(3)?.0,
            })
        });
        rows.and_then(|rows| rows.collect()).map_err(storage)
    }

    fn checkpoint(&self, folder: FolderId) -> Result<(Checkpoint, Vec<String>), StoreError> {
        let conn = self.conn();
        let checkpoint = conn
            .query_row(
                "SELECT validity, next_id FROM folder WHERE id = ?1",
                [folder.0],
                |r| {
                    Ok(Checkpoint {
                        validity: r.get(0)?,
                        // SQLite has no u64; a cursor that does not fit starts over.
                        next: u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                    })
                },
            )
            .optional()
            .map_err(storage)?
            .ok_or(StoreError::NotFound)?;
        let known = conn
            .prepare_cached("SELECT remote_id FROM message WHERE folder_id = ?1 AND found = 0")
            .and_then(|mut s| s.query_map([folder.0], |r| r.get(0))?.collect())
            .map_err(storage)?;
        Ok((checkpoint, known))
    }

    fn oldest(&self, folder: FolderId) -> Result<Option<i64>, StoreError> {
        self.conn()
            .query_row(
                "SELECT min(date) FROM message WHERE folder_id = ?1 AND found = 0",
                [folder.0],
                |r| r.get(0),
            )
            .map_err(storage)
    }

    fn older_done(&self, folder: FolderId) -> Result<bool, StoreError> {
        self.conn()
            .query_row(
                "SELECT older_done FROM folder WHERE id = ?1",
                [folder.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?
            .ok_or(StoreError::NotFound)
    }

    fn set_older_done(&self, folder: FolderId, done: bool) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE folder SET older_done = ?2 WHERE id = ?1",
                params![folder.0, done],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn save_changes(
        &self,
        folder: FolderId,
        changes: &Changes<NewMessage>,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        reset_if_new(&tx, folder, changes.checkpoint.validity)?;
        {
            let mut flags = tx.prepare_cached(SAVE_FLAGS).map_err(storage)?;
            for (remote_id, f) in &changes.flags {
                flags
                    .execute(params![folder.0, remote_id, flags_bits(*f)])
                    .map_err(storage)?;
            }
            let mut gone = tx
                .prepare_cached("DELETE FROM message WHERE folder_id = ?1 AND remote_id = ?2")
                .map_err(storage)?;
            for remote_id in &changes.gone {
                gone.execute(params![folder.0, remote_id])
                    .map_err(storage)?;
            }
        }
        upsert_messages(&tx, folder, &changes.new)?;
        tx.execute(
            "UPDATE folder SET next_id = ?2 WHERE id = ?1 AND next_id IS NOT ?2",
            params![
                folder.0,
                i64::try_from(changes.checkpoint.next).unwrap_or(0)
            ],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    fn list(&self, query: &ListQuery) -> Result<Vec<MessageRow>, StoreError> {
        let (date, id) = query
            .after
            .map_or((i64::MAX, i64::MAX), |c| (c.date, c.id.0));
        let (sql, scope): (&str, &dyn rusqlite::ToSql) = match &query.scope {
            ListScope::Folder(folder) => (LIST_FOLDER, &folder.0),
            ListScope::Unified(role) if walks_by_date(*role) => {
                (LIST_UNIFIED_BY_DATE, role_ref(*role))
            }
            ListScope::Unified(role) => (LIST_UNIFIED_BY_FOLDER, role_ref(*role)),
            ListScope::Inbox(category) => (LIST_INBOX_TAB, category_ref(*category)),
        };
        let conn = self.conn();
        let rows = conn.prepare_cached(sql).and_then(|mut s| {
            s.query_map(params![scope, date, id, query.limit], message_row)?
                .collect()
        });
        rows.map_err(storage)
    }

    fn inbox_unread(&self) -> Result<Vec<(Category, u32)>, StoreError> {
        let conn = self.conn();
        let rows = conn.prepare_cached(INBOX_UNREAD).and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, StoredCategory>(0)?.0, r.get(1)?)))?
                .collect()
        });
        rows.map_err(storage)
    }

    fn inbox_is_empty(&self) -> Result<bool, StoreError> {
        let conn = self.conn();
        conn.prepare_cached(INBOX_EMPTY)
            .and_then(|mut s| s.query_row([], |r| r.get(0)))
            .map_err(storage)
    }

    fn thread(&self, message: MessageId) -> Result<Vec<MessageRow>, StoreError> {
        let conn = self.conn();
        conn.prepare_cached(THREAD)
            .and_then(|mut s| s.query_map([message.0], message_row)?.collect())
            .map_err(storage)
    }

    fn search(&self, text: &str, limit: u32) -> Result<Vec<MessageRow>, StoreError> {
        let (Some(query), Some(words)) = (fts_query(text), hit_words(text)) else {
            return Ok(vec![]);
        };
        let conn = self.conn();
        conn.prepare_cached(SEARCH)
            .and_then(|mut s| {
                s.query_map(params![query, limit, words], message_row)?
                    .collect()
            })
            .map_err(storage)
    }

    fn save_found(
        &self,
        account: AccountId,
        text: &str,
        found: &[(FolderId, Vec<NewMessage>)],
    ) -> Result<(), StoreError> {
        let Some(words) = hit_words(text) else {
            return Ok(());
        };
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        tx.execute(
            "DELETE FROM search_hit WHERE message_id IN (
                 SELECT m.id FROM message m JOIN folder f ON f.id = m.folder_id
                 WHERE f.account_id = ?1)",
            [account.0],
        )
        .map_err(storage)?;
        {
            let mut save = tx.prepare_cached(SAVE_FOUND).map_err(storage)?;
            let mut hit = tx
                .prepare_cached(
                    "INSERT OR REPLACE INTO search_hit (message_id, words)
                     SELECT id, ?3 FROM message WHERE folder_id = ?1 AND remote_id = ?2",
                )
                .map_err(storage)?;
            for (folder, messages) in found {
                for m in messages {
                    if write_message(&mut save, *folder, m)? > 0 {
                        link_thread(&tx, account.0, *folder, m)?;
                    }
                    hit.execute(params![folder.0, m.remote_id, words])
                        .map_err(storage)?;
                }
            }
        }
        tx.execute(
            "DELETE FROM message
             WHERE found = 1 AND folder_id IN (SELECT id FROM folder WHERE account_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM search_hit h WHERE h.message_id = message.id)
               AND NOT EXISTS (SELECT 1 FROM op WHERE op.message_id = message.id)
               AND NOT EXISTS (SELECT 1 FROM outbox WHERE outbox.reply_to = message.id)",
            [account.0],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    fn take_fresh(&self, since: i64) -> Result<Vec<MessageRow>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let rows = tx
            .prepare_cached(FRESH_PEOPLE)
            .and_then(|mut s| s.query_map([since], message_row)?.collect())
            .map_err(storage)?;
        tx.execute(CLEAR_FRESH, []).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(rows)
    }

    fn clear_fresh(&self) -> Result<(), StoreError> {
        self.conn()
            .execute(CLEAR_FRESH, [])
            .map(|_| ())
            .map_err(storage)
    }

    fn shows_images(&self, message: MessageId) -> Result<bool, StoreError> {
        self.conn()
            .prepare_cached(
                // Never in junk: a sender there may be anyone, and may be pretending.
                "SELECT f.role IS NOT 'junk' AND (
                            m.category = 'people' AND m.from_email != ''
                            OR EXISTS (SELECT 1 FROM image_sender WHERE email = m.from_email))
                 FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
            )
            .and_then(|mut s| s.query_row([message.0], |r| r.get(0)).optional())
            .map_err(storage)?
            .ok_or(StoreError::NotFound)
    }

    fn allow_images(&self, message: MessageId) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                // REPLACE counts a sender already allowed, so zero means no such message.
                "INSERT OR REPLACE INTO image_sender (email)
                 SELECT from_email FROM message WHERE id = ?1",
                [message.0],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn correspondents(&self, message: MessageId) -> Result<Correspondents, StoreError> {
        self.conn()
            .prepare_cached("SELECT from_email, answer_to, recipients FROM message WHERE id = ?1")
            .and_then(|mut s| {
                s.query_row([message.0], |r| {
                    Ok(Correspondents {
                        from: r.get(0)?,
                        answer_to: recipients(&r.get::<_, String>(1)?),
                        recipients: recipients(&r.get::<_, String>(2)?),
                    })
                })
                .optional()
            })
            .map_err(storage)?
            .ok_or(StoreError::NotFound)
    }

    fn location(&self, message: MessageId) -> Result<MessageLocation, StoreError> {
        self.conn()
            .prepare_cached(
                "SELECT f.account_id, f.remote_id, f.validity, m.remote_id
                 FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
            )
            .and_then(|mut s| {
                s.query_row([message.0], |r| {
                    Ok(MessageLocation {
                        id: message,
                        account_id: AccountId(r.get(0)?),
                        folder_remote_id: r.get(1)?,
                        validity: r.get(2)?,
                        remote_id: r.get(3)?,
                    })
                })
                .optional()
            })
            .map_err(storage)?
            .ok_or(StoreError::NotFound)
    }

    fn without_body(
        &self,
        account: AccountId,
        limit: u32,
    ) -> Result<Vec<MessageLocation>, StoreError> {
        self.conn()
            .prepare_cached(WITHOUT_BODY)
            .and_then(|mut s| {
                s.query_map(params![account.0, limit], |r| {
                    Ok(MessageLocation {
                        id: MessageId(r.get(0)?),
                        account_id: account,
                        folder_remote_id: r.get(1)?,
                        validity: r.get(2)?,
                        remote_id: r.get(3)?,
                    })
                })?
                .collect()
            })
            .map_err(storage)
    }

    fn save_body(&self, message: MessageId, body: &Body) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        require(&tx, "SELECT 1 FROM message WHERE id = ?1", message.0)?;
        tx.execute(
            "INSERT INTO message_body (message_id, text, html) VALUES (?1, ?2, ?3)
             ON CONFLICT (message_id) DO UPDATE SET text = excluded.text, html = excluded.html",
            params![message.0, body.text, body.html],
        )
        .map_err(storage)?;
        tx.execute("DELETE FROM attachment WHERE message_id = ?1", [message.0])
            .map_err(storage)?;
        {
            let mut insert = tx
                .prepare_cached(
                    "INSERT INTO attachment (message_id, part, name, mime, size)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(storage)?;
            for a in &body.attachments {
                let size = i64::try_from(a.size).unwrap_or(i64::MAX);
                insert
                    .execute(params![message.0, a.part, a.name, a.mime, size])
                    .map_err(storage)?;
            }
        }
        tx.execute(
            // A body without text keeps the snippet the list had.
            "UPDATE message SET snippet = ?2 WHERE id = ?1 AND ?2 <> ''",
            params![message.0, body.snippet()],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    fn body(&self, message: MessageId) -> Result<Option<Body>, StoreError> {
        let conn = self.conn();
        let Some((text, html)) = conn
            .prepare_cached("SELECT text, html FROM message_body WHERE message_id = ?1")
            .and_then(|mut s| {
                s.query_row([message.0], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()
            })
            .map_err(storage)?
        else {
            return Ok(None);
        };
        let attachments = conn
            .prepare_cached(
                "SELECT part, name, mime, size FROM attachment WHERE message_id = ?1 ORDER BY part",
            )
            .and_then(|mut s| {
                s.query_map([message.0], |r| {
                    Ok(Attachment {
                        part: r.get(0)?,
                        name: r.get(1)?,
                        mime: r.get(2)?,
                        size: u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
                    })
                })?
                .collect()
            })
            .map_err(storage)?;
        Ok(Some(Body {
            text,
            html,
            attachments,
        }))
    }

    fn queue(
        &self,
        message: MessageId,
        action: Action,
        not_before: i64,
    ) -> Result<Vec<OpId>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let ids = tx
            .prepare_cached(THREAD_IDS)
            .and_then(|mut s| {
                s.query_map([message.0], |r| Ok((r.get(0)?, r.get::<_, u8>(1)?)))?
                    .collect::<Result<Vec<(i64, u8)>, _>>()
            })
            .map_err(storage)?;
        if ids.is_empty() {
            return Err(StoreError::NotFound);
        }
        if !matches!(action, Action::Seen(_)) && !folder_role(&tx, message)?.allows(action) {
            return Err(StoreError::NotAllowed);
        }
        let mut ops = vec![];
        for (id, flags) in ids {
            // The rest of the conversation only where it changes something; unread is
            // only the message itself.
            let changes = match action {
                Action::Seen(seen) => seen && flags & SEEN == 0,
                Action::Archive | Action::Trash | Action::Delete | Action::Empty => true,
            };
            if id == message.0 || changes {
                ops.push(queue_one(&tx, MessageId(id), action, not_before)?);
            }
        }
        tx.commit().map_err(storage)?;
        Ok(ops)
    }

    fn queue_empty(&self, folder: FolderId, not_before: i64) -> Result<Vec<OpId>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let role = tx
            .prepare_cached("SELECT role FROM folder WHERE id = ?1")
            .and_then(|mut s| s.query_row([folder.0], |r| r.get::<_, Role>(0)).optional())
            .map_err(storage)?
            .ok_or(StoreError::NotFound)?;
        if !role.0.allows(Action::Empty) {
            return Err(StoreError::NotAllowed);
        }
        let ids = tx
            .prepare_cached(FOLDER_MESSAGES)
            .and_then(|mut s| {
                s.query_map([folder.0], |r| r.get(0))?
                    .collect::<Result<Vec<i64>, _>>()
            })
            .map_err(storage)?;
        let ops = ids
            .into_iter()
            .map(|id| queue_one(&tx, MessageId(id), Action::Empty, not_before))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().map_err(storage)?;
        Ok(ops)
    }

    fn others(&self, ops: &[Op]) -> Result<Vec<String>, StoreError> {
        let Some(first) = ops.first() else {
            return Ok(vec![]);
        };
        let mut ids = self
            .conn()
            .prepare_cached(
                "SELECT m.remote_id FROM message m JOIN folder f ON f.id = m.folder_id
                 WHERE f.account_id = ?1 AND f.remote_id = ?2",
            )
            .and_then(|mut s| {
                s.query_map(
                    params![first.location.account_id.0, first.location.folder_remote_id],
                    |r| r.get(0),
                )?
                .collect::<Result<Vec<String>, _>>()
            })
            .map_err(storage)?;
        let taken: HashSet<_> = ops.iter().map(|o| &o.location.remote_id).collect();
        ids.retain(|id| !taken.contains(id));
        Ok(ids)
    }

    fn undo(&self, ops: &[OpId], now: i64) -> Result<bool, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        // All or none, so a conversation never comes back in part.
        for op in ops {
            let taken = take_back(
                &tx,
                "DELETE FROM op WHERE id = ?1 AND not_before > ?2 RETURNING message_id, kind, undo",
                *op,
                now,
            )?;
            if !taken {
                return Ok(false);
            }
        }
        tx.commit().map_err(storage)?;
        Ok(true)
    }

    fn next_send(&self, account: AccountId) -> Result<Option<i64>, StoreError> {
        self.conn()
            .prepare_cached(
                "SELECT min(at) FROM (
                     SELECT min(o.not_before) AS at
                     FROM op o JOIN message m ON m.id = o.message_id
                     JOIN folder f ON f.id = m.folder_id
                     WHERE f.account_id = ?1
                     UNION ALL
                     SELECT min(not_before) FROM outbox WHERE account_id = ?1 AND failed IS NULL
                 )",
            )
            .and_then(|mut s| s.query_row([account.0], |r| r.get(0)))
            .map_err(storage)
    }

    fn queue_send(
        &self,
        draft: &Draft,
        saved: Option<DraftId>,
        message_id: &str,
        date: i64,
        not_before: i64,
    ) -> Result<OutboxId, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        require(
            &tx,
            "SELECT 1 FROM account WHERE id = ?1",
            draft.account_id.0,
        )?;
        let in_reply_to: Option<String> = match draft.reply_to {
            None => None,
            Some(parent) => tx
                .query_row(
                    "SELECT message_id FROM message WHERE id = ?1",
                    [parent.0],
                    |r| r.get(0),
                )
                .optional()
                .map_err(storage)?
                .ok_or(StoreError::NotFound)?,
        };
        let id = tx
            .query_row(
                "INSERT INTO outbox (account_id, recipients, subject, text, reply_to, in_reply_to,
                                     message_id, date, not_before, quote)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) RETURNING id",
                params![
                    draft.account_id.0,
                    draft.to.join("\n"),
                    draft.subject,
                    draft.text,
                    draft.reply_to.map(|m| m.0),
                    in_reply_to,
                    message_id,
                    date,
                    not_before,
                    draft.quote,
                ],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if let Some(saved) = saved {
            tx.execute("DELETE FROM draft WHERE id = ?1", [saved.0])
                .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(OutboxId(id))
    }

    fn undo_send(&self, id: OutboxId, now: i64) -> Result<Option<SavedDraft>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let taken = tx
            .query_row(
                "DELETE FROM outbox WHERE id = ?1 AND (not_before > ?2 OR failed IS NOT NULL)
                 RETURNING account_id, recipients, subject, text, reply_to, quote",
                params![id.0, now],
                |r| {
                    Ok(SavedDraft {
                        id: None,
                        account_id: Some(AccountId(r.get(0)?)),
                        to: recipients(&r.get::<_, String>(1)?).join(", "),
                        subject: r.get(2)?,
                        text: r.get(3)?,
                        reply_to: r.get::<_, Option<i64>>(4)?.map(MessageId),
                        quote: r.get(5)?,
                        saved: now,
                    })
                },
            )
            .optional()
            .map_err(storage)?;
        let Some(mut draft) = taken else {
            return Ok(None);
        };
        draft.id = Some(insert_draft(&tx, &draft)?);
        tx.commit().map_err(storage)?;
        Ok(Some(draft))
    }

    fn save_draft(&self, draft: &SavedDraft) -> Result<DraftId, StoreError> {
        let conn = self.conn();
        let Some(id) = draft.id else {
            return insert_draft(&conn, draft);
        };
        // An account or answered message gone since leaves the draft without one.
        let changed = conn
            .execute(
                "UPDATE draft SET
                     account_id = (SELECT id FROM account WHERE id = ?1),
                     recipients = ?2, subject = ?3, text = ?4,
                     reply_to = (SELECT id FROM message WHERE id = ?5),
                     quote = ?6, saved = ?7
                 WHERE id = ?8",
                params![
                    draft.account_id.map(|a| a.0),
                    draft.to,
                    draft.subject,
                    draft.text,
                    draft.reply_to.map(|m| m.0),
                    draft.quote,
                    draft.saved,
                    id.0,
                ],
            )
            .map_err(storage)?;
        found(changed)?;
        Ok(id)
    }

    fn drafts(&self) -> Result<Vec<SavedDraft>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT id, account_id, recipients, subject, text, reply_to, quote, saved
                 FROM draft ORDER BY saved DESC, id DESC",
            )
            .and_then(|mut s| {
                s.query_map([], |r| {
                    Ok(SavedDraft {
                        id: Some(DraftId(r.get(0)?)),
                        account_id: r.get::<_, Option<i64>>(1)?.map(AccountId),
                        to: r.get(2)?,
                        subject: r.get(3)?,
                        text: r.get(4)?,
                        reply_to: r.get::<_, Option<i64>>(5)?.map(MessageId),
                        quote: r.get(6)?,
                        saved: r.get(7)?,
                    })
                })?
                .collect()
            });
        rows.map_err(storage)
    }

    fn delete_draft(&self, id: DraftId) -> Result<(), StoreError> {
        self.conn()
            .execute("DELETE FROM draft WHERE id = ?1", [id.0])
            .map(|_| ())
            .map_err(storage)
    }

    fn unsent(&self) -> Result<Vec<Unsent>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT id, subject, failed FROM outbox WHERE failed IS NOT NULL ORDER BY id DESC",
            )
            .and_then(|mut s| {
                s.query_map([], |r| {
                    Ok(Unsent {
                        id: OutboxId(r.get(0)?),
                        subject: r.get(1)?,
                        reason: r.get(2)?,
                    })
                })?
                .collect()
            });
        rows.map_err(storage)
    }

    fn due_sends(&self, account: AccountId, now: i64) -> Result<Vec<Outgoing>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT id, recipients, subject, text, message_id, in_reply_to, date, quote
                 FROM outbox
                 WHERE account_id = ?1 AND not_before <= ?2 AND failed IS NULL ORDER BY id",
            )
            .and_then(|mut s| {
                s.query_map(params![account.0, now], |r| {
                    Ok(Outgoing {
                        id: OutboxId(r.get(0)?),
                        to: recipients(&r.get::<_, String>(1)?),
                        subject: r.get(2)?,
                        text: with_quote(&r.get::<_, String>(3)?, &r.get::<_, String>(7)?),
                        message_id: r.get(4)?,
                        in_reply_to: r.get(5)?,
                        date: r.get(6)?,
                    })
                })?
                .collect()
            });
        rows.map_err(storage)
    }

    fn sent(&self, id: OutboxId) -> Result<(), StoreError> {
        self.conn()
            .execute("DELETE FROM outbox WHERE id = ?1", [id.0])
            .map(|_| ())
            .map_err(storage)
    }

    fn refused(&self, id: OutboxId, reason: &str) -> Result<(), StoreError> {
        self.conn()
            .execute(
                "UPDATE outbox SET failed = ?2 WHERE id = ?1",
                params![id.0, reason],
            )
            .map(|_| ())
            .map_err(storage)
    }

    fn due(&self, account: AccountId, now: i64) -> Result<Vec<Op>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT o.id, o.kind, o.value, m.id, f.remote_id, f.validity, m.remote_id, t.remote_id
                 FROM op o JOIN message m ON m.id = o.message_id JOIN folder f ON f.id = m.folder_id
                 LEFT JOIN folder t ON t.id = o.target
                 WHERE f.account_id = ?1 AND o.not_before <= ?2
                 ORDER BY o.id",
            )
            .and_then(|mut s| {
                s.query_map(params![account.0, now], |r| {
                    let kind: StoredAction = r.get(1)?;
                    Ok(Op {
                        id: OpId(r.get(0)?),
                        action: kind.with(r.get(2)?),
                        location: MessageLocation {
                            id: MessageId(r.get(3)?),
                            account_id: account,
                            folder_remote_id: r.get(4)?,
                            validity: r.get(5)?,
                            remote_id: r.get(6)?,
                        },
                        to: r.get(7)?,
                    })
                })?
                .collect()
            });
        rows.map_err(storage)
    }

    fn finish(&self, ops: &[Op]) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        {
            // An op the user changed since it was read stays queued with the new value.
            let mut delete = tx
                .prepare_cached("DELETE FROM op WHERE id = ?1 AND value = ?2 RETURNING message_id")
                .map_err(storage)?;
            let mut drop_message = tx
                .prepare_cached("DELETE FROM message WHERE id = ?1")
                .map_err(storage)?;
            for op in ops {
                let (_, value) = stored(op.action);
                let gone: Option<i64> = delete
                    .query_row(params![op.id.0, value], |r| r.get(0))
                    .optional()
                    .map_err(storage)?;
                if let (
                    Some(message),
                    Action::Archive | Action::Trash | Action::Delete | Action::Empty,
                ) = (gone, op.action)
                {
                    drop_message.execute([message]).map_err(storage)?;
                }
            }
        }
        tx.commit().map_err(storage)
    }

    fn revert(&self, ops: &[Op]) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        for op in ops {
            // Gone or changed since it was read: nothing of this op to undo.
            let (_, value) = stored(op.action);
            take_back(
                &tx,
                "DELETE FROM op WHERE id = ?1 AND value = ?2 RETURNING message_id, kind, undo",
                op.id,
                value,
            )?;
        }
        tx.commit().map_err(storage)
    }
}

/// Applies the action to one message now and queues it, replacing a queued one of its kind.
fn queue_one(
    tx: &Transaction<'_>,
    message: MessageId,
    action: Action,
    not_before: i64,
) -> Result<OpId, StoreError> {
    let (kind, value) = stored(action);
    let (flags, queued): (u8, Option<i64>) = tx
        .prepare_cached(
            "SELECT m.flags, o.undo FROM message m
             LEFT JOIN op o ON o.message_id = m.id AND o.kind = ?2 WHERE m.id = ?1",
        )
        .and_then(|mut s| {
            s.query_row(params![message.0, kind], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
        })
        .map_err(storage)?
        .ok_or(StoreError::NotFound)?;
    // What to restore on revert or undo, and where a move goes.
    let (undo, target) = match action {
        Action::Seen(seen) => {
            set_seen(tx, message.0, seen)?;
            (queued.unwrap_or(i64::from(flags & SEEN != 0)), None)
        }
        // Undone by dropping the op, which shows the message again.
        Action::Archive => (0, Some(role_folder(tx, message, FolderRole::Archive)?)),
        Action::Trash => (0, Some(role_folder(tx, message, FolderRole::Trash)?)),
        Action::Delete | Action::Empty => (0, None),
    };
    // Kept even when it undoes itself: a push may be sending the older value right now.
    let id = tx
        .prepare_cached(
            "INSERT INTO op (message_id, kind, value, undo, not_before, target)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (message_id, kind) DO UPDATE SET
                 value = excluded.value, not_before = excluded.not_before,
                 target = excluded.target
             RETURNING id",
        )
        .and_then(|mut s| {
            s.query_row(
                params![message.0, kind, value, undo, not_before, target],
                |r| r.get(0),
            )
        })
        .map_err(storage)?;
    Ok(OpId(id))
}

/// Records the folder's validity; when it changed, drops its messages and checkpoint.
fn reset_if_new(tx: &Transaction<'_>, folder: FolderId, validity: u32) -> Result<(), StoreError> {
    let current: u32 = tx
        .query_row(
            "SELECT validity FROM folder WHERE id = ?1",
            [folder.0],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage)?
        .ok_or(StoreError::NotFound)?;
    if current != validity {
        tx.execute("DELETE FROM message WHERE folder_id = ?1", [folder.0])
            .map_err(storage)?;
        tx.execute(
            "UPDATE folder SET validity = ?2, next_id = 0, older_done = 0 WHERE id = ?1",
            params![folder.0, validity],
        )
        .map_err(storage)?;
    }
    Ok(())
}

/// Rewrites a known row only when something changed, so a resync does not grow the WAL.
/// An empty snippet keeps the one taken from a saved body.
fn upsert_messages(
    tx: &Transaction<'_>,
    folder: FolderId,
    messages: &[NewMessage],
) -> Result<(), StoreError> {
    let mut upsert = tx.prepare_cached(SAVE_MESSAGE).map_err(storage)?;
    let account: i64 = tx
        .query_row(
            "SELECT account_id FROM folder WHERE id = ?1",
            [folder.0],
            |r| r.get(0),
        )
        .map_err(storage)?;
    for m in messages {
        write_message(&mut upsert, folder, m)?;
        link_thread(tx, account, folder, m)?;
    }
    Ok(())
}

/// Runs `SAVE_MESSAGE` or `SAVE_FOUND` for `m`; the number of rows written.
fn write_message(
    statement: &mut CachedStatement<'_>,
    folder: FolderId,
    m: &NewMessage,
) -> Result<usize, StoreError> {
    statement
        .execute(params![
            folder.0,
            m.remote_id,
            m.message_id,
            m.from.name,
            m.from.email,
            m.subject,
            m.snippet,
            m.date,
            flags_bits(m.flags),
            category_ref(m.category),
            m.answer_to.join("\n"),
            m.recipients.join("\n"),
        ])
        .map_err(storage)
}

/// Puts a saved message in the conversation of the messages it answers, that answer it, or
/// that answer the same ones, in its account. Conversations it joins become one.
fn link_thread(
    tx: &Transaction<'_>,
    account: i64,
    folder: FolderId,
    m: &NewMessage,
) -> Result<(), StoreError> {
    let (id, own): (i64, Option<i64>) = tx
        .prepare_cached("SELECT id, thread_id FROM message WHERE folder_id = ?1 AND remote_id = ?2")
        .and_then(|mut s| {
            s.query_row(params![folder.0, m.remote_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
        })
        .map_err(storage)?;
    let mut threads = vec![own.unwrap_or(id)];
    {
        // A message saved before was linked then; later ones link to it themselves.
        let saved: i64 = tx
            .prepare_cached("SELECT count(*) FROM message_ref WHERE message_id = ?1")
            .and_then(|mut s| s.query_row([id], |r| r.get(0)))
            .map_err(storage)?;
        if own.is_some() && usize::try_from(saved).ok() == Some(m.references.len()) {
            return Ok(());
        }
        let mut save = tx
            .prepare_cached("INSERT OR IGNORE INTO message_ref (message_id, ref) VALUES (?1, ?2)")
            .map_err(storage)?;
        for reference in &m.references {
            save.execute(params![id, reference]).map_err(storage)?;
        }
        let mut find = tx.prepare_cached(FIND_THREADS).map_err(storage)?;
        for key in m.references.iter().chain(&m.message_id) {
            let found = find
                .query_map(params![account, id, key], |r| r.get(0))
                .and_then(|rows| rows.collect::<Result<Vec<i64>, _>>())
                .map_err(storage)?;
            threads.extend(found);
        }
    }
    threads.sort_unstable();
    threads.dedup();
    let thread = threads[0];
    if own != Some(thread) {
        tx.execute(
            "UPDATE message SET thread_id = ?1 WHERE id = ?2",
            [thread, id],
        )
        .map_err(storage)?;
    }
    let mut merge = tx
        .prepare_cached("UPDATE message SET thread_id = ?1 WHERE thread_id = ?2")
        .map_err(storage)?;
    for other in &threads[1..] {
        merge.execute([thread, *other]).map_err(storage)?;
    }
    Ok(())
}

/// The folder of the message's account with `role`, the archive or the trash.
fn role_folder(
    tx: &Transaction<'_>,
    message: MessageId,
    role: FolderRole,
) -> Result<i64, StoreError> {
    let missing = if role == FolderRole::Trash {
        StoreError::NoTrash
    } else {
        StoreError::NoArchive
    };
    tx.prepare_cached(
        "SELECT a.id FROM message m JOIN folder f ON f.id = m.folder_id
         JOIN folder a ON a.account_id = f.account_id AND a.role = ?2
         WHERE m.id = ?1 ORDER BY a.id LIMIT 1",
    )
    .and_then(|mut s| {
        s.query_row(params![message.0, role_str(role)], |r| r.get(0))
            .optional()
    })
    .map_err(storage)?
    .ok_or(missing)
}

fn folder_role(tx: &Transaction<'_>, message: MessageId) -> Result<FolderRole, StoreError> {
    tx.prepare_cached(
        "SELECT f.role FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
    )
    .and_then(|mut s| s.query_row([message.0], |r| r.get::<_, Role>(0)))
    .map(|role| role.0)
    .map_err(storage)
}

/// Drops the op if `sql`, a `DELETE ... RETURNING message_id, kind, undo` with the op id
/// as `?1` and `arg` as `?2`, finds it, and restores what it changed on the device.
fn take_back(tx: &Transaction<'_>, sql: &str, op: OpId, arg: i64) -> Result<bool, StoreError> {
    let found: Option<(i64, StoredAction, i64)> = tx
        .prepare_cached(sql)
        .and_then(|mut s| {
            s.query_row(params![op.0, arg], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()
        })
        .map_err(storage)?;
    let Some((message, kind, undo)) = found else {
        return Ok(false);
    };
    match kind.with(undo) {
        Action::Seen(seen) => set_seen(tx, message, seen)?,
        // Without its op the message shows in its folder again.
        Action::Archive | Action::Trash | Action::Delete | Action::Empty => {}
    }
    Ok(true)
}

fn set_seen(tx: &Transaction<'_>, message: i64, seen: bool) -> Result<(), StoreError> {
    tx.prepare_cached("UPDATE message SET flags = (flags & ~?2) | ?3 WHERE id = ?1")
        .and_then(|mut s| s.execute(params![message, SEEN, if seen { SEEN } else { 0 }]))
        .map_err(storage)
        .map(|_| ())
}

/// The flags to keep for a known message: the server's, except the read state while a
/// queued "seen" action has not reached the server. `1` is `SEEN`.
macro_rules! merged_flags {
    ($server:literal) => {
        concat!(
            "CASE WHEN EXISTS (SELECT 1 FROM op WHERE op.message_id = message.id AND op.kind = 'seen')
              THEN (", $server, " & ~1) | (flags & 1) ELSE ", $server, " END"
        )
    };
}

/// Sets the flags of a saved message from the server's, under a queued action.
const SAVE_FLAGS: &str = concat!(
    "UPDATE message SET flags = ",
    merged_flags!("?3"),
    " WHERE folder_id = ?1 AND remote_id = ?2 AND flags IS NOT ",
    merged_flags!("?3")
);

const SAVE_MESSAGE: &str = concat!(
    "INSERT INTO message (folder_id, remote_id, message_id, from_name, from_email,
                          subject, snippet, date, flags, category, answer_to, recipients, fresh)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1)
     ON CONFLICT (folder_id, remote_id) DO UPDATE SET
         found = 0, message_id = excluded.message_id, from_name = excluded.from_name,
         from_email = excluded.from_email, subject = excluded.subject,
         answer_to = excluded.answer_to, recipients = excluded.recipients,
         snippet = coalesce(nullif(excluded.snippet, ''), snippet),
         date = excluded.date, category = excluded.category, flags = ",
    merged_flags!("excluded.flags"),
    " WHERE (found, message_id, from_name, from_email, subject, snippet, date, category,
            answer_to, recipients, flags)
         IS NOT (0, excluded.message_id, excluded.from_name, excluded.from_email,
                 excluded.subject, coalesce(nullif(excluded.snippet, ''), snippet),
                 excluded.date, excluded.category, excluded.answer_to, excluded.recipients, ",
    merged_flags!("excluded.flags"),
    ")"
);

/// Lists leave out a message while its move to the archive or trash is held.
macro_rules! not_moving {
    () => {
        not_moving!("m")
    };
    ($m:literal) => {
        concat!(
            "NOT EXISTS (SELECT 1 FROM op WHERE op.message_id = ",
            $m,
            ".id AND op.kind IN ('archive', 'trash', 'delete', 'empty'))"
        )
    };
}

/// The messages of `m`'s conversation, `n`, that lists show with it: in its folder and
/// inbox tab.
macro_rules! in_thread {
    () => {
        concat!(
            "FROM message n WHERE n.thread_id = m.thread_id AND n.folder_id = m.folder_id
             AND n.category = m.category AND n.found = 0 AND ",
            not_moving!("n")
        )
    };
}

/// Message `$n`, in folder `$nf`, is in the conversation of `$of` as the reader shows it:
/// in `$of`'s folder and inbox tab, or the user's reply from the sent folder.
macro_rules! in_conversation {
    ($n:literal, $nf:literal, $of:literal) => {
        concat!(
            $n,
            ".thread_id = ",
            $of,
            ".thread_id AND ((",
            $n,
            ".folder_id = ",
            $of,
            ".folder_id AND ",
            $n,
            ".category = ",
            $of,
            ".category) OR (",
            $nf,
            ".role = 'sent' AND NOT EXISTS (
                -- Mail to oneself, or a list that sends one's post back: shown once.
                SELECT 1 FROM message x WHERE x.thread_id = ",
            $of,
            ".thread_id
                AND x.folder_id = ",
            $of,
            ".folder_id AND x.message_id = ",
            $n,
            ".message_id)))"
        )
    };
}

/// One sender for `parse_senders`: the date, whether it is in the sent folder, the address
/// and the name, without the separators.
macro_rules! sender {
    ($m:literal, $f:literal) => {
        concat!(
            $m,
            ".date || char(31) || coalesce(",
            $f,
            ".role = 'sent', 0) || char(31) || replace(replace(",
            $m,
            ".from_email, char(30), ''), char(31), '') || char(31) || replace(replace(coalesce(",
            $m,
            ".from_name, ''), char(30), ''), char(31), '')"
        )
    };
}

/// The columns `message_row` reads, in its order, with `$thread`'s unread count and
/// senders, one per message. `1` is `SEEN`.
macro_rules! message_from {
    // The message on its own.
    () => {
        message_from!(concat!("1 - (m.flags & 1), ", sender!("m", "f")))
    };
    ($thread:expr) => {
        concat!(
            "SELECT m.id, f.account_id, m.from_name, m.from_email, m.subject, m.snippet, m.date,
                    m.flags, ",
            $thread,
            ", f.role, (SELECT group_concat(part || char(31) || name, char(30)) FROM attachment
                         WHERE message_id = m.id)",
            " FROM message m JOIN folder f ON f.id = m.folder_id"
        )
    };
}

macro_rules! message_select {
    () => {
        concat!(message_from!(), " WHERE ")
    };
}

/// Newest first, keyset-paged by (date, id) so deep pages cost the same as the first.
macro_rules! list_sql {
    ($scope:literal) => {
        concat!(
            message_from!(concat!(
                "(SELECT count(*) ",
                in_thread!(),
                " AND n.flags & 1 = 0), (SELECT group_concat(",
                sender!("n", "nf"),
                ", char(30)) FROM message n JOIN folder nf ON nf.id = n.folder_id WHERE ",
                in_conversation!("n", "nf", "m"),
                " AND ",
                not_moving!("n"),
                ")"
            )),
            " WHERE ",
            $scope,
            " AND m.found = 0 AND (m.date, m.id) < (?2, ?3)
             AND ",
            not_moving!(),
            // One row per conversation: its newest message.
            " AND NOT EXISTS (SELECT 1 ",
            in_thread!(),
            " AND (n.date, n.id) > (m.date, m.id))
             ORDER BY m.date DESC, m.id DESC
             LIMIT ?4"
        )
    };
}

const LIST_FOLDER: &str = list_sql!("m.folder_id = ?1");
// Walks the tab's date index; the `+` keeps SQLite from starting at the folder instead.
const LIST_INBOX_TAB: &str =
    list_sql!("m.category = ?1 AND +m.folder_id IN (SELECT id FROM folder WHERE role = 'inbox')");

/// The conversation of message `?1` in its folder and inbox tab, with the user's replies
/// from the sent folder, oldest first. `f` is the folder `message_from!` joins.
const THREAD: &str = concat!(
    message_from!(),
    " JOIN message s ON s.id = ?1 WHERE ",
    in_conversation!("m", "f", "s"),
    " AND ",
    not_moving!(),
    " ORDER BY m.date, m.id"
);

/// What an action on message `?1` covers: its conversation in its folder and inbox tab.
const THREAD_IDS: &str = concat!(
    "SELECT m.id, m.flags FROM message m
     JOIN message s ON s.thread_id = m.thread_id AND s.folder_id = m.folder_id
         AND s.category = m.category
     WHERE s.id = ?1 AND ",
    not_moving!(),
    " ORDER BY m.date, m.id"
);

/// Conversations in account `?1` of messages, other than `?2`, with Message-ID `?3`
/// or answering it.
const FIND_THREADS: &str = "
    SELECT m.thread_id FROM message m JOIN folder f ON f.id = m.folder_id
    WHERE m.message_id = ?3 AND f.account_id = ?1 AND m.id != ?2 AND m.thread_id IS NOT NULL
    UNION ALL
    SELECT m.thread_id FROM message_ref r JOIN message m ON m.id = r.message_id
        JOIN folder f ON f.id = m.folder_id
    WHERE r.ref = ?3 AND f.account_id = ?1 AND m.id != ?2 AND m.thread_id IS NOT NULL";

/// Copies of one message in several folders of an account show once: the newest copy.
/// Sorting by date reads every match; fine for one person's mail.
const SEARCH: &str = concat!(
    message_from!(),
    " WHERE m.id IN (SELECT rowid FROM message_search WHERE message_search MATCH ?1
                     UNION SELECT message_id FROM search_hit WHERE words = ?3)
       AND f.role NOT IN ('trash', 'junk') AND ",
    not_moving!(),
    " AND NOT EXISTS (
         SELECT 1 FROM message d JOIN folder g ON g.id = d.folder_id
         WHERE d.message_id = m.message_id AND d.id > m.id AND g.account_id = f.account_id
           AND g.role NOT IN ('trash', 'junk') AND ",
    not_moving!("d"),
    ")
     ORDER BY m.date DESC, m.id DESC LIMIT ?2"
);

/// Each word of `text` as a quoted prefix, so FTS5 syntax in it stays plain text, with ё
/// folded as in the index. `None` when there are no words.
fn fts_query(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.replace('"', "").replace('ё', "е").replace('Ё', "Е"))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// The words of `text` as the server searches of `search_hit` are kept by: lower case, one
/// space apart. `None` when there are none.
fn hit_words(text: &str) -> Option<String> {
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// Saves a message a server search found, unless it is saved already; not new mail.
const SAVE_FOUND: &str = "
    INSERT INTO message (folder_id, remote_id, message_id, from_name, from_email, subject,
                         snippet, date, flags, category, answer_to, recipients, found)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1)
    ON CONFLICT (folder_id, remote_id) DO NOTHING";

const CLEAR_FRESH: &str = "UPDATE message SET fresh = 0 WHERE fresh = 1";

// `1` is `SEEN`.
const FRESH_PEOPLE: &str = concat!(
    message_select!(),
    "m.fresh = 1 AND m.date > ?1 AND f.role = 'inbox' AND m.category = 'people' AND m.flags & 1 = 0 AND ",
    not_moving!(),
    " ORDER BY m.date DESC, m.id DESC"
);

// `1` is `SEEN`.
const INBOX_UNREAD: &str = concat!(
    "SELECT m.category, count(*) FROM message m
     WHERE m.folder_id IN (SELECT id FROM folder WHERE role = 'inbox') AND m.flags & 1 = 0
       AND m.found = 0 AND ",
    not_moving!(),
    " GROUP BY m.category"
);
// The messages of a folder, leaving out those on their way out.
/// Inbox mail of an account, newest `?2` of it, still without a body and staying.
const WITHOUT_BODY: &str = concat!(
    "SELECT m.id, f.remote_id, f.validity, m.remote_id
     FROM (SELECT id, folder_id, remote_id, date FROM message
           WHERE folder_id IN (SELECT id FROM folder WHERE account_id = ?1 AND role = 'inbox')
           ORDER BY date DESC, id DESC LIMIT ?2) m
     JOIN folder f ON f.id = m.folder_id
     WHERE NOT EXISTS (SELECT 1 FROM message_body b WHERE b.message_id = m.id) AND ",
    not_moving!(),
    " ORDER BY m.date DESC, m.id DESC"
);

const FOLDER_MESSAGES: &str = concat!(
    "SELECT m.id FROM message m WHERE m.folder_id = ?1 AND ",
    not_moving!()
);

// Stops at the first message found, so a full inbox costs no more than an empty one.
const INBOX_EMPTY: &str = concat!(
    "SELECT NOT EXISTS (SELECT 1 FROM message m
     WHERE m.folder_id IN (SELECT id FROM folder WHERE role = 'inbox') AND m.found = 0 AND ",
    not_moving!(),
    ")"
);
// The unary `+` keeps SQLite on the date index: rows come out in order and the scan stops at LIMIT.
// Right for roles that hold most messages; a sparse role would scan the whole table this way.
const LIST_UNIFIED_BY_DATE: &str =
    list_sql!("+m.folder_id IN (SELECT id FROM folder WHERE role = ?1)");
// Sparse roles read each folder's index range and sort the few rows they find.
const LIST_UNIFIED_BY_FOLDER: &str =
    list_sql!("m.folder_id IN (SELECT id FROM folder WHERE role = ?1)");

fn walks_by_date(role: FolderRole) -> bool {
    matches!(
        role,
        FolderRole::Inbox | FolderRole::Archive | FolderRole::Sent
    )
}

fn category_ref(category: Category) -> &'static &'static str {
    match category {
        Category::People => &"people",
        Category::Notifications => &"notifications",
        Category::Newsletters => &"newsletters",
    }
}

/// The addresses of a column that holds one per line.
fn recipients(stored: &str) -> Vec<String> {
    stored.lines().map(str::to_owned).collect()
}

fn role_ref(role: FolderRole) -> &'static &'static str {
    match role {
        FolderRole::Inbox => &"inbox",
        FolderRole::Sent => &"sent",
        FolderRole::Drafts => &"drafts",
        FolderRole::Archive => &"archive",
        FolderRole::Trash => &"trash",
        FolderRole::Junk => &"junk",
        FolderRole::Flagged => &"flagged",
        FolderRole::Custom => &"custom",
    }
}

fn role_str(role: FolderRole) -> &'static str {
    role_ref(role)
}

/// Fails with `NotFound` when a write changed no row.
fn found(changed: usize) -> Result<(), StoreError> {
    if changed == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// Saves a new draft; an account or answered message that is gone is left out.
fn insert_draft(conn: &Connection, draft: &SavedDraft) -> Result<DraftId, StoreError> {
    conn.query_row(
        "INSERT INTO draft (account_id, recipients, subject, text, reply_to, quote, saved)
         VALUES ((SELECT id FROM account WHERE id = ?1), ?2, ?3, ?4,
                 (SELECT id FROM message WHERE id = ?5), ?6, ?7)
         RETURNING id",
        params![
            draft.account_id.map(|a| a.0),
            draft.to,
            draft.subject,
            draft.text,
            draft.reply_to.map(|m| m.0),
            draft.quote,
            draft.saved,
        ],
        |r| r.get(0),
    )
    .map(DraftId)
    .map_err(storage)
}

/// Fails with `NotFound` when the row a write depends on is missing.
fn require(tx: &Transaction<'_>, sql: &str, id: i64) -> Result<(), StoreError> {
    tx.query_row(sql, [id], |_| Ok(()))
        .optional()
        .map_err(storage)?
        .ok_or(StoreError::NotFound)
}

fn message_row(r: &Row<'_>) -> rusqlite::Result<MessageRow> {
    let (thread_count, senders) =
        parse_senders(&r.get::<_, Option<String>>(9)?.unwrap_or_default());
    Ok(MessageRow {
        id: MessageId(r.get(0)?),
        account_id: AccountId(r.get(1)?),
        from: Address {
            name: r.get(2)?,
            email: r.get(3)?,
        },
        subject: r.get(4)?,
        snippet: r.get(5)?,
        date: r.get(6)?,
        flags: parse_flags(r.get(7)?),
        thread_unread: r.get(8)?,
        thread_count,
        senders,
        role: r.get::<_, Role>(10)?.0,
        attachments: parse_attachments(&r.get::<_, Option<String>>(11)?.unwrap_or_default()),
    })
}

/// Attachment names as `message_from!` joins them, each after its part and `\x1f`, by `\x1e`;
/// in part order.
fn parse_attachments(joined: &str) -> Vec<String> {
    let mut files: Vec<(u32, &str)> = joined
        .split('\x1e')
        .filter_map(|one| {
            let (part, name) = one.split_once('\x1f')?;
            Some((part.parse().ok()?, name))
        })
        .collect();
    files.sort_unstable_by_key(|f| f.0);
    files.into_iter().map(|(_, name)| name.to_owned()).collect()
}

/// Senders as `sender!` writes them, joined by `\x1e`: how many messages, and who wrote.
fn parse_senders(joined: &str) -> (u32, Vec<Sender>) {
    let wrote: Vec<_> = joined
        .split('\x1e')
        .filter_map(|one| {
            let mut parts = one.splitn(4, '\x1f');
            let date = parts.next()?.parse().ok()?;
            Some((date, parts.next()? == "1", parts.next()?, parts.next()?))
        })
        .collect();
    let count = u32::try_from(wrote.len()).unwrap_or(u32::MAX);
    (count, Sender::collect(wrote))
}

fn storage(e: rusqlite::Error) -> StoreError {
    StoreError::Storage(e.to_string())
}

fn kind_str(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Imap => "imap",
        AccountKind::Gmail => "gmail",
        AccountKind::Graph => "graph",
    }
}

/// Stored strings are read strictly: an unknown value is an error, never a silent default.
struct Kind(AccountKind);
struct Role(FolderRole);

/// The kind column of `op`; the value column completes it.
#[derive(Clone, Copy)]
enum StoredAction {
    Seen,
    Archive,
    Trash,
    Delete,
    Empty,
}

/// The kind and value columns of `op` for an action.
fn stored(action: Action) -> (&'static str, i64) {
    match action {
        Action::Seen(seen) => ("seen", i64::from(seen)),
        Action::Archive => ("archive", 1),
        Action::Trash => ("trash", 1),
        Action::Delete => ("delete", 1),
        Action::Empty => ("empty", 1),
    }
}

impl StoredAction {
    fn with(self, value: i64) -> Action {
        match self {
            Self::Seen => Action::Seen(value != 0),
            Self::Archive => Action::Archive,
            Self::Trash => Action::Trash,
            Self::Delete => Action::Delete,
            Self::Empty => Action::Empty,
        }
    }
}

impl FromSql for StoredAction {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "seen" => Ok(Self::Seen),
            "archive" => Ok(Self::Archive),
            "trash" => Ok(Self::Trash),
            "delete" => Ok(Self::Delete),
            "empty" => Ok(Self::Empty),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl FromSql for Kind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "imap" => Ok(Self(AccountKind::Imap)),
            "gmail" => Ok(Self(AccountKind::Gmail)),
            "graph" => Ok(Self(AccountKind::Graph)),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

/// The category column of `message`.
struct StoredCategory(Category);

impl FromSql for StoredCategory {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let stored = value.as_str()?;
        Category::ALL
            .into_iter()
            .find(|c| *category_ref(*c) == stored)
            .map(Self)
            .ok_or(FromSqlError::InvalidType)
    }
}

impl FromSql for Role {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let role = match value.as_str()? {
            "inbox" => FolderRole::Inbox,
            "sent" => FolderRole::Sent,
            "drafts" => FolderRole::Drafts,
            "archive" => FolderRole::Archive,
            "trash" => FolderRole::Trash,
            "junk" => FolderRole::Junk,
            "flagged" => FolderRole::Flagged,
            "custom" => FolderRole::Custom,
            _ => return Err(FromSqlError::InvalidType),
        };
        Ok(Self(role))
    }
}

const SEEN: u8 = 1;
const FLAGGED: u8 = 2;
const ANSWERED: u8 = 4;
const DRAFT: u8 = 8;

fn flags_bits(f: Flags) -> u8 {
    (f.seen as u8 * SEEN)
        | (f.flagged as u8 * FLAGGED)
        | (f.answered as u8 * ANSWERED)
        | (f.draft as u8 * DRAFT)
}

fn parse_flags(bits: u8) -> Flags {
    Flags {
        seen: bits & SEEN != 0,
        flagged: bits & FLAGGED != 0,
        answered: bits & ANSWERED != 0,
        draft: bits & DRAFT != 0,
    }
}

#[cfg(test)]
mod tests;
