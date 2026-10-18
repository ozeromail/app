//! A light pass over a message's HTML before the reading screen shows it. The web view's
//! sandbox is the real guard; this takes out what has no business in a message and holds
//! back remote images, so the sender cannot tell the message was opened.

/// A message's HTML ready to show, and the remote images it holds back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Each held back image carries `data-ozero-image="N"` instead of its source.
    pub html: String,
    /// The `https` addresses of the held back images, by `N`; each address once.
    pub images: Vec<String>,
}

/// Elements dropped with all they hold.
const DROPPED: [&str; 5] = ["script", "iframe", "object", "embed", "template"];
/// Tags dropped on their own: they would change how the page loads.
const VOID_DROPPED: [&str; 3] = ["base", "meta", "link"];
const IMAGE_MARK: &str = "data-ozero-image";
/// The mark where the sender wrote it, so only marks `clean` puts in load images. Read as
/// text or an attribute value it shows the same; as an attribute name it means nothing.
const FORGED_MARK: &str = "data-ozero-imag&#101;";

/// Drops scripts, frames and tracking pixels, and holds back remote images.
pub fn clean(html: &str) -> Page {
    let mut page = Page {
        html: String::with_capacity(html.len()),
        images: Vec::new(),
    };
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        push(&mut page.html, &rest[..lt]);
        rest = &rest[lt..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        let Some(tag) = Tag::read(rest) else {
            page.html.push('<');
            rest = &rest[1..];
            continue;
        };
        rest = &rest[tag.len..];
        let name = tag.name.as_str();
        if DROPPED.contains(&name) {
            if !tag.closing {
                rest = after_end_tag(rest, name);
            }
        } else if name == "img" && !tag.closing {
            image(&tag, &mut page);
        } else if !VOID_DROPPED.contains(&name) {
            push(&mut page.html, tag.raw);
        }
    }
    push(&mut page.html, rest);
    page
}

/// Writes what the sender wrote, with any image mark in it made harmless.
fn push(out: &mut String, sent: &str) {
    if sent.contains(IMAGE_MARK) {
        out.push_str(&sent.replace(IMAGE_MARK, FORGED_MARK));
    } else {
        out.push_str(sent);
    }
}

/// Puts the loaded images in: `images[N]` is the source for image `N`, or None to
/// leave it out.
pub fn with_images(html: &str, images: &[Option<String>]) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mark = format!("{IMAGE_MARK}=\"");
    while let Some(at) = rest.find(&mark) {
        let value = &rest[at + mark.len()..];
        let Some(end) = value.find('"') else {
            break;
        };
        out.push_str(&rest[..at]);
        let source = value[..end]
            .parse::<usize>()
            .ok()
            .and_then(|n| images.get(n))
            .and_then(Option::as_deref);
        if let Some(source) = source {
            out.push_str("src=\"");
            out.push_str(&source.replace('"', "%22"));
            out.push('"');
        }
        rest = &value[end + 1..];
    }
    out.push_str(rest);
    out
}

/// One start or end tag.
struct Tag<'a> {
    raw: &'a str,
    /// Lowercase.
    name: String,
    closing: bool,
    /// Bytes from `<` to `>`, both included.
    len: usize,
    attrs: Vec<(String, Option<&'a str>)>,
}

impl<'a> Tag<'a> {
    /// The tag at the start of `html`, which starts with `<`; None if it is not one.
    fn read(html: &'a str) -> Option<Self> {
        let bytes = html.as_bytes();
        let mut at = 1;
        let closing = bytes.get(at) == Some(&b'/');
        if closing {
            at += 1;
        }
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_alphanumeric) {
            at += 1;
        }
        if at == start || !bytes[start].is_ascii_alphabetic() {
            return None;
        }
        let name = html[start..at].to_ascii_lowercase();
        let mut attrs = Vec::new();
        loop {
            while bytes
                .get(at)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == b'/')
            {
                at += 1;
            }
            match bytes.get(at) {
                None => return None,
                Some(b'>') => break,
                _ => {}
            }
            let start = at;
            while bytes
                .get(at)
                .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b'=' | b'>' | b'/'))
            {
                at += 1;
            }
            let attr = html[start..at].to_ascii_lowercase();
            if at == start {
                // A lone `=`: skip it.
                at += 1;
                continue;
            }
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            let mut value = None;
            if bytes.get(at) == Some(&b'=') {
                at += 1;
                while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                    at += 1;
                }
                match bytes.get(at) {
                    Some(q @ (b'"' | b'\'')) => {
                        let end = html[at + 1..].find(char::from(*q))? + at + 1;
                        value = Some(&html[at + 1..end]);
                        at = end + 1;
                    }
                    _ => {
                        let start = at;
                        while bytes
                            .get(at)
                            .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'>')
                        {
                            at += 1;
                        }
                        value = Some(&html[start..at]);
                    }
                }
            }
            attrs.push((attr, value));
        }
        Some(Self {
            raw: &html[..=at],
            name,
            closing,
            len: at + 1,
            attrs,
        })
    }

    fn attr(&self, name: &str) -> Option<&'a str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| *v)
    }
}

/// What follows the end tag of `name`, or nothing if it never ends.
fn after_end_tag<'a>(html: &'a str, name: &str) -> &'a str {
    let end = format!("</{name}");
    let at = html.match_indices("</").map(|(at, _)| at).find(|&at| {
        html.get(at..at + end.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(&end))
    });
    let close = at.and_then(|at| html[at..].find('>').map(|close| at + close + 1));
    close.map_or("", |close| &html[close..])
}

