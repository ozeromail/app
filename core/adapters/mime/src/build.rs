//! `MessageBuilder`: plain text messages, with files if any, written by hand. All output is
//! 7-bit ASCII with CRLF.

use ozero_domain::{Address, MessageBuilder, Outgoing, OutgoingFile, base64};

#[derive(Debug, Default)]
pub struct MimeBuilder;

impl MessageBuilder for MimeBuilder {
    fn build(&self, from: &Address, message: &Outgoing) -> Vec<u8> {
        let mut out = String::new();
        header(&mut out, "Date", &rfc5322_date(message.date));
        header(&mut out, "From", &mailbox(from));
        // One address per line keeps each line short (RFC 5322 folding).
        header(&mut out, "To", &message.to.join(",\r\n "));
        if !message.cc.is_empty() {
            header(&mut out, "Cc", &message.cc.join(",\r\n "));
        }
        header(&mut out, "Subject", &encoded_words(&message.subject));
        header(&mut out, "Message-ID", &format!("<{}>", message.message_id));
        if let Some(parent) = &message.in_reply_to {
            let parent = format!("<{}>", one_line(parent.trim_matches(['<', '>'])));
            header(&mut out, "In-Reply-To", &parent);
            header(&mut out, "References", &parent);
        }
        header(&mut out, "MIME-Version", "1.0");
        if message.files.is_empty() {
            text(&mut out, &message.text);
            return out.into_bytes();
        }
        // Unique as the message id, and never in base64, which has no `=_`.
        let local = message.message_id.split('@').next().unwrap_or_default();
        let boundary = format!("=_{}", one_line(local).replace('"', ""));
        let content = format!("multipart/mixed; boundary=\"{boundary}\"");
        header(&mut out, "Content-Type", &content);
        out.push_str("\r\n");
        out.push_str(&format!("--{boundary}\r\n"));
        text(&mut out, &message.text);
        for file in &message.files {
            out.push_str(&format!("--{boundary}\r\n"));
            attachment(&mut out, file);
        }
        out.push_str(&format!("--{boundary}--\r\n"));
        out.into_bytes()
    }
}

fn header(out: &mut String, name: &str, value: &str) {
    out.push_str(name);
    out.push_str(": ");
    out.push_str(value);
    out.push_str("\r\n");
}

/// The text part: its headers, then the text.
fn text(out: &mut String, text: &str) {
    header(out, "Content-Type", "text/plain; charset=utf-8");
    header(out, "Content-Transfer-Encoding", "base64");
    out.push_str("\r\n");
    lines(out, crlf(text).as_bytes());
}

/// A file part: its headers, then the file.
fn attachment(out: &mut String, file: &OutgoingFile) {
    let mime = one_line(&file.mime).replace([';', '"', ' '], "");
    let mime = if mime.contains('/') {
        mime
    } else {
        "application/octet-stream".into()
    };
    // Mail apps read encoded words in a quoted name, though RFC 2047 does not ask them to.
    let name = encoded_words(&file.name.replace(['"', '\\'], ""));
    // Text comes out of a message as UTF-8, whatever its charset there.
    let charset = if mime.starts_with("text/") {
        "; charset=utf-8"
    } else {
        ""
    };
    header(
        out,
        "Content-Type",
        &format!("{mime}{charset}; name=\"{name}\""),
    );
    header(
        out,
        "Content-Disposition",
        &format!("attachment; filename=\"{name}\""),
    );
    header(out, "Content-Transfer-Encoding", "base64");
    out.push_str("\r\n");
    lines(out, &file.data);
}

/// `data` in base64, in lines of at most 76 characters (RFC 2045); base64 is ASCII, so bytes
/// are characters.
fn lines(out: &mut String, data: &[u8]) {
    for line in base64(data).as_bytes().chunks(76) {
        out.push_str(&String::from_utf8_lossy(line));
        out.push_str("\r\n");
    }
}

/// `Name <address>`, or the bare address without a name.
fn mailbox(address: &Address) -> String {
    match address.name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => format!("{} <{}>", phrase(name), address.email),
        _ => address.email.clone(),
    }
}

/// A display name: as is when it is plain ASCII words, else as encoded words.
fn phrase(name: &str) -> String {
    let plain = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '.' | '\'' | '_'));
    if plain {
        name.to_owned()
    } else {
        encoded_words(name)
    }
}

/// RFC 2047 base64 words for text that is not plain ASCII; each word stays under 76 characters.
fn encoded_words(text: &str) -> String {
    let text = one_line(text);
    if text.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        return text;
    }
    // 45 bytes encode to 60 characters, plus 12 for `=?UTF-8?B??=`.
    let mut words = Vec::new();
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if i + c.len_utf8() - start > 45 {
            words.push(&text[start..i]);
            start = i;
        }
    }
    words.push(&text[start..]);
    words
        .iter()
        .map(|w| format!("=?UTF-8?B?{}?=", base64(w.as_bytes())))
        .collect::<Vec<_>>()
        .join("\r\n ")
}

