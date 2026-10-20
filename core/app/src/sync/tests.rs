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
fn saves_inbox_and_sent_changes_since_the_checkpoint() {
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
    run(sync_mail(&store, &mut server, &Parser, AccountId(1), 50)).unwrap();

    let known: Vec<String> = vec!["5".into(), "6".into()];
    assert_eq!(
        server.synced,
        [
            ("INBOX".to_string(), since, known.clone(), 50),
            ("Sent".to_string(), since, known, 12),
        ]
    );
    let changes = store.changes.lock().unwrap();
    let folders: Vec<_> = changes.iter().map(|(f, _)| *f).collect();
    assert_eq!(folders, [FolderId(2), FolderId(1)]);
    let saved = &changes[0].1;
    assert_eq!(saved.checkpoint.next, 8);
    assert_eq!(saved.new[0].subject, "Hi");
    assert_eq!(saved.gone, ["6"]);
    assert_eq!(saved.flags.len(), 1);
}

#[test]
fn syncs_the_folders_asked_for() {
    let store = Memory::default();
    let mut server = Server::default();
    let archive = Folder {
        id: FolderId(4),
        account_id: AccountId(1),
        remote_id: "Archive".into(),
        name: "Archive".into(),
        role: FolderRole::Archive,
    };
    run(sync_folders(&store, &mut server, &Parser, &[archive], 50)).unwrap();

    let synced: Vec<_> = server.synced.iter().map(|s| (s.0.as_str(), s.3)).collect();
    assert_eq!(synced, [("Archive", 50)]);
    let changes = store.changes.lock().unwrap();
    assert_eq!(changes[0].0, FolderId(4));
}

#[test]
fn fails_without_an_inbox() {
    let mut server = Server {
        folders: vec![folder("Sent", FolderRole::Sent)],
        ..Server::default()
    };
    assert_eq!(
        run(sync_mail(
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
fn does_not_save_an_empty_download() {
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

#[test]
fn prefetches_bodies_past_gone_mail_until_one_fails() {
    let store = Memory {
        without_body: vec![
            location("7"),
            location("empty"),
            location("gone"),
            location("fail"),
            location("8"),
        ],
        ..Memory::default()
    };
    let mut server = Server::default();
    let done = run(prefetch_bodies(
        &store,
        &mut server,
        &Parser,
        AccountId(1),
        5,
    ));
    assert_eq!(
        done,
        Err(SyncError::Provider(ProviderError::Network("down".into())))
    );
    let asked: Vec<_> = server.asked.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(
        asked,
        ["INBOX/7", "INBOX/empty", "INBOX/gone", "INBOX/fail"]
    );
    // It stops at the failure, and saves only what came down.
    assert_eq!(store.bodies.lock().unwrap().len(), 1);
}

#[test]
fn waits_on_the_inbox_from_its_checkpoint() {
    let since = Checkpoint {
        validity: 9,
        next: 7,
    };
    let saved = |id, remote_id: &str, role| Folder {
        id: FolderId(id),
        account_id: AccountId(1),
        remote_id: remote_id.into(),
        name: remote_id.into(),
        role,
    };
    let store = Memory {
        folders: vec![
            saved(1, "Sent", FolderRole::Sent),
            saved(2, "INBOX", FolderRole::Inbox),
        ],
        checkpoint: (since, vec!["5".into()]),
        ..Memory::default()
    };
    let mut server = Server::default();
    let wait = Duration::from_secs(60);
    assert!(run(wait_for_mail(&store, &mut server, AccountId(1), wait)).unwrap());
    assert_eq!(server.waited, [("INBOX".to_string(), since, wait)]);
}

#[test]
fn does_not_wait_without_an_inbox() {
    let mut server = Server::default();
    let wait = Duration::from_secs(60);
    assert_eq!(
        run(wait_for_mail(
            &Memory::default(),
            &mut server,
            AccountId(1),
            wait
        )),
        Err(SyncError::NoInbox)
    );
    assert!(server.waited.is_empty());
}
