//! Facade the Android and iOS apps call through UniFFI.

use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use ozero_app::sync::{SyncError, download_body, sync_inbox};
use ozero_domain::{
    Account, AccountId, AccountKind, Body, FolderRole, ListQuery, ListScope, MessageId, MessageRow,
    NewAccount, ProviderError, Server, Store, StoreError,
};
use ozero_imap::ImapSession;
use ozero_imap::rustls::ClientConfig;
use ozero_mime::MimeParser;
use ozero_store_sqlite::SqliteStore;

uniffi::setup_scaffolding!();

/// How many of the newest inbox messages one sync brings in.
const SYNC_LIMIT: u32 = 200;

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

/// The mail core of one app install: the encrypted database and the network.
#[derive(uniffi::Object)]
pub struct Core {
    store: SqliteStore,
    parser: MimeParser,
    tls: OnceLock<Arc<ClientConfig>>,
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

    /// The newest messages of all inboxes, newest first.
    pub fn unified_inbox(&self, limit: u32) -> Result<Vec<MessageItem>, CoreError> {
        let rows = self.store.list(&ListQuery {
            scope: ListScope::Unified(FolderRole::Inbox),
            limit,
            after: None,
        })?;
        Ok(rows.into_iter().map(MessageItem::from).collect())
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Core {
    /// Saves an IMAP account once the server has accepted the login; `email` is the login name.
    pub async fn add_imap_account(
        &self,
        email: String,
        display_name: String,
        host: String,
        port: u16,
        password: String,
    ) -> Result<i64, CoreError> {
        // Checked before going online, so a repeat sign-in costs no login.
        let known = self.store.accounts()?;
        if known.iter().any(|a| a.email.eq_ignore_ascii_case(&email)) {
            return Err(CoreError::AccountExists);
        }
        let server = Server {
            host,
            port,
            user: email.clone(),
        };
        let imap = self.login(&server, &password).await?;
        // The login worked; a failed goodbye changes nothing.
        let _ = imap.logout().await;
        let account = NewAccount {
            kind: AccountKind::Imap,
            email,
            display_name,
            server: Some(server),
        };
        Ok(self.store.add_account(&account)?.0)
    }

    /// Brings the account's folders and newest inbox messages into the database.
    pub async fn sync_imap(&self, account: i64, password: String) -> Result<(), CoreError> {
        let account = AccountId(account);
        let mut imap = self.connect(account, &password).await?;
        sync_inbox(&self.store, &mut imap, &self.parser, account, SYNC_LIMIT).await?;
        let _ = imap.logout().await;
        Ok(())
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
        let mut imap = self.connect(location.account_id, &password).await?;
        let body = download_body(&self.store, &mut imap, &self.parser, &location).await?;
        // Closing without LOGOUT saves the reader a round trip; the server cleans up.
        drop(imap);
        Ok(body.into())
    }
}

impl Core {
    /// Logs in to a saved IMAP account.
    async fn connect(&self, account: AccountId, password: &str) -> Result<ImapSession, CoreError> {
        let server = self
            .store
            .accounts()?
            .into_iter()
            .find(|a| a.id == account && a.kind == AccountKind::Imap)
            .and_then(|a| a.server)
            .ok_or_else(|| CoreError::Storage {
                detail: "no such IMAP account".into(),
            })?;
        self.login(&server, password).await
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
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
        "<style>html{color-scheme:light;background:#fff}",
        "body{margin:0;padding:0 16px 16px;overflow-wrap:break-word}",
        "img{max-width:100%;height:auto}</style>",
        "</head><body dir=\"auto\">",
    );
    let mut page = String::with_capacity(HEAD.len() + html.len() + 14);
    page.push_str(HEAD);
    page.push_str(html);
    page.push_str("</body></html>");
    page
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
            ProviderError::Protocol(detail) => Self::Server { detail },
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
