//! Sends the outbox.

use ozero_domain::{
    AccountId, Address, MailProvider, MailSender, MessageBuilder, MessageId, MessageParser,
    Outgoing, OutgoingFile, ProviderError, Store, StoreError,
};

use crate::sync::{SyncError, download_whole};

/// Where a sent copy goes when the server does not keep one itself.
#[derive(Debug)]
pub struct SentCopy<'a, P> {
    pub provider: &'a mut P,
    pub folder: &'a str,
}

/// Hands each message to the server, then saves a copy into Sent. Stops at the first network
/// or login failure; what is left stays queued. A message the server refuses stays in the
/// outbox, not sent again.
pub async fn push_sends<P: MailProvider>(
    store: &dyn Store,
    sender: &mut impl MailSender,
    builder: &dyn MessageBuilder,
    from: &Address,
    mut copy: Option<SentCopy<'_, P>>,
    messages: Vec<Outgoing>,
) -> Result<(), SyncError> {
    for message in messages {
        let raw = builder.build(from, &message);
        match sender.send(&from.email, &message.to, &raw).await {
            Ok(()) => {
                // Recorded first: a failed copy must not send the message twice.
                store.sent(message.id)?;
                if let Some(copy) = copy.as_mut() {
                    // The message is out either way; only the copy in Sent is missing.
                    let _ = copy.provider.append(copy.folder, &raw).await;
                }
            }
            Err(ProviderError::Rejected(reason)) => store.refused(message.id, &reason)?,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// The files of a message forwarded from `account`, from one download of it whole. None
/// once it is gone from the device or the server, so the text still goes; none for a
/// message of another account, whose server `provider` is not.
pub async fn forwarded_files(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    account: AccountId,
    message: MessageId,
) -> Result<Vec<OutgoingFile>, SyncError> {
    let location = match store.location(message) {
        Ok(location) if location.account_id == account => location,
        Ok(_) | Err(StoreError::NotFound) => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let (raw, files) = match download_whole(store, provider, parser, &location).await {
        Err(SyncError::Provider(ProviderError::Gone)) => return Ok(vec![]),
        whole => whole?,
    };
    let parts: Vec<u32> = files.iter().map(|f| f.part).collect();
    let found = parser.attachments(&raw, &parts);
    Ok(files
        .into_iter()
        .zip(found)
        .filter_map(|(file, data)| {
            Some(OutgoingFile {
                name: file.name,
                mime: file.mime,
                data: data?,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests;
