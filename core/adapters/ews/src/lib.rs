//! Exchange Web Services provider (`MailProvider`, `MailSender`) for Exchange servers a
//! company runs itself, signed in with a password over Basic or NTLM; `EasSession` speaks
//! ActiveSync to a server that opens only that.
//!
//! Messages travel as MIME both ways. Item ids change when mail moves, so a folder has no
//! validity; its checkpoint is when its newest message came, for `wait`.

pub mod discover;

mod client;
mod eas;
mod md;
mod ntlm;
mod soap;
mod wbxml;
mod xml;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mail_parser::DateTime;
use ozero_domain::{
    Answer, Changes, Checkpoint, Flags, FolderRole, Listed, MailProvider, MailSender, Moved,
    NewFolder, Older, ProviderError, RawMessage, Server, base64, decode_base64,
};
use tokio_rustls::rustls::ClientConfig;

pub use crate::client::Client;
use crate::client::random;
pub use crate::eas::EasSession;
use crate::soap::{CONTENT_TYPE, check, envelope, first, messages};
use crate::xml::{Element, escape};

/// Where EWS lives on a server.
pub const PATH: &str = "/EWS/Exchange.asmx";
/// Most items one request names: Exchange's own limits are higher, answers stay small.
const MAX_IDS: usize = 100;
/// Most items one listing returns.
const PAGE: u32 = 1000;
/// How often `wait` asks whether new mail came.
const POLL: Duration = Duration::from_secs(60);

/// The folders the server names for what they mean.
const ROLES: [(&str, FolderRole); 5] = [
    ("inbox", FolderRole::Inbox),
    ("sentitems", FolderRole::Sent),
    ("drafts", FolderRole::Drafts),
    ("deleteditems", FolderRole::Trash),
    ("junkemail", FolderRole::Junk),
];
/// Mail folders that are no place for mail the person reads.
const SKIPPED: [&str; 3] = ["outbox", "syncissues", "conversationhistory"];
/// What Outlook calls the archive folder, which Exchange does not mark.
const ARCHIVE_NAMES: [&str; 2] = ["archive", "архив"];

/// Flag status (PR_FLAG_STATUS): 2 is flagged.
const FLAG_STATUS: &str = "0x1090";
/// The last thing done to a message (PR_LAST_VERB_EXECUTED): 102 replied, 103 replied to all.
const LAST_VERB: &str = "0x1081";
/// Hidden folders (PR_ATTR_HIDDEN).
const HIDDEN: &str = "0x10f4";

/// A mailbox on an Exchange server, signed in with a password.
#[derive(Debug)]
pub struct EwsSession {
    client: Client,
}

impl EwsSession {
    /// For the EWS endpoint at `server`, signed in as `server.user`.
    pub fn new(tls: Arc<ClientConfig>, server: &Server, password: &str) -> Self {
        Self::over(Client::new(
            tls,
            &server.host,
            server.port,
            PATH,
            &server.user,
            password,
        ))
    }

    pub(crate) fn over(client: Client) -> Self {
        Self { client }
    }

    /// The mailbox owner's name as the server knows it; None when it does not say.
    pub async fn name(&mut self, email: &str) -> Result<Option<String>, ProviderError> {
        let answer = self
            .call(
                &format!(
                    "<m:ResolveNames ReturnFullContactData=\"false\">\
<m:UnresolvedEntry>{}</m:UnresolvedEntry></m:ResolveNames>",
                    escape(email)
                ),
                true,
            )
            .await?;
        Ok(messages(&answer)
            .filter(|m| check(m).is_ok())
            .flat_map(|m| m.at(&["ResolutionSet"]).into_iter())
            .flat_map(|s| s.all("Resolution"))
            .find(|r| {
                r.text_at(&["Mailbox", "EmailAddress"])
                    .eq_ignore_ascii_case(email)
            })
            .map(|r| r.text_at(&["Mailbox", "Name"]).trim().to_owned())
            .filter(|n| !n.is_empty()))
    }

    /// Checks the login: the inbox is there to open.
    pub async fn check(&mut self) -> Result<(), ProviderError> {
        self.distinguished(&["inbox"]).await?;
        Ok(())
    }

    async fn call(&mut self, operation: &str, repeatable: bool) -> Result<Element, ProviderError> {
        let body = envelope(operation);
        let response = self
            .client
            .post(CONTENT_TYPE, body.as_bytes(), repeatable)
            .await?;
        soap::answer(&response)
    }

