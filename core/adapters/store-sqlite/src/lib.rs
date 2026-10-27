//! `Store` on SQLite, encrypted with SQLCipher.

mod schema;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use ozero_domain::{
    Account, AccountId, AccountKind, Action, Address, Attachment, Body, Category, Changes,
    Checkpoint, Correspondents, Draft, DraftCopy, DraftFile, DraftId, DraftToPush, FileId, Flags,
    Folder, FolderId, FolderRole, ListQuery, ListScope, MessageId, MessageLocation, MessageRow,
    Moved, NewAccount, NewFolder, NewMail, NewMessage, Op, OpId, OutboxId, Outgoing, OutgoingFile,
    RecentFile, SavedDraft, Sender, Server, ServerDraft, Store, StoreError, Unsent, Unsubscribe,
    default_label, full_text, typed_addresses,
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
        // Drafts thrown away in an earlier run are past undo, and their files go with them.
        conn.execute("DELETE FROM draft WHERE gone IS NOT NULL", [])
            .map_err(storage)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Calls `changed` as each write commits; reads and refused writes do not. It runs while
    /// the database is held, so it must not use the store.
    pub fn on_change(&self, changed: impl Fn() + Send + 'static) -> Result<(), StoreError> {
        // SQLite calls it only for transactions that wrote, but before they are on disk.
        self.conn()
            .commit_hook(Some(move || {
                changed();
                // Never turns the commit into a rollback.
                false
            }))
            .map_err(storage)
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

/// Whether the person chose to see all mail's images.
macro_rules! images_always {
    () => {
        "EXISTS (SELECT 1 FROM setting WHERE key = 'images' AND value = 'always')"
    };
}

/// `$m` is a copy of moved mail whose move the server has, or of sent mail the server took,
/// so a sync can find the server's own message in its place.
macro_rules! unclaimed {
    ($m:literal) => {
        concat!(
            $m,
            ".stand_in = 1 AND NOT EXISTS (SELECT 1 FROM op WHERE op.copy = ",
            $m,
            ".id) AND NOT EXISTS (SELECT 1 FROM outbox WHERE outbox.copy = ",
            $m,
            ".id)"
        )
    };
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
                                      server_user, server_certificate, smtp_host, smtp_port,
                                      smtp_user, smtp_certificate, new_mail, color, label,
                                      position)
                 VALUES ((SELECT last + 1 FROM account_seq), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                         ?10, ?11, ?13,
                         (SELECT min(c) FROM (SELECT 0 AS c UNION SELECT color + 1 FROM account)
                          WHERE c NOT IN (SELECT color FROM account)),
                         -- A second Gmail goes by its address rather than the same name.
                         CASE WHEN EXISTS (SELECT 1 FROM account WHERE label = ?12)
                              THEN '' ELSE ?12 END,
                         -- Last in the person's order.
                         (SELECT coalesce(max(position), 0) + 1 FROM account))
                 RETURNING id",
                params![
                    kind_str(account.kind),
                    account.email,
                    account.display_name,
                    server.map(|s| &s.host),
                    server.map(|s| s.port),
                    server.map(|s| &s.user),
                    server.and_then(|s| s.certificate.as_ref()),
                    smtp.map(|s| &s.host),
                    smtp.map(|s| s.port),
                    smtp.map(|s| &s.user),
                    smtp.and_then(|s| s.certificate.as_ref()),
                    default_label(&account.email),
                    new_mail_value(NewMail::default()),
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
                        server_certificate, smtp_host, smtp_port, smtp_user, smtp_certificate,
                        color, label, keep_days, needs_sign_in, new_mail, signature, in_all, notify
                 FROM account ORDER BY position, id",
            )
            .map_err(storage)?;
        let rows = stmt.query_map([], |r| {
            let server = |at: usize| -> rusqlite::Result<Option<Server>> {
                Ok(match (r.get(at)?, r.get(at + 1)?, r.get(at + 2)?) {
                    (Some(host), Some(port), Some(user)) => Some(Server {
                        host,
                        port,
                        user,
                        certificate: r.get(at + 3)?,
                    }),
                    _ => None,
                })
            };
            Ok(Account {
                id: AccountId(r.get(0)?),
                kind: r.get::<_, Kind>(1)?.0,
                email: r.get(2)?,
                display_name: r.get(3)?,
                server: server(4)?,
                smtp: server(8)?,
                color: r.get(12)?,
                label: r.get(13)?,
                keep_days: r.get(14)?,
                needs_sign_in: r.get(15)?,
                new_mail: r.get::<_, StoredNewMail>(16)?.0,
                signature: r.get(17)?,
                in_all: r.get(18)?,
                notify: r.get(19)?,
            })
        });
        rows.and_then(|rows| rows.collect()).map_err(storage)
    }

    fn remove_account(&self, account: AccountId) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute("DELETE FROM account WHERE id = ?1", [account.0])
            .map_err(storage)?;
        // The unified inbox keeps one account at least: all of them, when the last one in it goes.
        self.conn()
            .execute(
                "UPDATE account SET in_all = 1 WHERE NOT EXISTS (SELECT 1 FROM account WHERE in_all = 1)",
                [],
            )
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

    fn set_account_name(&self, account: AccountId, name: &str) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE account SET display_name = ?2 WHERE id = ?1",
                params![account.0, name.trim()],
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

    fn set_account_order(&self, order: &[AccountId]) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        // The rest keep their order after these.
        tx.execute(
            "UPDATE account SET position = position + ?1",
            [order.len() as i64],
        )
        .map_err(storage)?;
        for (place, account) in (0_i64..).zip(order) {
            tx.execute(
                "UPDATE account SET position = ?2 WHERE id = ?1",
                params![account.0, place],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)
    }

