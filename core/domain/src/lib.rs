//! Domain layer: types, invariants and ports (traits) of the mail core.

mod account;
mod encoding;
mod folder;
pub mod html;
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
    Address, Body, Correspondents, Cursor, Flags, ListQuery, ListScope, MessageId, MessageLocation,
    MessageRow, NewMessage, Sender,
};
pub use op::{Action, Op, OpId};
pub use outbox::{
    Draft, DraftId, OutboxId, Outgoing, SavedDraft, Unsent, forward_quote, forward_subject,
    is_address, parse_recipients, quote, reply_subject, with_quote,
};
pub use provider::{
    Changes, Checkpoint, MailProvider, MailSender, MessageBuilder, MessageParser, ProviderError,
    RawMessage,
};
pub use sort::{Category, Signals};
pub use store::{Store, StoreError};
