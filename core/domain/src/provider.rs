use std::fmt;
use std::future::Future;
use std::time::Duration;

use crate::{Address, Answer, Body, Flags, NewFolder, NewMessage, Outgoing};

/// A message as the server returns it, before MIME parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMessage {
    pub remote_id: String,
    pub flags: Flags,
    /// When the server received it, in seconds since the Unix epoch. Lists sort by this.
    pub received: i64,
    /// The header block of the message, as sent.
    pub header: Vec<u8>,
}

/// Where the last sync of a folder stopped. The default means it was never synced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Checkpoint {
    /// When this changes, earlier `remote_id`s of the folder name other messages.
    /// IMAP sends it as UIDVALIDITY; providers with stable ids always return 0.
    pub validity: u32,
    /// Where new messages start, as the provider counts: IMAP UIDNEXT. 0 when unknown.
    pub next: u64,
}

/// What changed in a folder since a checkpoint. `M` is the message as sent or as parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changes<M = RawMessage> {
    /// Where this sync stopped. A new `validity` makes every saved message of the folder stale.
    pub checkpoint: Checkpoint,
    /// New messages, oldest first.
    pub new: Vec<M>,
    /// The current flags of known messages still in the folder.
    pub flags: Vec<(String, Flags)>,
    /// Known messages no longer in the folder.
    pub gone: Vec<String>,
}

impl<M> Changes<M> {
    pub fn map<N>(self, f: impl FnMut(M) -> N) -> Changes<N> {
        Changes {
            checkpoint: self.checkpoint,
            new: self.new.into_iter().map(f).collect(),
            flags: self.flags,
            gone: self.gone,
        }
    }
}

/// Where moved messages are now: their ids in the folder they went to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Moved {
    /// The validity of that folder the new ids belong to; None where an id is the same in
    /// every folder, as in Gmail.
    pub validity: Option<u32>,
    /// Each message the server told about: its id before the move and after it.
    pub ids: Vec<(String, String)>,
}

/// Mail of a folder older than what is saved of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Older {
    /// Oldest first.
    pub new: Vec<RawMessage>,
    /// The folder may hold older mail still.
    pub more: bool,
}

/// A mail server: IMAP, Gmail API or Microsoft Graph.
pub trait MailProvider: Send {
    fn folders(&mut self) -> impl Future<Output = Result<Vec<NewFolder>, ProviderError>> + Send;

    /// What changed in a folder since `since`, for the messages saved with `known` ids, the
    /// earliest of which was received at `oldest` (seconds; `i64::MAX` with none saved).
    /// After a validity change, or without a checkpoint, `new` holds the newest `limit`
    /// messages and nothing else is returned. Otherwise `new` holds at most the newest
    /// `limit` messages that arrived since.
    fn changes(
        &mut self,
        folder: &str,
        since: Checkpoint,
        known: &[String],
        oldest: i64,
        limit: u32,
    ) -> impl Future<Output = Result<Changes, ProviderError>> + Send;

    /// Up to `limit` of the folder's messages just older than the saved ones, which have the
    /// `known` ids and the earliest of which was received at `oldest` (seconds; `i64::MAX`
    /// with none saved). Fails with `Gone` if the folder's validity is no longer `validity`.
    fn older(
        &mut self,
        folder: &str,
        validity: u32,
        known: &[String],
        oldest: i64,
        limit: u32,
    ) -> impl Future<Output = Result<Older, ProviderError>> + Send;

