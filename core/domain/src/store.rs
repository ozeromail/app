use std::fmt;

use crate::{
    Account, AccountId, Action, Body, Category, Changes, Checkpoint, Correspondents, Draft,
    DraftFile, DraftId, FileId, Folder, FolderId, ListQuery, MessageId, MessageLocation,
    MessageRow, Moved, NewAccount, NewFolder, NewMessage, Op, OpId, OutboxId, Outgoing,
    OutgoingFile, RecentFile, SavedDraft, Unsent,
};

/// Local database port. The UI reads only from here; sync writes here.
pub trait Store: Send + Sync {
    fn add_account(&self, account: &NewAccount) -> Result<AccountId, StoreError>;
    fn accounts(&self) -> Result<Vec<Account>, StoreError>;
    /// Deletes the account with all its folders, mail and queued work.
    fn remove_account(&self, account: AccountId) -> Result<(), StoreError>;
    /// Sets the account's colour, an index into the apps' palette.
    fn set_account_color(&self, account: AccountId, color: u32) -> Result<(), StoreError>;
    /// Sets the name the account's mail goes out under.
    fn set_account_name(&self, account: AccountId, name: &str) -> Result<(), StoreError>;
    /// Sets the user's name for the account; empty clears it.
    fn set_account_label(&self, account: AccountId, label: &str) -> Result<(), StoreError>;
    /// Sets how many days back the account's mail is kept; 0 keeps all of it.
    fn set_account_keep(&self, account: AccountId, days: u32) -> Result<(), StoreError>;
    /// Forgets the account's mail received before `before` (seconds), but for mail a queued
    /// action or message needs.
    fn prune(&self, account: AccountId, before: i64) -> Result<(), StoreError>;
    /// Marks whether the account waits for the person to sign in again.
    fn set_needs_sign_in(&self, account: AccountId, needs: bool) -> Result<(), StoreError>;
    /// What the account's provider kept under `key`.
    fn provider_state(&self, account: AccountId, key: &str) -> Result<Option<String>, StoreError>;
    /// Keeps `value` for the account's provider under `key`; None forgets it.
    fn set_provider_state(
        &self,
        account: AccountId,
        key: &str,
        value: Option<&str>,
    ) -> Result<(), StoreError>;

    /// Inserts new folders and updates known ones, matched by `remote_id`.
    fn save_folders(
        &self,
        account: AccountId,
        folders: &[NewFolder],
    ) -> Result<Vec<Folder>, StoreError>;
    fn folders(&self, account: AccountId) -> Result<Vec<Folder>, StoreError>;

    /// Where the folder's last sync stopped, and the `remote_id`s of its saved messages.
    fn checkpoint(&self, folder: FolderId) -> Result<(Checkpoint, Vec<String>), StoreError>;
    /// When the folder's oldest saved message was received, if it has any.
    fn oldest(&self, folder: FolderId) -> Result<Option<i64>, StoreError>;
    /// Whether the folder's server had no older mail to load, or none the account keeps. A
    /// new validity, or a longer time the account keeps mail, makes it false again.
    fn older_done(&self, folder: FolderId) -> Result<bool, StoreError>;
    fn set_older_done(&self, folder: FolderId, done: bool) -> Result<(), StoreError>;
    /// Saves a sync in one transaction. A new validity first drops the folder's messages:
    /// their `remote_id`s now name other messages. Messages are matched by `remote_id`,
    /// and a queued action stays on top of what the server sent until it is sent. A new
    /// message joins the conversations, in its account, of the messages it references, that
    /// reference it, or that reference the same ones; conversations it joins become one.
    fn save_changes(
        &self,
        folder: FolderId,
        changes: &Changes<NewMessage>,
    ) -> Result<(), StoreError>;
    /// One row per conversation in a folder and inbox tab: its newest message in the list.
    fn list(&self, query: &ListQuery) -> Result<Vec<MessageRow>, StoreError>;
    /// The conversation of a message in its folder and inbox tab, with the user's replies from
    /// the sent folder, oldest first, leaving out what is being archived.
    fn thread(&self, message: MessageId) -> Result<Vec<MessageRow>, StoreError>;
    /// Unread messages in each tab of the unified inbox, leaving out those being archived.
    /// A tab without any is missing.
    fn inbox_unread(&self) -> Result<Vec<(Category, u32)>, StoreError>;
    /// Whether no tab of the unified inbox has a message, leaving out those being archived.
    fn inbox_is_empty(&self) -> Result<bool, StoreError>;
    /// Messages, newest first, whose subject, sender or body has every word of `text`, each
    /// as a word or the start of one, case and accents aside. Trash, junk and what is being
    /// archived are left out; empty `text` finds nothing.
    /// Also the messages the last server search of their account found for the same words.
    fn search(&self, text: &str, limit: u32) -> Result<Vec<MessageRow>, StoreError>;
    /// Saves what a server search of the account's folders found for `text`, in place of what
    /// its last one found. A message not saved yet is kept apart from its folder's list, and
    /// goes with the next search unless a queued action or message needs it.
    fn save_found(
        &self,
        account: AccountId,
        text: &str,
        found: &[(FolderId, Vec<NewMessage>)],
    ) -> Result<(), StoreError>;
    /// Unread People mail in an inbox saved since the last call and dated after `since`
    /// (seconds), newest first, for a notification. Everything saved stops being new.
    fn take_fresh(&self, since: i64) -> Result<Vec<MessageRow>, StoreError>;
    /// Like `take_fresh` without reading the mail: it is on screen already.
    fn clear_fresh(&self) -> Result<(), StoreError>;

