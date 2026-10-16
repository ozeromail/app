//! Domain layer: types, invariants and ports (traits) of the mail core.

mod account;
mod encoding;
mod folder;
mod message;
mod op;
mod outbox;
mod provider;
mod sort;
mod store;

pub use account::{
    Account, AccountId, AccountKind, NewAccount, Server, guess_imap_host, guess_smtp_host,
    saves_sent_itself,
};
pub use encoding::base64;
pub use folder::{Folder, FolderId, FolderRole, NewFolder};
pub use message::{
    Address, Body, Cursor, Flags, ListQuery, ListScope, MessageId, MessageLocation, MessageRow,
    NewMessage,
};
pub use op::{Action, Op, OpId};
pub use outbox::{Draft, OutboxId, Outgoing, Unsent, is_address, parse_recipients, reply_subject};
pub use provider::{
    Changes, Checkpoint, MailProvider, MailSender, MessageBuilder, MessageParser, ProviderError,
    RawMessage,
};
pub use sort::{Category, Signals};
pub use store::{Store, StoreError};
