//! Facade the Android and iOS apps call through UniFFI.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::hash::{BuildHasher, RandomState};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ozero_app::drafts::{DraftsFolder, push_drafts};
use ozero_app::ops::push_ops;
use ozero_app::send::{SentCopy, forwarded_files, push_sends};
use ozero_app::sync::{
    SyncError, download_attachments, download_body, load_older, prefetch_bodies, search_server,
    sync_folders, sync_mail, wait_for_mail,
};
use ozero_domain::{
    Account, AccountId, AccountKind, Action, Address, Attachment, Body, Category, Changes,
    Checkpoint, Draft, DraftFile, DraftId, FileId, Folder, FolderId, FolderRole, ListQuery,
    ListScope, MailProvider, MailSender, MailService, MessageId, MessageRow, Moved, NewAccount,
    NewFolder, NewMail, Older, OpId, OutboxId, Outgoing, OutgoingFile, ProviderError,
    ProviderState, RawMessage, RecentFile, SavedDraft, Sender, Server, Store, StoreError, html,
    parse_recipients, saves_sent_itself,
};
use ozero_ews::{EasSession, EwsSession, discover};
use ozero_gmail::{GmailSession, oauth};
use ozero_http::Tls;
use ozero_imap::rustls::ClientConfig;
use ozero_imap::rustls::client::ResolvesClientCert;
use ozero_imap::rustls::client::danger::ServerCertVerifier;
use ozero_imap::{ImapSession, RefusedCertificates, Watch};
use ozero_mime::{MimeBuilder, MimeParser};
use ozero_smtp::SmtpSession;
use ozero_store_sqlite::SqliteStore;

uniffi::setup_scaffolding!();

/// How many new messages of the inbox and of the sent folder one sync brings in; also of a
/// folder the user opens.
const SYNC_LIMIT: u32 = 200;
/// How many older messages of a folder a list scrolled to its end brings in at a time.
const OLDER_PAGE: u32 = 100;
/// How many of the newest matches in each folder a server search brings in.
const SERVER_HITS: u32 = 50;
/// Seconds in a day, for how long an account keeps its mail.
const DAY: i64 = 86_400;
/// How many of the newest inbox messages `prefetch` downloads whole.
const PREFETCH: u32 = 20;
/// Longest wait for new mail on one connection; a mobile network may drop a quiet one sooner.
const MAIL_WAIT: Duration = Duration::from_secs(5 * 60);
/// How long a Gmail account whose IMAP refused its token waits by asking the API.
const IDLE_RETRY: Duration = Duration::from_secs(60 * 60);
/// Mail older than this many seconds is not announced: a day.
const NEWS_AGE: i64 = DAY;
/// Milliseconds an archived or sent message waits before the server hears of it.
const HOLD: i64 = 5_000;
/// The app's Google OAuth client, one per platform; neither has a secret. An account's tokens
/// stay with the client that signed it in, and accounts never leave the device they were added on.
#[cfg(not(target_os = "ios"))]
const GOOGLE_CLIENT_ID: &str =
    "212695996222-1jq9p8or7u441h6spset2o15ghblbt1l.apps.googleusercontent.com";
#[cfg(target_os = "ios")]
const GOOGLE_CLIENT_ID: &str =
    "212695996222-nhjb5g2i394kese5s7tppk3q38tl89n4.apps.googleusercontent.com";
/// Where Google sends the browser back: on Android the package name as a URI scheme, on iOS
/// the client id reversed, the scheme Google gives an iOS client.
#[cfg(not(target_os = "ios"))]
const GOOGLE_REDIRECT: &str = "com.ozeromail.app:/oauth2redirect";
#[cfg(target_os = "ios")]
const GOOGLE_REDIRECT: &str =
    "com.googleusercontent.apps.212695996222-nhjb5g2i394kese5s7tppk3q38tl89n4:/oauth2redirect";
/// An access token is renewed this long before Google says it expires.
const TOKEN_MARGIN: Duration = Duration::from_secs(60);

/// How long the apps offer undo after archiving or sending: a second shorter than the hold,
/// so a tap is never late.
#[uniffi::export]
pub fn undo_seconds() -> u32 {
    u32::try_from(HOLD / 1_000 - 1).unwrap_or(0)
}

/// The usual ports of IMAP and SMTP over TLS, which a found account is tried on.
const IMAPS_PORT: u16 = 993;
const SMTPS_PORT: u16 = 465;
/// Gmail's IMAP, which only waits for mail here: the API syncs it.
const GMAIL_IMAP: &str = "imap.gmail.com";

/// How long a found account's IMAP server has to answer before Exchange is tried.
const FIND_WAIT: Duration = Duration::from_secs(10);

/// Whether a host name resolves, to tell a domain without mail servers from a device
/// without a connection.
async fn resolves(host: &str) -> bool {
    tokio::time::timeout(FIND_WAIT, tokio::net::lookup_host((host, 443)))
        .await
        .is_ok_and(|found| found.is_ok())
}

/// How many files of saved mail compose offers to send on.
const RECENT_FILES: usize = 6;
/// The most bytes the files a person adds to one message may take together: what Gmail takes.
const FILE_LIMIT: u64 = 25 * 1024 * 1024;

/// The most bytes the files added to one message may take together, for the apps to check
/// before they read a file.
#[uniffi::export]
pub fn file_limit() -> u64 {
    FILE_LIMIT
}

/// Version of the core, shown in About and in manual crash reports.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// The server sign-in suggests for an email address, or nothing until it has a domain.
#[uniffi::export]
pub fn suggested_imap_host(email: String) -> Option<String> {
    ozero_domain::guess_imap_host(&email)
}

/// The service an address is at, if sign-in knows it by its domain.
#[uniffi::export]
pub fn known_service(email: String) -> Option<MailService> {
    ozero_domain::mail_service(&email)
}

/// The SMTP server sign-in suggests, like `suggested_imap_host`.
#[uniffi::export]
pub fn suggested_smtp_host(email: String) -> Option<String> {
    ozero_domain::guess_smtp_host(&email)
}

/// Starts Google sign-in: the app opens `url` in the browser and keeps the rest for
/// `google_account` when the browser comes back to `GOOGLE_REDIRECT`. With `email`, to sign
/// in to a saved account again, Google offers that account.
#[uniffi::export]
pub fn google_sign_in(email: Option<String>) -> Result<GoogleSignIn, CoreError> {
    Ok(oauth::sign_in(GOOGLE_CLIENT_ID, GOOGLE_REDIRECT, email.as_deref())?.into())
}

/// The name the mailbox's mail goes out under, as set in Gmail. Without it the mail goes out
/// under the address alone, so a failure to read it stops nothing.
async fn sender_name(gmail: &mut GmailSession) -> Option<String> {
    gmail.name().await.unwrap_or_else(|e| {
        log::warn!("no sender name from Gmail: {e:?}");
        None
    })
}

/// A client certificate's name and a trusted server certificate's SHA-256.
type TlsKey = (Option<String>, Option<[u8; 32]>);

/// The mail core of one app install: the encrypted database and the network.
#[derive(uniffi::Object)]
pub struct Core {
    store: Arc<SqliteStore>,
    parser: MimeParser,
    tls: OnceLock<Arc<ClientConfig>>,
    /// The platform's client certificates, once the app has given them.
    certificates: OnceLock<Arc<dyn ClientCertificates>>,
    /// The platform's TLS, for Exchange with a client certificate, once the app has given it.
    platform_tls: OnceLock<Arc<dyn PlatformTls>>,
    /// TLS settings by the client certificate they offer and the server certificate the
    /// person trusted, built on first use.
    certificate_tls: Mutex<HashMap<TlsKey, Arc<ClientConfig>>>,
    /// One push or sync per account at a time, so a sync never saves flags from before a push.
    busy: Mutex<HashMap<AccountId, Arc<tokio::sync::Mutex<()>>>>,
    /// Accounts with a push waiting for held actions to come due.
    waiting: Mutex<HashSet<AccountId>>,
    /// Gmail accounts asked for their sender name since the core opened: one saved before
    /// names were read has none.
    named: Mutex<HashSet<AccountId>>,
    /// Gmail access tokens and until when they work, by the wall clock: a monotonic one stops
    /// while the phone sleeps, and the token would look fresh for too long.
    tokens: Mutex<HashMap<AccountId, (String, SystemTime)>>,
    /// Gmail accounts whose IMAP refused the token, as an old sign-in without its scope: they
    /// wait by asking the API.
    no_idle: Mutex<HashMap<AccountId, Instant>>,
    /// The last pages read with all their images, newest last, so opening one again is
    /// instant; up to `PAGES_KEPT` times `IMAGES_SIZE` in memory.
    pages: Mutex<VecDeque<KeptPage>>,
    /// How many writes changed the database since it was opened, for `changed`.
    changes: Arc<tokio::sync::watch::Sender<u64>>,
}

/// A page with its images, and the message HTML it was made from.
struct KeptPage {
    message: MessageId,
    source: String,
    page: ImagesPage,
}

/// Marks an account as having a waiting push until dropped.
struct Waiter<'a> {
    waiting: &'a Mutex<HashSet<AccountId>>,
    account: AccountId,
}

impl<'a> Waiter<'a> {
    /// None if another push already waits for the account.
    fn claim(waiting: &'a Mutex<HashSet<AccountId>>, account: AccountId) -> Option<Self> {
        let mut set = waiting.lock().unwrap_or_else(|p| p.into_inner());
        set.insert(account).then_some(Self { waiting, account })
    }
}

impl Drop for Waiter<'_> {
    fn drop(&mut self) {
        let mut set = self.waiting.lock().unwrap_or_else(|p| p.into_inner());
        set.remove(&self.account);
    }
}

impl fmt::Debug for Core {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Core")
    }
}

#[uniffi::export]
impl Core {
    /// Opens or creates the database at `path` with the 32-byte key from the platform keystore.
    #[uniffi::constructor]
    pub fn open(path: String, key: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let key: [u8; 32] = key.try_into().map_err(|_| CoreError::WrongKey)?;
        let store = SqliteStore::open(Path::new(&path), &key)?;
        let changes = Arc::new(tokio::sync::watch::Sender::new(0));
        let changed = changes.clone();
        store.on_change(move || changed.send_modify(|n| *n += 1))?;
        Ok(Arc::new(Self {
            store: Arc::new(store),
            parser: MimeParser::default(),
            tls: OnceLock::new(),
            certificates: OnceLock::new(),
            platform_tls: OnceLock::new(),
            certificate_tls: Mutex::default(),
            busy: Mutex::default(),
            waiting: Mutex::default(),
            named: Mutex::default(),
            tokens: Mutex::default(),
            no_idle: Mutex::default(),
            pages: Mutex::default(),
            changes,
        }))
    }

    /// Lets accounts use client certificates the platform keeps; the app calls it once, at
    /// start.
    pub fn use_certificates(&self, certificates: Arc<dyn ClientCertificates>) {
        let _ = self.certificates.set(certificates);
    }

    /// Lets ActiveSync with a client certificate go through the platform's TLS, which
    /// renegotiates when a server asks for the certificate late; the app calls it once, at start.
    pub fn use_platform_tls(&self, tls: Arc<dyn PlatformTls>) {
        let _ = self.platform_tls.set(tls);
    }

    /// Whether actions the server refused were undone on the device since this was last
    /// called, so the app can say so.
    pub fn take_reverted(&self) -> Result<bool, CoreError> {
        Ok(self.store.take_reverted()?)
    }

    /// The accounts in the order they were added.
    pub fn accounts(&self) -> Result<Vec<AccountItem>, CoreError> {
        let signature = self.store.signature()?;
        Ok(self
            .store
            .accounts()?
            .into_iter()
            .map(|a| AccountItem::new(a, &signature))
            .collect())
    }

    /// Sets when the phone looks for the account's new mail with the app closed.
    pub fn set_account_new_mail(&self, account: i64, when: NewMail) -> Result<(), CoreError> {
        Ok(self.store.set_account_new_mail(AccountId(account), when)?)
    }

    /// The signature of accounts without one of their own; empty for none.
    pub fn signature(&self) -> Result<String, CoreError> {
        Ok(self.store.signature()?)
    }

    /// Sets the signature of accounts without one of their own; empty for none.
    pub fn set_signature(&self, signature: String) -> Result<(), CoreError> {
        Ok(self.store.set_signature(&signature)?)
    }

    /// Gives the account a signature of its own; empty goes back to the default one.
    pub fn set_account_signature(&self, account: i64, signature: String) -> Result<(), CoreError> {
        Ok(self
            .store
            .set_account_signature(AccountId(account), &signature)?)
    }

    /// Sets the account's colour to one of `account_colors`, by its place there.
    pub fn set_account_color(&self, account: i64, color: u32) -> Result<(), CoreError> {
        let color = color % ACCOUNT_COLORS.len() as u32;
        Ok(self.store.set_account_color(AccountId(account), color)?)
    }

    /// Gives the account a name of the user's on this phone; empty goes back to the usual one.
    pub fn set_account_label(&self, account: i64, label: String) -> Result<(), CoreError> {
        Ok(self.store.set_account_label(AccountId(account), &label)?)
    }

