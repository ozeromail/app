use crate::{AccountId, FolderId};

/// Local id of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId(pub i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
}

/// A message header as the server reports it. Bodies are stored separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    /// Provider id inside the folder: an IMAP UID, a Gmail or Graph message id.
    pub remote_id: String,
    /// The `Message-ID` header, used for threading.
    pub message_id: Option<String>,
    pub from: Address,
    pub subject: String,
    pub snippet: String,
    /// Seconds since the Unix epoch.
    pub date: i64,
    pub flags: Flags,
}

/// One row of a message list, ready for the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub id: MessageId,
    pub account_id: AccountId,
    pub from: Address,
    pub subject: String,
    pub snippet: String,
    pub date: i64,
    pub flags: Flags,
}

impl MessageRow {
    /// Where the next page starts.
    pub fn cursor(&self) -> Cursor {
        Cursor {
            date: self.date,
            id: self.id,
        }
    }
}

/// Keyset position in a list sorted newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub date: i64,
    pub id: MessageId,
}

/// Which messages a list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListScope {
    /// One folder of one account.
    Folder(FolderId),
    /// The folders with this role across all accounts, e.g. the unified inbox.
    Unified(crate::FolderRole),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListQuery {
    pub scope: ListScope,
    pub limit: u32,
    /// Show messages older than this; `None` starts from the newest.
    pub after: Option<Cursor>,
}