    /// The ids of the named folders that this mailbox has, by name.
    async fn distinguished(
        &mut self,
        names: &[&str],
    ) -> Result<HashMap<String, String>, ProviderError> {
        let ids: String = names
            .iter()
            .map(|n| format!("<t:DistinguishedFolderId Id=\"{n}\"/>"))
            .collect();
        let answer = self
            .call(
                &format!(
                    "<m:GetFolder><m:FolderShape><t:BaseShape>IdOnly</t:BaseShape></m:FolderShape>\
<m:FolderIds>{ids}</m:FolderIds></m:GetFolder>"
                ),
                true,
            )
            .await?;
        let mut found = HashMap::new();
        for (name, message) in names.iter().zip(messages(&answer)) {
            match check(message) {
                Ok(()) => {}
                // A mailbox may lack junk mail or sync issues.
                Err(ProviderError::Gone | ProviderError::Rejected(_)) if *name != "inbox" => {
                    continue;
                }
                Err(e) => return Err(e),
            }
            let id = message
                .at(&["Folders"])
                .and_then(|f| f.children.first())
                .and_then(|f| f.child("FolderId"))
                .and_then(|f| f.attr("Id"));
            if let Some(id) = id {
                found.insert((*name).to_owned(), id.to_owned());
            }
        }
        if !found.contains_key("inbox") {
            return Err(ProviderError::Protocol("no inbox".into()));
        }
        Ok(found)
    }

    /// One page of the folder's items, newest first, from `offset` on: their ids, flags and
    /// when they came, and whether older ones follow. `restriction` and `query` narrow it.
    async fn list(
        &mut self,
        folder: &str,
        restriction: &str,
        query: &str,
        offset: u32,
        count: u32,
    ) -> Result<(Vec<Item>, bool), ProviderError> {
        let query = if query.is_empty() {
            String::new()
        } else {
            format!("<m:QueryString>{}</m:QueryString>", escape(query))
        };
        let answer = self
            .call(
                &format!(
                    "<m:FindItem Traversal=\"Shallow\"><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape>\
<t:AdditionalProperties>{FLAG_FIELDS}<t:FieldURI FieldURI=\"item:DateTimeReceived\"/>\
</t:AdditionalProperties></m:ItemShape>\
<m:IndexedPageItemView MaxEntriesReturned=\"{count}\" Offset=\"{offset}\" BasePoint=\"Beginning\"/>\
{restriction}<m:SortOrder><t:FieldOrder Order=\"Descending\"><t:FieldURI FieldURI=\"item:DateTimeReceived\"/>\
</t:FieldOrder></m:SortOrder><m:ParentFolderIds>{}</m:ParentFolderIds>{query}</m:FindItem>",
                    folder_id(folder)
                ),
                true,
            )
            .await?;
        let (items, more) = page(&answer, "FindItem", "Items")?;
        Ok((items.into_iter().filter_map(Item::from).collect(), more))
    }

    /// Every item of the folder received at `since` or later, newest first.
    async fn list_since(
        &mut self,
        folder: &str,
        since: Option<i64>,
    ) -> Result<Vec<Item>, ProviderError> {
        let restriction = since.map_or(String::new(), |t| restrict("IsGreaterThanOrEqualTo", t));
        let mut all = Vec::new();
        loop {
            let (page, more) = self
                .list(folder, &restriction, "", all.len() as u32, PAGE)
                .await?;
            let empty = page.is_empty();
            all.extend(page);
            if !more || empty {
                return Ok(all);
            }
        }
    }

    /// The header and flags of each item still there, in no order.
    async fn headers(&mut self, ids: &[String]) -> Result<Vec<RawMessage>, ProviderError> {
        let mut found = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_IDS) {
            let answer = self
                .call(
                    &format!(
                        "<m:GetItem><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape>\
<t:AdditionalProperties>{FLAG_FIELDS}{HEADER_FIELDS}</t:AdditionalProperties></m:ItemShape>\
<m:ItemIds>{}</m:ItemIds></m:GetItem>",
                        item_ids(chunk)
                    ),
                    true,
                )
                .await?;
            // What was deleted between the list and here is left out.
            for message in went_through(&answer)?.into_iter().flatten() {
                found.extend(items(message).filter_map(raw_message));
            }
        }
        Ok(found)
    }

    /// Runs `operation`, made from each chunk of ids, over all of `ids`; items no longer
    /// there are skipped. The item each answer names, in order: None for a skipped one.
    async fn each_chunk(
        &mut self,
        ids: &[String],
        repeatable: bool,
        operation: impl Fn(&[String]) -> String,
    ) -> Result<Vec<Option<String>>, ProviderError> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_IDS) {
            let answer = self.call(&operation(chunk), repeatable).await?;
            for message in went_through(&answer)? {
                let id = message.and_then(|m| items(m).next()).and_then(item_id);
                out.push(id.map(str::to_owned));
            }
        }
        Ok(out)
    }

    /// Saves MIME `message` into a folder, or sends it with a copy in Sent; the id of the
    /// saved item, when the server tells it.
    async fn create(
        &mut self,
        message: &[u8],
        into: &str,
        how: Create,
    ) -> Result<Option<String>, ProviderError> {
        // PR_MESSAGE_FLAGS: MSGFLAG_READ, with MSGFLAG_UNSENT for a draft.
        let flags = |value: u32| {
            format!(
                "<t:ExtendedProperty><t:ExtendedFieldURI PropertyTag=\"0x0e07\" PropertyType=\"Integer\"/>\
<t:Value>{value}</t:Value></t:ExtendedProperty>"
            )
        };
        let (disposition, flags) = match how {
            Create::Send => ("SendAndSaveCopy", String::new()),
            Create::Read => ("SaveOnly", flags(1)),
            Create::Draft => ("SaveOnly", flags(9)),
        };
        let answer = self
            .call(
                &format!(
                    "<m:CreateItem MessageDisposition=\"{disposition}\"><m:SavedItemFolderId>{}</m:SavedItemFolderId>\
<m:Items><t:Message><t:MimeContent>{}</t:MimeContent>{flags}</t:Message></m:Items></m:CreateItem>",
                    folder_id(into),
                    base64(message)
                ),
                false,
            )
            .await?;
        let made = first(&answer, "CreateItem")?;
        Ok(items(made).next().and_then(item_id).map(str::to_owned))
    }
}