    /// Removes the account and all its mail from the device at once; a sync or push under way
    /// or waiting then ends without an error. The app forgets its secret.
    pub fn remove_account(&self, account: i64) -> Result<(), CoreError> {
        let account = AccountId(account);
        self.store.remove_account(account)?;
        self.tokens
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&account);
        self.busy
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&account);
        self.kept_pages().clear();
        Ok(())
    }

    /// The account works again after `check_password` or `google_account` for it, once the
    /// app has kept the new secret: its mail and queued work go on where they stopped. A Gmail
    /// account takes the sender `name` Gmail has now.
    pub fn signed_in_again(&self, account: i64, name: Option<String>) -> Result<(), CoreError> {
        let account = AccountId(account);
        if let Some(name) = name {
            self.store.set_account_name(account, &name)?;
        }
        self.store.set_needs_sign_in(account, false)?;
        self.tokens
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&account);
        Ok(())
    }

    /// Saves the Gmail account `google_account` returned, once the app has kept its secret.
    pub fn add_gmail_account(&self, email: String, name: Option<String>) -> Result<i64, CoreError> {
        if self.exists(&email)? {
            return Err(CoreError::AccountExists);
        }
        let account = NewAccount {
            kind: AccountKind::Gmail,
            display_name: name.unwrap_or_else(|| email.clone()),
            email,
            server: None,
            smtp: None,
        };
        Ok(self.store.add_account(&account)?.0)
    }

    /// The newest conversations of one tab across all inboxes, newest first: each row is the
    /// newest message of its conversation. Fewer than `limit` once the saved mail ends, so
    /// `older_mail` can bring in more.
    pub fn unified_inbox(&self, tab: Category, limit: u32) -> Result<Vec<MessageItem>, CoreError> {
        self.list(ListScope::Inbox(tab), INBOX, limit)
    }

    /// The folders of every account, in the order the server listed them first.
    pub fn folders(&self) -> Result<Vec<FolderItem>, CoreError> {
        let mut folders = vec![];
        for account in self.store.accounts()? {
            folders.extend(
                self.store
                    .folders(account.id)?
                    .into_iter()
                    .map(FolderItem::from),
            );
        }
        Ok(folders)
    }

    /// The newest conversations of a mailbox, newest first, as `unified_inbox` without tabs.
    pub fn mailbox(&self, mailbox: Mailbox, limit: u32) -> Result<Vec<MessageItem>, CoreError> {
        let scope = match mailbox {
            Mailbox::Unified { role } => ListScope::Unified(role),
            Mailbox::Folder { id } => ListScope::Folder(FolderId(id)),
        };
        self.list(scope, mailbox, limit)
    }

    /// Unread mail in People from the last day that arrived since this or `forget_new_mail`
    /// was last called, newest first: what a notification shows. Older mail that comes back,
    /// say after a mailbox reset, is not news.
    pub fn take_new_mail(&self) -> Result<Vec<MessageItem>, CoreError> {
        Ok(self
            .store
            .take_fresh(now() / 1_000 - NEWS_AGE)?
            .into_iter()
            .map(MessageItem::from)
            .collect())
    }

    /// Mail that arrived so far is not new any more: the inbox calls it after a sync it shows,
    /// so that mail is not announced later.
    pub fn forget_new_mail(&self) -> Result<(), CoreError> {
        Ok(self.store.clear_fresh()?)
    }

    /// The inbox tabs in order, each with its unread count.
    pub fn inbox_tabs(&self) -> Result<Vec<InboxTab>, CoreError> {
        let unread = self.store.inbox_unread()?;
        Ok(Category::ALL
            .into_iter()
            .map(|category| InboxTab {
                category,
                unread: unread
                    .iter()
                    .find(|(c, _)| *c == category)
                    .map_or(0, |(_, n)| *n),
            })
            .collect())
    }

    /// Whether every inbox tab is empty, so they can share one empty note.
    pub fn inbox_is_empty(&self) -> Result<bool, CoreError> {
        Ok(self.store.inbox_is_empty()?)
    }

    /// Messages matching every word of `text`, newest first, with what `search_server`
    /// found for the same words.
    pub fn search(&self, text: String, limit: u32) -> Result<Vec<MessageItem>, CoreError> {
        let rows = self.store.search(&text, limit)?;
        self.items(rows)
    }

    /// The conversation of a message, oldest first.
    pub fn thread(&self, message: i64) -> Result<Vec<MessageItem>, CoreError> {
        let rows = self.store.thread(MessageId(message))?;
        Ok(rows.into_iter().map(MessageItem::from).collect())
    }

    /// Who `message` went to, and who answering all would add.
    pub fn people(&self, message: MessageItem) -> Result<PeopleItem, CoreError> {
        let own = self.account(AccountId(message.account_id))?.email;
        let saved = self.store.correspondents(MessageId(message.id))?;
        let person = |a: &Address| {
            let name = shown_name(a.name.as_deref()).unwrap_or(&a.email);
            PersonItem {
                first: first_name(name).to_owned(),
                name: name.to_owned(),
                email: a.email.clone(),
                me: a.email.eq_ignore_ascii_case(&own),
            }
        };
        let to: Vec<PersonItem> = saved.to.iter().map(person).collect();
        let cc: Vec<PersonItem> = saved.cc.iter().map(person).collect();
        let mut shown: Vec<PersonItem> = Vec::new();
        for p in to.iter().chain(&cc) {
            if !shown.iter().any(|s| s.email.eq_ignore_ascii_case(&p.email)) {
                shown.push(p.clone());
            }
        }
        // The user first: "To you, Oleg and Maria".
        shown.sort_by_key(|p| !p.me);
        let others = saved.others(&own).into_iter().map(person).collect();
        Ok(PeopleItem {
            to,
            cc,
            shown,
            others,
        })
    }

    /// Marks the messages' conversations read, or the messages alone unread, on the device at
    /// once and in one go; `push_actions` tells the server.
    pub fn mark_read(&self, messages: Vec<i64>, seen: bool) -> Result<(), CoreError> {
        self.store
            .queue_all(&with(messages, Action::Seen(seen)), now())?;
        Ok(())
    }

    /// Takes the messages' conversations out of the lists at once, all or none, and holds the
    /// move to the archive for `HOLD`, so `undo` can take it back. Returns the actions for
    /// `undo`. Fails with `NoArchive` if an account has no archive folder.
    pub fn archive(&self, messages: Vec<i64>) -> Result<Vec<i64>, CoreError> {
        self.hold_move(messages, Action::Archive)
    }

    /// As `archive`, back into the inbox: for mail in the archive, trash or spam.
    pub fn move_to_inbox(&self, messages: Vec<i64>) -> Result<Vec<i64>, CoreError> {
        self.hold_move(messages, Action::Inbox)
    }

    /// As `archive`, into the account's snoozed folder until `until` (seconds), when `wake`
    /// brings the messages back. Fails with `NoSnoozed` if an account has none yet.
    pub fn snooze(&self, messages: Vec<i64>, until: i64) -> Result<Vec<i64>, CoreError> {
        let messages: Vec<MessageId> = messages.into_iter().map(MessageId).collect();
        Ok(ids(self.store.snooze(&messages, until, now() + HOLD)?))
    }

    /// Brings the snoozed mail whose time has come back into the inbox, on top and unread,
    /// and returns it, for a notification; `push_actions` tells the server.
    pub fn wake(&self) -> Result<Vec<MessageItem>, CoreError> {
        let rows = self.store.wake(now() / 1_000, now())?;
        self.items(rows)
    }

    /// When `wake` next has mail to bring back (seconds), if any is snoozed.
    pub fn next_wake(&self) -> Result<Option<i64>, CoreError> {
        Ok(self.store.next_wake()?)
    }

    /// As `archive`, `messages` into the trash and `forever`, mail in the trash, gone from the
    /// server for good. Fails with `NoTrash` if an account has no trash folder.
    pub fn delete(&self, messages: Vec<i64>, forever: Vec<i64>) -> Result<Vec<i64>, CoreError> {
        let mut actions = with(messages, Action::Trash);
        actions.extend(with(forever, Action::Delete));
        Ok(ids(self.store.queue_all(&actions, now() + HOLD)?))
    }

    /// Empties a trash or spam mailbox for good, every account's for a unified one, held like
    /// `archive`: on the server too, mail the app has not loaded included. Returns the actions
    /// for `undo`.
    pub fn empty_mailbox(&self, mailbox: Mailbox) -> Result<Vec<i64>, CoreError> {
        let folders = match mailbox {
            Mailbox::Folder { id } => vec![FolderId(id)],
            Mailbox::Unified { role } => {
                let mut folders = vec![];
                for account in self.store.accounts()? {
                    let all = self.store.folders(account.id)?;
                    folders.extend(all.into_iter().filter(|f| f.role == role).map(|f| f.id));
                }
                folders
            }
        };
        let mut ops = vec![];
        for folder in folders {
            ops.extend(self.store.queue_empty(folder, now() + HOLD)?);
        }
        Ok(ids(ops))
    }

    /// A blank message from the first account, signed as it signs. None picked: it goes
    /// from the first account unless the person picks another.
    pub fn new_draft(&self) -> Result<DraftItem, CoreError> {
        let signature = match self.store.accounts()?.first() {
            Some(first) => self.signs_with(first)?,
            None => String::new(),
        };
        Ok(DraftItem {
            id: 0,
            account_id: 0,
            to: String::new(),
            subject: String::new(),
            text: String::new(),
            reply_to: None,
            quote: String::new(),
            saved: 0,
            forward: None,
            files: vec![],
            signature,
            cc: String::new(),
        })
    }

    /// An answer to `message` from the account that got it, `Re: ` once: to its sender, and
    /// with `all` a copy to everyone else it went to, `PeopleItem::others`.
    pub fn reply(&self, message: MessageItem, all: bool) -> Result<DraftItem, CoreError> {
        let account = self.account(AccountId(message.account_id))?;
        let correspondents = self.store.correspondents(MessageId(message.id))?;
        let cc: Vec<&str> = if all {
            let others = correspondents.others(&account.email);
            others.into_iter().map(|a| a.email.as_str()).collect()
        } else {
            vec![]
        };
        Ok(DraftItem {
            id: 0,
            account_id: message.account_id,
            to: correspondents.answer(&account.email).join(", "),
            cc: cc.join(", "),
            subject: ozero_domain::reply_subject(&message.subject),
            text: String::new(),
            reply_to: Some(message.id),
            quote: String::new(),
            saved: 0,
            forward: None,
            files: vec![],
            signature: self.signs_with(&account)?,
        })
    }

    /// The text of `message` under `header`, each line marked with `>`; downloaded if it is
    /// not saved. `secret` as for `message_body`.
    pub async fn quote(
        &self,
        message: i64,
        header: String,
        secret: String,
    ) -> Result<String, CoreError> {
        let body = self.body(MessageId(message), &secret).await?;
        Ok(ozero_domain::quote(header, &body.text))
    }

    /// Takes back held actions. False if any may be on its way to the server.
    pub fn undo(&self, actions: Vec<i64>) -> Result<bool, CoreError> {
        let ops: Vec<_> = actions.into_iter().map(OpId).collect();
        Ok(self.store.undo(&ops, now())?)
    }

    /// Puts a message in the outbox, held for `HOLD` so `undo_send` can take it back;
    /// `push_actions` sends it. `to` is addresses split by commas or spaces; `reply_to` the
    /// message it answers, and `quote` what goes under the text. The saved draft it was
    /// written in, if any, goes, and its files go with the message. Returns the id for
    /// `undo_send`.
    pub fn send(&self, draft: DraftItem) -> Result<i64, CoreError> {
        let DraftItem {
            id: saved,
            account_id: account,
            to,
            subject,
            text,
            reply_to,
            quote,
            forward,
            signature,
            cc,
            ..
        } = draft;
        let to = parse_recipients(&to).ok_or(CoreError::BadRecipients)?;
        let cc = match cc.trim() {
            "" => vec![],
            cc => parse_recipients(cc).ok_or(CoreError::BadCopies)?,
        };
        let from = self.account(AccountId(account))?;
        // Would wait in the outbox forever.
        if from.kind == AccountKind::Imap && from.smtp.is_none() {
            return Err(CoreError::NoSmtp);
        }
        let draft = Draft {
            account_id: from.id,
            to,
            cc,
            subject,
            text,
            signature,
            reply_to: reply_to.map(MessageId),
            quote,
            forward: forward.map(MessageId),
        };
        let now = now();
        let id = self.store.queue_send(
            &draft,
            draft_id(saved),
            &message_id(&from.email, now),
            now / 1_000,
            now + HOLD,
        )?;
        Ok(id.0)
    }

    /// The messages the server refused, newest first; `undo_send` takes one back to fix it.
    pub fn unsent(&self) -> Result<Vec<UnsentItem>, CoreError> {
        Ok(self
            .store
            .unsent()?
            .into_iter()
            .map(|u| UnsentItem {
                id: u.id.0,
                subject: u.subject,
                reason: u.reason,
            })
            .collect())
    }

    /// Takes back a held or refused message to edit it again, as a saved draft. None once it
    /// may be on its way.
    pub fn undo_send(&self, id: i64) -> Result<Option<DraftItem>, CoreError> {
        Ok(self
            .store
            .undo_send(OutboxId(id), now())?
            .map(DraftItem::from))
    }

    /// Keeps a message being written on the device, as it is now: a new draft if its `id` is
    /// 0, from the first account if its `account_id` is 0. Returns its id. Fails with `Gone`
    /// once it was sent or thrown away. `push_actions` saves it on the server too.
    pub fn save_draft(&self, draft: DraftItem) -> Result<i64, CoreError> {
        let account_id = match draft.account_id {
            0 => self.store.accounts()?.first().map(|a| a.id),
            id => Some(AccountId(id)),
        };
        let saved = SavedDraft {
            id: draft_id(draft.id),
            account_id,
            to: draft.to,
            cc: draft.cc,
            subject: draft.subject,
            text: draft.text,
            signature: draft.signature,
            reply_to: draft.reply_to.map(MessageId),
            quote: draft.quote,
            forward: draft.forward.map(MessageId),
            saved: now(),
            files: vec![],
        };
        match self.store.save_draft(&saved) {
            Ok(id) => Ok(id.0),
            Err(StoreError::NotFound) => Err(CoreError::Gone),
            Err(e) => Err(e.into()),
        }
    }

    /// The drafts kept on the device, last saved first.
    pub fn drafts(&self) -> Result<Vec<DraftItem>, CoreError> {
        Ok(self
            .store
            .drafts()?
            .into_iter()
            .map(DraftItem::from)
            .collect())
    }

    /// Throws a draft away, its files too, held like `archive` so `restore_draft` can bring it
    /// back.
    pub fn delete_draft(&self, id: i64) -> Result<(), CoreError> {
        let now = now();
        Ok(self.store.delete_draft(DraftId(id), now, now + HOLD)?)
    }

    /// Brings back `draft`, which `delete_draft` threw away, as it is now, and returns it
    /// saved: with its files while the hold lasts, else as a new draft without them.
    pub fn restore_draft(&self, draft: DraftItem) -> Result<DraftItem, CoreError> {
        let id = draft_id(draft.id);
        if let Some(id) = id {
            self.store.restore_draft(id, now())?;
        }
        // Back, or never thrown away: its files are the ones it holds, whatever the app saw.
        let kept = self
            .store
            .drafts()?
            .into_iter()
            .find(|d| d.id.is_some() && d.id == id);
        let files = kept.map(|d| d.files);
        let id = self.save_draft(DraftItem {
            id: if files.is_some() { draft.id } else { 0 },
            ..draft.clone()
        })?;
        let files = files.unwrap_or_default();
        Ok(DraftItem {
            id,
            files: files.into_iter().map(DraftFileItem::from).collect(),
            ..draft
        })
    }

    /// Adds a file to the saved draft `draft`; it goes with the message when it is sent.
    /// Fails with `TooLarge` when the draft's files, a forwarded message's among them, would
    /// take more than `file_limit`, and with `Gone` once the draft was sent or thrown away.
    pub fn attach_file(
        &self,
        draft: i64,
        name: String,
        mime: String,
        data: Vec<u8>,
    ) -> Result<DraftFileItem, CoreError> {
        let file = OutgoingFile { name, mime, data };
        match self.store.attach(DraftId(draft), &file, FILE_LIMIT) {
            Ok(Some(file)) => Ok(file.into()),
            Ok(None) => Err(CoreError::TooLarge),
            Err(StoreError::NotFound) => Err(CoreError::Gone),
            Err(e) => Err(e.into()),
        }
    }

    /// The files of saved mail to send on, newest first, each name and size once.
    pub fn recent_files(&self) -> Result<Vec<RecentFileItem>, CoreError> {
        Ok(self
            .store
            .recent_files(RECENT_FILES)?
            .into_iter()
            .map(RecentFileItem::from)
            .collect())
    }

    /// Takes a file off its draft.
    pub fn detach_file(&self, file: i64) -> Result<(), CoreError> {
        Ok(self.store.detach(FileId(file))?)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Core {
    /// Waits until the database changes after `seen`, the number the last call returned (0
    /// the first time), and returns the new number: at once if it changed meanwhile. Screens
    /// read their data again then.
    pub async fn changed(&self, seen: u64) -> u64 {
        let mut changes = self.changes.subscribe();
        // The sender lives as long as `self`, so the wait never fails.
        changes.wait_for(|n| *n != seen).await.map_or(seen, |n| *n)
    }

    /// Saves an IMAP account once both servers have accepted the login; `email` is the login
    /// name on both.
    pub async fn add_imap_account(
        &self,
        email: String,
        display_name: String,
        imap: Endpoint,
        smtp: Endpoint,
        password: String,
    ) -> Result<i64, CoreError> {
        // Checked before going online, so a repeat sign-in costs no login.
        if self.exists(&email)? {
            return Err(CoreError::AccountExists);
        }
        let server = imap.server(&email);
        let smtp = smtp.server(&email);
        // Both at once; a working login needs no goodbye, the server cleans up.
        tokio::try_join!(
            self.login(&server, &password),
            self.send_login(&smtp, &password)
        )?;
        let account = NewAccount {
            kind: AccountKind::Imap,
            email,
            display_name,
            server: Some(server),
            smtp: Some(smtp),
        };
        Ok(self.store.add_account(&account)?.0)
    }

    /// Saves an account from an address and a password alone, for a domain sign-in does not
    /// know: first IMAP and SMTP at the domain's usual hosts, then Exchange found from the
    /// address. Fails with `NotFound` when neither answers, so the app asks for the servers.
    pub async fn add_found_account(
        &self,
        email: String,
        password: String,
    ) -> Result<i64, CoreError> {
        let (Some(imap), Some(smtp)) = (
            ozero_domain::guess_imap_host(&email),
            ozero_domain::guess_smtp_host(&email),
        ) else {
            return Err(CoreError::NotFound);
        };
        let imap = Endpoint {
            host: imap,
            port: IMAPS_PORT,
        };
        let smtp = Endpoint {
            host: smtp,
            port: SMTPS_PORT,
        };
        if self.exists(&email)? {
            return Err(CoreError::AccountExists);
        }
        // IMAP alone first, so a slow or missing SMTP server does not hide one that answers;
        // briefly, as a host that never answers would hold back the Exchange try.
        let server = imap.clone().server(&email);
        let probe = tokio::time::timeout(FIND_WAIT, self.login(&server, &password))
            .await
            .unwrap_or_else(|_| {
                Err(CoreError::Network {
                    detail: "timed out".into(),
                })
            });
        match probe {
            Ok(_) => {
                return match self
                    .add_imap_account(email.clone(), email, imap, smtp, password)
                    .await
                {
                    // IMAP answers, but SMTP not at its usual host: the servers are typed.
                    Err(CoreError::Network { .. } | CoreError::Server { .. }) => {
                        Err(CoreError::NotFound)
                    }
                    added => added,
                };
            }
            // Nothing answers there, or something else than IMAP: maybe the mail is on Exchange.
            Err(CoreError::Network { .. } | CoreError::Server { .. }) => {}
            Err(e) => return Err(e),
        }
        // No name: the server is asked for the owner's.
        let domain = ozero_domain::email_domain(&email).unwrap_or_default();
        match self
            .add_exchange_account(email, String::new(), None, None, None, None, password)
            .await
        {
            // Without even the domain's own address, the device is most likely offline.
            Err(CoreError::NoServer) if !resolves(&domain).await => Err(CoreError::Network {
                detail: format!("no address for {domain}"),
            }),
            Err(CoreError::NoServer) => Err(CoreError::NotFound),
            added => added,
        }
    }

    /// Saves an Exchange account once its server has taken the login. Without `server` the
    /// server is found from the address; with it, it is a host or a URL the person typed.
    /// `login` is the address unless given, such as `DOMAIN\user`. `certificate` names a
    /// client certificate the platform keeps, for a server that asks for one. `trusted` is the
    /// fingerprint of a server certificate the person chose to trust although the checks
    /// refused it, as `UntrustedCertificate` gave it. Fails with `NoServer` when no server is
    /// found, so the app asks for one.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_exchange_account(
        &self,
        email: String,
        display_name: String,
        server: Option<String>,
        login: Option<String>,
        certificate: Option<String>,
        trusted: Option<String>,
        password: String,
    ) -> Result<i64, CoreError> {
        let trusted = trusted.as_deref().and_then(unhex);
        let certificate = certificate.filter(|c| !c.is_empty());
        // Its own, so a refused certificate it finds is this sign-in's.
        let refused = Arc::new(RefusedCertificates::default());
        let client = self.client_certificate(certificate.as_deref())?;
        let tls = tls_config(client, trusted, Some(refused.clone()))?;
        let active_sync = self
            .tunnel(certificate.as_deref(), trusted, Some(refused.clone()))?
            .unwrap_or_else(|| tls.clone().into());
        let added = self
            .add_exchange(
                email,
                display_name,
                server,
                login,
                certificate,
                (tls, active_sync),
                password,
            )
            .await;
        match added {
            // A refused server certificate breaks the handshake before any of these.
            Err(
                CoreError::Network { .. } | CoreError::Tls { .. } | CoreError::CertificateNeeded,
            ) => match refused.any() {
                Some((host, refused)) => Err(CoreError::UntrustedCertificate {
                    host,
                    fingerprint: hex(&refused.fingerprint),
                    reason: refused.reason,
                }),
                None => added,
            },
            Ok(id) => {
                let Some(trusted) = trusted else {
                    return Ok(id);
                };
                let account = AccountId(id);
                // Without it the account cannot reach its server.
                if let Err(e) =
                    self.store
                        .set_provider_state(account, TRUSTED, Some(&hex(&trusted)))
                {
                    let _ = self.store.remove_account(account);
                    return Err(e.into());
                }
                Ok(id)
            }
            added => added,
        }
    }

    /// Checks a new password of a saved IMAP or Exchange account on its servers, for signing
    /// in again; `signed_in_again` follows once the app has kept it.
    pub async fn check_password(&self, account: i64, password: String) -> Result<(), CoreError> {
        let account = self.account(AccountId(account))?;
        if account.kind == AccountKind::Exchange {
            if self.speaks_active_sync(account.id)? {
                self.eas(&account, &password).await?.check().await?;
            } else {
                self.ews(&account, &password).await?;
            }
            return Ok(());
        }
        match &account.smtp {
            Some(smtp) => {
                tokio::try_join!(
                    self.connect(&account, &password),
                    self.send_login(smtp, &password)
                )?;
            }
            None => drop(self.connect(&account, &password).await?),
        }
        Ok(())
    }

    /// Finishes Google sign-in with the address the browser came back to: the account's
    /// address, its sender name and the secret the app keeps for it. `add_gmail_account` then
    /// saves it; or, with `again`, the sign-in was for that saved account, and
    /// `signed_in_again` follows.
    /// Fails with `WrongAccount` if the person chose another Google account.
    pub async fn google_account(
        &self,
        sign_in: GoogleSignIn,
        redirect: String,
        again: Option<i64>,
    ) -> Result<GoogleAccount, CoreError> {
        let start = oauth::SignIn::from(sign_in);
        let code = oauth::code(&redirect, &start)?;
        let tls = self.tls()?;
        let tokens = oauth::exchange(
            tls.clone(),
            GOOGLE_CLIENT_ID,
            GOOGLE_REDIRECT,
            &code,
            &start,
        )
        .await?;
        let mut gmail = GmailSession::new(tls, &tokens.access);
        let email = gmail.email().await?;
        match again {
            Some(id)
                if !self
                    .account(AccountId(id))?
                    .email
                    .eq_ignore_ascii_case(&email) =>
            {
                return Err(CoreError::WrongAccount);
            }
            None if self.exists(&email)? => return Err(CoreError::AccountExists),
            _ => {}
        }
        Ok(GoogleAccount {
            email,
            name: sender_name(&mut gmail).await,
            secret: tokens.refresh.unwrap_or_default(),
        })
    }

    /// Sends the account's queued actions, waiting for held ones to come due; without any
    /// it does not connect. After a failure they stay queued for the next push or sync.
    /// `secret` is the account's password, or its Google secret.
    pub async fn push_actions(&self, account: i64, secret: String) -> Result<(), CoreError> {
        let id = AccountId(account);
        let pushed = async {
            let account = self.account(id)?;
            let mut waiter = None;
            loop {
                let wait = {
                    let lock = self.busy(account.id);
                    let _busy = lock.lock().await;
                    // Removed already: nothing to send.
                    let found = self.find(account.id);
                    if !matches!(found, Ok(Some(_))) {
                        drop(waiter.take());
                        return found.map(drop);
                    }
                    // Read under the lock: a push that waited finds what the one before it sent gone.
                    let step = async {
                        let mut remote = None;
                        self.push_due(&account, &secret, &mut remote).await?;
                        if let Some(remote) = remote {
                            remote.close().await;
                        }
                        Ok::<_, CoreError>(self.store.next_send(account.id)?)
                    }
                    .await;
                    // The waiter is taken and given up under the lock, so a push that finds it
                    // taken can leave its held action to the waiting push.
                    let wait = match step {
                        // One that came due while this push sent: at once, not left behind.
                        Ok(next) => next.map_or(0, |at| (at - now()).max(1)),
                        Err(e) => {
                            drop(waiter.take());
                            return self.refused(&account, Err(e));
                        }
                    };
                    let Ok(wait @ 1..) = u64::try_from(wait) else {
                        drop(waiter.take());
                        return Ok(());
                    };
                    if waiter.is_none() {
                        waiter = Waiter::claim(&self.waiting, account.id);
                        if waiter.is_none() {
                            return Ok(());
                        }
                    }
                    wait
                };
                // Without the lock, so a sync can run meanwhile.
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
        }
        .await;
        self.unless_removed(id, pushed)
    }

    /// Sends the account's queued actions and mail, then brings its folders and what changed
    /// in its inbox and sent folder into the database. `secret` as for `push_actions`.
    pub async fn sync(&self, account: i64, secret: String) -> Result<(), CoreError> {
        self.sync_mailbox(account, secret, INBOX).await
    }

    /// As `sync` for the unified inbox; for another mailbox, brings in what changed in the
    /// account's folders of it, as the last `sync` listed them.
    pub async fn sync_mailbox(
        &self,
        account: i64,
        secret: String,
        mailbox: Mailbox,
    ) -> Result<(), CoreError> {
        let id = AccountId(account);
        let synced = async {
            let lock = self.busy(id);
            let _busy = lock.lock().await;
            // Removed already: nothing to sync.
            let Some(account) = self.find(id)? else {
                return Ok(());
            };
            let since = keep_since(account.keep_days);
            let folders: Vec<Folder> = if mailbox == INBOX {
                vec![]
            } else {
                let all = self.store.folders(id)?;
                all.into_iter().filter(|f| mailbox.holds(f)).collect()
            };
            let mut slot = None;
            // A send that fails, say on a broken SMTP server, must not stop new mail coming in.
            let pushed = self.refused(&account, self.push_due(&account, &secret, &mut slot).await);
            if mailbox == INBOX || !folders.is_empty() {
                let remote = self.remote(&mut slot, &account, &secret).await?;
                let synced = if mailbox == INBOX {
                    sync_mail(&*self.store, remote, &self.parser, id, SYNC_LIMIT, since).await
                } else {
                    let parser = &self.parser;
                    sync_folders(&*self.store, remote, parser, &folders, SYNC_LIMIT, since).await
                };
                self.refused(&account, synced)?;
            }
            if let Some(remote) = slot {
                remote.close().await;
            }
            pushed
        }
        .await;
        self.unless_removed(id, synced)
    }

    /// Brings in older mail of the account's folders in `mailbox`, for a list scrolled to its
    /// end: up to `OLDER_PAGE` messages of each, none older than the account keeps. True while
    /// one of them may have older mail still. `secret` as for `push_actions`.
    pub async fn older_mail(
        &self,
        account: i64,
        secret: String,
        mailbox: Mailbox,
    ) -> Result<bool, CoreError> {
        let id = AccountId(account);
        let loaded = async {
            let lock = self.busy(id);
            let _busy = lock.lock().await;
            let Some(account) = self.find(id)? else {
                return Ok(false);
            };
            let folders = self.loadable(id, mailbox)?;
            let since = keep_since(account.keep_days);
            let (mut slot, mut more) = (None, false);
            for (folder, _) in &folders {
                let remote = self.remote(&mut slot, &account, &secret).await?;
                let loaded = load_older(
                    &*self.store,
                    remote,
                    &self.parser,
                    folder,
                    OLDER_PAGE,
                    since,
                );
                match self.refused(&account, loaded.await) {
                    Ok(true) => more = true,
                    // The next sync starts the folder over.
                    Ok(false) | Err(CoreError::Gone) => {}
                    Err(e) => return Err(e),
                }
            }
            if let Some(remote) = slot {
                remote.close().await;
            }
            Ok(more)
        }
        .await;
        self.unless_removed(id, loaded)
    }

    /// Searches the account's mail on its server for every word of `text`, mail older than it
    /// keeps on the device included; `search` then shows what it found. `secret` as for
    /// `push_actions`.
    pub async fn search_server(
        &self,
        account: i64,
        secret: String,
        text: String,
    ) -> Result<(), CoreError> {
        let Some(account) = self.find(AccountId(account))? else {
            return Ok(());
        };
        let mut slot = None;
        let remote = self.remote(&mut slot, &account, &secret).await?;
        let found = search_server(
            &*self.store,
            remote,
            &self.parser,
            account.id,
            &text,
            SERVER_HITS,
        )
        .await;
        if let Some(remote) = slot {
            remote.close().await;
        }
        let hits = self.refused(&account, found)?;
        log::info!(
            "server search for account {}: {hits} results over its folders",
            account.id.0
        );
        Ok(())
    }

    /// Keeps the account's mail of the last `days` days on the device, or all of it with 0:
    /// older mail goes at once, and lists stop loading older mail there. Returns after a sync
    /// under way, which may save mail by the old limit, and takes that out too.
    pub async fn set_account_keep(&self, account: i64, days: u32) -> Result<(), CoreError> {
        let account = AccountId(account);
        self.store.set_account_keep(account, days)?;
        let since = keep_since(days);
        self.store.prune(account, since)?;
        let lock = self.busy(account);
        if lock.try_lock().is_ok() {
            return Ok(());
        }
        let _busy = lock.lock().await;
        Ok(self.store.prune(account, since)?)
    }

    /// Downloads the missing bodies of the account's newest inbox mail, so it opens at once.
    /// Runs apart from `sync`, so a sync and the actions waiting on it stay quick.
    pub async fn prefetch(&self, account: i64, secret: String) -> Result<(), CoreError> {
        let Some(account) = self.find(AccountId(account))? else {
            return Ok(());
        };
        let mut slot = None;
        let remote = self.remote(&mut slot, &account, &secret).await?;
        let fetched =
            prefetch_bodies(&*self.store, remote, &self.parser, account.id, PREFETCH).await;
        drop(slot);
        self.refused(&account, fetched)
    }

    /// Waits on its own connection until the account's inbox may have changed since the last
    /// sync, at most a few minutes: true if it may have, so a sync is worth it. For the app on
    /// screen, called again after each answer.
    pub async fn wait_for_mail(&self, account: i64, secret: String) -> Result<bool, CoreError> {
        let account = self.account(AccountId(account))?;
        let mut slot = None;
        let idles = self
            .no_idle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&account.id)
            .is_none_or(|refused| refused.elapsed() > IDLE_RETRY);
        if account.kind == AccountKind::Gmail && idles {
            let token = self.refused(&account, self.access_token(account.id, &secret).await)?;
            match self.gmail_watch(&account, &token).await {
                Ok((mut imap, watch)) => {
                    // Mail since the last sync, asked once IDLE would hear of what comes after.
                    let remote = self.remote(&mut slot, &account, &secret).await?;
                    let waited =
                        wait_for_mail(&*self.store, remote, account.id, Duration::ZERO).await;
                    if let Some(remote) = slot.take() {
                        remote.close().await;
                    }
                    let changed = match self.refused(&account, waited) {
                        Ok(false) => imap.watch(watch, MAIL_WAIT).await.map_err(Into::into),
                        done => done,
                    };
                    let _ = imap.logout().await;
                    return changed;
                }
                // An old sign-in without IMAP's scope, or a token Google took back: the API
                // gets a new one, and is asked now and then.
                Err(CoreError::Auth) => {
                    log::info!("Gmail IMAP refused the token; the API is asked instead");
                    self.tokens
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&account.id);
                    self.no_idle
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .insert(account.id, Instant::now());
                }
                Err(e) => return Err(e),
            }
        }
        let remote = self.remote(&mut slot, &account, &secret).await?;
        let waited = wait_for_mail(&*self.store, remote, account.id, MAIL_WAIT).await;
        if let Some(remote) = slot {
            remote.close().await;
        }
        self.refused(&account, waited)
    }

    /// The message body: the saved one, or downloaded once and saved with its snippet.
    /// `secret` is the message's account's, as for `push_actions`.
    pub async fn message_body(
        &self,
        message: i64,
        secret: String,
    ) -> Result<MessageBody, CoreError> {
        let message = MessageId(message);
        let shows_images = self.store.shows_images(message)?;
        Ok(MessageBody {
            shows_images,
            ..self.body(message, &secret).await?.into()
        })
    }

    /// `message` sent on from the account that got it, `Fwd: ` once, to no one yet: under the
    /// text go `labels` with its sender, `date` as the app writes it and subject, then its
    /// text, downloaded if not saved yet with `secret`, as for `message_body`.
    pub async fn forward(
        &self,
        message: MessageItem,
        labels: ForwardLabels,
        date: String,
        secret: String,
    ) -> Result<DraftItem, CoreError> {
        let id = MessageId(message.id);
        let text = self.body(id, &secret).await?.text;
        let from = match shown_name(message.from_name.as_deref()) {
            Some(name) => format!("{name} <{}>", message.from_email),
            None => message.from_email.clone(),
        };
        let fields = [
            (labels.from.as_str(), from.as_str()),
            (&labels.date, &date),
            (&labels.subject, &message.subject),
        ];
        Ok(DraftItem {
            id: 0,
            account_id: message.account_id,
            to: String::new(),
            subject: ozero_domain::forward_subject(&message.subject),
            text: String::new(),
            reply_to: None,
            quote: ozero_domain::forward_quote(&labels.title, &fields, &text),
            saved: 0,
            forward: Some(message.id),
            files: vec![],
            cc: String::new(),
            // Removed meanwhile: the send says so.
            signature: self
                .account(AccountId(message.account_id))
                .and_then(|a| self.signs_with(&a))
                .unwrap_or_default(),
        })
    }

    /// The content of the message's attachment `part`, downloaded now with `secret` as for
    /// `message_body`; `Gone` once the message is off the server or the part is not in it.
    pub async fn download_file(
        &self,
        message: i64,
        part: u32,
        secret: String,
    ) -> Result<Vec<u8>, CoreError> {
        let found = self.download_parts(message, &[part], &secret).await?;
        found.into_iter().next().flatten().ok_or(CoreError::Gone)
    }

    /// The message's attachment `part`s, in order, from one download of the whole message;
    /// none for a part it lacks.
    async fn download_parts(
        &self,
        message: i64,
        parts: &[u32],
        secret: &str,
    ) -> Result<Vec<Option<Vec<u8>>>, CoreError> {
        let location = self.store.location(MessageId(message))?;
        let account = self.account(location.account_id)?;
        let mut slot = None;
        let remote = self.remote(&mut slot, &account, secret).await?;
        let found =
            download_attachments(&*self.store, remote, &self.parser, &location, parts).await;
        drop(slot);
        self.refused(&account, found)
    }

    /// Writes the message's attachment `part`s to their `path`s, from one download of the
    /// whole message; each file appears whole or not at all. `secret` as for `message_body`.
    pub async fn save_attachments(
        &self,
        message: i64,
        files: Vec<AttachmentFile>,
        secret: String,
    ) -> Result<(), CoreError> {
        let parts: Vec<u32> = files.iter().map(|f| f.part).collect();
        let found = self.download_parts(message, &parts, &secret).await?;
        // Every part there is gets written; one the whole message lacks is as good as gone.
        let gone = found.iter().any(Option::is_none);
        let write = move || {
            for (file, data) in files.iter().zip(found) {
                let Some(data) = data else { continue };
                let partial = format!("{}.partial", file.path);
                std::fs::write(&partial, data)
                    .and_then(|()| std::fs::rename(&partial, &file.path))?;
            }
            Ok::<_, std::io::Error>(())
        };
        let storage = |detail: String| CoreError::Storage { detail };
        tokio::task::spawn_blocking(write)
            .await
            .map_err(|e| storage(e.to_string()))?
            .map_err(|e| storage(e.to_string()))?;
        if gone {
            return Err(CoreError::Gone);
        }
        Ok(())
    }

    /// The message's page with its remote images loaded now, for `MessageBody::html`; None
    /// when it has none. An image that does not load stays out.
    pub async fn message_images(&self, message: i64) -> Result<Option<ImagesPage>, CoreError> {
        let message = MessageId(message);
        let Some(source) = self.store.body(message)?.and_then(|b| b.html) else {
            return Ok(None);
        };
        // The same id can come back for another message, so the page must match too.
        let same = |k: &KeptPage| k.message == message && k.source == source;
        {
            let mut pages = self.kept_pages();
            if let Some(kept) = pages.iter().position(same).and_then(|n| pages.remove(n)) {
                let page = kept.page.clone();
                pages.push_back(kept);
                return Ok(Some(page));
            }
        }
        let page = self.load_images(message, &source).await?;
        // A page with an image missing tries again next time.
        if let Some(page) = page
            .as_ref()
            .filter(|p| p.images.iter().all(Option::is_some))
        {
            let mut pages = self.kept_pages();
            pages.retain(|k| k.message != message);
            if pages.len() == PAGES_KEPT {
                pages.pop_front();
            }
            pages.push_back(KeptPage {
                message,
                source,
                page: page.clone(),
            });
        }
        Ok(page)
    }

    /// From now on the remote images of mail from this message's sender load on their own.
    pub fn allow_images(&self, message: i64) -> Result<(), CoreError> {
        Ok(self.store.allow_images(MessageId(message))?)
    }
}

