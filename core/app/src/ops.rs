//! Sends queued actions to the server.

use std::collections::BTreeMap;

use ozero_domain::{MailProvider, Op, ProviderError, Store};

use crate::sync::SyncError;

/// Sends actions to their server, one request per folder and action. Stops at the first
/// network or login failure; what is left stays queued for the next run.
pub async fn push_ops(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    ops: Vec<Op>,
) -> Result<(), SyncError> {
    let mut batches: BTreeMap<_, Vec<Op>> = BTreeMap::new();
    for op in ops {
        let key = (
            op.location.folder_remote_id.clone(),
            op.location.validity,
            op.action,
        );
        batches.entry(key).or_default().push(op);
    }
    for ((folder, validity, action), ops) in batches {
        let remote_ids: Vec<_> = ops.iter().map(|o| o.location.remote_id.clone()).collect();
        match provider.apply(&folder, validity, &remote_ids, action).await {
            // Gone: the messages are no longer there, and the next sync drops them.
            Ok(()) | Err(ProviderError::Gone) => store.finish(&ops)?,
            Err(ProviderError::Protocol(_)) => store.revert(&ops)?,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
