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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount {
    pub kind: AccountKind,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub kind: AccountKind,
    pub email: String,
    pub display_name: String,
}