/// How many remote images a message loads, how many at a time, and how many bytes in all.
const IMAGES: usize = 48;
const IMAGES_AT_ONCE: usize = 6;
const IMAGES_SIZE: usize = 12 * 1024 * 1024;
/// How many pages with their images stay in memory.
const PAGES_KEPT: usize = 2;

/// The host a page asks for its loaded images on; `.invalid` never reaches the network.
macro_rules! image_host {
    () => {
        "images.ozero.invalid"
    };
}

/// Where a page asks for its loaded image `N`: `{IMAGE_ADDRESS}{message}/{N}`. The app answers
/// these from `ImagesPage::images`. The web view may keep images in memory by address across
/// pages, so the message id keeps another message's image `N` from showing.
const IMAGE_ADDRESS: &str = concat!("https://", image_host!(), "/");

/// The host a plain page asks for the app's reading font on: `{FONT_HOST}/literata.ttf`.
macro_rules! font_host {
    () => {
        "fonts.ozero.invalid"
    };
}

impl Core {
    /// Downloads the page's remote images for `message_images`.
    async fn load_images(
        &self,
        message: MessageId,
        html: &str,
    ) -> Result<Option<ImagesPage>, CoreError> {
        let page = html::clean(html);
        if page.images.is_empty() {
            return Ok(None);
        }
        let tls = self.tls()?;
        let slots = Arc::new(tokio::sync::Semaphore::new(IMAGES_AT_ONCE));
        // A JoinSet stops the downloads when the reader leaves and this future is dropped.
        let mut fetches = tokio::task::JoinSet::new();
        for (n, url) in page.images.iter().take(IMAGES).enumerate() {
            let (url, tls, slots) = (url.clone(), tls.clone(), slots.clone());
            fetches.spawn(async move {
                let _slot = slots.acquire().await;
                (n, ozero_http::image(&url, tls).await.ok())
            });
        }
        let mut images = vec![None; fetches.len()];
        let mut left = IMAGES_SIZE;
        while let Some(fetched) = fetches.join_next().await {
            let Ok((n, Some(image))) = fetched else {
                continue;
            };
            let Some(image) = RemoteImage::new(&image.content_type, image.body) else {
                continue;
            };
            if image.data.len() <= left {
                left -= image.data.len();
                images[n] = Some(image);
            }
        }
        // Addresses, not `data:` copies: a page with all its images inline is too big to
        // pass around on the app's side.
        let sources: Vec<_> = images
            .iter()
            .enumerate()
            .map(|(n, image)| {
                image
                    .as_ref()
                    .map(|_| format!("{IMAGE_ADDRESS}{}/{n}", message.0))
            })
            .collect();
        Ok(Some(ImagesPage {
            html: sandboxed(
                &html::fold_quote(&html::with_images(&page.html, &sources)),
                html::is_styled(&page.html),
            ),
            images,
        }))
    }

