//! `MessageParser` on mail-parser: decodes headers in any charset.

mod build;

use std::collections::HashSet;

pub use build::MimeBuilder;

use mail_parser::{Address as Addresses, Message, MessagePart, MimeHeaders, PartType};
use ozero_domain::{
    Address, Attachment, Body, Category, Invite, InviteKind, MessageParser, NewMessage, RawMessage,
    Signals, Unsubscribe, percent_decode,
};

#[derive(Debug, Default)]
pub struct MimeParser(mail_parser::MessageParser);

impl MessageParser for MimeParser {
    fn parse(&self, raw: RawMessage) -> NewMessage {
        let parsed = self.0.parse_headers(&raw.header);
        let header = parsed.as_ref();
        let from = header.and_then(|h| h.from()).and_then(|a| a.first());
        let email = from.and_then(|a| a.address()).unwrap_or_default();
        let raw_header = |name: &'static str| header.and_then(|h| h.header_raw(name));
        let signals = Signals {
            from: email,
            list_id: raw_header("List-Id"),
            list_unsubscribe: raw_header("List-Unsubscribe"),
            list_unsubscribe_post: raw_header("List-Unsubscribe-Post"),
            precedence: raw_header("Precedence"),
            auto_submitted: raw_header("Auto-Submitted"),
        };
        NewMessage {
            remote_id: raw.remote_id,
            message_id: header.and_then(|h| h.message_id()).map(str::to_owned),
            references: header.map(references).unwrap_or_default(),
            from: Address {
                name: from.and_then(|a| a.name()).map(str::to_owned),
                email: email.to_owned(),
            },
            answer_to: addresses(header.and_then(|h| h.reply_to()))
                .into_iter()
                .map(|a| a.email)
                .collect(),
            to: addresses(header.and_then(|h| h.to())),
            cc: addresses(header.and_then(|h| h.cc())),
            subject: header
                .and_then(|h| h.subject())
                .unwrap_or_default()
                .to_owned(),
            snippet: String::new(),
            date: raw.received,
            flags: raw.flags,
            category: Category::of(&signals),
            unsubscribe: Unsubscribe::new(signals.list_unsubscribe, signals.list_unsubscribe_post),
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
        let calendar = calendar(&message);
        let mut files = attachments(&message, html.as_deref().unwrap_or_default());
        if let Some((parts, _)) = &calendar {
            // The card shows the invitation; its files would only repeat it.
            files.retain(|f| !parts.contains(&f.part));
        }
        let text = message
            .body_text(0)
            .map(|t| t.trim_end().to_owned())
            .unwrap_or_default();
        Body {
            // A message that is only an invitation has no text but the calendar's own.
            text: if text.starts_with("BEGIN:VCALENDAR") {
                String::new()
            } else {
                text
            },
            attachments: files,
            html,
            calendar: calendar.map(|(_, text)| text),
        }
    }

    fn attachments(&self, raw: &[u8], parts: &[u32]) -> Vec<Option<Vec<u8>>> {
        let message = self.0.parse(raw);
        let contents = |part: &u32| {
            let message = message.as_ref()?;
            if !message.attachments.contains(part) {
                return None;
            }
            message.part(*part).map(|p| p.contents().to_vec())
        };
        parts.iter().map(contents).collect()
    }
}

/// The message's attachments but the images its HTML shows by `cid:`.
fn attachments(message: &Message<'_>, html: &str) -> Vec<Attachment> {
    if message.attachments.is_empty() {
        return vec![];
    }
    // `cid:` URLs are case-blind and may be percent-encoded (RFC 2392).
    let html = html.to_ascii_lowercase();
    let shown: HashSet<String> = html
        .match_indices("cid:")
        .map(|(at, _)| {
            let id = &html[at + 4..];
            let end = id.find(['"', '\'', ')', ' ', '>']).unwrap_or(id.len());
            percent_decode(&id[..end])
        })
        .collect();
    message
        .attachments
        .iter()
        .filter_map(|&n| Some((n, message.part(n)?)))
        .filter(|(_, p)| {
            let id = p
                .content_id()
                .map(|id| id.trim_matches(['<', '>']).to_ascii_lowercase());
            !id.is_some_and(|id| shown.contains(&id))
        })
        .map(|(part, p)| Attachment {
            part,
            name: name(p),
            mime: p.content_type().map_or_else(
                || "application/octet-stream".to_owned(),
                |t| match t.subtype() {
                    Some(sub) => format!("{}/{sub}", t.ctype()).to_ascii_lowercase(),
                    None => t.ctype().to_ascii_lowercase(),
                },
            ),
            size: u64::try_from(p.len()).unwrap_or(u64::MAX),
        })
        .collect()
}

/// The invitation or cancellation a message carries, and the parts that hold it: its
/// `text/calendar` part, else an `.ics` file. Any other calendar, an answer or an event to
/// save, stays a file.
fn calendar(message: &Message<'_>) -> Option<(Vec<u32>, String)> {
    let found: Vec<(u32, bool, String, Invite)> = message
        .parts
        .iter()
        .enumerate()
        .filter_map(|(part, p)| {
            let mime = p.content_type()?;
            let mime = format!("{}/{}", mime.ctype(), mime.subtype().unwrap_or_default());
            if !is_calendar(&mime, &name(p)) {
                return None;
            }
            let text = String::from_utf8_lossy(p.contents()).into_owned();
            let invite = Invite::parse(&text).filter(|i| i.kind != InviteKind::Other)?;
            let inline = mime.eq_ignore_ascii_case("text/calendar");
            Some((u32::try_from(part).ok()?, inline, text, invite))
        })
        .collect();
    let (_, _, text, chosen) = found
        .iter()
        .find(|(_, inline, ..)| *inline)
        .or_else(|| found.first())?;
    // Outlook sends the invitation twice: as the message's calendar and as a file.
    let parts = found
        .iter()
        .filter(|(.., i)| i.uid == chosen.uid)
        .map(|(part, ..)| *part)
        .collect();
    Some((parts, text.clone()))
}

/// An iCalendar part or file, by its type or name.
fn is_calendar(mime: &str, name: &str) -> bool {
    let mime = mime.to_ascii_lowercase();
    mime == "text/calendar"
        || mime == "application/ics"
        || name.to_ascii_lowercase().ends_with(".ics")
}

/// The file name as sent, without any folders or control characters; a forwarded message
/// is named by its subject.
fn name(part: &MessagePart<'_>) -> String {
    let name = match part.attachment_name() {
        Some(name) => name.rsplit(['/', '\\']).next().unwrap_or(name).to_owned(),
        None => match part.message().and_then(Message::subject).map(str::trim) {
            Some(subject) if !subject.is_empty() => {
                format!("{}.eml", subject.replace(['/', '\\'], "-"))
            }
            _ => String::new(),
        },
    };
    name.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Each address of a header, groups opened, with its name as sent.
fn addresses(header: Option<&Addresses<'_>>) -> Vec<Address> {
    header
        .into_iter()
        .flat_map(|h| h.iter())
        .filter_map(|a| {
            Some(Address {
                name: a.name().map(str::to_owned),
                email: a.address()?.to_owned(),
            })
        })
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

    const EVENT: &str = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:1\r\n\
DTSTART:20261029T080000Z\r\nSUMMARY:Sprint\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn finds_the_invitation_and_hides_its_file() {
        let raw = format!(
            "From: anna@example.com\r\nContent-Type: multipart/mixed; boundary=m\r\n\r\n\
--m\r\nContent-Type: multipart/alternative; boundary=a\r\n\r\n\
--a\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSprint planning\r\n\
--a\r\nContent-Type: text/calendar; charset=utf-8; method=REQUEST\r\n\r\n{EVENT}\
--a--\r\n\
--m\r\nContent-Type: application/ics; name=invite.ics\r\n\
Content-Disposition: attachment; filename=invite.ics\r\n\r\n{EVENT}\
--m\r\nContent-Type: application/pdf; name=agenda.pdf\r\n\
Content-Disposition: attachment; filename=agenda.pdf\r\n\r\nPDF\r\n\
--m--\r\n"
        );
        let body = MimeParser::default().parse_body(raw.as_bytes());
        assert_eq!(body.text, "Sprint planning");
        assert_eq!(
            body.calendar.as_deref().map(str::trim_end),
            Some(EVENT.trim_end())
        );
        let names: Vec<_> = body.attachments.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["agenda.pdf"]);

        let alone = format!(
            "From: anna@example.com\r\nContent-Type: text/calendar; method=CANCEL\r\n\r\n{EVENT}"
        );
        let body = MimeParser::default().parse_body(alone.as_bytes());
        assert_eq!(body.text, "");
        assert!(body.calendar.is_some());
    }

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
        let emails = |a: &[Address]| a.iter().map(|a| a.email.clone()).collect::<Vec<_>>();
        assert_eq!(
            emails(&m.to),
            ["me@example.com", "bob@example.com", "cat@example.com"]
        );
        assert_eq!(emails(&m.cc), ["dan@example.com"]);
        assert_eq!(m.cc[0].name.as_deref(), Some("Dan"));
        assert_eq!(m.to[0].name, None);
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

    const MIXED: &[u8] = b"Content-Type: multipart/mixed; boundary=m\r\n\r\n\
        --m\r\nContent-Type: multipart/related; boundary=r\r\n\r\n\
        --r\r\nContent-Type: text/html\r\n\r\n<p>Hi</p><img src=\"CID:Logo%40x\">\r\n\
        --r\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\n\
        Content-Transfer-Encoding: base64\r\n\r\naGk=\r\n--r--\r\n\
        --m\r\nContent-Type: application/PDF; name=\"=?UTF-8?B?0KHQv9GA0LDQstC60LAucGRm?=\"\r\n\
        Content-Disposition: attachment; filename=\"C:\\\\docs\\\\x.pdf\"\r\n\
        Content-Transfer-Encoding: base64\r\n\r\nJVBERi0=\r\n\
        --m\r\nContent-Type: image/jpeg\r\nContent-ID: <photo@x>\r\n\
        Content-Disposition: inline\r\nContent-Transfer-Encoding: base64\r\n\r\n/9j/\r\n\
        --m\r\nContent-Type: message/rfc822\r\n\r\nSubject: Old/plan\r\n\r\nText\r\n\
        --m--\r\n";

    #[test]
    fn lists_attachments_but_images_the_page_shows() {
        let body = MimeParser::default().parse_body(MIXED);
        let files: Vec<_> = body
            .attachments
            .iter()
            .map(|a| (a.name.as_str(), a.mime.as_str(), a.size))
            .collect();
        assert_eq!(
            files,
            [
                ("x.pdf", "application/pdf", 5),
                ("", "image/jpeg", 3),
                ("Old-plan.eml", "message/rfc822", 25),
            ]
        );
    }

    #[test]
    fn decodes_attachments() {
        let parser = MimeParser::default();
        let body = parser.parse_body(MIXED);
        let pdf = body.attachments[0].part;
        // Only attachments: not the HTML part, not a part past the end.
        assert_eq!(
            parser.attachments(MIXED, &[pdf, 2, 99]),
            [Some(b"%PDF-".to_vec()), None, None]
        );
    }

    #[test]
    fn survives_garbage() {
        assert_eq!(MimeParser::default().parse_body(b"\xff\xfe").text, "");
        let m = parse(b"\xff\xfe not a header");
        assert_eq!(m.remote_id, "1");
        assert_eq!(m.from.email, "");
    }
}