    /// Whether the message's remote images load without asking: mail from people, and from
    /// senders whose images the user chose to see; never in junk.
    fn shows_images(&self, message: MessageId) -> Result<bool, StoreError>;
    /// From now on the message's sender's images load without asking.
    fn allow_images(&self, message: MessageId) -> Result<(), StoreError>;
    /// Who the message came from and went to.
    fn correspondents(&self, message: MessageId) -> Result<Correspondents, StoreError>;
    fn location(&self, message: MessageId) -> Result<MessageLocation, StoreError>;
    /// Those of the account's `limit` newest inbox messages that have no saved body and are
    /// not being moved, newest first.
    fn without_body(
        &self,
        account: AccountId,
        limit: u32,
    ) -> Result<Vec<MessageLocation>, StoreError>;
    /// Saves the body and sets the message's snippet from its text, if it has any.
    fn save_body(&self, message: MessageId, body: &Body) -> Result<(), StoreError>;
    fn body(&self, message: MessageId) -> Result<Option<Body>, StoreError>;

    /// Applies the action to the message's conversation in its folder and inbox tab now, in one transaction,
    /// and queues it for the server from `not_before`: the message itself, and the others
    /// where it changes something; marking unread leaves the others alone. It replaces queued actions of the same kind. Lists leave
    /// out a message being moved, and show a copy of it in the folder it goes to until a sync
    /// of that folder brings the server's own; actions on the copy wait for that. A move fails
    /// with `NoArchive` or `NoTrash` if the account has no such folder, and with `NotAllowed`
    /// where the message's folder does not allow it.
    fn queue(
        &self,
        message: MessageId,
        action: Action,
        not_before: i64,
    ) -> Result<Vec<OpId>, StoreError> {
        self.queue_all(&[(message, action)], not_before)
    }
    /// As `queue` for each message and its action, all in one transaction: all of them or, on
    /// an error, none. A message whose conversation an earlier one already took along is left
    /// alone.
    fn queue_all(
        &self,
        actions: &[(MessageId, Action)],
        not_before: i64,
    ) -> Result<Vec<OpId>, StoreError>;
    /// Snoozes the messages' conversations, as `queue_all` with `Action::Snooze`, until `until`
    /// (seconds): each message comes back then, wherever its server put it in the snoozed
    /// folder.
    fn snooze(
        &self,
        messages: &[MessageId],
        until: i64,
        not_before: i64,
    ) -> Result<Vec<OpId>, StoreError>;
    /// Brings back the snoozed mail whose time came by `now` (seconds): queues it into the
    /// inbox from `not_before`, unread and dated `now`, so it shows on top. Returns the
    /// messages as the inbox shows them. Mail moved out of the snoozed folder meanwhile, or
    /// whose snooze was undone, stays where it is.
    fn wake(&self, now: i64, not_before: i64) -> Result<Vec<MessageRow>, StoreError>;
    /// When the next snoozed message comes back (seconds), if one is snoozed.
    fn next_wake(&self) -> Result<Option<i64>, StoreError>;
    /// Queues `Action::Empty` on every message of a folder, as `queue` would one by one, and
    /// `Action::Delete` on copies of mail moved there. Fails with `NotAllowed` for any folder
    /// but the trash or spam.
    fn queue_empty(&self, folder: FolderId, not_before: i64) -> Result<Vec<OpId>, StoreError>;
    /// The `remote_id`s of the messages saved in the folder of `ops` that they leave out.
    fn others(&self, ops: &[Op]) -> Result<Vec<String>, StoreError>;
    /// Takes back actions that are still held (their `not_before` is after `now`), so the
    /// server never sees them: all of them, or none if any may already be on its way.
    fn undo(&self, ops: &[OpId], now: i64) -> Result<bool, StoreError>;
    /// When the account's next held action or message may be sent, if one is held.
    fn next_send(&self, account: AccountId) -> Result<Option<i64>, StoreError>;
    /// The account's queued actions that may be sent at `now`, oldest first, but those on
    /// copies of moved mail.
    fn due(&self, account: AccountId, now: i64) -> Result<Vec<Op>, StoreError>;
    /// The server has the actions: forget them, unless the user changed one since `due`.
    /// A moved message is dropped: its copy shows it in the folder it went to.
    fn finish(&self, ops: &[Op]) -> Result<(), StoreError>;
    /// Gives the copies the moves of `ops` made the ids the server gave their messages, where
    /// the folder still has the validity they belong to: the copies are the server's messages
    /// from now on. Call it before `finish`.
    fn land(&self, ops: &[Op], moved: &Moved) -> Result<(), StoreError>;
    /// The account's folders with copies of moved mail whose move the server has, but whose
    /// ids it did not tell: a sync of them puts the server's own messages in their place, so
    /// actions on them can be sent.
    fn unclaimed(&self, account: AccountId) -> Result<Vec<Folder>, StoreError>;
    /// Queues a message to be sent from `not_before`, and forgets the saved draft it was
    /// written in, if any; that draft's files go with the message. Fails with `NotFound` if the account or the message it answers is
    /// gone.
    fn queue_send(
        &self,
        draft: &Draft,
        saved: Option<DraftId>,
        message_id: &str,
        date: i64,
        not_before: i64,
    ) -> Result<OutboxId, StoreError>;
    /// Takes back a message that is still held or was refused, so it can be edited: it becomes
    /// a saved draft, saved at `now`, with its files. None once it may be on its way.
    fn undo_send(&self, id: OutboxId, now: i64) -> Result<Option<SavedDraft>, StoreError>;

