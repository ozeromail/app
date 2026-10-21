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
        ..Body::default()
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
fn passes_attachments_on() {
    let body = MessageBody::from(Body {
        text: "See the file".into(),
        attachments: vec![Attachment {
            part: 3,
            name: "a.pdf".into(),
            mime: "application/pdf".into(),
            size: 10,
        }],
        ..Body::default()
    });
    assert_eq!(
        body.attachments,
        [AttachmentItem {
            part: 3,
            name: "a.pdf".into(),
            mime: "application/pdf".into(),
            size: 10,
        }]
    );
}

#[test]
fn holds_back_remote_images_and_counts_them() {
    let body = MessageBody::from(Body {
        text: String::new(),
        html: Some("<img src=https://x.com/a.png><img src=https://x.com/p.gif width=1>".into()),
        ..Body::default()
    });
    assert_eq!(body.remote_images, 1);
    assert!(!body.html.unwrap().contains("x.com"));
}

#[test]
fn keeps_only_images() {
    let image = RemoteImage::new("Image/PNG; charset=x", b"ab".to_vec()).unwrap();
    assert_eq!(image.mime, "image/png");
    assert_eq!(RemoteImage::new("text/html", b"ab".to_vec()), None);
    assert_eq!(
        RemoteImage::new("image/png\"><script>", b"ab".to_vec()),
        None
    );
}

#[test]
fn lets_the_page_load_its_images() {
    let host = IMAGE_ADDRESS.trim_end_matches('/');
    assert!(sandboxed("", false).contains(&format!("img-src data: {host};")));
}

#[test]
fn lets_a_plain_page_load_the_reading_font() {
    let page = sandboxed("", false);
    assert!(page.contains(concat!("font-src data: https://", font_host!(), ";")));
    assert!(page.contains(concat!("url(https://", font_host!(), "/literata.ttf)")));
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
    // The first sync brought the whole inbox.
    let older = runtime.block_on(core.older_mail(account, "pass".into(), INBOX));
    assert_eq!(older, Ok(false));

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
        quote: String::new(),
        styled: false,
        html: None,
        remote_images: 0,
        shows_images: true,
        attachments: vec![],
    };
    assert_eq!(body("pass").unwrap(), plain);
    // Saved now: no login, so a wrong password does not matter.
    assert_eq!(body("wrong").unwrap(), plain);
    assert_eq!(
        core.unified_inbox(Category::People, 50).unwrap()[1].snippet,
        "Hello 2"
    );

    // A forward carries the saved text under its header, to no one yet.
    let labels = ForwardLabels {
        title: "Forwarded".into(),
        from: "From:".into(),
        date: "Date:".into(),
        subject: "Subject:".into(),
    };
    let forward = runtime
        .block_on(core.forward(inbox[1].clone(), labels, "today".into(), "wrong".into()))
        .unwrap();
    assert_eq!(forward.subject, "Fwd: Message 2");
    assert!(forward.to.is_empty() && forward.reply_to.is_none());
    assert!(
        forward.quote.starts_with("Forwarded\nFrom: "),
        "{}",
        forward.quote
    );
    assert!(
        forward
            .quote
            .contains("\nDate: today\nSubject: Message 2\n")
    );
    assert!(forward.quote.ends_with("\n\nHello 2"), "{}", forward.quote);

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
        core.send(DraftItem {
            id: 0,
            account_id: account,
            to: "bob@example.org".into(),
            subject: "Ответ из Ozero".into(),
            text: "Привет!".into(),
            reply_to: Some(first),
            quote: "Anna wrote:\n> Hi".into(),
            saved: 0,
        })
        .unwrap()
    };
    let draft = core.undo_send(send()).unwrap().unwrap();
    assert_eq!(draft.to, "bob@example.org");
    assert_eq!(draft.reply_to, Some(first));
    assert_eq!(draft.quote, "Anna wrote:\n> Hi");
    assert_eq!(core.drafts().unwrap(), std::slice::from_ref(&draft));
    core.send(draft).unwrap();
    assert!(core.drafts().unwrap().is_empty());
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
fn a_unified_list_ends_where_an_account_has_older_mail_to_load() {
    let db = TempDb::new("floor");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let inbox = |email: &str, dates: &[i64]| {
        let store = &core.store;
        let account = store
            .add_account(&NewAccount {
                kind: AccountKind::Imap,
                email: email.into(),
                display_name: email.into(),
                server: None,
                smtp: None,
            })
            .unwrap();
        let folder = NewFolder {
            remote_id: "INBOX".into(),
            name: "Inbox".into(),
            role: FolderRole::Inbox,
        };
        let folder = store.save_folders(account, &[folder]).unwrap()[0].id;
        let new = dates
            .iter()
            .map(|&date| ozero_domain::NewMessage {
                remote_id: format!("{email}{date}"),
                message_id: None,
                references: vec![],
                from: Address {
                    name: None,
                    email: "anna@example.com".into(),
                },
                answer_to: vec![],
                recipients: vec![],
                subject: date.to_string(),
                snippet: String::new(),
                date,
                flags: Default::default(),
                category: Category::People,
            })
            .collect();
        let checkpoint = store.checkpoint(folder).unwrap().0;
        let changes = Changes {
            checkpoint,
            new,
            flags: vec![],
            gone: vec![],
        };
        store.save_changes(folder, &changes).unwrap();
        folder
    };
    inbox("a@example.com", &[10, 20, 30]);
    let b = inbox("b@example.com", &[25, 35]);
    let dates = || -> Vec<i64> {
        let rows = core.unified_inbox(Category::People, 50).unwrap();
        rows.iter().map(|m| m.date).collect()
    };
    assert_eq!(dates(), [35, 30, 25]);
    // Once the other account has nothing older, the first one's mail shows all the way down.
    core.store.set_older_done(b, true).unwrap();
    assert_eq!(dates(), [35, 30, 25, 20, 10]);
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
        senders: Vec::new(),
        role: FolderRole::Inbox,
        attachments: vec!["a.pdf".into()],
    };
    let first = add("a@example.com");
    let item = &core.items(vec![row(first)]).unwrap()[0];
    assert_eq!(item.account, None);
    assert_eq!(item.attachments, ["a.pdf"]);

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
    core.send(DraftItem {
        id: 0,
        account_id: id,
        to: "you@example.com".into(),
        subject: "Hi".into(),
        text: "Text".into(),
        reply_to: None,
        quote: String::new(),
        saved: 0,
    })
    .unwrap();
}

#[test]
fn starts_google_sign_in_at_google() {
    let start = google_sign_in().unwrap();
    assert!(start.url.starts_with("https://accounts.google.com/"));
    assert!(start.url.contains(&start.state));
    assert!(!start.url.contains(&start.verifier));
}
