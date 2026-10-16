use ozero_imap::rustls::RootCertStore;
use ozero_imap::rustls::pki_types::CertificateDer;
use ozero_imap::rustls::pki_types::pem::PemObject;

use super::*;

/// A database path whose files are removed when the test ends.
struct TempDb(String);

impl TempDb {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("ozero-api-{}-{name}.db", std::process::id()));
        Self(path.display().to_string())
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0));
        }
    }
}

#[test]
fn version_is_set() {
    assert!(!core_version().is_empty());
}

#[test]
fn html_is_wrapped_in_a_locked_page() {
    let body = MessageBody::from(Body {
        text: "Hi".into(),
        html: Some("<p>Hi</p>".into()),
    });
    let page = body.html.unwrap();
    assert!(page.starts_with("<!doctype html>"));
    assert!(page.contains("default-src 'none'"));
    assert!(page.ends_with("<p>Hi</p></body></html>"));
    // A policy covers only what follows it, so it must come before the message.
    let policy = page.find("Content-Security-Policy").unwrap();
    assert!(policy < page.find("<p>Hi</p>").unwrap());
}

#[test]
fn rejects_a_short_key() {
    assert_eq!(
        Core::open(TempDb::new("short-key").0.clone(), vec![1; 16]).unwrap_err(),
        CoreError::WrongKey
    );
}

#[test]
#[ignore = "needs tools/dovecot-test.sh"]
fn syncs_an_imap_inbox_into_the_list() {
    let db = TempDb::new("sync");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();

    let mut roots = RootCertStore::empty();
    let ca = include_bytes!("../../adapters/imap/tests/dovecot/ca.crt");
    roots
        .add(CertificateDer::from_pem_slice(ca).unwrap())
        .unwrap();
    let tls = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    assert!(core.tls.set(Arc::new(tls)).is_ok());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let add = |password: &str| {
        runtime.block_on(core.add_imap_account(
            "test@example.com".into(),
            "Test".into(),
            "localhost".into(),
            31993,
            password.into(),
        ))
    };
    assert_eq!(add("wrong").unwrap_err(), CoreError::Auth);
    assert!(core.accounts().unwrap().is_empty());
    let account = add("pass").unwrap();
    assert_eq!(add("pass").unwrap_err(), CoreError::AccountExists);

    let sync = || runtime.block_on(core.sync_imap(account, "pass".into()));
    sync().unwrap();
    // A second sync updates the same rows.
    sync().unwrap();

    let inbox = core.unified_inbox(Tab::People, 50).unwrap();
    let subjects: Vec<_> = inbox.iter().map(|m| m.subject.as_str()).collect();
    assert_eq!(subjects, ["Message 3", "Message 2", "Message 1"]);
    assert_eq!(inbox[0].from_name.as_deref(), Some("Анна"));
    assert!(inbox[0].seen && inbox[0].flagged);
    assert_eq!(inbox[0].account_id, account);

    let message = inbox[1].id;
    let body = |password: &str| runtime.block_on(core.message_body(message, password.into()));
    let plain = MessageBody {
        text: "Hello 2".into(),
        html: None,
    };
    assert_eq!(body("pass").unwrap(), plain);
    // Saved now: no login, so a wrong password does not matter.
    assert_eq!(body("wrong").unwrap(), plain);
    assert_eq!(
        core.unified_inbox(Tab::People, 50).unwrap()[1].snippet,
        "Hello 2"
    );

    // Read on the device at once, on the server after the push; then unread again.
    let first = inbox[2].id;
    for seen in [true, false] {
        core.mark_read(first, seen).unwrap();
        assert_eq!(core.unified_inbox(Tab::People, 50).unwrap()[2].seen, seen);
        runtime
            .block_on(core.push_actions(account, "pass".into()))
            .unwrap();
        sync().unwrap();
        assert_eq!(core.unified_inbox(Tab::People, 50).unwrap()[2].seen, seen);
    }
}