    fn kept_pages(&self) -> std::sync::MutexGuard<'_, VecDeque<KeptPage>> {
        self.pages.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The account's folders in `mailbox` whose server may have older mail to load, with when
    /// their oldest saved message was received.
    fn loadable(
        &self,
        account: AccountId,
        mailbox: Mailbox,
    ) -> Result<Vec<(Folder, Option<i64>)>, CoreError> {
        let mut loadable = vec![];
        for folder in self.store.folders(account)? {
            if !mailbox.holds(&folder) {
                continue;
            }
            if !self.store.older_done(folder.id)? {
                let oldest = self.store.oldest(folder.id)?;
                loadable.push((folder, oldest));
            }
        }
        Ok(loadable)
    }

    /// The rows of a list of `mailbox`'s folders, as far down as all of them have their mail
    /// saved: the latest of their oldest saved messages, leaving out those with nothing older
    /// to load. Mail older than that would leave a gap in the list when the others load theirs.
    fn list(
        &self,
        scope: ListScope,
        mailbox: Mailbox,
        limit: u32,
    ) -> Result<Vec<MessageItem>, CoreError> {
        let mut rows = self.store.list(&ListQuery {
            scope,
            limit,
            after: None,
        })?;
        let mut floor = None;
        for account in self.store.accounts()? {
            for (_, oldest) in self.loadable(account.id, mailbox)? {
                floor = floor.max(oldest);
            }
        }
        if let Some(floor) = floor {
            rows.retain(|r| r.date >= floor);
        }
        self.items(rows)
    }

    /// Queues a move held for the undo time and returns its ops for `undo`.
    fn hold_move(&self, messages: Vec<i64>, action: Action) -> Result<Vec<i64>, CoreError> {
        Ok(ids(self
            .store
            .queue_all(&with(messages, action), now() + HOLD)?))
    }

