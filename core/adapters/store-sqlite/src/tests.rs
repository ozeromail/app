use std::path::PathBuf;

use super::*;
use ozero_domain::Cursor;

const KEY: [u8; 32] = [7; 32];

/// A database file removed when the test ends.
struct TempDb(PathBuf);

impl TempDb {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("ozero-{}-{name}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

fn store() -> SqliteStore {
    SqliteStore::open_in_memory(&KEY).unwrap()
}

fn account(store: &SqliteStore, email: &str) -> AccountId {
    store
        .add_account(&NewAccount {
            kind: AccountKind::Imap,
            email: email.into(),
            display_name: email.into(),
        })
        .unwrap()
}

fn inbox(store: &SqliteStore, account: AccountId) -> FolderId {
    let folders = store
        .save_folders(
            account,
            &[NewFolder {
                remote_id: "INBOX".into(),
                name: "Inbox".into(),
                role: FolderRole::Inbox,
            }],
        )
        .unwrap();
    folders[0].id
}

fn message(uid: u32, date: i64) -> NewMessage {
    NewMessage {
        remote_id: uid.to_string(),
        message_id: Some(format!("<{uid}@example.com>")),
        from: Address {
            name: Some("Anna".into()),
            email: "anna@example.com".into(),
        },
        subject: format!("Subject {uid}"),
        snippet: "Hello".into(),
        date,
        flags: Flags::default(),
    }
}

fn list(
    store: &SqliteStore,
    scope: ListScope,
    limit: u32,
    after: Option<Cursor>,
) -> Vec<MessageRow> {
    store
        .list(&ListQuery {
            scope,
            limit,
            after,
        })
        .unwrap()
}

#[test]
fn file_is_encrypted_and_needs_the_key() {
    let db = TempDb::new("encrypted");
    {
        let store = SqliteStore::open(&db.0, &KEY).unwrap();
        account(&store, "a@example.com");
    }
    let bytes = std::fs::read(&db.0).unwrap();
    assert!(!bytes.starts_with(b"SQLite format 3"));
    assert!(!bytes.windows(13).any(|w| w == b"a@example.com"));

    assert_eq!(
        SqliteStore::open(&db.0, &[8; 32]).unwrap_err(),
        StoreError::WrongKey
    );

    let reopened = SqliteStore::open(&db.0, &KEY).unwrap();
    assert_eq!(reopened.accounts().unwrap()[0].email, "a@example.com");
}

#[test]
fn refuses_a_newer_schema() {
    let db = TempDb::new("too-new");
    {
        let store = SqliteStore::open(&db.0, &KEY).unwrap();
        store
            .conn()
            .execute_batch("PRAGMA user_version = 999")
            .unwrap();
    }
    assert_eq!(
        SqliteStore::open(&db.0, &KEY).unwrap_err(),
        StoreError::TooNew { version: 999 }
    );
}

#[test]
fn save_folders_updates_by_remote_id() {
    let store = store();
    let acc = account(&store, "a@example.com");
    let first = inbox(&store, acc);
    let again = store
        .save_folders(
            acc,
            &[NewFolder {
                remote_id: "INBOX".into(),
                name: "Входящие".into(),
                role: FolderRole::Inbox,
            }],
        )
        .unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, first);
    assert_eq!(again[0].name, "Входящие");
}

#[test]
fn save_messages_updates_a_known_message() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    store.save_messages(folder, &[message(1, 100)]).unwrap();

    let mut changed = message(1, 100);
    changed.subject = "Other".into();
    changed.flags.seen = true;
    store.save_messages(folder, &[changed]).unwrap();

    let rows = list(&store, ListScope::Folder(folder), 10, None);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].subject, "Other");
    assert!(rows[0].flags.seen);
}

#[test]
fn save_messages_to_unknown_folder_fails() {
    let store = store();
    assert_eq!(
        store
            .save_messages(FolderId(42), &[message(1, 100)])
            .unwrap_err(),
        StoreError::NotFound
    );
}

#[test]
fn save_folders_for_unknown_account_fails() {
    let store = store();
    assert_eq!(
        store.save_folders(AccountId(42), &[]).unwrap_err(),
        StoreError::NotFound
    );
}

#[test]
fn save_folders_returns_input_order() {
    let store = store();
    let acc = account(&store, "a@example.com");
    let new = |id: &str| NewFolder {
        remote_id: id.into(),
        name: id.into(),
        role: FolderRole::Custom,
    };
    store.save_folders(acc, &[new("B")]).unwrap();
    let saved = store.save_folders(acc, &[new("A"), new("B")]).unwrap();
    let names: Vec<_> = saved.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["A", "B"]);
}

#[test]
fn unknown_stored_role_is_an_error() {
    let store = store();
    let acc = account(&store, "a@example.com");
    inbox(&store, acc);
    store
        .conn()
        .execute_batch("UPDATE folder SET role = 'spam'")
        .unwrap();
    assert!(matches!(store.folders(acc), Err(StoreError::Storage(_))));
}

#[test]
fn list_pages_newest_first() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    // Two messages share a date to check the id tie-break.
    let messages: Vec<_> = [(1, 100), (2, 300), (3, 200), (4, 200)]
        .into_iter()
        .map(|(uid, date)| message(uid, date))
        .collect();
    store.save_messages(folder, &messages).unwrap();

    let first = list(&store, ListScope::Folder(folder), 2, None);
    let second = list(
        &store,
        ListScope::Folder(folder),
        2,
        Some(first[1].cursor()),
    );
    let subjects: Vec<_> = first
        .iter()
        .chain(&second)
        .map(|r| r.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        ["Subject 2", "Subject 4", "Subject 3", "Subject 1"]
    );
    assert!(
        list(
            &store,
            ListScope::Folder(folder),
            2,
            Some(second[1].cursor())
        )
        .is_empty()
    );
}

#[test]
fn unified_inbox_merges_accounts() {
    let store = store();
    let a = account(&store, "a@example.com");
    let b = account(&store, "b@example.com");
    store
        .save_messages(inbox(&store, a), &[message(1, 100)])
        .unwrap();
    store
        .save_messages(inbox(&store, b), &[message(1, 200)])
        .unwrap();

    let rows = list(&store, ListScope::Unified(FolderRole::Inbox), 10, None);
    let accounts: Vec<_> = rows.iter().map(|r| r.account_id).collect();
    assert_eq!(accounts, [b, a]);
}

#[test]
fn large_lists_read_an_index_in_order() {
    let store = store();
    let conn = store.conn();
    for sql in [LIST_FOLDER, LIST_UNIFIED_BY_DATE] {
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let plan: Vec<String> = stmt
            .query_map(params!["1", 0, 0, 30], |r| r.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let plan = plan.join("\n");
        assert!(!plan.contains("TEMP B-TREE"), "sorts in memory:\n{plan}");
    }
}
