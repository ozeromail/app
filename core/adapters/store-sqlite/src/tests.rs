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

/// One step of a sync at a time, through `save_changes`.
trait Steps {
    fn set_validity(&self, folder: FolderId, validity: u32) -> Result<(), StoreError>;
    fn save_messages(&self, folder: FolderId, messages: &[NewMessage]) -> Result<(), StoreError>;
}

impl Steps for SqliteStore {
    fn set_validity(&self, folder: FolderId, validity: u32) -> Result<(), StoreError> {
        let checkpoint = Checkpoint {
            validity,
            ..self.checkpoint(folder)?.0
        };
        self.save_changes(folder, &changes(checkpoint, vec![]))
    }

    fn save_messages(&self, folder: FolderId, messages: &[NewMessage]) -> Result<(), StoreError> {
        let checkpoint = self.checkpoint(folder)?.0;
        self.save_changes(folder, &changes(checkpoint, messages.to_vec()))
    }
}

fn changes(checkpoint: Checkpoint, new: Vec<NewMessage>) -> Changes<NewMessage> {
    Changes {
        checkpoint,
        new,
        flags: vec![],
        gone: vec![],
    }
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
            smtp: Some(Server {
                host: "smtp.example.com".into(),
                port: 465,
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
        references: vec![],
        from: Address {
            name: Some("Anna".into()),
            email: "anna@example.com".into(),
        },
        answer_to: vec![],
        recipients: vec![],
        subject: format!("Subject {uid}"),
        snippet: "Hello".into(),
        date,
        flags: Flags::default(),
        category: Category::People,
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
    for sql in [LIST_FOLDER, LIST_UNIFIED_BY_DATE, LIST_INBOX_TAB] {
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
fn saves_changes_since_the_checkpoint() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    assert_eq!(
        store.checkpoint(folder).unwrap(),
        (Checkpoint::default(), vec![])
    );
    let checkpoint = Checkpoint {
        validity: 5,
        next: 4,
    };
    let first = Changes {
        checkpoint,
        new: vec![message(1, 100), message(2, 200), message(3, 300)],
        flags: vec![],
        gone: vec![],
    };
    store.save_changes(folder, &first).unwrap();
    let (saved, mut known) = store.checkpoint(folder).unwrap();
    known.sort();
    assert_eq!(
        (saved, known),
        (checkpoint, vec!["1".into(), "2".into(), "3".into()])
    );

    // A queued "mark unread" stays on top of the server's flags until it is sent.
    let one = list(&store, ListScope::Folder(folder), 10, None)[2].id;
    store.queue(one, Action::Seen(false), 0).unwrap();
    let read = Flags {
        seen: true,
        flagged: true,
        ..Flags::default()
    };
    let second = Changes {
        checkpoint: Checkpoint {
            validity: 5,
            next: 5,
        },
        new: vec![message(4, 400)],
        flags: vec![("1".into(), read), ("3".into(), read)],
        gone: vec!["2".into()],
    };
    store.save_changes(folder, &second).unwrap();
    let rows = list(&store, ListScope::Folder(folder), 10, None);
    let got: Vec<_> = rows
        .iter()
        .map(|r| (r.subject.as_str(), r.flags.seen, r.flags.flagged))
        .collect();
    assert_eq!(
        got,
        [
            ("Subject 4", false, false),
            ("Subject 3", true, true),
            ("Subject 1", false, true),
        ]
    );
    assert_eq!(store.checkpoint(folder).unwrap().0.next, 5);

    // A new validity starts over.
    let reset = Changes {
        checkpoint: Checkpoint {
            validity: 6,
            next: 2,
        },
        new: vec![message(1, 500)],
        flags: vec![],
        gone: vec![],
    };
    store.save_changes(folder, &reset).unwrap();
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 1);
    assert_eq!(store.checkpoint(folder).unwrap().0, reset.checkpoint);
    assert_eq!(
        store.save_changes(FolderId(42), &reset).unwrap_err(),
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

    let file = |part: u32, name: &str| Attachment {
        part,
        name: name.into(),
        mime: "application/pdf".into(),
        size: 84_000,
    };
    let mut body = Body {
        text: "Hi Anna,\nsee you soon".into(),
        html: Some("<p>Hi Anna,</p>".into()),
        attachments: vec![file(4, "b.pdf"), file(2, "")],
    };
    store.save_body(id, &body).unwrap();
    // In part order, the nameless one too.
    body.attachments.reverse();
    assert_eq!(store.body(id).unwrap().as_ref(), Some(&body));
    let rows = list(&store, ListScope::Folder(folder), 10, None);
    assert_eq!(rows[0].attachments, ["", "b.pdf"]);
    // Saving again replaces the list.
    body.attachments = vec![file(3, "c.pdf")];
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

#[test]
fn without_body_lists_the_newest_inbox_mail_left_to_download() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    store
        .save_messages(folder, &[message(1, 100), message(2, 300), message(3, 200)])
        .unwrap();
    let ids: Vec<_> = list(&store, ListScope::Folder(folder), 10, None)
        .iter()
        .map(|r| r.id)
        .collect();
    // Newest first: 2, then 3, then 1; 3 already has its body.
    let body = Body {
        text: "Hi".into(),
        ..Body::default()
    };
    store.save_body(ids[1], &body).unwrap();

    let left = store.without_body(account, 10).unwrap();
    let uids: Vec<_> = left.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(uids, ["2", "1"]);
    // Only the newest count: 1 is older than the two newest.
    let newest: Vec<_> = store
        .without_body(account, 2)
        .unwrap()
        .into_iter()
        .map(|m| m.remote_id)
        .collect();
    assert_eq!(newest, ["2"]);
    let other = self::account(&store, "b@example.com");
    assert!(store.without_body(other, 10).unwrap().is_empty());
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
    assert!(!store.undo(&op, 50).unwrap());
    assert!(store.undo(&op, 49).unwrap());
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
fn archived_mail_goes_back_to_the_inbox() {
    let store = store();
    let account = account(&store, "a@example.com");
    archive_folder(&store, account);
    let archive = store
        .folders(account)
        .unwrap()
        .into_iter()
        .find(|f| f.role == FolderRole::Archive)
        .unwrap()
        .id;
    store.save_messages(archive, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(archive), 10, None)[0].id;

    store.queue(id, Action::Inbox, 0).unwrap();
    assert!(list(&store, ListScope::Folder(archive), 10, None).is_empty());
    let due = store.due(account, 0).unwrap();
    assert_eq!(due[0].to.as_deref(), Some("INBOX"));
    store.finish(&due).unwrap();
    assert_eq!(store.location(id).unwrap_err(), StoreError::NotFound);

    let inbox = inbox(&store, account);
    store.save_messages(inbox, &[message(2, 200)]).unwrap();
    let id = list(&store, ListScope::Folder(inbox), 10, None)[0].id;
    assert_eq!(
        store.queue(id, Action::Inbox, 0).unwrap_err(),
        StoreError::NotAllowed
    );
}

#[test]
fn trash_moves_the_message_like_archive() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(folder), 10, None)[0].id;
    assert_eq!(
        store.queue(id, Action::Trash, 0).unwrap_err(),
        StoreError::NoTrash
    );

    store
        .save_folders(
            account,
            &[NewFolder {
                remote_id: "Trash".into(),
                name: "Trash".into(),
                role: FolderRole::Trash,
            }],
        )
        .unwrap();
    let op = store.queue(id, Action::Trash, 50).unwrap();
    assert!(list(&store, ListScope::Folder(folder), 10, None).is_empty());
    assert!(store.undo(&op, 49).unwrap());
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 1);

    store.queue(id, Action::Trash, 0).unwrap();
    let due = store.due(account, 0).unwrap();
    assert_eq!(
        (due[0].action, due[0].to.as_deref()),
        (Action::Trash, Some("Trash"))
    );
    store.finish(&due).unwrap();
    assert_eq!(store.location(id).unwrap_err(), StoreError::NotFound);
}

/// An account with an inbox and a trash.
fn inbox_and_trash(store: &SqliteStore) -> (AccountId, FolderId, FolderId) {
    let account = account(store, "a@example.com");
    let folders = store
        .save_folders(
            account,
            &[
                NewFolder {
                    remote_id: "INBOX".into(),
                    name: "Inbox".into(),
                    role: FolderRole::Inbox,
                },
                NewFolder {
                    remote_id: "Trash".into(),
                    name: "Trash".into(),
                    role: FolderRole::Trash,
                },
            ],
        )
        .unwrap();
    (account, folders[0].id, folders[1].id)
}

/// `message(uid)` as the server lists it in another folder, under `remote_id`.
fn moved(uid: u32, date: i64, remote_id: &str) -> NewMessage {
    NewMessage {
        remote_id: remote_id.into(),
        ..message(uid, date)
    }
}

fn ids(rows: &[MessageRow]) -> Vec<MessageId> {
    rows.iter().map(|r| r.id).collect()
}

#[test]
fn moved_mail_shows_in_its_new_folder_at_once() {
    let store = store();
    let (account, inbox, trash) = inbox_and_trash(&store);
    store
        .save_messages(inbox, &[message(1, 100), message(2, 200)])
        .unwrap();
    let rows = list(&store, ListScope::Folder(inbox), 10, None);
    let (two, one) = (rows[0].id, rows[1].id);
    let body = Body {
        text: "Hi".into(),
        ..Body::default()
    };
    store.save_body(one, &body).unwrap();
    store.queue(one, Action::Seen(true), 0).unwrap();
    store.finish(&store.due(account, 0).unwrap()).unwrap();

    let held = store.queue(one, Action::Trash, 50).unwrap();
    store.queue(two, Action::Trash, 50).unwrap();
    assert!(list(&store, ListScope::Folder(inbox), 10, None).is_empty());
    let copies = list(&store, ListScope::Unified(FolderRole::Trash), 10, None);
    assert_eq!(copies.len(), 2);
    let copy = copies[1].id;
    assert_eq!(copies[1].role, FolderRole::Trash);
    assert!(copies[1].flags.seen);
    assert_eq!(store.body(copy).unwrap(), Some(body));
    // Until the move is sent, the message is where it was.
    assert_eq!(
        store.location(copy).unwrap(),
        MessageLocation {
            id: copy,
            ..store.location(one).unwrap()
        }
    );
    // Taking the move back takes the copy too.
    assert!(store.undo(&held, 49).unwrap());
    assert_eq!(
        ids(&list(&store, ListScope::Folder(inbox), 10, None)),
        [one]
    );
    assert_eq!(list(&store, ListScope::Folder(trash), 10, None).len(), 1);
    assert_eq!(store.body(copy).unwrap(), None);

    // A sync of the trash before the server has the move keeps the copy.
    store.save_messages(trash, &[message(3, 50)]).unwrap();
    assert_eq!(list(&store, ListScope::Folder(trash), 10, None).len(), 2);
    let due = store.due(account, 50).unwrap();
    assert_eq!(due.len(), 1);
    assert!(store.unclaimed(account).unwrap().is_empty());
    store.finish(&due).unwrap();
    let copy = list(&store, ListScope::Folder(trash), 10, None)[0].id;
    assert_eq!(
        store.location(copy).unwrap().remote_id,
        format!("~{}", two.0)
    );
    store.queue(copy, Action::Seen(false), 60).unwrap();
    assert!(store.due(account, 60).unwrap().is_empty());
    assert_eq!(store.next_send(account).unwrap(), None);
    let unclaimed = store.unclaimed(account).unwrap();
    assert_eq!(unclaimed.iter().map(|f| f.id).collect::<Vec<_>>(), [trash]);
    // Not something the server knows by that id.
    assert_eq!(store.checkpoint(trash).unwrap().1, ["3"]);

    // The server's own takes the copy's place, under its id.
    store.save_messages(trash, &[moved(2, 200, "7")]).unwrap();
    let rows = list(&store, ListScope::Folder(trash), 10, None);
    assert_eq!((rows.len(), rows[0].id), (2, copy));
    assert_eq!(store.location(copy).unwrap().remote_id, "7");
    assert!(store.unclaimed(account).unwrap().is_empty());
    assert_eq!(store.due(account, 60).unwrap()[0].location.remote_id, "7");
}

#[test]
fn a_copy_takes_the_id_the_server_gave_its_message() {
    let store = store();
    let (account, inbox, trash) = inbox_and_trash(&store);
    store.set_validity(trash, 4).unwrap();
    store
        .save_messages(inbox, &[message(1, 100), message(2, 200)])
        .unwrap();
    for row in list(&store, ListScope::Folder(inbox), 10, None) {
        store.queue(row.id, Action::Trash, 0).unwrap();
    }
    let due = store.due(account, 0).unwrap();
    let copies = ids(&list(&store, ListScope::Folder(trash), 10, None));
    // Only for the validity the trash has.
    let moved = |validity| Moved {
        validity,
        ids: vec![("1".into(), "21".into()), ("2".into(), "22".into())],
    };
    store.land(&due, &moved(Some(5))).unwrap();
    assert_eq!(store.location(copies[0]).unwrap().remote_id, "2");
    store.land(&due, &moved(Some(4))).unwrap();
    store.finish(&due).unwrap();
    assert!(store.unclaimed(account).unwrap().is_empty());
    let landed: Vec<_> = copies
        .iter()
        .map(|id| store.location(*id).unwrap().remote_id)
        .collect();
    assert_eq!(landed, ["22", "21"]);
    // The server knows them by these ids: a sync keeps them.
    store.save_messages(trash, &[]).unwrap();
    assert_eq!(
        ids(&list(&store, ListScope::Folder(trash), 10, None)),
        copies
    );
}

#[test]
fn a_copy_the_server_does_not_bring_goes() {
    let store = store();
    let (account, inbox, trash) = inbox_and_trash(&store);
    store.save_messages(inbox, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(inbox), 10, None)[0].id;
    store.queue(id, Action::Trash, 0).unwrap();
    store.finish(&store.due(account, 0).unwrap()).unwrap();
    assert_eq!(list(&store, ListScope::Folder(trash), 10, None).len(), 1);
    // A page of older mail leaves it.
    store.save_messages(trash, &[message(4, 50)]).unwrap();
    assert_eq!(list(&store, ListScope::Folder(trash), 10, None).len(), 2);
    let checkpoint = Checkpoint {
        validity: 0,
        next: 6,
    };
    store
        .save_changes(trash, &changes(checkpoint, vec![message(5, 500)]))
        .unwrap();
    let rows = list(&store, ListScope::Folder(trash), 10, None);
    assert_eq!(subjects(&rows), ["Subject 5", "Subject 4"]);
}

#[test]
fn a_new_validity_keeps_copies_for_the_server_to_claim() {
    let store = store();
    let (account, inbox, trash) = inbox_and_trash(&store);
    store.save_messages(inbox, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(inbox), 10, None)[0].id;
    store.queue(id, Action::Trash, 0).unwrap();
    store.finish(&store.due(account, 0).unwrap()).unwrap();
    let copy = list(&store, ListScope::Folder(trash), 10, None)[0].id;
    let checkpoint = Checkpoint {
        validity: 9,
        next: 0,
    };
    store
        .save_changes(trash, &changes(checkpoint, vec![moved(1, 100, "4")]))
        .unwrap();
    assert_eq!(
        ids(&list(&store, ListScope::Folder(trash), 10, None)),
        [copy]
    );
}

#[test]
fn trash_a_batch_then_empty_the_trash() {
    let store = store();
    let (account, inbox, trash) = inbox_and_trash(&store);
    store
        .save_messages(inbox, &[message(1, 100), message(2, 200)])
        .unwrap();
    store.save_messages(trash, &[message(3, 300)]).unwrap();
    for row in list(&store, ListScope::Folder(inbox), 10, None) {
        store.queue(row.id, Action::Trash, 10).unwrap();
    }
    assert_eq!(list(&store, ListScope::Folder(trash), 10, None).len(), 3);

    // Emptying shows at once; the copies are deleted by id once the server has them there.
    let emptied = store.queue_empty(trash, 20).unwrap();
    assert_eq!(emptied.len(), 3);
    assert!(list(&store, ListScope::Folder(trash), 10, None).is_empty());
    assert!(list(&store, ListScope::Folder(inbox), 10, None).is_empty());
    let due = store.due(account, 20).unwrap();
    let actions: Vec<_> = due.iter().map(|o| o.action).collect();
    assert_eq!(actions, [Action::Trash, Action::Trash, Action::Empty]);
    assert!(store.others(&due[2..]).unwrap().is_empty());
    store.finish(&due).unwrap();
    assert!(store.due(account, 20).unwrap().is_empty());
    assert_eq!(store.unclaimed(account).unwrap().len(), 1);

    store
        .save_messages(trash, &[moved(1, 100, "8"), moved(2, 200, "9")])
        .unwrap();
    assert!(list(&store, ListScope::Folder(trash), 10, None).is_empty());
    let due = store.due(account, 20).unwrap();
    let mut sent: Vec<_> = due
        .iter()
        .map(|o| (o.action, o.location.remote_id.as_str()))
        .collect();
    sent.sort_unstable();
    assert_eq!(sent, [(Action::Delete, "8"), (Action::Delete, "9")]);
    store.finish(&due).unwrap();
    assert!(store.due(account, 20).unwrap().is_empty());
    assert!(store.unclaimed(account).unwrap().is_empty());
    store.save_messages(trash, &[]).unwrap();
    assert!(list(&store, ListScope::Folder(trash), 10, None).is_empty());
}

#[test]
fn deletes_for_good_only_from_the_trash() {
    let store = store();
    let (account, inbox, trash) = inbox_and_trash(&store);
    store.save_messages(inbox, &[message(1, 100)]).unwrap();
    store
        .save_messages(trash, &[message(2, 200), message(3, 300)])
        .unwrap();
    let kept = list(&store, ListScope::Folder(inbox), 10, None)[0].id;
    assert_eq!(
        store.queue(kept, Action::Delete, 0).unwrap_err(),
        StoreError::NotAllowed
    );
    assert_eq!(
        store.queue_empty(inbox, 0).unwrap_err(),
        StoreError::NotAllowed
    );

    // Held like a move: gone from the list, back on undo.
    let one = list(&store, ListScope::Folder(trash), 10, None)[0].id;
    let op = store.queue(one, Action::Delete, 50).unwrap();
    assert_eq!(list(&store, ListScope::Folder(trash), 10, None).len(), 1);
    assert!(store.undo(&op, 49).unwrap());

    // Emptying takes every message of the folder.
    let all = store.queue_empty(trash, 0).unwrap();
    assert_eq!(all.len(), 2);
    assert!(list(&store, ListScope::Folder(trash), 10, None).is_empty());
    let due = store.due(account, 0).unwrap();
    assert!(
        due.iter()
            .all(|o| o.action == Action::Empty && o.to.is_none())
    );
    // What the server must keep there: whatever the ops leave out.
    assert_eq!(
        store.others(&due[..1]).unwrap(),
        [due[1].location.remote_id.clone()]
    );
    assert!(store.others(&due).unwrap().is_empty());
    store.finish(&due).unwrap();
    assert_eq!(store.location(one).unwrap_err(), StoreError::NotFound);
    assert_eq!(list(&store, ListScope::Folder(inbox), 10, None).len(), 1);
}

#[test]
fn moves_only_leave_folders_that_allow_them() {
    let store = store();
    let account = account(&store, "a@example.com");
    archive_folder(&store, account);
    let archive = store.folders(account).unwrap()[1].id;
    store.save_messages(archive, &[message(1, 100)]).unwrap();
    let row = &list(&store, ListScope::Folder(archive), 10, None)[0];
    assert_eq!(row.role, FolderRole::Archive);
    assert_eq!(
        store.queue(row.id, Action::Archive, 0).unwrap_err(),
        StoreError::NotAllowed
    );
    assert!(store.due(account, 0).unwrap().is_empty());
    store.queue(row.id, Action::Seen(true), 0).unwrap();
}

#[test]
fn an_inbox_with_only_mail_being_archived_is_empty() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    archive_folder(&store, account);
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    assert!(!store.inbox_is_empty().unwrap());
    let id = list(&store, ListScope::Folder(folder), 10, None)[0].id;
    store.queue(id, Action::Archive, 0).unwrap();
    assert!(store.inbox_is_empty().unwrap());
}

