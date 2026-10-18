//! Facade the Android and iOS apps call through UniFFI.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasher, RandomState};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ozero_app::ops::push_ops;
use ozero_app::send::{SentCopy, push_sends};
use ozero_app::sync::{SyncError, download_body, sync_mail};
use ozero_domain::{
    Account, AccountId, AccountKind, Action, Address, Body, Category, Changes, Checkpoint, Draft,
    FolderRole, ListQuery, ListScope, MailProvider, MessageId, MessageRow, NewAccount, NewFolder,
    OpId, OutboxId, ProviderError, Sender, Server, Store, StoreError, html, parse_recipients,
    saves_sent_itself,
};
use ozero_gmail::{GmailSession, oauth};
use ozero_imap::ImapSession;
use ozero_imap::rustls::ClientConfig;
use ozero_mime::{MimeBuilder, MimeParser};
use ozero_smtp::SmtpSession;
use ozero_store_sqlite::SqliteStore;

uniffi::setup_scaffolding!();

/// How many new messages of the inbox and of the sent folder one sync brings in.
const SYNC_LIMIT: u32 = 200;
/// Mail older than this many seconds is not announced: a day.
const NEWS_AGE: i64 = 86_400;
/// Milliseconds an archived or sent message waits before the server hears of it.
const HOLD: i64 = 5_000;
/// The app's Google OAuth client (Android type, so it has no secret).
const GOOGLE_CLIENT_ID: &str =
    "212695996222-1jq9p8or7u441h6spset2o15ghblbt1l.apps.googleusercontent.com";
/// Where Google sends the browser back: the app's package name as a URI scheme.
const GOOGLE_REDIRECT: &str = "com.ozeromail.app:/oauth2redirect";
/// An access token is renewed this long before Google says it expires.
const TOKEN_MARGIN: Duration = Duration::from_secs(60);

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

