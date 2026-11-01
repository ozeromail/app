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
fn keeps_a_plain_page_clear_of_a_quoted_sheet() {
    let page = sandboxed("", false);
    assert!(page.contains("html,body{background:transparent!important}"));
    assert!(!sandboxed("", true).contains("@scope"));
}

#[test]
fn rejects_a_short_key() {
    assert_eq!(
        Core::open(TempDb::new("short-key").0.clone(), vec![1; 16]).unwrap_err(),
        CoreError::WrongKey
    );
}

#[test]
fn wakes_the_app_when_the_database_changes() {
    let db = TempDb::new("changed");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let waiting = runtime.block_on(async {
        let wait = core.changed(0);
        tokio::time::timeout(Duration::from_millis(50), wait).await
    });
    assert!(waiting.is_err());
    core.add_gmail_account("a@example.com".into(), None)
        .unwrap();
    let seen = runtime.block_on(core.changed(0));
    assert!(seen > 0);
    // Reading changes nothing.
    core.accounts().unwrap();
    let waiting = runtime.block_on(async {
        tokio::time::timeout(Duration::from_millis(50), core.changed(seen)).await
    });
    assert!(waiting.is_err());
}

#[test]
#[ignore = "needs tools/dovecot-test.sh"]
fn syncs_an_imap_inbox_into_the_list() {
    // A debug build's futures outgrow the test thread's 2 MB; the apps run a release core.
    let run = std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(sync_an_imap_inbox_into_the_list)
        .unwrap();
    if let Err(panic) = run.join() {
        std::panic::resume_unwind(panic);
    }
}