    /// List rows, marked with their account when there is more than one.
    fn items(&self, rows: Vec<MessageRow>) -> Result<Vec<MessageItem>, CoreError> {
        let accounts = self.store.accounts()?;
        let marks: HashMap<AccountId, AccountMark> = if accounts.len() > 1 {
            accounts
                .iter()
                .map(|a| (a.id, AccountMark::from(a)))
                .collect()
        } else {
            HashMap::new()
        };
        Ok(rows
            .into_iter()
            .map(|row| {
                let account = marks.get(&row.account_id).cloned();
                MessageItem {
                    account,
                    ..MessageItem::from(row)
                }
            })
            .collect())
    }

    /// What goes under the mail of `account`.
    fn signs_with(&self, account: &Account) -> Result<String, CoreError> {
        Ok(account.signs_with(&self.store.signature()?).to_owned())
    }

    /// `result`, or nothing done if it failed because the account was removed meanwhile.
    fn unless_removed<T: Default>(
        &self,
        account: AccountId,
        result: Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        match result {
            Err(_) if matches!(self.find(account), Ok(None)) => Ok(T::default()),
            result => result,
        }
    }

    fn busy(&self, account: AccountId) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.busy.lock().unwrap_or_else(|p| p.into_inner());
        locks.entry(account).or_default().clone()
    }

    fn account(&self, id: AccountId) -> Result<Account, CoreError> {
        self.find(id)?.ok_or_else(|| CoreError::Storage {
            detail: "no such account".into(),
        })
    }

    fn find(&self, id: AccountId) -> Result<Option<Account>, CoreError> {
        Ok(self.store.accounts()?.into_iter().find(|a| a.id == id))
    }

    fn exists(&self, email: &str) -> Result<bool, CoreError> {
        let known = self.store.accounts()?;
        Ok(known.iter().any(|a| a.email.eq_ignore_ascii_case(email)))
    }

    /// The saved body, or downloaded once and saved with its snippet.
    async fn body(&self, message: MessageId, secret: &str) -> Result<Body, CoreError> {
        if let Some(body) = self.store.body(message)? {
            return Ok(body);
        }
        let location = self.store.location(message)?;
        let account = self.account(location.account_id)?;
        let mut slot = None;
        let remote = self.remote(&mut slot, &account, secret).await?;
        let body = download_body(&*self.store, remote, &self.parser, &location).await;
        // Closing without LOGOUT saves the reader a round trip; the server cleans up.
        drop(slot);
        self.refused(&account, body)
    }

    /// The connection in `slot`, opened on first use.
    async fn remote<'a>(
        &self,
        slot: &'a mut Option<Remote>,
        account: &Account,
        secret: &str,
    ) -> Result<&'a mut Remote, CoreError> {
        if let Some(remote) = slot {
            return Ok(remote);
        }
        let opened = self.signed_in(account, async {
            Ok(match account.kind {
                AccountKind::Imap => Remote::Imap(Box::new(self.connect(account, secret).await?)),
                AccountKind::Gmail => {
                    let token = self.access_token(account.id, secret).await?;
                    Remote::Gmail(GmailSession::new(self.tls()?, &token))
                }
                AccountKind::Exchange if self.speaks_active_sync(account.id)? => {
                    Remote::Eas(self.eas(account, secret).await?)
                }
                AccountKind::Exchange => Remote::Ews(self.ews(account, secret).await?),
                AccountKind::Graph => {
                    return Err(CoreError::Storage {
                        detail: "Outlook accounts are not supported yet".into(),
                    });
                }
            })
        });
        Ok(slot.insert(opened.await?))
    }

    /// Logs in with `login` unless the account waits for the person to sign in again: a
    /// server may lock an account after many refused logins. A refused one marks it so.
    async fn signed_in<T>(
        &self,
        account: &Account,
        login: impl Future<Output = Result<T, CoreError>>,
    ) -> Result<T, CoreError> {
        if account.needs_sign_in {
            return Err(CoreError::Auth);
        }
        let result = login.await;
        if let Err(CoreError::Auth) = result {
            // An account removed meanwhile has nothing to mark.
            let _ = self.store.set_needs_sign_in(account.id, true);
        }
        result
    }

    /// A Gmail access token: the one from earlier while it works, else a new one.
    async fn access_token(&self, account: AccountId, secret: &str) -> Result<String, CoreError> {
        {
            let tokens = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((token, until)) = tokens.get(&account)
                && *until > SystemTime::now()
            {
                return Ok(token.clone());
            }
        }
        let fresh = oauth::refresh(self.tls()?, GOOGLE_CLIENT_ID, secret).await?;
        let lives = Duration::from_secs(fresh.expires_in).saturating_sub(TOKEN_MARGIN);
        let mut tokens = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
        tokens.insert(account, (fresh.access.clone(), SystemTime::now() + lives));
        Ok(fresh.access)
    }

    /// Passes `result` on; a refused login drops the account's access token, so the next
    /// call gets a new one. Only a refused new one asks the person to sign in again.
    fn refused<T, E: Into<CoreError>>(
        &self,
        account: &Account,
        result: Result<T, E>,
    ) -> Result<T, CoreError> {
        let result = result.map_err(Into::into);
        if let Err(CoreError::Auth) = result {
            let mut tokens = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
            tokens.remove(&account.id);
        }
        result
    }

    /// Saves an Exchange account that opens only ActiveSync, once the server has taken the
    /// login and let the device in.
    async fn add_active_sync_account(
        &self,
        email: String,
        display_name: String,
        server: Server,
        tls: Tls,
        password: String,
    ) -> Result<i64, CoreError> {
        let kept = Arc::new(Kept::default());
        EasSession::register(&*kept)?;
        let mut eas = EasSession::new(tls, &server, &password, kept.clone())?;
        eas.check().await?;
        let display_name = match display_name.trim() {
            // Without it the mail goes out under the address alone.
            "" => eas
                .name()
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| email.clone()),
            name => name.to_owned(),
        };
        let account = NewAccount {
            kind: AccountKind::Exchange,
            email,
            display_name,
            server: Some(server),
            smtp: None,
        };
        let id = self.store.add_account(&account)?;
        let state = [(PROTOCOL.to_owned(), ACTIVE_SYNC.to_owned())]
            .into_iter()
            .chain(kept.take())
            .try_for_each(|(key, value)| self.store.set_provider_state(id, &key, Some(&value)));
        // Without its state the account would speak EWS, or be another phone.
        if let Err(e) = state {
            let _ = self.store.remove_account(id);
            return Err(e.into());
        }
        Ok(id.0)
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_exchange(
        &self,
        email: String,
        display_name: String,
        server: Option<String>,
        login: Option<String>,
        certificate: Option<String>,
        (tls, active_sync): (Arc<ClientConfig>, Tls),
        password: String,
    ) -> Result<i64, CoreError> {
        if self.exists(&email)? {
            return Err(CoreError::AccountExists);
        }
        let login = login
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| email.clone());
        let (endpoint, mut ews) = match server.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(typed) => {
                let endpoint = discover::endpoint(typed).ok_or(CoreError::NoServer)?;
                let ews = match discover::check(tls.clone(), &endpoint, &login, &password).await {
                    // Refused, or the connection broke off: maybe for want of a certificate.
                    Err(ProviderError::Rejected(_) | ProviderError::Network(_))
                        if certificate.is_none()
                            && discover::asks_for_certificate(tls.clone(), &endpoint).await =>
                    {
                        return Err(CoreError::CertificateNeeded);
                    }
                    // Something answers there with an HTTP error, but not EWS. Try ActiveSync
                    // with the login: a front server may refuse anyone who has not signed in.
                    Err(ProviderError::Rejected(_)) => {
                        let server = Server {
                            host: endpoint.host,
                            port: endpoint.port,
                            user: login,
                            certificate,
                        };
                        return match self
                            .add_active_sync_account(
                                email,
                                display_name,
                                server,
                                active_sync,
                                password,
                            )
                            .await
                        {
                            Err(CoreError::Server { .. }) => Err(CoreError::NoEws),
                            // Said under the error, so the person's IT team sees which part.
                            Err(CoreError::Network { detail }) => Err(CoreError::Network {
                                detail: format!("ActiveSync: {detail}"),
                            }),
                            added => added,
                        };
                    }
                    found => found?,
                };
                (endpoint, ews)
            }
            None => match discover::discover(tls, &email, &login, &password).await {
                Err(ProviderError::Gone) => return Err(CoreError::NoServer),
                found => found?,
            },
        };
        let display_name = match display_name.trim() {
            // Without it the mail goes out under the address alone.
            "" => ews
                .name(&email)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| email.clone()),
            name => name.to_owned(),
        };
        let server = Server {
            host: endpoint.host,
            port: endpoint.port,
            user: login,
            certificate,
        };
        let account = NewAccount {
            kind: AccountKind::Exchange,
            email,
            display_name,
            server: Some(server),
            smtp: None,
        };
        Ok(self.store.add_account(&account)?.0)
    }

    /// The server certificate the person chose to trust for a saved account, if any.
    fn trusted(&self, account: AccountId) -> Result<Option<[u8; 32]>, CoreError> {
        let trusted = self.store.provider_state(account, TRUSTED)?;
        Ok(trusted.as_deref().and_then(unhex))
    }

    /// A Gmail inbox opened to wait in IDLE, signed in to Gmail's IMAP with the account's API
    /// `token`: the API has no push without a server of ours. Its failures leave the account
    /// signed in.
    async fn gmail_watch(
        &self,
        account: &Account,
        token: &str,
    ) -> Result<(ImapSession, Watch), CoreError> {
        let mut imap =
            ImapSession::connect_oauth(GMAIL_IMAP, IMAPS_PORT, self.tls()?, &account.email, token)
                .await?;
        let watch = imap.begin_watch("INBOX").await?;
        Ok((imap, watch))
    }

    /// Whether a saved Exchange account speaks ActiveSync rather than EWS.
    fn speaks_active_sync(&self, account: AccountId) -> Result<bool, CoreError> {
        let protocol = self.store.provider_state(account, PROTOCOL)?;
        Ok(protocol.as_deref() == Some(ACTIVE_SYNC))
    }

    /// A session with a saved ActiveSync account. Each request signs in, so the first one
    /// finds a wrong password.
    async fn eas(&self, account: &Account, password: &str) -> Result<EasSession, CoreError> {
        let server = account.server.as_ref().ok_or_else(|| CoreError::Storage {
            detail: "no Exchange server".into(),
        })?;
        let (certificate, trusted) = (server.certificate.as_deref(), self.trusted(account.id)?);
        let tls = match self.tunnel(certificate, trusted, None)? {
            Some(tunnel) => tunnel,
            None => self.tls_for(certificate, trusted)?.into(),
        };
        let state = Arc::new(AccountState {
            store: self.store.clone(),
            account: account.id,
        });
        Ok(EasSession::new(tls, server, password, state)?.keeping(account.keep_days))
    }

    /// Signs in to a saved Exchange account: one call, so a wrong password is found before
    /// more of them count against the company's lockout.
    async fn ews(&self, account: &Account, password: &str) -> Result<EwsSession, CoreError> {
        let server = account.server.as_ref().ok_or_else(|| CoreError::Storage {
            detail: "no Exchange server".into(),
        })?;
        let tls = self.tls_for(server.certificate.as_deref(), self.trusted(account.id)?)?;
        let mut ews = EwsSession::new(tls, server, password);
        ews.check().await?;
        Ok(ews)
    }

    /// Logs in to a saved IMAP account.
    async fn connect(&self, account: &Account, password: &str) -> Result<ImapSession, CoreError> {
        let server = account.server.as_ref().ok_or_else(|| CoreError::Storage {
            detail: "no IMAP server".into(),
        })?;
        self.login(server, password).await
    }

    /// Sends what is due: actions, then mail; over IMAP and SMTP with a copy into Sent, or
    /// over the Gmail API. Connects only when it is needed and leaves the connection in `slot`.
    async fn push_due(
        &self,
        account: &Account,
        secret: &str,
        slot: &mut Option<Remote>,
    ) -> Result<(), CoreError> {
        // Mail first: an action may take a forwarded message off the device, and its files
        // with it. Mail that cannot go does not hold the actions back.
        let mailed = self.push_mail(account, secret, slot).await;
        // Boxed, as below: connecting inline overflows a debug build's stack.
        Box::pin(self.send_ops(account, secret, slot)).await?;
        // After the mail, so the copies of drafts just sent go too. Drafts stay on the device
        // until a push goes through, so a failure here is no failure of the sync: the inbox
        // must not say the account is unreachable for it.
        if let Err(e) = Box::pin(self.push_drafts(account, secret, slot)).await {
            log::warn!("drafts not saved to the server: {e:?}");
        }
        // Moved mail shows in its new folder as a copy; a sync there brings the server's own,
        // so what the person did to the copy meanwhile can go too.
        let unclaimed = self.store.unclaimed(account.id)?;
        if !unclaimed.is_empty() {
            let remote = Box::pin(self.remote(slot, account, secret)).await?;
            let (parser, since) = (&self.parser, keep_since(account.keep_days));
            let synced = sync_folders(&*self.store, remote, parser, &unclaimed, SYNC_LIMIT, since);
            // The actions went already: what waits on the copies goes with a later push.
            if Box::pin(synced).await.is_ok() {
                Box::pin(self.send_ops(account, secret, slot)).await?;
            }
        }
        mailed
    }

    /// Saves the account's drafts into its drafts folder and deletes their stale copies;
    /// connects only when there is any to save or delete.
    async fn push_drafts(
        &self,
        account: &Account,
        secret: &str,
        slot: &mut Option<Remote>,
    ) -> Result<(), CoreError> {
        let folders = self.store.folders(account.id)?;
        // Kept on the device until a sync finds the folder.
        let Some(folder) = folders.iter().find(|f| f.role == FolderRole::Drafts) else {
            return Ok(());
        };
        let drafts = self.store.drafts_to_push(account.id)?;
        if drafts.is_empty() && self.store.stale_drafts(account.id, now())?.is_empty() {
            return Ok(());
        }
        let remote = self.remote(slot, account, secret).await?;
        let from = sender(account);
        let to = DraftsFolder {
            account: account.id,
            from: &from,
            folder: &folder.remote_id,
            message_id: &|| message_id(&account.email, now()),
        };
        push_drafts(&*self.store, remote, &MimeBuilder, to, drafts, now()).await?;
        Ok(())
    }

    /// Sends the account's due actions; connects only when there are any.
    async fn send_ops(
        &self,
        account: &Account,
        secret: &str,
        slot: &mut Option<Remote>,
    ) -> Result<(), CoreError> {
        let ops = self.store.due(account.id, now())?;
        if !ops.is_empty() {
            let remote = self.remote(slot, account, secret).await?;
            push_ops(&*self.store, remote, ops).await?;
        }
        Ok(())
    }

    /// The sender name of a Gmail account saved before names were read, asked for once a
    /// session; a failure stops nothing, and the next send asks again.
    async fn gmail_name(
        &self,
        account: &Account,
        secret: &str,
        slot: &mut Option<Remote>,
    ) -> Option<String> {
        if self.named().contains(&account.id) {
            return None;
        }
        let Ok(Remote::Gmail(gmail)) = Box::pin(self.remote(slot, account, secret)).await else {
            return None;
        };
        let name = match gmail.name().await {
            Ok(name) => name,
            Err(e) => {
                log::warn!("no sender name from Gmail: {e:?}");
                return None;
            }
        };
        self.named().insert(account.id);
        let name = name?;
        if let Err(e) = self.store.set_account_name(account.id, &name) {
            log::warn!("sender name not saved: {e:?}");
        }
        Some(name)
    }

    fn named(&self) -> std::sync::MutexGuard<'_, HashSet<AccountId>> {
        self.named.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Sends the account's due mail, one message at a time, so only one message's files are
    /// held. The larger futures are boxed: inline, they overflow a 2 MB stack in debug builds.
    async fn push_mail(
        &self,
        account: &Account,
        secret: &str,
        slot: &mut Option<Remote>,
    ) -> Result<(), CoreError> {
        let mail = self.store.due_sends(account.id, now())?;
        if mail.is_empty() {
            return Ok(());
        }
        let mut from = sender(account);
        if matches!(account.kind, AccountKind::Gmail | AccountKind::Exchange) {
            if account.kind == AccountKind::Gmail && from.name.is_none() {
                from.name = Box::pin(self.gmail_name(account, secret, slot)).await;
            }
            for message in mail {
                let message = Box::pin(self.with_files(account, secret, slot, message)).await?;
                // Gmail and Exchange keep the copy in Sent themselves.
                let copy: Option<SentCopy<'_, Remote>> = None;
                let remote = Box::pin(self.remote(slot, account, secret)).await?;
                let one = vec![message];
                push_sends(&*self.store, remote, &MimeBuilder, &from, copy, one).await?;
            }
            return Ok(());
        }
        let server = account.smtp.as_ref().ok_or(CoreError::NoSmtp)?;
        let sent = if saves_sent_itself(&server.host) {
            None
        } else {
            let folders = self.store.folders(account.id)?;
            folders.into_iter().find(|f| f.role == FolderRole::Sent)
        };
        let (mut smtp, mut no_imap) = (None, false);
        for message in mail {
            let message = Box::pin(self.with_files(account, secret, slot, message)).await?;
            // Signed in after the files came, so the session is not left idle meanwhile.
            let mut session = match smtp.take() {
                Some(session) => session,
                None => {
                    self.signed_in(account, self.send_login(server, secret))
                        .await?
                }
            };
            // Without IMAP the mail still goes; only its copy in Sent is missing.
            let copy = match &sent {
                Some(folder) if !no_imap => {
                    match Box::pin(self.remote(slot, account, secret)).await {
                        Ok(provider) => Some(SentCopy {
                            provider,
                            folder: &folder.remote_id,
                        }),
                        Err(_) => {
                            no_imap = true;
                            None
                        }
                    }
                }
                _ => None,
            };
            let one = vec![message];
            push_sends(&*self.store, &mut session, &MimeBuilder, &from, copy, one).await?;
            smtp = Some(session);
        }
        if let Some(session) = smtp {
            let _ = session.quit().await;
        }
        Ok(())
    }

    /// The message with the files added to it, then those of the message it forwards, read
    /// from the server.
    async fn with_files(
        &self,
        account: &Account,
        secret: &str,
        slot: &mut Option<Remote>,
        mut message: Outgoing,
    ) -> Result<Outgoing, CoreError> {
        message.files = self.store.outbox_files(message.id)?;
        if let Some(original) = message.forward {
            let remote = self.remote(slot, account, secret).await?;
            let files = forwarded_files(&*self.store, remote, &self.parser, account.id, original);
            message.files.extend(files.await?);
        }
        Ok(message)
    }

    async fn send_login(&self, server: &Server, password: &str) -> Result<SmtpSession, CoreError> {
        Ok(SmtpSession::connect(
            &server.host,
            server.port,
            self.tls_for(server.certificate.as_deref(), None)?,
            &server.user,
            password,
        )
        .await?)
    }

    async fn login(&self, server: &Server, password: &str) -> Result<ImapSession, CoreError> {
        Ok(ImapSession::connect(
            &server.host,
            server.port,
            self.tls_for(server.certificate.as_deref(), None)?,
            &server.user,
            password,
        )
        .await?)
    }

    /// Built on first use: on Android `CoreNative.init` must have run before that.
    fn tls(&self) -> Result<Arc<ClientConfig>, CoreError> {
        if let Some(tls) = self.tls.get() {
            return Ok(tls.clone());
        }
        let tls = tls_config(None, None, None)?;
        Ok(self.tls.get_or_init(|| tls).clone())
    }

    /// TLS settings that offer the platform's client certificate `certificate`, if any, and
    /// take the server certificate `trusted`, the SHA-256 of one the person chose to trust.
    fn tls_for(
        &self,
        certificate: Option<&str>,
        trusted: Option<[u8; 32]>,
    ) -> Result<Arc<ClientConfig>, CoreError> {
        if certificate.is_none() && trusted.is_none() {
            return self.tls();
        }
        let key = (certificate.map(str::to_owned), trusted);
        let mut built = self
            .certificate_tls
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(tls) = built.get(&key) {
            return Ok(tls.clone());
        }
        let client = self.client_certificate(certificate)?;
        let tls = tls_config(client, trusted, None)?;
        built.insert(key, tls.clone());
        Ok(tls)
    }

    /// The platform's TLS offering `certificate`, when there is one and the app gave it.
    fn tunnel(
        &self,
        certificate: Option<&str>,
        trusted: Option<[u8; 32]>,
        refused: Option<Arc<RefusedCertificates>>,
    ) -> Result<Option<Tls>, CoreError> {
        let (Some(certificate), Some(tls)) = (certificate, self.platform_tls.get()) else {
            return Ok(None);
        };
        Ok(Some(Tls::Tunnel(Arc::new(tunnel::Platform {
            tls: tls.clone(),
            certificate: certificate.to_owned(),
            checks: server_checks(trusted, refused)?,
        }))))
    }

    /// Offers the platform's client certificate `certificate` to a server that asks.
    fn client_certificate(
        &self,
        certificate: Option<&str>,
    ) -> Result<Option<Arc<dyn ResolvesClientCert>>, CoreError> {
        let Some(name) = certificate else {
            return Ok(None);
        };
        let certificates = self.certificates.get().ok_or_else(|| CoreError::Tls {
            detail: "the app gave no client certificates".into(),
        })?;
        Ok(Some(Arc::new(certificate::Resolver::new(
            certificates.clone(),
            name,
        ))))
    }
}