    /// The newest `limit` of the folder's messages that have every word of `text` in their
    /// header or body, oldest first. Fails with `Gone` if the folder's validity is no longer
    /// `validity`.
    fn search(
        &mut self,
        folder: &str,
        validity: u32,
        text: &str,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<RawMessage>, ProviderError>> + Send;

    /// The raw message with `remote_id` in a folder, without marking it read; may be cut at a
    /// size limit. Fails with `Gone` if the folder's validity is no longer `validity`.
    fn body(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> impl Future<Output = Result<Vec<u8>, ProviderError>> + Send;

    /// The whole raw message, like `body` but never cut: for its attachments. A provider
    /// whose `body` is never cut keeps this.
    fn message(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> impl Future<Output = Result<Vec<u8>, ProviderError>> + Send {
        self.body(folder, validity, remote_id)
    }

    /// Marks messages of a folder read or unread. Messages no longer there are skipped;
    /// fails with `Gone` if the folder's validity is no longer `validity`.
    fn set_seen(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        seen: bool,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;

    /// Flags messages of a folder or takes their flags off, like `set_seen` otherwise.
    fn set_flagged(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        flagged: bool,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;

    /// Moves messages of a folder into folder `to`, like `set_seen` otherwise, and returns
    /// the ids they have there, as far as the server tells.
    fn move_to(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> impl Future<Output = Result<Moved, ProviderError>> + Send;

    /// Removes messages of a folder from the server for good, like `set_seen` otherwise.
    fn delete(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;

    /// Removes everything in a folder from the server for good but the `keep` messages,
    /// mail the app has not seen included.
    fn empty(
        &mut self,
        folder: &str,
        validity: u32,
        keep: &[String],
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;

    /// Makes the folder snoozed mail waits in, named `name`, at the top level or where the
    /// server keeps the user's folders, and returns it; or the one there already.
    fn create_snoozed(
        &mut self,
        name: &str,
    ) -> impl Future<Output = Result<NewFolder, ProviderError>> + Send;

    /// Saves a message into a folder, marked read: a sent copy into Sent.
    fn append(
        &mut self,
        folder: &str,
        message: &[u8],
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;

    /// Saves a message being written into the drafts folder `folder`, marked as a draft and
    /// read, and returns its validity and id there, if the server tells. `message_id` is its
    /// `Message-ID`, to find it by. Fails with `Rejected` where the server keeps no drafts
    /// for the app.
    fn save_draft(
        &mut self,
        folder: &str,
        message: &[u8],
        message_id: &str,
    ) -> impl Future<Output = Result<Option<(u32, String)>, ProviderError>> + Send {
        let _ = (folder, message, message_id);
        async { Err(ProviderError::Rejected("drafts stay on the device".into())) }
    }

    /// Answers the meeting invitation `remote_id` of a folder in the calendar the server
    /// keeps for the person, as Exchange does. A provider without one keeps this, which
    /// does nothing: the answer goes to the organizer as mail. Fails with `Gone` if the
    /// invitation is no longer there.
    fn answer(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
        answer: Answer,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send {
        let _ = (folder, validity, remote_id, answer);
        async { Ok(()) }
    }

    /// Waits until a folder may have changed since `since`, at most `wait`: true if it may
    /// have, so a sync is worth it.
    fn wait(
        &mut self,
        folder: &str,
        since: Checkpoint,
        wait: Duration,
    ) -> impl Future<Output = Result<bool, ProviderError>> + Send;
}

/// What a provider keeps between sessions for one account, such as ActiveSync's sync keys.
pub trait ProviderState: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    /// Keeps `value` under `key`; None forgets it.
    fn set(&self, key: &str, value: Option<&str>);
}

/// Hands mail over for delivery: SMTP, or a provider's own send call.
pub trait MailSender: Send {
    /// Sends the whole message to `to`, from the account's address `from`.
    fn send(
        &mut self,
        from: &str,
        to: &[String],
        message: &[u8],
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;
}

/// Writes a queued message as MIME.
pub trait MessageBuilder: Send + Sync {
    fn build(&self, from: &Address, message: &Outgoing) -> Vec<u8>;
}

/// Turns what a provider returned into what the store keeps.
pub trait MessageParser: Send + Sync {
    fn parse(&self, raw: RawMessage) -> NewMessage;
    fn parse_body(&self, raw: &[u8]) -> Body;
    /// The decoded contents of these `Attachment::part`s of a raw message, in order; None for
    /// a part that is no attachment of it.
    fn attachments(&self, raw: &[u8], parts: &[u32]) -> Vec<Option<Vec<u8>>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// The server refused the login or token.
    Auth,
    /// The company's server has not let this device in, or has blocked it.
    Blocked,
    /// Connection failed, broke or timed out.
    Network(String),
    /// The server answered something we cannot use.
    Protocol(String),
    /// The server refused this for good; trying again gets the same answer.
    Rejected(String),
    /// The message is no longer on the server under this id.
    Gone,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth => write!(f, "the server refused the login"),
            Self::Blocked => write!(f, "the server has not let this device in"),
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Protocol(e) => write!(f, "protocol error: {e}"),
            Self::Rejected(e) => write!(f, "refused: {e}"),
            Self::Gone => write!(f, "the message is gone from the server"),
        }
    }
}

impl std::error::Error for ProviderError {}
