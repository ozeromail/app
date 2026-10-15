use std::future::{Future, ready};
use std::pin::pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use ozero_domain::{
    Account, Address, Flags, Folder, FolderId, ListQuery, MessageRow, NewAccount, NewFolder,
    NewMessage, RawMessage, Recent,
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
}

#[derive(Default)]
struct Memory {
    validity: Mutex<Vec<(FolderId, u32)>>,
    saved: Mutex<Vec<(FolderId, Vec<NewMessage>)>>,
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
