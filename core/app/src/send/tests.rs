use ozero_domain::OutboxId;

use super::*;
use crate::fakes::{Builder, Memory, Postman, Server, run};

fn outgoing(id: i64, to: &str) -> Outgoing {
    Outgoing {
        id: OutboxId(id),
        to: vec![to.into()],
        cc: vec![],
        subject: format!("Message {id}"),
        text: String::new(),
        message_id: format!("{id}@example.com"),
        in_reply_to: None,
        date: 100,
        forward: None,
        files: vec![],
    }
}

fn anna() -> Address {
    Address {
        name: None,
        email: "anna@example.com".into(),
    }
}

#[test]
fn sends_and_keeps_a_copy_in_sent() {
    let store = Memory::default();
    let (mut postman, mut server) = (Postman::default(), Server::default());
    let copy = SentCopy {
        provider: &mut server,
        folder: "Sent",
    };
    let messages = vec![
        outgoing(1, "bob@example.org"),
        Outgoing {
            cc: vec!["dan@example.com".into()],
            ..outgoing(2, "cat@example.net")
        },
    ];
    run(push_sends(
        &store,
        &mut postman,
        &Builder,
        &anna(),
        Some(copy),
        messages,
    ))
    .unwrap();

    assert_eq!(
        postman.sent,
        [
            (
                "anna@example.com".into(),
                vec!["bob@example.org".to_string()],
                b"Message 1".to_vec()
            ),
            (
                "anna@example.com".into(),
                vec!["cat@example.net".to_string(), "dan@example.com".to_string()],
                b"Message 2".to_vec()
            ),
        ]
    );
    assert_eq!(*store.sent.lock().unwrap(), [OutboxId(1), OutboxId(2)]);
    assert_eq!(
        server.appended,
        [
            ("Sent".into(), b"Message 1".to_vec()),
            ("Sent".into(), b"Message 2".to_vec())
        ]
    );
}

#[test]
fn a_refusal_stays_and_a_network_failure_stops() {
    let store = Memory::default();
    let mut postman = Postman {
        refuse: vec![
            ProviderError::Rejected("550 no such user".into()),
            ProviderError::Network("reset".into()),
        ],
        ..Postman::default()
    };
    let messages = vec![
        outgoing(1, "nobody@example.org"),
        outgoing(2, "bob@example.org"),
        outgoing(3, "cat@example.net"),
    ];
    let result = run(push_sends::<Server>(
        &store,
        &mut postman,
        &Builder,
        &anna(),
        None,
        messages,
    ));

    assert_eq!(
        result,
        Err(SyncError::Provider(ProviderError::Network("reset".into())))
    );
    assert_eq!(
        *store.refused.lock().unwrap(),
        [(OutboxId(1), "550 no such user".to_string())]
    );
    // Message 2 failed on the network and 3 never went: both stay queued.
    assert!(store.sent.lock().unwrap().is_empty());
}

#[test]
fn forwards_no_files_of_a_message_gone_from_the_device() {
    let files = run(forwarded_files(
        &Memory::default(),
        &mut Server::default(),
        &crate::fakes::Parser,
        ozero_domain::AccountId(1),
        MessageId(1),
    ));
    assert_eq!(files, Ok(vec![]));
}
