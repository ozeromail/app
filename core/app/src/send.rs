//! Sends the outbox.

use ozero_domain::{
    Address, MailProvider, MailSender, MessageBuilder, Outgoing, ProviderError, Store,
};

use crate::sync::SyncError;

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
            Err(ProviderError::Protocol(reason)) => store.refused(message.id, &reason)?,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
