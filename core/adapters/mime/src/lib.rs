//! `MessageParser` on mail-parser: decodes headers in any charset.

mod build;

pub use build::MimeBuilder;

use mail_parser::{Address as Addresses, Message, PartType};
use ozero_domain::{Address, Body, Category, MessageParser, NewMessage, RawMessage, Signals};

#[derive(Debug, Default)]
pub struct MimeParser(mail_parser::MessageParser);

impl MessageParser for MimeParser {
    fn parse(&self, raw: RawMessage) -> NewMessage {
        let parsed = self.0.parse_headers(&raw.header);
        let header = parsed.as_ref();
        let from = header.and_then(|h| h.from()).and_then(|a| a.first());
        let email = from.and_then(|a| a.address()).unwrap_or_default();
        let raw_header = |name: &'static str| header.and_then(|h| h.header_raw(name));
        let category = Category::of(&Signals {
            from: email,
            list_id: raw_header("List-Id"),
            list_unsubscribe: raw_header("List-Unsubscribe"),
            list_unsubscribe_post: raw_header("List-Unsubscribe-Post"),
            precedence: raw_header("Precedence"),
            auto_submitted: raw_header("Auto-Submitted"),
        });
        NewMessage {
            remote_id: raw.remote_id,
            message_id: header.and_then(|h| h.message_id()).map(str::to_owned),
            references: header.map(references).unwrap_or_default(),
            from: Address {
                name: from.and_then(|a| a.name()).map(str::to_owned),
                email: email.to_owned(),
            },
            answer_to: emails([header.and_then(|h| h.reply_to())]),
            recipients: emails([header.and_then(|h| h.to()), header.and_then(|h| h.cc())]),
            subject: header
                .and_then(|h| h.subject())
                .unwrap_or_default()
                .to_owned(),
            snippet: String::new(),
            date: raw.received,
            flags: raw.flags,
            category,
        }
    }

    fn parse_body(&self, raw: &[u8]) -> Body {
        let Some(message) = self.0.parse(raw) else {
            return Body::default();
        };
        // `body_html` would turn a plain text part into HTML; only a real HTML part counts.
        let html = message.html_part(0).and_then(|p| match &p.body {
            PartType::Html(html) => Some(html.to_string()),
            _ => None,
        });
        Body {
            text: message
                .body_text(0)
                .map(|t| t.trim_end().to_owned())
                .unwrap_or_default(),
            html,
        }
    }
}