/// What `create` does with a message.
#[derive(Clone, Copy)]
enum Create {
    Send,
    /// Saved read.
    Read,
    /// Saved read, as a draft.
    Draft,
}

impl MailProvider for EwsSession {
    async fn folders(&mut self) -> Result<Vec<NewFolder>, ProviderError> {
        let names: Vec<&str> = ROLES.iter().map(|(n, _)| *n).chain(SKIPPED).collect();
        let named = self.distinguished(&names).await?;
        let role_of: HashMap<&str, FolderRole> = ROLES
            .iter()
            .filter_map(|(n, role)| Some((named.get(*n)?.as_str(), *role)))
            .collect();
        let skipped: HashSet<&str> = SKIPPED
            .iter()
            .filter_map(|n| named.get(*n).map(String::as_str))
            .collect();
        let mut found = Vec::new();
        loop {
            let answer = self
                .call(
                    &format!(
                        "<m:FindFolder Traversal=\"Deep\"><m:FolderShape><t:BaseShape>IdOnly</t:BaseShape>\
<t:AdditionalProperties><t:FieldURI FieldURI=\"folder:DisplayName\"/>\
<t:FieldURI FieldURI=\"folder:FolderClass\"/><t:FieldURI FieldURI=\"folder:ParentFolderId\"/>\
<t:ExtendedFieldURI PropertyTag=\"{HIDDEN}\" PropertyType=\"Boolean\"/></t:AdditionalProperties></m:FolderShape>\
<m:IndexedPageFolderView MaxEntriesReturned=\"{PAGE}\" Offset=\"{}\" BasePoint=\"Beginning\"/>\
<m:ParentFolderIds><t:DistinguishedFolderId Id=\"msgfolderroot\"/></m:ParentFolderIds></m:FindFolder>",
                        found.len()
                    ),
                    true,
                )
                .await?;
            let (page, more) = page(&answer, "FindFolder", "Folders")?;
            let empty = page.is_empty();
            found.extend(page.into_iter().filter_map(Entry::from));
            if !more || empty {
                break;
            }
        }
        let by_id: HashMap<&str, &Entry> = found.iter().map(|f| (f.id.as_str(), f)).collect();
        let mut folders = Vec::new();
        // At most one archive: the first found.
        let mut archived = false;
        for f in &found {
            let role = role_of.get(f.id.as_str()).copied();
            // Mail folders only: not calendars, contacts or tasks, not hidden ones.
            let mail = f.class.is_empty() || f.class.starts_with("IPF.Note");
            if role.is_none() && (!mail || f.hidden || skipped.contains(f.id.as_str())) {
                continue;
            }
            let (name, top) = path(&f.name, &f.parent, |id| {
                by_id.get(id).map(|p| (p.name.as_str(), p.parent.as_str()))
            });
            let archive =
                !archived && top && ARCHIVE_NAMES.contains(&f.name.to_lowercase().as_str());
            archived |= archive;
            let role = role.unwrap_or(if archive {
                FolderRole::Archive
            } else if top {
                FolderRole::from_name(&f.name)
            } else {
                FolderRole::Custom
            });
            folders.push(NewFolder {
                remote_id: f.id.clone(),
                name,
                role,
            });
        }
        Ok(folders)
    }

