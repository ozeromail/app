//! Domain layer: types, invariants and ports (traits) of the mail core.

mod account;
mod folder;
mod message;
mod sort;
mod store;

pub use account::{Account, AccountId, AccountKind, NewAccount};
pub use folder::{Folder, FolderId, FolderRole, NewFolder};
pub use message::{
    Address, Cursor, Flags, ListQuery, ListScope, MessageId, MessageRow, NewMessage,
};
pub use sort::Category;
pub use store::{Store, StoreError};
