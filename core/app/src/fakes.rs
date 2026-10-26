//! Fake ports for use case tests.

use std::future::{Future, ready};
use std::pin::pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use ozero_domain::{
    Account, AccountId, Action, Address, Body, Category, Changes, Checkpoint, Correspondents,
    Draft, DraftFile, DraftId, FileId, Flags, Folder, FolderId, FolderRole, ListQuery,
    MailProvider, MailSender, MessageBuilder, MessageId, MessageLocation, MessageParser,
    MessageRow, Moved, NewAccount, NewFolder, NewMail, NewMessage, Older, Op, OpId, OutboxId, Outgoing,
    OutgoingFile, ProviderError, RawMessage, RecentFile, SavedDraft, Store, StoreError, Unsent,
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
    /// Each `create_snoozed` call's name; names in `refuse_names` fail.
    pub created: Vec<String>,
    pub refuse_names: Vec<String>,
    /// Each `wait` call: the folder, checkpoint and how long.
    pub waited: Vec<(String, Checkpoint, Duration)>,
    /// Each `older` call: the folder, validity, known ids, oldest date and limit.
    pub paged: Vec<(String, u32, Vec<String>, i64, u32)>,
    /// Each `search` call: the folder, validity, text and limit. Folders named "Gone" and "Refused" fail.
    pub searched: Vec<(String, u32, String, u32)>,
}