#[test]
fn a_refused_archive_shows_the_message_again() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    store.save_messages(folder, &[message(1, 100)]).unwrap();
    let id = list(&store, ListScope::Folder(folder), 10, None)[0].id;
    assert_eq!(
        store.queue(id, Action::Archive, 0).unwrap_err(),
        StoreError::NoArchive
    );

    archive_folder(&store, account);
    store.queue(id, Action::Archive, 0).unwrap();
    let due = store.due(account, 0).unwrap();
    store.revert(&due).unwrap();
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 1);
    assert!(store.due(account, 0).unwrap().is_empty());
}

#[test]
fn tabs_list_and_count_their_own_messages() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    assert!(store.inbox_is_empty().unwrap());
    let mut news = message(2, 200);
    news.category = Category::Newsletters;
    store
        .save_messages(folder, &[message(1, 100), news.clone()])
        .unwrap();

    let tab = |category| {
        let rows = list(&store, ListScope::Inbox(category), 10, None);
        rows.iter().map(|r| r.subject.clone()).collect::<Vec<_>>()
    };
    assert_eq!(tab(Category::People), ["Subject 1"]);
    assert_eq!(tab(Category::Newsletters), ["Subject 2"]);
    assert!(tab(Category::Notifications).is_empty());
    let mut unread = store.inbox_unread().unwrap();
    unread.sort_by_key(|(c, _)| Category::ALL.iter().position(|a| a == c));
    assert_eq!(unread, [(Category::People, 1), (Category::Newsletters, 1)]);
    assert!(!store.inbox_is_empty().unwrap());

    // A resync sorts a known message again.
    news.category = Category::Notifications;
    store.save_messages(folder, &[news]).unwrap();
    assert_eq!(tab(Category::Notifications), ["Subject 2"]);
    assert!(tab(Category::Newsletters).is_empty());
}

