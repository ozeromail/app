//! Keeps drafts in their account's drafts folder, and brings in those written elsewhere.

use std::collections::BTreeMap;
use std::fmt;

use ozero_domain::{
    AccountId, Address, DraftCopy, DraftToPush, MailProvider, MessageBuilder, MessageParser,
    ProviderError, RawMessage, SavedDraft, Store,
};

use crate::sync::SyncError;

/// Where `push_drafts` saves drafts, and how it names them.
pub struct DraftsFolder<'a> {
    pub account: AccountId,
    pub from: &'a Address,
    pub folder: &'a str,
    /// Makes the `Message-ID` of a draft's first copy.
    pub message_id: &'a (dyn Fn() -> String + Sync),
}

impl fmt::Debug for DraftsFolder<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DraftsFolder")
            .field("account", &self.account)
            .field("folder", &self.folder)
            .finish_non_exhaustive()
    }
}

/// Saves a copy of each of `drafts` into the drafts folder, then deletes the copies left
/// stale, by `now` (ms): a new copy is in place before its old one goes. Stops at the first network or login
/// failure; what is left goes with the next push. A draft the server will not keep stays on
/// the device, until it is written again.
pub async fn push_drafts(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    builder: &dyn MessageBuilder,
    to: DraftsFolder<'_>,
    drafts: Vec<DraftToPush>,
    now: i64,
) -> Result<(), SyncError> {
    let DraftsFolder {
        account,
        from,
        folder,
        message_id,
    } = to;
    for draft in drafts {
        let mut message = draft.message;
        if message.message_id.is_empty() {
            message.message_id = message_id();
        }
        message.files = store.draft_files(draft.id)?;
        let raw = builder.build(from, &message);
        let copy = match provider.save_draft(folder, &raw, &message.message_id).await {
            Ok(place) => place.map(|(validity, remote_id)| DraftCopy {
                folder: folder.to_owned(),
                validity,
                remote_id,
            }),
            Err(ProviderError::Rejected(_) | ProviderError::Protocol(_)) => None,
            Err(e) => return Err(e.into()),
        };
        store.pushed_draft(
            account,
            draft.id,
            draft.saved,
            &message.message_id,
            copy.as_ref(),
        )?;
    }
    // One delete for each folder generation.
    let mut by_folder: BTreeMap<(String, u32), Vec<(i64, String)>> = BTreeMap::new();
    for (id, copy) in store.stale_drafts(account, now)? {
        by_folder
            .entry((copy.folder, copy.validity))
            .or_default()
            .push((id, copy.remote_id));
    }
    for ((folder, validity), copies) in by_folder {
        let (ids, remote_ids): (Vec<i64>, Vec<String>) = copies.into_iter().unzip();
        match provider.delete(&folder, validity, &remote_ids).await {
            // Gone already, or never to go: nothing more to do.
            Ok(())
            | Err(ProviderError::Gone | ProviderError::Rejected(_) | ProviderError::Protocol(_)) => {
                store.forget_stale_drafts(&ids)?
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// How many drafts written elsewhere a sync brings in, each with its files.
pub const DRAFT_IMPORTS: usize = 20;

/// Keeps on the device up to `limit` of the newest drafts written elsewhere, from the
/// account's drafts folder as the last sync saved it: their text and files, to go on
/// writing here. One gone from the server meanwhile is skipped.
pub async fn import_drafts(
    store: &dyn Store,
    provider: &mut impl MailProvider,
    parser: &dyn MessageParser,
    account: AccountId,
    limit: usize,
) -> Result<(), SyncError> {
    for found in store.server_drafts(account)?.into_iter().take(limit) {
        let copy = &found.copy;
        let raw = match provider
            .message(&copy.folder, copy.validity, &copy.remote_id)
            .await
        {
            Err(ProviderError::Gone) => continue,
            raw => raw?,
        };
        // The header alone, without the files.
        let end = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map_or(raw.len(), |at| at + 4);
        let header = parser.parse(RawMessage {
            remote_id: copy.remote_id.clone(),
            flags: Default::default(),
            received: 0,
            header: raw[..end].to_vec(),
        });
        let body = parser.parse_body(&raw);
        let parts: Vec<u32> = body.attachments.iter().map(|a| a.part).collect();
        let files = body
            .attachments
            .iter()
            .zip(parser.attachments(&raw, &parts))
            .filter_map(|(file, data)| {
                Some(ozero_domain::OutgoingFile {
                    name: file.name.clone(),
                    mime: file.mime.clone(),
                    data: data?,
                })
            })
            .collect::<Vec<_>>();
        let typed = |list: &[ozero_domain::Address]| {
            let emails: Vec<&str> = list.iter().map(|a| a.email.as_str()).collect();
            emails.join(", ")
        };
        let draft = SavedDraft {
            id: None,
            account_id: Some(account),
            to: typed(&header.to),
            cc: typed(&header.cc),
            subject: header.subject,
            text: body.text,
            signature: String::new(),
            reply_to: None,
            quote: String::new(),
            forward: None,
            saved: header.date.saturating_mul(1_000),
            files: vec![],
        };
        store.import_draft(&draft, found.message_id.as_deref(), copy, &files)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
