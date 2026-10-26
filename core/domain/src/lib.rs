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
    Account, AccountId, AccountKind, MailService, NewAccount, Server, email_domain,
    guess_imap_host, guess_smtp_host, mail_service, saves_sent_itself,
};
pub use encoding::{base64, decode_base64};
pub use folder::{Folder, FolderId, FolderRole, NewFolder, SNOOZED};
pub use message::{
    Address, Attachment, Body, Correspondents, Cursor, Flags, ListQuery, ListScope, MessageId,
    MessageLocation, MessageRow, NewMessage, RecentFile, Sender,
};
pub use op::{Action, Op, OpId};
pub use outbox::{
    Draft, DraftFile, DraftId, FileId, OutboxId, Outgoing, OutgoingFile, SavedDraft, Unsent,
    forward_quote, forward_subject, is_address, parse_recipients, quote, reply_subject, with_quote,
};
pub use provider::{
    Changes, Checkpoint, MailProvider, MailSender, MessageBuilder, MessageParser, Moved, Older,
    ProviderError, ProviderState, RawMessage,
};
pub use sort::{Category, Signals};
pub use store::{Store, StoreError};