fn draft(account: AccountId) -> Draft {
    Draft {
        account_id: account,
        to: vec!["bob@example.org".into(), "cat@example.net".into()],
        subject: "Hi".into(),
        text: "Hello".into(),
        reply_to: None,
        quote: String::new(),
        forward: None,
    }
}

/// `draft` as `undo_send` saves it at `now`, without the id it gets.
fn saved(draft: &Draft, now: i64) -> SavedDraft {
    SavedDraft {
        id: None,
        account_id: Some(draft.account_id),
        to: draft.to.join(", "),
        subject: draft.subject.clone(),
        text: draft.text.clone(),
        reply_to: draft.reply_to,
        quote: draft.quote.clone(),
        forward: draft.forward,
        saved: now,
    }
}

/// What `undo_send` gives back, without its id, which must be the saved draft's.
fn taken_back(store: &SqliteStore, id: OutboxId, now: i64) -> Option<SavedDraft> {
    let draft = store.undo_send(id, now).unwrap()?;
    assert_eq!(store.drafts().unwrap()[0], draft);
    Some(SavedDraft { id: None, ..draft })
}

#[test]
fn a_held_message_can_be_taken_back_until_it_is_due() {
    let store = store();
    let a = account(&store, "a@example.com");
    let id = store
        .queue_send(&draft(a), None, "1@example.com", 100, 105)
        .unwrap();
    assert_eq!(store.next_send(a).unwrap(), Some(105));
    assert!(store.due_sends(a, 104).unwrap().is_empty());

    assert_eq!(taken_back(&store, id, 104), Some(saved(&draft(a), 104)));
    assert_eq!(taken_back(&store, id, 104), None);
    assert_eq!(store.next_send(a).unwrap(), None);

    let id = store
        .queue_send(&draft(a), None, "2@example.com", 100, 105)
        .unwrap();
    assert_eq!(store.undo_send(id, 105).unwrap(), None, "too late");
    let due = store.due_sends(a, 105).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].to, draft(a).to);
    assert_eq!(due[0].message_id, "2@example.com");
    assert_eq!(due[0].date, 100);
    store.sent(id).unwrap();
    assert!(store.due_sends(a, 200).unwrap().is_empty());
}

