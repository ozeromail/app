//! Brings server state into the store.

use std::fmt;

use ozero_domain::{
    AccountId, Body, FolderRole, MailProvider, MessageLocation, MessageParser, ProviderError,
    Store, StoreError,
};

/// Saves the account's folders, then what changed in its inbox since the last sync:
/// up to `limit` new messages, new flags and removed messages.
pub async fn sync_inbox(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    account: AccountId,
    limit: u32,
) -> Result<(), SyncError> {
    let folders = provider.folders().await?;
    let folders = store.save_folders(account, &folders)?;
    let inbox = folders
        .iter()
        .find(|f| f.role == FolderRole::Inbox)
        .ok_or(SyncError::NoInbox)?;
    let (since, known) = store.checkpoint(inbox.id)?;
    let changes = provider
        .changes(&inbox.remote_id, since, &known, limit)
        .await?;
    store.save_changes(inbox.id, &changes.map(|m| parser.parse(m)))?;
    Ok(())
}

/// Downloads a saved message's body and returns it; saves it unless it has no text,
/// so a message the parser could not read is tried again next time.
pub async fn download_body(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    message: &MessageLocation,
) -> Result<Body, SyncError> {
    let raw = provider
        .body(
            &message.folder_remote_id,
            message.validity,
            &message.remote_id,
        )
        .await?;
    let body = parser.parse_body(&raw);
    if !body.text.is_empty() {
        store.save_body(message.id, &body)?;
    }
    Ok(body)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    Provider(ProviderError),
    Store(StoreError),
    /// The server lists no inbox.
    NoInbox,
}

impl From<ProviderError> for SyncError {
    fn from(e: ProviderError) -> Self {
        Self::Provider(e)
    }
}

impl From<StoreError> for SyncError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::NoInbox => write!(f, "the server has no inbox"),
        }
    }
}

impl std::error::Error for SyncError {}

#[cfg(test)]
mod tests;
