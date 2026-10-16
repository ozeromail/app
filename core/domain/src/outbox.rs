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
    pub text: String,
    /// `Message-ID` without angle brackets, made when it was queued so a retry keeps it.
    pub message_id: String,
    /// The `Message-ID` of the message this answers.
    pub in_reply_to: Option<String>,
    /// Seconds since the Unix epoch, for the `Date` header.
    pub date: i64,
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

/// `Re: ` once, the way mail clients answer.
pub fn reply_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    let prefixed = trimmed
        .get(..3)
        .is_some_and(|p| p.eq_ignore_ascii_case("re:"));
    if prefixed {
        trimmed.to_owned()
    } else {
        format!("Re: {trimmed}")
    }
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
    fn answers_with_one_re() {
        assert_eq!(reply_subject("Hi"), "Re: Hi");
        assert_eq!(reply_subject("RE: Hi"), "RE: Hi");
        assert_eq!(reply_subject(" "), "Re: ");
        assert_eq!(reply_subject("Отчёт"), "Re: Отчёт");
    }
}
