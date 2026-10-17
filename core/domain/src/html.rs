//! A light pass over a message's HTML before the reading screen shows it. The web view's
//! sandbox is the real guard; this takes out what has no business in a message and holds
//! back remote images, so the sender cannot tell the message was opened.

/// A message's HTML ready to show, and the remote images it holds back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Each held back image carries `data-ozero-image="N"` instead of its source.
    pub html: String,
    /// The `https` addresses of the held back images, by `N`.
    pub images: Vec<String>,
}

/// Elements dropped with all they hold.
const DROPPED: [&str; 5] = ["script", "iframe", "object", "embed", "template"];
/// Tags dropped on their own: they would change how the page loads.
const VOID_DROPPED: [&str; 3] = ["base", "meta", "link"];
const IMAGE_MARK: &str = "data-ozero-image";

/// Drops scripts, frames and tracking pixels, and holds back remote images.
pub fn clean(html: &str) -> Page {
    let mut page = Page {
        html: String::with_capacity(html.len()),
        images: Vec::new(),
    };
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        page.html.push_str(&rest[..lt]);
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
        if !tag.closing && DROPPED.contains(&tag.name.as_str()) {
            rest = after_end_tag(rest, &tag.name);
        } else if VOID_DROPPED.contains(&tag.name.as_str()) || DROPPED.contains(&tag.name.as_str())
        {
            // Dropped.
        } else if tag.name == "img" && !tag.closing {
            image(&tag, &mut page);
        } else {
            page.html.push_str(tag.raw);
        }
    }
    page.html.push_str(rest);
    page
}

/// Puts the loaded images in: `images[N]` is a `data:` address for image `N`, or None to
/// leave it out.
pub fn with_images(html: &str, images: &[Option<String>]) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mark = format!("{IMAGE_MARK}=\"");
    while let Some(at) = rest.find(&mark) {
        out.push_str(&rest[..at]);
        let value = &rest[at + mark.len()..];
        let Some(end) = value.find('"') else {
            break;
        };
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
    let lower = html.to_ascii_lowercase();
    let end = format!("</{name}");
    let Some(at) = lower.find(&end) else {
        return "";
    };
    match html[at..].find('>') {
        Some(close) => &html[at + close + 1..],
        None => "",
    }
}

/// Writes an image: as is, without a pixel, or held back.
fn image(tag: &Tag<'_>, page: &mut Page) {
    if is_pixel(tag) {
        return;
    }
    let remote = tag.attr("src").and_then(remote_source);
    let Some(source) = remote else {
        page.html.push_str(tag.raw);
        return;
    };
    page.html.push_str("<img");
    for (name, value) in &tag.attrs {
        if matches!(name.as_str(), "src" | "srcset" | IMAGE_MARK) {
            continue;
        }
        page.html.push(' ');
        page.html.push_str(name);
        if let Some(value) = value {
            page.html.push_str("=\"");
            page.html.push_str(&value.replace('"', "&quot;"));
            page.html.push('"');
        }
    }
    page.html
        .push_str(&format!(" {IMAGE_MARK}=\"{}\">", page.images.len()));
    page.images.push(source);
}

/// An image too small to see, or hidden: it is only there to report the message was opened.
fn is_pixel(tag: &Tag<'_>) -> bool {
    let tiny = |attr| {
        tag.attr(attr)
            .and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
            .is_some_and(|n| n <= 1.0)
    };
    let style: String = tag
        .attr("style")
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let hidden = ["display:none", "visibility:hidden"]
        .iter()
        .any(|s| style.contains(s));
    let tiny_style = ["width:0", "width:1px", "height:0", "height:1px"]
        .iter()
        .any(|s| style.split(';').any(|rule| rule.starts_with(s)));
    tiny("width") || tiny("height") || hidden || tiny_style
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