fn sync_an_imap_inbox_into_the_list() {
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

    // A refused password keeps the account and its mail and asks to sign in again.
    let needs_sign_in = || core.accounts().unwrap()[0].needs_sign_in;
    assert_eq!(
        runtime
            .block_on(core.sync(account, "wrong".into()))
            .unwrap_err(),
        CoreError::Auth
    );
    assert!(needs_sign_in());
    // Until then the server hears no more logins, not even a right one.
    assert_eq!(sync().unwrap_err(), CoreError::Auth);
    assert_eq!(
        runtime
            .block_on(core.check_password(account, "wrong".into()))
            .unwrap_err(),
        CoreError::Auth
    );
    runtime
        .block_on(core.check_password(account, "pass".into()))
        .unwrap();
    core.signed_in_again(account, None).unwrap();
    assert!(!needs_sign_in());
    sync().unwrap();

    let inbox = core.inbox(None, Category::People, 50).unwrap();
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
        core.inbox(None, Category::People, 50).unwrap()[1].snippet,
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
    assert_eq!(forward.forward, Some(inbox[1].id));
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
        core.mark_read(vec![first], seen).unwrap();
        assert_eq!(
            core.inbox(None, Category::People, 50).unwrap()[2].seen,
            seen
        );
        runtime
            .block_on(core.push_actions(account, "pass".into()))
            .unwrap();
        sync().unwrap();
        assert_eq!(
            core.inbox(None, Category::People, 50).unwrap()[2].seen,
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
            forward: None,
            files: vec![],
            signature: String::new(),
            cc: String::new(),
        })
        .unwrap()
    };
    let draft = core.undo_send(send()).unwrap().unwrap();
    assert_eq!(draft.to, "bob@example.org");
    assert_eq!(draft.reply_to, Some(first));
    assert_eq!(draft.quote, "Anna wrote:\n> Hi");
    assert_eq!(core.drafts().unwrap()[0].draft, draft);
    // A file added to the draft goes with it.
    core.attach_file(
        draft.id,
        "notes.txt".into(),
        "text/plain".into(),
        b"hi".to_vec(),
    )
    .unwrap();
    core.send(draft).unwrap();
    assert!(core.drafts().unwrap().is_empty());
    // A forward reads the message's files from the server as it goes.
    core.send(DraftItem {
        to: "cat@example.net".into(),
        ..forward
    })
    .unwrap();
    runtime
        .block_on(core.push_actions(account, "pass".into()))
        .unwrap();
    let received = mailpit_messages();
    assert!(received.contains("bob@example.org"), "{received}");
    assert!(received.contains("Ответ из Ozero"), "{received}");
    assert!(received.contains("\"Attachments\":1"), "{received}");
    assert!(received.contains("Fwd: Message 2"), "{received}");
    let sent = runtime.block_on(async {
        let account = core.account(AccountId(account))?;
        let mut imap = core.connect(&account, "pass").await?;
        Ok::<_, CoreError>(
            imap.changes("Sent", Checkpoint::default(), &[], i64::MAX, 50)
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

    // Trashed mail shows in the trash at once; after the push it is the server's message there,
    // under the same id.
    let filed = core.folders().unwrap();
    let filed = Mailbox::Folder {
        id: filed.iter().find(|f| f.name == "Filed").unwrap().id,
    };
    runtime
        .block_on(core.sync_mailbox(account, "pass".into(), filed))
        .unwrap();
    let trash = Mailbox::Unified {
        role: FolderRole::Trash,
    };
    let trashed = || core.mailbox(trash, 50).unwrap().first().map(|m| m.id);
    core.delete(vec![core.mailbox(filed, 50).unwrap()[0].id], vec![])
        .unwrap();
    assert!(core.mailbox(filed, 50).unwrap().is_empty());
    let copy = trashed().unwrap();
    runtime
        .block_on(core.push_actions(account, "pass".into()))
        .unwrap();
    assert_eq!(trashed(), Some(copy));
    let location = core.store.location(MessageId(copy)).unwrap();
    assert_eq!(location.folder_remote_id, "Trash");
    assert!(location.remote_id.parse::<u32>().is_ok(), "{location:?}");

    // The first sync made the snoozed folder. Snoozed mail waits there on the server, and
    // comes back on top, unread, once its time has come.
    let snoozed = Mailbox::Unified {
        role: FolderRole::Snoozed,
    };
    let newest = core.inbox(None, Category::People, 50).unwrap()[0].clone();
    core.mark_read(vec![newest.id], true).unwrap();
    core.snooze(vec![newest.id], now() / 1_000).unwrap();
    assert_ne!(
        core.inbox(None, Category::People, 50).unwrap()[0].id,
        newest.id
    );
    assert_eq!(core.mailbox(snoozed, 50).unwrap().len(), 1);
    runtime
        .block_on(core.push_actions(account, "pass".into()))
        .unwrap();
    sync().unwrap();
    let woke = core.wake().unwrap();
    assert_eq!(woke.len(), 1);
    assert!(!woke[0].seen);
    assert!(core.next_wake().unwrap().is_none());
    runtime
        .block_on(core.push_actions(account, "pass".into()))
        .unwrap();
    sync().unwrap();
    let back = &core.inbox(None, Category::People, 50).unwrap()[0];
    assert_eq!(
        (back.subject.as_str(), back.seen),
        (newest.subject.as_str(), false)
    );
    assert!(back.date >= woke[0].date);
    assert!(core.mailbox(snoozed, 50).unwrap().is_empty());

    // A draft is kept in Drafts on the server too; written again, only its new copy stays.
    let on_server = || {
        runtime.block_on(async {
            let account = core.account(AccountId(account)).unwrap();
            let mut imap = core.connect(&account, "pass").await.unwrap();
            let found = imap
                .changes("Drafts", Checkpoint::default(), &[], i64::MAX, 50)
                .await
                .unwrap()
                .new;
            (imap, found)
        })
    };
    let mut draft = DraftItem {
        id: 0,
        account_id: account,
        to: "dan@example.com".into(),
        subject: "Plan".into(),
        text: "First".into(),
        reply_to: None,
        quote: String::new(),
        saved: 0,
        forward: None,
        files: vec![],
        signature: "Test".into(),
        cc: String::new(),
    };
    for text in ["First", "Second"] {
        draft.text = text.into();
        draft.id = core.save_draft(draft.clone()).unwrap();
        runtime
            .block_on(core.push_actions(account, "pass".into()))
            .unwrap();
        let (_, found) = on_server();
        assert_eq!(found.len(), 1);
        let header = String::from_utf8_lossy(&found[0].header).into_owned();
        assert!(header.contains("Subject: Plan"), "{header}");
    }
    // One written elsewhere comes to the device with the next sync.
    let (mut imap, _) = on_server();
    runtime
        .block_on(imap.append(
            "Drafts",
            b"Subject: From the laptop\r\nTo: eve@example.com\r\nMessage-ID: <laptop@example.com>\r\n\r\nHi there\r\n",
        ))
        .unwrap();
    sync().unwrap();
    sync().unwrap();
    let drafts: Vec<_> = core
        .drafts()
        .unwrap()
        .into_iter()
        .map(|r| r.draft)
        .collect();
    let subjects: Vec<_> = drafts.iter().map(|d| d.subject.as_str()).collect();
    assert_eq!(subjects.len(), 2, "{subjects:?}");
    let laptop = drafts
        .iter()
        .find(|d| d.subject == "From the laptop")
        .unwrap();
    assert_eq!(laptop.text.trim_end(), "Hi there");
    assert_eq!(laptop.to, "eve@example.com");
    assert_eq!(laptop.account_id, account);
    // Sent, a draft leaves the server's Drafts.
    core.send(draft).unwrap();
    runtime
        .block_on(core.push_actions(account, "pass".into()))
        .unwrap();
    let (_, found) = on_server();
    let headers: Vec<_> = found
        .iter()
        .map(|m| String::from_utf8_lossy(&m.header).into_owned())
        .collect();
    assert_eq!(headers.len(), 1, "{headers:?}");
    assert!(headers[0].contains("From the laptop"));
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
                to: vec![],
                cc: vec![],
                subject: date.to_string(),
                snippet: String::new(),
                date,
                flags: Default::default(),
                category: Category::People,
                unsubscribe: None,
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
        let rows = core.inbox(None, Category::People, 50).unwrap();
        rows.iter().map(|m| m.date).collect()
    };
    assert_eq!(dates(), [35, 30, 25]);
    // Once the other account has nothing older, the first one's mail shows all the way down.
    core.store.set_older_done(b, true).unwrap();
    assert_eq!(dates(), [35, 30, 25, 20, 10]);
}

#[test]
fn tells_who_a_message_went_to_and_copies_the_others_on_reply_all() {
    let db = TempDb::new("people");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let store = &core.store;
    let account = store
        .add_account(&NewAccount {
            kind: AccountKind::Imap,
            email: "me@example.com".into(),
            display_name: "Me".into(),
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
    let named = |name: Option<&str>, email: &str| Address {
        name: name.map(str::to_owned),
        email: email.into(),
    };
    let message = ozero_domain::NewMessage {
        remote_id: "1".into(),
        message_id: None,
        references: vec![],
        from: named(Some("Anna"), "anna@example.com"),
        answer_to: vec![],
        to: vec![
            named(None, "ME@example.com"),
            named(Some("Bob"), "bob@example.com"),
        ],
        cc: vec![named(Some(" "), "cat@example.com")],
        subject: "Plans".into(),
        snippet: String::new(),
        date: 10,
        flags: Default::default(),
        category: Category::People,
        unsubscribe: None,
    };
    let checkpoint = store.checkpoint(folder).unwrap().0;
    let changes = Changes {
        checkpoint,
        new: vec![message],
        flags: vec![],
        gone: vec![],
    };
    store.save_changes(folder, &changes).unwrap();
    let item = core.inbox(None, Category::People, 10).unwrap().remove(0);

    let people = core.people(item.clone()).unwrap();
    let names = |p: &[PersonItem]| p.iter().map(|p| (p.name.clone(), p.me)).collect::<Vec<_>>();
    assert_eq!(
        names(&people.to),
        [("ME@example.com".into(), true), ("Bob".into(), false)]
    );
    assert_eq!(names(&people.cc), [("cat@example.com".into(), false)]);
    let firsts = people
        .shown
        .iter()
        .map(|p| p.first.as_str())
        .collect::<Vec<_>>();
    assert_eq!(firsts, ["ME", "Bob", "cat"]);
    assert_eq!(
        names(&people.others),
        [("Bob".into(), false), ("cat@example.com".into(), false)]
    );

    let one = core.reply(item.clone(), false).unwrap();
    assert_eq!((one.to.as_str(), one.cc.as_str()), ("anna@example.com", ""));
    let all = core.reply(item.clone(), true).unwrap();
    assert_eq!(all.to, "anna@example.com");
    assert_eq!(all.cc, "bob@example.com, cat@example.com");

    // A reply begun goes on, rather than starting a second draft.
    let id = core
        .save_draft(DraftItem {
            text: "Soon".into(),
            ..one
        })
        .unwrap();
    let again = core.reply(item.clone(), false).unwrap();
    assert_eq!(
        (again.id, again.text.as_str(), again.cc.as_str()),
        (id, "Soon", "")
    );
    // Reply all copies the others on it too.
    let copied = core.reply(item, true).unwrap();
    assert_eq!(
        (copied.id, copied.cc.as_str()),
        (id, "bob@example.com, cat@example.com")
    );

    // Its row names who it goes to as the saved mail does.
    let rows = core.drafts().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].people.as_str(), rows[0].snippet.as_str()),
        ("Anna", "Soon")
    );
    core.save_draft(DraftItem {
        to: "anna@example.com, BOB@example.com, new@example.com".into(),
        ..again
    })
    .unwrap();
    assert_eq!(core.drafts().unwrap()[0].people, "Anna, BOB, new");
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
        sending: false,
        can_unsubscribe: false,
        invite: None,
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
            // Named by the domain when added; the second has the same domain, so its address.
            AccountMark {
                name: "Example".into(),
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
fn keeps_files_with_a_draft_up_to_the_limit() {
    let db = TempDb::new("files");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let draft = DraftItem {
        id: 0,
        account_id: 0,
        to: "you@example.com".into(),
        subject: "Hi".into(),
        text: String::new(),
        reply_to: None,
        quote: String::new(),
        saved: 0,
        forward: None,
        files: vec![],
        signature: String::new(),
        cc: String::new(),
    };
    let id = core.save_draft(draft.clone()).unwrap();
    let attach = |name: &str, size: u64| {
        let data = vec![0; usize::try_from(size).unwrap()];
        core.attach_file(id, name.into(), "image/jpeg".into(), data)
    };
    let photo = attach("photo.jpg", FILE_LIMIT - 1).unwrap();
    assert_eq!(attach("more.jpg", 2), Err(CoreError::TooLarge));
    let small = attach("small.jpg", 1).unwrap();
    assert_eq!(
        core.drafts().unwrap()[0].draft.files,
        [photo, small.clone()]
    );
    core.detach_file(small.id).unwrap();
    assert_eq!(core.drafts().unwrap()[0].draft.files.len(), 1);

    // Thrown away, it comes back with its files and the text as it is now.
    core.delete_draft(id).unwrap();
    assert!(core.drafts().unwrap().is_empty());
    assert_eq!(attach("late.jpg", 1), Err(CoreError::Gone));
    let thrown = DraftItem {
        id,
        text: "Hello".into(),
        ..draft.clone()
    };
    let back = core.restore_draft(thrown).unwrap();
    assert_eq!((back.id, back.text.as_str()), (id, "Hello"));
    assert_eq!(core.drafts().unwrap()[0].draft.files.len(), 1);
    // One never saved, or gone for good, comes back new, without files.
    let fresh = core
        .restore_draft(DraftItem {
            files: vec![],
            ..draft
        })
        .unwrap();
    assert_ne!(fresh.id, 0);
    assert_eq!(core.drafts().unwrap().len(), 2);
}

#[test]
fn saves_a_gmail_account_that_can_send() {
    let db = TempDb::new("gmail");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let id = core
        .add_gmail_account("me@gmail.com".into(), Some("Anna".into()))
        .unwrap();
    assert_eq!(
        core.add_gmail_account("Me@Gmail.com".into(), None),
        Err(CoreError::AccountExists)
    );
    let account = core.account(AccountId(id)).unwrap();
    assert_eq!((account.kind, account.smtp), (AccountKind::Gmail, None));
    assert_eq!(account.display_name, "Anna");
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
        forward: None,
        files: vec![],
        signature: String::new(),
        cc: String::new(),
    })
    .unwrap();
}