/// The provider state key that says which protocol an Exchange account speaks.
const PROTOCOL: &str = "protocol";
const ACTIVE_SYNC: &str = "activesync";
/// The provider state key of the server certificate the person chose to trust, as `hex`.
const TRUSTED: &str = "trusted_certificate";

/// A fingerprint as the apps show it: upper-case hex pairs split by colons.
fn hex(fingerprint: &[u8; 32]) -> String {
    let pairs: Vec<String> = fingerprint.iter().map(|b| format!("{b:02X}")).collect();
    pairs.join(":")
}

/// A fingerprint from `hex`; None for anything else.
fn unhex(text: &str) -> Option<[u8; 32]> {
    let bytes: Vec<u8> = text
        .split(':')
        .map(|pair| {
            u8::from_str_radix(pair, 16)
                .ok()
                .filter(|_| pair.len() == 2)
        })
        .collect::<Option<_>>()?;
    bytes.try_into().ok()
}

/// An account's provider state in the store. A failed write is logged: the provider then
/// starts over where it needed the value.
struct AccountState {
    store: Arc<SqliteStore>,
    account: AccountId,
}

impl ProviderState for AccountState {
    fn get(&self, key: &str) -> Option<String> {
        self.store
            .provider_state(self.account, key)
            .unwrap_or_else(|e| {
                log::warn!("no provider state {key}: {e}");
                None
            })
    }

    fn set(&self, key: &str, value: Option<&str>) {
        if let Err(e) = self.store.set_provider_state(self.account, key, value) {
            log::warn!("provider state {key} not kept: {e}");
        }
    }
}

/// Provider state kept in memory while signing in, before the account is saved.
#[derive(Default)]
struct Kept(Mutex<HashMap<String, String>>);

