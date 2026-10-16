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
    /// Where IMAP accounts send mail; same password as `server`.
    pub smtp: Option<Server>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub kind: AccountKind,
    pub email: String,
    pub display_name: String,
    pub server: Option<Server>,
    pub smtp: Option<Server>,
}

/// The usual IMAP host for an email address (`imap.<domain>`), for sign-in to suggest.
pub fn guess_imap_host(email: &str) -> Option<String> {
    guess_host(email, "imap")
}

/// The usual SMTP host for an email address (`smtp.<domain>`).
pub fn guess_smtp_host(email: &str) -> Option<String> {
    guess_host(email, "smtp")
}

fn guess_host(email: &str, service: &str) -> Option<String> {
    // The last '@': a quoted local part may contain one too.
    let domain = email.rsplit_once('@')?.1.trim().to_ascii_lowercase();
    (!domain.is_empty()).then(|| format!("{service}.{domain}"))
}

/// Servers that put a sent message into the Sent folder on their own, so the client must not.
pub fn saves_sent_itself(smtp_host: &str) -> bool {
    let host = smtp_host.trim_end_matches('.').to_ascii_lowercase();
    [
        "gmail.com",
        "googlemail.com",
        "office365.com",
        "outlook.com",
    ]
    .iter()
    .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_the_imap_host() {
        assert_eq!(
            guess_imap_host("a@example.com ").as_deref(),
            Some("imap.example.com")
        );
        assert_eq!(
            guess_imap_host("\"a@b\"@Example.com").as_deref(),
            Some("imap.example.com")
        );
        assert_eq!(guess_imap_host("a@"), None);
        assert_eq!(guess_imap_host("example.com"), None);
        assert_eq!(
            guess_smtp_host("a@example.com").as_deref(),
            Some("smtp.example.com")
        );
    }

    #[test]
    fn knows_servers_that_save_sent_mail() {
        assert!(saves_sent_itself("smtp.gmail.com"));
        assert!(saves_sent_itself("SMTP.Office365.com."));
        assert!(!saves_sent_itself("smtp.example.com"));
        assert!(!saves_sent_itself("notgmail.com"));
    }
}