    async fn changes(
        &mut self,
        folder: &str,
        since: Checkpoint,
        known: &[String],
        oldest: i64,
        limit: u32,
    ) -> Result<Changes, ProviderError> {
        // Everything back to the oldest saved message: what is not there any more is gone. The
        // first time, only the newest, which says nothing of the rest.
        let first = since.next == 0 || known.is_empty() || oldest == i64::MAX;
        let listed = if first {
            self.list(folder, "", "", 0, limit).await?.0
        } else {
            self.list_since(folder, Some(oldest)).await?
        };
        let known: HashSet<&str> = known.iter().map(String::as_str).collect();
        let mut changes = Changes {
            checkpoint: Checkpoint {
                validity: 0,
                next: next(since.next, &listed),
            },
            new: vec![],
            flags: vec![],
            gone: vec![],
        };
        let mut seen = HashSet::new();
        let mut added = Vec::new();
        for item in listed {
            // Mail coming or going while it pages can list one twice.
            if !seen.insert(item.id.clone()) {
                continue;
            }
            if known.contains(item.id.as_str()) {
                changes.flags.push((item.id, item.flags));
            } else if added.len() < limit as usize {
                added.push(item.id);
            }
        }
        changes.gone = known
            .iter()
            .filter(|id| !first && !seen.contains(**id))
            .map(|id| (*id).to_owned())
            .collect();
        changes.new = self.headers(&added).await?;
        changes.new.sort_by_key(|m| m.received);
        Ok(changes)
    }

    async fn older(
        &mut self,
        folder: &str,
        _validity: u32,
        known: &[String],
        oldest: i64,
        limit: u32,
    ) -> Result<Older, ProviderError> {
        let restriction = match oldest {
            i64::MAX => String::new(),
            oldest => restrict("IsLessThanOrEqualTo", oldest),
        };
        let known: HashSet<&str> = known.iter().map(String::as_str).collect();
        let mut offset = 0;
        // A page of only saved mail, say many messages in the oldest saved second, goes on.
        let (ids, more) = loop {
            let (page, more) = self.list(folder, &restriction, "", offset, limit).await?;
            offset += page.len() as u32;
            let ids: Vec<String> = page
                .into_iter()
                .map(|i| i.id)
                .filter(|id| !known.contains(id.as_str()))
                .collect();
            if !ids.is_empty() || !more || offset == 0 {
                break (ids, more);
            }
        };
        let mut new = self.headers(&ids).await?;
        new.sort_by_key(|m| m.received);
        Ok(Older { new, more })
    }

