/// Local id of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(pub i64);

/// How the core talks to the account's server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountKind {
    Imap,
    Gmail,
    Graph,
}

/// Where an IMAP account lives; the password is kept by the platform, not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub host: String,
    pub port: u16,
    pub user: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount {
    pub kind: AccountKind,
    pub email: String,
    pub display_name: String,
    /// Set for IMAP accounts.
    pub server: Option<Server>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub kind: AccountKind,
    pub email: String,
    pub display_name: String,
    pub server: Option<Server>,
}
