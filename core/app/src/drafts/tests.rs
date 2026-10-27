use ozero_domain::{Action, DraftId, DraftToPush, MessageId, OutboxId, Outgoing, ServerDraft};

use super::*;
use crate::fakes::{Builder, Memory, Parser, Server, run};

fn to_push(id: i64, message_id: &str) -> DraftToPush {
    DraftToPush {
        id: DraftId(id),
        saved: id * 10,
        message: Outgoing {
            id: OutboxId(0),
            to: vec![],
            cc: vec![],
            subject: format!("Draft {id}"),
            text: String::new(),
            message_id: message_id.into(),
            in_reply_to: None,
            date: 100,
            forward: None,
            files: vec![],
        },
    }
}

fn copy(remote_id: &str, validity: u32) -> DraftCopy {
    DraftCopy {
        folder: "Drafts".into(),
        validity,
        remote_id: remote_id.into(),
    }
}

fn anna() -> Address {
    Address {
        name: None,
        email: "anna@example.com".into(),
    }
}

fn push(store: &Memory, server: &mut Server, drafts: Vec<DraftToPush>) -> Result<(), SyncError> {
    let to = DraftsFolder {
        account: AccountId(1),
        from: &anna(),
        folder: "Drafts",
        message_id: &|| "new@example.com".into(),
    };
    run(push_drafts(store, server, &Builder, to, drafts, 0))
}

#[test]
fn saves_each_draft_then_deletes_stale_copies() {
    let store = Memory {
        stale: vec![(5, copy("a", 7)), (6, copy("b", 6)), (8, copy("c", 7))],
        ..Memory::default()
    };
    let mut server = Server::default();
    let drafts = vec![to_push(1, ""), to_push(2, "2@example.com")];
    push(&store, &mut server, drafts).unwrap();

    assert_eq!(
        server.drafts,
        [
            (
                "Drafts".into(),
                b"Draft 1".to_vec(),
                "new@example.com".into()
            ),
            ("Drafts".into(), b"Draft 2".to_vec(), "2@example.com".into()),
        ]
    );
    assert_eq!(
        *store.pushed.lock().unwrap(),
        [
            (
                DraftId(1),
                10,
                "new@example.com".into(),
                Some(copy("d1", 7))
            ),
            (DraftId(2), 20, "2@example.com".into(), Some(copy("d2", 7))),
        ]
    );
    // One delete per folder generation.
    assert_eq!(
        server.applied,
        [
            ("Drafts".into(), 6, vec!["b".into()], Action::Delete),
            (
                "Drafts".into(),
                7,
                vec!["a".into(), "c".into()],
                Action::Delete
            ),
        ]
    );
    assert_eq!(*store.forgotten.lock().unwrap(), [6, 5, 8]);
}

#[test]
fn keeps_a_draft_the_server_refuses_on_the_device() {
    let store = Memory::default();
    let mut server = Server {
        refuse: vec![ProviderError::Rejected("no".into())],
        ..Server::default()
    };
    push(&store, &mut server, vec![to_push(1, "1@example.com")]).unwrap();
    assert_eq!(
        *store.pushed.lock().unwrap(),
        [(DraftId(1), 10, "1@example.com".into(), None)]
    );
}

#[test]
fn stops_when_the_network_fails_and_keeps_stale_copies() {
    let store = Memory {
        stale: vec![(5, copy("a", 7))],
        ..Memory::default()
    };
    let mut server = Server {
        refuse: vec![ProviderError::Network("down".into())],
        ..Server::default()
    };
    let drafts = vec![to_push(1, ""), to_push(2, "")];
    assert!(push(&store, &mut server, drafts).is_err());
    assert_eq!(server.drafts.len(), 1);
    assert!(store.pushed.lock().unwrap().is_empty());

    let mut server = Server {
        refuse: vec![ProviderError::Network("down".into())],
        ..Server::default()
    };
    assert!(push(&store, &mut server, vec![]).is_err());
    assert!(store.forgotten.lock().unwrap().is_empty());
}

#[test]
fn keeps_drafts_written_elsewhere() {
    let found = |remote_id: &str| ServerDraft {
        message: MessageId(1),
        copy: copy(remote_id, 7),
        message_id: Some(format!("{remote_id}@example.com")),
    };
    let store = Memory {
        server_drafts: vec![found("x"), found("gone"), found("y"), found("z")],
        ..Memory::default()
    };
    run(import_drafts(
        &store,
        &mut Server::default(),
        &Parser,
        AccountId(1),
        3,
    ))
    .unwrap();

    let imported = store.imported.lock().unwrap();
    let ids: Vec<&str> = imported.iter().map(|d| d.2.remote_id.as_str()).collect();
    assert_eq!(ids, ["x", "y"]);
    let (draft, message_id, _) = &imported[0];
    assert_eq!(draft.account_id, Some(AccountId(1)));
    assert_eq!(draft.text, "Hello");
    assert_eq!(draft.subject, "Hello");
    assert_eq!(message_id.as_deref(), Some("x@example.com"));
}