    async fn search(
        &mut self,
        folder: &str,
        _validity: u32,
        text: &str,
        limit: u32,
    ) -> Result<Vec<RawMessage>, ProviderError> {
        // Each word as a phrase, so search syntax in the text stays plain words.
        let words: Vec<String> = text
            .split_whitespace()
            .map(|w| w.replace('"', ""))
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{w}\""))
            .collect();
        if words.is_empty() {
            return Ok(vec![]);
        }
        let (page, _) = self
            .list(folder, "", &words.join(" AND "), 0, limit)
            .await?;
        let ids: Vec<String> = page.into_iter().map(|i| i.id).collect();
        let mut found = self.headers(&ids).await?;
        found.sort_by_key(|m| m.received);
        Ok(found)
    }

    async fn body(
        &mut self,
        _folder: &str,
        _validity: u32,
        remote_id: &str,
    ) -> Result<Vec<u8>, ProviderError> {
        let answer = self
            .call(
                &format!(
                    "<m:GetItem><m:ItemShape><t:BaseShape>IdOnly</t:BaseShape>\
<t:IncludeMimeContent>true</t:IncludeMimeContent></m:ItemShape>\
<m:ItemIds>{}</m:ItemIds></m:GetItem>",
                    item_ids(&[remote_id.to_owned()])
                ),
                true,
            )
            .await?;
        let message = first(&answer, "GetItem")?;
        let mime = items(message)
            .next()
            .map_or("", |i| i.text_at(&["MimeContent"]));
        // Copied only if the server broke it into lines.
        let mime = if mime.contains(|c: char| c.is_ascii_whitespace()) {
            Cow::Owned(mime.split_ascii_whitespace().collect())
        } else {
            Cow::Borrowed(mime)
        };
        decode_base64(&mime)
            .filter(|m| !m.is_empty())
            .ok_or_else(|| protocol("bad MIME content"))
    }

    async fn set_seen(
        &mut self,
        _folder: &str,
        _validity: u32,
        remote_ids: &[String],
        seen: bool,
    ) -> Result<(), ProviderError> {
        let change = |id: &String| {
            format!(
                "<t:ItemChange><t:ItemId Id=\"{}\"/><t:Updates><t:SetItemField>\
<t:FieldURI FieldURI=\"message:IsRead\"/><t:Message><t:IsRead>{seen}</t:IsRead></t:Message>\
</t:SetItemField></t:Updates></t:ItemChange>",
                escape(id)
            )
        };
        self.each_chunk(remote_ids, true, |ids| {
            let changes: String = ids.iter().map(change).collect();
            format!(
                "<m:UpdateItem MessageDisposition=\"SaveOnly\" ConflictResolution=\"AlwaysOverwrite\" \
SuppressReadReceipts=\"true\"><m:ItemChanges>{changes}</m:ItemChanges></m:UpdateItem>"
            )
        })
        .await
        .map(drop)
    }

    async fn set_flagged(
        &mut self,
        _folder: &str,
        _validity: u32,
        remote_ids: &[String],
        flagged: bool,
    ) -> Result<(), ProviderError> {
        let status = if flagged { "Flagged" } else { "NotFlagged" };
        let change = |id: &String| {
            format!(
                "<t:ItemChange><t:ItemId Id=\"{}\"/><t:Updates><t:SetItemField>\
<t:FieldURI FieldURI=\"item:Flag\"/><t:Message><t:Flag><t:FlagStatus>{status}</t:FlagStatus></t:Flag>\
</t:Message></t:SetItemField></t:Updates></t:ItemChange>",
                escape(id)
            )
        };
        self.each_chunk(remote_ids, true, |ids| {
            let changes: String = ids.iter().map(change).collect();
            format!(
                "<m:UpdateItem MessageDisposition=\"SaveOnly\" ConflictResolution=\"AlwaysOverwrite\">\
<m:ItemChanges>{changes}</m:ItemChanges></m:UpdateItem>"
            )
        })
        .await
        .map(drop)
    }

    async fn move_to(
        &mut self,
        _folder: &str,
        _validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> Result<Moved, ProviderError> {
        let to = folder_id(to);
        // Not repeatable: a second move finds the mail gone and loses its new ids.
        let new = self
            .each_chunk(remote_ids, false, |ids| {
                format!(
                    "<m:MoveItem ReturnNewItemIds=\"true\"><m:ToFolderId>{to}</m:ToFolderId>\
<m:ItemIds>{}</m:ItemIds></m:MoveItem>",
                    item_ids(ids)
                )
            })
            .await?;
        Ok(Moved {
            validity: Some(0),
            ids: remote_ids
                .iter()
                .zip(new)
                .filter_map(|(old, new)| Some((old.clone(), new?)))
                .collect(),
        })
    }

    async fn delete(
        &mut self,
        _folder: &str,
        _validity: u32,
        remote_ids: &[String],
    ) -> Result<(), ProviderError> {
        // A soft delete leaves it where the server keeps deleted mail for a while, as Outlook does.
        self.each_chunk(remote_ids, true, |ids| {
            format!(
                "<m:DeleteItem DeleteType=\"SoftDelete\"><m:ItemIds>{}</m:ItemIds></m:DeleteItem>",
                item_ids(ids)
            )
        })
        .await
        .map(drop)
    }

    async fn empty(
        &mut self,
        folder: &str,
        _validity: u32,
        keep: &[String],
    ) -> Result<(), ProviderError> {
        let keep: HashSet<&str> = keep.iter().map(String::as_str).collect();
        let ids: Vec<String> = self
            .list_since(folder, None)
            .await?
            .into_iter()
            .map(|i| i.id)
            .filter(|id| !keep.contains(id.as_str()))
            .collect();
        self.delete(folder, 0, &ids).await
    }

    async fn create_snoozed(&mut self, name: &str) -> Result<NewFolder, ProviderError> {
        let answer = self
            .call(
                &format!(
                    "<m:CreateFolder><m:ParentFolderId><t:DistinguishedFolderId Id=\"msgfolderroot\"/>\
</m:ParentFolderId><m:Folders><t:Folder><t:FolderClass>IPF.Note</t:FolderClass>\
<t:DisplayName>{}</t:DisplayName></t:Folder></m:Folders></m:CreateFolder>",
                    escape(name)
                ),
                false,
            )
            .await?;
        // Refused also when it is there already, which the listing tells.
        let refused = match first(&answer, "CreateFolder") {
            Ok(made) => {
                let id = made
                    .at(&["Folders"])
                    .and_then(|f| f.children.first())
                    .and_then(|f| f.child("FolderId"))
                    .and_then(|f| f.attr("Id"))
                    .ok_or_else(|| protocol("no id for the new folder"))?;
                return Ok(NewFolder {
                    remote_id: id.to_owned(),
                    name: name.to_owned(),
                    role: FolderRole::Snoozed,
                });
            }
            Err(e @ ProviderError::Rejected(_)) => e,
            Err(e) => return Err(e),
        };
        let folders = self.folders().await?;
        folders
            .into_iter()
            .find(|f| f.role == FolderRole::Snoozed)
            .ok_or(refused)
    }

    async fn append(&mut self, folder: &str, message: &[u8]) -> Result<(), ProviderError> {
        self.create(message, folder, Create::Read).await.map(drop)
    }

    async fn save_draft(
        &mut self,
        folder: &str,
        message: &[u8],
        _message_id: &str,
    ) -> Result<Option<(u32, String)>, ProviderError> {
        let id = self.create(message, folder, Create::Draft).await?;
        Ok(id.map(|id| (0, id)))
    }

    async fn answer(
        &mut self,
        _folder: &str,
        _validity: u32,
        remote_id: &str,
        answer: Answer,
    ) -> Result<(), ProviderError> {
        let kind = match answer {
            Answer::Yes => "AcceptItem",
            Answer::Maybe => "TentativelyAcceptItem",
            Answer::No => "DeclineItem",
        };
        // Exchange puts the meeting in the calendar and tells the organizer, as Outlook does.
        // Not repeatable: a second answer would reach the organizer twice.
        let answer = self
            .call(
                &format!(
                    "<m:CreateItem MessageDisposition=\"SendAndSaveCopy\"><m:Items><t:{kind}>\
<t:ReferenceItemId Id=\"{}\"/></t:{kind}></m:Items></m:CreateItem>",
                    escape(remote_id)
                ),
                false,
            )
            .await?;
        first(&answer, "CreateItem").map(drop)
    }

    async fn directory(&mut self, text: &str, limit: u32) -> Result<Vec<Listed>, ProviderError> {
        let answer = self
            .call(
                &format!(
                    "<m:ResolveNames ReturnFullContactData=\"true\" SearchScope=\"ActiveDirectory\">\
<m:UnresolvedEntry>{}</m:UnresolvedEntry></m:ResolveNames>",
                    escape(text)
                ),
                true,
            )
            .await?;
        let mut found = Vec::new();
        for message in messages(&answer) {
            if message.text_at(&["ResponseCode"]) == "ErrorNameResolutionNoResults" {
                continue;
            }
            // Several matches come as a warning, which `check` lets through.
            check(message)?;
            let resolutions = message.at(&["ResolutionSet"]).into_iter();
            for r in resolutions.flat_map(|s| s.all("Resolution")) {
                let email = r.text_at(&["Mailbox", "EmailAddress"]).trim();
                if !email.contains('@') {
                    continue;
                }
                let name = [
                    r.text_at(&["Mailbox", "Name"]),
                    r.text_at(&["Contact", "DisplayName"]),
                ]
                .into_iter()
                .map(str::trim)
                .find(|n| !n.is_empty())
                .unwrap_or(email);
                let title = [
                    r.text_at(&["Contact", "JobTitle"]),
                    r.text_at(&["Contact", "Department"]),
                ]
                .into_iter()
                .map(str::trim)
                .find(|t| !t.is_empty());
                found.push(Listed {
                    name: name.to_owned(),
                    email: email.to_owned(),
                    title: title.map(str::to_owned),
                });
            }
        }
        found.truncate(limit as usize);
        Ok(found)
    }

    async fn wait(
        &mut self,
        folder: &str,
        since: Checkpoint,
        wait: Duration,
    ) -> Result<bool, ProviderError> {
        let until = Instant::now() + wait;
        loop {
            let (newest, _) = self.list(folder, "", "", 0, 1).await?;
            if next(0, &newest) > since.next {
                return Ok(true);
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            tokio::time::sleep(left.min(POLL)).await;
        }
    }
}

impl MailSender for EwsSession {
    async fn send(
        &mut self,
        _from: &str,
        _to: &[String],
        message: &[u8],
    ) -> Result<(), ProviderError> {
        // Exchange reads the recipients from the message and keeps a copy in Sent.
        self.create(message, "sentitems", Create::Send)
            .await
            .map(drop)
    }
}

/// The flags of a message: read, flagged, answered, draft.
const FLAG_FIELDS: &str = "<t:FieldURI FieldURI=\"message:IsRead\"/><t:FieldURI FieldURI=\"item:IsDraft\"/>\
<t:ExtendedFieldURI PropertyTag=\"0x1090\" PropertyType=\"Integer\"/>\
<t:ExtendedFieldURI PropertyTag=\"0x1081\" PropertyType=\"Integer\"/>";

/// What a message header is made of.
const HEADER_FIELDS: &str = "<t:FieldURI FieldURI=\"item:Subject\"/>\
<t:FieldURI FieldURI=\"item:DateTimeReceived\"/><t:FieldURI FieldURI=\"item:DateTimeSent\"/>\
<t:FieldURI FieldURI=\"item:InReplyTo\"/><t:FieldURI FieldURI=\"item:InternetMessageHeaders\"/>\
<t:FieldURI FieldURI=\"message:From\"/><t:FieldURI FieldURI=\"message:ToRecipients\"/>\
<t:FieldURI FieldURI=\"message:CcRecipients\"/><t:FieldURI FieldURI=\"message:InternetMessageId\"/>\
<t:FieldURI FieldURI=\"message:References\"/>";

/// An item of a listing.
#[derive(Debug, Clone)]
struct Item {
    id: String,
    flags: Flags,
    received: i64,
}

impl Item {
    fn from(item: &Element) -> Option<Self> {
        Some(Self {
            id: item_id(item)?.to_owned(),
            flags: flags(item),
            received: time(item.text_at(&["DateTimeReceived"])).unwrap_or(0),
        })
    }
}

/// A folder of a listing.
#[derive(Debug, Clone)]
struct Entry {
    id: String,
    name: String,
    class: String,
    parent: String,
    hidden: bool,
}

impl Entry {
    fn from(folder: &Element) -> Option<Self> {
        Some(Self {
            id: folder.child("FolderId")?.attr("Id")?.to_owned(),
            name: folder.text_at(&["DisplayName"]).to_owned(),
            class: folder.text_at(&["FolderClass"]).to_owned(),
            parent: folder
                .child("ParentFolderId")
                .and_then(|p| p.attr("Id"))
                .unwrap_or_default()
                .to_owned(),
            hidden: extended(folder, HIDDEN) == Some("true"),
        })
    }
}

/// Each response message: Some if it went through, None if its item is gone. Any other
/// error fails it all.
fn went_through(answer: &Element) -> Result<Vec<Option<&Element>>, ProviderError> {
    messages(answer)
        .map(|m| match check(m) {
            Ok(()) => Ok(Some(m)),
            Err(ProviderError::Gone) => Ok(None),
            Err(e) => Err(e),
        })
        .collect()
}

/// The entries of a listing's one page, and whether more pages follow.
fn page<'a>(
    answer: &'a Element,
    operation: &str,
    container: &str,
) -> Result<(Vec<&'a Element>, bool), ProviderError> {
    let root = first(answer, operation)?
        .child("RootFolder")
        .ok_or_else(|| protocol(format!("no RootFolder in {operation}")))?;
    let entries = root
        .child(container)
        .into_iter()
        .flat_map(|c| c.children.iter())
        .collect();
    Ok((
        entries,
        root.attr("IncludesLastItemInRange") == Some("false"),
    ))
}

