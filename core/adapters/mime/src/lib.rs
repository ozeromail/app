//! `MessageParser` on mail-parser: decodes headers in any charset.

use ozero_domain::{Address, MessageParser, NewMessage, RawMessage};

#[derive(Debug, Default)]
pub struct MimeParser(mail_parser::MessageParser);

impl MessageParser for MimeParser {
    fn parse(&self, raw: RawMessage) -> NewMessage {
        let parsed = self.0.parse_headers(&raw.header);
        let header = parsed.as_ref();
        let from = header.and_then(|h| h.from()).and_then(|a| a.first());
        NewMessage {
            remote_id: raw.remote_id,
            message_id: header.and_then(|h| h.message_id()).map(str::to_owned),
            from: Address {
                name: from.and_then(|a| a.name()).map(str::to_owned),
                email: from
                    .and_then(|a| a.address())
                    .unwrap_or_default()
                    .to_owned(),
            },
            subject: header
                .and_then(|h| h.subject())
                .unwrap_or_default()
                .to_owned(),
            snippet: String::new(),
            date: raw.received,
            flags: raw.flags,
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
    fn decodes_legacy_cyrillic() {
        // "Отчёт" in KOI8-R.
        let m = parse(b"Subject: =?KOI8-R?B?79Teo9Q=?=\r\n\r\n");
        assert_eq!(m.subject, "Отчёт");
    }

    #[test]
    fn survives_garbage() {
        let m = parse(b"\xff\xfe not a header");
        assert_eq!(m.remote_id, "1");
        assert_eq!(m.from.email, "");
    }
}