#[test]
fn a_refused_message_waits_to_be_fixed() {
    let store = store();
    let a = account(&store, "a@example.com");
    let id = store
        .queue_send(&draft(a), None, "1@example.com", 100, 100)
        .unwrap();
    store.refused(id, "550 no such user").unwrap();
    assert!(store.due_sends(a, 200).unwrap().is_empty());
    assert_eq!(store.next_send(a).unwrap(), None);
    let unsent = store.unsent().unwrap();
    assert_eq!(
        (unsent[0].id, unsent[0].reason.as_str()),
        (id, "550 no such user")
    );
    // Taken back to edit, it is gone from the outbox.
    assert_eq!(taken_back(&store, id, 0), Some(saved(&draft(a), 0)));
    assert!(store.unsent().unwrap().is_empty());
}

#[test]
fn a_reply_answers_the_message_id() {
    let store = store();
    let a = account(&store, "a@example.com");
    store
        .save_messages(inbox(&store, a), &[message(1, 100)])
        .unwrap();
    let parent = list(&store, ListScope::Folder(inbox(&store, a)), 10, None)[0].id;
    let reply = Draft {
        reply_to: Some(parent),
        ..draft(a)
    };
    store
        .queue_send(&reply, None, "2@example.com", 100, 100)
        .unwrap();
    let due = store.due_sends(a, 100).unwrap();
    assert_eq!(due[0].in_reply_to.as_deref(), Some("<1@example.com>"));

    let gone = Draft {
        reply_to: Some(MessageId(999)),
        ..draft(a)
    };
    assert_eq!(
        store.queue_send(&gone, None, "3@example.com", 100, 100),
        Err(StoreError::NotFound)
    );
}