/// The items an item response message holds.
fn items(message: &Element) -> impl Iterator<Item = &Element> {
    message
        .child("Items")
        .into_iter()
        .flat_map(|i| i.children.iter())
}

fn item_id(item: &Element) -> Option<&str> {
    item.child("ItemId")?.attr("Id")
}

fn item_ids(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("<t:ItemId Id=\"{}\"/>", escape(id)))
        .collect()
}

/// A folder by its id, or by the name the server gives it.
fn folder_id(folder: &str) -> String {
    if ROLES.iter().any(|(n, _)| *n == folder) {
        format!("<t:DistinguishedFolderId Id=\"{folder}\"/>")
    } else {
        format!("<t:FolderId Id=\"{}\"/>", escape(folder))
    }
}

/// A restriction on when items came, against `time` (seconds).
fn restrict(comparison: &str, time: i64) -> String {
    format!(
        "<m:Restriction><t:{comparison}><t:FieldURI FieldURI=\"item:DateTimeReceived\"/>\
<t:FieldURIOrConstant><t:Constant Value=\"{}\"/></t:FieldURIOrConstant></t:{comparison}></m:Restriction>",
        DateTime::from_timestamp(time).to_rfc3339()
    )
}

/// The checkpoint after a listing: past the newest item seen, never back.
fn next(since: u64, listed: &[Item]) -> u64 {
    let newest = listed.iter().map(|i| i.received).max().unwrap_or(0);
    since.max(u64::try_from(newest).unwrap_or(0) + 1)
}

