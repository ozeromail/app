//! Fake ports for use case tests.

use std::future::{Future, ready};
use std::pin::pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use ozero_domain::{
    Account, AccountId, Action, Address, Body, Category, Changes, Checkpoint, Correspondents,
    Draft, Flags, Folder, FolderId, ListQuery, MailProvider, MailSender, MessageBuilder, MessageId,
    MessageLocation, MessageParser, MessageRow, NewAccount, NewFolder, NewMessage, Op, OpId,
    OutboxId, Outgoing, ProviderError, RawMessage, Store, StoreError, Unsent,
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
    /// Each `changes` call: the folder, checkpoint, known ids and limit.
    pub synced: Vec<(String, Checkpoint, Vec<String>, u32)>,
    /// Each `set_seen` and `move_to` call; they fail with the first of `refuse` while any remain.
    pub applied: Vec<(String, u32, Vec<String>, Action)>,
    pub refuse: Vec<ProviderError>,
    pub moved_to: Vec<String>,
    pub appended: Vec<(String, Vec<u8>)>,
    /// Each `wait` call: the folder, checkpoint and how long.
    pub waited: Vec<(String, Checkpoint, Duration)>,
}

impl MailProvider for Server {
    fn folders(&mut self) -> impl Future<Output = Result<Vec<NewFolder>, ProviderError>> + Send {
        ready(Ok(self.folders.clone()))
    }

    fn changes(
        &mut self,
        folder: &str,
        since: Checkpoint,
        known: &[String],
        limit: u32,
    ) -> impl Future<Output = Result<Changes, ProviderError>> + Send {
        self.synced
            .push((folder.into(), since, known.to_vec(), limit));
        ready(Ok(Changes {
            checkpoint: Checkpoint {
                validity: 9,
                next: 8,
            },
            new: vec![RawMessage {
                remote_id: "7".into(),
                flags: Flags::default(),
                received: 100,
                header: b"Hi".to_vec(),
            }],
            flags: vec![("5".into(), Flags::default())],
            gone: vec!["6".into()],
        }))
    }

