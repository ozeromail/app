//! Facade the Android and iOS apps call through UniFFI.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasher, RandomState};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ozero_app::ops::push_ops;
use ozero_app::send::{SentCopy, push_sends};
use ozero_app::sync::{SyncError, download_body, sync_inbox};
use ozero_domain::{
    Account, AccountId, AccountKind, Action, Address, Body, Category, Draft, FolderRole, ListQuery,
    ListScope, MessageId, MessageRow, NewAccount, OpId, OutboxId, ProviderError, Server, Store,
    StoreError, parse_recipients, saves_sent_itself,
};
use ozero_imap::ImapSession;
use ozero_imap::rustls::ClientConfig;
use ozero_mime::{MimeBuilder, MimeParser};
use ozero_smtp::SmtpSession;
use ozero_store_sqlite::SqliteStore;

uniffi::setup_scaffolding!();

/// How many of the newest inbox messages one sync brings in.
const SYNC_LIMIT: u32 = 200;
/// Milliseconds an archived or sent message waits before the server hears of it.
const HOLD: i64 = 5_000;

/// How long the apps offer undo after archiving or sending: a second shorter than the hold,
/// so a tap is never late.
#[uniffi::export]
pub fn undo_seconds() -> u32 {
    u32::try_from(HOLD / 1_000 - 1).unwrap_or(0)
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

/// The SMTP server sign-in suggests, like `suggested_imap_host`.
#[uniffi::export]
pub fn suggested_smtp_host(email: String) -> Option<String> {
    ozero_domain::guess_smtp_host(&email)
}

/// An answer to `message`: from the account that got it, to its sender, `Re: ` once.
#[uniffi::export]
pub fn reply_draft(message: MessageItem) -> DraftItem {
    DraftItem {
        account_id: message.account_id,
        to: message.from_email,
        subject: ozero_domain::reply_subject(&message.subject),
        text: String::new(),
        reply_to: Some(message.id),
    }
}

/// The mail core of one app install: the encrypted database and the network.
#[derive(uniffi::Object)]
pub struct Core {
    store: SqliteStore,
    parser: MimeParser,
    tls: OnceLock<Arc<ClientConfig>>,
    /// One push or sync per account at a time, so a sync never saves flags from before a push.
    busy: Mutex<HashMap<AccountId, Arc<tokio::sync::Mutex<()>>>>,
    /// Accounts with a push waiting for held actions to come due.
    waiting: Mutex<HashSet<AccountId>>,
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
        Ok(Arc::new(Self {
            store: SqliteStore::open(Path::new(&path), &key)?,
            parser: MimeParser::default(),
            tls: OnceLock::new(),
            busy: Mutex::default(),
            waiting: Mutex::default(),
        }))
    }

    pub fn accounts(&self) -> Result<Vec<AccountItem>, CoreError> {
        Ok(self
            .store
            .accounts()?
            .into_iter()
            .map(AccountItem::from)
            .collect())
    }

    /// The newest messages of one tab across all inboxes, newest first.
    pub fn unified_inbox(&self, tab: Category, limit: u32) -> Result<Vec<MessageItem>, CoreError> {
        let rows = self.store.list(&ListQuery {
            scope: ListScope::Inbox(tab),
            limit,
            after: None,
        })?;
        Ok(rows.into_iter().map(MessageItem::from).collect())
    }

    /// Unread mail in People that arrived since the last call, newest first: what a
    /// notification shows. The app calls it in the foreground too, so mail seen there is not
    /// announced later.
    pub fn take_new_mail(&self) -> Result<Vec<MessageItem>, CoreError> {
        Ok(self
            .store
            .take_fresh()?
            .into_iter()
            .map(MessageItem::from)
            .collect())
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

    /// Marks a message read or unread on the device at once; `push_actions` tells the server.
    pub fn mark_read(&self, message: i64, seen: bool) -> Result<(), CoreError> {
        self.store
            .queue(MessageId(message), Action::Seen(seen), now())?;
        Ok(())
    }

    /// Takes the message out of the lists at once and holds the move to the archive for
    /// `HOLD`, so `undo` can take it back. Returns the action for `undo`.
    /// Fails with `NoArchive` if the account has no archive folder.
    pub fn archive(&self, message: i64) -> Result<i64, CoreError> {
        let op = self
            .store
            .queue(MessageId(message), Action::Archive, now() + HOLD)?;
        Ok(op.0)
    }

    /// Takes back a held action. False once it may be on its way to the server.
    pub fn undo(&self, action: i64) -> Result<bool, CoreError> {
        Ok(self.store.undo(OpId(action), now())?)
    }

    /// Puts a message in the outbox, held for `HOLD` so `undo_send` can take it back;
    /// `push_actions` sends it. `to` is addresses split by commas or spaces; `reply_to` the
    /// message it answers. Returns the id for `undo_send`.
    pub fn send(
        &self,
        account: i64,
        to: String,
        subject: String,
        text: String,
        reply_to: Option<i64>,
    ) -> Result<i64, CoreError> {
        let to = parse_recipients(&to).ok_or(CoreError::BadRecipients)?;
        let from = self.account(AccountId(account))?;
        // Would wait in the outbox forever.
        if from.smtp.is_none() {
            return Err(CoreError::NoSmtp);
        }
        let draft = Draft {
            account_id: from.id,
            to,
            subject,
            text,
            reply_to: reply_to.map(MessageId),
        };
        let now = now();
        let id = self.store.queue_send(
            &draft,
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

    /// Takes back a held or refused message to edit it again. None once it may be on its way.
    pub fn undo_send(&self, id: i64) -> Result<Option<DraftItem>, CoreError> {
        Ok(self
            .store
            .undo_send(OutboxId(id), now())?
            .map(DraftItem::from))
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Core {
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
        let known = self.store.accounts()?;
        if known.iter().any(|a| a.email.eq_ignore_ascii_case(&email)) {
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

    /// Sends the account's queued actions, waiting for held ones to come due; without any
    /// it does not connect. After a failure they stay queued for the next push or sync.
    pub async fn push_actions(&self, account: i64, password: String) -> Result<(), CoreError> {
        let account = self.account(AccountId(account))?;
        let mut waiter = None;
        loop {
            let wait = {
                let lock = self.busy(account.id);
                let _busy = lock.lock().await;
                // Read under the lock: a push that waited finds what the one before it sent gone.
                let step = async {
                    let mut imap = None;
                    self.push_due(&account, &password, &mut imap).await?;
                    if let Some(imap) = imap {
                        let _ = imap.logout().await;
                    }
                    Ok::<_, CoreError>(self.store.next_send(account.id)?)
                }
                .await;
                // The waiter is taken and given up under the lock, so a push that finds it
                // taken can leave its held action to the waiting push.
                let wait = match step {
                    Ok(next) => next.map_or(0, |at| at - now()),
                    Err(e) => {
                        drop(waiter.take());
                        return Err(e);
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

    /// Sends the account's queued actions and mail, then brings its folders and newest inbox
    /// messages into the database.
    pub async fn sync_imap(&self, account: i64, password: String) -> Result<(), CoreError> {
        let account = self.account(AccountId(account))?;
        let lock = self.busy(account.id);
        let _busy = lock.lock().await;
        let mut slot = None;
        // A send that fails, say on a broken SMTP server, must not stop new mail coming in.
        let pushed = self.push_due(&account, &password, &mut slot).await;
        let imap = self.imap(&mut slot, &account, &password).await?;
        sync_inbox(&self.store, imap, &self.parser, account.id, SYNC_LIMIT).await?;
        if let Some(imap) = slot {
            let _ = imap.logout().await;
        }
        pushed
    }

    /// The message body: the saved one, or downloaded once and saved with its snippet.
    /// `password` is the message's account's.
    pub async fn message_body(
        &self,
        message: i64,
        password: String,
    ) -> Result<MessageBody, CoreError> {
        let message = MessageId(message);
        if let Some(body) = self.store.body(message)? {
            return Ok(body.into());
        }
        let location = self.store.location(message)?;
        let account = self.account(location.account_id)?;
        let mut imap = self.connect(&account, &password).await?;
        let body = download_body(&self.store, &mut imap, &self.parser, &location).await?;
        // Closing without LOGOUT saves the reader a round trip; the server cleans up.
        drop(imap);
        Ok(body.into())
    }
}

impl Core {
    fn busy(&self, account: AccountId) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.busy.lock().unwrap_or_else(|p| p.into_inner());
        locks.entry(account).or_default().clone()
    }

    fn account(&self, id: AccountId) -> Result<Account, CoreError> {
        self.store
            .accounts()?
            .into_iter()
            .find(|a| a.id == id && a.kind == AccountKind::Imap)
            .ok_or_else(|| CoreError::Storage {
                detail: "no such IMAP account".into(),
            })
    }

    /// Logs in to a saved IMAP account.
    async fn connect(&self, account: &Account, password: &str) -> Result<ImapSession, CoreError> {
        let server = account.server.as_ref().ok_or_else(|| CoreError::Storage {
            detail: "no IMAP server".into(),
        })?;
        self.login(server, password).await
    }

    /// The IMAP session in `slot`, logged in on first use.
    async fn imap<'a>(
        &self,
        slot: &'a mut Option<ImapSession>,
        account: &Account,
        password: &str,
    ) -> Result<&'a mut ImapSession, CoreError> {
        match slot {
            Some(imap) => Ok(imap),
            None => Ok(slot.insert(self.connect(account, password).await?)),
        }
    }

    /// Sends what is due: actions over IMAP, then mail over SMTP with a copy into Sent.
    /// Connects to IMAP only when it is needed and leaves the session in `slot`.
    async fn push_due(
        &self,
        account: &Account,
        password: &str,
        slot: &mut Option<ImapSession>,
    ) -> Result<(), CoreError> {
        let ops = self.store.due(account.id, now())?;
        if !ops.is_empty() {
            let imap = self.imap(slot, account, password).await?;
            push_ops(&self.store, imap, ops).await?;
        }
        let mail = self.store.due_sends(account.id, now())?;
        if mail.is_empty() {
            return Ok(());
        }
        let server = account.smtp.as_ref().ok_or(CoreError::NoSmtp)?;
        let mut smtp = self.send_login(server, password).await?;
        let sent = if saves_sent_itself(&server.host) {
            None
        } else {
            let folders = self.store.folders(account.id)?;
            folders.into_iter().find(|f| f.role == FolderRole::Sent)
        };
        // Without IMAP the mail still goes; only its copy in Sent is missing.
        let copy = match &sent {
            Some(folder) => self
                .imap(slot, account, password)
                .await
                .ok()
                .map(|provider| SentCopy {
                    provider,
                    folder: &folder.remote_id,
                }),
            None => None,
        };
        let from = Address {
            name: Some(account.display_name.clone()).filter(|n| *n != account.email),
            email: account.email.clone(),
        };
        push_sends(&self.store, &mut smtp, &MimeBuilder, &from, copy, mail).await?;
        let _ = smtp.quit().await;
        Ok(())
    }

    async fn send_login(&self, server: &Server, password: &str) -> Result<SmtpSession, CoreError> {
        Ok(SmtpSession::connect(
            &server.host,
            server.port,
            self.tls()?,
            &server.user,
            password,
        )
        .await?)
    }

    async fn login(&self, server: &Server, password: &str) -> Result<ImapSession, CoreError> {
        Ok(ImapSession::connect(
            &server.host,
            server.port,
            self.tls()?,
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
        let tls = tls_config()?;
        Ok(self.tls.get_or_init(|| tls).clone())
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

/// Milliseconds since the Unix epoch.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn tls_config() -> Result<Arc<ClientConfig>, CoreError> {
    // Without the JNI setup the Android verifier panics on first use; refuse cleanly instead.
    #[cfg(target_os = "android")]
    if !android::READY.load(std::sync::atomic::Ordering::Acquire) {
        return Err(CoreError::Tls {
            detail: "CoreNative.init was not called".into(),
        });
    }
    ozero_imap::platform_tls().map_err(|e| CoreError::Tls {
        detail: e.to_string(),
    })
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
        env.with_env(|env| {
            rustls_platform_verifier::android::init_with_env(env, context)?;
            READY.store(true, Ordering::Release);
            Ok::<_, jni::errors::Error>(())
        })
        .resolve::<ThrowRuntimeExAndDefault>();
    }
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
}

impl From<Account> for AccountItem {
    fn from(a: Account) -> Self {
        Self {
            id: a.id.0,
            email: a.email,
            display_name: a.display_name,
        }
    }
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
    pub account_id: i64,
    /// Addresses joined by ", ".
    pub to: String,
    pub subject: String,
    pub text: String,
    pub reply_to: Option<i64>,
}

impl From<Draft> for DraftItem {
    fn from(d: Draft) -> Self {
        Self {
            account_id: d.account_id.0,
            to: d.to.join(", "),
            subject: d.subject,
            text: d.text,
            reply_to: d.reply_to.map(|m| m.0),
        }
    }
}

/// What the reading screen shows.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MessageBody {
    pub text: String,
    /// A whole HTML document for a web view with JavaScript and network off.
    pub html: Option<String>,
}

impl From<Body> for MessageBody {
    fn from(body: Body) -> Self {
        Self {
            html: body.html.as_deref().map(sandboxed),
            text: body.text,
        }
    }
}

/// Wraps a message's HTML so the page can load nothing from outside, even if the
/// web view's own blocking fails: no scripts, frames, forms or remote images.
fn sandboxed(html: &str) -> String {
    const HEAD: &str = concat!(
        "<!doctype html><html><head><meta charset=\"utf-8\">",
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; ",
        "img-src data:; style-src 'unsafe-inline'; font-src data:; ",
        "base-uri 'none'; form-action 'none'\">",
        "<meta http-equiv=\"x-dns-prefetch-control\" content=\"off\">",
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
        "<style>html{color-scheme:light;background:#fff}",
        "body{margin:0;padding:0 16px 16px;overflow-wrap:break-word}",
        "img{max-width:100%;height:auto}</style>",
        "</head><body dir=\"auto\">",
    );
    [HEAD, html, "</body></html>"].concat()
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
}

impl From<MessageRow> for MessageItem {
    fn from(row: MessageRow) -> Self {
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
    /// A recipient is not an email address, or there is none.
    BadRecipients,
    /// The account has no SMTP server to send through.
    NoSmtp,
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
            Self::Auth => write!(f, "the server refused the login"),
            Self::Network { detail } => write!(f, "network error: {detail}"),
            Self::Tls { detail } => write!(f, "TLS setup failed: {detail}"),
            Self::Server { detail } => write!(f, "server error: {detail}"),
            Self::Gone => write!(f, "the message is gone from the server"),
            Self::NoArchive => write!(f, "the account has no archive folder"),
            Self::BadRecipients => write!(f, "a recipient is not an email address"),
            Self::NoSmtp => write!(f, "the account has no SMTP server"),
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

#[cfg(test)]
mod tests;
