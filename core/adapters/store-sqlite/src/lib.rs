//! `Store` on SQLite, encrypted with SQLCipher.

mod schema;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use ozero_domain::{
    Account, AccountId, AccountKind, Action, Address, Body, Category, Draft, Flags, Folder,
    FolderId, FolderRole, ListQuery, ListScope, MessageId, MessageLocation, MessageRow, NewAccount,
    NewFolder, NewMessage, Op, OpId, OutboxId, Outgoing, Server, Store, StoreError, Unsent,
};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Row, Transaction, params};

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
        let conn = self.conn();
        let (server, smtp) = (account.server.as_ref(), account.smtp.as_ref());
        conn.execute(
            "INSERT INTO account (kind, email, display_name, server_host, server_port, server_user,
                                  smtp_host, smtp_port, smtp_user)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
        )
        .map_err(storage)?;
        Ok(AccountId(conn.last_insert_rowid()))
    }

    fn accounts(&self) -> Result<Vec<Account>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, kind, email, display_name, server_host, server_port, server_user,
                        smtp_host, smtp_port, smtp_user
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
            })
        });
        rows.and_then(|rows| rows.collect()).map_err(storage)
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

    fn set_validity(&self, folder: FolderId, validity: u32) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
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
                "UPDATE folder SET validity = ?2 WHERE id = ?1",
                params![folder.0, validity],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)
    }

    fn save_messages(&self, folder: FolderId, messages: &[NewMessage]) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        require(&tx, "SELECT 1 FROM folder WHERE id = ?1", folder.0)?;
        {
            // Rewrites a known row only when something changed, so a resync does not grow the WAL.
            // An empty snippet keeps the one taken from a saved body.
            let mut upsert = tx.prepare_cached(SAVE_MESSAGE).map_err(storage)?;
            for m in messages {
                upsert
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
                    ])
                    .map_err(storage)?;
            }
        }
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

    fn take_fresh(&self) -> Result<Vec<MessageRow>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let rows = tx
            .prepare_cached(FRESH_PEOPLE)
            .and_then(|mut s| s.query_map([], message_row)?.collect())
            .map_err(storage)?;
        tx.execute("UPDATE message SET fresh = 0 WHERE fresh = 1", [])
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(rows)
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
        tx.execute(
            "UPDATE message SET snippet = ?2 WHERE id = ?1",
            params![message.0, body.snippet()],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    fn body(&self, message: MessageId) -> Result<Option<Body>, StoreError> {
        self.conn()
            .prepare_cached("SELECT text, html FROM message_body WHERE message_id = ?1")
            .and_then(|mut s| {
                s.query_row([message.0], |r| {
                    Ok(Body {
                        text: r.get(0)?,
                        html: r.get(1)?,
                    })
                })
                .optional()
            })
            .map_err(storage)
    }

    fn queue(
        &self,
        message: MessageId,
        action: Action,
        not_before: i64,
    ) -> Result<OpId, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
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
                set_seen(&tx, message.0, seen)?;
                (queued.unwrap_or(i64::from(flags & SEEN != 0)), None)
            }
            // Undone by dropping the op, which shows the message again.
            Action::Archive => (0, Some(archive_folder(&tx, message)?)),
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
        tx.commit().map_err(storage)?;
        Ok(OpId(id))
    }

    fn undo(&self, op: OpId, now: i64) -> Result<bool, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let held = take_back(
            &tx,
            "DELETE FROM op WHERE id = ?1 AND not_before > ?2 RETURNING message_id, kind, undo",
            op,
            now,
        )?;
        tx.commit().map_err(storage)?;
        Ok(held)
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
                                     message_id, date, not_before)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) RETURNING id",
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
                ],
                |r| r.get(0),
            )
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(OutboxId(id))
    }

    fn undo_send(&self, id: OutboxId, now: i64) -> Result<Option<Draft>, StoreError> {
        self.conn()
            .prepare_cached(
                "DELETE FROM outbox WHERE id = ?1 AND (not_before > ?2 OR failed IS NOT NULL)
                 RETURNING account_id, recipients, subject, text, reply_to",
            )
            .and_then(|mut s| {
                s.query_row(params![id.0, now], |r| {
                    Ok(Draft {
                        account_id: AccountId(r.get(0)?),
                        to: recipients(&r.get::<_, String>(1)?),
                        subject: r.get(2)?,
                        text: r.get(3)?,
                        reply_to: r.get::<_, Option<i64>>(4)?.map(MessageId),
                    })
                })
                .optional()
            })
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
                "SELECT id, recipients, subject, text, message_id, in_reply_to, date FROM outbox
                 WHERE account_id = ?1 AND not_before <= ?2 AND failed IS NULL ORDER BY id",
            )
            .and_then(|mut s| {
                s.query_map(params![account.0, now], |r| {
                    Ok(Outgoing {
                        id: OutboxId(r.get(0)?),
                        to: recipients(&r.get::<_, String>(1)?),
                        subject: r.get(2)?,
                        text: r.get(3)?,
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
                if let (Some(message), Action::Archive) = (gone, op.action) {
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

/// The folder of the message's account that has the archive role.
fn archive_folder(tx: &Transaction<'_>, message: MessageId) -> Result<i64, StoreError> {
    tx.prepare_cached(
        "SELECT a.id FROM message m JOIN folder f ON f.id = m.folder_id
         JOIN folder a ON a.account_id = f.account_id AND a.role = ?2
         WHERE m.id = ?1 ORDER BY a.id LIMIT 1",
    )
    .and_then(|mut s| {
        s.query_row(params![message.0, role_str(FolderRole::Archive)], |r| {
            r.get(0)
        })
        .optional()
    })
    .map_err(storage)?
    .ok_or(StoreError::NoArchive)
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
        Action::Archive => {}
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
    () => {
        "CASE WHEN EXISTS (SELECT 1 FROM op WHERE op.message_id = message.id AND op.kind = 'seen')
              THEN (excluded.flags & ~1) | (flags & 1) ELSE excluded.flags END"
    };
}

const SAVE_MESSAGE: &str = concat!(
    "INSERT INTO message (folder_id, remote_id, message_id, from_name, from_email,
                          subject, snippet, date, flags, category, fresh)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1)
     ON CONFLICT (folder_id, remote_id) DO UPDATE SET
         message_id = excluded.message_id, from_name = excluded.from_name,
         from_email = excluded.from_email, subject = excluded.subject,
         snippet = coalesce(nullif(excluded.snippet, ''), snippet),
         date = excluded.date, category = excluded.category, flags = ",
    merged_flags!(),
    " WHERE (message_id, from_name, from_email, subject, snippet, date, category, flags)
         IS NOT (excluded.message_id, excluded.from_name, excluded.from_email,
                 excluded.subject, coalesce(nullif(excluded.snippet, ''), snippet),
                 excluded.date, excluded.category, ",
    merged_flags!(),
    ")"
);

/// Lists leave out a message while its move to the archive is held.
macro_rules! not_archiving {
    () => {
        "NOT EXISTS (SELECT 1 FROM op WHERE op.message_id = m.id AND op.kind = 'archive')"
    };
}

/// Newest first, keyset-paged by (date, id) so deep pages cost the same as the first.
macro_rules! list_sql {
    ($scope:literal) => {
        concat!(
            "SELECT m.id, f.account_id, m.from_name, m.from_email, m.subject, m.snippet, m.date, m.flags
             FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE ",
            $scope,
            " AND (m.date, m.id) < (?2, ?3)
             AND ",
            not_archiving!(),
            "
             ORDER BY m.date DESC, m.id DESC
             LIMIT ?4"
        )
    };
}

const LIST_FOLDER: &str = list_sql!("m.folder_id = ?1");
// Walks the tab's date index; the `+` keeps SQLite from starting at the folder instead.
const LIST_INBOX_TAB: &str =
    list_sql!("m.category = ?1 AND +m.folder_id IN (SELECT id FROM folder WHERE role = 'inbox')");

// `1` is `SEEN`.
const FRESH_PEOPLE: &str = concat!(
    "SELECT m.id, f.account_id, m.from_name, m.from_email, m.subject, m.snippet, m.date, m.flags
     FROM message m JOIN folder f ON f.id = m.folder_id
     WHERE m.fresh = 1 AND f.role = 'inbox' AND m.category = 'people' AND m.flags & 1 = 0 AND ",
    not_archiving!(),
    " ORDER BY m.date DESC, m.id DESC"
);

// `1` is `SEEN`.
const INBOX_UNREAD: &str = concat!(
    "SELECT m.category, count(*) FROM message m
     WHERE m.folder_id IN (SELECT id FROM folder WHERE role = 'inbox') AND m.flags & 1 = 0 AND ",
    not_archiving!(),
    " GROUP BY m.category"
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

/// The addresses of `outbox.recipients`, one per line.
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

/// Fails with `NotFound` when the row a write depends on is missing.
fn require(tx: &Transaction<'_>, sql: &str, id: i64) -> Result<(), StoreError> {
    tx.query_row(sql, [id], |_| Ok(()))
        .optional()
        .map_err(storage)?
        .ok_or(StoreError::NotFound)
}

fn message_row(r: &Row<'_>) -> rusqlite::Result<MessageRow> {
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
    })
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
}

/// The kind and value columns of `op` for an action.
fn stored(action: Action) -> (&'static str, i64) {
    match action {
        Action::Seen(seen) => ("seen", i64::from(seen)),
        Action::Archive => ("archive", 1),
    }
}

impl StoredAction {
    fn with(self, value: i64) -> Action {
        match self {
            Self::Seen => Action::Seen(value != 0),
            Self::Archive => Action::Archive,
        }
    }
}

impl FromSql for StoredAction {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "seen" => Ok(Self::Seen),
            "archive" => Ok(Self::Archive),
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
