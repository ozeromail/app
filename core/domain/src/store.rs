use std::fmt;

use crate::{
    Account, AccountId, Action, Body, Category, Changes, Checkpoint, Draft, Folder, FolderId,
    ListQuery, MessageId, MessageLocation, MessageRow, NewAccount, NewFolder, NewMessage, Op, OpId,
    OutboxId, Outgoing, Unsent,
};

/// Local database port. The UI reads only from here; sync writes here.
pub trait Store: Send + Sync {
    fn add_account(&self, account: &NewAccount) -> Result<AccountId, StoreError>;
    fn accounts(&self) -> Result<Vec<Account>, StoreError>;

    /// Inserts new folders and updates known ones, matched by `remote_id`.
    fn save_folders(
        &self,
        account: AccountId,
        folders: &[NewFolder],
    ) -> Result<Vec<Folder>, StoreError>;
    fn folders(&self, account: AccountId) -> Result<Vec<Folder>, StoreError>;

    /// Where the folder's last sync stopped, and the `remote_id`s of its saved messages.
    fn checkpoint(&self, folder: FolderId) -> Result<(Checkpoint, Vec<String>), StoreError>;
    /// Saves a sync in one transaction. A new validity first drops the folder's messages:
    /// their `remote_id`s now name other messages. Messages are matched by `remote_id`,
    /// and a queued action stays on top of what the server sent until it is sent.
    fn save_changes(
        &self,
        folder: FolderId,
        changes: &Changes<NewMessage>,
    ) -> Result<(), StoreError>;
    fn list(&self, query: &ListQuery) -> Result<Vec<MessageRow>, StoreError>;
    /// Unread messages in each tab of the unified inbox, leaving out those being archived.
    /// A tab without any is missing.
    fn inbox_unread(&self) -> Result<Vec<(Category, u32)>, StoreError>;
    /// Unread People mail in an inbox saved since the last call and dated after `since`
    /// (seconds), newest first, for a notification. Everything saved stops being new.
    fn take_fresh(&self, since: i64) -> Result<Vec<MessageRow>, StoreError>;
    /// Like `take_fresh` without reading the mail: it is on screen already.
    fn clear_fresh(&self) -> Result<(), StoreError>;

    fn location(&self, message: MessageId) -> Result<MessageLocation, StoreError>;
    /// Saves the body and sets the message's snippet from it.
    fn save_body(&self, message: MessageId, body: &Body) -> Result<(), StoreError>;
    fn body(&self, message: MessageId) -> Result<Option<Body>, StoreError>;

    /// Applies the action to the message now and queues it for the server from `not_before`.
    /// It replaces a queued action of the same kind. Lists leave out a message being archived;
    /// archiving fails with `NoArchive` if the account has no archive folder.
    fn queue(
        &self,
        message: MessageId,
        action: Action,
        not_before: i64,
    ) -> Result<OpId, StoreError>;
    /// Takes back an action that is still held (its `not_before` is after `now`), so the server
    /// never sees it. Returns false if it may already be on its way.
    fn undo(&self, op: OpId, now: i64) -> Result<bool, StoreError>;
    /// When the account's next held action or message may be sent, if one is held.
    fn next_send(&self, account: AccountId) -> Result<Option<i64>, StoreError>;
    /// The account's queued actions that may be sent at `now`, oldest first.
    fn due(&self, account: AccountId, now: i64) -> Result<Vec<Op>, StoreError>;
    /// The server has the actions: forget them, unless the user changed one since `due`.
    /// An archived message is dropped: it now lives in a folder that is synced on its own.
    fn finish(&self, ops: &[Op]) -> Result<(), StoreError>;
    /// Queues a message to be sent from `not_before`. Fails with `NotFound` if the account or
    /// the message it answers is gone.
    fn queue_send(
        &self,
        draft: &Draft,
        message_id: &str,
        date: i64,
        not_before: i64,
    ) -> Result<OutboxId, StoreError>;
    /// Takes back a message that is still held or was refused, so it can be edited. None once
    /// it may be on its way.
    fn undo_send(&self, id: OutboxId, now: i64) -> Result<Option<Draft>, StoreError>;
    /// The messages the server refused, newest first.
    fn unsent(&self) -> Result<Vec<Unsent>, StoreError>;
    /// The account's messages that may be sent at `now`, oldest first; refused ones stay out.
    fn due_sends(&self, account: AccountId, now: i64) -> Result<Vec<Outgoing>, StoreError>;
    /// The server took the message.
    fn sent(&self, id: OutboxId) -> Result<(), StoreError>;
    /// The server refused the message for good. It stays in the outbox, not sent again.
    fn refused(&self, id: OutboxId, reason: &str) -> Result<(), StoreError>;
    /// The server refused the actions for good: undo them on the device and forget them.
    /// Ops gone or changed since `due` are left alone.
    fn revert(&self, ops: &[Op]) -> Result<(), StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The database exists but the key does not open it.
    WrongKey,
    /// The database was written by a newer app version.
    TooNew {
        version: u32,
    },
    NotFound,
    /// The message's account has no folder with the archive role.
    NoArchive,
    /// Anything else from the storage engine, without message contents.
    Storage(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongKey => write!(f, "the database key is wrong"),
            Self::TooNew { version } => {
                write!(f, "database schema {version} is newer than this app")
            }
            Self::NotFound => write!(f, "not found"),
            Self::NoArchive => write!(f, "the account has no archive folder"),
            Self::Storage(e) => write!(f, "storage error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}