/// Writes an image: as is, without a pixel, or held back.
fn image(tag: &Tag<'_>, page: &mut Page) {
    if is_pixel(tag) {
        return;
    }
    let remote = tag.attr("src").and_then(remote_source);
    let Some(source) = remote else {
        push(&mut page.html, tag.raw);
        return;
    };
    page.html.push_str("<img");
    for (name, value) in &tag.attrs {
        if matches!(name.as_str(), "src" | "srcset" | IMAGE_MARK) {
            continue;
        }
        page.html.push(' ');
        push(&mut page.html, name);
        if let Some(value) = value {
            page.html.push_str("=\"");
            push(&mut page.html, &value.replace('"', "&quot;"));
            page.html.push('"');
        }
    }
    // A repeated image, such as a logo, loads once.
    let n = page
        .images
        .iter()
        .position(|s| *s == source)
        .unwrap_or_else(|| {
            page.images.push(source);
            page.images.len() - 1
        });
    page.html.push_str(&format!(" {IMAGE_MARK}=\"{n}\">"));
}

/// An image too small to see, or hidden: it is only there to report the message was opened.
/// A line, 600 by 1, is not one.
fn is_pixel(tag: &Tag<'_>) -> bool {
    let style = tag
        .attr("style")
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let hidden = ["display:none", "visibility:hidden"]
        .iter()
        .any(|s| style.contains(s));
    let size = |name: &str| {
        let rule = format!("{name}:");
        let styled = style.split(';').find_map(|r| r.strip_prefix(&rule));
        styled
            .or(tag.attr(name))
            .and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
    };
    let (width, height) = (size("width"), size("height"));
    let zero = |n: Option<f32>| n.is_some_and(|n| n <= 0.0);
    let tiny = |n: Option<f32>, other: Option<f32>| {
        n.is_some_and(|n| n <= 1.0) && other.is_none_or(|n| n <= 1.0)
    };
    hidden || zero(width) || zero(height) || tiny(width, height) || tiny(height, width)
}

/// The `https` address of a remote image source; `http` becomes `https`.
fn remote_source(source: &str) -> Option<String> {
    let source = source
        .trim()
        .replace("&amp;", "&")
        .replace("&#38;", "&")
        .replace("&quot;", "\"");
    let lower = source.to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &source[8..]
    } else if lower.starts_with("http://") {
        &source[7..]
    } else if lower.starts_with("//") {
        &source[2..]
    } else {
        return None;
    };
    (!rest.is_empty()).then(|| format!("https://{rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_back_remote_images() {
        let page = clean(
            r#"<p>Hi</p><IMG alt='a "b"' SRC="http://x.com/a.png?x=1&amp;y=2" srcset="b 2x"><img src="data:image/png;base64,AA">"#,
        );
        assert_eq!(page.images, ["https://x.com/a.png?x=1&y=2"]);
        assert_eq!(
            page.html,
            r#"<p>Hi</p><img alt="a &quot;b&quot;" data-ozero-image="0"><img src="data:image/png;base64,AA">"#
        );
        let shown = with_images(&page.html, &[Some("data:image/png;base64,BB".into())]);
        assert_eq!(
            shown,
            r#"<p>Hi</p><img alt="a &quot;b&quot;" src="data:image/png;base64,BB"><img src="data:image/png;base64,AA">"#
        );
        assert!(!with_images(&page.html, &[None]).contains("src=\"data:image/png;base64,BB"));
    }

    #[test]
    fn loads_only_its_own_marks() {
        let page =
            clean(r#"<p data-ozero-image="0">data-ozero-image="0"</p><img src=https://x/a>"#);
        let shown = with_images(&page.html, &[Some("data:image/png;base64,AA".into())]);
        assert_eq!(shown.matches("src=").count(), 1, "{shown}");
        assert_eq!(
            with_images("a data-ozero-image=\"0", &[]),
            "a data-ozero-image=\"0"
        );
    }

    #[test]
    fn keeps_lines_but_not_pixels() {
        for pixel in [
            "<img src=https://x/p width=1>",
            "<img src=https://x/p style='width:1px;height:1px'>",
            "<img src=https://x/p width=600 height=0>",
        ] {
            assert!(clean(pixel).html.is_empty(), "{pixel}");
        }
        let line = clean("<img src=https://x/l width=600 height=1>");
        assert_eq!(line.images.len(), 1);
    }

    #[test]
    fn loads_a_repeated_image_once() {
        let page = clean("<img src=https://x/l.png><SCRIPT>x</Script ><img src=https://x/l.png>");
        assert_eq!(page.images, ["https://x/l.png"]);
        assert_eq!(
            page.html,
            r#"<img data-ozero-image="0"><img data-ozero-image="0">"#
        );
    }

    #[test]
    fn drops_pixels_scripts_and_comments() {
        let page = clean(concat!(
            "<img src=https://t.co/p width=1 height=1>",
            "<img src='https://t.co/q' style='display: none'>",
            "<script>alert('<img src=https://x/>')</script>",
            "<!-- <img src=https://x/c> -->",
            "<base href=https://x/><meta http-equiv=refresh content='0;url=https://x'>",
            "<b>ok</b> 1 < 2",
        ));
        assert!(page.images.is_empty());
        assert_eq!(page.html, "<b>ok</b> 1 < 2");
    }

    #[test]
    fn keeps_what_it_cannot_read() {
        let html = "<p title=\"unclosed>text";
        assert_eq!(clean(html).html, html);
        assert_eq!(clean("Привет <i>мир</i>").html, "Привет <i>мир</i>");
    }
}