#[test]
fn a_forward_keeps_its_message_through_the_outbox() {
    let store = store();
    let a = account(&store, "a@example.com");
    store
        .save_messages(inbox(&store, a), &[message(1, 100)])
        .unwrap();
    let original = list(&store, ListScope::Folder(inbox(&store, a)), 10, None)[0].id;
    let forward = Draft {
        forward: Some(original),
        ..draft(a)
    };
    let id = store
        .queue_send(&forward, None, "2@example.com", 100, 105)
        .unwrap();
    assert_eq!(store.due_sends(a, 105).unwrap()[0].forward, Some(original));
    // Taken back, the draft still forwards it.
    assert_eq!(taken_back(&store, id, 104), Some(saved(&forward, 104)));
    assert_eq!(store.drafts().unwrap()[0].forward, Some(original));

    // A message gone since leaves only the text to send.
    let gone = Draft {
        forward: Some(MessageId(999)),
        ..draft(a)
    };
    store
        .queue_send(&gone, None, "3@example.com", 100, 100)
        .unwrap();
    assert_eq!(store.due_sends(a, 100).unwrap()[0].forward, None);
}

#[test]
fn accounts_keep_their_smtp_server() {
    let store = store();
    account(&store, "a@example.com");
    let smtp = store.accounts().unwrap()[0].smtp.clone().unwrap();
    assert_eq!((smtp.host.as_str(), smtp.port), ("smtp.example.com", 465));
}

#[test]
fn new_people_mail_is_taken_once() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    let mut news = message(2, 200);
    news.category = Category::Newsletters;
    let mut read = message(3, 300);
    read.flags.seen = true;
    store
        .save_messages(folder, &[message(1, 100), news, read])
        .unwrap();

    let subjects = |rows: Vec<MessageRow>| rows.into_iter().map(|r| r.subject).collect::<Vec<_>>();
    assert_eq!(subjects(store.take_fresh(0).unwrap()), ["Subject 1"]);
    // A resync of known mail brings nothing new; a new message does.
    store
        .save_messages(folder, &[message(1, 100), message(4, 400)])
        .unwrap();
    assert_eq!(subjects(store.take_fresh(0).unwrap()), ["Subject 4"]);
    assert!(store.take_fresh(0).unwrap().is_empty());
    // Old mail that comes back, say after a mailbox reset, is not news.
    store.save_messages(folder, &[message(6, 600)]).unwrap();
    assert!(store.take_fresh(600).unwrap().is_empty());
    store.save_messages(folder, &[message(5, 500)]).unwrap();
    store.clear_fresh().unwrap();
    assert!(store.take_fresh(0).unwrap().is_empty());
}

/// Message `uid` with Message-ID `<uid@example.com>` answering `refs`.
fn reply(uid: u32, date: i64, refs: &[u32]) -> NewMessage {
    NewMessage {
        references: refs.iter().map(|r| format!("<{r}@example.com>")).collect(),
        ..message(uid, date)
    }
}

fn subjects(rows: &[MessageRow]) -> Vec<&str> {
    rows.iter().map(|r| r.subject.as_str()).collect()
}

#[test]
fn replies_join_one_conversation_in_any_order() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    // 2 answers 1; 3 answers 2 but arrives before both; 4 answers 1 too.
    store
        .save_messages(folder, &[reply(3, 300, &[1, 2])])
        .unwrap();
    store.save_messages(folder, &[message(5, 50)]).unwrap();
    store.save_messages(folder, &[reply(2, 200, &[1])]).unwrap();
    store
        .save_messages(folder, &[message(1, 100), reply(4, 400, &[1])])
        .unwrap();

    let rows = list(&store, ListScope::Folder(folder), 10, None);
    assert_eq!(subjects(&rows), ["Subject 4", "Subject 5"]);
    assert_eq!((rows[0].thread_count, rows[0].thread_unread), (4, 4));
    assert_eq!((rows[1].thread_count, rows[1].thread_unread), (1, 1));

    let thread = store.thread(rows[0].id).unwrap();
    assert_eq!(
        subjects(&thread),
        ["Subject 1", "Subject 2", "Subject 3", "Subject 4"]
    );
    assert_eq!(store.thread(thread[0].id).unwrap(), thread);
}

#[test]
fn a_reply_bridges_two_conversations() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    store
        .save_messages(folder, &[message(1, 100), message(2, 200)])
        .unwrap();
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 2);

    store
        .save_messages(folder, &[reply(3, 300, &[1, 2])])
        .unwrap();
    let rows = list(&store, ListScope::Folder(folder), 10, None);
    assert_eq!(subjects(&rows), ["Subject 3"]);
    assert_eq!(rows[0].thread_count, 3);
}