#[test]
fn signs_with_the_accounts_signature_else_the_default_one() {
    let db = TempDb::new("signature");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let work = core
        .add_gmail_account("work@gmail.com".into(), None)
        .unwrap();
    let home = core
        .add_gmail_account("home@gmail.com".into(), None)
        .unwrap();
    let signs = |core: &Core| -> Vec<(String, String)> {
        core.accounts()
            .unwrap()
            .into_iter()
            .map(|a| (a.signature, a.signs_with))
            .collect()
    };
    assert_eq!(signs(&core), vec![(String::new(), String::new()); 2]);

    core.set_signature("--\nAnna\n".into()).unwrap();
    core.set_account_signature(work, "--\nAnna, Work".into())
        .unwrap();
    assert_eq!(core.signature().unwrap(), "--\nAnna");
    assert_eq!(
        signs(&core),
        [
            ("--\nAnna, Work".into(), "--\nAnna, Work".into()),
            (String::new(), "--\nAnna".into())
        ]
    );

    // A new message goes from the first account, signed as it signs.
    assert_eq!(core.new_draft().unwrap().signature, "--\nAnna, Work");

    // The message keeps its own, edited or not, through undo; it goes under the text.
    let draft = DraftItem {
        id: 0,
        account_id: home,
        to: "you@example.com".into(),
        subject: "Hi".into(),
        text: "Text".into(),
        reply_to: None,
        quote: "You wrote:\n> Hi".into(),
        saved: 0,
        forward: None,
        files: vec![],
        signature: "--\nA.".into(),
        cc: String::new(),
    };
    let saved = core.save_draft(draft.clone()).unwrap();
    assert_eq!(core.drafts().unwrap()[0].draft.signature, "--\nA.");
    let sent = core.send(DraftItem { id: saved, ..draft }).unwrap();
    let back = core.undo_send(sent).unwrap().unwrap();
    assert_eq!(back.signature, "--\nA.");
    core.send(back).unwrap();
    let due = core.store.due_sends(AccountId(home), i64::MAX).unwrap();
    let text = ozero_domain::rich::to_plain(&due[0].text, &|_| None);
    assert_eq!(text, "Text\n\n--\nA.\n\nYou wrote:\n> Hi");
}