impl Kept {
    fn take(&self) -> HashMap<String, String> {
        std::mem::take(&mut self.0.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

impl ProviderState for Kept {
    fn get(&self, key: &str) -> Option<String> {
        let kept = self.0.lock().unwrap_or_else(|p| p.into_inner());
        kept.get(key).cloned()
    }

    fn set(&self, key: &str, value: Option<&str>) {
        let mut kept = self.0.lock().unwrap_or_else(|p| p.into_inner());
        match value {
            Some(v) => kept.insert(key.to_owned(), v.to_owned()),
            None => kept.remove(key),
        };
    }
}

/// The open connection to an account's server.
enum Remote {
    Imap(Box<ImapSession>),
    Gmail(GmailSession),
    Ews(EwsSession),
    Eas(EasSession),
}

impl Remote {
    async fn close(self) {
        if let Self::Imap(imap) = self {
            let _ = imap.logout().await;
        }
    }
}

/// Sends through the provider's own call, where it has one.
impl MailSender for Remote {
    async fn send(
        &mut self,
        from: &str,
        to: &[String],
        message: &[u8],
    ) -> Result<(), ProviderError> {
        match self {
            Self::Gmail(p) => p.send(from, to, message).await,
            Self::Ews(p) => p.send(from, to, message).await,
            Self::Eas(p) => p.send(from, to, message).await,
            Self::Imap(_) => Err(ProviderError::Rejected("IMAP sends over SMTP".into())),
        }
    }
}

/// Passes each call on to the account's own provider.
impl MailProvider for Remote {
    async fn folders(&mut self) -> Result<Vec<NewFolder>, ProviderError> {
        match self {
            Self::Imap(p) => p.folders().await,
            Self::Gmail(p) => p.folders().await,
            Self::Ews(p) => p.folders().await,
            Self::Eas(p) => p.folders().await,
        }
    }

    async fn changes(
        &mut self,
        folder: &str,
        since: Checkpoint,
        known: &[String],
        oldest: i64,
        limit: u32,
    ) -> Result<Changes, ProviderError> {
        match self {
            Self::Imap(p) => p.changes(folder, since, known, oldest, limit).await,
            Self::Gmail(p) => p.changes(folder, since, known, oldest, limit).await,
            Self::Ews(p) => p.changes(folder, since, known, oldest, limit).await,
            Self::Eas(p) => p.changes(folder, since, known, oldest, limit).await,
        }
    }

    async fn older(
        &mut self,
        folder: &str,
        validity: u32,
        known: &[String],
        oldest: i64,
        limit: u32,
    ) -> Result<Older, ProviderError> {
        match self {
            Self::Imap(p) => p.older(folder, validity, known, oldest, limit).await,
            Self::Gmail(p) => p.older(folder, validity, known, oldest, limit).await,
            Self::Ews(p) => p.older(folder, validity, known, oldest, limit).await,
            Self::Eas(p) => p.older(folder, validity, known, oldest, limit).await,
        }
    }

    async fn search(
        &mut self,
        folder: &str,
        validity: u32,
        text: &str,
        limit: u32,
    ) -> Result<Vec<RawMessage>, ProviderError> {
        match self {
            Self::Imap(p) => p.search(folder, validity, text, limit).await,
            Self::Gmail(p) => p.search(folder, validity, text, limit).await,
            Self::Ews(p) => p.search(folder, validity, text, limit).await,
            Self::Eas(p) => p.search(folder, validity, text, limit).await,
        }
    }

    async fn body(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> Result<Vec<u8>, ProviderError> {
        match self {
            Self::Imap(p) => p.body(folder, validity, remote_id).await,
            Self::Gmail(p) => p.body(folder, validity, remote_id).await,
            Self::Ews(p) => p.body(folder, validity, remote_id).await,
            Self::Eas(p) => p.body(folder, validity, remote_id).await,
        }
    }

    async fn message(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> Result<Vec<u8>, ProviderError> {
        match self {
            Self::Imap(p) => p.message(folder, validity, remote_id).await,
            Self::Gmail(p) => p.message(folder, validity, remote_id).await,
            Self::Ews(p) => p.message(folder, validity, remote_id).await,
            Self::Eas(p) => p.message(folder, validity, remote_id).await,
        }
    }

    async fn set_seen(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        seen: bool,
    ) -> Result<(), ProviderError> {
        match self {
            Self::Imap(p) => p.set_seen(folder, validity, remote_ids, seen).await,
            Self::Gmail(p) => p.set_seen(folder, validity, remote_ids, seen).await,
            Self::Ews(p) => p.set_seen(folder, validity, remote_ids, seen).await,
            Self::Eas(p) => p.set_seen(folder, validity, remote_ids, seen).await,
        }
    }

    async fn move_to(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> Result<Moved, ProviderError> {
        match self {
            Self::Imap(p) => p.move_to(folder, validity, remote_ids, to).await,
            Self::Gmail(p) => p.move_to(folder, validity, remote_ids, to).await,
            Self::Ews(p) => p.move_to(folder, validity, remote_ids, to).await,
            Self::Eas(p) => p.move_to(folder, validity, remote_ids, to).await,
        }
    }

    async fn delete(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
    ) -> Result<(), ProviderError> {
        match self {
            Self::Imap(p) => p.delete(folder, validity, remote_ids).await,
            Self::Gmail(p) => p.delete(folder, validity, remote_ids).await,
            Self::Ews(p) => p.delete(folder, validity, remote_ids).await,
            Self::Eas(p) => p.delete(folder, validity, remote_ids).await,
        }
    }

    async fn empty(
        &mut self,
        folder: &str,
        validity: u32,
        keep: &[String],
    ) -> Result<(), ProviderError> {
        match self {
            Self::Imap(p) => p.empty(folder, validity, keep).await,
            Self::Gmail(p) => p.empty(folder, validity, keep).await,
            Self::Ews(p) => p.empty(folder, validity, keep).await,
            Self::Eas(p) => p.empty(folder, validity, keep).await,
        }
    }

    async fn create_snoozed(&mut self, name: &str) -> Result<NewFolder, ProviderError> {
        match self {
            Self::Imap(p) => p.create_snoozed(name).await,
            Self::Gmail(p) => p.create_snoozed(name).await,
            Self::Ews(p) => p.create_snoozed(name).await,
            Self::Eas(p) => p.create_snoozed(name).await,
        }
    }

    async fn append(&mut self, folder: &str, message: &[u8]) -> Result<(), ProviderError> {
        match self {
            Self::Imap(p) => p.append(folder, message).await,
            Self::Gmail(p) => p.append(folder, message).await,
            Self::Ews(p) => p.append(folder, message).await,
            Self::Eas(p) => p.append(folder, message).await,
        }
    }

    async fn save_draft(
        &mut self,
        folder: &str,
        message: &[u8],
        message_id: &str,
    ) -> Result<Option<(u32, String)>, ProviderError> {
        match self {
            Self::Imap(p) => p.save_draft(folder, message, message_id).await,
            Self::Gmail(p) => p.save_draft(folder, message, message_id).await,
            Self::Ews(p) => p.save_draft(folder, message, message_id).await,
            Self::Eas(p) => p.save_draft(folder, message, message_id).await,
        }
    }

    async fn wait(
        &mut self,
        folder: &str,
        since: Checkpoint,
        wait: Duration,
    ) -> Result<bool, ProviderError> {
        match self {
            Self::Imap(p) => p.wait(folder, since, wait).await,
            Self::Gmail(p) => p.wait(folder, since, wait).await,
            Self::Ews(p) => p.wait(folder, since, wait).await,
            Self::Eas(p) => p.wait(folder, since, wait).await,
        }
    }
}

/// Who the account's mail is from: its sender name, unless that is only the address.
fn sender(account: &Account) -> Address {
    Address {
        name: Some(account.display_name.clone()).filter(|n| *n != account.email),
        email: account.email.clone(),
    }
}

/// A `Message-ID` (without brackets) no other message has: the time and 64 random bits.
fn message_id(email: &str, now: i64) -> String {
    let random = RandomState::new().hash_one(now);
    let domain = email
        .rsplit_once('@')
        .map(|(_, d)| d)
        .filter(|d| !d.is_empty() && d.is_ascii())
        .unwrap_or("localhost");
    format!("{now:x}.{random:016x}@{domain}")
}

/// Mail received before this, in seconds, is not kept by an account that keeps `days` days.
fn keep_since(days: u32) -> i64 {
    match days {
        0 => i64::MIN,
        days => now() / 1_000 - i64::from(days) * DAY,
    }
}

/// Milliseconds since the Unix epoch.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn tls_config(
    client: Option<Arc<dyn ResolvesClientCert>>,
    trusted: Option<[u8; 32]>,
    refused: Option<Arc<RefusedCertificates>>,
) -> Result<Arc<ClientConfig>, CoreError> {
    ready()?;
    ozero_imap::platform_tls(client, trusted, refused).map_err(|e| CoreError::Tls {
        detail: e.to_string(),
    })
}

/// The checks `tls_config` builds into its settings, for the platform's TLS to ask.
fn server_checks(
    trusted: Option<[u8; 32]>,
    refused: Option<Arc<RefusedCertificates>>,
) -> Result<Arc<dyn ServerCertVerifier>, CoreError> {
    ready()?;
    ozero_imap::platform_checks(trusted, refused).map_err(|e| CoreError::Tls {
        detail: e.to_string(),
    })
}

/// Without the JNI setup the Android verifier panics on first use; refuse cleanly instead.
fn ready() -> Result<(), CoreError> {
    #[cfg(target_os = "android")]
    if !android::READY.load(std::sync::atomic::Ordering::Acquire) {
        return Err(CoreError::Tls {
            detail: "CoreNative.init was not called".into(),
        });
    }
    Ok(())
}

#[cfg(target_os = "android")]
mod android {
    use std::sync::atomic::{AtomicBool, Ordering};

    use jni::EnvUnowned;
    use jni::errors::ThrowRuntimeExAndDefault;
    use jni::jni_mangle;
    use jni::objects::{JClass, JObject};

    pub static READY: AtomicBool = AtomicBool::new(false);

    /// `CoreNative.init(context)`: lets the certificate verifier call Android's trust manager.
    #[jni_mangle("com.ozeromail.app.CoreNative")]
    pub fn init<'caller>(
        mut env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        context: JObject<'caller>,
    ) {
        android_logger::init_once(
            android_logger::Config::default()
                .with_tag("ozero")
                .with_max_level(log::LevelFilter::Info),
        );
        env.with_env(|env| {
            rustls_platform_verifier::android::init_with_env(env, context)?;
            READY.store(true, Ordering::Release);
            Ok::<_, jni::errors::Error>(())
        })
        .resolve::<ThrowRuntimeExAndDefault>();
    }
}

/// What a folder means to the user, whatever the server calls it.
#[uniffi::remote(Enum)]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Trash,
    Junk,
    Flagged,
    Snoozed,
    Custom,
}

/// A list of mail: the folders of one role across all accounts, or one folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Mailbox {
    Unified { role: FolderRole },
    Folder { id: i64 },
}

/// Held actions as the apps pass them back to `undo`.
fn ids(ops: Vec<OpId>) -> Vec<i64> {
    ops.into_iter().map(|op| op.0).collect()
}

/// Each message with `action`.
fn with(messages: Vec<i64>, action: Action) -> Vec<(MessageId, Action)> {
    messages
        .into_iter()
        .map(|id| (MessageId(id), action))
        .collect()
}

const INBOX: Mailbox = Mailbox::Unified {
    role: FolderRole::Inbox,
};

impl Mailbox {
    fn holds(self, folder: &Folder) -> bool {
        match self {
            Self::Unified { role } => folder.role == role,
            Self::Folder { id } => folder.id.0 == id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FolderItem {
    pub id: i64,
    pub account_id: i64,
    /// As the server names it; apps name folders with a role themselves.
    pub name: String,
    pub role: FolderRole,
}

impl From<Folder> for FolderItem {
    fn from(f: Folder) -> Self {
        Self {
            id: f.id.0,
            account_id: f.account_id.0,
            name: f.name,
            role: f.role,
        }
    }
}

/// How the core talks to the account's server.
#[uniffi::remote(Enum)]
pub enum AccountKind {
    Imap,
    Gmail,
    Graph,
    Exchange,
}

/// When the phone looks for an account's new mail while the app is closed.
#[uniffi::remote(Enum)]
pub enum NewMail {
    Every { minutes: u32 },
    Manually,
}

/// A mail service sign-in knows by the domain of an address.
#[uniffi::remote(Enum)]
pub enum MailService {
    Google,
    ICloud,
    Yahoo,
    Yandex,
    MailRu,
}

/// Inbox tab, sorted by rules on the device.
#[uniffi::remote(Enum)]
pub enum Category {
    People,
    Notifications,
    Newsletters,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InboxTab {
    pub category: Category,
    pub unread: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountItem {
    pub id: i64,
    pub email: String,
    pub display_name: String,
    /// What to call the account: the user's name for it, else `email`.
    pub name: String,
    /// The user's name for it; empty when none.
    pub label: String,
    /// 0xRRGGBB.
    pub color: u32,
    /// The colour's place in `account_colors`.
    pub color_index: u32,
    /// How many days back its mail is kept on the device; 0 keeps all of it.
    pub keep_days: u32,
    /// When the phone looks for its new mail with the app closed.
    pub new_mail: NewMail,
    pub kind: AccountKind,
    /// The server refused the saved secret: mail stops until the person signs in again.
    pub needs_sign_in: bool,
    /// Its own signature; empty when it uses the default one.
    pub signature: String,
    /// What goes under its mail: its own signature, else the default one.
    pub signs_with: String,
}

impl AccountItem {
    /// `signature` is the default one, for accounts without their own.
    fn new(a: Account, signature: &str) -> Self {
        Self {
            signs_with: a.signs_with(signature).to_owned(),
            id: a.id.0,
            color: rgb(a.color),
            color_index: a.color % ACCOUNT_COLORS.len() as u32,
            name: a.name().to_owned(),
            email: a.email,
            display_name: a.display_name,
            label: a.label,
            keep_days: a.keep_days,
            new_mail: a.new_mail,
            kind: a.kind,
            needs_sign_in: a.needs_sign_in,
            signature: a.signature,
        }
    }
}

/// Who a message went to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PeopleItem {
    pub to: Vec<PersonItem>,
    /// Copies (`Cc`).
    pub cc: Vec<PersonItem>,
    /// Everyone in To and Cc once, the user first: the line under the sender.
    pub shown: Vec<PersonItem>,
    /// Who `Core::reply` with `all` copies besides those it answers: never the user.
    pub others: Vec<PersonItem>,
}

/// Someone a message went to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PersonItem {
    /// The name, or the address when there is none.
    pub name: String,
    /// What the name starts with, as said to them: "Anna" of "Anna Lee" or "Lee, Anna".
    pub first: String,
    pub email: String,
    /// The account the message is in.
    pub me: bool,
}

/// Someone who wrote in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SenderItem {
    /// The name, or the address when there is none.
    pub name: String,
    /// What the name starts with: "Anna, me, Boris".
    pub first: String,
    /// The user.
    pub me: bool,
}

impl From<Sender> for SenderItem {
    fn from(s: Sender) -> Self {
        Self {
            first: first_name(&s.name).to_owned(),
            name: s.name,
            me: s.me,
        }
    }
}

/// The account a row is in, for lists that show mail of several accounts.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountMark {
    pub name: String,
    /// 0xRRGGBB.
    pub color: u32,
}

impl From<&Account> for AccountMark {
    fn from(a: &Account) -> Self {
        Self {
            name: a.name().to_owned(),
            color: rgb(a.color),
        }
    }
}

/// The colours an account can have, as 0xRRGGBB.
#[uniffi::export]
pub fn account_colors() -> Vec<u32> {
    ACCOUNT_COLORS.to_vec()
}

/// Account colours in the order accounts get them: cut paper of one brightness, coral first.
const ACCOUNT_COLORS: [u32; 10] = [
    0xE2_60_5A, 0x1F_9C_8A, 0x7B_66_E0, 0xE3_90_2F, 0xCF_A3_2A, 0x4E_9E_5F, 0x3D_8B_E8, 0x4F_5B_CB,
    0xCC_52_94, 0x64_70_8A,
];

fn rgb(color: u32) -> u32 {
    ACCOUNT_COLORS[color as usize % ACCOUNT_COLORS.len()]
}

/// A Google sign-in under way, from `google_sign_in`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleSignIn {
    /// Opens in the browser.
    pub url: String,
    /// Kept in memory until the browser comes back; never shown or logged.
    pub verifier: String,
    pub state: String,
}

impl From<oauth::SignIn> for GoogleSignIn {
    fn from(s: oauth::SignIn) -> Self {
        Self {
            url: s.url,
            verifier: s.verifier,
            state: s.state,
        }
    }
}

impl From<GoogleSignIn> for oauth::SignIn {
    fn from(s: GoogleSignIn) -> Self {
        Self {
            url: s.url,
            verifier: s.verifier,
            state: s.state,
        }
    }
}

/// A Gmail account Google let in, not saved yet.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleAccount {
    pub email: String,
    /// The name its mail goes out under, as set in Gmail.
    pub name: Option<String>,
    /// The refresh token: the platform keeps it like a password.
    pub secret: String,
}

/// A server address as sign-in asks for it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    fn server(self, user: &str) -> Server {
        Server {
            host: self.host,
            port: self.port,
            user: user.to_owned(),
            certificate: None,
        }
    }
}

/// A message the server refused.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UnsentItem {
    pub id: i64,
    pub subject: String,
    /// The server's answer.
    pub reason: String,
}

/// A message being written, as compose shows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DraftItem {
    /// The saved draft; 0 until it is saved.
    pub id: i64,
    /// 0 for none picked yet.
    pub account_id: i64,
    /// Addresses joined by ", ".
    pub to: String,
    pub subject: String,
    pub text: String,
    pub reply_to: Option<i64>,
    /// Goes under the text; empty for none.
    pub quote: String,
    /// When it was last saved, in seconds since the Unix epoch; 0 if never.
    pub saved: i64,
    /// The forwarded message, whose files go with it.
    #[uniffi(default = None)]
    pub forward: Option<i64>,
    /// The files added to it, which `Core::attach_file` and `Core::detach_file` change; saving
    /// or sending the draft leaves them be.
    #[uniffi(default = [])]
    pub files: Vec<DraftFileItem>,
    /// Goes between the text and the quote; empty for none. A message started by the core
    /// has its account's `signs_with`.
    #[uniffi(default = "")]
    pub signature: String,
    /// Copies (`Cc`), as `to`; empty for none.
    #[uniffi(default = "")]
    pub cc: String,
}

/// A file of saved mail, offered to send on.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RecentFileItem {
    /// The message it came with.
    pub message: i64,
    pub account_id: i64,
    /// Its part in the message, as `AttachmentItem::part`.
    pub part: u32,
    pub name: String,
    /// The lowercase content type, such as `application/pdf`.
    pub mime: String,
    /// Bytes.
    pub size: u64,
    /// Who sent it: the name, or the address without one.
    pub sender: String,
}

impl From<RecentFile> for RecentFileItem {
    fn from(f: RecentFile) -> Self {
        let sender = shown_name(f.from_name.as_deref()).map_or(f.from_email, str::to_owned);
        Self {
            message: f.message.0,
            account_id: f.account_id.0,
            part: f.file.part,
            name: f.file.name,
            mime: f.file.mime,
            size: f.file.size,
            sender,
        }
    }
}

/// "Anna" of "Anna Lee", "Lee, Anna" or "anna@x.com"; the name itself when that leaves nothing.
fn first_name(name: &str) -> &str {
    let given = name.split('@').next().unwrap_or(name);
    let given = given
        .split_once(", ")
        .map_or(given, |(_, after)| after)
        .trim();
    match given.split(' ').next() {
        Some(first) if !first.is_empty() => first,
        _ => name,
    }
}

