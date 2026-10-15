//! Runs against a real Dovecot started by `tools/dovecot-test.sh`.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use ozero_domain::{FolderRole, MailProvider, ProviderError};
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
            ("Empty", "Empty", FolderRole::Custom),
            ("INBOX", "INBOX", FolderRole::Inbox),
            ("Sent", "Sent", FolderRole::Sent),
        ]
    );
    imap.logout().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn fetches_the_newest_messages_oldest_first() {
    let mut imap = session().await;
    let recent = imap.recent("INBOX", 2).await.unwrap();
    assert!(recent.validity > 0);
    let messages = recent.messages;
    let uids: Vec<_> = messages.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(uids, ["2", "3"]);

    let last = &messages[1];
    assert!(last.flags.seen && last.flags.flagged);
    assert!(!messages[0].flags.seen);
    assert!(last.received > 1_700_000_000);
    let header = String::from_utf8_lossy(&last.header);
    assert!(header.contains("Subject: Message 3"), "{header}");
    assert!(!header.contains("Hello"), "body must not be fetched");

    assert!(imap.recent("Empty", 50).await.unwrap().messages.is_empty());
    assert_eq!(imap.recent("INBOX", 50).await.unwrap().messages.len(), 3);
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "needs tools/dovecot-test.sh"]
async fn fetches_a_body_without_marking_it_read() {
    let mut imap = session().await;
    let validity = imap.recent("INBOX", 1).await.unwrap().validity;
    let body = imap.body("INBOX", validity, "2").await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("Subject: Message 2") && text.contains("Hello 2"),
        "{text}"
    );

    let second = &imap.recent("INBOX", 2).await.unwrap().messages[0];
    assert_eq!(second.remote_id, "2");
    assert!(!second.flags.seen);

    for (uid, validity) in [("99", validity), ("2", validity + 1)] {
        let err = imap.body("INBOX", validity, uid).await.unwrap_err();
        assert_eq!(err, ProviderError::Gone, "{uid}");
    }
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