#[test]
fn finds_no_server_for_an_address_without_a_domain() {
    let db = TempDb::new("found");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let found = runtime.block_on(core.add_found_account("anna".into(), "pw".into()));
    assert_eq!(found, Err(CoreError::NotFound));
}

#[test]
fn saves_an_exchange_account_that_sends_without_smtp() {
    let db = TempDb::new("exchange");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let add = |server: &str| {
        runtime.block_on(core.add_exchange_account(
            "me@corp.ru".into(),
            String::new(),
            Some(server.into()),
            None,
            None,
            None,
            "pw".into(),
        ))
    };
    // Not a server address: the app asks again before going online.
    assert_eq!(add("http://mail.corp.ru"), Err(CoreError::NoServer));
    let account = NewAccount {
        kind: AccountKind::Exchange,
        email: "me@corp.ru".into(),
        display_name: "Anna".into(),
        server: Some(Server {
            host: "mail.corp.ru".into(),
            port: 443,
            user: "CORP\\anna".into(),
            certificate: None,
        }),
        smtp: None,
    };
    let id = core.store.add_account(&account).unwrap().0;
    assert_eq!(add("mail.corp.ru"), Err(CoreError::AccountExists));
    assert_eq!(core.accounts().unwrap()[0].kind, AccountKind::Exchange);
    // Exchange sends over EWS.
    core.send(DraftItem {
        id: 0,
        account_id: id,
        to: "you@example.com".into(),
        subject: "Hi".into(),
        text: "Text".into(),
        reply_to: None,
        quote: String::new(),
        saved: 0,
        forward: None,
        files: vec![],
        signature: String::new(),
        cc: String::new(),
    })
    .unwrap();
}

