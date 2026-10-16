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
            server: Some(Server {
                host: "imap.example.com".into(),
                port: 993,
                user: email.into(),
            }),
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
    let accounts = reopened.accounts().unwrap();
    assert_eq!(accounts[0].email, "a@example.com");
    assert_eq!(
        accounts[0]
            .server
            .as_ref()
            .map(|s| (s.host.as_str(), s.port)),
        Some(("imap.example.com", 993))
    );
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

#[test]
fn new_validity_drops_the_folder_messages() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    store.set_validity(folder, 5).unwrap();
    store.save_messages(folder, &[message(1, 100)]).unwrap();

    store.set_validity(folder, 5).unwrap();
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 1);

    store.set_validity(folder, 6).unwrap();
    assert!(list(&store, ListScope::Folder(folder), 10, None).is_empty());
    assert_eq!(
        store.set_validity(FolderId(42), 1).unwrap_err(),
        StoreError::NotFound
    );
}

#[test]
fn body_is_saved_with_its_snippet() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    store.set_validity(folder, 5).unwrap();
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(folder), 10, None)[0].id;

    assert_eq!(store.body(id).unwrap(), None);
    assert_eq!(
        store.location(id).unwrap(),
        MessageLocation {
            id,
            account_id: account,
            folder_remote_id: "INBOX".into(),
            validity: 5,
            remote_id: "1".into(),
        }
    );

    let body = Body {
        text: "Hi Anna,\nsee you soon".into(),
        html: Some("<p>Hi Anna,</p>".into()),
    };
    store.save_body(id, &body).unwrap();
    assert_eq!(store.body(id).unwrap().as_ref(), Some(&body));

    // A resync with no snippet keeps the one from the body.
    let mut again = message(1, 100);
    again.snippet = String::new();
    store.save_messages(folder, &[again]).unwrap();
    let rows = list(&store, ListScope::Folder(folder), 10, None);
    assert_eq!(rows[0].snippet, "Hi Anna, see you soon");

    // The body goes with its message.
    store.set_validity(folder, 6).unwrap();
    assert_eq!(store.body(id).unwrap(), None);
    assert_eq!(store.location(id).unwrap_err(), StoreError::NotFound);
    assert_eq!(
        store.save_body(id, &body).unwrap_err(),
        StoreError::NotFound
    );
}

fn seen(store: &SqliteStore, folder: FolderId) -> Vec<bool> {
    list(store, ListScope::Folder(folder), 10, None)
        .iter()
        .map(|m| m.flags.seen)
        .collect()
}

#[test]
fn queued_read_state_shows_at_once_and_survives_a_sync() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(folder), 10, None)[0].id;

    store.queue(id, Action::Seen(true), 50).unwrap();
    assert_eq!(seen(&store, folder), [true]);
    // The server has not got it yet and still says unread.
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    assert_eq!(seen(&store, folder), [true]);

    assert!(store.due(account, 49).unwrap().is_empty());
    let other = self::account(&store, "b@example.com");
    assert!(store.due(other, 50).unwrap().is_empty());
    let due = store.due(account, 50).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].action, Action::Seen(true));
    assert_eq!(due[0].location, store.location(id).unwrap());

    store.finish(&due).unwrap();
    assert!(store.due(account, 50).unwrap().is_empty());
    // Now the server decides again.
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    assert_eq!(seen(&store, folder), [false]);
}

#[test]
fn a_change_made_while_sending_stays_queued() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(folder), 10, None)[0].id;

    store.queue(id, Action::Seen(true), 0).unwrap();
    store.queue(id, Action::Seen(true), 0).unwrap();
    let sending = store.due(account, 0).unwrap();
    assert_eq!(sending.len(), 1);
    // The user marks it unread again while the push runs.
    store.queue(id, Action::Seen(false), 0).unwrap();
    store.finish(&sending).unwrap();
    store.revert(&sending).unwrap();
    let left = store.due(account, 0).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].action, Action::Seen(false));
    assert_eq!(seen(&store, folder), [false]);

    assert_eq!(
        store
            .queue(MessageId(99), Action::Seen(true), 0)
            .unwrap_err(),
        StoreError::NotFound
    );
}

#[test]
fn revert_restores_the_state_before_the_first_action() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(folder), 10, None)[0].id;

    store.queue(id, Action::Seen(true), 0).unwrap();
    let ops = store.due(account, 0).unwrap();
    store.revert(&ops).unwrap();
    assert_eq!(seen(&store, folder), [false]);
    assert!(store.due(account, 0).unwrap().is_empty());
    // Already gone: nothing to do.
    store.revert(&ops).unwrap();
}

fn archive_folder(store: &SqliteStore, account: AccountId) {
    store
        .save_folders(
            account,
            &[
                NewFolder {
                    remote_id: "INBOX".into(),
                    name: "Inbox".into(),
                    role: FolderRole::Inbox,
                },
                NewFolder {
                    remote_id: "Archive".into(),
                    name: "Archive".into(),
                    role: FolderRole::Archive,
                },
            ],
        )
        .unwrap();
}

#[test]
fn archiving_hides_the_message_until_undo_or_the_server_has_it() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    archive_folder(&store, account);
    store
        .save_messages(folder, &[message(1, 100), message(2, 200)])
        .unwrap();
    let rows = list(&store, ListScope::Folder(folder), 10, None);
    let (newer, older) = (rows[0].id, rows[1].id);

    // Held until 50: undo before then takes it back, after then it may be sent.
    let op = store.queue(newer, Action::Archive, 50).unwrap();
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 1);
    // A sync does not bring it back while it waits.
    store.save_messages(folder, &[message(2, 200)]).unwrap();
    assert_eq!(
        list(&store, ListScope::Unified(FolderRole::Inbox), 10, None).len(),
        1
    );
    assert!(store.due(account, 49).unwrap().is_empty());
    assert_eq!(store.next_send(account).unwrap(), Some(50));
    assert!(!store.undo(op, 50).unwrap());
    assert!(store.undo(op, 49).unwrap());
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 2);
    assert_eq!(store.next_send(account).unwrap(), None);

    store.queue(older, Action::Archive, 0).unwrap();
    let due = store.due(account, 0).unwrap();
    assert_eq!(due[0].to.as_deref(), Some("Archive"));
    store.finish(&due).unwrap();
    assert_eq!(store.location(older).unwrap_err(), StoreError::NotFound);
    let rows = list(&store, ListScope::Folder(folder), 10, None);
    assert_eq!(rows.iter().map(|m| m.id).collect::<Vec<_>>(), [newer]);
}

#[test]
fn a_refused_archive_shows_the_message_again() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(folder), 10, None)[0].id;

    store.queue(id, Action::Archive, 0).unwrap();
    let due = store.due(account, 0).unwrap();
    // No archive folder on this account.
    assert_eq!(due[0].to, None);
    store.revert(&due).unwrap();
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 1);
    assert!(store.due(account, 0).unwrap().is_empty());
}
