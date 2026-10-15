use std::fmt;

use crate::{
    Account, AccountId, Folder, FolderId, ListQuery, MessageRow, NewAccount, NewFolder, NewMessage,
};

/// Local database port. The UI reads only from here; sync writes here.
pub trait Store: Send + Sync {
    fn add_account(&self, account: &NewAccount) -> Result<AccountId, StoreError>;
    fn accounts(&self) -> Result<Vec<Account>, StoreError>;

    /// Inserts new folders and updates known ones, matched by `remote_id`.
    fn save_folders(
        &self,
        account: AccountId,
        folders: &[NewFolder],
    ) -> Result<Vec<Folder>, StoreError>;
    fn folders(&self, account: AccountId) -> Result<Vec<Folder>, StoreError>;

    /// Records the folder's `Recent::validity`. When it changed, drops the folder's
    /// messages first: their `remote_id`s now name other messages.
    fn set_validity(&self, folder: FolderId, validity: u32) -> Result<(), StoreError>;

    /// Inserts new messages and updates known ones, matched by `remote_id`, in one transaction.
    fn save_messages(&self, folder: FolderId, messages: &[NewMessage]) -> Result<(), StoreError>;
    fn list(&self, query: &ListQuery) -> Result<Vec<MessageRow>, StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The database exists but the key does not open it.
    WrongKey,
    /// The database was written by a newer app version.
    TooNew {
        version: u32,
    },
    NotFound,
    /// Anything else from the storage engine, without message contents.
    Storage(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongKey => write!(f, "the database key is wrong"),
            Self::TooNew { version } => {
                write!(f, "database schema {version} is newer than this app")
            }
            Self::NotFound => write!(f, "not found"),
            Self::Storage(e) => write!(f, "storage error: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}