/// A sender's name as shown, none when it is blank.
fn shown_name(name: Option<&str>) -> Option<&str> {
    name.map(str::trim).filter(|n| !n.is_empty())
}

/// A file added to a message being written.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DraftFileItem {
    /// What `Core::detach_file` asks for.
    pub id: i64,
    pub name: String,
    /// The content type, such as `application/pdf`.
    pub mime: String,
    /// Bytes.
    pub size: u64,
}

impl From<DraftFile> for DraftFileItem {
    fn from(f: DraftFile) -> Self {
        Self {
            id: f.id.0,
            name: f.name,
            mime: f.mime,
            size: f.size,
        }
    }
}

/// The header words of a forwarded message, in the app's language.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ForwardLabels {
    /// The line over it all.
    pub title: String,
    pub from: String,
    pub date: String,
    pub subject: String,
}

impl From<SavedDraft> for DraftItem {
    fn from(d: SavedDraft) -> Self {
        Self {
            id: d.id.map_or(0, |id| id.0),
            account_id: d.account_id.map_or(0, |a| a.0),
            to: d.to,
            subject: d.subject,
            text: d.text,
            reply_to: d.reply_to.map(|m| m.0),
            quote: d.quote,
            saved: d.saved / 1_000,
            forward: d.forward.map(|m| m.0),
            files: d.files.into_iter().map(DraftFileItem::from).collect(),
            signature: d.signature,
            cc: d.cc,
        }
    }
}

/// A saved draft's id; none for 0.
fn draft_id(id: i64) -> Option<DraftId> {
    (id != 0).then_some(DraftId(id))
}

/// What the reading screen shows.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MessageBody {
    /// The text without the quoted mail at its end.
    pub text: String,
    /// That quote, for the reader to fold; empty for none.
    pub quote: String,
    /// A whole HTML document for a web view with JavaScript and network off, without its
    /// remote images; `message_images` loads them.
    pub html: Option<String>,
    /// The page sets its own colours, so it stays light in a dark reader.
    pub styled: bool,
    /// How many remote images the page holds back.
    pub remote_images: u32,
    /// The remote images load without asking: the mail is from a person, or the user chose
    /// to see this sender's.
    pub shows_images: bool,
    /// The files sent with it; `Core::save_attachments` downloads them.
    pub attachments: Vec<AttachmentItem>,
}

/// A file sent with a message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AttachmentItem {
    /// What `Core::save_attachments` asks for.
    pub part: u32,
    /// The file name as sent; empty when there is none.
    pub name: String,
    /// The lowercase content type, such as `application/pdf`.
    pub mime: String,
    /// Bytes.
    pub size: u64,
}

/// Where `Core::save_attachments` writes an attachment.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AttachmentFile {
    /// `AttachmentItem::part`.
    pub part: u32,
    /// A file path the app may write.
    pub path: String,
}

impl From<Attachment> for AttachmentItem {
    fn from(a: Attachment) -> Self {
        Self {
            part: a.part,
            name: a.name,
            mime: a.mime,
            size: a.size,
        }
    }
}

impl From<Body> for MessageBody {
    fn from(body: Body) -> Self {
        let page = body.html.as_deref().map(html::clean);
        let (text, quote) = body.split_quote();
        let styled = page.as_ref().is_some_and(|p| html::is_styled(&p.html));
        Self {
            remote_images: page
                .as_ref()
                .map_or(0, |p| u32::try_from(p.images.len()).unwrap_or(u32::MAX)),
            styled,
            html: page.map(|p| sandboxed(&html::fold_quote(&p.html), styled)),
            shows_images: false,
            text: text.to_owned(),
            quote: quote.to_owned(),
            attachments: body
                .attachments
                .into_iter()
                .map(AttachmentItem::from)
                .collect(),
        }
    }
}

/// A page with its remote images, which it asks for by address.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ImagesPage {
    /// As `MessageBody::html`, with each loaded image's address in its place.
    pub html: String,
    /// By the last part of an image's address; None for an image that did not load.
    pub images: Vec<Option<RemoteImage>>,
}

/// One downloaded image.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RemoteImage {
    /// A plain `image/...` type.
    pub mime: String,
    pub data: Vec<u8>,
}

impl RemoteImage {
    /// None for a type that is not an image.
    fn new(content_type: &str, data: Vec<u8>) -> Option<Self> {
        let mime = content_type.split(';').next()?.trim().to_ascii_lowercase();
        let plain = mime
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '+' | '-'));
        (mime.starts_with("image/") && plain).then_some(Self { mime, data })
    }
}

/// Wraps a message's HTML so the page can load nothing from outside, even if the
/// web view's own blocking fails: no scripts, frames, forms or remote images. A `styled`
/// page stays light; a plain one follows the reader's light or dark theme.
fn sandboxed(html: &str, styled: bool) -> String {
    const HEAD: &str = concat!(
        "<!doctype html><html><head><meta charset=\"utf-8\">",
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; ",
        "img-src data: https://",
        image_host!(),
        "; style-src 'unsafe-inline'; font-src data: https://",
        font_host!(),
        "; ",
        "base-uri 'none'; form-action 'none'\">",
        "<meta http-equiv=\"x-dns-prefetch-control\" content=\"off\">",
        // No initial scale: a page wider than the screen opens zoomed out to fit it, but not
        // below a scale where its text can still be read; the rest scrolls sideways. A page up to
        // about 800 px wide still fits a phone.
        "<meta name=\"viewport\" content=\"width=device-width,minimum-scale=0.48\">",
        "<style>body{margin:0;padding:0 16px 16px;overflow-wrap:break-word}",
        "img{max-width:100%;height:auto}",
        "details.ozero-quote>summary{list-style:none;display:inline-block;margin:10px 0;",
        "padding:0 10px;border-radius:9px;background:#e9eef5;color:#6c7890;",
        "font:700 13px/22px sans-serif;letter-spacing:1px}",
        "details.ozero-quote>summary::-webkit-details-marker{display:none}",
    );
    const LIGHT: &str = "html{color-scheme:light;background:#fff}";
    // The app's own colours and reading type, kept in step with the Android Theme.kt and Type.kt
    // (Literata at its default optical size; where the app does not serve it, the system serif).
    // The web view is clear, so the screen shows through.
    const THEMED: &str = concat!(
        "@font-face{font-family:Literata;font-weight:200 900;src:url(https://",
        font_host!(),
        "/literata.ttf)}",
        "html{color-scheme:light dark;background:transparent;color:#142036}",
        "body{padding-inline:20px;font:17.5px/28px Literata,ui-serif,Georgia,serif;",
        "font-optical-sizing:none;text-wrap:pretty}",
        "p,ul,ol,blockquote,pre,table{margin:0 0 14px}",
        "h1,h2,h3{font-weight:650;line-height:1.3;margin:22px 0 10px}",
        "a{color:#1a6fe0;text-decoration-thickness:1px;text-underline-offset:3px}",
        "blockquote{padding-left:14px;border-left:2px solid #e3e9f2;color:#76829a}",
        "li::marker{color:#76829a}",
        "@media(prefers-color-scheme:dark){html{color:#e5e9f1}a{color:#6ab2ff}",
        "blockquote{border-color:#2b3343;color:#8792a7}",
        "details.ozero-quote>summary{background:#1f2532;color:#8792a7}}",
    );
    const BODY: &str = "</style></head><body dir=\"auto\">";
    let theme = if styled { LIGHT } else { THEMED };
    [HEAD, theme, BODY, html, "</body></html>"].concat()
}

/// One row of a message list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MessageItem {
    pub id: i64,
    pub account_id: i64,
    pub from_name: Option<String>,
    pub from_email: String,
    pub subject: String,
    pub snippet: String,
    /// Seconds since the Unix epoch.
    pub date: i64,
    pub seen: bool,
    pub flagged: bool,
    /// Messages in its conversation, this one included.
    pub thread_count: u32,
    /// How many of them are unread.
    pub thread_unread: u32,
    /// Who wrote in the conversation, the user included, oldest first, each once.
    pub senders: Vec<SenderItem>,
    /// The user wrote it: it is in the sent folder.
    pub sent: bool,
    /// Done can archive it: its folder holds mail not filed yet.
    pub can_archive: bool,
    /// It can go to the trash: it is not there already; delete there is for good.
    pub can_trash: bool,
    /// It can go back to the inbox: it is in the archive, trash, spam or snoozed folder.
    pub can_move_to_inbox: bool,
    /// It can be snoozed: its folder holds mail not filed yet.
    pub can_snooze: bool,
    /// In mixed lists (the unified inbox, search), set when there is more than one account.
    pub account: Option<AccountMark>,
    /// The names of its attachments, once its body is downloaded; a name may be empty.
    pub attachments: Vec<String>,
    /// The user sent it and it waits to go: nothing can be done to it until it has.
    pub sending: bool,
}

impl From<MessageRow> for MessageItem {
    fn from(row: MessageRow) -> Self {
        let allows = |action| !row.sending && row.role.allows(action);
        Self {
            id: row.id.0,
            account_id: row.account_id.0,
            from_name: row.from.name,
            from_email: row.from.email,
            subject: row.subject,
            snippet: row.snippet,
            date: row.date,
            seen: row.flags.seen,
            flagged: row.flags.flagged,
            thread_count: row.thread_count,
            thread_unread: row.thread_unread,
            senders: row.senders.into_iter().map(SenderItem::from).collect(),
            sent: row.role == FolderRole::Sent,
            can_archive: allows(Action::Archive),
            can_trash: allows(Action::Trash),
            can_move_to_inbox: allows(Action::Inbox),
            can_snooze: allows(Action::Snooze),
            account: None,
            attachments: row.attachments,
            sending: row.sending,
        }
    }
}

/// Apps map each variant to a string key; `detail` and `Display` are for logs and never hold mail contents.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
pub enum CoreError {
    /// The database key is wrong or not 32 bytes.
    WrongKey,
    /// The database was written by a newer app version.
    TooNew,
    /// An account with this email is already added.
    AccountExists,
    /// Signing in again chose another account than the one asked for.
    WrongAccount,
    /// The server refused the login.
    Auth,
    Network {
        detail: String,
    },
    /// Secure connections cannot be set up on this device.
    Tls {
        detail: String,
    },
    Server {
        detail: String,
    },
    /// The message is no longer on the server; a sync will drop it.
    Gone,
    /// The account has no folder marked as its archive.
    NoArchive,
    /// The account has no folder marked as its trash.
    NoTrash,
    /// The account has no snoozed folder yet; its next sync makes one.
    NoSnoozed,
    /// A recipient is not an email address, or there is none.
    BadRecipients,
    /// A copy (`Cc`) is not an email address.
    BadCopies,
    /// The account has no SMTP server to send through.
    NoSmtp,
    /// No Exchange server was found for the address, or the one typed is no server address.
    NoServer,
    /// Neither IMAP nor Exchange answers for an address, so its servers have to be typed.
    NotFound,
    /// The Exchange server typed answers, but not as EWS.
    NoEws,
    /// The Exchange server typed asks for a client certificate, and none was picked.
    CertificateNeeded,
    /// The server's certificate did not pass the checks; the person may choose to trust it,
    /// and sign in again with its `fingerprint`.
    UntrustedCertificate {
        host: String,
        fingerprint: String,
        reason: String,
    },
    /// The company's Exchange server has not let this device in over ActiveSync, or blocked it.
    DeviceBlocked,
    /// The files added to a message would take more than `file_limit`.
    TooLarge,
    Storage {
        detail: String,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongKey => write!(f, "the database key is wrong"),
            Self::TooNew => write!(f, "the database is from a newer app version"),
            Self::AccountExists => write!(f, "the account is already added"),
            Self::WrongAccount => write!(f, "another account signed in"),
            Self::Auth => write!(f, "the server refused the login"),
            Self::Network { detail } => write!(f, "network error: {detail}"),
            Self::Tls { detail } => write!(f, "TLS setup failed: {detail}"),
            Self::Server { detail } => write!(f, "server error: {detail}"),
            Self::Gone => write!(f, "the message is gone from the server"),
            Self::NoArchive => write!(f, "the account has no archive folder"),
            Self::NoTrash => write!(f, "the account has no trash folder"),
            Self::NoSnoozed => write!(f, "the account has no snoozed folder"),
            Self::BadRecipients => write!(f, "a recipient is not an email address"),
            Self::BadCopies => write!(f, "a copy is not an email address"),
            Self::NoSmtp => write!(f, "the account has no SMTP server"),
            Self::NoServer => write!(f, "no Exchange server found"),
            Self::NotFound => write!(f, "no server found for the address"),
            Self::NoEws => write!(f, "the server does not answer as EWS"),
            Self::CertificateNeeded => write!(f, "the server asks for a client certificate"),
            Self::UntrustedCertificate { host, reason, .. } => {
                write!(
                    f,
                    "the certificate of {host} did not pass the checks: {reason}"
                )
            }
            Self::DeviceBlocked => write!(f, "the server has not let this device in"),
            Self::TooLarge => write!(f, "the files are too large for one message"),
            Self::Storage { detail } => write!(f, "storage error: {detail}"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<StoreError> for CoreError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::WrongKey => Self::WrongKey,
            StoreError::TooNew { .. } => Self::TooNew,
            StoreError::NoArchive => Self::NoArchive,
            StoreError::NoTrash => Self::NoTrash,
            StoreError::NoSnoozed => Self::NoSnoozed,
            other => Self::Storage {
                detail: other.to_string(),
            },
        }
    }
}

impl From<ProviderError> for CoreError {
    fn from(e: ProviderError) -> Self {
        match e {
            ProviderError::Auth => Self::Auth,
            ProviderError::Blocked => Self::DeviceBlocked,
            ProviderError::Network(detail) => Self::Network { detail },
            ProviderError::Protocol(detail) | ProviderError::Rejected(detail) => {
                Self::Server { detail }
            }
            ProviderError::Gone => Self::Gone,
        }
    }
}

impl From<SyncError> for CoreError {
    fn from(e: SyncError) -> Self {
        match e {
            SyncError::Provider(e) => e.into(),
            SyncError::Store(e) => e.into(),
            SyncError::NoInbox => Self::Server {
                detail: e.to_string(),
            },
        }
    }
}

mod certificate;

pub use certificate::{ClientCertificates, Signing};

mod tunnel;

pub use tunnel::{PlatformSocket, PlatformTls, PlatformTlsError, ServerChecks};

#[cfg(test)]
mod tests;