    /// Saves a message being written: a new one if it has no id, else over the saved one.
    /// Fails with `NotFound` if that one is gone, sent or thrown away, so it never comes back.
    fn save_draft(&self, draft: &SavedDraft) -> Result<DraftId, StoreError>;
    /// The saved drafts with their files, last saved first.
    fn drafts(&self) -> Result<Vec<SavedDraft>, StoreError>;
    /// Throws a saved draft away, with its files, at `until`; till then `restore_draft` brings
    /// it back, and it is gone everywhere else. Drafts whose time came by `now` go for good.
    /// One already gone is fine.
    fn delete_draft(&self, id: DraftId, now: i64, until: i64) -> Result<(), StoreError>;
    /// Brings back a draft thrown away whose time has not come at `now`. False if it is gone.
    fn restore_draft(&self, id: DraftId, now: i64) -> Result<bool, StoreError>;
    /// Adds a file to a saved draft, unless its files, the forwarded message's among them, would
    /// then take more than `limit` bytes: None then. Fails with `NotFound` if the draft is gone.
    fn attach(
        &self,
        draft: DraftId,
        file: &OutgoingFile,
        limit: u64,
    ) -> Result<Option<DraftFile>, StoreError>;
    /// The files of saved mail, newest first and each name and size once, at most `limit`;
    /// none from trash or spam.
    fn recent_files(&self, limit: usize) -> Result<Vec<RecentFile>, StoreError>;
    /// Takes a file off its draft; one already gone, or sent with its message, is left be.
    fn detach(&self, file: FileId) -> Result<(), StoreError>;
    /// The files added to a queued message, whole, in the order they were added.
    fn outbox_files(&self, id: OutboxId) -> Result<Vec<OutgoingFile>, StoreError>;
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
    /// Whether `revert` undid actions since this was last called.
    fn take_reverted(&self) -> Result<bool, StoreError>;
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
    /// The message's account has no folder with the trash role.
    NoTrash,
    /// The message's account has no snoozed folder yet.
    NoSnoozed,
    /// The action does not apply in the message's folder, like archiving from the archive.
    NotAllowed,
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
            Self::NoTrash => write!(f, "the account has no trash folder"),
            Self::NoSnoozed => write!(f, "the account has no snoozed folder"),
            Self::NotAllowed => write!(f, "the action does not apply in this folder"),
            Self::Storage(e) => write!(f, "storage error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}
