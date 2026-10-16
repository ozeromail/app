use std::fmt;
use std::future::Future;

use crate::{Body, Flags, NewFolder, NewMessage};

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

/// The newest messages of a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    /// When this changes, earlier `remote_id`s of the folder name other messages.
    /// IMAP sends it as UIDVALIDITY; providers with stable ids always return 0.
    pub validity: u32,
    /// Oldest first.
    pub messages: Vec<RawMessage>,
}

/// A mail server: IMAP, Gmail API or Microsoft Graph.
pub trait MailProvider: Send {
    fn folders(&mut self) -> impl Future<Output = Result<Vec<NewFolder>, ProviderError>> + Send;

    /// The newest `limit` messages of a folder.
    fn recent(
        &mut self,
        folder: &str,
        limit: u32,
    ) -> impl Future<Output = Result<Recent, ProviderError>> + Send;

    /// The raw message with `remote_id` in a folder, without marking it read; may be cut at a
    /// size limit. Fails with `Gone` if the folder's validity is no longer `validity`.
    fn body(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> impl Future<Output = Result<Vec<u8>, ProviderError>> + Send;

    /// Marks messages of a folder read or unread. Messages no longer there are skipped;
    /// fails with `Gone` if the folder's validity is no longer `validity`.
    fn set_seen(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        seen: bool,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;

    /// Moves messages of a folder into folder `to`, like `set_seen` otherwise.
    fn move_to(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> impl Future<Output = Result<(), ProviderError>> + Send;
}

/// Turns what a provider returned into what the store keeps.
pub trait MessageParser: Send + Sync {
    fn parse(&self, raw: RawMessage) -> NewMessage;
    fn parse_body(&self, raw: &[u8]) -> Body;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// The server refused the login or token.
    Auth,
    /// Connection failed, broke or timed out.
    Network(String),
    /// The server answered something we cannot use.
    Protocol(String),
    /// The message is no longer on the server under this id.
    Gone,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth => write!(f, "the server refused the login"),
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Protocol(e) => write!(f, "protocol error: {e}"),
            Self::Gone => write!(f, "the message is gone from the server"),
        }
    }
}

impl std::error::Error for ProviderError {}
