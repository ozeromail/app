use std::future::{Future, ready};
use std::pin::pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use ozero_domain::{
    Account, Address, Body, Flags, Folder, FolderId, ListQuery, MessageId, MessageRow, NewAccount,
    NewFolder, NewMessage, RawMessage, Recent,
};

use super::*;

/// Fakes answer at once, so one poll finishes the future.
fn run<F: Future>(f: F) -> F::Output {
    match pin!(f).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(out) => out,
        Poll::Pending => panic!("fakes never wait"),
    }
}

struct Server {
    folders: Vec<NewFolder>,
    asked: Vec<(String, u32)>,
}

impl MailProvider for Server {
    fn folders(&mut self) -> impl Future<Output = Result<Vec<NewFolder>, ProviderError>> + Send {
        ready(Ok(self.folders.clone()))
    }

    fn recent(
        &mut self,
        folder: &str,
        limit: u32,
    ) -> impl Future<Output = Result<Recent, ProviderError>> + Send {
        self.asked.push((folder.into(), limit));
        ready(Ok(Recent {
            validity: 9,
            messages: vec![RawMessage {
                remote_id: "7".into(),
                flags: Flags::default(),
                received: 100,
                header: b"Hi".to_vec(),
            }],
        }))
    }

    fn body(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> impl Future<Output = Result<Vec<u8>, ProviderError>> + Send {
        self.asked.push((format!("{folder}/{remote_id}"), validity));
        ready(if remote_id == "empty" {
            Ok(vec![])
        } else {
            Ok(b"Hello".to_vec())
        })
    }
}

struct Parser;

impl MessageParser for Parser {
    fn parse(&self, raw: RawMessage) -> NewMessage {
        NewMessage {
            remote_id: raw.remote_id,
            message_id: None,
            from: Address {
                name: None,
                email: "a@example.com".into(),
            },
            subject: String::from_utf8_lossy(&raw.header).into(),
            snippet: String::new(),
            date: raw.received,
            flags: raw.flags,
        }
    }

    fn parse_body(&self, raw: &[u8]) -> Body {
        Body {
            text: String::from_utf8_lossy(raw).into(),
            html: None,
        }
    }
}

#[derive(Default)]
struct Memory {
    validity: Mutex<Vec<(FolderId, u32)>>,
    saved: Mutex<Vec<(FolderId, Vec<NewMessage>)>>,
    bodies: Mutex<Vec<(MessageId, Body)>>,
}

impl Store for Memory {
    fn add_account(&self, _: &NewAccount) -> Result<AccountId, StoreError> {
        Ok(AccountId(1))
    }

    fn accounts(&self) -> Result<Vec<Account>, StoreError> {
        Ok(vec![])
    }

    fn save_folders(
        &self,
        account: AccountId,
        folders: &[NewFolder],
    ) -> Result<Vec<Folder>, StoreError> {
        Ok(folders
            .iter()
            .zip(1..)
            .map(|(f, id)| Folder {
                id: FolderId(id),
                account_id: account,
                remote_id: f.remote_id.clone(),
                name: f.name.clone(),
                role: f.role,
            })
            .collect())
    }

    fn folders(&self, _: AccountId) -> Result<Vec<Folder>, StoreError> {
        Ok(vec![])
    }

    fn set_validity(&self, folder: FolderId, validity: u32) -> Result<(), StoreError> {
        self.validity.lock().unwrap().push((folder, validity));
        Ok(())
    }

    fn save_messages(&self, folder: FolderId, messages: &[NewMessage]) -> Result<(), StoreError> {
        self.saved.lock().unwrap().push((folder, messages.to_vec()));
        Ok(())
    }

    fn list(&self, _: &ListQuery) -> Result<Vec<MessageRow>, StoreError> {
        Ok(vec![])
    }

    fn location(&self, _: MessageId) -> Result<MessageLocation, StoreError> {
        Err(StoreError::NotFound)
    }

    fn save_body(&self, message: MessageId, body: &Body) -> Result<(), StoreError> {
        self.bodies.lock().unwrap().push((message, body.clone()));
        Ok(())
    }

    fn body(&self, _: MessageId) -> Result<Option<Body>, StoreError> {
        Ok(None)
    }
}

fn folder(remote_id: &str, role: FolderRole) -> NewFolder {
    NewFolder {
        remote_id: remote_id.into(),
        name: remote_id.into(),
        role,
    }
}

#[test]
fn saves_the_newest_inbox_messages() {
    let store = Memory::default();
    let mut server = Server {
        folders: vec![
            folder("Sent", FolderRole::Sent),
            folder("INBOX", FolderRole::Inbox),
        ],
        asked: vec![],
    };
    run(sync_inbox(&store, &mut server, &Parser, AccountId(1), 50)).unwrap();

    assert_eq!(server.asked, [("INBOX".to_string(), 50)]);
    assert_eq!(*store.validity.lock().unwrap(), [(FolderId(2), 9)]);
    let saved = store.saved.lock().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, FolderId(2));
    assert_eq!(saved[0].1[0].subject, "Hi");
}

#[test]
fn fails_without_an_inbox() {
    let mut server = Server {
        folders: vec![folder("Sent", FolderRole::Sent)],
        asked: vec![],
    };
    assert_eq!(
        run(sync_inbox(
            &Memory::default(),
            &mut server,
            &Parser,
            AccountId(1),
            50
        )),
        Err(SyncError::NoInbox)
    );
    assert!(server.asked.is_empty());
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

fn server() -> Server {
    Server {
        folders: vec![],
        asked: vec![],
    }
}

#[test]
fn downloads_a_body() {
    let store = Memory::default();
    let mut server = server();
    let body = run(download_body(&store, &mut server, &Parser, &location("7"))).unwrap();
    assert_eq!(body.text, "Hello");

    assert_eq!(server.asked, [("INBOX/7".to_string(), 9)]);
    let bodies = store.bodies.lock().unwrap();
    assert_eq!(bodies[0].0, MessageId(4));
    assert_eq!(bodies[0].1.text, "Hello");
}

#[test]
fn does_not_save_a_body_without_text() {
    let store = Memory::default();
    let body = run(download_body(
        &store,
        &mut server(),
        &Parser,
        &location("empty"),
    ))
    .unwrap();
    assert_eq!(body.text, "");
    assert!(store.bodies.lock().unwrap().is_empty());
}
