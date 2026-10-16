//! `Store` on SQLite, encrypted with SQLCipher.

mod schema;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use ozero_domain::{
    Account, AccountId, AccountKind, Action, Address, Body, Flags, Folder, FolderId, FolderRole,
    ListQuery, ListScope, MessageId, MessageLocation, MessageRow, NewAccount, NewFolder,
    NewMessage, Op, OpId, Server, Store, StoreError,
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
        let server = account.server.as_ref();
        conn.execute(
            "INSERT INTO account (kind, email, display_name, server_host, server_port, server_user)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                kind_str(account.kind),
                account.email,
                account.display_name,
                server.map(|s| &s.host),
                server.map(|s| s.port),
                server.map(|s| &s.user),
            ],
        )
        .map_err(storage)?;
        Ok(AccountId(conn.last_insert_rowid()))
    }

    fn accounts(&self) -> Result<Vec<Account>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, kind, email, display_name, server_host, server_port, server_user
                 FROM account ORDER BY id",
            )
            .map_err(storage)?;
        let rows = stmt.query_map([], |r| {
            let server = match (r.get(4)?, r.get(5)?, r.get(6)?) {
                (Some(host), Some(port), Some(user)) => Some(Server { host, port, user }),
                _ => None,
            };
            Ok(Account {
                id: AccountId(r.get(0)?),
                kind: r.get::<_, Kind>(1)?.0,
                email: r.get(2)?,
                display_name: r.get(3)?,
                server,
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
        };
        let conn = self.conn();
        let rows = conn.prepare_cached(sql).and_then(|mut s| {
            s.query_map(params![scope, date, id, query.limit], message_row)?
                .collect()
        });
        rows.map_err(storage)
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

    fn queue(&self, message: MessageId, action: Action, not_before: i64) -> Result<(), StoreError> {
        let Action::Seen(seen) = action;
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        let (flags, queued): (u8, Option<bool>) = tx
            .prepare_cached(
                "SELECT m.flags, o.undo FROM message m
                 LEFT JOIN op o ON o.message_id = m.id AND o.kind = 'seen' WHERE m.id = ?1",
            )
            .and_then(|mut s| {
                s.query_row([message.0], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()
            })
            .map_err(storage)?
            .ok_or(StoreError::NotFound)?;
        let undo = queued.unwrap_or(flags & SEEN != 0);
        set_seen(&tx, message.0, seen)?;
        // Kept even when it undoes itself: a push may be sending the older value right now.
        tx.prepare_cached(
            "INSERT INTO op (message_id, kind, value, undo, not_before)
             VALUES (?1, 'seen', ?2, ?3, ?4)
             ON CONFLICT (message_id, kind) DO UPDATE SET
                 value = excluded.value, not_before = excluded.not_before",
        )
        .and_then(|mut s| s.execute(params![message.0, seen, undo, not_before]))
        .map_err(storage)?;
        tx.commit().map_err(storage)
    }

    fn due(&self, account: AccountId, now: i64) -> Result<Vec<Op>, StoreError> {
        let conn = self.conn();
        let rows = conn
            .prepare_cached(
                "SELECT o.id, o.kind, o.value, m.id, f.remote_id, f.validity, m.remote_id
                 FROM op o JOIN message m ON m.id = o.message_id JOIN folder f ON f.id = m.folder_id
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
                .prepare_cached("DELETE FROM op WHERE id = ?1 AND value = ?2")
                .map_err(storage)?;
            for op in ops {
                delete
                    .execute(params![op.id.0, stored_value(op.action)])
                    .map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)
    }

    fn revert(&self, ops: &[Op]) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(storage)?;
        {
            let mut undo = tx
                .prepare_cached(
                    "SELECT message_id, kind, undo FROM op WHERE id = ?1 AND value = ?2",
                )
                .map_err(storage)?;
            let mut delete = tx
                .prepare_cached("DELETE FROM op WHERE id = ?1")
                .map_err(storage)?;
            for op in ops {
                // Gone or changed since it was read: nothing of this op to undo.
                let found: Option<(i64, StoredAction, i64)> = undo
                    .query_row(params![op.id.0, stored_value(op.action)], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })
                    .optional()
                    .map_err(storage)?;
                let Some((message, kind, value)) = found else {
                    continue;
                };
                let Action::Seen(seen) = kind.with(value);
                set_seen(&tx, message, seen)?;
                delete.execute([op.id.0]).map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)
    }
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
                          subject, snippet, date, flags)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
     ON CONFLICT (folder_id, remote_id) DO UPDATE SET
         message_id = excluded.message_id, from_name = excluded.from_name,
         from_email = excluded.from_email, subject = excluded.subject,
         snippet = coalesce(nullif(excluded.snippet, ''), snippet),
         date = excluded.date, flags = ",
    merged_flags!(),
    " WHERE (message_id, from_name, from_email, subject, snippet, date, flags)
         IS NOT (excluded.message_id, excluded.from_name, excluded.from_email,
                 excluded.subject, coalesce(nullif(excluded.snippet, ''), snippet),
                 excluded.date, ",
    merged_flags!(),
    ")"
);

/// Newest first, keyset-paged by (date, id) so deep pages cost the same as the first.
macro_rules! list_sql {
    ($scope:literal) => {
        concat!(
            "SELECT m.id, f.account_id, m.from_name, m.from_email, m.subject, m.snippet, m.date, m.flags
             FROM message m JOIN folder f ON f.id = m.folder_id
             WHERE ",
            $scope,
            " AND (m.date, m.id) < (?2, ?3)
             ORDER BY m.date DESC, m.id DESC
             LIMIT ?4"
        )
    };
}

const LIST_FOLDER: &str = list_sql!("m.folder_id = ?1");
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
}

/// The value column of `op` for an action.
fn stored_value(action: Action) -> i64 {
    match action {
        Action::Seen(seen) => i64::from(seen),
    }
}

impl StoredAction {
    fn with(self, value: i64) -> Action {
        match self {
            Self::Seen => Action::Seen(value != 0),
        }
    }
}

impl FromSql for StoredAction {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "seen" => Ok(Self::Seen),
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
