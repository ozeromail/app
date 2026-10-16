//! `MessageParser` on mail-parser: decodes headers in any charset.

use mail_parser::PartType;
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
            from: Address {
                name: from.and_then(|a| a.name()).map(str::to_owned),
                email: email.to_owned(),
            },
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
        assert_eq!(m.date, 100);
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
