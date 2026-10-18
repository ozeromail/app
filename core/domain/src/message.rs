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
    /// The `Message-ID`s this one answers: `In-Reply-To` and `References`, oldest first.
    pub references: Vec<String>,
    pub from: Address,
    /// `Reply-To`: where answers go instead of `from`.
    pub answer_to: Vec<String>,
    /// `To` and `Cc`.
    pub recipients: Vec<String>,
    pub subject: String,
    pub snippet: String,
    /// Seconds since the Unix epoch.
    pub date: i64,
    pub flags: Flags,
    pub category: Category,
}

/// The addresses of a saved message, for answering it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Correspondents {
    pub from: String,
    pub answer_to: Vec<String>,
    pub recipients: Vec<String>,
}

impl Correspondents {
    /// Who an answer from `own` goes to: the sender, or `Reply-To` when there is one; with
    /// `all`, everyone else it went to as well. Never `own`, and each address once. An answer
    /// to the user's own message goes to the people it was sent to.
    pub fn answer(&self, own: &str, all: bool) -> Vec<String> {
        let first = if self.answer_to.is_empty() {
            std::slice::from_ref(&self.from)
        } else {
            &self.answer_to[..]
        };
        let mine = self.from.eq_ignore_ascii_case(own);
        let rest = if all || mine {
            &self.recipients[..]
        } else {
            &[]
        };
        let mut to: Vec<String> = Vec::new();
        for address in first.iter().chain(rest) {
            let known = to.iter().any(|a| a.eq_ignore_ascii_case(address));
            if !address.is_empty() && !address.eq_ignore_ascii_case(own) && !known {
                to.push(address.clone());
            }
        }
        to
    }
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
    /// In a list, the messages of its conversation in the same folder and inbox tab, this
    /// one included;
    /// elsewhere 1.
    pub thread_count: u32,
    /// How many of those are unread.
    pub thread_unread: u32,
    /// Who wrote in the conversation, the user's replies included, oldest first, each once.
    pub senders: Vec<Sender>,
    /// In the sent folder: the user wrote it.
    pub sent: bool,
}

/// Someone who wrote in a conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sender {
    /// The name, or the address when there is none.
    pub name: String,
    /// The user, writing from the sent folder.
    pub me: bool,
}

impl Sender {
    /// Who wrote, from each message's date, whether the user sent it, address and name:
    /// oldest first, each address once. An address the user wrote from is the user's.
    pub fn collect(mut wrote: Vec<(i64, bool, &str, &str)>) -> Vec<Self> {
        wrote.sort_by_key(|w| w.0);
        let mut senders: Vec<(String, Self)> = Vec::new();
        for (_, me, email, name) in wrote {
            let email = email.to_lowercase();
            match senders.iter_mut().find(|(e, _)| *e == email) {
                Some((_, known)) => known.me |= me,
                None => {
                    let name = if name.trim().is_empty() {
                        email.clone()
                    } else {
                        name.to_owned()
                    };
                    senders.push((email, Self { name, me }));
                }
            }
        }
        senders.into_iter().map(|(_, s)| s).collect()
    }
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
            .filter(|l| !is_quoted(l))
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

    /// The text without the quoted mail at its end, and that quote with the line that
    /// introduces it, such as "On Mon, 1 Oct, Anna wrote:". No quote when nothing is
    /// written above it.
    pub fn split_quote(&self) -> (&str, &str) {
        let text = self.text.trim_end();
        let mut start = text.len();
        let mut quoted = false;
        // Back over the quoted lines and the blank lines between them.
        for line in text.lines().rev() {
            let blank = line.trim().is_empty();
            if !blank && !is_quoted(line) {
                break;
            }
            quoted |= !blank;
            start -= line.len();
            start = text[..start].trim_end_matches(['\n', '\r']).len();
        }
        if !quoted {
            return (text, "");
        }
        let above = text[..start].trim_end();
        let intro = above.rsplit('\n').next().unwrap_or_default();
        // "On 1 Oct 2026, Anna wrote:" has a date; "My answers:" is the user's own line.
        let intro_line = intro.trim_end();
        let start =
            if intro_line.ends_with(':') && intro_line.contains(|c: char| c.is_ascii_digit()) {
                above.len() - intro.len()
            } else {
                start
            };
        let reply = text[..start].trim_end();
        if reply.is_empty() {
            return (text, "");
        }
        (reply, text[start..].trim_start_matches(['\n', '\r']))
    }
}

/// A line of quoted mail, `> ...`.
fn is_quoted(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

/// Where a saved message lives on its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageLocation {
    pub id: MessageId,
    pub account_id: AccountId,
    pub folder_remote_id: String,
    /// The folder's validity when the message was saved; see `Checkpoint::validity`.
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
    fn splits_the_quote_at_the_end() {
        let body = |text: &str| Body {
            text: text.into(),
            ..Body::default()
        };
        let b = body("Yes, at one.\n\nOn Mon, 12 Oct, Anna wrote:\n> Lunch?\n>\n> Or later\n\n");
        assert_eq!(
            b.split_quote(),
            (
                "Yes, at one.",
                "On Mon, 12 Oct, Anna wrote:\n> Lunch?\n>\n> Or later"
            )
        );
        let b = body("Yes\r\n> Lunch?\r\n");
        assert_eq!(b.split_quote(), ("Yes", "> Lunch?"));
        let b = body("Thanks\nMy answers:\n> Q1");
        assert_eq!(b.split_quote(), ("Thanks\nMy answers:", "> Q1"));
        for text in [
            "> Lunch?\nYes",
            "On 1 Oct Anna wrote:\n> Lunch?",
            "Just text",
        ] {
            assert_eq!(body(text).split_quote(), (text, ""), "{text}");
        }
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

    fn mail(from: &str, answer_to: &[&str], recipients: &[&str]) -> Correspondents {
        let list = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect();
        Correspondents {
            from: from.into(),
            answer_to: list(answer_to),
            recipients: list(recipients),
        }
    }

    #[test]
    fn answers_the_sender_or_reply_to() {
        let m = mail("anna@x.com", &[], &["me@x.com", "bob@x.com"]);
        assert_eq!(m.answer("me@x.com", false), ["anna@x.com"]);
        let m = mail("list@x.com", &["anna@x.com"], &["me@x.com"]);
        assert_eq!(m.answer("me@x.com", false), ["anna@x.com"]);
    }

    #[test]
    fn answers_all_without_me_or_repeats() {
        let m = mail("anna@x.com", &[], &["ME@x.com", "bob@x.com", "Anna@x.com"]);
        assert_eq!(m.answer("me@x.com", true), ["anna@x.com", "bob@x.com"]);
    }

    #[test]
    fn answers_my_own_message_to_its_recipients() {
        let m = mail("me@x.com", &[], &["anna@x.com", "bob@x.com"]);
        assert_eq!(m.answer("me@x.com", false), ["anna@x.com", "bob@x.com"]);
    }
}
