//! Brings server state into the store.

use std::fmt;
use std::time::Duration;

use ozero_domain::{
    AccountId, Attachment, Body, Changes, Folder, FolderRole, MailProvider, MessageLocation,
    MessageParser, ProviderError, SNOOZED, Store, StoreError,
};

/// Forgets the account's mail received before `since` (seconds), then saves its folders and
/// what changed in its inbox, sent and snoozed folders since the last sync: up to `limit` new
/// messages in the inbox and fewer in the others, new flags and removed messages.
/// The sent folder puts the user's own replies into conversations; the snoozed folder, made
/// on the server when it has none, holds the mail to bring back.
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
    let mut folders = store.save_folders(account, &folders)?;
    let inbox = inbox(&folders)?.clone();
    sync_folder(store, provider, parser, &inbox, limit, since).await?;
    if !folders.iter().any(|f| f.role == FolderRole::Snoozed) {
        // Snooze waits for it; the next sync tries again.
        for name in SNOOZED {
            match provider.create_snoozed(name).await {
                Ok(made) => {
                    folders.extend(store.save_folders(account, &[made])?);
                    break;
                }
                // The server refused the name: the next one may do.
                Err(ProviderError::Rejected(_) | ProviderError::Protocol(_)) => {}
                Err(_) => break,
            }
        }
    }
    // Neither stops the inbox: sent is context for conversations, and snoozed mail comes
    // back from what is saved of it.
    // The snoozed folder only while something waits there.
    let snoozed = store.next_wake()?.is_some();
    for role in [FolderRole::Sent, FolderRole::Snoozed] {
        if role == FolderRole::Snoozed && !snoozed {
            continue;
        }
        if let Some(folder) = folders.iter().find(|f| f.role == role) {
            let _ = sync_folder(store, provider, parser, folder, limit / SIDE_SHARE, since).await;
        }
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
/// older mail to save, which the store then remembers.
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
    if !more {
        store.set_older_done(folder.id, true)?;
    }
    Ok(more)
}

/// Asks the server for the newest `limit` messages of each of the account's folders that
/// have every word of `text`, and saves them as what its search found. Trash, spam and
/// drafts are left out, as local search leaves them; so is a folder reset since its sync.
/// Returns how many results the folders gave, a message in two folders counted twice.
pub async fn search_server(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    account: AccountId,
    text: &str,
    limit: u32,
) -> Result<usize, SyncError> {
    let mut found = vec![];
    for folder in store.folders(account)? {
        if !matches!(
            folder.role,
            FolderRole::Inbox | FolderRole::Archive | FolderRole::Sent | FolderRole::Custom
        ) {
            continue;
        }
        let (checkpoint, _) = store.checkpoint(folder.id)?;
        let messages = match provider
            .search(&folder.remote_id, checkpoint.validity, text, limit)
            .await
        {
            Ok(messages) => messages,
            // One folder's refusal, such as a charset it lacks, leaves the others' results.
            Err(ProviderError::Gone | ProviderError::Protocol(_) | ProviderError::Rejected(_)) => {
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        found.push((
            folder.id,
            messages.into_iter().map(|m| parser.parse(m)).collect(),
        ));
    }
    store.save_found(account, text, &found)?;
    Ok(found.iter().map(|(_, m)| m.len()).sum())
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

/// The sent and snoozed folders get this fraction of the inbox's limit.
const SIDE_SHARE: u32 = 4;

async fn sync_folder(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    folder: &Folder,
    limit: u32,
    since: i64,
) -> Result<(), SyncError> {
    let (checkpoint, known) = store.checkpoint(folder.id)?;
    let oldest = store.oldest(folder.id)?.unwrap_or(i64::MAX);
    let mut changes = provider
        .changes(&folder.remote_id, checkpoint, &known, oldest, limit)
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

/// Downloads a saved message whole, once, and returns these attachment `parts` of it,
/// decoded, in order; None for a part it has not. Saves the body again if its attachments
/// differ, so those a cut body left out are listed from now on.
pub async fn download_attachments(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    message: &MessageLocation,
    parts: &[u32],
) -> Result<Vec<Option<Vec<u8>>>, SyncError> {
    let (raw, _) = download_whole(store, provider, parser, message).await?;
    Ok(parser.attachments(&raw, parts))
}

/// Downloads a saved message whole, and returns it with the files it lists; saves the body
/// again if those differ from the saved ones.
pub async fn download_whole(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    message: &MessageLocation,
) -> Result<(Vec<u8>, Vec<Attachment>), SyncError> {
    let raw = provider
        .message(
            &message.folder_remote_id,
            message.validity,
            &message.remote_id,
        )
        .await?;
    let body = parser.parse_body(&raw);
    let saved = store.body(message.id)?.map(|b| b.attachments);
    if !raw.is_empty() && saved.as_ref() != Some(&body.attachments) {
        store.save_body(message.id, &body)?;
    }
    Ok((raw, body.attachments))
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