    fn set_account_in_all(&self, account: AccountId, in_all: bool) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE account SET in_all = ?2 WHERE id = ?1",
                params![account.0, in_all],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn set_account_notify(&self, account: AccountId, notify: bool) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE account SET notify = ?2 WHERE id = ?1",
                params![account.0, notify],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn set_account_new_mail(&self, account: AccountId, when: NewMail) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE account SET new_mail = ?2 WHERE id = ?1",
                params![account.0, new_mail_value(when)],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn set_account_signature(&self, account: AccountId, signature: &str) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE account SET signature = ?2 WHERE id = ?1",
                params![account.0, signature.trim_end()],
            )
            .map_err(storage)?;
        found(changed)
    }

    fn signature(&self) -> Result<String, StoreError> {
        self.conn()
            .query_row(
                "SELECT value FROM setting WHERE key = 'signature'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map(Option::unwrap_or_default)
            .map_err(storage)
    }

    fn set_signature(&self, signature: &str) -> Result<(), StoreError> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO setting (key, value) VALUES ('signature', ?1)",
                [signature.trim_end()],
            )
            .map_err(storage)?;
        Ok(())
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

    fn provider_state(&self, account: AccountId, key: &str) -> Result<Option<String>, StoreError> {
        self.conn()
            .query_row(
                "SELECT value FROM provider_state WHERE account_id = ?1 AND key = ?2",
                params![account.0, key],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)
    }

    fn set_provider_state(
        &self,
        account: AccountId,
        key: &str,
        value: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        match value {
            Some(value) => conn.execute(
                "INSERT INTO provider_state (account_id, key, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT (account_id, key) DO UPDATE SET value = excluded.value",
                params![account.0, key, value],
            ),
            None => conn.execute(
                "DELETE FROM provider_state WHERE account_id = ?1 AND key = ?2",
                params![account.0, key],
            ),
        }
        .map_err(storage)?;
        Ok(())
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
                     RETURNING id, pinned",
                )
                .map_err(storage)?;
            folders
                .iter()
                .map(|f| {
                    let (id, pinned) = upsert
                        .query_row(
                            params![account.0, f.remote_id, f.name, role_str(f.role)],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .map_err(storage)?;
                    Ok(Folder {
                        id: FolderId(id),
                        account_id: account,
                        remote_id: f.remote_id.clone(),
                        name: f.name.clone(),
                        role: f.role,
                        pinned,
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
                "SELECT id, remote_id, name, role, pinned FROM folder WHERE account_id = ?1 ORDER BY id",
            )
            .map_err(storage)?;
        let rows = stmt.query_map([account.0], folder_row(account));
        rows.and_then(|rows| rows.collect()).map_err(storage)
    }

    fn set_folder_pinned(&self, folder: FolderId, pinned: bool) -> Result<(), StoreError> {
        let changed = self
            .conn()
            .execute(
                "UPDATE folder SET pinned = ?2 WHERE id = ?1",
                params![folder.0, pinned],
            )
            .map_err(storage)?;
        found(changed)
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
            .prepare_cached(
                "SELECT remote_id FROM message WHERE folder_id = ?1 AND found = 0 AND stand_in = 0",
            )
            .and_then(|mut s| s.query_map([folder.0], |r| r.get(0))?.collect())
            .map_err(storage)?;
        Ok((checkpoint, known))
    }

    fn oldest(&self, folder: FolderId) -> Result<Option<i64>, StoreError> {
        self.conn()
            .query_row(
                "SELECT min(date) FROM message WHERE folder_id = ?1 AND found = 0 AND stand_in = 0",
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
        // A page of older mail keeps the checkpoint, and never holds mail moved here since.
        let reached_new: bool = tx
            .query_row(
                "SELECT validity IS NOT ?2 OR next_id IS NOT ?3 FROM folder WHERE id = ?1",
                params![
                    folder.0,
                    changes.checkpoint.validity,
                    i64::try_from(changes.checkpoint.next).unwrap_or(0)
                ],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?
            .unwrap_or(false);
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
        if !changes.gone.is_empty() {
            copies_gone(&tx, folder, &changes.gone)?;
        }
        upsert_messages(&tx, folder, &changes.new)?;
        // A copy of moved mail the server's own did not take the place of: the server has it
        // somewhere this sync did not reach, such as older mail.
        if reached_new {
            tx.execute(
                concat!(
                    "DELETE FROM message WHERE folder_id = ?1 AND ",
                    unclaimed!("message")
                ),
                [folder.0],
            )
            .map_err(storage)?;
        }
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
        let inbox;
        // The value of `?1`, and of `?5` for a tab.
        let (sql, scope, tab): (&str, &dyn rusqlite::ToSql, _) = match &query.scope {
            ListScope::Folder(folder) => (LIST_FOLDER, &folder.0, None),
            ListScope::Inbox(account, category) => {
                inbox = account.map(|a| a.0);
                (LIST_INBOX_TAB, &inbox, Some(*category_ref(*category)))
            }
            // Their scopes take no value; `?1` stays unused.
            ListScope::Unread => (LIST_UNREAD, &0, None),
            ListScope::Files => (LIST_FILES, &0, None),
            ListScope::Unified(role) if walks_by_date(*role) => {
                (LIST_UNIFIED_BY_DATE, role_ref(*role), None)
            }
            ListScope::Unified(role) => (LIST_UNIFIED_BY_FOLDER, role_ref(*role), None),
        };
        let conn = self.conn();
        let rows = conn.prepare_cached(sql).and_then(|mut s| {
            match tab {
                Some(tab) => s.query_map(params![scope, date, id, query.limit, tab], message_row),
                None => s.query_map(params![scope, date, id, query.limit], message_row),
            }?
            .collect()
        });
        rows.map_err(storage)
    }

    fn inbox_unread(&self, account: Option<AccountId>) -> Result<Vec<(Category, u32)>, StoreError> {
        let conn = self.conn();
        let rows = conn.prepare_cached(INBOX_UNREAD).and_then(|mut s| {
            s.query_map([account.map(|a| a.0)], |r| {
                Ok((r.get::<_, StoredCategory>(0)?.0, r.get(1)?))
            })?
            .collect()
        });
        rows.map_err(storage)
    }

    fn unread_by_account(&self, category: Category) -> Result<Vec<(AccountId, u32)>, StoreError> {
        let conn = self.conn();
        let rows = conn.prepare_cached(UNREAD_BY_ACCOUNT).and_then(|mut s| {
            s.query_map([category_ref(category)], |r| {
                Ok((AccountId(r.get(0)?), r.get(1)?))
            })?
            .collect()
        });
        rows.map_err(storage)
    }

    fn inbox_is_empty(&self, account: Option<AccountId>) -> Result<bool, StoreError> {
        let conn = self.conn();
        conn.prepare_cached(INBOX_EMPTY)
            .and_then(|mut s| s.query_row([account.map(|a| a.0)], |r| r.get(0)))
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
                        write_unsubscribe(&tx, *folder, m)?;
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
                concat!(
                    "SELECT f.role IS NOT 'junk' AND (
                            m.category = 'people' AND m.from_email != ''
                            OR EXISTS (SELECT 1 FROM image_sender WHERE email = m.from_email)
                            OR ",
                    images_always!(),
                    ") FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1"
                ),
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

    fn image_senders(&self) -> Result<Vec<String>, StoreError> {
        self.conn()
            .prepare_cached("SELECT email FROM image_sender ORDER BY email")
            .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
            .map_err(storage)
    }

    fn forbid_images(&self, email: &str) -> Result<(), StoreError> {
        self.conn()
            .execute("DELETE FROM image_sender WHERE email = ?1", [email])
            .map(|_| ())
            .map_err(storage)
    }

    fn images_always(&self) -> Result<bool, StoreError> {
        self.conn()
            .query_row(concat!("SELECT ", images_always!()), [], |r| r.get(0))
            .map_err(storage)
    }

    fn set_images_always(&self, always: bool) -> Result<(), StoreError> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO setting (key, value) VALUES ('images', ?1)",
                [if always { "always" } else { "ask" }],
            )
            .map(|_| ())
            .map_err(storage)
    }

    fn correspondents(&self, message: MessageId) -> Result<Correspondents, StoreError> {
        self.conn()
            .prepare_cached("SELECT from_email, answer_to, recipients FROM message WHERE id = ?1")
            .and_then(|mut s| {
                s.query_row([message.0], |r| {
                    let (to, cc) = people(&r.get::<_, String>(2)?);
                    Ok(Correspondents {
                        from: r.get(0)?,
                        answer_to: recipients(&r.get::<_, String>(1)?),
                        to,
                        cc,
                    })
                })
                .optional()
            })
            .map_err(storage)?
            .ok_or(StoreError::NotFound)
    }

    fn unsubscribe(&self, message: MessageId) -> Result<Option<Unsubscribe>, StoreError> {
        self.conn()
            .prepare_cached(
                "SELECT links, one_click FROM unsubscribe WHERE message_id = ?1 AND state = 0",
            )
            .and_then(|mut s| {
                s.query_row([message.0], |r| {
                    Ok(Unsubscribe {
                        links: r.get(0)?,
                        one_click: r.get(1)?,
                    })
                })
                .optional()
            })
            .map_err(storage)
    }

    fn leave(&self, message: MessageId, queued: bool) -> Result<(), StoreError> {
        self.conn()
            .execute(
                "UPDATE unsubscribe SET state = ?2 WHERE message_id = ?1",
                params![message.0, if queued { 1 } else { 2 }],
            )
            .map_err(storage)?;
        Ok(())
    }

    fn leaves_due(&self, account: AccountId) -> Result<Vec<(MessageId, Unsubscribe)>, StoreError> {
        self.conn()
            .prepare_cached(
                "SELECT u.message_id, u.links, u.one_click FROM unsubscribe u
                 JOIN message m ON m.id = u.message_id JOIN folder f ON f.id = m.folder_id
                 WHERE u.state = 1 AND f.account_id = ?1",
            )
            .and_then(|mut s| {
                s.query_map([account.0], |r| {
                    Ok((
                        MessageId(r.get(0)?),
                        Unsubscribe {
                            links: r.get(1)?,
                            one_click: r.get(2)?,
                        },
                    ))
                })?
                .collect()
            })
            .map_err(storage)
    }

    fn location(&self, message: MessageId) -> Result<MessageLocation, StoreError> {
        self.conn()
            .prepare_cached(
                // A copy of moved mail is where the message is until the move is sent.
                "SELECT f.account_id, f.remote_id, f.validity, m.remote_id
                 FROM message m JOIN folder f ON f.id = m.folder_id
                 WHERE m.id = coalesce((SELECT message_id FROM op WHERE copy = ?1), ?1)",
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

    fn queue_all(
        &self,
        actions: &[(MessageId, Action)],
        not_before: i64,
    ) -> Result<Vec<OpId>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let ops = queue_threads(&tx, actions, not_before)?;
        tx.commit().map_err(storage)?;
        Ok(ops)
    }

    fn queue_alone(
        &self,
        actions: &[(MessageId, Action)],
        not_before: i64,
    ) -> Result<Vec<OpId>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let mut ops = vec![];
        for &(message, action) in actions {
            if !folder_role(&tx, message)?.allows(action) {
                return Err(StoreError::NotAllowed);
            }
            ops.push(queue_one(&tx, message, action, not_before)?);
        }
        tx.commit().map_err(storage)?;
        Ok(ops)
    }

    fn snooze(
        &self,
        messages: &[MessageId],
        until: i64,
        not_before: i64,
    ) -> Result<Vec<OpId>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let actions: Vec<_> = messages.iter().map(|&m| (m, Action::Snooze)).collect();
        let ops = queue_threads(&tx, &actions, not_before)?;
        {
            let mut wake = tx.prepare_cached(SNOOZE_UNTIL).map_err(storage)?;
            for op in &ops {
                wake.execute(params![op.0, until]).map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)?;
        Ok(ops)
    }

    fn wake(&self, now: i64, not_before: i64) -> Result<Vec<MessageRow>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        // Mail back for a month is dated as the server has it again.
        tx.execute(
            "DELETE FROM snooze WHERE woke = 1 AND wake_at < ?1",
            [now - WOKE_KEPT],
        )
        .map_err(storage)?;
        let due: Vec<(i64, i64, String)> = tx
            .prepare_cached(SNOOZED_DUE)
            .and_then(|mut s| {
                s.query_map([now], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .map_err(storage)?;
        let mut copies = vec![];
        for (message, account, key) in due {
            tx.prepare_cached(
                "UPDATE snooze SET woke = 1, wake_at = ?3 WHERE account_id = ?1 AND message_id = ?2",
            )
            .and_then(|mut s| s.execute(params![account, key, now]))
            .map_err(storage)?;
            queue_one(&tx, MessageId(message), Action::Seen(false), not_before)?;
            let op = queue_one(&tx, MessageId(message), Action::Inbox, not_before)?;
            let copy: Option<i64> = tx
                .prepare_cached(
                    "UPDATE message SET date = ?2 WHERE id = (SELECT copy FROM op WHERE id = ?1)
                     RETURNING id",
                )
                .and_then(|mut s| s.query_row(params![op.0, now], |r| r.get(0)).optional())
                .map_err(storage)?;
            copies.extend(copy);
        }
        // Undone, or gone from the snoozed folder some other way: nothing to bring back.
        tx.execute("DELETE FROM snooze WHERE woke = 0 AND wake_at <= ?1", [now])
            .map_err(storage)?;
        let rows = {
            let mut row = tx.prepare_cached(MESSAGE_BY_ID).map_err(storage)?;
            copies
                .iter()
                .map(|c| row.query_row([c], message_row).map_err(storage))
                .collect::<Result<Vec<_>, _>>()?
        };
        tx.commit().map_err(storage)?;
        Ok(rows)
    }

    fn next_wake(&self) -> Result<Option<i64>, StoreError> {
        let conn = self.conn();
        conn.query_row("SELECT min(wake_at) FROM snooze WHERE woke = 0", [], |r| {
            r.get(0)
        })
        .map_err(storage)
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
                s.query_map([folder.0], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<(i64, bool)>, _>>()
            })
            .map_err(storage)?;
        // A copy of moved mail goes on its own once the server has it there: emptying again
        // then would take what arrived since.
        let ops = ids
            .into_iter()
            .map(|(id, stand_in)| {
                let action = if stand_in {
                    Action::Delete
                } else {
                    Action::Empty
                };
                queue_one(&tx, MessageId(id), action, not_before)
            })
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
                 WHERE f.account_id = ?1 AND f.remote_id = ?2 AND m.stand_in = 0",
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
                "DELETE FROM op WHERE id = ?1 AND not_before > ?2
                 RETURNING message_id, kind, undo, copy",
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
                     WHERE f.account_id = ?1 AND m.stand_in = 0
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
                                     message_id, date, not_before, quote, forward, signature, cc)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                         (SELECT id FROM message WHERE id = ?11), ?12, ?13)
                 RETURNING id",
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
                    draft.forward.map(|m| m.0),
                    draft.signature,
                    draft.cc.join("\n"),
                ],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if let Some(saved) = saved {
            tx.execute(
                "UPDATE draft_file SET draft = NULL, outbox = ?2 WHERE draft = ?1",
                params![saved.0, id],
            )
            .map_err(storage)?;
            tx.execute("DELETE FROM draft WHERE id = ?1", [saved.0])
                .map_err(storage)?;
        }
        let copy = copy_into_sent(&tx, draft, message_id, in_reply_to.as_deref(), date)?;
        if let Some(copy) = copy {
            tx.execute(
                "UPDATE outbox SET copy = ?2 WHERE id = ?1",
                params![id, copy],
            )
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
                "SELECT account_id, recipients, subject, text, reply_to, quote, forward, copy,
                        signature, cc
                 FROM outbox WHERE id = ?1 AND (not_before > ?2 OR failed IS NOT NULL)",
                params![id.0, now],
                |r| {
                    let draft = SavedDraft {
                        id: None,
                        account_id: Some(AccountId(r.get(0)?)),
                        to: recipients(&r.get::<_, String>(1)?).join(", "),
                        cc: recipients(&r.get::<_, String>(9)?).join(", "),
                        subject: r.get(2)?,
                        text: r.get(3)?,
                        signature: r.get(8)?,
                        reply_to: r.get::<_, Option<i64>>(4)?.map(MessageId),
                        quote: r.get(5)?,
                        forward: r.get::<_, Option<i64>>(6)?.map(MessageId),
                        saved: now,
                        files: vec![],
                    };
                    Ok((draft, r.get::<_, Option<i64>>(7)?))
                },
            )
            .optional()
            .map_err(storage)?;
        let Some((mut draft, copy)) = taken else {
            return Ok(None);
        };
        if let Some(copy) = copy {
            drop_message(&tx, copy)?;
        }
        let saved = insert_draft(&tx, &draft)?;
        // The files move over before the message goes, which would take them with it.
        tx.execute(
            "UPDATE draft_file SET outbox = NULL, draft = ?2 WHERE outbox = ?1",
            params![id.0, saved.0],
        )
        .map_err(storage)?;
        tx.execute("DELETE FROM outbox WHERE id = ?1", [id.0])
            .map_err(storage)?;
        draft.files = files_of(&tx, saved)?;
        draft.id = Some(saved);
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
                     quote = ?6, saved = ?7,
                     forward = (SELECT id FROM message WHERE id = ?9), signature = ?10, cc = ?11
                 WHERE id = ?8 AND gone IS NULL",
                params![
                    draft.account_id.map(|a| a.0),
                    draft.to,
                    draft.subject,
                    draft.text,
                    draft.reply_to.map(|m| m.0),
                    draft.quote,
                    draft.saved,
                    id.0,
                    draft.forward.map(|m| m.0),
                    draft.signature,
                    draft.cc,
                ],
            )
            .map_err(storage)?;
        found(changed)?;
        Ok(id)
    }

    fn drafts(&self) -> Result<Vec<SavedDraft>, StoreError> {
        let conn = self.conn();
        let rows: Result<Vec<SavedDraft>, _> = conn
            .prepare_cached(
                "SELECT id, account_id, recipients, subject, text, reply_to, quote, saved, forward,
                        signature, cc
                 FROM draft WHERE gone IS NULL ORDER BY saved DESC, id DESC",
            )
            .and_then(|mut s| {
                s.query_map([], |r| {
                    Ok(SavedDraft {
                        id: Some(DraftId(r.get(0)?)),
                        account_id: r.get::<_, Option<i64>>(1)?.map(AccountId),
                        to: r.get(2)?,
                        cc: r.get(10)?,
                        subject: r.get(3)?,
                        text: r.get(4)?,
                        signature: r.get(9)?,
                        reply_to: r.get::<_, Option<i64>>(5)?.map(MessageId),
                        quote: r.get(6)?,
                        forward: r.get::<_, Option<i64>>(8)?.map(MessageId),
                        saved: r.get(7)?,
                        files: vec![],
                    })
                })?
                .collect()
            });
        let mut drafts = rows.map_err(storage)?;
        let files: Result<Vec<(i64, DraftFile)>, _> = conn
            .prepare_cached(
                "SELECT f.draft, f.id, f.name, f.mime, f.size
                 FROM draft_file f JOIN draft d ON d.id = f.draft
                 WHERE d.gone IS NULL ORDER BY f.id",
            )
            .and_then(|mut s| {
                s.query_map([], |r| Ok((r.get(0)?, draft_file(r, 1)?)))?
                    .collect()
            });
        let mut by_draft: HashMap<i64, Vec<DraftFile>> = HashMap::new();
        for (id, file) in files.map_err(storage)? {
            by_draft.entry(id).or_default().push(file);
        }
        for draft in &mut drafts {
            if let Some(DraftId(id)) = draft.id {
                draft.files = by_draft.remove(&id).unwrap_or_default();
            }
        }
        Ok(drafts)
    }

    fn delete_draft(&self, id: DraftId, now: i64, until: i64) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute("DELETE FROM draft WHERE gone <= ?1", [now])
            .map_err(storage)?;
        conn.execute(
            "UPDATE draft SET gone = ?2 WHERE id = ?1 AND gone IS NULL",
            params![id.0, until],
        )
        .map(|_| ())
        .map_err(storage)
    }

    fn drafts_to_push(&self, account: AccountId) -> Result<Vec<DraftToPush>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT d.id, d.saved, d.recipients, d.subject, d.text, d.signature, d.quote,
                        d.message_id, m.message_id, d.cc
                 FROM draft d LEFT JOIN message m ON m.id = d.reply_to
                 WHERE d.account_id = ?1 AND d.gone IS NULL AND d.pushed IS NOT d.saved
                 ORDER BY d.id",
            )
            .and_then(|mut s| {
                s.query_map([account.0], |r| {
                    let saved: i64 = r.get(1)?;
                    let text: String = r.get(4)?;
                    let signature: String = r.get(5)?;
                    let quote: String = r.get(6)?;
                    Ok(DraftToPush {
                        id: DraftId(r.get(0)?),
                        saved,
                        message: Outgoing {
                            id: OutboxId(0),
                            to: typed_addresses(&r.get::<_, String>(2)?),
                            cc: typed_addresses(&r.get::<_, String>(9)?),
                            subject: r.get(3)?,
                            text: full_text(&text, &signature, &quote),
                            message_id: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                            in_reply_to: r.get(8)?,
                            date: saved / 1_000,
                            // A forwarded message's files are read when it is sent.
                            forward: None,
                            files: vec![],
                        },
                    })
                })?
                .collect()
            });
        rows.map_err(storage)
    }

    fn draft_files(&self, id: DraftId) -> Result<Vec<OutgoingFile>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT f.name, f.mime, d.data, f.id FROM draft_file f JOIN file_data d ON d.file = f.id
                 WHERE f.draft = ?1 ORDER BY f.id",
            )
            .and_then(|mut s| s.query_map([id.0], outgoing_file)?.collect());
        rows.map_err(storage)
    }

    fn pushed_draft(
        &self,
        account: AccountId,
        id: DraftId,
        saved: i64,
        message_id: &str,
        copy: Option<&DraftCopy>,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let before: Option<Option<(i64, DraftCopy)>> = tx
            .query_row(
                "SELECT remote_account, remote_folder, remote_validity, remote_id
                 FROM draft WHERE id = ?1",
                [id.0],
                |r| {
                    Ok(match (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?) {
                        (Some(account), Some(folder), Some(validity), Some(remote_id)) => Some((
                            account,
                            DraftCopy {
                                folder,
                                validity,
                                remote_id,
                            },
                        )),
                        _ => None,
                    })
                },
            )
            .optional()
            .map_err(storage)?;
        let gone = |account: i64, copy: &DraftCopy| {
            tx.execute(
                "INSERT INTO draft_gone (account_id, folder, validity, remote_id, message_id)
                 SELECT ?1, ?2, ?3, ?4, NULL WHERE EXISTS (SELECT 1 FROM account WHERE id = ?1)",
                params![account, copy.folder, copy.validity, copy.remote_id],
            )
            .map(drop)
            .map_err(storage)
        };
        match before {
            // Sent or thrown away while it went: the new copy goes too.
            None => {
                if let Some(copy) = copy {
                    gone(account.0, copy)?;
                }
            }
            Some(before) => {
                if let Some((was, old)) = before.filter(|(_, old)| Some(old) != copy) {
                    gone(was, &old)?;
                }
                tx.execute(
                    "UPDATE draft SET pushed = ?2, message_id = ?3, remote_account = ?4,
                                      remote_folder = ?5, remote_validity = ?6, remote_id = ?7
                     WHERE id = ?1",
                    params![
                        id.0,
                        saved,
                        message_id,
                        copy.map(|_| account.0),
                        copy.map(|c| &c.folder),
                        copy.map(|c| c.validity),
                        copy.map(|c| &c.remote_id),
                    ],
                )
                .map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)
    }

    fn stale_drafts(
        &self,
        account: AccountId,
        now: i64,
    ) -> Result<Vec<(i64, DraftCopy)>, StoreError> {
        let conn = self.conn();
        conn.execute("DELETE FROM draft WHERE gone <= ?1", [now])
            .map_err(storage)?;
        let rows = conn
            .prepare_cached(
                "SELECT id, folder, validity, remote_id FROM draft_gone
                 WHERE account_id = ?1 ORDER BY id",
            )
            .and_then(|mut s| {
                s.query_map([account.0], |r| {
                    Ok((
                        r.get(0)?,
                        DraftCopy {
                            folder: r.get(1)?,
                            validity: r.get(2)?,
                            remote_id: r.get(3)?,
                        },
                    ))
                })?
                .collect()
            });
        rows.map_err(storage)
    }

    fn forget_stale_drafts(&self, ids: &[i64]) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        {
            let mut forget = tx
                .prepare_cached("DELETE FROM draft_gone WHERE id = ?1")
                .map_err(storage)?;
            for id in ids {
                forget.execute([id]).map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)
    }

    fn server_drafts(&self, account: AccountId) -> Result<Vec<ServerDraft>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        // A copy the server did not say the place of, found by the draft's Message-ID.
        tx.execute(
            "UPDATE draft SET remote_account = f.account_id, remote_folder = f.remote_id,
                              remote_validity = f.validity, remote_id = m.remote_id
             FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE draft.account_id = ?1 AND draft.remote_id IS NULL
               AND draft.pushed IS NOT NULL AND draft.message_id IS NOT NULL
               AND f.account_id = ?1 AND f.role = 'drafts' AND m.stand_in = 0
               AND m.message_id = draft.message_id",
            [account.0],
        )
        .map_err(storage)?;
        // An older copy of a draft, left by a push that broke off: it goes. A newer one is
        // another app's edit, left be. Only IMAP UIDs tell which is older.
        tx.execute(
            "INSERT INTO draft_gone (account_id, folder, validity, remote_id)
             SELECT ?1, f.remote_id, f.validity, m.remote_id
             FROM message m JOIN folder f ON f.id = m.folder_id
             JOIN draft d ON d.message_id = m.message_id AND d.remote_account = ?1
                 AND d.remote_folder = f.remote_id AND d.remote_validity = f.validity
             JOIN message c ON c.folder_id = m.folder_id AND c.remote_id = d.remote_id
             WHERE f.account_id = ?1 AND f.role = 'drafts' AND m.stand_in = 0
               AND m.remote_id NOT GLOB '*[^0-9]*' AND c.remote_id NOT GLOB '*[^0-9]*'
               AND CAST(m.remote_id AS INTEGER) < CAST(c.remote_id AS INTEGER)
               AND NOT EXISTS (SELECT 1 FROM draft_gone g WHERE g.account_id = ?1
                               AND g.folder = f.remote_id AND g.remote_id = m.remote_id)",
            [account.0],
        )
        .map_err(storage)?;
        let found = tx
            .prepare_cached(
                "SELECT m.id, f.remote_id, f.validity, m.remote_id, m.message_id
                 FROM message m JOIN folder f ON f.id = m.folder_id
                 WHERE f.account_id = ?1 AND f.role = 'drafts' AND m.stand_in = 0
                   AND NOT EXISTS (SELECT 1 FROM draft d
                                   WHERE d.message_id = m.message_id
                                      OR (d.remote_account = ?1 AND d.remote_folder = f.remote_id
                                          AND d.remote_validity = f.validity
                                          AND d.remote_id = m.remote_id))
                   AND NOT EXISTS (SELECT 1 FROM draft_gone g
                                   WHERE g.account_id = ?1
                                     AND (g.message_id = m.message_id
                                          OR (g.folder = f.remote_id AND g.remote_id = m.remote_id)))
                 ORDER BY m.date DESC, m.id DESC",
            )
            .and_then(|mut s| {
                s.query_map([account.0], |r| {
                    Ok(ServerDraft {
                        message: MessageId(r.get(0)?),
                        copy: DraftCopy {
                            folder: r.get(1)?,
                            validity: r.get(2)?,
                            remote_id: r.get(3)?,
                        },
                        message_id: r.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
            })
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(found)
    }

    fn import_draft(
        &self,
        draft: &SavedDraft,
        message_id: Option<&str>,
        copy: &DraftCopy,
        files: &[OutgoingFile],
    ) -> Result<DraftId, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let id = insert_draft(&tx, draft)?;
        tx.execute(
            "UPDATE draft SET pushed = saved, message_id = ?2, remote_account = account_id,
                              remote_folder = ?3, remote_validity = ?4, remote_id = ?5
             WHERE id = ?1",
            params![id.0, message_id, copy.folder, copy.validity, copy.remote_id],
        )
        .map_err(storage)?;
        for file in files {
            insert_file(&tx, id, file)?;
        }
        tx.commit().map_err(storage)?;
        Ok(id)
    }

    fn restore_draft(&self, id: DraftId, now: i64) -> Result<bool, StoreError> {
        self.conn()
            .execute(
                "UPDATE draft SET gone = NULL WHERE id = ?1 AND gone > ?2",
                params![id.0, now],
            )
            .map(|changed| changed > 0)
            .map_err(storage)
    }

    fn attach(
        &self,
        draft: DraftId,
        file: &OutgoingFile,
        limit: u64,
    ) -> Result<Option<DraftFile>, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        // What goes already: the files added, and those of the message it forwards.
        let taken: Option<i64> = tx
            .query_row(
                "SELECT (SELECT coalesce(sum(size), 0) FROM draft_file WHERE draft = d.id)
                      + (SELECT coalesce(sum(size), 0) FROM attachment WHERE message_id = d.forward)
                 FROM draft d WHERE d.id = ?1 AND d.gone IS NULL",
                [draft.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        let taken = u64::try_from(taken.ok_or(StoreError::NotFound)?).unwrap_or(0);
        let size = file.data.len() as u64;
        if taken + size > limit {
            return Ok(None);
        }
        let id = insert_file(&tx, draft, file)?;
        tx.commit().map_err(storage)?;
        Ok(Some(DraftFile {
            id: FileId(id),
            name: file.name.clone(),
            mime: file.mime.clone(),
            size,
        }))
    }

    fn recent_files(&self, limit: usize) -> Result<Vec<RecentFile>, StoreError> {
        let conn = self.conn();
        let mut statement = conn.prepare_cached(RECENT_FILES).map_err(storage)?;
        let rows = statement
            .query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |r| {
                Ok(RecentFile {
                    message: MessageId(r.get(0)?),
                    account_id: AccountId(r.get(1)?),
                    from_name: r.get(2)?,
                    from_email: r.get(3)?,
                    file: Attachment {
                        part: r.get(4)?,
                        name: r.get(5)?,
                        mime: r.get(6)?,
                        size: u64::try_from(r.get::<_, i64>(7)?).unwrap_or(0),
                    },
                })
            })
            .map_err(storage)?;
        rows.collect::<Result<_, _>>().map_err(storage)
    }

    fn detach(&self, file: FileId) -> Result<(), StoreError> {
        self.conn()
            .execute(
                "DELETE FROM draft_file WHERE id = ?1 AND draft IS NOT NULL",
                [file.0],
            )
            .map(|_| ())
            .map_err(storage)
    }

    fn file_data(&self, file: FileId) -> Result<Vec<u8>, StoreError> {
        self.conn()
            .query_row(
                "SELECT data FROM file_data WHERE file = ?1",
                [file.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?
            .ok_or(StoreError::NotFound)
    }

    fn outbox_files(&self, id: OutboxId) -> Result<Vec<OutgoingFile>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT f.name, f.mime, d.data, f.id FROM draft_file f JOIN file_data d ON d.file = f.id
                 WHERE f.outbox = ?1 ORDER BY f.id",
            )
            .and_then(|mut s| s.query_map([id.0], outgoing_file)?.collect());
        rows.map_err(storage)
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
                "SELECT id, recipients, subject, text, message_id, in_reply_to, date, quote, forward,
                        signature, cc
                 FROM outbox
                 WHERE account_id = ?1 AND not_before <= ?2 AND failed IS NULL ORDER BY id",
            )
            .and_then(|mut s| {
                s.query_map(params![account.0, now], |r| {
                    Ok(Outgoing {
                        id: OutboxId(r.get(0)?),
                        to: recipients(&r.get::<_, String>(1)?),
                        cc: recipients(&r.get::<_, String>(10)?),
                        subject: r.get(2)?,
                        text: full_text(
                            &r.get::<_, String>(3)?,
                            &r.get::<_, String>(9)?,
                            &r.get::<_, String>(7)?,
                        ),
                        message_id: r.get(4)?,
                        in_reply_to: r.get(5)?,
                        date: r.get(6)?,
                        forward: r.get::<_, Option<i64>>(8)?.map(MessageId),
                        files: vec![],
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
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let copy: Option<i64> = tx
            .query_row(
                "UPDATE outbox SET failed = ?2 WHERE id = ?1 RETURNING copy",
                params![id.0, reason],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?
            .flatten();
        // It shows among the refused ones, not as sent.
        if let Some(copy) = copy {
            drop_message(&tx, copy)?;
        }
        tx.commit().map_err(storage)
    }

    fn due(&self, account: AccountId, now: i64) -> Result<Vec<Op>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT o.id, o.kind, o.value, m.id, f.remote_id, f.validity, m.remote_id, t.remote_id
                 FROM op o JOIN message m ON m.id = o.message_id JOIN folder f ON f.id = m.folder_id
                 LEFT JOIN folder t ON t.id = o.target
                 WHERE f.account_id = ?1 AND o.not_before <= ?2 AND m.stand_in = 0
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
            for op in ops {
                let (_, value) = stored(op.action);
                let gone: Option<i64> = delete
                    .query_row(params![op.id.0, value], |r| r.get(0))
                    .optional()
                    .map_err(storage)?;
                if let Some(message) = gone.filter(|_| op.action.removes()) {
                    drop_message(&tx, message)?;
                }
            }
        }
        tx.commit().map_err(storage)
    }

    fn land(&self, ops: &[Op], moved: &Moved) -> Result<(), StoreError> {
        let new: HashMap<&str, &str> = moved
            .ids
            .iter()
            .map(|(old, new)| (old.as_str(), new.as_str()))
            .collect();
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        {
            let mut land = tx.prepare_cached(LAND).map_err(storage)?;
            for op in ops {
                if let Some(id) = new.get(op.location.remote_id.as_str()) {
                    land.execute(params![op.id.0, id, moved.validity])
                        .map_err(storage)?;
                }
            }
        }
        tx.commit().map_err(storage)
    }

    fn unclaimed(&self, account: AccountId) -> Result<Vec<Folder>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(concat!(
                "SELECT f.id, f.remote_id, f.name, f.role, f.pinned FROM folder f
                 WHERE f.account_id = ?1 AND EXISTS (
                     SELECT 1 FROM message m WHERE m.folder_id = f.id AND ",
                unclaimed!("m"),
                ") ORDER BY f.id"
            ))
            .and_then(|mut s| s.query_map([account.0], folder_row(account))?.collect());
        rows.map_err(storage)
    }

    fn revert(&self, ops: &[Op]) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let mut reverted = false;
        for op in ops {
            // Gone or changed since it was read: nothing of this op to undo.
            let (_, value) = stored(op.action);
            reverted |= take_back(
                &tx,
                "DELETE FROM op WHERE id = ?1 AND value = ?2 RETURNING message_id, kind, undo, copy",
                op.id,
                value,
            )?;
        }
        if reverted {
            tx.execute("UPDATE notice SET reverted = 1", [])
                .map_err(storage)?;
        }
        tx.commit().map_err(storage)
    }

    fn take_reverted(&self) -> Result<bool, StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let reverted = tx
            .query_row("SELECT reverted FROM notice", [], |r| r.get(0))
            .map_err(storage)?;
        if reverted {
            tx.execute("UPDATE notice SET reverted = 0", [])
                .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(reverted)
    }
}

/// Queues each message's conversation once, however many of its messages are given.
fn queue_threads(
    tx: &Transaction<'_>,
    actions: &[(MessageId, Action)],
    not_before: i64,
) -> Result<Vec<OpId>, StoreError> {
    let mut taken = HashSet::new();
    let mut ops = vec![];
    for &(message, action) in actions {
        ops.extend(queue_thread(tx, message, action, not_before, &mut taken)?);
    }
    Ok(ops)
}

/// `queue` within `tx`, adding the messages it queues to `taken`; nothing for a message
/// already there.
fn queue_thread(
    tx: &Transaction<'_>,
    message: MessageId,
    action: Action,
    not_before: i64,
    taken: &mut HashSet<i64>,
) -> Result<Vec<OpId>, StoreError> {
    if taken.contains(&message.0) {
        return Ok(vec![]);
    }
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
    if action.removes() && !folder_role(tx, message)?.allows(action) {
        return Err(StoreError::NotAllowed);
    }
    let mut ops = vec![];
    for (id, flags) in ids {
        // The rest of the conversation only where it changes something; unread is
        // only the message itself.
        let changes = match action {
            Action::Seen(seen) => seen && flags & SEEN == 0,
            Action::Flagged(_) => false,
            _ => true,
        };
        if id == message.0 || changes {
            ops.push(queue_one(tx, MessageId(id), action, not_before)?);
            taken.insert(id);
        }
    }
    Ok(ops)
}

/// Applies the action to one message now and queues it, replacing a queued one of its kind.
fn queue_one(
    tx: &Transaction<'_>,
    message: MessageId,
    action: Action,
    not_before: i64,
) -> Result<OpId, StoreError> {
    let (kind, value) = stored(action);
    let (flags, queued, old_copy): (u8, Option<i64>, Option<i64>) = tx
        .prepare_cached(
            "SELECT m.flags, o.undo, o.copy FROM message m
             LEFT JOIN op o ON o.message_id = m.id AND o.kind = ?2 WHERE m.id = ?1",
        )
        .and_then(|mut s| {
            s.query_row(params![message.0, kind], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
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
        Action::Flagged(flagged) => {
            set_flagged(tx, message.0, flagged)?;
            (queued.unwrap_or(i64::from(flags & FLAGGED != 0)), None)
        }
        // Undone by dropping the op, which shows the message again.
        Action::Archive => (0, Some(role_folder(tx, message, FolderRole::Archive)?)),
        Action::Trash => (0, Some(role_folder(tx, message, FolderRole::Trash)?)),
        Action::Spam => (0, Some(role_folder(tx, message, FolderRole::Junk)?)),
        Action::Inbox => (0, Some(role_folder(tx, message, FolderRole::Inbox)?)),
        Action::Snooze => (0, Some(role_folder(tx, message, FolderRole::Snoozed)?)),
        Action::Move(folder) => (0, Some(other_folder(tx, message, folder)?)),
        Action::Delete | Action::Empty => (0, None),
    };
    if let Some(old) = old_copy {
        drop_message(tx, old)?;
    }
    let copy = target
        .map(|folder| copy_into(tx, message, folder))
        .transpose()?;
    // Kept even when it undoes itself: a push may be sending the older value right now.
    let id = tx
        .prepare_cached(
            "INSERT INTO op (message_id, kind, value, undo, not_before, target, copy)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (message_id, kind) DO UPDATE SET
                 value = excluded.value, not_before = excluded.not_before,
                 target = excluded.target, copy = excluded.copy
             RETURNING id",
        )
        .and_then(|mut s| {
            s.query_row(
                params![message.0, kind, value, undo, not_before, target, copy],
                |r| r.get(0),
            )
        })
        .map_err(storage)?;
    Ok(OpId(id))
}

/// Shows a message waiting in the outbox in its account's sent folder at once, as copies of
/// moved mail show: the stand-in's id, or None without a sent folder. Its Message-ID, new
/// to each message, keeps its remote id apart from every other stand-in's.
fn copy_into_sent(
    tx: &Transaction<'_>,
    draft: &Draft,
    message_id: &str,
    in_reply_to: Option<&str>,
    date: i64,
) -> Result<Option<i64>, StoreError> {
    let Some(folder) = tx
        .prepare_cached("SELECT id FROM folder WHERE account_id = ?1 AND role = 'sent' LIMIT 1")
        .and_then(|mut s| {
            s.query_row([draft.account_id.0], |r| r.get::<_, i64>(0))
                .optional()
        })
        .map_err(storage)?
    else {
        return Ok(None);
    };
    let body = Body {
        // As the message reads in plain text, until the server's copy comes.
        text: ozero_domain::rich::to_plain(
            &full_text(&draft.text, &draft.signature, &draft.quote),
            &|_| None,
        ),
        ..Body::default()
    };
    let copy: i64 = tx
        .prepare_cached(
            "INSERT INTO message (folder_id, remote_id, message_id, from_name, from_email,
                                  subject, snippet, date, flags, recipients, thread_id, stand_in)
             SELECT ?1, '~' || ?2, ?2, display_name, email, ?3, ?4, ?5, 1, ?6,
                    (SELECT thread_id FROM message WHERE id = ?7), 1
             FROM account WHERE id = ?8
             RETURNING id",
        )
        .and_then(|mut s| {
            s.query_row(
                params![
                    folder,
                    message_id,
                    draft.subject,
                    body.snippet(),
                    date,
                    people_column(&bare(&draft.to), &bare(&draft.cc)),
                    draft.reply_to.map(|m| m.0),
                    draft.account_id.0,
                ],
                |r| r.get(0),
            )
        })
        .map_err(storage)?;
    // Its own conversation unless it answers one.
    tx.execute(
        "UPDATE message SET thread_id = id WHERE id = ?1 AND thread_id IS NULL",
        [copy],
    )
    .map_err(storage)?;
    tx.execute(
        "INSERT INTO message_body (message_id, text) VALUES (?1, ?2)",
        params![copy, body.text],
    )
    .map_err(storage)?;
    // The server's copy links to the same conversation by the answered message's id.
    if let Some(parent) = in_reply_to {
        tx.execute(
            "INSERT INTO message_ref (message_id, ref) VALUES (?1, ?2)",
            params![copy, parent],
        )
        .map_err(storage)?;
    }
    Ok(Some(copy))
}

/// Shows a message being moved in `folder` at once: a copy with its body, files and
/// conversation, marked as standing in for the server's own until a sync finds that.
fn copy_into(tx: &Transaction<'_>, message: MessageId, folder: i64) -> Result<i64, StoreError> {
    let copy: i64 = tx
        .prepare_cached(
            "INSERT INTO message (folder_id, remote_id, message_id, from_name, from_email,
                                  subject, snippet, date, flags, category, answer_to, recipients,
                                  thread_id, stand_in)
             SELECT ?2, '~' || id, message_id, from_name, from_email, subject, snippet, date,
                    flags, category, answer_to, recipients, thread_id, 1
             FROM message WHERE id = ?1
             RETURNING id",
        )
        .and_then(|mut s| s.query_row(params![message.0, folder], |r| r.get(0)))
        .map_err(storage)?;
    for sql in [
        "INSERT INTO message_body (message_id, text, html)
         SELECT ?2, text, html FROM message_body WHERE message_id = ?1",
        "INSERT INTO attachment (message_id, part, name, mime, size)
         SELECT ?2, part, name, mime, size FROM attachment WHERE message_id = ?1",
        "INSERT INTO unsubscribe (message_id, links, one_click, state)
         SELECT ?2, links, one_click, state FROM unsubscribe WHERE message_id = ?1",
        "INSERT INTO message_ref (message_id, ref)
         SELECT ?2, ref FROM message_ref WHERE message_id = ?1",
    ] {
        tx.prepare_cached(sql)
            .and_then(|mut s| s.execute(params![message.0, copy]))
            .map_err(storage)?;
    }
    Ok(copy)
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
        // Copies of moved mail wait for the server's own, which the sync brings.
        tx.execute(
            "DELETE FROM message WHERE folder_id = ?1 AND stand_in = 0",
            [folder.0],
        )
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
    // The server's own copy of moved mail takes the place of ours, which keeps its id.
    let mut claim = tx.prepare_cached(CLAIM).map_err(storage)?;
    let copies: bool = tx
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM message WHERE folder_id = ?1 AND stand_in = 1)",
        )
        .and_then(|mut s| s.query_row([folder.0], |r| r.get(0)))
        .map_err(storage)?;
    for m in messages {
        if let Some(message_id) = m.message_id.as_ref().filter(|_| copies) {
            claim
                .execute(params![folder.0, message_id, m.remote_id])
                .map_err(storage)?;
        }
        write_message(&mut upsert, folder, m)?;
        write_unsubscribe(tx, folder, m)?;
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
            people_column(&m.to, &m.cc),
        ])
        .map_err(storage)
}

/// Keeps how to leave the list `m` came from, if it says; what the person did stays.
fn write_unsubscribe(
    tx: &Transaction<'_>,
    folder: FolderId,
    m: &NewMessage,
) -> Result<(), StoreError> {
    let Some(leave) = &m.unsubscribe else {
        return Ok(());
    };
    tx.prepare_cached(
        "INSERT INTO unsubscribe (message_id, links, one_click)
         SELECT id, ?3, ?4 FROM message WHERE folder_id = ?1 AND remote_id = ?2
         ON CONFLICT (message_id) DO UPDATE SET links = excluded.links, one_click = excluded.one_click",
    )
    .and_then(|mut s| s.execute(params![folder.0, m.remote_id, leave.links, leave.one_click]))
    .map_err(storage)?;
    Ok(())
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

/// The folder of the message's account with `role`, where a move goes. An account without an
/// inbox has not synced yet, so a missing one reads as the message not being there.
fn role_folder(
    tx: &Transaction<'_>,
    message: MessageId,
    role: FolderRole,
) -> Result<i64, StoreError> {
    let missing = match role {
        FolderRole::Trash => StoreError::NoTrash,
        FolderRole::Archive => StoreError::NoArchive,
        FolderRole::Snoozed => StoreError::NoSnoozed,
        FolderRole::Junk => StoreError::NoSpam,
        _ => StoreError::NotFound,
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

/// `folder`, where a move goes, if it is another folder of the message's account that takes
/// moves.
fn other_folder(
    tx: &Transaction<'_>,
    message: MessageId,
    folder: FolderId,
) -> Result<i64, StoreError> {
    tx.prepare_cached(
        "SELECT t.id, t.role FROM message m JOIN folder f ON f.id = m.folder_id
         JOIN folder t ON t.account_id = f.account_id AND t.id != f.id
         WHERE m.id = ?1 AND t.id = ?2",
    )
    .and_then(|mut s| {
        s.query_row(params![message.0, folder.0], |r| {
            Ok((r.get(0)?, r.get::<_, Role>(1)?))
        })
        .optional()
    })
    .map_err(storage)?
    .filter(|(_, role)| role.0.takes_moves())
    .map(|(id, _)| id)
    .ok_or(StoreError::NotAllowed)
}

fn drop_message(tx: &Transaction<'_>, message: i64) -> Result<(), StoreError> {
    tx.prepare_cached("DELETE FROM message WHERE id = ?1")
        .and_then(|mut s| s.execute([message]))
        .map(|_| ())
        .map_err(storage)
}

/// Reads `id, remote_id, name, role, pinned` of one of the account's folders.
fn folder_row(account: AccountId) -> impl Fn(&Row<'_>) -> rusqlite::Result<Folder> {
    move |r| {
        Ok(Folder {
            id: FolderId(r.get(0)?),
            account_id: account,
            remote_id: r.get(1)?,
            name: r.get(2)?,
            role: r.get::<_, Role>(3)?.0,
            pinned: r.get(4)?,
        })
    }
}

fn folder_role(tx: &Transaction<'_>, message: MessageId) -> Result<FolderRole, StoreError> {
    tx.prepare_cached(
        "SELECT f.role FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1",
    )
    .and_then(|mut s| s.query_row([message.0], |r| r.get::<_, Role>(0)))
    .map(|role| role.0)
    .map_err(storage)
}

/// Drops the op if `sql`, a `DELETE ... RETURNING message_id, kind, undo, copy` with the op
/// id as `?1` and `arg` as `?2`, finds it, and restores what it changed on the device.
fn take_back(tx: &Transaction<'_>, sql: &str, op: OpId, arg: i64) -> Result<bool, StoreError> {
    let found: Option<(i64, StoredAction, i64, Option<i64>)> = tx
        .prepare_cached(sql)
        .and_then(|mut s| {
            s.query_row(params![op.0, arg], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()
        })
        .map_err(storage)?;
    let Some((message, kind, undo, copy)) = found else {
        return Ok(false);
    };
    match kind.with(undo) {
        Action::Seen(seen) => set_seen(tx, message, seen)?,
        Action::Flagged(flagged) => set_flagged(tx, message, flagged)?,
        // As the other moves, and nothing waits to come back.
        Action::Snooze => {
            tx.execute(UNSNOOZE, [message]).map_err(storage)?;
            if let Some(copy) = copy {
                drop_message(tx, copy)?;
            }
        }
        // Without its op the message shows in its folder again, and its copy goes.
        _ => {
            if let Some(copy) = copy {
                drop_message(tx, copy)?;
            }
        }
    }
    Ok(true)
}

fn set_seen(tx: &Transaction<'_>, message: i64, seen: bool) -> Result<(), StoreError> {
    set_flag(tx, message, SEEN, seen)
}

fn set_flagged(tx: &Transaction<'_>, message: i64, flagged: bool) -> Result<(), StoreError> {
    set_flag(tx, message, FLAGGED, flagged)
}

fn set_flag(tx: &Transaction<'_>, message: i64, flag: u8, on: bool) -> Result<(), StoreError> {
    tx.prepare_cached("UPDATE message SET flags = (flags & ~?2) | ?3 WHERE id = ?1")
        .and_then(|mut s| s.execute(params![message, flag, if on { flag } else { 0 }]))
        .map_err(storage)
        .map(|_| ())
}

/// The flags the device holds for a message while a queued action has not reached the server:
/// `1` (`SEEN`) for "seen", `2` (`FLAGGED`) for "flagged".
macro_rules! held_flags {
    () => {
        "(SELECT coalesce(max(CASE op.kind WHEN 'seen' THEN 1 ELSE 0 END), 0)
                 | coalesce(max(CASE op.kind WHEN 'flagged' THEN 2 ELSE 0 END), 0)
          FROM op WHERE op.message_id = message.id)"
    };
}

/// The flags to keep for a known message: the server's, except those `held_flags` holds.
macro_rules! merged_flags {
    ($server:literal) => {
        concat!(
            "((",
            $server,
            ") & ~",
            held_flags!(),
            ") | (flags & ",
            held_flags!(),
            ")"
        )
    };
}

/// Gives a copy of moved mail in folder `?1` with Message-ID `?2`, whose move the server has,
/// the server's id `?3`: it is no longer a stand-in.
const CLAIM: &str = concat!(
    "UPDATE message SET remote_id = ?3, stand_in = 0
     WHERE id = (SELECT id FROM message m WHERE m.folder_id = ?1 AND m.message_id = ?2 AND ",
    unclaimed!("m"),
    " ORDER BY m.id LIMIT 1)
       AND NOT EXISTS (SELECT 1 FROM message WHERE folder_id = ?1 AND remote_id = ?3)"
);

/// Gives the copy op `?1` made the server's id `?2`, if its folder has validity `?3` or that
/// is NULL.
const LAND: &str = "
    UPDATE message SET remote_id = ?2, stand_in = 0
    WHERE id = (SELECT copy FROM op WHERE id = ?1) AND stand_in = 1
      AND (?3 IS NULL OR ?3 = (SELECT validity FROM folder WHERE id = message.folder_id))
      AND NOT EXISTS (SELECT 1 FROM message x
                      WHERE x.folder_id = message.folder_id AND x.remote_id = ?2)";

/// Sets the flags of a saved message from the server's, under a queued action.
const SAVE_FLAGS: &str = concat!(
    "UPDATE message SET flags = ",
    merged_flags!("?3"),
    " WHERE folder_id = ?1 AND remote_id = ?2 AND flags IS NOT ",
    merged_flags!("?3")
);

/// The key of message `$m` that its snooze is kept under: its Message-ID, else what moving
/// it leaves alone.
macro_rules! snooze_key {
    ($m:literal) => {
        concat!(
            "coalesce(",
            $m,
            ".message_id, ",
            $m,
            ".from_email || char(31) || ",
            $m,
            ".date || char(31) || ",
            $m,
            ".subject)"
        )
    };
}

/// Mail that came back from a snooze is dated when it came back, in the inbox.
const SAVE_MESSAGE: &str = concat!(
    "INSERT INTO message (folder_id, remote_id, message_id, from_name, from_email,
                          subject, snippet, date, flags, category, answer_to, recipients, fresh)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
             coalesce((SELECT s.wake_at FROM snooze s JOIN folder f ON f.account_id = s.account_id
                       WHERE f.id = ?1 AND f.role = 'inbox' AND s.woke = 1
                         AND s.message_id = coalesce(?3, ?5 || char(31) || ?8 || char(31) || ?6)),
                      ?8),
             ?9, ?10, ?11, ?12, 1)
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

/// Lists leave out a message while its move to another folder is held.
macro_rules! not_moving {
    () => {
        not_moving!("m")
    };
    ($m:literal) => {
        concat!(
            "NOT EXISTS (SELECT 1 FROM op WHERE op.message_id = ",
            $m,
            ".id AND op.kind IN ('archive', 'trash', 'spam', 'inbox', 'snooze', 'move', 'delete', 'empty'))"
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

/// Folder `$f` is one whose mail shows beyond its own list: not the trash or junk, nor the
/// drafts folder, whose copies show as drafts.
macro_rules! shown_role {
    ($f:literal) => {
        concat!($f, ".role NOT IN ('trash', 'junk', 'drafts')")
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
                         WHERE message_id = m.id),
                    m.stand_in = 1 AND EXISTS (SELECT 1 FROM outbox WHERE copy = m.id),
                    EXISTS (SELECT 1 FROM unsubscribe WHERE message_id = m.id AND state = 0)",
            " FROM message m JOIN folder f ON f.id = m.folder_id"
        )
    };
}

macro_rules! message_select {
    () => {
        concat!(message_from!(), " WHERE ")
    };
}

/// The inbox of account `?1`, or with `?1` NULL the inboxes in the unified one.
macro_rules! inboxes_of {
    () => {
        "(SELECT f.id FROM folder f JOIN account a ON a.id = f.account_id
          WHERE f.role = 'inbox' AND (f.account_id = ?1 OR ?1 IS NULL AND a.in_all = 1))"
    };
}

/// Newest first, keyset-paged by (date, id) so deep pages cost the same as the first.
macro_rules! list_sql {
    ($($scope:tt)+) => {
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
            $($scope)+,
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
const LIST_INBOX_TAB: &str = list_sql!("m.category = ?5 AND +m.folder_id IN ", inboxes_of!());
// A conversation with unread mail, or with files, shows as its newest message. The few
// conversations are found first, from the unread index or the files, then sorted: the `+`s keep
// SQLite from walking all mail by date instead.
const LIST_UNREAD: &str = list_sql!(
    "(m.thread_id, +m.folder_id, +m.category) IN (SELECT n.thread_id, n.folder_id, n.category
         FROM message n JOIN folder nf ON nf.id = n.folder_id
         WHERE n.flags & 1 = 0 AND n.found = 0 AND nf.role NOT IN ('trash', 'junk', 'drafts') AND ",
    not_moving!("n"),
    ")"
);
const LIST_FILES: &str = list_sql!(
    "(m.thread_id, +m.folder_id, +m.category) IN (SELECT n.thread_id, n.folder_id, n.category
         FROM attachment a JOIN message n ON n.id = a.message_id JOIN folder nf ON nf.id = n.folder_id
         WHERE a.name != '' AND n.found = 0 AND nf.role NOT IN ('trash', 'junk', 'drafts') AND ",
    not_moving!("n"),
    ")"
);

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

/// The newest `?1` files of saved mail. A file sent on in a conversation comes again with
/// each reply: one row per name and size, from its newest message (SQLite takes the bare
/// columns from the MAX row). CROSS JOIN reads the few attachments first, not every message.
const RECENT_FILES: &str = concat!(
    "SELECT a.message_id, f.account_id, m.from_name, m.from_email,
            a.part, a.name, a.mime, a.size, MAX(m.date) AS newest
     FROM attachment a CROSS JOIN message m ON m.id = a.message_id
     JOIN folder f ON f.id = m.folder_id
     WHERE ",
    shown_role!("f"),
    " AND m.stand_in = 0 AND a.name != '' AND ",
    not_moving!(),
    " GROUP BY a.name, a.size ORDER BY newest DESC, a.part LIMIT ?1"
);

/// What an action on message `?1` covers: its conversation in its folder and inbox tab.
const THREAD_IDS: &str = concat!(
    "SELECT m.id, m.flags FROM message m
     JOIN message s ON s.thread_id = m.thread_id AND s.folder_id = m.folder_id
         AND s.category = m.category
     WHERE s.id = ?1
       AND NOT EXISTS (SELECT 1 FROM outbox WHERE outbox.copy = m.id) AND ",
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
       AND ",
    shown_role!("f"),
    " AND ",
    not_moving!(),
    " AND NOT EXISTS (
         SELECT 1 FROM message d JOIN folder g ON g.id = d.folder_id
         WHERE d.message_id = m.message_id AND d.id > m.id AND g.account_id = f.account_id
           AND ",
    shown_role!("g"),
    " AND ",
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

/// How long mail that came back from a snooze stays dated when it came back, in seconds.
const WOKE_KEPT: i64 = 30 * 24 * 60 * 60;

/// Snoozes the message of op `?1` until `?2`.
const SNOOZE_UNTIL: &str = concat!(
    "INSERT INTO snooze (account_id, message_id, wake_at)
     SELECT f.account_id, ",
    snooze_key!("m"),
    ", ?2 FROM op o JOIN message m ON m.id = o.message_id JOIN folder f ON f.id = m.folder_id
     WHERE o.id = ?1
     ON CONFLICT (account_id, message_id) DO UPDATE SET wake_at = excluded.wake_at, woke = 0"
);

/// Forgets the snooze of message `?1`.
const UNSNOOZE: &str = concat!(
    "DELETE FROM snooze WHERE (account_id, message_id) IN (
         SELECT f.account_id, ",
    snooze_key!("m"),
    " FROM message m JOIN folder f ON f.id = m.folder_id WHERE m.id = ?1)"
);

const MESSAGE_BY_ID: &str = concat!(message_select!(), "m.id = ?1");

/// The messages in a snoozed folder whose time came by `?1`.
const SNOOZED_DUE: &str = concat!(
    "SELECT m.id, s.account_id, s.message_id FROM snooze s
     JOIN folder f ON f.account_id = s.account_id AND f.role = 'snoozed'
     JOIN message m ON m.folder_id = f.id AND ",
    snooze_key!("m"),
    " = s.message_id
     WHERE s.woke = 0 AND s.wake_at <= ?1 AND m.found = 0 AND ",
    not_moving!(),
    " ORDER BY m.id"
);

const CLEAR_FRESH: &str = "UPDATE message SET fresh = 0 WHERE fresh = 1";

// `1` is `SEEN`.
const FRESH_PEOPLE: &str = concat!(
    message_select!(),
    "m.fresh = 1 AND m.date > ?1 AND f.role = 'inbox' AND m.category = 'people' AND m.flags & 1 = 0
       AND EXISTS (SELECT 1 FROM account WHERE id = f.account_id AND notify = 1) AND ",
    not_moving!(),
    " ORDER BY m.date DESC, m.id DESC"
);

// `1` is `SEEN`.
const INBOX_UNREAD: &str = concat!(
    "SELECT m.category, count(*) FROM message m
     WHERE m.folder_id IN ",
    inboxes_of!(),
    " AND m.flags & 1 = 0
       AND m.found = 0 AND ",
    not_moving!(),
    " GROUP BY m.category"
);
/// The account's newest `?2` inbox messages and the rest of their conversations, the user's
/// replies included, so a conversation opens whole: those still without a body and staying.
/// A long conversation brings at most a few times `?2` more.
const WITHOUT_BODY: &str = concat!(
    "WITH s AS (SELECT id, folder_id, thread_id, category FROM message
                WHERE folder_id IN (SELECT id FROM folder WHERE account_id = ?1 AND role = 'inbox')
                  AND stand_in = 0
                ORDER BY date DESC, id DESC LIMIT ?2),
          c AS (SELECT id FROM s UNION
                SELECT m.id FROM s JOIN message m ON m.thread_id = s.thread_id
                JOIN folder f ON f.id = m.folder_id WHERE ",
    in_conversation!("m", "f", "s"),
    ")
     SELECT m.id, f.remote_id, f.validity, m.remote_id
     FROM c JOIN message m ON m.id = c.id JOIN folder f ON f.id = m.folder_id
     WHERE m.stand_in = 0
       AND NOT EXISTS (SELECT 1 FROM message_body b WHERE b.message_id = m.id) AND ",
    not_moving!(),
    " ORDER BY m.date DESC, m.id DESC LIMIT ?2 * 4"
);

const FOLDER_MESSAGES: &str = concat!(
    "SELECT m.id, m.stand_in FROM message m WHERE m.folder_id = ?1 AND ",
    not_moving!()
);

// `1` is `SEEN`.
const UNREAD_BY_ACCOUNT: &str = concat!(
    "SELECT f.account_id, count(*) FROM message m JOIN folder f ON f.id = m.folder_id
     WHERE f.role = 'inbox' AND m.category = ?1 AND m.flags & 1 = 0 AND m.found = 0 AND ",
    not_moving!(),
    " GROUP BY f.account_id ORDER BY f.account_id"
);

// Stops at the first message found, so a full inbox costs no more than an empty one.
const INBOX_EMPTY: &str = concat!(
    "SELECT NOT EXISTS (SELECT 1 FROM message m
     WHERE m.folder_id IN ",
    inboxes_of!(),
    " AND m.found = 0 AND ",
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

/// A message's `To` and `Cc` for its `recipients` column, one a line: the address, `\x1f`,
/// `to` or `cc`, `\x1f` and the name, empty for none.
fn people_column(to: &[Address], cc: &[Address]) -> String {
    let lines: Vec<String> = (to.iter().map(|a| (a, "to")))
        .chain(cc.iter().map(|a| (a, "cc")))
        .map(|(a, kind)| {
            let name = a.name.as_deref().unwrap_or_default();
            format!(
                "{}\x1f{kind}\x1f{}",
                a.email,
                name.replace(['\n', '\r'], " ")
            )
        })
        .collect();
    lines.join("\n")
}

/// The `To` and `Cc` that `people_column` wrote. A line saved before it, a bare address,
/// counts as `To`.
fn people(stored: &str) -> (Vec<Address>, Vec<Address>) {
    let (mut to, mut cc) = (Vec::new(), Vec::new());
    for line in stored.lines() {
        let mut parts = line.splitn(3, '\x1f');
        let email = parts.next().unwrap_or_default().to_owned();
        let copy = parts.next() == Some("cc");
        let name = parts.next().filter(|n| !n.is_empty()).map(str::to_owned);
        let address = Address { name, email };
        if copy {
            cc.push(address)
        } else {
            to.push(address)
        }
    }
    (to, cc)
}

/// Addresses without names.
fn bare(emails: &[String]) -> Vec<Address> {
    emails
        .iter()
        .map(|email| Address {
            name: None,
            email: email.clone(),
        })
        .collect()
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
        FolderRole::Snoozed => &"snoozed",
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

/// What `draft_file` reads into a `DraftFile`.
const FILE_FIELDS: &str = "id, name, mime, size";

/// A `DraftFile` from `FILE_FIELDS`, read from column `at` on.
fn draft_file(r: &Row<'_>, at: usize) -> rusqlite::Result<DraftFile> {
    Ok(DraftFile {
        id: FileId(r.get(at)?),
        name: r.get(at + 1)?,
        mime: r.get(at + 2)?,
        size: u64::try_from(r.get::<_, i64>(at + 3)?).unwrap_or(0),
    })
}

/// The files of a draft, in the order they were added, without their data.
fn files_of(conn: &Connection, draft: DraftId) -> Result<Vec<DraftFile>, StoreError> {
    let rows = conn
        .prepare_cached(&format!(
            "SELECT {FILE_FIELDS} FROM draft_file WHERE draft = ?1 ORDER BY id"
        ))
        .and_then(|mut s| s.query_map([draft.0], |r| draft_file(r, 0))?.collect());
    rows.map_err(storage)
}

/// Drafts whose copies in `folder` are gone from the server, sent or thrown away elsewhere: one
/// not written since goes too, any other makes a new copy. One thrown away here is left be.
fn copies_gone(tx: &Transaction<'_>, folder: FolderId, gone: &[String]) -> Result<(), StoreError> {
    // Most folders hold no draft's copy: no lookup for each message gone from them.
    let holds_copies: bool = tx
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM draft d JOIN folder f
             ON f.account_id = d.remote_account AND f.remote_id = d.remote_folder
                AND f.validity = d.remote_validity WHERE f.id = ?1)",
            [folder.0],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if !holds_copies {
        return Ok(());
    }
    let mut drafts = tx
        .prepare_cached(
            "SELECT d.id, d.pushed IS d.saved AND d.gone IS NULL FROM draft d JOIN folder f
             ON f.account_id = d.remote_account AND f.remote_id = d.remote_folder
                AND f.validity = d.remote_validity
             WHERE f.id = ?1 AND d.remote_id = ?2",
        )
        .map_err(storage)?;
    let mut found: Vec<(i64, bool)> = vec![];
    for remote_id in gone {
        let rows = drafts
            .query_map(params![folder.0, remote_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
            .map_err(storage)?;
        found.extend(rows);
    }
    for (id, unchanged) in found {
        // Without a copy first, so no deletion of it is queued.
        tx.execute(
            "UPDATE draft SET remote_account = NULL, remote_folder = NULL, remote_validity = NULL,
                              remote_id = NULL, pushed = NULL
             WHERE id = ?1",
            [id],
        )
        .map_err(storage)?;
        if unchanged {
            tx.execute("DELETE FROM draft WHERE id = ?1", [id])
                .map_err(storage)?;
        }
    }
    Ok(())
}

/// A file to send, from its name, type, contents and id.
fn outgoing_file(r: &Row<'_>) -> rusqlite::Result<OutgoingFile> {
    Ok(OutgoingFile {
        id: Some(FileId(r.get(3)?)),
        name: r.get(0)?,
        mime: r.get(1)?,
        data: r.get(2)?,
    })
}

/// Adds `file` to a draft; returns its id.
fn insert_file(conn: &Connection, draft: DraftId, file: &OutgoingFile) -> Result<i64, StoreError> {
    let size = i64::try_from(file.data.len()).unwrap_or(i64::MAX);
    let id = conn
        .query_row(
            "INSERT INTO draft_file (draft, name, mime, size) VALUES (?1, ?2, ?3, ?4)
             RETURNING id",
            params![draft.0, file.name, file.mime, size],
            |r| r.get(0),
        )
        .map_err(storage)?;
    conn.execute(
        "INSERT INTO file_data (file, data) VALUES (?1, ?2)",
        params![id, file.data],
    )
    .map_err(storage)?;
    Ok(id)
}

/// Saves a new draft; an account or answered message that is gone is left out.
fn insert_draft(conn: &Connection, draft: &SavedDraft) -> Result<DraftId, StoreError> {
    conn.query_row(
        "INSERT INTO draft (account_id, recipients, subject, text, reply_to, quote, saved, forward,
                            signature, cc)
         VALUES ((SELECT id FROM account WHERE id = ?1), ?2, ?3, ?4,
                 (SELECT id FROM message WHERE id = ?5), ?6, ?7,
                 (SELECT id FROM message WHERE id = ?8), ?9, ?10)
         RETURNING id",
        params![
            draft.account_id.map(|a| a.0),
            draft.to,
            draft.subject,
            draft.text,
            draft.reply_to.map(|m| m.0),
            draft.quote,
            draft.saved,
            draft.forward.map(|m| m.0),
            draft.signature,
            draft.cc,
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
        sending: r.get(12)?,
        can_unsubscribe: r.get(13)?,
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
        AccountKind::Exchange => "exchange",
    }
}

/// The new_mail column of `account`: -1 only with the app open, else every so many minutes.
fn new_mail_value(when: NewMail) -> i64 {
    match when {
        NewMail::Every { minutes } => i64::from(minutes.max(1)),
        NewMail::Manually => -1,
    }
}

/// Stored strings are read strictly: an unknown value is an error, never a silent default.
struct Kind(AccountKind);
struct StoredNewMail(NewMail);
struct Role(FolderRole);

/// The kind column of `op`; the value column completes it.
#[derive(Clone, Copy)]
enum StoredAction {
    Seen,
    Flagged,
    Spam,
    Archive,
    Trash,
    Inbox,
    Snooze,
    Move,
    Delete,
    Empty,
}

/// The kind and value columns of `op` for an action.
fn stored(action: Action) -> (&'static str, i64) {
    match action {
        Action::Seen(seen) => ("seen", i64::from(seen)),
        Action::Flagged(flagged) => ("flagged", i64::from(flagged)),
        Action::Spam => ("spam", 1),
        Action::Archive => ("archive", 1),
        Action::Trash => ("trash", 1),
        Action::Inbox => ("inbox", 1),
        Action::Snooze => ("snooze", 1),
        Action::Move(folder) => ("move", folder.0),
        Action::Delete => ("delete", 1),
        Action::Empty => ("empty", 1),
    }
}

impl StoredAction {
    fn with(self, value: i64) -> Action {
        match self {
            Self::Seen => Action::Seen(value != 0),
            Self::Flagged => Action::Flagged(value != 0),
            Self::Spam => Action::Spam,
            Self::Archive => Action::Archive,
            Self::Trash => Action::Trash,
            Self::Inbox => Action::Inbox,
            Self::Snooze => Action::Snooze,
            Self::Move => Action::Move(FolderId(value)),
            Self::Delete => Action::Delete,
            Self::Empty => Action::Empty,
        }
    }
}

impl FromSql for StoredAction {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "seen" => Ok(Self::Seen),
            "flagged" => Ok(Self::Flagged),
            "spam" => Ok(Self::Spam),
            "archive" => Ok(Self::Archive),
            "trash" => Ok(Self::Trash),
            "inbox" => Ok(Self::Inbox),
            "snooze" => Ok(Self::Snooze),
            "move" => Ok(Self::Move),
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
            "exchange" => Ok(Self(AccountKind::Exchange)),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl FromSql for StoredNewMail {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_i64()? {
            -1 => Ok(Self(NewMail::Manually)),
            m => u32::try_from(m)
                .ok()
                .filter(|&minutes| minutes > 0)
                .ok_or(FromSqlError::OutOfRange(m))
                .map(|minutes| Self(NewMail::Every { minutes })),
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
            "snoozed" => FolderRole::Snoozed,
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
