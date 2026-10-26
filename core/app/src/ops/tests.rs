use ozero_domain::{AccountId, Action, MessageId, MessageLocation, OpId};

use super::*;
use crate::fakes::{Memory, Server, run};

fn op(id: i64, folder: &str, uid: &str, action: Action) -> Op {
    Op {
        id: OpId(id),
        action,
        location: MessageLocation {
            id: MessageId(id),
            account_id: AccountId(1),
            folder_remote_id: folder.into(),
            validity: 9,
            remote_id: uid.into(),
        },
        to: (action == Action::Archive).then(|| "Archive".into()),
    }
}

fn store(ops: Vec<Op>) -> Memory {
    Memory {
        ops,
        ..Memory::default()
    }
}

#[test]
fn sends_one_request_per_folder_and_action() {
    let store = store(vec![
        op(1, "INBOX", "5", Action::Seen(true)),
        op(2, "Work", "8", Action::Seen(true)),
        op(3, "INBOX", "6", Action::Seen(true)),
        op(4, "INBOX", "7", Action::Seen(false)),
    ]);
    let mut server = Server::default();
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();

    let calls: Vec<_> = server
        .applied
        .iter()
        .map(|(f, v, ids, a)| (f.as_str(), *v, ids.join(","), *a))
        .collect();
    assert_eq!(
        calls,
        [
            ("INBOX", 9, "7".into(), Action::Seen(false)),
            ("INBOX", 9, "5,6".into(), Action::Seen(true)),
            ("Work", 9, "8".into(), Action::Seen(true)),
        ]
    );
    let mut finished = store.finished.lock().unwrap().clone();
    finished.sort_by_key(|o| o.0);
    assert_eq!(finished, [OpId(1), OpId(2), OpId(3), OpId(4)]);
}

#[test]
fn a_refusal_reverts_and_a_gone_folder_finishes() {
    let store = store(vec![
        op(1, "A", "5", Action::Seen(true)),
        op(2, "B", "6", Action::Seen(true)),
    ]);
    let mut server = Server {
        refuse: vec![ProviderError::Protocol("NO".into()), ProviderError::Gone],
        ..Server::default()
    };
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();
    assert_eq!(*store.reverted.lock().unwrap(), [OpId(1)]);
    assert_eq!(*store.finished.lock().unwrap(), [OpId(2)]);
}

#[test]
fn a_rejection_reverts_and_the_rest_still_goes() {
    let store = store(vec![
        op(1, "A", "5", Action::Seen(true)),
        op(2, "B", "6", Action::Seen(true)),
    ]);
    let mut server = Server {
        refuse: vec![ProviderError::Rejected("400 Bad Request".into())],
        ..Server::default()
    };
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();
    assert_eq!(*store.reverted.lock().unwrap(), [OpId(1)]);
    assert_eq!(*store.finished.lock().unwrap(), [OpId(2)]);
}

#[test]
fn a_network_failure_keeps_the_rest_queued() {
    let store = store(vec![
        op(1, "A", "5", Action::Seen(true)),
        op(2, "B", "6", Action::Seen(true)),
    ]);
    let mut server = Server {
        refuse: vec![ProviderError::Network("down".into())],
        ..Server::default()
    };
    let err = run(push_ops(&store, &mut server, store.ops.clone())).unwrap_err();
    assert_eq!(
        err,
        SyncError::Provider(ProviderError::Network("down".into()))
    );
    assert_eq!(server.applied.len(), 1);
    assert!(store.finished.lock().unwrap().is_empty());
    assert!(store.reverted.lock().unwrap().is_empty());
}

#[test]
fn archives_after_setting_read_state() {
    let store = store(vec![
        op(1, "INBOX", "5", Action::Archive),
        op(2, "INBOX", "5", Action::Seen(true)),
    ]);
    let mut server = Server::default();
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();
    let actions: Vec<_> = server.applied.iter().map(|a| a.3).collect();
    assert_eq!(actions, [Action::Seen(true), Action::Archive]);
    assert_eq!(server.moved_to, ["Archive"]);
    // The ids the server gave go to the store before the move is finished.
    let landed = store.landed.lock().unwrap();
    assert_eq!(landed[0].ids, [("5".to_owned(), "50".to_owned())]);
    assert_eq!(landed[0].validity, Some(9));
}

#[test]
fn snooze_moves_to_the_snoozed_folder() {
    let mut snooze = op(1, "INBOX", "5", Action::Snooze);
    snooze.to = Some("Snoozed".into());
    let store = store(vec![snooze]);
    let mut server = Server::default();
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();
    assert_eq!(server.moved_to, ["Snoozed"]);
    assert_eq!(*store.finished.lock().unwrap(), [OpId(1)]);
}

#[test]
fn trash_moves_to_its_own_folder() {
    let mut trash = op(1, "INBOX", "5", Action::Trash);
    trash.to = Some("Trash".into());
    let store = store(vec![trash, op(2, "INBOX", "6", Action::Archive)]);
    let mut server = Server::default();
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();
    let mut moved = server.moved_to.clone();
    moved.sort();
    assert_eq!(moved, ["Archive", "Trash"]);
    assert_eq!(store.finished.lock().unwrap().len(), 2);
}

#[test]
fn deletes_for_good() {
    let store = store(vec![op(1, "Trash", "5", Action::Delete)]);
    let mut server = Server::default();
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();
    assert_eq!(server.applied[0].3, Action::Delete);
    assert!(server.moved_to.is_empty());
    assert_eq!(store.finished.lock().unwrap().len(), 1);
}

#[test]
fn empties_first_and_keeps_what_the_folder_still_has() {
    let mut trash = op(1, "INBOX", "4", Action::Trash);
    trash.to = Some("Trash".into());
    let mut store = store(vec![trash, op(2, "Trash", "5", Action::Empty)]);
    store.checkpoint.1 = vec!["5".into(), "6".into()];
    let mut server = Server::default();
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();
    assert_eq!(server.applied[0].2, ["6"]);
    assert_eq!(server.applied[0].3, Action::Empty);
    assert_eq!(server.moved_to, ["Trash"]);
    assert_eq!(store.finished.lock().unwrap().len(), 2);
}

#[test]
fn archiving_without_an_archive_folder_is_reverted() {
    let mut no_folder = op(1, "INBOX", "5", Action::Archive);
    no_folder.to = None;
    let store = store(vec![no_folder]);
    let mut server = Server::default();
    run(push_ops(&store, &mut server, store.ops.clone())).unwrap();
    assert!(server.applied.is_empty());
    assert_eq!(*store.reverted.lock().unwrap(), [OpId(1)]);
}
