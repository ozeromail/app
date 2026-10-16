use ozero_domain::{Checkpoint, FolderId, MessageId, NewFolder};

use super::*;
use crate::fakes::{Memory, Parser, Server, run};

fn folder(remote_id: &str, role: FolderRole) -> NewFolder {
    NewFolder {
        remote_id: remote_id.into(),
        name: remote_id.into(),
        role,
    }
}

#[test]
fn saves_inbox_changes_since_the_checkpoint() {
    let since = Checkpoint {
        validity: 9,
        next: 7,
    };
    let store = Memory {
        checkpoint: (since, vec!["5".into(), "6".into()]),
        ..Memory::default()
    };
    let mut server = Server {
        folders: vec![
            folder("Sent", FolderRole::Sent),
            folder("INBOX", FolderRole::Inbox),
        ],
        ..Server::default()
    };
    run(sync_inbox(&store, &mut server, &Parser, AccountId(1), 50)).unwrap();

    assert_eq!(
        server.synced,
        [("INBOX".to_string(), since, vec!["5".into(), "6".into()], 50)]
    );
    let changes = store.changes.lock().unwrap();
    let (folder, saved) = &changes[0];
    assert_eq!(*folder, FolderId(2));
    assert_eq!(saved.checkpoint.next, 8);
    assert_eq!(saved.new[0].subject, "Hi");
    assert_eq!(saved.gone, ["6"]);
    assert_eq!(saved.flags.len(), 1);
}

#[test]
fn fails_without_an_inbox() {
    let mut server = Server {
        folders: vec![folder("Sent", FolderRole::Sent)],
        ..Server::default()
    };
    assert_eq!(
        run(sync_inbox(
            &Memory::default(),
            &mut server,
            &Parser,
            AccountId(1),
            50
        )),
        Err(SyncError::NoInbox)
    );
    assert!(server.synced.is_empty());
}

fn location(remote_id: &str) -> MessageLocation {
    MessageLocation {
        id: MessageId(4),
        account_id: AccountId(1),
        folder_remote_id: "INBOX".into(),
        validity: 9,
        remote_id: remote_id.into(),
    }
}

#[test]
fn downloads_a_body() {
    let store = Memory::default();
    let mut server = Server::default();
    let body = run(download_body(&store, &mut server, &Parser, &location("7"))).unwrap();
    assert_eq!(body.text, "Hello");

    assert_eq!(server.asked, [("INBOX/7".to_string(), 9)]);
    let bodies = store.bodies.lock().unwrap();
    assert_eq!(bodies[0].0, MessageId(4));
    assert_eq!(bodies[0].1.text, "Hello");
}

#[test]
fn does_not_save_a_body_without_text() {
    let store = Memory::default();
    let body = run(download_body(
        &store,
        &mut Server::default(),
        &Parser,
        &location("empty"),
    ))
    .unwrap();
    assert_eq!(body.text, "");
    assert!(store.bodies.lock().unwrap().is_empty());
}
