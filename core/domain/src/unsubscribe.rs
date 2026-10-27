use crate::percent_decode;

/// How a message's sender lets you leave its list: its `List-Unsubscribe` header as sent
/// (RFC 2369), and whether `List-Unsubscribe-Post` asks for a one-click POST (RFC 8058).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsubscribe {
    pub links: String,
    pub one_click: bool,
}

/// What leaving takes, best first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leave {
    /// A POST to this `https` address does it, with nothing for the person to do.
    Post(String),
    /// A message to `to` does it.
    Mail {
        to: String,
        subject: String,
        body: String,
    },
    /// The person does it on this `https` page.
    Page(String),
}

impl Unsubscribe {
    /// From the headers as sent; None without a link.
    pub fn new(links: Option<&str>, post: Option<&str>) -> Option<Self> {
        let links = links?.trim();
        let one_click =
            post.is_some_and(|p| p.trim().eq_ignore_ascii_case("List-Unsubscribe=One-Click"));
        (!links.is_empty()).then(|| Self {
            links: links.to_owned(),
            one_click,
        })
    }

    /// The way to leave: a one-click POST, else a message, else a page; None if no link is
    /// one the app can use.
    pub fn leave(&self) -> Option<Leave> {
        let (mut https, mut mailto) = (None, None);
        for link in self.links.split(',') {
            let Some(link) = link
                .trim()
                .strip_prefix('<')
                .and_then(|l| l.strip_suffix('>'))
            else {
                continue;
            };
            let link = link.trim();
            if https.is_none() && is_scheme(link, "https:") {
                https = Some(link);
            } else if mailto.is_none() {
                mailto = mail(link);
            }
        }
        match (https, mailto) {
            (Some(url), _) if self.one_click => Some(Leave::Post(url.to_owned())),
            (_, Some(mail)) => Some(mail),
            (url, None) => url.map(|url| Leave::Page(url.to_owned())),
        }
    }
}

pub(crate) fn is_scheme(link: &str, scheme: &str) -> bool {
    link.get(..scheme.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(scheme))
}

/// A `mailto:` link (RFC 6068) as a message; "unsubscribe" for a subject it does not give.
fn mail(link: &str) -> Option<Leave> {
    if !is_scheme(link, "mailto:") {
        return None;
    }
    let (to, query) = link[7..].split_once('?').unwrap_or((&link[7..], ""));
    let to = percent_decode(to);
    if !crate::is_address(&to) {
        return None;
    }
    let (mut subject, mut body) = (String::new(), String::new());
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key.eq_ignore_ascii_case("subject") {
            subject = percent_decode(value);
        } else if key.eq_ignore_ascii_case("body") {
            body = percent_decode(value);
        }
    }
    if subject.trim().is_empty() {
        subject = "unsubscribe".to_owned();
    }
    Some(Leave::Mail { to, subject, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leave(links: &str, post: Option<&str>) -> Option<Leave> {
        Unsubscribe::new(Some(links), post)?.leave()
    }

    #[test]
    fn one_click_goes_first() {
        let links = "<mailto:u@shop.example>, <https://shop.example/u?id=1>";
        assert_eq!(
            leave(links, Some("List-Unsubscribe=One-Click")),
            Some(Leave::Post("https://shop.example/u?id=1".into()))
        );
        // Without the POST header the page needs the person; a message does not.
        assert_eq!(
            leave(links, None),
            Some(Leave::Mail {
                to: "u@shop.example".into(),
                subject: "unsubscribe".into(),
                body: String::new(),
            })
        );
    }

    #[test]
    fn a_mailto_link_gives_its_subject_and_body() {
        assert_eq!(
            leave(
                "<mailto:list@example.com?subject=Leave%20me&body=bye>",
                None
            ),
            Some(Leave::Mail {
                to: "list@example.com".into(),
                subject: "Leave me".into(),
                body: "bye".into(),
            })
        );
    }

    #[test]
    fn a_page_is_the_last_way() {
        assert_eq!(
            leave("<https://news.example/leave>", None),
            Some(Leave::Page("https://news.example/leave".into()))
        );
    }

    #[test]
    fn links_the_app_cannot_use_are_none() {
        assert_eq!(
            leave(
                "<http://plain.example/u>",
                Some("List-Unsubscribe=One-Click")
            ),
            None
        );
        assert_eq!(leave("<mailto:not-an-address>", None), None);
        assert_eq!(Unsubscribe::new(Some("  "), None), None);
    }
}
