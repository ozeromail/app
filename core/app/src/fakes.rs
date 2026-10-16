//! Fake ports for use case tests.

use std::future::{Future, ready};
use std::pin::pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use ozero_domain::{
    Account, AccountId, Action, Address, Body, Flags, Folder, FolderId, ListQuery, MailProvider,
    MessageId, MessageLocation, MessageParser, MessageRow, NewAccount, NewFolder, NewMessage, Op,
    OpId, ProviderError, RawMessage, Recent, Store, StoreError,
};

/// Fakes answer at once, so one poll finishes the future.
pub fn run<F: Future>(f: F) -> F::Output {
    match pin!(f).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(out) => out,
        Poll::Pending => panic!("fakes never wait"),
    }
}

#[derive(Default)]
pub struct Server {
    pub folders: Vec<NewFolder>,
    pub asked: Vec<(String, u32)>,
    /// Each `apply` call; `apply` fails with the first of `refuse` while any remain.
    pub applied: Vec<(String, u32, Vec<String>, Action)>,
    pub refuse: Vec<ProviderError>,
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

    fn apply(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        action: Action,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send {
        self.applied
            .push((folder.into(), validity, remote_ids.to_vec(), action));
        ready(if self.refuse.is_empty() {
            Ok(())
        } else {
            Err(self.refuse.remove(0))
        })
    }
}

pub struct Parser;

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
            ..Body::default()
        }
    }
}

#[derive(Default)]
pub struct Memory {
    pub validity: Mutex<Vec<(FolderId, u32)>>,
    pub saved: Mutex<Vec<(FolderId, Vec<NewMessage>)>>,
    pub bodies: Mutex<Vec<(MessageId, Body)>>,
    /// What `due` returns, and which ops were finished or reverted.
    pub ops: Vec<Op>,
    pub finished: Mutex<Vec<OpId>>,
    pub reverted: Mutex<Vec<OpId>>,
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

    fn queue(&self, _: MessageId, _: Action, _: i64) -> Result<(), StoreError> {
        Ok(())
    }

    fn due(&self, _: AccountId, _: i64) -> Result<Vec<Op>, StoreError> {
        Ok(self.ops.clone())
    }

    fn finish(&self, ops: &[Op]) -> Result<(), StoreError> {
        self.finished
            .lock()
            .unwrap()
            .extend(ops.iter().map(|o| o.id));
        Ok(())
    }

    fn revert(&self, ops: &[Op]) -> Result<(), StoreError> {
        self.reverted
            .lock()
            .unwrap()
            .extend(ops.iter().map(|o| o.id));
        Ok(())
    }
}
