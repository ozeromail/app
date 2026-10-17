use ozero_imap::rustls::RootCertStore;
use ozero_imap::rustls::pki_types::CertificateDer;
use ozero_imap::rustls::pki_types::pem::PemObject;

use ozero_domain::{Checkpoint, MailProvider};

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
fn holds_back_remote_images_and_counts_them() {
    let body = MessageBody::from(Body {
        text: String::new(),
        html: Some("<img src=https://x.com/a.png><img src=https://x.com/p.gif width=1>".into()),
    });
    assert_eq!(body.remote_images, 1);
    assert!(!body.html.unwrap().contains("x.com"));
}

#[test]
fn makes_data_addresses_only_for_images() {
    assert_eq!(
        data_uri("Image/PNG; charset=x", b"ab").as_deref(),
        Some("data:image/png;base64,YWI=")
    );
    assert_eq!(data_uri("text/html", b"ab"), None);
    assert_eq!(data_uri("image/png\"><script>", b"ab"), None);
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
            Endpoint {
                host: "localhost".into(),
                port: 31993,
            },
            Endpoint {
                host: "localhost".into(),
                port: 31587,
            },
            password.into(),
        ))
    };
    assert_eq!(add("wrong").unwrap_err(), CoreError::Auth);
    assert!(core.accounts().unwrap().is_empty());
    let account = add("pass").unwrap();
    assert_eq!(add("pass").unwrap_err(), CoreError::AccountExists);

    let sync = || runtime.block_on(core.sync(account, "pass".into()));
    sync().unwrap();
    // A second sync updates the same rows.
    sync().unwrap();

    let inbox = core.unified_inbox(Category::People, 50).unwrap();
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
        remote_images: 0,
        shows_images: true,
    };
    assert_eq!(body("pass").unwrap(), plain);
    // Saved now: no login, so a wrong password does not matter.
    assert_eq!(body("wrong").unwrap(), plain);
    assert_eq!(
        core.unified_inbox(Category::People, 50).unwrap()[1].snippet,
        "Hello 2"
    );

    // Read on the device at once, on the server after the push; then unread again.
    let first = inbox[2].id;
    for seen in [true, false] {
        core.mark_read(first, seen).unwrap();
        assert_eq!(
            core.unified_inbox(Category::People, 50).unwrap()[2].seen,
            seen
        );
        runtime
            .block_on(core.push_actions(account, "pass".into()))
            .unwrap();
        sync().unwrap();
        assert_eq!(
            core.unified_inbox(Category::People, 50).unwrap()[2].seen,
            seen
        );
    }

    // Undo gives the draft back; sent again, it reaches the SMTP server and Sent.
    let send = || {
        core.send(
            account,
            "bob@example.org".into(),
            "Ответ из Ozero".into(),
            "Привет!".into(),
            Some(first),
            "Anna wrote:\n> Hi".into(),
        )
        .unwrap()
    };
    let draft = core.undo_send(send()).unwrap().unwrap();
    assert_eq!(draft.to, "bob@example.org");
    assert_eq!(draft.reply_to, Some(first));
    assert_eq!(draft.quote, "Anna wrote:\n> Hi");
    send();
    runtime
        .block_on(core.push_actions(account, "pass".into()))
        .unwrap();
    let received = mailpit_messages();
    assert!(received.contains("bob@example.org"), "{received}");
    assert!(received.contains("Ответ из Ozero"), "{received}");
    let sent = runtime.block_on(async {
        let account = core.account(AccountId(account))?;
        let mut imap = core.connect(&account, "pass").await?;
        Ok::<_, CoreError>(
            imap.changes("Sent", Checkpoint::default(), &[], 50)
                .await?
                .new,
        )
    });
    let headers: Vec<_> = sent
        .unwrap()
        .iter()
        .map(|m| String::from_utf8_lossy(&m.header).into_owned())
        .collect();
    assert!(
        headers.iter().any(|h| h.contains("Message-ID: <")),
        "{headers:?}"
    );
}

/// The message list of the Mailpit test server, as JSON.
fn mailpit_messages() -> String {
    use std::io::{Read, Write};
    let mut http = std::net::TcpStream::connect("localhost:31825").unwrap();
    http.write_all(b"GET /api/v1/messages HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    http.read_to_string(&mut response).unwrap();
    response
}

#[test]
fn rows_are_marked_only_with_several_accounts() {
    let db = TempDb::new("marks");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let add = |email: &str| {
        core.store
            .add_account(&NewAccount {
                kind: AccountKind::Imap,
                email: email.into(),
                display_name: email.into(),
                server: None,
                smtp: None,
            })
            .unwrap()
    };
    let row = |account| MessageRow {
        id: MessageId(1),
        account_id: account,
        from: Address {
            name: None,
            email: "anna@example.com".into(),
        },
        subject: "Hi".into(),
        snippet: String::new(),
        date: 0,
        flags: Default::default(),
        thread_count: 1,
        thread_unread: 0,
        sent: false,
    };
    let first = add("a@example.com");
    assert_eq!(core.items(vec![row(first)]).unwrap()[0].account, None);

    let second = add("b@example.com");
    let marks: Vec<_> = core
        .items(vec![row(first), row(second)])
        .unwrap()
        .into_iter()
        .map(|m| m.account.unwrap())
        .collect();
    assert_eq!(
        marks,
        [
            AccountMark {
                name: "a@example.com".into(),
                color: ACCOUNT_COLORS[0],
            },
            AccountMark {
                name: "b@example.com".into(),
                color: ACCOUNT_COLORS[1],
            },
        ]
    );
}

#[test]
fn saves_a_gmail_account_that_can_send() {
    let db = TempDb::new("gmail");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let id = core.add_gmail_account("me@gmail.com".into()).unwrap();
    assert_eq!(
        core.add_gmail_account("Me@Gmail.com".into()),
        Err(CoreError::AccountExists)
    );
    let account = core.account(AccountId(id)).unwrap();
    assert_eq!((account.kind, account.smtp), (AccountKind::Gmail, None));
    // Gmail sends over its API, so no SMTP server is needed.
    core.send(
        id,
        "you@example.com".into(),
        "Hi".into(),
        "Text".into(),
        None,
        String::new(),
    )
    .unwrap();
}

#[test]
fn starts_google_sign_in_at_google() {
    let start = google_sign_in().unwrap();
    assert!(start.url.starts_with("https://accounts.google.com/"));
    assert!(start.url.contains(&start.state));
    assert!(!start.url.contains(&start.verifier));
}
