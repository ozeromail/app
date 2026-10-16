//! Runs against a real Dovecot started by `tools/dovecot-test.sh`.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use ozero_domain::{Changes, Checkpoint, FolderRole, MailProvider, ProviderError};
use ozero_imap::ImapSession;
use tokio_rustls::rustls::pki_types::CertificateDer;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};

const PORT: u16 = 31993;
const USER: &str = "test@example.com";
const PASSWORD: &str = "pass";

async fn connect(roots: RootCertStore, password: &str) -> Result<ImapSession, ProviderError> {
    let tls = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    ImapSession::connect("localhost", PORT, Arc::new(tls), USER, password).await
}

/// Trusts only the test CA that signed the server certificate.
fn test_ca() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(include_bytes!("dovecot/ca.crt")).unwrap())
        .unwrap();
    roots
}

async fn session() -> ImapSession {
    connect(test_ca(), PASSWORD).await.unwrap()
}

/// The newest `limit` messages of a folder, as a first sync gets them.
async fn recent(imap: &mut ImapSession, folder: &str, limit: u32) -> Changes {
    imap.changes(folder, Checkpoint::default(), &[], limit)
        .await
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn lists_folders_with_roles_and_names() {
    let mut imap = session().await;
    let mut folders = imap.folders().await.unwrap();
    folders.sort_by(|a, b| a.remote_id.cmp(&b.remote_id));
    let got: Vec<_> = folders
        .iter()
        .map(|f| (f.remote_id.as_str(), f.name.as_str(), f.role))
        .collect();
    assert_eq!(
        got,
        [
            ("&BB4EQgRHBFEEQgRL-", "Отчёты", FolderRole::Custom),
            ("Archive", "Archive", FolderRole::Archive),
            ("Changes", "Changes", FolderRole::Custom),
            ("Empty", "Empty", FolderRole::Custom),
            ("Gone", "Gone", FolderRole::Custom),
            ("INBOX", "INBOX", FolderRole::Inbox),
            ("Moves", "Moves", FolderRole::Custom),
            ("Sent", "Sent", FolderRole::Sent),
        ]
    );
    imap.logout().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn fetches_the_newest_messages_oldest_first() {
    let mut imap = session().await;
    let newest = recent(&mut imap, "INBOX", 2).await;
    assert!(newest.checkpoint.validity > 0);
    let messages = newest.new;
    let uids: Vec<_> = messages.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(uids, ["2", "3"]);

    let last = &messages[1];
    assert!(last.flags.seen && last.flags.flagged);
    assert!(!messages[0].flags.seen);
    assert!(last.received > 1_700_000_000);
    let header = String::from_utf8_lossy(&last.header);
    assert!(header.contains("Subject: Message 3"), "{header}");
    assert!(!header.contains("Hello"), "body must not be fetched");

    assert!(recent(&mut imap, "Empty", 50).await.new.is_empty());
    assert_eq!(recent(&mut imap, "INBOX", 50).await.new.len(), 3);
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn fetches_a_body_without_marking_it_read() {
    let mut imap = session().await;
    let validity = recent(&mut imap, "INBOX", 1).await.checkpoint.validity;
    let body = imap.body("INBOX", validity, "2").await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("Subject: Message 2") && text.contains("Hello 2"),
        "{text}"
    );

    let second = &recent(&mut imap, "INBOX", 2).await.new[0];
    assert_eq!(second.remote_id, "2");
    assert!(!second.flags.seen);

    for (uid, validity) in [("99", validity), ("2", validity + 1)] {
        let err = imap.body("INBOX", validity, uid).await.unwrap_err();
        assert_eq!(err, ProviderError::Gone, "{uid}");
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn marks_messages_read_and_unread() {
    let mut imap = session().await;
    let validity = recent(&mut imap, "INBOX", 1).await.checkpoint.validity;
    // UID 1 only, so other tests see the flags they expect; 99 does not exist.
    let ids = ["1".to_string(), "99".to_string()];
    for seen in [true, false] {
        imap.set_seen("INBOX", validity, &ids, seen).await.unwrap();
        let first = &recent(&mut imap, "INBOX", 3).await.new[0];
        assert_eq!((first.remote_id.as_str(), first.flags.seen), ("1", seen));
    }
    let err = imap
        .set_seen("INBOX", validity + 1, &ids, true)
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Gone);
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn moves_messages_to_another_folder() {
    // Its own folders, so other tests keep their messages.
    let mut imap = session().await;
    let validity = recent(&mut imap, "Moves", 1).await.checkpoint.validity;
    imap.move_to("Moves", validity, &["1".into()], "Archive")
        .await
        .unwrap();
    assert!(recent(&mut imap, "Moves", 10).await.new.is_empty());
    let archived = recent(&mut imap, "Archive", 10).await.new;
    assert_eq!(archived.len(), 1);
    let header = String::from_utf8_lossy(&archived[0].header);
    assert!(header.contains("Subject: Move me"), "{header}");
    // The inbox tabs need the list headers.
    assert!(header.contains("List-Unsubscribe:"), "{header}");
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn fetches_only_what_changed() {
    // Its own folders, so other tests keep their messages.
    let mut imap = session().await;
    let first = imap
        .changes("Changes", Checkpoint::default(), &[], 50)
        .await
        .unwrap();
    let uids: Vec<_> = first.new.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(uids, ["1", "2"]);
    assert_eq!(first.checkpoint.next, 3);
    let known = ["1".to_string(), "2".to_string()];

    let same = imap
        .changes("Changes", first.checkpoint, &known, 50)
        .await
        .unwrap();
    assert!(same.new.is_empty() && same.gone.is_empty());
    assert_eq!(same.flags.len(), 2);

    let validity = first.checkpoint.validity;
    imap.set_seen("Changes", validity, &["1".into()], true)
        .await
        .unwrap();
    imap.move_to("Changes", validity, &["2".into()], "Gone")
        .await
        .unwrap();
    imap.append(
        "Changes",
        b"From: anna@example.com\r\nSubject: Change 3\r\n\r\nHello\r\n",
    )
    .await
    .unwrap();
    let changed = imap
        .changes("Changes", first.checkpoint, &known, 50)
        .await
        .unwrap();
    let uids: Vec<_> = changed.new.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(uids, ["3"]);
    assert!(String::from_utf8_lossy(&changed.new[0].header).contains("Subject: Change 3"));
    assert_eq!(changed.gone, ["2"]);
    assert_eq!(changed.flags.len(), 1);
    assert_eq!(changed.flags[0].0, "1");
    assert!(changed.flags[0].1.seen);
    assert_eq!(changed.checkpoint.next, 4);

    // Mail saved before checkpoints existed tells what is new.
    let upgraded = Checkpoint { validity, next: 0 };
    let after = imap
        .changes("Changes", upgraded, &["1".into()], 50)
        .await
        .unwrap();
    let uids: Vec<_> = after.new.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(uids, ["3"]);
    assert_eq!((after.flags.len(), after.checkpoint.next), (1, 4));

    // Another validity means nothing known is trusted: the newest messages come again.
    let stale = Checkpoint {
        validity: validity + 1,
        next: 4,
    };
    let reset = imap.changes("Changes", stale, &known, 1).await.unwrap();
    let uids: Vec<_> = reset.new.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(uids, ["3"]);
    assert!(reset.gone.is_empty() && reset.flags.is_empty());
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn wrong_password_is_an_auth_error() {
    let err = connect(test_ca(), "wrong").await.unwrap_err();
    assert_eq!(err, ProviderError::Auth);
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn rejects_an_untrusted_certificate() {
    let err = connect(RootCertStore::empty(), PASSWORD).await.unwrap_err();
    assert!(matches!(err, ProviderError::Network(_)), "{err:?}");
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn appends_a_sent_copy_marked_read() {
    let mut imap = session().await;
    imap.append(
        "Sent",
        b"From: anna@example.com\r\nSubject: Sent copy\r\n\r\nHello\r\n",
    )
    .await
    .unwrap();
    // The api test also sends into Sent.
    let sent = recent(&mut imap, "Sent", 10).await.new;
    let copy = sent
        .iter()
        .find(|m| String::from_utf8_lossy(&m.header).contains("Subject: Sent copy"))
        .unwrap();
    assert!(copy.flags.seen);
}
