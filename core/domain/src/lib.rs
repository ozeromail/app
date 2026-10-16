//! Domain layer: types, invariants and ports (traits) of the mail core.

mod account;
mod folder;
mod message;
mod op;
mod provider;
mod sort;
mod store;

pub use account::{Account, AccountId, AccountKind, NewAccount, Server, guess_imap_host};
pub use folder::{Folder, FolderId, FolderRole, NewFolder};
pub use message::{
    Address, Body, Cursor, Flags, ListQuery, ListScope, MessageId, MessageLocation, MessageRow,
    NewMessage,
};
pub use op::{Action, Op, OpId};
pub use provider::{MailProvider, MessageParser, ProviderError, RawMessage, Recent};
pub use sort::{Category, Signals};
pub use store::{Store, StoreError};
