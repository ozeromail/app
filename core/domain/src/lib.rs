//! Domain layer: types, invariants and ports (traits) of the mail core.

mod account;
mod encoding;
mod folder;
pub mod html;
mod message;
mod op;
mod outbox;
mod provider;
pub mod rich;
mod sort;
mod store;
mod unsubscribe;

pub use account::{
    Account, AccountId, AccountKind, MailService, NewAccount, NewMail, Server, default_label,
    email_domain, guess_imap_host, guess_smtp_host, mail_service, saves_sent_itself,
};
pub use encoding::{base64, decode_base64, markup_escape, percent_decode};
pub use folder::{Folder, FolderId, FolderRole, NewFolder, SNOOZED};
pub use message::{
    Address, Attachment, Body, Correspondents, Cursor, Flags, ListQuery, ListScope, MessageId,
    MessageLocation, MessageRow, NewMessage, RecentFile, Sender, close_blank_lines,
};
pub use op::{Action, Op, OpId};
pub use outbox::{
    Draft, DraftCopy, DraftFile, DraftId, DraftToPush, FileId, OutboxId, Outgoing, OutgoingFile,
    SavedDraft, ServerDraft, Unsent, forward_quote, forward_subject, full_text, is_address,
    parse_recipients, quote, reply_subject, typed_addresses,
};
pub use provider::{
    Changes, Checkpoint, MailProvider, MailSender, MessageBuilder, MessageParser, Moved, Older,
    ProviderError, ProviderState, RawMessage,
};
pub use sort::{Category, Signals};
pub use store::{Store, StoreError};
pub use unsubscribe::{Leave, Unsubscribe};