/// A message as `Server` returns it.
fn raw(remote_id: &str, received: i64) -> RawMessage {
    RawMessage {
        remote_id: remote_id.into(),
        flags: Flags::default(),
        received,
        header: b"Hi".to_vec(),
    }
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
        _oldest: i64,
        limit: u32,
    ) -> impl Future<Output = Result<Changes, ProviderError>> + Send {
        self.synced
            .push((folder.into(), since, known.to_vec(), limit));
        ready(Ok(Changes {
            checkpoint: Checkpoint {
                validity: 9,
                next: 8,
            },
            new: vec![raw("7", 100)],
            flags: vec![("5".into(), Flags::default())],
            gone: vec!["6".into()],
        }))
    }

    /// Two older messages, received at 50 and 60, and more after them.
    fn older(
        &mut self,
        folder: &str,
        validity: u32,
        known: &[String],
        oldest: i64,
        limit: u32,
    ) -> impl Future<Output = Result<Older, ProviderError>> + Send {
        self.paged
            .push((folder.into(), validity, known.to_vec(), oldest, limit));
        ready(Ok(Older {
            new: vec![raw("3", 50), raw("4", 60)],
            more: true,
        }))
    }

    /// One message, named after the folder.
    fn search(
        &mut self,
        folder: &str,
        validity: u32,
        text: &str,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<RawMessage>, ProviderError>> + Send {
        self.searched
            .push((folder.into(), validity, text.into(), limit));
        ready(match folder {
            "Gone" => Err(ProviderError::Gone),
            "Refused" => Err(ProviderError::Rejected("BADCHARSET".into())),
            _ => Ok(vec![raw(folder, 10)]),
        })
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
    ) -> impl Future<Output = Result<Moved, ProviderError>> + Send {
        self.moved_to.push(to.into());
        // There a message's id gets a 0 at the end.
        let moved = Moved {
            validity: Some(validity),
            ids: remote_ids
                .iter()
                .map(|id| (id.clone(), format!("{id}0")))
                .collect(),
        };
        ready(
            self.apply(folder, validity, remote_ids, Action::Archive)
                .map(|()| moved),
        )
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

    fn create_snoozed(
        &mut self,
        name: &str,
    ) -> impl Future<Output = Result<NewFolder, ProviderError>> + Send {
        self.created.push(name.into());
        if self.refuse_names.iter().any(|n| n == name) {
            return ready(Err(ProviderError::Rejected("taken".into())));
        }
        ready(Ok(NewFolder {
            remote_id: name.into(),
            name: name.into(),
            role: FolderRole::from_name(name),
        }))
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

    /// The whole message is attachment 0.
    fn attachments(&self, raw: &[u8], parts: &[u32]) -> Vec<Option<Vec<u8>>> {
        parts
            .iter()
            .map(|&p| (p == 0).then(|| raw.to_vec()))
            .collect()
    }
}

/// One `save_found`: the account, the words and what each folder found.
pub type Found = (AccountId, String, Vec<(FolderId, Vec<NewMessage>)>);

#[derive(Default)]
pub struct Memory {
    /// What `folders` returns.
    pub folders: Vec<Folder>,
    /// What `checkpoint` returns, and each `save_changes`; `others` leaves out of it.
    pub checkpoint: (Checkpoint, Vec<String>),
    /// What `oldest` returns, and each `prune`.
    pub oldest: Option<i64>,
    /// What `next_wake` returns: when snoozed mail waits.
    pub next_wake: Option<i64>,
    pub pruned: Mutex<Vec<(AccountId, i64)>>,
    /// Folders `set_older_done` marked.
    pub older_done: Mutex<Vec<(FolderId, bool)>>,
    /// Each `save_found`.
    pub found: Mutex<Vec<Found>>,
    pub changes: Mutex<Vec<(FolderId, Changes<NewMessage>)>>,
    pub bodies: Mutex<Vec<(MessageId, Body)>>,
    /// What `without_body` returns.
    pub without_body: Vec<MessageLocation>,
    /// What `due` returns, and which ops were finished or reverted.
    pub ops: Vec<Op>,
    pub finished: Mutex<Vec<OpId>>,
    /// Each `land`.
    pub landed: Mutex<Vec<Moved>>,
    /// What `unclaimed` returns.
    pub unclaimed: Vec<Folder>,
    pub reverted: Mutex<Vec<OpId>>,
    /// How many of `reverted` `take_reverted` told of.
    pub told: Mutex<usize>,
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

    fn provider_state(&self, _: AccountId, _: &str) -> Result<Option<String>, StoreError> {
        Ok(None)
    }

    fn set_provider_state(&self, _: AccountId, _: &str, _: Option<&str>) -> Result<(), StoreError> {
        Ok(())
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

    fn set_account_name(&self, _: AccountId, _: &str) -> Result<(), StoreError> {
        Ok(())
    }

    fn set_account_label(&self, _: AccountId, _: &str) -> Result<(), StoreError> {
        Ok(())
    }

    fn set_account_keep(&self, _: AccountId, _: u32) -> Result<(), StoreError> {
        Ok(())
    }

    fn set_account_new_mail(&self, _: AccountId, _: NewMail) -> Result<(), StoreError> {
        Ok(())
    }

    fn prune(&self, account: AccountId, before: i64) -> Result<(), StoreError> {
        self.pruned.lock().unwrap().push((account, before));
        Ok(())
    }

    fn set_needs_sign_in(&self, _: AccountId, _: bool) -> Result<(), StoreError> {
        Ok(())
    }

    fn snooze(&self, _: &[MessageId], _: i64, _: i64) -> Result<Vec<OpId>, StoreError> {
        Ok(vec![])
    }

    fn wake(&self, _: i64, _: i64) -> Result<Vec<MessageRow>, StoreError> {
        Ok(vec![])
    }

    fn next_wake(&self) -> Result<Option<i64>, StoreError> {
        Ok(self.next_wake)
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

    fn oldest(&self, _: FolderId) -> Result<Option<i64>, StoreError> {
        Ok(self.oldest)
    }

    fn older_done(&self, _: FolderId) -> Result<bool, StoreError> {
        Ok(false)
    }

    fn set_older_done(&self, folder: FolderId, done: bool) -> Result<(), StoreError> {
        self.older_done.lock().unwrap().push((folder, done));
        Ok(())
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

    fn save_found(
        &self,
        account: AccountId,
        text: &str,
        found: &[(FolderId, Vec<NewMessage>)],
    ) -> Result<(), StoreError> {
        let saved = (account, text.into(), found.to_vec());
        self.found.lock().unwrap().push(saved);
        Ok(())
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

    fn queue_all(&self, _: &[(MessageId, Action)], _: i64) -> Result<Vec<OpId>, StoreError> {
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

    fn land(&self, _: &[Op], moved: &Moved) -> Result<(), StoreError> {
        self.landed.lock().unwrap().push(moved.clone());
        Ok(())
    }

    fn unclaimed(&self, _: AccountId) -> Result<Vec<Folder>, StoreError> {
        Ok(self.unclaimed.clone())
    }

    fn queue_send(
        &self,
        _: &Draft,
        _: Option<DraftId>,
        _: &str,
        _: i64,
        _: i64,
    ) -> Result<OutboxId, StoreError> {
        Ok(OutboxId(1))
    }

    fn undo_send(&self, _: OutboxId, _: i64) -> Result<Option<SavedDraft>, StoreError> {
        Ok(None)
    }

    fn save_draft(&self, _: &SavedDraft) -> Result<DraftId, StoreError> {
        Ok(DraftId(1))
    }

    fn drafts(&self) -> Result<Vec<SavedDraft>, StoreError> {
        Ok(vec![])
    }

    fn delete_draft(&self, _: DraftId, _: i64, _: i64) -> Result<(), StoreError> {
        Ok(())
    }

    fn restore_draft(&self, _: DraftId, _: i64) -> Result<bool, StoreError> {
        Ok(false)
    }

    fn attach(
        &self,
        _: DraftId,
        _: &OutgoingFile,
        _: u64,
    ) -> Result<Option<DraftFile>, StoreError> {
        Err(StoreError::NotFound)
    }

    fn recent_files(&self, _: usize) -> Result<Vec<RecentFile>, StoreError> {
        Ok(vec![])
    }

    fn detach(&self, _: FileId) -> Result<(), StoreError> {
        Ok(())
    }

    fn outbox_files(&self, _: OutboxId) -> Result<Vec<OutgoingFile>, StoreError> {
        Ok(vec![])
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

    fn take_reverted(&self) -> Result<bool, StoreError> {
        let reverted = self.reverted.lock().unwrap().len();
        let mut told = self.told.lock().unwrap();
        let new = reverted > *told;
        *told = reverted;
        Ok(new)
    }
}
