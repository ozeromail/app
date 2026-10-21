use std::fmt::Write;

use crate::{AccountId, MessageId};

/// Local id of a message waiting to be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboxId(pub i64);

/// A message the user wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub account_id: AccountId,
    /// Bare addresses, checked by `parse_recipients`.
    pub to: Vec<String>,
    pub subject: String,
    pub text: String,
    /// The message this answers.
    pub reply_to: Option<MessageId>,
    /// The answered message, quoted under the text; empty for none.
    pub quote: String,
    /// The forwarded message, whose files go with it.
    pub forward: Option<MessageId>,
}

/// Local id of a message kept on the device while it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DraftId(pub i64);

/// A message being written, kept on the device until it is sent or thrown away. Never on
/// the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedDraft {
    /// None until it is first saved.
    pub id: Option<DraftId>,
    /// None while no account is picked.
    pub account_id: Option<AccountId>,
    /// The recipients as typed, checked only when it is sent.
    pub to: String,
    pub subject: String,
    pub text: String,
    pub reply_to: Option<MessageId>,
    pub quote: String,
    pub forward: Option<MessageId>,
    /// Milliseconds since the Unix epoch of the last save.
    pub saved: i64,
}

/// A message the server refused, kept for the user to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsent {
    pub id: OutboxId,
    pub subject: String,
    /// The server's answer.
    pub reason: String,
}

/// A queued message, with what sending it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub id: OutboxId,
    pub to: Vec<String>,
    pub subject: String,
    /// With the quote under it.
    pub text: String,
    /// `Message-ID` without angle brackets, made when it was queued so a retry keeps it.
    pub message_id: String,
    /// The `Message-ID` of the message this answers.
    pub in_reply_to: Option<String>,
    /// Seconds since the Unix epoch, for the `Date` header.
    pub date: i64,
    /// The forwarded message, whose files go with it.
    pub forward: Option<MessageId>,
    /// The files sent with it, read from `forward` when it is sent.
    pub files: Vec<OutgoingFile>,
}

/// A file sent with a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingFile {
    pub name: String,
    /// The content type, such as `application/pdf`.
    pub mime: String,
    pub data: Vec<u8>,
}

/// The recipients typed into one field: addresses split by commas, semicolons or spaces.
/// None if one of them is not an address, or there are none.
pub fn parse_recipients(field: &str) -> Option<Vec<String>> {
    let to: Vec<String> = field
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect();
    (!to.is_empty() && to.iter().all(|a| is_address(a))).then_some(to)
}

/// One `@` with something on both sides, and nothing that could break a header or command.
/// ASCII only: sending to `пример.рф` needs SMTPUTF8, which we do not speak yet.
pub fn is_address(address: &str) -> bool {
    let Some((local, domain)) = address.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !local.contains('@')
        && address.chars().all(|c| {
            c.is_ascii_graphic() && !matches!(c, '<' | '>' | '"' | '(' | ')' | '[' | ']' | '\\')
        })
}

/// `text` of `header` marked with `>` line by line, the way mail clients quote.
pub fn quote(header: String, text: &str) -> String {
    let mut quote = header;
    for line in text.trim().lines() {
        quote.push_str(if line.is_empty() { "\n>" } else { "\n> " });
        quote.push_str(line);
    }
    quote
}

/// What a message says: the text, then the quote, if any, after a blank line.
pub fn with_quote(text: &str, quote: &str) -> String {
    let text = text.trim_end();
    match (text.is_empty(), quote.is_empty()) {
        (_, true) => text.to_owned(),
        (true, false) => quote.to_owned(),
        (false, false) => format!("{text}\n\n{quote}"),
    }
}

/// `Re: ` once, the way mail clients answer.
pub fn reply_subject(subject: &str) -> String {
    prefixed(subject, "Re: ", &["re:"])
}

/// `Fwd: ` once, the way mail clients forward; `Fw: ` counts too.
pub fn forward_subject(subject: &str) -> String {
    prefixed(subject, "Fwd: ", &["fwd:", "fw:"])
}

/// `prefix` before the subject unless it starts with one of `known`, in any case.
fn prefixed(subject: &str, prefix: &str, known: &[&str]) -> String {
    let trimmed = subject.trim();
    let has = known.iter().any(|k| {
        trimmed
            .get(..k.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(k))
    });
    if has {
        trimmed.to_owned()
    } else {
        format!("{prefix}{trimmed}")
    }
}

/// A forwarded message under the text: `title`, a line for each of its `fields` that has a
/// value, a blank line, then its `text` as it is.
pub fn forward_quote(title: &str, fields: &[(&str, &str)], text: &str) -> String {
    let mut quote = title.to_owned();
    for (label, value) in fields.iter().filter(|(_, v)| !v.is_empty()) {
        let _ = write!(quote, "\n{label} {value}");
    }
    let text = text.trim();
    if !text.is_empty() {
        quote.push_str("\n\n");
        quote.push_str(text);
    }
    quote
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_recipients() {
        assert_eq!(
            parse_recipients(" anna@example.com, bob@example.org;\ncat@example.net ").as_deref(),
            Some(
                &[
                    "anna@example.com".to_owned(),
                    "bob@example.org".into(),
                    "cat@example.net".into()
                ][..]
            )
        );
    }

    #[test]
    fn refuses_what_is_not_an_address() {
        for field in [
            "",
            " , ",
            "anna",
            "@example.com",
            "anna@",
            "a@b@c",
            "<a@b>",
            "a@b\r\nRCPT",
            "cat@пример.рф",
        ] {
            assert_eq!(parse_recipients(field), None, "{field:?}");
        }
    }

    #[test]
    fn quotes_line_by_line() {
        let quote = quote("Anna wrote:".into(), "Hi\n\nBye\n");
        assert_eq!(quote, "Anna wrote:\n> Hi\n>\n> Bye");
        assert_eq!(with_quote("Hello\n", &quote), format!("Hello\n\n{quote}"));
        assert_eq!(with_quote("Hello", ""), "Hello");
        assert_eq!(with_quote(" \n", &quote), quote);
    }

    #[test]
    fn answers_with_one_re() {
        assert_eq!(reply_subject("Hi"), "Re: Hi");
        assert_eq!(reply_subject("RE: Hi"), "RE: Hi");
        assert_eq!(reply_subject(" "), "Re: ");
        assert_eq!(reply_subject("Отчёт"), "Re: Отчёт");
    }

    #[test]
    fn forwards_with_one_fwd() {
        assert_eq!(forward_subject("Hi"), "Fwd: Hi");
        assert_eq!(forward_subject(" FWD: Hi"), "FWD: Hi");
        assert_eq!(forward_subject("Fw: Hi"), "Fw: Hi");
        assert_eq!(forward_subject("Re: Hi"), "Fwd: Re: Hi");
        assert_eq!(forward_subject("Отчёт"), "Fwd: Отчёт");
    }

    #[test]
    fn forwards_the_text_under_its_header() {
        let fields = [("From:", "Anna <anna@example.com>"), ("To:", "")];
        assert_eq!(
            forward_quote("Forwarded message", &fields, "\nHi\n> old\n"),
            "Forwarded message\nFrom: Anna <anna@example.com>\n\nHi\n> old"
        );
        assert_eq!(
            forward_quote("Forwarded", &fields, " "),
            "Forwarded\nFrom: Anna <anna@example.com>"
        );
    }
}
