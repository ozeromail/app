use std::fmt;
use std::future::Future;

use crate::{Flags, NewFolder, NewMessage};

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
}

/// Turns what a provider returned into what the store keeps.
pub trait MessageParser: Send + Sync {
    fn parse(&self, raw: RawMessage) -> NewMessage;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// The server refused the login or token.
    Auth,
    /// Connection failed, broke or timed out.
    Network(String),
    /// The server answered something we cannot use.
    Protocol(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth => write!(f, "the server refused the login"),
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Protocol(e) => write!(f, "protocol error: {e}"),
        }
    }
}

impl std::error::Error for ProviderError {}