/// The value of an extended property, by its tag as `0x1234`.
fn extended<'a>(element: &'a Element, tag: &str) -> Option<&'a str> {
    element
        .all("ExtendedProperty")
        .find(|p| {
            p.child("ExtendedFieldURI")
                .and_then(|u| u.attr("PropertyTag"))
                .is_some_and(|t| tag_number(t) == tag_number(tag))
        })
        .map(|p| p.text_at(&["Value"]))
}

/// A property tag as a number, whether written `0x0E07` or `3591`.
fn tag_number(tag: &str) -> Option<u32> {
    match tag.strip_prefix("0x").or(tag.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => tag.parse().ok(),
    }
}

fn flags(item: &Element) -> Flags {
    let verb = extended(item, LAST_VERB).and_then(|v| v.parse::<u32>().ok());
    Flags {
        // Mail without the field, as some reports, counts as read.
        seen: item.text_at(&["IsRead"]) != "false",
        flagged: extended(item, FLAG_STATUS) == Some("2"),
        answered: matches!(verb, Some(102 | 103)),
        draft: item.text_at(&["IsDraft"]) == "true",
    }
}

/// Seconds since the Unix epoch of a time as Exchange writes it, with or without fractions
/// of a second: `2026-10-26T10:00:00.000Z`.
fn time(text: &str) -> Option<i64> {
    let text = text.trim();
    let whole = match (text.split_once('.'), text.ends_with('Z')) {
        (Some((seconds, _)), true) => Cow::Owned(format!("{seconds}Z")),
        _ => Cow::Borrowed(text),
    };
    DateTime::parse_rfc3339(&whole).map(|t| t.to_timestamp())
}

