/// Local id of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(pub i64);

/// How the core talks to the account's server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountKind {
    Imap,
    Gmail,
    Graph,
    /// Exchange Web Services: an Exchange server a company runs itself.
    Exchange,
}

/// Where an IMAP account lives; the password is kept by the platform, not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// The client certificate the server asks for, by the name the platform keeps it under.
    pub certificate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount {
    pub kind: AccountKind,
    pub email: String,
    pub display_name: String,
    /// Set for IMAP accounts, and for Exchange ones: the EWS host and the login.
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
    /// Which of the account colours marks its mail; the next one when it was added.
    pub color: u32,
    /// The name the user gave the account on this phone; empty when none.
    pub label: String,
    /// How many days back its mail is kept on the phone; 0 keeps all of it.
    pub keep_days: u32,
    /// When the phone looks for its new mail while the app is closed.
    pub new_mail: NewMail,
    /// The server refused the saved password or Google secret: the account waits for the
    /// person to sign in again, its mail and queued work kept.
    pub needs_sign_in: bool,
}

/// When the phone looks for an account's new mail while the app is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NewMail {
    /// As soon as the server has it, on a connection kept open.
    #[default]
    Push,
    /// Every so many minutes, as often as the system lets it.
    Every { minutes: u32 },
    /// Only when the app is open.
    Manually,
}

impl Account {
    /// What the apps call the account: the user's name for it, else the address. Not the
    /// sender name: a person's accounts all share it, so it would not tell them apart.
    pub fn name(&self) -> &str {
        [&self.label, &self.email]
            .into_iter()
            .map(|n| n.trim())
            .find(|n| !n.is_empty())
            .unwrap_or_default()
    }
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
    Some(format!("{service}.{}", email_domain(email)?))
}

/// A mail service sign-in knows by the domain of an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MailService {
    Google,
    ICloud,
    Yahoo,
    Yandex,
    MailRu,
}

/// The service an address is at, if sign-in knows it by its domain.
pub fn mail_service(email: &str) -> Option<MailService> {
    let service = match email_domain(email)?.as_str() {
        "gmail.com" | "googlemail.com" => MailService::Google,
        "icloud.com" | "me.com" | "mac.com" => MailService::ICloud,
        "yahoo.com" | "ymail.com" | "rocketmail.com" => MailService::Yahoo,
        "yandex.ru" | "yandex.com" | "ya.ru" | "yandex.by" | "yandex.kz" => MailService::Yandex,
        "mail.ru" | "bk.ru" | "list.ru" | "inbox.ru" | "internet.ru" => MailService::MailRu,
        _ => return None,
    };
    Some(service)
}

/// What to call a new account at first: its service, or its domain without the ending
/// (`tbank.ru` is Tbank). Empty when the address says nothing.
pub fn default_label(email: &str) -> String {
    let known = match mail_service(email) {
        Some(MailService::Google) => "Gmail",
        Some(MailService::ICloud) => "iCloud",
        Some(MailService::Yahoo) => "Yahoo",
        Some(MailService::Yandex) => "Yandex",
        Some(MailService::MailRu) => "Mail.ru",
        None => "",
    };
    if !known.is_empty() {
        return known.to_owned();
    }
    let domain = email_domain(email).unwrap_or_default();
    let mut parts = domain.split('.').filter(|p| !p.is_empty());
    let name = match (parts.next(), parts.next()) {
        // A host such as mail.company.com is named by the company.
        (Some("mail" | "email" | "corp"), Some(next)) => next,
        (Some(first), Some(_)) => first,
        _ => "",
    };
    let mut chars = name.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The domain of an email address, lowercased; None without one.
pub fn email_domain(email: &str) -> Option<String> {
    // The last '@': a quoted local part may contain one too.
    let domain = email.rsplit_once('@')?.1.trim().to_ascii_lowercase();
    (!domain.is_empty()).then_some(domain)
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
    fn is_called_by_the_users_name_first() {
        let mut account = Account {
            id: AccountId(1),
            kind: AccountKind::Imap,
            email: "a@example.com".into(),
            display_name: " ".into(),
            server: None,
            smtp: None,
            color: 0,
            label: String::new(),
            keep_days: 0,
            new_mail: NewMail::Push,
            needs_sign_in: false,
        };
        assert_eq!(account.name(), "a@example.com");
        account.display_name = "Anna".into();
        assert_eq!(account.name(), "a@example.com");
        account.label = "Work".into();
        assert_eq!(account.name(), "Work");
    }

    #[test]
    fn names_a_new_account_by_its_domain() {
        assert_eq!(default_label("k@tbank.ru"), "Tbank");
        assert_eq!(default_label("a@gmail.com"), "Gmail");
        assert_eq!(default_label("a@mail.example.co.uk"), "Example");
        assert_eq!(default_label("a@yandex.ru"), "Yandex");
        assert_eq!(default_label("a@localhost"), "");
        assert_eq!(default_label("nobody"), "");
    }

    #[test]
    fn tells_a_known_service_by_the_domain() {
        assert_eq!(mail_service("a@Gmail.com "), Some(MailService::Google));
        assert_eq!(mail_service("a@ya.ru"), Some(MailService::Yandex));
        assert_eq!(mail_service("a@bk.ru"), Some(MailService::MailRu));
        assert_eq!(mail_service("a@example.com"), None);
        assert_eq!(mail_service("gmail.com"), None);
    }

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
