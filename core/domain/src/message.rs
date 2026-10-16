use crate::{AccountId, Category, FolderId};

/// Local id of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId(pub i64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
}

/// A message header as the server reports it. Bodies are stored separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    /// Provider id inside the folder: an IMAP UID, a Gmail or Graph message id.
    pub remote_id: String,
    /// The `Message-ID` header, used for threading.
    pub message_id: Option<String>,
    pub from: Address,
    pub subject: String,
    pub snippet: String,
    /// Seconds since the Unix epoch.
    pub date: i64,
    pub flags: Flags,
    pub category: Category,
}

/// One row of a message list, ready for the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub id: MessageId,
    pub account_id: AccountId,
    pub from: Address,
    pub subject: String,
    pub snippet: String,
    pub date: i64,
    pub flags: Flags,
}

impl MessageRow {
    /// Where the next page starts.
    pub fn cursor(&self) -> Cursor {
        Cursor {
            date: self.date,
            id: self.id,
        }
    }
}

/// The readable part of a message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Body {
    /// Plain text; an HTML-only message is converted to text.
    pub text: String,
    /// The HTML part as sent, if there is one.
    pub html: Option<String>,
}

impl Body {
    /// The start of the text on one line, for message lists.
    pub fn snippet(&self) -> String {
        const LEN: usize = 160;
        let (mut out, mut len) = (String::new(), 0);
        let words = self
            .text
            .lines()
            .filter(|l| !l.trim_start().starts_with('>'))
            .flat_map(str::split_whitespace);
        for word in words {
            let word_len = word.chars().count();
            if len == 0 && word_len > LEN {
                // One long word, such as a link: cut it rather than show nothing.
                return word.chars().take(LEN).collect();
            }
            if len + word_len + 1 > LEN {
                break;
            }
            if len > 0 {
                out.push(' ');
                len += 1;
            }
            out.push_str(word);
            len += word_len;
        }
        out
    }
}

/// Where a saved message lives on its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageLocation {
    pub id: MessageId,
    pub account_id: AccountId,
    pub folder_remote_id: String,
    /// The folder's validity when the message was saved; see `Recent::validity`.
    pub validity: u32,
    pub remote_id: String,
}

/// Keyset position in a list sorted newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub date: i64,
    pub id: MessageId,
}

/// Which messages a list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListScope {
    /// One folder of one account.
    Folder(FolderId),
    /// The folders with this role across all accounts, e.g. the unified inbox.
    Unified(crate::FolderRole),
    /// One tab of the unified inbox.
    Inbox(Category),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListQuery {
    pub scope: ListScope,
    pub limit: u32,
    /// Show messages older than this; `None` starts from the newest.
    pub after: Option<Cursor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(text: &str) -> String {
        Body {
            text: text.into(),
            ..Body::default()
        }
        .snippet()
    }

    #[test]
    fn snippet_joins_lines_and_skips_quotes() {
        assert_eq!(
            snippet("Hi Anna,\n\n> old reply\n  see you\tsoon"),
            "Hi Anna, see you soon"
        );
    }

    #[test]
    fn snippet_stops_at_a_word() {
        let s = snippet(&"слово ".repeat(100));
        assert!(s.chars().count() <= 160);
        assert!(s.ends_with("слово"));
    }

    #[test]
    fn snippet_cuts_a_long_first_word() {
        assert_eq!(snippet(&"x".repeat(500)).len(), 160);
    }
}