#[test]
fn conversations_stay_in_their_account() {
    let store = store();
    let a = inbox(&store, account(&store, "a@example.com"));
    let b = inbox(&store, account(&store, "b@example.com"));
    store.save_messages(a, &[message(1, 100)]).unwrap();
    store.save_messages(b, &[reply(2, 200, &[1])]).unwrap();
    assert_eq!(
        store
            .thread(list(&store, ListScope::Folder(b), 10, None)[0].id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn actions_apply_to_the_whole_conversation() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    archive_folder(&store, account);
    store
        .save_messages(
            folder,
            &[message(1, 100), reply(2, 200, &[1]), message(3, 300)],
        )
        .unwrap();
    let rows = list(&store, ListScope::Folder(folder), 10, None);
    let conversation = rows[1].id;

    store.queue(conversation, Action::Seen(true), 0).unwrap();
    let rows = list(&store, ListScope::Folder(folder), 10, None);
    assert_eq!((rows[1].thread_count, rows[1].thread_unread), (2, 0));
    assert_eq!(rows[0].thread_unread, 1);
    // Unread is only the message itself, as if it had just come.
    let ops = store.queue(conversation, Action::Seen(false), 0).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(
        list(&store, ListScope::Folder(folder), 10, None)[1].thread_unread,
        1
    );

    let ops = store.queue(conversation, Action::Archive, 50).unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(
        subjects(&list(&store, ListScope::Folder(folder), 10, None)),
        ["Subject 3"]
    );
    assert!(store.thread(conversation).unwrap().is_empty());
    // One op no longer held: none come back.
    let mut late = ops.clone();
    late.push(OpId(999));
    assert!(!store.undo(&late, 49).unwrap());
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 1);
    assert!(store.undo(&ops, 49).unwrap());
    assert_eq!(list(&store, ListScope::Folder(folder), 10, None).len(), 2);
}

#[test]
fn a_conversation_reads_its_index() {
    let store = store();
    let conn = store.conn();
    for sql in [THREAD, THREAD_IDS] {
        let plan: Vec<String> = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap()
            .query_map([1], |r| r.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let plan = plan.join("\n");
        assert!(plan.contains("message_thread"), "{plan}");
        assert!(!plan.contains("SCAN"), "{plan}");
    }
}

#[test]
fn a_conversation_shows_the_users_replies() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folders = store
        .save_folders(
            account,
            &[
                NewFolder {
                    remote_id: "INBOX".into(),
                    name: "Inbox".into(),
                    role: FolderRole::Inbox,
                },
                NewFolder {
                    remote_id: "Sent".into(),
                    name: "Sent".into(),
                    role: FolderRole::Sent,
                },
            ],
        )
        .unwrap();
    let (inbox, sent) = (folders[0].id, folders[1].id);
    store
        .save_messages(inbox, &[message(1, 100), reply(3, 300, &[1, 2])])
        .unwrap();
    let mine = NewMessage {
        from: Address {
            name: None,
            email: "A@example.com".into(),
        },
        ..reply(2, 200, &[1])
    };
    store.save_messages(sent, &[mine]).unwrap();

    // The row counts the user's reply and names who wrote, each once.
    let rows = list(&store, ListScope::Folder(inbox), 10, None);
    assert_eq!(
        (subjects(&rows), rows[0].thread_count, rows[0].thread_unread),
        (vec!["Subject 3"], 3, 2)
    );
    let senders: Vec<_> = rows[0]
        .senders
        .iter()
        .map(|s| (s.name.as_str(), s.me))
        .collect();
    assert_eq!(senders, [("Anna", false), ("a@example.com", true)]);
    let thread = store.thread(rows[0].id).unwrap();
    assert_eq!(subjects(&thread), ["Subject 1", "Subject 2", "Subject 3"]);
    let sent_flags: Vec<_> = thread.iter().map(|m| m.role == FolderRole::Sent).collect();
    assert_eq!(sent_flags, [false, true, false]);
    // A copy of an inbox message in sent shows once.
    store
        .save_messages(
            sent,
            &[NewMessage {
                remote_id: "9".into(),
                ..message(1, 100)
            }],
        )
        .unwrap();
    assert_eq!(store.thread(rows[0].id).unwrap().len(), 3);
    assert_eq!(
        list(&store, ListScope::Folder(inbox), 10, None)[0].thread_count,
        3
    );
    // Reading the conversation leaves the sent folder alone.
    store.queue(rows[0].id, Action::Seen(true), 0).unwrap();
    assert!(
        !list(&store, ListScope::Folder(sent), 10, None)[0]
            .flags
            .seen
    );
}

#[test]
fn keeps_what_a_server_search_found_apart_until_the_next_one() {
    let store = store();
    let a = account(&store, "a@example.com");
    let inbox = inbox(&store, a);
    store.save_messages(inbox, &[message(5, 50)]).unwrap();
    // The server matched words in bodies the device does not have.
    let found = vec![(inbox, vec![message(1, 10), message(5, 50)])];
    store.save_found(a, "Quarterly  Report", &found).unwrap();
    let ids = |rows: Vec<MessageRow>| -> Vec<i64> { rows.iter().map(|m| m.date).collect() };
    assert_eq!(ids(store.search("quarterly report", 10).unwrap()), [50, 10]);
    assert!(store.search("quarterly", 10).unwrap().is_empty());
    // Lists, syncs and paging see only the folder's own mail.
    assert_eq!(ids(list(&store, ListScope::Folder(inbox), 10, None)), [50]);
    assert_eq!(store.checkpoint(inbox).unwrap().1, ["5"]);
    assert_eq!(store.oldest(inbox).unwrap(), Some(50));
    // The keep limit is for synced mail; search finds older mail on purpose.
    store.prune(a, 20).unwrap();
    assert_eq!(ids(store.search("quarterly report", 10).unwrap()), [50, 10]);

    // The next search lets it go; loading older mail makes it the folder's own.
    store.save_found(a, "other", &[]).unwrap();
    assert!(store.search("quarterly report", 10).unwrap().is_empty());
    store
        .save_found(a, "report", &[(inbox, vec![message(1, 10)])])
        .unwrap();
    store.save_messages(inbox, &[message(1, 10)]).unwrap();
    assert_eq!(
        ids(list(&store, ListScope::Folder(inbox), 10, None)),
        [50, 10]
    );
    store.save_found(a, "other", &[]).unwrap();
    assert_eq!(store.checkpoint(inbox).unwrap().1.len(), 2);
}

#[test]
fn search_finds_words_in_subject_sender_and_body() {
    let store = store();
    let account = account(&store, "a@example.com");
    let folder = inbox(&store, account);
    let mut ticket = message(2, 200);
    ticket.subject = "Ваш билет на поезд, ёлка".into();
    ticket.snippet = "Место 12".into();
    ticket.from = Address {
        name: Some("Rail Café".into()),
        email: "tickets@rail.example".into(),
    };
    store
        .save_messages(folder, &[message(1, 100), ticket, message(3, 300)])
        .unwrap();
    let id = |uid: usize| list(&store, ListScope::Folder(folder), 10, None)[3 - uid].id;
    store
        .save_body(
            id(3),
            &Body {
                text: "The invoice is attached".into(),
                ..Body::default()
            },
        )
        .unwrap();

    let found = |text: &str| subjects(&store.search(text, 10).unwrap()).join(", ");
    assert_eq!(found("БИЛЕТ поез"), "Ваш билет на поезд, ёлка");
    assert_eq!(found("cafe"), "Ваш билет на поезд, ёлка");
    assert_eq!(found("елка"), "Ваш билет на поезд, ёлка");
    assert_eq!(found("Ёлка"), "Ваш билет на поезд, ёлка");
    assert_eq!(found("rail.example"), "Ваш билет на поезд, ёлка");
    assert_eq!(found("invoice"), "Subject 3");
    assert_eq!(found("hello"), "Subject 1");
    // A copy in another folder shows once.
    let sent = store
        .save_folders(
            account,
            &[NewFolder {
                remote_id: "Sent".into(),
                name: "Sent".into(),
                role: FolderRole::Sent,
            }],
        )
        .unwrap()[0]
        .id;
    store.save_messages(sent, &[message(1, 100)]).unwrap();
    assert_eq!(found("hello"), "Subject 1");
    assert_eq!(found("anna"), "Subject 3, Subject 1");
    assert_eq!(found("anna invoice"), "Subject 3");
    assert_eq!(found("\"anna\" OR NEAR("), "");
    assert_eq!(found("  "), "");

    archive_folder(&store, account);
    // Being archived, it is found once: in the archive.
    store.queue(id(3), Action::Archive, 0).unwrap();
    assert_eq!(found("anna"), "Subject 3, Subject 1");
    let archived = &store.search("invoice", 10).unwrap()[0];
    assert_eq!(
        (archived.id == id(3), archived.role),
        (false, FolderRole::Archive)
    );
}

#[test]
fn each_new_account_gets_the_next_color() {
    let store = store();
    for email in ["a@example.com", "b@example.com", "c@example.com"] {
        account(&store, email);
    }
    let colors: Vec<_> = store.accounts().unwrap().iter().map(|a| a.color).collect();
    assert_eq!(colors, [0, 1, 2]);
}

#[test]
fn sets_an_account_color() {
    let store = store();
    let a = account(&store, "a@example.com");
    store.set_account_color(a, 7).unwrap();
    assert_eq!(store.accounts().unwrap()[0].color, 7);
    assert_eq!(
        store.set_account_color(AccountId(99), 1),
        Err(StoreError::NotFound)
    );
}

#[test]
fn names_an_account() {
    let store = store();
    let a = account(&store, "a@example.com");
    store.set_account_label(a, " Work ").unwrap();
    assert_eq!(store.accounts().unwrap()[0].label, "Work");
    store.set_account_label(a, "").unwrap();
    assert_eq!(store.accounts().unwrap()[0].label, "");
    assert_eq!(
        store.set_account_label(AccountId(99), "x"),
        Err(StoreError::NotFound)
    );
}

#[test]
fn prunes_mail_older_than_an_account_keeps() {
    let store = store();
    let a = account(&store, "a@example.com");
    let b = account(&store, "b@example.com");
    store.set_account_keep(a, 30).unwrap();
    assert_eq!(store.accounts().unwrap()[0].keep_days, 30);
    let (inbox_a, inbox_b) = (inbox(&store, a), inbox(&store, b));
    assert_eq!(store.oldest(inbox_a).unwrap(), None);
    let old = [message(1, 10), message(2, 20), message(3, 30)];
    store.save_messages(inbox_a, &old).unwrap();
    store.save_messages(inbox_b, &old).unwrap();
    assert_eq!(store.oldest(inbox_a).unwrap(), Some(10));
    // A queued action keeps its message until the server has it.
    let held = list(&store, ListScope::Folder(inbox_a), 10, None)
        .into_iter()
        .find(|m| m.date == 10)
        .unwrap();
    store.queue(held.id, Action::Seen(true), 0).unwrap();

    store.prune(a, 25).unwrap();
    let (_, known) = store.checkpoint(inbox_a).unwrap();
    assert_eq!(known.len(), 2);
    assert_eq!(store.oldest(inbox_a).unwrap(), Some(10));
    assert_eq!(store.checkpoint(inbox_b).unwrap().1.len(), 3);
    // Kept longer, or reset, a folder may have older mail again.
    store.set_older_done(inbox_a, true).unwrap();
    store.set_account_keep(a, 30).unwrap();
    assert!(store.older_done(inbox_a).unwrap());
    store.set_account_keep(a, 0).unwrap();
    assert!(!store.older_done(inbox_a).unwrap());
    store.set_older_done(inbox_a, true).unwrap();
    store.set_validity(inbox_a, 5).unwrap();
    assert!(!store.older_done(inbox_a).unwrap());
    assert_eq!(
        store.set_account_keep(AccountId(99), 1),
        Err(StoreError::NotFound)
    );
}

#[test]
fn marks_an_account_that_needs_sign_in() {
    let store = store();
    let a = account(&store, "a@example.com");
    assert!(!store.accounts().unwrap()[0].needs_sign_in);
    store.set_needs_sign_in(a, true).unwrap();
    assert!(store.accounts().unwrap()[0].needs_sign_in);
    store.set_needs_sign_in(a, false).unwrap();
    assert!(!store.accounts().unwrap()[0].needs_sign_in);
    assert_eq!(
        store.set_needs_sign_in(AccountId(99), true),
        Err(StoreError::NotFound)
    );
}

#[test]
fn removing_an_account_removes_its_mail() {
    let store = store();
    let a = account(&store, "a@example.com");
    let b = account(&store, "b@example.com");
    store
        .save_messages(inbox(&store, a), &[message(1, 100)])
        .unwrap();
    store
        .save_messages(inbox(&store, b), &[message(2, 200)])
        .unwrap();

    store.remove_account(a).unwrap();

    let accounts = store.accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, b);
    let left = list(&store, ListScope::Inbox(Category::People), 10, None);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].subject, "Subject 2");
    assert_eq!(store.remove_account(a), Err(StoreError::NotFound));

    store.remove_account(b).unwrap();
    let c = account(&store, "c@example.com");
    assert!(c.0 > b.0, "ids are not used again");
}

