//! A light pass over a message's HTML before the reading screen shows it. The web view's
//! sandbox is the real guard; this takes out what has no business in a message and holds
//! back remote images, so the sender cannot tell the message was opened.

use std::borrow::Cow;

use crate::typo::{Lang, Typesetter};

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

/// What mail apps put a quoted message in, by class.
const QUOTE_CLASSES: [&str; 3] = ["gmail_quote", "yahoo_quoted", "protonmail_quote"];
/// The same, by id: Outlook's.
const QUOTE_IDS: [&str; 1] = ["mail-editor-reference-message-container"];
/// The folded quote opens with a tap, with no script.
const FOLD_OPEN: &str =
    "<details class=\"ozero-quote\"><summary>\u{2022}\u{2022}\u{2022}</summary>";
const FOLD_CLOSE: &str = "</details>";

/// The quote `fold_quote` folds is wrapped in this when it sets its own look, so a plain page
/// can keep it light.
const STYLED_OPEN: &str = "<div class=\"ozero-styled\">";
const STYLED_CLOSE: &str = "</div>";

/// A message's HTML with its quote folded, and whether it sets its own look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folded<'a> {
    pub html: Cow<'a, str>,
    /// The page sets its own look, with backgrounds or a table layout, as newsletters do: it
    /// keeps the light page it was made for. Plain mail takes the reader's colours and type,
    /// even with a coloured word or a newsletter in the folded quote under it.
    /// Only the markup counts, tags and `<style>` sheets, never the words of the mail.
    pub styled: bool,
    /// Where the folded quote sits when it sets its own look, its wrapper included.
    kept: Option<(usize, usize)>,
}

fn sets_look(html: &str) -> bool {
    let paints = |s: &str| ["bgcolor", "background"].iter().any(|n| s.contains(n));
    html.split('<').skip(1).any(|part| {
        let (tag, after) = part.split_once('>').unwrap_or((part, ""));
        let tag = tag.to_ascii_lowercase();
        tag.starts_with("table")
            || paints(&tag)
            || (tag.starts_with("style") && paints(&after.to_ascii_lowercase()))
    })
}

/// Folds the quoted mail at the end of a reply under a mark that opens it. Answers written
/// between quotes stay open, and so does a forward with nothing written above it.
pub fn fold_quote(html: &str) -> Folded<'_> {
    let Some((start, end)) = folded(html) else {
        return Folded {
            html: Cow::Borrowed(html),
            styled: sets_look(html),
            kept: None,
        };
    };
    let (before, quote, after) = (&html[..start], &html[start..end], &html[end..]);
    let kept = sets_look(quote);
    let (open, close) = if kept {
        (STYLED_OPEN, STYLED_CLOSE)
    } else {
        ("", "")
    };
    let at = start + FOLD_OPEN.len();
    Folded {
        html: Cow::Owned([before, FOLD_OPEN, open, quote, close, FOLD_CLOSE, after].concat()),
        styled: sets_look(before) || sets_look(after),
        kept: kept.then_some((at, at + open.len() + quote.len() + close.len())),
    }
}

/// Where the quote `fold_quote` folds starts and ends.
fn folded(html: &str) -> Option<(usize, usize)> {
    last_quote(html).filter(|&(start, end)| has_text(&html[..start]) && !has_text(&html[end..]))
}

/// Where the last quote that is not inside another one starts and ends.
fn last_quote(html: &str) -> Option<(usize, usize)> {
    let mut last = None;
    // The open quote's tag name, how deep its own tag goes, and where it starts.
    let mut open: Option<(String, usize, usize)> = None;
    let mut at = 0;
    while let Some(lt) = html[at..].find('<') {
        at += lt;
        let Some(tag) = Tag::read(&html[at..]) else {
            at += 1;
            continue;
        };
        let end = at + tag.len;
        match &mut open {
            None if !tag.closing && tag.is_quote() => open = Some((tag.name, 1, at)),
            Some((name, depth, start)) if *name == tag.name => {
                if !tag.closing {
                    *depth += 1;
                } else if *depth == 1 {
                    last = Some((*start, end));
                    open = None;
                } else {
                    *depth -= 1;
                }
            }
            _ => {}
        }
        at = end;
    }
    open.map(|(_, _, start)| (start, html.len())).or(last)
}