#[test]
fn removes_an_account_at_once_while_it_syncs() {
    let db = TempDb::new("remove");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let id = core.add_gmail_account("me@gmail.com".into(), None).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // As a sync under way would.
    let lock = core.busy(AccountId(id));
    let syncing = lock.try_lock().unwrap();
    core.remove_account(id).unwrap();
    assert!(core.accounts().unwrap().is_empty());
    drop(syncing);
    // What waited for it ends quietly.
    assert_eq!(runtime.block_on(core.sync(id, String::new())), Ok(()));
    assert_eq!(
        runtime.block_on(core.push_actions(id, String::new())),
        Ok(())
    );
}

#[test]
fn signs_in_to_a_gmail_account_again() {
    let db = TempDb::new("again");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let id = core.add_gmail_account("me@gmail.com".into(), None).unwrap();
    let needs_sign_in = || core.accounts().unwrap()[0].needs_sign_in;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    // A refused access token only asks for a new one.
    let account = core.account(AccountId(id)).unwrap();
    assert_eq!(
        core.refused(&account, Err::<(), _>(CoreError::Auth)),
        Err(CoreError::Auth)
    );
    assert!(!needs_sign_in());
    // A refused login asks the person, and no login is tried until they sign in again.
    let login = |result| {
        runtime.block_on(core.signed_in(&core.account(AccountId(id)).unwrap(), async { result }))
    };
    assert_eq!(login(Err(CoreError::Auth)), Err(CoreError::Auth));
    assert!(needs_sign_in());
    assert_eq!(login(Ok(())), Err(CoreError::Auth));
    assert_eq!(core.accounts().unwrap()[0].kind, AccountKind::Gmail);
    core.signed_in_again(id, None).unwrap();
    assert!(!needs_sign_in());
    assert_eq!(login(Ok(())), Ok(()));
}

#[test]
fn offers_the_account_when_signing_in_again() {
    let start = google_sign_in(Some("me@gmail.com".into())).unwrap();
    assert!(start.url.contains("login_hint=me%40gmail.com"));
}

#[test]
fn starts_google_sign_in_at_google() {
    let start = google_sign_in(None).unwrap();
    assert!(start.url.starts_with("https://accounts.google.com/"));
    assert!(start.url.contains(&start.state));
    assert!(!start.url.contains(&start.verifier));
}

#[test]
fn reads_back_a_fingerprint_it_shows() {
    let fingerprint: [u8; 32] = std::array::from_fn(|i| i as u8 * 8);
    let shown = hex(&fingerprint);
    assert!(shown.starts_with("00:08:10:18:"));
    assert_eq!(unhex(&shown), Some(fingerprint));
    assert_eq!(unhex("00:08"), None);
    assert_eq!(unhex(&shown.replace(':', "")), None);
}

#[test]
fn says_a_first_name() {
    assert_eq!(first_name("Anna Lee"), "Anna");
    assert_eq!(first_name("Lee, Anna"), "Anna");
    assert_eq!(first_name("anna@x.com"), "anna");
    assert_eq!(first_name(" "), " ");
}

#[test]
fn folders_sit_inside_the_folder_their_name_continues() {
    let folder = |id: i64, name: &str, role: FolderRole| Folder {
        id: FolderId(id),
        account_id: AccountId(1),
        remote_id: name.into(),
        name: name.into(),
        role,
        pinned: false,
    };
    let tree = |folders: &[Folder]| -> Vec<(Option<i64>, String)> {
        folder_tree(folders)
            .into_iter()
            .map(|(parent, name)| (parent.map(|p| p.0), name))
            .collect()
    };
    let slashes = [
        folder(1, "INBOX", FolderRole::Inbox),
        folder(2, "Trips", FolderRole::Custom),
        folder(3, "Trips/2026", FolderRole::Custom),
        folder(4, "Trips/2026/Kazan", FolderRole::Custom),
        folder(5, "Tripsy", FolderRole::Custom),
        folder(6, "Dr", FolderRole::Custom),
        folder(7, "Dr. Who", FolderRole::Custom),
    ];
    let expected = [
        (None, "INBOX"),
        (None, "Trips"),
        (Some(2), "2026"),
        (Some(3), "Kazan"),
        (None, "Tripsy"),
        (None, "Dr"),
        (None, "Dr. Who"),
    ];
    assert_eq!(tree(&slashes), expected.map(|(p, n)| (p, n.to_owned())));

    // Kept under INBOX and split with dots, as on some servers.
    let dots = [
        folder(1, "INBOX", FolderRole::Inbox),
        folder(2, "INBOX.Bills", FolderRole::Custom),
        folder(3, "INBOX.Bills.2026", FolderRole::Custom),
    ];
    let expected = [(None, "INBOX"), (None, "Bills"), (Some(2), "2026")];
    assert_eq!(tree(&dots), expected.map(|(p, n)| (p, n.to_owned())));
}