#[test]
fn a_new_account_gets_a_free_color() {
    let store = store();
    let a = account(&store, "a@example.com");
    account(&store, "b@example.com");
    store.set_account_color(a, 5).unwrap();
    account(&store, "c@example.com");
    let colors: Vec<_> = store.accounts().unwrap().iter().map(|a| a.color).collect();
    assert_eq!(colors, [5, 1, 0]);
}

#[test]
fn keeps_who_a_message_went_to() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    let sent = NewMessage {
        answer_to: vec!["list@example.com".into()],
        recipients: vec!["a@example.com".into(), "bob@example.com".into()],
        ..message(1, 100)
    };
    store.save_messages(folder, &[sent]).unwrap();
    let id = list(&store, ListScope::Inbox(Category::People), 1, None)[0].id;
    assert_eq!(
        store.correspondents(id).unwrap(),
        Correspondents {
            from: "anna@example.com".into(),
            answer_to: vec!["list@example.com".into()],
            recipients: vec!["a@example.com".into(), "bob@example.com".into()],
        }
    );
}

#[test]
fn a_taken_back_answer_keeps_its_quote() {
    let store = store();
    let a = account(&store, "a@example.com");
    let draft = Draft {
        quote: "Bob wrote:\n> Hi".into(),
        ..draft(a)
    };
    let id = store.queue_send(&draft, None, "1@x", 0, 100).unwrap();
    assert_eq!(
        store.due_sends(a, 100).unwrap()[0].text,
        "Hello\n\nBob wrote:\n> Hi"
    );
    assert_eq!(taken_back(&store, id, 50), Some(saved(&draft, 50)));
}