/// The bare addresses of address headers, in order.
fn emails<const N: usize>(headers: [Option<&Addresses<'_>>; N]) -> Vec<String> {
    headers
        .into_iter()
        .flatten()
        .flat_map(|h| h.iter())
        .filter_map(|a| a.address())
        .map(str::to_owned)
        .collect()
}

/// How many `Message-ID`s of a long `References` chain are kept: the root and the newest.
const MAX_REFERENCES: usize = 20;

/// The `References` chain, then `In-Reply-To` if it is not already its end; no duplicates.
fn references(header: &Message<'_>) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let chain = header.references().as_text_list().unwrap_or_default();
    let answers = header.in_reply_to().as_text_list().unwrap_or_default();
    for id in chain.iter().chain(answers) {
        if !id.is_empty() && !ids.iter().any(|known| known == id) {
            ids.push(id.to_string());
        }
    }
    if ids.len() > MAX_REFERENCES {
        ids.drain(1..ids.len() - (MAX_REFERENCES - 1));
    }
    ids
}

#[cfg(test)]
mod tests {
    use ozero_domain::Flags;

    use super::*;

    fn parse(header: &[u8]) -> NewMessage {
        MimeParser::default().parse(RawMessage {
            remote_id: "1".into(),
            flags: Flags::default(),
            received: 100,
            header: header.to_vec(),
        })
    }

    #[test]
    fn decodes_encoded_words() {
        let m = parse(
            b"From: =?UTF-8?B?0JDQvdC90LA=?= <anna@example.com>\r\n\
              Subject: =?UTF-8?Q?=D0=9F=D1=80=D0=B8=D0=B2=D0=B5=D1=82?=\r\n\
              Message-ID: <1@example.com>\r\n\r\n",
        );
        assert_eq!(m.from.name.as_deref(), Some("Анна"));
        assert_eq!(m.from.email, "anna@example.com");
        assert_eq!(m.subject, "Привет");
        assert_eq!(m.message_id.as_deref(), Some("1@example.com"));
    }

    #[test]
    fn reads_who_else_it_went_to() {
        let m = parse(
            b"From: Anna <anna@example.com>\r\n\
              Reply-To: list@example.com\r\n\
              To: me@example.com, Team: bob@example.com, cat@example.com;\r\n\
              Cc: Dan <dan@example.com>\r\n\r\n",
        );
        assert_eq!(m.answer_to, ["list@example.com"]);
        assert_eq!(
            m.recipients,
            [
                "me@example.com",
                "bob@example.com",
                "cat@example.com",
                "dan@example.com"
            ]
        );
        assert_eq!(m.date, 100);
    }

    #[test]
    fn keeps_the_references_once() {
        let m = parse(
            b"From: anna@example.com\r\n\
              References: <1@example.com>\r\n <2@example.com>\r\n\
              In-Reply-To: <2@example.com>\r\n\r\n",
        );
        assert_eq!(m.references, ["1@example.com", "2@example.com"]);

        let chain: String = (1..=30).map(|i| format!("<{i}@x> ")).collect();
        let m = parse(format!("References: {chain}\r\nIn-Reply-To: <31@x>\r\n\r\n").as_bytes());
        assert_eq!(m.references.len(), MAX_REFERENCES);
        assert_eq!(m.references[0], "1@x");
        assert_eq!(m.references.last().map(String::as_str), Some("31@x"));
    }

    #[test]
    fn sorts_by_headers() {
        let category = |header: &[u8]| parse(header).category;
        assert_eq!(
            category(b"From: anna@example.com\r\n\r\n"),
            Category::People
        );
        assert_eq!(
            category(
                b"From: news@shop.example\r\nList-Unsubscribe: <mailto:u@shop.example>\r\n\r\n"
            ),
            Category::Newsletters
        );
        assert_eq!(
            category(b"From: it@example.com\r\nAuto-Submitted: auto-generated\r\n\r\n"),
            Category::Notifications
        );
        assert_eq!(
            category(b"From: digest@blog.example\r\nLIST-ID: <digest.blog.example>\r\nPrecedence: Bulk\r\n\r\n"),
            Category::Newsletters
        );
    }

    #[test]
    fn decodes_legacy_cyrillic() {
        // "Отчёт" in KOI8-R.
        let m = parse(b"Subject: =?KOI8-R?B?79Teo9Q=?=\r\n\r\n");
        assert_eq!(m.subject, "Отчёт");
    }

    #[test]
    fn reads_the_text_part() {
        let body = MimeParser::default().parse_body(
            b"Content-Type: multipart/alternative; boundary=b\r\n\r\n\
              --b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n\
              \xd0\x9f\xd1\x80\xd0\xb8\xd0\xb2\xd0\xb5\xd1\x82\r\n\
              --b\r\nContent-Type: text/html\r\n\r\n<p>Hello</p>\r\n--b--\r\n",
        );
        assert_eq!(body.text, "Привет");
        assert_eq!(body.html.as_deref(), Some("<p>Hello</p>"));
    }

    #[test]
    fn turns_html_only_into_text() {
        let body = MimeParser::default()
            .parse_body(b"Content-Type: text/html\r\n\r\n<p>Hello <b>there</b></p>");
        assert_eq!(body.text, "Hello there");
        assert_eq!(body.html.as_deref(), Some("<p>Hello <b>there</b></p>"));
    }

    #[test]
    fn plain_text_has_no_html() {
        let body = MimeParser::default().parse_body(b"Subject: Hi\r\n\r\nJust text");
        assert_eq!(body.text, "Just text");
        assert_eq!(body.html, None);
    }

    #[test]
    fn survives_a_cut_message() {
        let body = MimeParser::default().parse_body(
            b"Content-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\n\r\nPart one\r\n--b\r\nContent-Type: app",
        );
        assert_eq!(body.text, "Part one");
    }

    #[test]
    fn survives_garbage() {
        assert_eq!(MimeParser::default().parse_body(b"\xff\xfe").text, "");
        let m = parse(b"\xff\xfe not a header");
        assert_eq!(m.remote_id, "1");
        assert_eq!(m.from.email, "");
    }
}