/// Line breaks would end the header and start another; a space keeps the words apart.
fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

/// Every line break as CRLF.
fn crlf(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 32);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push_str("\r\n");
            }
            '\n' => out.push_str("\r\n"),
            c => out.push(c),
        }
    }
    out
}

/// `Thu, 16 Oct 2026 09:48:03 +0000` for seconds since the Unix epoch, in UTC.
fn rfc5322_date(seconds: i64) -> String {
    mail_parser::DateTime::from_timestamp(seconds).to_rfc822()
}

#[cfg(test)]
mod tests {
    use mail_parser::{MessageParser, MimeHeaders};
    use ozero_domain::OutboxId;

    use super::*;

    fn outgoing(subject: &str, text: &str) -> Outgoing {
        Outgoing {
            id: OutboxId(1),
            to: vec!["bob@example.org".into(), "cat@example.net".into()],
            cc: vec!["dan@example.com".into()],
            subject: subject.into(),
            text: text.into(),
            message_id: "1.2@example.com".into(),
            in_reply_to: Some("parent@example.org".into()),
            date: 1_791_971_283,
            forward: None,
            files: vec![],
        }
    }

    #[test]
    fn writes_dates() {
        assert_eq!(rfc5322_date(0), "Thu, 1 Jan 1970 00:00:00 +0000");
        assert_eq!(
            rfc5322_date(1_791_971_283),
            "Wed, 14 Oct 2026 09:48:03 +0000"
        );
        assert_eq!(rfc5322_date(951_782_400), "Tue, 29 Feb 2000 00:00:00 +0000");
    }

    #[test]
    fn builds_what_a_parser_reads_back() {
        let from = Address {
            name: Some("Анна Иванова".into()),
            email: "anna@example.com".into(),
        };
        let subject = "Отчёт за октябрь: длинная тема, которая не влезет в одно слово";
        let raw = MimeBuilder.build(&from, &outgoing(subject, "Привет!\n\n.точка\r\nКонец"));
        assert!(raw.is_ascii());
        assert!(
            raw.windows(2)
                .filter(|w| w[1] == b'\n')
                .all(|w| w[0] == b'\r')
        );

        let parsed = MessageParser::default().parse(&raw).unwrap();
        assert_eq!(parsed.subject(), Some(subject));
        let sender = parsed.from().and_then(|a| a.first()).unwrap();
        assert_eq!(sender.name(), Some("Анна Иванова"));
        assert_eq!(sender.address(), Some("anna@example.com"));
        assert_eq!(parsed.to().map(|a| a.iter().count()), Some(2));
        let cc = parsed
            .cc()
            .and_then(|a| a.first())
            .and_then(|a| a.address());
        assert_eq!(cc, Some("dan@example.com"));
        assert_eq!(parsed.message_id(), Some("1.2@example.com"));
        assert_eq!(parsed.in_reply_to().as_text(), Some("parent@example.org"));
        assert_eq!(
            parsed.body_text(0).as_deref(),
            Some("Привет!\r\n\r\n.точка\r\nКонец")
        );
    }

    #[test]
    fn a_line_break_cannot_add_a_header() {
        let from = Address {
            name: None,
            email: "anna@example.com".into(),
        };
        let raw = MimeBuilder.build(&from, &outgoing("Hi\r\nBcc: eve@example.com", ""));
        let text = String::from_utf8(raw).unwrap();
        assert!(text.contains("Subject: Hi  Bcc: eve@example.com\r\n"));
        assert!(!text.contains("\r\nBcc:"));
    }

    #[test]
    fn sends_files_after_the_text() {
        let from = Address {
            name: None,
            email: "anna@example.com".into(),
        };
        let pdf: Vec<u8> = (0..=255).cycle().take(1000).collect();
        let message = Outgoing {
            files: vec![
                OutgoingFile {
                    name: "Счёт \"октябрь\".pdf".into(),
                    mime: "application/pdf".into(),
                    data: pdf.clone(),
                },
                OutgoingFile {
                    name: "notes.txt".into(),
                    mime: String::new(),
                    data: b"hi".to_vec(),
                },
            ],
            ..outgoing("Fwd: bill", "See below")
        };
        let raw = MimeBuilder.build(&from, &message);
        assert!(raw.is_ascii());
        let parsed = MessageParser::default().parse(&raw).unwrap();
        assert_eq!(parsed.body_text(0).as_deref(), Some("See below"));
        let files: Vec<_> = parsed.attachments().collect();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].attachment_name(), Some("Счёт октябрь.pdf"));
        assert_eq!(files[0].contents(), pdf);
        assert_eq!(files[1].attachment_name(), Some("notes.txt"));
        assert_eq!(files[1].contents(), b"hi");
    }
}