/// A header block of these fields, one a line whatever line breaks a value holds; empty
/// ones are left out.
fn header<'a>(fields: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<u8> {
    let mut header = Vec::new();
    for (name, value) in fields {
        let value = value.replace(['\r', '\n'], " ");
        if !value.trim().is_empty() {
            header.extend_from_slice(format!("{name}: {}\r\n", value.trim()).as_bytes());
        }
    }
    header.extend_from_slice(b"\r\n");
    header
}

/// A folder's full path, so folders of one name under different parents tell apart, and
/// whether it is at the top. `parent_of` gives a folder's name and parent by its id.
fn path<'a>(
    name: &str,
    parent: &'a str,
    parent_of: impl Fn(&str) -> Option<(&'a str, &'a str)>,
) -> (String, bool) {
    let mut path = name.to_owned();
    let mut next = parent_of(parent);
    let top = next.is_none();
    let mut depth = 0;
    while let Some((name, parent)) = next
        && depth < 32
    {
        path = format!("{name}/{path}");
        next = parent_of(parent);
        depth += 1;
    }
    (path, top)
}

/// Headers the server adds on the way that only take room.
fn dropped(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "received" || name.starts_with("x-ms-") || name.starts_with("x-microsoft-")
}

/// A message's header and flags from its properties. The core fields come from the
/// properties, which the server has decoded; the rest, such as List-Id, as the message
/// arrived.
fn raw_message(item: &Element) -> Option<RawMessage> {
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut add = |name: &str, value: String| {
        if !value.trim().is_empty() {
            fields.push((name.to_owned(), value));
        }
    };
    let sent = time(item.text_at(&["DateTimeSent"]));
    add("Subject", item.text_at(&["Subject"]).to_owned());
    add(
        "From",
        item.at(&["From", "Mailbox"])
            .and_then(address)
            .unwrap_or_default(),
    );
    add(
        "To",
        addresses(item.child("ToRecipients")).unwrap_or_default(),
    );
    add(
        "Cc",
        addresses(item.child("CcRecipients")).unwrap_or_default(),
    );
    add(
        "Date",
        sent.map(|t| DateTime::from_timestamp(t).to_rfc822())
            .unwrap_or_default(),
    );
    add(
        "Message-ID",
        item.text_at(&["InternetMessageId"]).to_owned(),
    );
    add("In-Reply-To", item.text_at(&["InReplyTo"]).to_owned());
    add("References", item.text_at(&["References"]).to_owned());
    // A field the properties lack, such as an address the server could not give as SMTP,
    // comes from the header as it came.
    const OURS: [&str; 8] = [
        "subject",
        "from",
        "to",
        "cc",
        "date",
        "message-id",
        "in-reply-to",
        "references",
    ];
    for h in item
        .child("InternetMessageHeaders")
        .into_iter()
        .flat_map(|h| h.all("InternetMessageHeader"))
    {
        let Some(name) = h.attr("HeaderName") else {
            continue;
        };
        let known = OURS.contains(&name.to_ascii_lowercase().as_str())
            && fields.iter().any(|(n, _)| n.eq_ignore_ascii_case(name));
        if known || dropped(name) || name.is_empty() || name.contains([':', '\r', '\n', ' ']) {
            continue;
        }
        fields.push((name.to_owned(), h.text.clone()));
    }
    let header = header(fields.iter().map(|(n, v)| (n.as_str(), v.as_str())));
    Some(RawMessage {
        remote_id: item_id(item)?.to_owned(),
        flags: flags(item),
        received: time(item.text_at(&["DateTimeReceived"])).unwrap_or(0),
        header,
    })
}

/// A mailbox as `"Name" <address>`; None when the server has no SMTP address for it.
fn address(mailbox: &Element) -> Option<String> {
    let email = mailbox.text_at(&["EmailAddress"]).trim();
    let routing = mailbox.text_at(&["RoutingType"]);
    if !email.contains('@') || !(routing.is_empty() || routing.eq_ignore_ascii_case("SMTP")) {
        return None;
    }
    let name = mailbox.text_at(&["Name"]).trim();
    Some(if name.is_empty() || name.eq_ignore_ascii_case(email) {
        format!("<{email}>")
    } else {
        let name = name.replace(['\\', '"'], "");
        format!("\"{name}\" <{email}>")
    })
}

/// The mailboxes of a recipient list, comma-separated; None if one has no SMTP address.
fn addresses(list: Option<&Element>) -> Option<String> {
    let Some(list) = list else {
        return Some(String::new());
    };
    let all: Option<Vec<String>> = list.all("Mailbox").map(address).collect();
    all.map(|a| a.join(", "))
}

fn protocol(msg: impl Into<String>) -> ProviderError {
    ProviderError::Protocol(msg.into())
}

#[cfg(test)]
mod tests;