/// Whether any text shows: not tags, styles or spaces.
fn has_text(html: &str) -> bool {
    let mut rest = html;
    loop {
        let lt = rest.find('<').unwrap_or(rest.len());
        let shown = rest[..lt]
            .split("&nbsp;")
            .flat_map(|t| t.split("&#160;"))
            .any(|t| t.chars().any(|c| !c.is_whitespace()));
        if shown {
            return true;
        }
        rest = &rest[lt..];
        if rest.is_empty() {
            return false;
        }
        match Tag::read(rest) {
            Some(tag) if !tag.closing && matches!(tag.name.as_str(), "style" | "title") => {
                rest = after_end_tag(&rest[tag.len..], &tag.name);
            }
            Some(tag) => rest = &rest[tag.len..],
            None => return true,
        }
    }
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

    /// A quoted message, as mail apps mark one.
    fn is_quote(&self) -> bool {
        let class = self.attr("class").unwrap_or_default();
        (self.name == "blockquote"
            && self
                .attr("type")
                .is_some_and(|t| t.eq_ignore_ascii_case("cite")))
            || class.split_whitespace().any(|c| QUOTE_CLASSES.contains(&c))
            || self.attr("id").is_some_and(|id| QUOTE_IDS.contains(&id))
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

/// Elements whose text is shown as written.
const VERBATIM: [&str; 8] = [
    "pre", "code", "kbd", "samp", "tt", "style", "title", "textarea",
];
/// Tags that start a new line.
const LINE_TAGS: [&str; 20] = [
    "br",
    "p",
    "div",
    "li",
    "ul",
    "ol",
    "blockquote",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "tr",
    "td",
    "th",
    "table",
    "hr",
    "details",
    "summary",
];
/// Tags a list item's line may hold.
const INLINE_TAGS: [&str; 12] = [
    "a", "b", "i", "u", "s", "em", "strong", "span", "font", "small", "mark", "code",
];
const HANG_OPEN: &str = "<span class=\"ozero-hang\">";

/// A plain page typeset for reading in the language of its text: quotes, dashes, spaces that
/// do not break, a quote that opens a line hung in the margin, and lines that number or bullet
/// a list set as one. A folded quote with its own look stays as it is.
pub fn typeset(page: &Folded<'_>) -> String {
    let html = page.html.as_ref();
    let shown = html
        .split('<')
        .flat_map(|part| part.split_once('>').map_or(part, |(_, text)| text).chars());
    let mut setter = Typesetter::new(Lang::of_chars(shown));
    match page.kept {
        Some((start, end)) => {
            let before = typeset_part(&mut setter, &html[..start]);
            setter.break_line();
            [
                &before,
                &html[start..end],
                &typeset_part(&mut setter, &html[end..]),
            ]
            .concat()
        }
        None => typeset_part(&mut setter, html),
    }
}

fn typeset_part(setter: &mut Typesetter, html: &str) -> String {
    let mut out = String::with_capacity(html.len() + html.len() / 8);
    let mut rest = html;
    while !rest.is_empty() {
        let lt = rest.find('<').unwrap_or(rest.len());
        let text = &rest[..lt];
        if text.trim().is_empty() {
            out.push_str(text);
        } else {
            let at_start = setter.at_line_start();
            let set = setter.set(&decode_quotes(text));
            let lead = set.len() - set.trim_start().len();
            match set[lead..].chars().next() {
                Some(q @ ('«' | '„' | '“' | '‘')) if at_start => {
                    out.push_str(&set[..lead]);
                    out.push_str(HANG_OPEN);
                    out.push(q);
                    out.push_str("</span>");
                    out.push_str(&set[lead + q.len_utf8()..]);
                }
                _ => out.push_str(&set),
            }
        }
        rest = &rest[lt..];
        if rest.is_empty() {
            break;
        }
        let Some(tag) = Tag::read(rest) else {
            out.push('<');
            rest = &rest[1..];
            continue;
        };
        let name = tag.name.as_str();
        if LINE_TAGS.contains(&name) {
            setter.break_line();
        }
        let after = &rest[tag.len..];
        // What `pre`, `code` and the like hold goes as written.
        let next = if !tag.closing && VERBATIM.contains(&name) {
            after_end_tag(after, name)
        } else {
            after
        };
        out.push_str(&rest[..rest.len() - next.len()]);
        rest = next;
    }
    lists(&out)
}

/// Quotes and spaces written as entities, as characters the typesetter reads.
fn decode_quotes(text: &str) -> Cow<'_, str> {
    if !text.contains('&') {
        return Cow::Borrowed(text);
    }
    let mut text = text.to_owned();
    for (entity, c) in [
        ("&quot;", "\""),
        ("&#34;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&nbsp;", "\u{A0}"),
        ("&#160;", "\u{A0}"),
    ] {
        text = text.replace(entity, c);
    }
    Cow::Owned(text)
}

/// Two or more lines in a row that start with a number or a bullet, each set as a list item
/// with its text hanging past the mark.
fn lists(html: &str) -> String {
    // The lines between `<br>` tags, and each `<br>` before a line.
    let mut lines = Vec::new();
    let mut breaks = vec![""];
    let (mut start, mut at) = (0, 0);
    while let Some(lt) = html[at..].find('<') {
        at += lt;
        match Tag::read(&html[at..]) {
            Some(tag) if tag.name == "br" => {
                lines.push(&html[start..at]);
                breaks.push(tag.raw);
                at += tag.len;
                start = at;
            }
            Some(tag) => at += tag.len,
            None => at += 1,
        }
    }
    lines.push(&html[start..]);
    if lines.len() < 3 {
        return html.to_owned();
    }
    // The first line holds whatever came before it, so it is never an item.
    let mut items: Vec<Option<(&str, &str)>> = lines
        .iter()
        .enumerate()
        .map(|(n, line)| if n == 0 { None } else { item(line) })
        .collect();
    for n in 0..items.len() {
        let alone = items[n].is_some()
            && !n.checked_sub(1).is_some_and(|p| items[p].is_some())
            && !items.get(n + 1).is_some_and(Option::is_some);
        if alone {
            items[n] = None;
        }
    }
    let mut out = String::with_capacity(html.len() + 64);
    for (n, line) in lines.iter().enumerate() {
        // An item is a block of its own: the breaks around it go.
        let around = items[n].is_some() || n.checked_sub(1).is_some_and(|p| items[p].is_some());
        if !around {
            out.push_str(breaks[n]);
        }
        match items[n] {
            Some((mark, text)) => {
                out.push_str("<div class=\"ozero-li\"><span>");
                out.push_str(mark);
                out.push_str("</span><span>");
                out.push_str(text);
                out.push_str("</span></div>");
            }
            None => out.push_str(line),
        }
    }
    out
}

/// A line that starts a list item, as its mark and its text: `1.`, `2)`, `-` or `•` and a
/// space, with only inline tags, each closed, after it.
fn item(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let (mark, after) =
        if (1..=3).contains(&digits) && matches!(line.as_bytes().get(digits), Some(b'.' | b')')) {
            (&line[..=digits], &line[digits + 1..])
        } else {
            ("\u{2022}", line.strip_prefix(['-', '•'])?)
        };
    let text = after.strip_prefix([' ', '\u{A0}'])?.trim_start();
    if text.is_empty() || !inline_and_closed(text) {
        return None;
    }
    Some((mark, text))
}

/// Whether `html` holds only inline tags, each closed in it.
fn inline_and_closed(html: &str) -> bool {
    let mut open: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(lt) = html[at..].find('<') {
        at += lt;
        let Some(tag) = Tag::read(&html[at..]) else {
            at += 1;
            continue;
        };
        at += tag.len;
        if !INLINE_TAGS.contains(&tag.name.as_str()) {
            return false;
        }
        if !tag.closing {
            open.push(tag.name);
        } else if open.pop().as_deref() != Some(tag.name.as_str()) {
            return false;
        }
    }
    open.is_empty()
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
    #[test]
    fn folds_the_quote_under_a_reply() {
        let html = concat!(
            "<div>Sounds good</div>",
            "<div class=\"gmail_quote\"><div>On Mon, Anna wrote:</div>",
            "<blockquote class=\"gmail_quote\"><div>Lunch?</div></blockquote></div>",
            "<br>&nbsp;",
        );
        assert_eq!(
            fold_quote(html).html,
            concat!(
                "<div>Sounds good</div><details class=\"ozero-quote\"><summary>\u{2022}\u{2022}\u{2022}</summary>",
                "<div class=\"gmail_quote\"><div>On Mon, Anna wrote:</div>",
                "<blockquote class=\"gmail_quote\"><div>Lunch?</div></blockquote></div>",
                "</details><br>&nbsp;",
            )
        );
        let apple = "Yes<BLOCKQUOTE type=Cite>On Mon<blockquote type=cite>old</blockquote>";
        assert_eq!(
            fold_quote(apple).html,
            format!("Yes{FOLD_OPEN}{}{FOLD_CLOSE}", &apple[3..])
        );
    }

    #[test]
    fn tells_a_newsletter_from_plain_mail() {
        for plain in [
            "<div dir=ltr>Hi <b>Anna</b></div><blockquote type=cite>old</blockquote>",
            "<p>Did the background check pass? Favourite color: blue</p>",
            "<style>p{margin:0}</style><p>Hi</p>",
            "<p>Hi</p><font color=#888888>Anna</font>",
            "<style>a:link{color:#0563C1}</style><p style='color:black'>Hi</p>",
        ] {
            assert!(!fold_quote(plain).styled, "{plain}");
        }
        for styled in [
            "<style>BODY { Background: #f4f4f4 }</style><p>Hi</p>",
            "<table width=600><tr><td>News</td></tr></table>",
            "<div style='Background-Color: #222'>Hi</div>",
            "<body BGCOLOR=#fff>Hi</body>",
        ] {
            assert!(fold_quote(styled).styled, "{styled}");
        }
    }

    #[test]
    fn reads_a_reply_to_a_newsletter_as_plain_mail() {
        // A plain answer from Gmail, with the newsletter it answers quoted under it.
        let news = "<table bgcolor=\"#f6f6f6\" width=600><tr><td>News</td></tr></table>";
        let reply = format!(
            "<div dir=\"auto\">Hi<br/>Please send it</div><br><div class=\"gmail_extra\">\
             <div>On Fri, Anna wrote:<br><blockquote class=\"gmail_quote\">{news}</blockquote></div></div>"
        );
        assert!(!fold_quote(&reply).styled);
        assert!(
            fold_quote(&reply)
                .html
                .contains(&format!("{FOLD_OPEN}{STYLED_OPEN}<blockquote"))
        );
        // The same newsletter forwarded bare stays a newsletter.
        let forward = format!("<blockquote class=\"gmail_quote\">{news}</blockquote>");
        let bare = fold_quote(&forward);
        assert!(bare.styled);
        assert_eq!(bare.html, forward);
    }

    #[test]
    fn keeps_inline_answers_and_bare_forwards() {
        let inline = "<blockquote type=cite>Lunch?</blockquote>Yes<blockquote type=cite>At 1?</blockquote>Sure";
        assert_eq!(fold_quote(inline).html, inline);
        let forward = "<style>p{}</style><p>&nbsp;</p><div class=gmail_quote>Forwarded</div>";
        assert_eq!(fold_quote(forward).html, forward);
        assert_eq!(fold_quote("Just text").html, "Just text");
    }

    #[test]
    fn typesets_a_plain_page() {
        let html = concat!(
            "<div dir=\"auto\">\"Выезд\" - от 30 октября<br/>Прошу:<br/>1. Направить <b>копию</b>",
            "<br/>2. Направить расчёт<br/><br/>Махмудов Х. Р.<br/><code>\"x\"</code></div>",
        );
        assert_eq!(
            typeset(&fold_quote(html)),
            concat!(
                "<div dir=\"auto\"><span class=\"ozero-hang\">«</span>Выезд»\u{A0}— от\u{A0}30\u{A0}октября",
                "<br/>Прошу:<div class=\"ozero-li\"><span>1.</span><span>Направить <b>копию</b></span></div>",
                "<div class=\"ozero-li\"><span>2.</span><span>Направить расчёт</span></div>",
                "<br/>Махмудов Х.\u{A0}Р.<br/><code>\"x\"</code></div>",
            )
        );
    }

    #[test]
    fn leaves_one_numbered_line_and_the_folded_newsletter() {
        let one = "Hi<br>2026. Big year<br>Bye";
        assert_eq!(typeset(&fold_quote(one)), one);
        let reply = concat!(
            "<p>\"Ok\"</p><blockquote type=cite><table bgcolor=#fff><tr><td>\"News\"",
            "<br>1. a<br>2. b<br>3. c</td></tr></table></blockquote>",
        );
        let folded = fold_quote(reply);
        let quote = &reply["<p>\"Ok\"</p>".len()..];
        assert_eq!(
            typeset(&folded),
            format!(
                "<p>{HANG_OPEN}\u{201C}</span>Ok\u{201D}</p>{FOLD_OPEN}{STYLED_OPEN}{quote}{STYLED_CLOSE}{FOLD_CLOSE}"
            )
        );
    }
}
