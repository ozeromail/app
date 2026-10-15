//! Brings server state into the store.

use std::fmt;

use ozero_domain::{
    AccountId, FolderRole, MailProvider, MessageParser, ProviderError, Store, StoreError,
};

/// Saves the account's folders, then the newest `limit` messages of its inbox.
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
    let recent = provider.recent(&inbox.remote_id, limit).await?;
    store.set_validity(inbox.id, recent.validity)?;
    let messages: Vec<_> = recent
        .messages
        .into_iter()
        .map(|m| parser.parse(m))
        .collect();
    store.save_messages(inbox.id, &messages)?;
    Ok(())
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