#[test]
fn shows_images_from_people_and_chosen_senders() {
    let store = store();
    let folder = inbox(&store, account(&store, "a@example.com"));
    let news = NewMessage {
        category: Category::Newsletters,
        ..message(2, 200)
    };
    store
        .save_messages(folder, &[message(1, 100), news])
        .unwrap();
    let id = |tab| list(&store, ListScope::Inbox(tab), 1, None)[0].id;
    let (person, letter) = (id(Category::People), id(Category::Newsletters));
    assert!(store.shows_images(person).unwrap());
    assert!(!store.shows_images(letter).unwrap());
    store.allow_images(letter).unwrap();
    store.allow_images(letter).unwrap();
    assert!(store.shows_images(letter).unwrap());
    store
        .conn()
        .execute_batch("UPDATE folder SET role = 'junk'")
        .unwrap();
    assert!(!store.shows_images(person).unwrap());
    assert!(!store.shows_images(letter).unwrap());
    assert_eq!(
        store.allow_images(MessageId(999)),
        Err(StoreError::NotFound)
    );
}

#[test]
fn a_draft_is_kept_until_it_is_sent_or_thrown_away() {
    let store = store();
    let a = account(&store, "a@example.com");
    let mut draft = SavedDraft {
        id: None,
        account_id: Some(a),
        to: "bob@exam".into(),
        subject: String::new(),
        text: "Hel".into(),
        reply_to: None,
        quote: String::new(),
        forward: None,
        saved: 100,
    };
    let id = store.save_draft(&draft).unwrap();
    draft.id = Some(id);
    draft.text = "Hello".into();
    draft.saved = 200;
    assert_eq!(store.save_draft(&draft).unwrap(), id);
    let other = store
        .save_draft(&SavedDraft {
            id: None,
            saved: 150,
            ..draft.clone()
        })
        .unwrap();
    assert_eq!(
        store
            .drafts()
            .unwrap()
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        [Some(id), Some(other)],
        "last saved first"
    );
    assert_eq!(store.drafts().unwrap()[0], draft);

    store.delete_draft(other).unwrap();
    store.delete_draft(other).unwrap();
    store
        .queue_send(&self::draft(a), Some(id), "1@x", 0, 100)
        .unwrap();
    assert!(store.drafts().unwrap().is_empty());
    // A save still on its way does not bring it back.
    assert_eq!(store.save_draft(&draft), Err(StoreError::NotFound));
    assert!(store.drafts().unwrap().is_empty());
}

#[test]
fn a_draft_outlives_its_answered_message_but_not_its_account() {
    let store = store();
    let a = account(&store, "a@example.com");
    let draft = SavedDraft {
        id: None,
        account_id: Some(a),
        to: String::new(),
        subject: "Re: Hi".into(),
        text: "Hello".into(),
        reply_to: Some(MessageId(999)),
        quote: String::new(),
        forward: Some(MessageId(999)),
        saved: 100,
    };
    store.save_draft(&draft).unwrap();
    assert_eq!(store.drafts().unwrap()[0].reply_to, None);
    assert_eq!(store.drafts().unwrap()[0].forward, None);
    let gone = SavedDraft {
        account_id: Some(AccountId(999)),
        ..draft.clone()
    };
    store.save_draft(&gone).unwrap();
    assert_eq!(store.drafts().unwrap().len(), 2);
    store.remove_account(a).unwrap();
    let left = store.drafts().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].account_id, None);
}