#[test]
fn leaves_a_list_by_its_page_when_it_offers_nothing_else() {
    let db = TempDb::new("unsubscribe");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let store = &core.store;
    let account = store
        .add_account(&NewAccount {
            kind: AccountKind::Imap,
            email: "me@example.com".into(),
            display_name: "Me".into(),
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
    let message = |id: &str, links: Option<&str>| ozero_domain::NewMessage {
        remote_id: id.into(),
        message_id: None,
        references: vec![],
        from: Address {
            name: None,
            email: "news@shop.example".into(),
        },
        answer_to: vec![],
        to: vec![],
        cc: vec![],
        subject: id.into(),
        snippet: String::new(),
        date: id.parse().unwrap_or_default(),
        flags: Default::default(),
        category: Category::People,
        unsubscribe: ozero_domain::Unsubscribe::new(links, None),
    };
    let changes = Changes {
        checkpoint: store.checkpoint(folder).unwrap().0,
        new: vec![
            message("1", None),
            message("2", Some("<https://shop.example/leave>")),
        ],
        flags: vec![],
        gone: vec![],
    };
    store.save_changes(folder, &changes).unwrap();
    let rows = core.inbox(None, Category::People, 10).unwrap();

    assert!(rows[0].can_unsubscribe);
    assert_eq!(
        core.unsubscribe(rows[0].id),
        Ok(Unsubscribed::Page {
            url: "https://shop.example/leave".into()
        })
    );
    assert!(!rows[1].can_unsubscribe);
    assert_eq!(core.unsubscribe(rows[1].id), Err(CoreError::NotFound));
}

#[test]
fn the_sender_name_is_the_users_or_only_the_address() {
    let db = TempDb::new("sender");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let account = core
        .store
        .add_account(&NewAccount {
            kind: AccountKind::Imap,
            email: "a@example.com".into(),
            display_name: "Anna".into(),
            server: None,
            smtp: None,
        })
        .unwrap();
    let from = || core.accounts().unwrap()[0].sender_name.clone();
    core.set_account_sender(account.0, " Anna Petrova ".into())
        .unwrap();
    assert_eq!(from(), "Anna Petrova");
    core.set_account_sender(account.0, "  ".into()).unwrap();
    assert_eq!(from(), "");
    // The address alone is no name, so signing in again does not bring Gmail's back.
    core.set_account_sender(account.0, "A@example.com".into())
        .unwrap();
    assert_eq!(core.account(account).unwrap().display_name, "");
    core.signed_in_again(account.0, Some("Google Name".into()))
        .unwrap();
    assert_eq!(from(), "");
    assert!(core.accounts().unwrap()[0].in_all_fixed);
    assert!(core.set_account_sender(999, "Bob".into()).is_err());
}

#[test]
fn answers_an_invitation_by_mail_and_takes_it_back() {
    let db = TempDb::new("invite");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let store = &core.store;
    let server = |host: &str| Server {
        host: host.into(),
        port: 993,
        user: "me@example.com".into(),
        certificate: None,
    };
    let account = store
        .add_account(&NewAccount {
            kind: AccountKind::Imap,
            email: "me@example.com".into(),
            display_name: "Me".into(),
            server: Some(server("imap.example.com")),
            smtp: Some(server("smtp.example.com")),
        })
        .unwrap();
    let folders = [
        ("INBOX", FolderRole::Inbox),
        ("Archive", FolderRole::Archive),
    ]
    .map(|(name, role)| NewFolder {
        remote_id: name.into(),
        name: name.into(),
        role,
    });
    let inbox = store.save_folders(account, &folders).unwrap()[0].id;
    let message = ozero_domain::NewMessage {
        remote_id: "1".into(),
        message_id: Some("m1@example.org".into()),
        references: vec![],
        from: Address {
            name: Some("Anna".into()),
            email: "anna@example.org".into(),
        },
        answer_to: vec![],
        to: vec![],
        cc: vec![],
        subject: "Invitation: Sprint".into(),
        snippet: String::new(),
        date: 10,
        flags: Default::default(),
        category: Category::People,
        unsubscribe: None,
    };
    let changes = Changes {
        checkpoint: store.checkpoint(inbox).unwrap().0,
        new: vec![message],
        flags: vec![],
        gone: vec![],
    };
    store.save_changes(inbox, &changes).unwrap();
    let id = core.inbox(None, Category::People, 10).unwrap()[0].id;
    assert_eq!(core.invite(id), Ok(None));
    let calendar = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:s1\r\n\
SUMMARY:Sprint\r\nDTSTART:20261029T080000Z\r\nDTEND:20261029T090000Z\r\n\
LOCATION:Baikal\r\nORGANIZER;CN=Olga:mailto:olga@example.org\r\n\
ATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:me@example.com\r\n\
ATTENDEE;PARTSTAT=ACCEPTED:mailto:ivan@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let body = Body {
        calendar: Some(calendar.into()),
        ..Body::default()
    };
    store.save_body(MessageId(id), &body).unwrap();
    let row = core.inbox(None, Category::People, 10).unwrap().remove(0);
    assert_eq!(row.invite.map(|i| i.location), Some("Baikal".into()));

    let invite = core.invite(id).unwrap().unwrap();
    assert_eq!(invite.summary, "Sprint");
    assert_eq!(invite.organizer_name.as_deref(), Some("Olga"));
    assert_eq!(invite.others, 1);
    assert_eq!(invite.answer, None);
    assert!(invite.can_answer && invite.can_add_to_calendar && !invite.cancelled);

    let answered = core
        .answer_invite(
            id,
            InviteAnswer::Yes,
            " Late by 5 ".into(),
            "Accepted: Sprint".into(),
        )
        .unwrap();
    assert!(answered.archived);
    assert_eq!(
        core.invite(id).unwrap().unwrap().answer,
        Some(InviteAnswer::Yes)
    );
    assert!(core.inbox(None, Category::People, 10).unwrap().is_empty());
    let mail = OutboxId(answered.mail.unwrap());
    let files = store.outbox_files(mail).unwrap();
    assert_eq!(files[0].mime, "text/calendar; method=REPLY");
    let reply = String::from_utf8(files[0].data.clone()).unwrap();
    assert!(reply.contains("ATTENDEE;PARTSTAT=ACCEPTED;CN=\"Me\":mailto:me@example.com\r\n"));
    assert!(reply.contains("COMMENT:Late by 5\r\n"));
    assert!(core.drafts().unwrap().is_empty());

    assert_eq!(core.undo_answer(answered), Ok(true));
    assert_eq!(core.invite(id).unwrap().unwrap().answer, None);
    assert_eq!(core.inbox(None, Category::People, 10).unwrap().len(), 1);
    assert_eq!(store.due_sends(account, i64::MAX).unwrap(), vec![]);
    assert!(core.drafts().unwrap().is_empty());
}

#[test]
fn suggests_people_and_names_them_as_the_contacts_do() {
    let db = TempDb::new("contacts");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let store = &core.store;
    let account = store
        .add_account(&NewAccount {
            kind: AccountKind::Imap,
            email: "me@example.com".into(),
            display_name: "Me".into(),
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
    let message = |uid: &str, name: &str, email: &str, date: i64| ozero_domain::NewMessage {
        remote_id: uid.into(),
        message_id: None,
        references: vec![],
        from: Address {
            name: Some(name.into()),
            email: email.into(),
        },
        answer_to: vec![],
        to: vec![],
        cc: vec![],
        subject: "Hi".into(),
        snippet: String::new(),
        date,
        flags: Default::default(),
        category: Category::People,
        unsubscribe: None,
    };
    let changes = Changes {
        checkpoint: store.checkpoint(folder).unwrap().0,
        new: vec![
            message("1", "Patimat M.", "patimat@example.com", 10),
            message("2", "Pavel", "pavel@example.com", 20),
            message("3", "Pavel", "pavel@example.com", 30),
        ],
        flags: vec![],
        gone: vec![],
    };
    store.save_changes(folder, &changes).unwrap();
    let emails =
        |found: Vec<PersonFound>| -> Vec<String> { found.into_iter().map(|p| p.email).collect() };
    assert_eq!(
        emails(core.find_people("pa".into(), 5).unwrap()),
        ["pavel@example.com", "patimat@example.com"]
    );

    core.set_contacts(vec![
        ContactItem {
            key: "k1".into(),
            name: "Мама".into(),
            emails: vec!["patimat@example.com".into(), "mama@home.example".into()],
        },
        ContactItem {
            key: "k2".into(),
            name: " ".into(),
            emails: vec!["nobody@example.com".into()],
        },
    ])
    .unwrap();
    let found = core.find_people("мам".into(), 5).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|p| (p.name.as_str(), p.contact))
            .collect::<Vec<_>>(),
        [("Мама", true), ("Мама", true)]
    );
    // A contact with only an address is still one, by its address.
    let nobody = core.find_people("nobody".into(), 5).unwrap();
    assert_eq!(
        (nobody[0].name.as_str(), nobody[0].contact),
        ("nobody@example.com", true)
    );
    let rows = core.inbox(None, Category::People, 10).unwrap();
    let senders: Vec<_> = rows
        .iter()
        .map(|r| {
            r.contact_name
                .clone()
                .or(r.from_name.clone())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(senders, ["Pavel", "Pavel", "Мама"]);
    // Quotes and forwards keep the name the mail gives.
    assert_ne!(rows[2].from_name.as_deref(), Some("Мама"));
    assert_eq!(rows[2].senders[0].name, "Мама");
    let card = core
        .person("patimat@example.com".into(), Some("Patimat M.".into()))
        .unwrap();
    assert_eq!(card.name, "Мама");
    assert_eq!(card.emails, ["patimat@example.com", "mama@home.example"]);
    assert_eq!(card.contact.as_deref(), Some("k1"));
    let mail = core
        .mail_with(vec!["pavel@example.com".into()], 10)
        .unwrap();
    assert_eq!(mail.len(), 2);

    // Without access, the mail's names come back.
    core.set_contacts(vec![]).unwrap();
    let rows = core.inbox(None, Category::People, 10).unwrap();
    assert_eq!(rows[2].from_name.as_deref(), Some("Patimat M."));
    assert_eq!(
        core.person("patimat@example.com".into(), None)
            .unwrap()
            .contact,
        None
    );
}

#[test]
fn moves_chosen_messages_only_where_all_of_them_can_go() {
    let db = TempDb::new("move-targets");
    let core = Core::open(db.0.clone(), vec![7; 32]).unwrap();
    let store = &core.store;
    let add = |email: &str| {
        store
            .add_account(&NewAccount {
                kind: AccountKind::Imap,
                email: email.into(),
                display_name: email.into(),
                server: None,
                smtp: None,
            })
            .unwrap()
    };
    let folder = |remote_id: &str, role| NewFolder {
        remote_id: remote_id.into(),
        name: remote_id.into(),
        role,
    };
    let message = |id: &str| ozero_domain::NewMessage {
        remote_id: id.into(),
        message_id: None,
        references: vec![],
        from: Address {
            name: None,
            email: "anna@example.com".into(),
        },
        answer_to: vec![],
        to: vec![],
        cc: vec![],
        subject: id.into(),
        snippet: String::new(),
        date: 0,
        flags: Default::default(),
        category: Category::People,
        unsubscribe: ozero_domain::Unsubscribe::new(None, None),
    };
    // One message in each of the given folders, by their ids.
    let fill = |folders: &[i64]| -> Vec<i64> {
        folders
            .iter()
            .map(|&id| {
                let folder = FolderId(id);
                let changes = Changes {
                    checkpoint: store.checkpoint(folder).unwrap().0,
                    new: vec![message("1")],
                    flags: vec![],
                    gone: vec![],
                };
                store.save_changes(folder, &changes).unwrap();
                core.mailbox(Mailbox::Folder { id }, 10).unwrap()[0].id
            })
            .collect()
    };
    let names = |messages: Vec<i64>| -> Vec<String> {
        core.move_targets(messages)
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect()
    };

    let me = add("me@example.com");
    let saved = store
        .save_folders(
            me,
            &[
                folder("INBOX", FolderRole::Inbox),
                folder("Archive", FolderRole::Archive),
                folder("Work", FolderRole::Custom),
                folder("Trips", FolderRole::Custom),
                folder("Junk", FolderRole::Junk),
            ],
        )
        .unwrap();
    let ids: Vec<i64> = saved.iter().map(|f| f.id.0).collect();
    let mine = fill(&[ids[0], ids[2], ids[4]]);

    assert_eq!(names(vec![mine[0]]), ["Archive", "Work", "Trips"]);
    // Either can go where the other is.
    assert_eq!(
        names(vec![mine[0], mine[1]]),
        ["INBOX", "Archive", "Work", "Trips"]
    );
    assert_eq!(names(vec![mine[2]]), ["INBOX", "Archive", "Work", "Trips"]);
    // Only the one in spam says so, for Not spam.
    let spam = |id| core.mailbox(Mailbox::Folder { id }, 10).unwrap()[0].spam;
    assert_eq!([ids[0], ids[2], ids[4]].map(spam), [false, false, true]);

    let other = add("other@example.com");
    let theirs = store
        .save_folders(other, &[folder("INBOX", FolderRole::Inbox)])
        .unwrap();
    let theirs = fill(&[theirs[0].id.0]);
    assert!(names(vec![mine[0], theirs[0]]).is_empty());
    assert!(names(vec![]).is_empty());

    // Only the one not there yet moves.
    assert_eq!(
        core.move_to_folder(vec![mine[0], mine[1]], ids[2])
            .unwrap()
            .len(),
        1
    );
}
