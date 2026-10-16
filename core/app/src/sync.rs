//! Brings server state into the store.

use std::fmt;

use ozero_domain::{
    AccountId, Body, Folder, FolderRole, MailProvider, MessageLocation, MessageParser,
    ProviderError, Store, StoreError,
};

/// Saves the account's folders, then what changed in its inbox and its sent folder since
/// the last sync: up to `limit` new messages in the inbox and fewer in sent, new flags and
/// removed messages.
/// The sent folder puts the user's own replies into conversations.
pub async fn sync_mail(
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
    sync_folder(store, provider, parser, inbox, limit).await?;
    // Only context for conversations: a broken sent folder must not stop the inbox.
    if let Some(sent) = folders.iter().find(|f| f.role == FolderRole::Sent) {
        let _ = sync_folder(store, provider, parser, sent, limit / SENT_SHARE).await;
    }
    Ok(())
}

/// The sent folder gets this fraction of the inbox's limit.
const SENT_SHARE: u32 = 4;

async fn sync_folder(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    folder: &Folder,
    limit: u32,
) -> Result<(), SyncError> {
    let (since, known) = store.checkpoint(folder.id)?;
    let changes = provider
        .changes(&folder.remote_id, since, &known, limit)
        .await?;
    store.save_changes(folder.id, &changes.map(|m| parser.parse(m)))?;
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
