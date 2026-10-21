//! Brings server state into the store.

use std::fmt;
use std::time::Duration;

use ozero_domain::{
    AccountId, Body, Changes, Folder, FolderRole, MailProvider, MessageLocation, MessageParser,
    ProviderError, Store, StoreError,
};

/// Forgets the account's mail received before `since` (seconds), then saves its folders and
/// what changed in its inbox and its sent folder since the last sync: up to `limit` new
/// messages in the inbox and fewer in sent, new flags and removed messages.
/// The sent folder puts the user's own replies into conversations.
pub async fn sync_mail(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    account: AccountId,
    limit: u32,
    since: i64,
) -> Result<(), SyncError> {
    store.prune(account, since)?;
    let folders = provider.folders().await?;
    let folders = store.save_folders(account, &folders)?;
    let inbox = inbox(&folders)?;
    sync_folder(store, provider, parser, inbox, limit, since).await?;
    // Only context for conversations: a broken sent folder must not stop the inbox.
    if let Some(sent) = folders.iter().find(|f| f.role == FolderRole::Sent) {
        let _ = sync_folder(store, provider, parser, sent, limit / SENT_SHARE, since).await;
    }
    Ok(())
}

/// Brings what changed in these folders into the store, up to `limit` new messages each and
/// none received before `since`: a folder the user opens. The folder list is the one the last
/// `sync_mail` saved.
pub async fn sync_folders(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    folders: &[Folder],
    limit: u32,
    since: i64,
) -> Result<(), SyncError> {
    for folder in folders {
        sync_folder(store, provider, parser, folder, limit, since).await?;
    }
    Ok(())
}

/// Saves up to `limit` of the folder's messages just older than the saved ones, none received
/// before `since`: the next page of a list scrolled to its end. False once the folder has no
/// older mail to save.
pub async fn load_older(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    folder: &Folder,
    limit: u32,
    since: i64,
) -> Result<bool, SyncError> {
    let (checkpoint, known) = store.checkpoint(folder.id)?;
    let oldest = store.oldest(folder.id)?.unwrap_or(i64::MAX);
    let mut older = provider
        .older(
            &folder.remote_id,
            checkpoint.validity,
            &known,
            oldest,
            limit,
        )
        .await?;
    let all = older.new.len();
    older.new.retain(|m| m.received >= since);
    // Nothing new would ask for the same page again.
    let more = older.more && older.new.len() == all && !older.new.is_empty();
    let changes = Changes {
        checkpoint,
        new: older.new,
        flags: vec![],
        gone: vec![],
    };
    store.save_changes(folder.id, &changes.map(|m| parser.parse(m)))?;
    Ok(more)
}

/// Waits until the account's inbox may have changed since its last sync, at most `wait`:
/// true if it may have.
pub async fn wait_for_mail(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    account: AccountId,
    wait: Duration,
) -> Result<bool, SyncError> {
    let folders = store.folders(account)?;
    let inbox = inbox(&folders)?;
    let (since, _) = store.checkpoint(inbox.id)?;
    Ok(provider.wait(&inbox.remote_id, since, wait).await?)
}

fn inbox(folders: &[Folder]) -> Result<&Folder, SyncError> {
    folders
        .iter()
        .find(|f| f.role == FolderRole::Inbox)
        .ok_or(SyncError::NoInbox)
}

/// The sent folder gets this fraction of the inbox's limit.
const SENT_SHARE: u32 = 4;

async fn sync_folder(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    folder: &Folder,
    limit: u32,
    since: i64,
) -> Result<(), SyncError> {
    let (checkpoint, known) = store.checkpoint(folder.id)?;
    let mut changes = provider
        .changes(&folder.remote_id, checkpoint, &known, limit)
        .await?;
    changes.new.retain(|m| m.received >= since);
    store.save_changes(folder.id, &changes.map(|m| parser.parse(m)))?;
    Ok(())
}

/// Downloads a saved message's body and returns it; saves it unless the server sent
/// nothing, so a message with only attachments is not downloaded again.
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
    if !raw.is_empty() {
        store.save_body(message.id, &body)?;
    }
    Ok(body)
}

/// Downloads the missing bodies among the account's `limit` newest inbox messages, so they
/// open at once. A message gone from the server is skipped; any other failure stops it, and
/// the reader downloads what is left.
pub async fn prefetch_bodies(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    account: AccountId,
    limit: u32,
) -> Result<(), SyncError> {
    for message in store.without_body(account, limit)? {
        match download_body(store, provider, parser, &message).await {
            Ok(_) | Err(SyncError::Provider(ProviderError::Gone)) => {}
            Err(e) => return Err(e),
        }
    }
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