/// Starts Google sign-in: the app opens `url` in the browser and keeps the rest for
/// `google_account` when the browser comes back to `GOOGLE_REDIRECT`.
#[uniffi::export]
pub fn google_sign_in() -> Result<GoogleSignIn, CoreError> {
    Ok(oauth::sign_in(GOOGLE_CLIENT_ID, GOOGLE_REDIRECT)?.into())
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
    /// Gmail access tokens and until when they work, by the wall clock: a monotonic one stops
    /// while the phone sleeps, and the token would look fresh for too long.
    tokens: Mutex<HashMap<AccountId, (String, SystemTime)>>,
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
            tokens: Mutex::default(),
        }))
    }

    /// The accounts in the order they were added.
    pub fn accounts(&self) -> Result<Vec<AccountItem>, CoreError> {
        Ok(self
            .store
            .accounts()?
            .into_iter()
            .map(AccountItem::from)
            .collect())
    }

    /// Sets the account's colour to one of `account_colors`, by its place there.
    pub fn set_account_color(&self, account: i64, color: u32) -> Result<(), CoreError> {
        let color = color % ACCOUNT_COLORS.len() as u32;
        Ok(self.store.set_account_color(AccountId(account), color)?)
    }

    /// Removes the account and all its mail from the device, once its sync is done; a sync or
    /// push that waited then finds nothing to do. The app forgets its secret.
    pub async fn remove_account(&self, account: i64) -> Result<(), CoreError> {
        let account = AccountId(account);
        let lock = self.busy(account);
        let _busy = lock.lock().await;
        self.store.remove_account(account)?;
        self.tokens
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&account);
        self.busy
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&account);
        Ok(())
    }

    /// Saves the Gmail account `google_account` returned, once the app has kept its secret.
    pub fn add_gmail_account(&self, email: String) -> Result<i64, CoreError> {
        if self.exists(&email)? {
            return Err(CoreError::AccountExists);
        }
        let account = NewAccount {
            kind: AccountKind::Gmail,
            display_name: email.clone(),
            email,
            server: None,
            smtp: None,
        };
        Ok(self.store.add_account(&account)?.0)
    }

    /// The newest conversations of one tab across all inboxes, newest first: each row is the
    /// newest message of its conversation.
    pub fn unified_inbox(&self, tab: Category, limit: u32) -> Result<Vec<MessageItem>, CoreError> {
        let rows = self.store.list(&ListQuery {
            scope: ListScope::Inbox(tab),
            limit,
            after: None,
        })?;
        self.items(rows)
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

    /// Messages matching every word of `text`, newest first.
    pub fn search(&self, text: String, limit: u32) -> Result<Vec<MessageItem>, CoreError> {
        let rows = self.store.search(&text, limit)?;
        self.items(rows)
    }

    /// The conversation of a message, oldest first.
    pub fn thread(&self, message: i64) -> Result<Vec<MessageItem>, CoreError> {
        let rows = self.store.thread(MessageId(message))?;
        Ok(rows.into_iter().map(MessageItem::from).collect())
    }

    /// Marks a message's conversation read, or the message alone unread, on the device at once;
    /// `push_actions` tells the server.
    pub fn mark_read(&self, message: i64, seen: bool) -> Result<(), CoreError> {
        self.store
            .queue(MessageId(message), Action::Seen(seen), now())?;
        Ok(())
    }

    /// Takes a message's conversation out of the lists at once and holds the move to the
    /// archive for `HOLD`, so `undo` can take it back. Returns the actions for `undo`.
    /// Fails with `NoArchive` if the account has no archive folder.
    pub fn archive(&self, message: i64) -> Result<Vec<i64>, CoreError> {
        self.hold_move(message, Action::Archive)
    }

    /// As `archive`, into the trash. Fails with `NoTrash` if the account has no trash folder.
    pub fn trash(&self, message: i64) -> Result<Vec<i64>, CoreError> {
        self.hold_move(message, Action::Trash)
    }

    /// An answer to `message` from the account that got it, `Re: ` once: to its sender, or
    /// with `all` to everyone it went to as well.
    pub fn reply(&self, message: MessageItem, all: bool) -> Result<DraftItem, CoreError> {
        let account = self.account(AccountId(message.account_id))?;
        let correspondents = self.store.correspondents(MessageId(message.id))?;
        Ok(DraftItem {
            account_id: message.account_id,
            to: correspondents.answer(&account.email, all).join(", "),
            subject: ozero_domain::reply_subject(&message.subject),
            text: String::new(),
            reply_to: Some(message.id),
            quote: String::new(),
        })
    }

    /// The saved text of `message` under `header`, each line marked with `>`; empty while
    /// its body is not saved.
    pub fn quote(&self, message: i64, header: String) -> Result<String, CoreError> {
        let Some(body) = self.store.body(MessageId(message))? else {
            return Ok(String::new());
        };
        Ok(ozero_domain::quote(header, &body.text))
    }

    /// Takes back held actions. False if any may be on its way to the server.
    pub fn undo(&self, actions: Vec<i64>) -> Result<bool, CoreError> {
        let ops: Vec<_> = actions.into_iter().map(OpId).collect();
        Ok(self.store.undo(&ops, now())?)
    }

    /// Puts a message in the outbox, held for `HOLD` so `undo_send` can take it back;
    /// `push_actions` sends it. `to` is addresses split by commas or spaces; `reply_to` the
    /// message it answers, and `quote` what goes under the text. Returns the id for
    /// `undo_send`.
    pub fn send(
        &self,
        account: i64,
        to: String,
        subject: String,
        text: String,
        reply_to: Option<i64>,
        quote: String,
    ) -> Result<i64, CoreError> {
        let to = parse_recipients(&to).ok_or(CoreError::BadRecipients)?;
        let from = self.account(AccountId(account))?;
        // Would wait in the outbox forever.
        if from.kind == AccountKind::Imap && from.smtp.is_none() {
            return Err(CoreError::NoSmtp);
        }
        let draft = Draft {
            account_id: from.id,
            to,
            subject,
            text,
            reply_to: reply_to.map(MessageId),
            quote,
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

    /// Finishes Google sign-in with the address the browser came back to: the account's
    /// address and the secret the app keeps for it. `add_gmail_account` then saves it.
    pub async fn google_account(
        &self,
        sign_in: GoogleSignIn,
        redirect: String,
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
        let email = GmailSession::new(tls, &tokens.access).email().await?;
        if self.exists(&email)? {
            return Err(CoreError::AccountExists);
        }
        Ok(GoogleAccount {
            email,
            secret: tokens.refresh.unwrap_or_default(),
        })
    }

    /// Sends the account's queued actions, waiting for held ones to come due; without any
    /// it does not connect. After a failure they stay queued for the next push or sync.
    /// `secret` is the account's password, or its Google secret.
    pub async fn push_actions(&self, account: i64, secret: String) -> Result<(), CoreError> {
        let account = self.account(AccountId(account))?;
        let mut waiter = None;
        loop {
            let wait = {
                let lock = self.busy(account.id);
                let _busy = lock.lock().await;
                // Removed while this push waited: nothing to send.
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
                    Ok(next) => next.map_or(0, |at| at - now()),
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

    /// Sends the account's queued actions and mail, then brings its folders and what changed
    /// in its inbox and sent folder into the database. `secret` as for `push_actions`.
    pub async fn sync(&self, account: i64, secret: String) -> Result<(), CoreError> {
        let id = AccountId(account);
        let lock = self.busy(id);
        let _busy = lock.lock().await;
        // Read under the lock: an account removed meanwhile has nothing to sync.
        let Some(account) = self.find(id)? else {
            return Ok(());
        };
        let mut slot = None;
        // A send that fails, say on a broken SMTP server, must not stop new mail coming in.
        let pushed = self.refused(&account, self.push_due(&account, &secret, &mut slot).await);
        let remote = self.remote(&mut slot, &account, &secret).await?;
        let synced = sync_mail(&self.store, remote, &self.parser, account.id, SYNC_LIMIT).await;
        self.refused(&account, synced)?;
        if let Some(remote) = slot {
            remote.close().await;
        }
        pushed
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
        let page = |body: Body| MessageBody {
            shows_images,
            ..body.into()
        };
        if let Some(body) = self.store.body(message)? {
            return Ok(page(body));
        }
        let location = self.store.location(message)?;
        let account = self.account(location.account_id)?;
        let mut slot = None;
        let remote = self.remote(&mut slot, &account, &secret).await?;
        let body = download_body(&self.store, remote, &self.parser, &location).await;
        // Closing without LOGOUT saves the reader a round trip; the server cleans up.
        drop(slot);
        Ok(page(self.refused(&account, body)?))
    }

    /// The message's page with its remote images loaded now, for `MessageBody::html`; None
    /// when it has none. An image that does not load stays out.
    pub async fn message_images(&self, message: i64) -> Result<Option<ImagesPage>, CoreError> {
        let Some(html) = self.store.body(MessageId(message))?.and_then(|b| b.html) else {
            return Ok(None);
        };
        let page = html::clean(&html);
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
                    .map(|_| format!("{IMAGE_ADDRESS}{message}/{n}"))
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

    /// From now on the remote images of mail from this message's sender load on their own.
    pub fn allow_images(&self, message: i64) -> Result<(), CoreError> {
        Ok(self.store.allow_images(MessageId(message))?)
    }
}

/// How many remote images a message loads, how many at a time, and how many bytes in all.
const IMAGES: usize = 48;
const IMAGES_AT_ONCE: usize = 6;
const IMAGES_SIZE: usize = 12 * 1024 * 1024;

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

impl Core {
    /// Queues a move held for the undo time and returns its ops for `undo`.
    fn hold_move(&self, message: i64, action: Action) -> Result<Vec<i64>, CoreError> {
        let ops = self.store.queue(MessageId(message), action, now() + HOLD)?;
        Ok(ops.into_iter().map(|op| op.0).collect())
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
        let remote = match account.kind {
            AccountKind::Imap => Remote::Imap(self.connect(account, secret).await?),
            AccountKind::Gmail => {
                let token = self.access_token(account.id, secret).await?;
                Remote::Gmail(GmailSession::new(self.tls()?, &token))
            }
            AccountKind::Graph => {
                return Err(CoreError::Storage {
                    detail: "Outlook accounts are not supported yet".into(),
                });
            }
        };
        Ok(slot.insert(remote))
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
    /// call gets a new one.
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
        let ops = self.store.due(account.id, now())?;
        if !ops.is_empty() {
            let remote = self.remote(slot, account, secret).await?;
            push_ops(&self.store, remote, ops).await?;
        }
        let mail = self.store.due_sends(account.id, now())?;
        if mail.is_empty() {
            return Ok(());
        }
        let from = Address {
            name: Some(account.display_name.clone()).filter(|n| *n != account.email),
            email: account.email.clone(),
        };
        if account.kind == AccountKind::Gmail {
            // Gmail keeps the copy in Sent itself.
            let copy: Option<SentCopy<'_, Remote>> = None;
            if let Remote::Gmail(gmail) = self.remote(slot, account, secret).await? {
                push_sends(&self.store, gmail, &MimeBuilder, &from, copy, mail).await?;
            }
            return Ok(());
        }
        let server = account.smtp.as_ref().ok_or(CoreError::NoSmtp)?;
        let mut smtp = self.send_login(server, secret).await?;
        let sent = if saves_sent_itself(&server.host) {
            None
        } else {
            let folders = self.store.folders(account.id)?;
            folders.into_iter().find(|f| f.role == FolderRole::Sent)
        };
        // Without IMAP the mail still goes; only its copy in Sent is missing.
        let copy = match &sent {
            Some(folder) => match self.remote(slot, account, secret).await {
                Ok(provider) => Some(SentCopy {
                    provider,
                    folder: &folder.remote_id,
                }),
                _ => None,
            },
            None => None,
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

/// The open connection to an account's server.
enum Remote {
    Imap(ImapSession),
    Gmail(GmailSession),
}

impl Remote {
    async fn close(self) {
        if let Self::Imap(imap) = self {
            let _ = imap.logout().await;
        }
    }
}

/// Passes each call on to the account's own provider.
impl MailProvider for Remote {
    async fn folders(&mut self) -> Result<Vec<NewFolder>, ProviderError> {
        match self {
            Self::Imap(p) => p.folders().await,
            Self::Gmail(p) => p.folders().await,
        }
    }

    async fn changes(
        &mut self,
        folder: &str,
        since: Checkpoint,
        known: &[String],
        limit: u32,
    ) -> Result<Changes, ProviderError> {
        match self {
            Self::Imap(p) => p.changes(folder, since, known, limit).await,
            Self::Gmail(p) => p.changes(folder, since, known, limit).await,
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
        }
    }

    async fn move_to(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> Result<(), ProviderError> {
        match self {
            Self::Imap(p) => p.move_to(folder, validity, remote_ids, to).await,
            Self::Gmail(p) => p.move_to(folder, validity, remote_ids, to).await,
        }
    }

    async fn append(&mut self, folder: &str, message: &[u8]) -> Result<(), ProviderError> {
        match self {
            Self::Imap(p) => p.append(folder, message).await,
            Self::Gmail(p) => p.append(folder, message).await,
        }
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
    /// 0xRRGGBB.
    pub color: u32,
    /// The colour's place in `account_colors`.
    pub color_index: u32,
}

impl From<Account> for AccountItem {
    fn from(a: Account) -> Self {
        Self {
            id: a.id.0,
            color: rgb(a.color),
            color_index: a.color % ACCOUNT_COLORS.len() as u32,
            email: a.email,
            display_name: a.display_name,
        }
    }
}

/// Someone who wrote in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SenderItem {
    /// The name, or the address when there is none.
    pub name: String,
    /// The user.
    pub me: bool,
}

impl From<Sender> for SenderItem {
    fn from(s: Sender) -> Self {
        Self {
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
            name: a.display_name.clone(),
            color: rgb(a.color),
        }
    }
}

/// The colours an account can have, as 0xRRGGBB.
#[uniffi::export]
pub fn account_colors() -> Vec<u32> {
    ACCOUNT_COLORS.to_vec()
}

/// Account colours in the order accounts get them: the logo's three first.
const ACCOUNT_COLORS: [u32; 10] = [
    0xEF_4A_5C, 0x14_A3_8B, 0x8A_5C_F6, 0xE8_90_2A, 0xE8_B5_2A, 0x43_A9_5A, 0x2F_8C_F0, 0x4F_5B_D5,
    0xD1_48_9A, 0x5F_6B_85,
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
    /// Goes under the text; empty for none.
    pub quote: String,
}

impl From<Draft> for DraftItem {
    fn from(d: Draft) -> Self {
        Self {
            account_id: d.account_id.0,
            to: d.to.join(", "),
            subject: d.subject,
            text: d.text,
            reply_to: d.reply_to.map(|m| m.0),
            quote: d.quote,
        }
    }
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
        "; style-src 'unsafe-inline'; font-src data:; ",
        "base-uri 'none'; form-action 'none'\">",
        "<meta http-equiv=\"x-dns-prefetch-control\" content=\"off\">",
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
        "<style>body{margin:0;padding:0 16px 16px;overflow-wrap:break-word}",
        "img{max-width:100%;height:auto}",
        "details.ozero-quote>summary{list-style:none;display:inline-block;margin:10px 0;",
        "padding:0 10px;border-radius:9px;background:#eef0f3;color:#6b7280;",
        "font:700 13px/22px sans-serif;letter-spacing:1px}",
        "details.ozero-quote>summary::-webkit-details-marker{display:none}",
    );
    const LIGHT: &str = "html{color-scheme:light;background:#fff}";
    // The app's own colours, kept in step with the Android Theme.kt; the web view is clear, so the screen shows through.
    const THEMED: &str = concat!(
        "html{color-scheme:light dark;background:transparent;color:#1d1f24}",
        "a{color:#2f7cf6}",
        "@media(prefers-color-scheme:dark){html{color:#ececee}a{color:#4c8ff8}",
        "details.ozero-quote>summary{background:#25272b;color:#8e939c}}",
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
    /// In mixed lists (the unified inbox, search), set when there is more than one account.
    pub account: Option<AccountMark>,
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
            thread_count: row.thread_count,
            thread_unread: row.thread_unread,
            senders: row.senders.into_iter().map(SenderItem::from).collect(),
            sent: row.sent,
            account: None,
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
    /// The account has no folder marked as its trash.
    NoTrash,
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
            Self::NoTrash => write!(f, "the account has no trash folder"),
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
            StoreError::NoTrash => Self::NoTrash,
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
