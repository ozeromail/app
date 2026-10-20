//! Sends queued actions to the server.

use std::collections::BTreeMap;

use ozero_domain::{Action, MailProvider, Op, ProviderError, Store};

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
            op.action != Action::Empty,
            op.location.folder_remote_id.clone(),
            op.location.validity,
            op.action,
            op.to.clone(),
        );
        batches.entry(key).or_default().push(op);
    }
    // Emptying goes first, so what is still on its way into the folder stays; read state
    // goes before moves, so it is set while the messages are still here.
    for ((_, folder, validity, action, to), ops) in batches {
        let ids: Vec<_> = ops.iter().map(|o| o.location.remote_id.clone()).collect();
        let sent = match (action, to) {
            (Action::Seen(seen), _) => provider.set_seen(&folder, validity, &ids, seen).await,
            (Action::Archive | Action::Trash, Some(to)) => {
                provider.move_to(&folder, validity, &ids, &to).await
            }
            (Action::Delete, _) => provider.delete(&folder, validity, &ids).await,
            (Action::Empty, _) => {
                provider
                    .empty(&folder, validity, &store.others(&ops)?)
                    .await
            }
            (Action::Archive | Action::Trash, None) => {
                Err(ProviderError::Protocol("no folder to move to".into()))
            }
        };
        match sent {
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
