//! Facade the Android and iOS apps call through UniFFI.

use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use ozero_app::sync::{SyncError, sync_inbox};
use ozero_domain::{
    Account, AccountId, AccountKind, FolderRole, ListQuery, ListScope, MessageRow, NewAccount,
    ProviderError, Store, StoreError,
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

    pub fn add_imap_account(&self, email: String, display_name: String) -> Result<i64, CoreError> {
        let account = NewAccount {
            kind: AccountKind::Imap,
            email,
            display_name,
        };
        Ok(self.store.add_account(&account)?.0)
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
    /// Brings the account's folders and newest inbox messages into the database.
    pub async fn sync_imap(
        &self,
        account: i64,
        host: String,
        port: u16,
        user: String,
        password: String,
    ) -> Result<(), CoreError> {
        let account = AccountId(account);
        // Check before going online, so a stale id costs no login.
        let known = self.store.accounts()?;
        if !known
            .iter()
            .any(|a| a.id == account && a.kind == AccountKind::Imap)
        {
            return Err(CoreError::Storage {
                message: "no such IMAP account".into(),
            });
        }
        let mut imap = ImapSession::connect(&host, port, self.tls()?, &user, &password).await?;
        sync_inbox(&self.store, &mut imap, &self.parser, account, SYNC_LIMIT).await?;
        // The data is saved; a failed goodbye changes nothing.
        let _ = imap.logout().await;
        Ok(())
    }
}

impl Core {
    /// Built on first use: on Android the verifier must be set up from Kotlin before that.
    fn tls(&self) -> Result<Arc<ClientConfig>, CoreError> {
        if let Some(tls) = self.tls.get() {
            return Ok(tls.clone());
        }
        let tls = tls_config()?;
        Ok(self.tls.get_or_init(|| tls).clone())
    }
}

#[cfg(not(target_os = "android"))]
fn tls_config() -> Result<Arc<ClientConfig>, CoreError> {
    ozero_imap::platform_tls().map_err(|e| CoreError::Tls {
        message: e.to_string(),
    })
}

/// The Android verifier needs a JNI handshake that is not wired yet; it would panic
/// on first use, so refuse cleanly until it is.
#[cfg(target_os = "android")]
fn tls_config() -> Result<Arc<ClientConfig>, CoreError> {
    Err(CoreError::Tls {
        message: "certificate checks are not set up on Android yet".into(),
    })
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

/// Apps map each variant to a string key; `message` and `Display` are for logs and never hold mail contents.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
pub enum CoreError {
    /// The database key is wrong or not 32 bytes.
    WrongKey,
    /// The database was written by a newer app version.
    TooNew,
    /// The server refused the login.
    Auth,
    Network {
        message: String,
    },
    /// Secure connections cannot be set up on this device.
    Tls {
        message: String,
    },
    Server {
        message: String,
    },
    Storage {
        message: String,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongKey => write!(f, "the database key is wrong"),
            Self::TooNew => write!(f, "the database is from a newer app version"),
            Self::Auth => write!(f, "the server refused the login"),
            Self::Network { message } => write!(f, "network error: {message}"),
            Self::Tls { message } => write!(f, "TLS setup failed: {message}"),
            Self::Server { message } => write!(f, "server error: {message}"),
            Self::Storage { message } => write!(f, "storage error: {message}"),
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
                message: other.to_string(),
            },
        }
    }
}

impl From<ProviderError> for CoreError {
    fn from(e: ProviderError) -> Self {
        match e {
            ProviderError::Auth => Self::Auth,
            ProviderError::Network(message) => Self::Network { message },
            ProviderError::Protocol(message) => Self::Server { message },
        }
    }
}

impl From<SyncError> for CoreError {
    fn from(e: SyncError) -> Self {
        match e {
            SyncError::Provider(e) => e.into(),
            SyncError::Store(e) => e.into(),
            SyncError::NoInbox => Self::Server {
                message: e.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests;