    fn body(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> impl Future<Output = Result<Vec<u8>, ProviderError>> + Send {
        self.asked.push((format!("{folder}/{remote_id}"), validity));
        ready(match remote_id {
            "empty" => Ok(vec![]),
            "fail" => Err(ProviderError::Network("down".into())),
            "gone" => Err(ProviderError::Gone),
            _ => Ok(b"Hello".to_vec()),
        })
    }

    fn set_seen(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        seen: bool,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send {
        ready(self.apply(folder, validity, remote_ids, Action::Seen(seen)))
    }

    fn move_to(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send {
        self.moved_to.push(to.into());
        ready(self.apply(folder, validity, remote_ids, Action::Archive))
    }

    fn delete(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
    ) -> impl Future<Output = Result<(), ProviderError>> + Send {
        ready(self.apply(folder, validity, remote_ids, Action::Delete))
    }

    fn empty(
        &mut self,
        folder: &str,
        validity: u32,
        keep: &[String],
    ) -> impl Future<Output = Result<(), ProviderError>> + Send {
        ready(self.apply(folder, validity, keep, Action::Empty))
    }

    fn append(
        &mut self,
        folder: &str,
        message: &[u8],
    ) -> impl Future<Output = Result<(), ProviderError>> + Send {
        self.appended.push((folder.into(), message.to_vec()));
        ready(Ok(()))
    }

    fn wait(
        &mut self,
        folder: &str,
        since: Checkpoint,
        wait: Duration,
    ) -> impl Future<Output = Result<bool, ProviderError>> + Send {
        self.waited.push((folder.into(), since, wait));
        ready(Ok(true))
    }
}

/// A fake SMTP server: records each message, or fails with the first of `refuse`.
#[derive(Default)]
pub struct Postman {
    pub sent: Vec<(String, Vec<String>, Vec<u8>)>,
    pub refuse: Vec<ProviderError>,
}

impl MailSender for Postman {
    fn send(
        &mut self,
        from: &str,
        to: &[String],
        message: &[u8],
    ) -> impl Future<Output = Result<(), ProviderError>> + Send {
        ready(if self.refuse.is_empty() {
            self.sent.push((from.into(), to.to_vec(), message.to_vec()));
            Ok(())
        } else {
            Err(self.refuse.remove(0))
        })
    }
}

/// Writes only the subject.
pub struct Builder;

impl MessageBuilder for Builder {
    fn build(&self, _: &Address, message: &Outgoing) -> Vec<u8> {
        message.subject.clone().into_bytes()
    }
}

impl Server {
    fn apply(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        action: Action,
    ) -> Result<(), ProviderError> {
        self.applied
            .push((folder.into(), validity, remote_ids.to_vec(), action));
        if self.refuse.is_empty() {
            Ok(())
        } else {
            Err(self.refuse.remove(0))
        }
    }
}

pub struct Parser;

impl MessageParser for Parser {
    fn parse(&self, raw: RawMessage) -> NewMessage {
        NewMessage {
            references: vec![],
            remote_id: raw.remote_id,
            message_id: None,
            from: Address {
                name: None,
                email: "a@example.com".into(),
            },
            answer_to: vec![],
            recipients: vec![],
            subject: String::from_utf8_lossy(&raw.header).into(),
            snippet: String::new(),
            date: raw.received,
            flags: raw.flags,
            category: Category::People,
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
    /// What `folders` returns.
    pub folders: Vec<Folder>,
    /// What `checkpoint` returns, and each `save_changes`; `others` leaves out of it.
    pub checkpoint: (Checkpoint, Vec<String>),
    pub changes: Mutex<Vec<(FolderId, Changes<NewMessage>)>>,
    pub bodies: Mutex<Vec<(MessageId, Body)>>,
    /// What `without_body` returns.
    pub without_body: Vec<MessageLocation>,
    /// What `due` returns, and which ops were finished or reverted.
    pub ops: Vec<Op>,
    pub finished: Mutex<Vec<OpId>>,
    pub reverted: Mutex<Vec<OpId>>,
    pub sent: Mutex<Vec<OutboxId>>,
    pub refused: Mutex<Vec<(OutboxId, String)>>,
}

impl Store for Memory {
    fn add_account(&self, _: &NewAccount) -> Result<AccountId, StoreError> {
        Ok(AccountId(1))
    }

    fn accounts(&self) -> Result<Vec<Account>, StoreError> {
        Ok(vec![])
    }

    fn shows_images(&self, _: MessageId) -> Result<bool, StoreError> {
        Ok(false)
    }

    fn allow_images(&self, _: MessageId) -> Result<(), StoreError> {
        Ok(())
    }

    fn correspondents(&self, _: MessageId) -> Result<Correspondents, StoreError> {
        Ok(Correspondents::default())
    }

    fn remove_account(&self, _: AccountId) -> Result<(), StoreError> {
        Ok(())
    }

    fn set_account_color(&self, _: AccountId, _: u32) -> Result<(), StoreError> {
        Ok(())
    }

    fn set_account_label(&self, _: AccountId, _: &str) -> Result<(), StoreError> {
        Ok(())
    }

    fn set_needs_sign_in(&self, _: AccountId, _: bool) -> Result<(), StoreError> {
        Ok(())
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
        Ok(self.folders.clone())
    }

    fn checkpoint(&self, _: FolderId) -> Result<(Checkpoint, Vec<String>), StoreError> {
        Ok(self.checkpoint.clone())
    }

    fn save_changes(
        &self,
        folder: FolderId,
        changes: &Changes<NewMessage>,
    ) -> Result<(), StoreError> {
        self.changes.lock().unwrap().push((folder, changes.clone()));
        Ok(())
    }

    fn inbox_unread(&self) -> Result<Vec<(Category, u32)>, StoreError> {
        Ok(vec![])
    }

    fn inbox_is_empty(&self) -> Result<bool, StoreError> {
        Ok(true)
    }

    fn take_fresh(&self, _: i64) -> Result<Vec<MessageRow>, StoreError> {
        Ok(vec![])
    }

    fn clear_fresh(&self) -> Result<(), StoreError> {
        Ok(())
    }

    fn list(&self, _: &ListQuery) -> Result<Vec<MessageRow>, StoreError> {
        Ok(vec![])
    }

    fn thread(&self, _: MessageId) -> Result<Vec<MessageRow>, StoreError> {
        Ok(vec![])
    }

    fn search(&self, _: &str, _: u32) -> Result<Vec<MessageRow>, StoreError> {
        Ok(vec![])
    }

    fn location(&self, _: MessageId) -> Result<MessageLocation, StoreError> {
        Err(StoreError::NotFound)
    }

    fn without_body(&self, _: AccountId, limit: u32) -> Result<Vec<MessageLocation>, StoreError> {
        Ok(self
            .without_body
            .iter()
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn save_body(&self, message: MessageId, body: &Body) -> Result<(), StoreError> {
        self.bodies.lock().unwrap().push((message, body.clone()));
        Ok(())
    }

    fn body(&self, _: MessageId) -> Result<Option<Body>, StoreError> {
        Ok(None)
    }

    fn queue(&self, _: MessageId, _: Action, _: i64) -> Result<Vec<OpId>, StoreError> {
        Ok(vec![OpId(1)])
    }

    fn queue_empty(&self, _: FolderId, _: i64) -> Result<Vec<OpId>, StoreError> {
        Ok(vec![])
    }

    /// The `checkpoint` messages that `ops` leave out.
    fn others(&self, ops: &[Op]) -> Result<Vec<String>, StoreError> {
        let mut ids = self.checkpoint.1.clone();
        ids.retain(|id| ops.iter().all(|o| &o.location.remote_id != id));
        Ok(ids)
    }

    fn undo(&self, _: &[OpId], _: i64) -> Result<bool, StoreError> {
        Ok(false)
    }

    fn next_send(&self, _: AccountId) -> Result<Option<i64>, StoreError> {
        Ok(None)
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

    fn queue_send(&self, _: &Draft, _: &str, _: i64, _: i64) -> Result<OutboxId, StoreError> {
        Ok(OutboxId(1))
    }

    fn undo_send(&self, _: OutboxId, _: i64) -> Result<Option<Draft>, StoreError> {
        Ok(None)
    }

    fn unsent(&self) -> Result<Vec<Unsent>, StoreError> {
        Ok(vec![])
    }

    fn due_sends(&self, _: AccountId, _: i64) -> Result<Vec<Outgoing>, StoreError> {
        Ok(vec![])
    }

    fn sent(&self, id: OutboxId) -> Result<(), StoreError> {
        self.sent.lock().unwrap().push(id);
        Ok(())
    }

    fn refused(&self, id: OutboxId, reason: &str) -> Result<(), StoreError> {
        self.refused.lock().unwrap().push((id, reason.into()));
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
