//! The text of a message being written, with a little Markdown: `**bold**`, `*italic*` or
//! `_italic_`, `~~struck~~`, `` `code` ``, `[links](https://…)`, and lines that start with
//! `# ` (a heading), `- ` (a list item) or `> ` (a quote). A line that is only
//! `![](file:ID)` shows the draft's file ID there, `"s"` or `"m"` after the ID making it
//! smaller. A backslash keeps the next mark as it is. The editors hide the marks and show
//! what they mean; sending turns the text into HTML and plain text.
//!
//! Ranges are byte offsets into the text.

use std::ops::Range;

use crate::FileId;

/// What a mark does to the text it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    Heading,
    Item,
    Quote,
    /// A backslash that keeps the next character as it is.
    Escape,
}

impl Style {
    /// Styles that hold a whole line.
    pub fn is_line(self) -> bool {
        matches!(self, Self::Heading | Self::Item | Self::Quote)
    }
}

/// One mark: the characters that open and close it, which the editors hide, and the text
/// between them, which it styles. Line styles have nothing that closes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub style: Style,
    pub open: Range<usize>,
    pub close: Range<usize>,
    /// Where a link goes.
    pub url: Option<String>,
}

impl Mark {
    pub fn content(&self) -> Range<usize> {
        self.open.end..self.close.start
    }
}

/// How wide an image is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageSize {
    Small,
    Medium,
    #[default]
    Full,
}

/// A run of text lines, or an image line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text(Range<usize>),
    Image {
        line: Range<usize>,
        file: FileId,
        size: ImageSize,
    },
}

const ESCAPED: &[u8] = b"\\*_~`[]()#->!";

/// The image line for `file`.
pub fn image_line(file: FileId, size: ImageSize) -> String {
    match size {
        ImageSize::Small => format!("![](file:{} \"s\")", file.0),
        ImageSize::Medium => format!("![](file:{} \"m\")", file.0),
        ImageSize::Full => format!("![](file:{})", file.0),
    }
}

/// The image a line shows, if it is an image line.
pub fn image(line: &str) -> Option<(FileId, ImageSize)> {
    let inner = line.strip_prefix("![](file:")?.strip_suffix(')')?;
    let (id, size) = match inner.split_once(' ') {
        None => (inner, ImageSize::Full),
        Some((id, "\"s\"")) => (id, ImageSize::Small),
        Some((id, "\"m\"")) => (id, ImageSize::Medium),
        Some(_) => return None,
    };
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((FileId(id.parse().ok()?), size))
}

/// Each line with where it starts.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0;
    text.split('\n').map(move |line| {
        let start = at;
        at += line.len() + 1;
        (start, line)
    })
}

/// The text as runs of text lines between image lines. Every run of text lines, empty ones
/// too, is one block, so joining the blocks with line breaks gives the text back.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut run: Option<Range<usize>> = None;
    for (start, line) in lines(text) {
        let end = start + line.len();
        match image(line) {
            Some((file, size)) => {
                blocks.extend(run.take().map(Block::Text));
                blocks.push(Block::Image {
                    line: start..end,
                    file,
                    size,
                });
            }
            None => run = Some(run.map_or(start..end, |r| r.start..end)),
        }
    }
    blocks.extend(run.map(Block::Text));
    blocks
}

/// The files the text shows, in order.
pub fn images(text: &str) -> Vec<(FileId, ImageSize)> {
    lines(text).filter_map(|(_, line)| image(line)).collect()
}

/// The marks of the text, image lines left out, outer ones first.
pub fn marks(text: &str) -> Vec<Mark> {
    let mut marks = Vec::new();
    for (start, line) in lines(text) {
        if image(line).is_none() {
            line_marks(text, start, start + line.len(), &mut marks);
        }
    }
    marks
}

fn line_marks(text: &str, start: usize, end: usize, marks: &mut Vec<Mark>) {
    let line = &text[start..end];
    let style = match line.get(..2) {
        Some("# ") => Some(Style::Heading),
        Some("- ") => Some(Style::Item),
        Some("> ") => Some(Style::Quote),
        _ => None,
    };
    let from = match style {
        Some(style) => {
            marks.push(Mark {
                style,
                open: start..start + 2,
                close: end..end,
                url: None,
            });
            start + 2
        }
        None => start,
    };
    inline(text, from, end, marks);
}

/// The marks within `text[from..to]`, which holds no line break.
fn inline(text: &str, from: usize, to: usize, marks: &mut Vec<Mark>) {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < to {
        let b = bytes[i];
        if b == b'\\' && i + 1 < to && ESCAPED.contains(&bytes[i + 1]) {
            marks.push(Mark {
                style: Style::Escape,
                open: i..i + 1,
                close: i + 2..i + 2,
                url: None,
            });
            i += 2;
            continue;
        }
        if b == b'`'
            && let Some(close) = find(bytes, i + 1, to, b"`").filter(|&c| c > i + 1)
        {
            marks.push(mark(Style::Code, i, 1, close, 1));
            i = close + 1;
            continue;
        }
        if b == b'['
            && let Some((mark, end)) = link(text, i, to)
        {
            let content = mark.content();
            marks.push(mark);
            inline(text, content.start, content.end, marks);
            i = end;
            continue;
        }
        if let Some((style, delim)) = delimiter(&bytes[i..to])
            && opens(text, i, delim)
            && let Some(close) = closer(
                text,
                i + delim.len() + first_len(text, i + delim.len()),
                to,
                delim,
            )
        {
            let len = delim.len();
            marks.push(mark(style, i, len, close, len));
            inline(text, i + len, close, marks);
            i = close + len;
            continue;
        }
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
}

fn mark(style: Style, open: usize, open_len: usize, close: usize, close_len: usize) -> Mark {
    Mark {
        style,
        open: open..open + open_len,
        close: close..close + close_len,
        url: None,
    }
}

/// `[text](url)` at `at`, and where it ends.
fn link(text: &str, at: usize, to: usize) -> Option<(Mark, usize)> {
    let bytes = text.as_bytes();
    let close = find(bytes, at + 1, to, b"](")?;
    if text[at + 1..close].contains('[') {
        return None;
    }
    let end = find(bytes, close + 2, to, b")")?;
    let url = &text[close + 2..end];
    if url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    Some((
        Mark {
            style: Style::Link,
            open: at..at + 1,
            close: close..end + 1,
            url: Some(url.to_owned()),
        },
        end + 1,
    ))
}

fn delimiter(rest: &[u8]) -> Option<(Style, &'static str)> {
    if rest.starts_with(b"**") {
        Some((Style::Bold, "**"))
    } else if rest.starts_with(b"~~") {
        Some((Style::Strike, "~~"))
    } else if rest.starts_with(b"*") {
        Some((Style::Italic, "*"))
    } else if rest.starts_with(b"_") {
        Some((Style::Italic, "_"))
    } else {
        None
    }
}

/// The length of the character at `at`; content holds one at least.
fn first_len(text: &str, at: usize) -> usize {
    after(text, at).map_or(1, char::len_utf8)
}

fn before(text: &str, at: usize) -> Option<char> {
    text[..at].chars().next_back()
}

fn after(text: &str, at: usize) -> Option<char> {
    text[at..].chars().next()
}

/// Whether `delim` at `at` can open a mark: something other than a space follows, and an
/// underscore starts a word, so `snake_case` stays as it is.
fn opens(text: &str, at: usize, delim: &str) -> bool {
    let next = after(text, at + delim.len());
    if next.is_none_or(char::is_whitespace) {
        return false;
    }
    delim != "_" || !before(text, at).is_some_and(char::is_alphanumeric)
}

/// The first `delim` from `from` to `to` that can close a mark.
fn closer(text: &str, from: usize, to: usize, delim: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < to {
        if bytes[i] == b'\\' {
            i += 1 + after(text, i + 1).map_or(0, char::len_utf8);
            continue;
        }
        if bytes[i..to].starts_with(delim.as_bytes())
            && !before(text, i).is_some_and(char::is_whitespace)
            // `**` would close `*`.
            && !(delim == "*" && bytes.get(i + 1) == Some(&b'*'))
            && (delim != "_" || !after(text, i + 1).is_some_and(char::is_alphanumeric))
        {
            return Some(i);
        }
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

fn find(bytes: &[u8], from: usize, to: usize, what: &[u8]) -> Option<usize> {
    if from > to {
        return None;
    }
    bytes[from..to]
        .windows(what.len())
        .position(|w| w == what)
        .map(|at| from + at)
}

/// The text with a backslash before every character that would start a mark, so it shows
/// as written: for a signature or a quote, which are plain text.
pub fn escape(plain: &str) -> String {
    let mut out = String::with_capacity(plain.len() + plain.len() / 16);
    for c in plain.chars() {
        if c.is_ascii() && ESCAPED.contains(&(c as u8)) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Whether the text has anything plain text cannot show: a style or an image.
pub fn is_rich(text: &str) -> bool {
    !images(text).is_empty() || marks(text).iter().any(|m| m.style != Style::Escape)
}

/// Pieces of a line in order, for a writer: text to show, and marks opening and closing.
enum Piece<'a> {
    Text(&'a str),
    Open(&'a Mark),
    Close(&'a Mark),
}

/// The pieces of `text[start..end]`, given the marks within it.
fn pieces<'a>(text: &'a str, start: usize, end: usize, marks: &'a [Mark]) -> Vec<Piece<'a>> {
    let mut events: Vec<(usize, usize, bool, &Mark)> = Vec::new();
    for (n, mark) in marks.iter().enumerate() {
        if mark.open.start >= start && mark.open.start <= end && !mark.style.is_line() {
            events.push((mark.open.start, n, true, mark));
            events.push((mark.close.start, usize::MAX - n, false, mark));
        }
    }
    // By place; at one place closing marks first, inner ones first.
    events.sort_by_key(|&(at, order, open, _)| (at, open, order));
    let mut out = Vec::new();
    let mut at = start;
    for (place, _, open, mark) in events {
        if place > at {
            out.push(Piece::Text(&text[at..place]));
        }
        let skip = if open { &mark.open } else { &mark.close };
        at = at.max(skip.end);
        out.push(if open {
            Piece::Open(mark)
        } else {
            Piece::Close(mark)
        });
    }
    if end > at {
        out.push(Piece::Text(&text[at..end]));
    }
    out
}

/// The text as plain text: `*bold*`, `_italic_`, a link as its text and address, and each
/// image as its name in brackets from `name`; an image without one is left out.
pub fn to_plain(text: &str, name: &dyn Fn(FileId) -> Option<String>) -> String {
    let marks = marks(text);
    let mut out = String::with_capacity(text.len());
    let mut first = true;
    for (start, line) in lines(text) {
        if let Some((file, _)) = image(line) {
            match name(file) {
                Some(name) => {
                    if !first {
                        out.push('\n');
                    }
                    out.push('[');
                    out.push_str(&name);
                    out.push(']');
                }
                None => continue,
            }
            first = false;
            continue;
        }
        if !first {
            out.push('\n');
        }
        first = false;
        let end = start + line.len();
        let mut from = start;
        if let Some(m) = marks
            .iter()
            .find(|m| m.style.is_line() && m.open.start == start)
        {
            out.push_str(match m.style {
                Style::Item => "• ",
                Style::Quote => "> ",
                _ => "",
            });
            from = m.open.end;
        }
        for piece in pieces(text, from, end, &marks) {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Open(m) | Piece::Close(m) => match m.style {
                    Style::Bold => out.push('*'),
                    Style::Italic => out.push('_'),
                    Style::Link => {
                        if matches!(piece, Piece::Close(_)) {
                            let url = m.url.as_deref().unwrap_or_default();
                            if &text[m.content()] != url {
                                out.push_str(" (");
                                out.push_str(url);
                                out.push(')');
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
    }
    out
}

/// What the text shows, without its marks and images: for a list of drafts.
pub fn shown(text: &str) -> String {
    let marks = marks(text);
    let mut out = String::with_capacity(text.len());
    for (start, line) in lines(text) {
        if image(line).is_some() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        let mut from = start;
        if let Some(m) = marks
            .iter()
            .find(|m| m.style.is_line() && m.open.start == start)
        {
            from = m.open.end;
        }
        for piece in pieces(text, from, start + line.len(), &marks) {
            if let Piece::Text(t) = piece {
                out.push_str(t);
            }
        }
    }
    out
}

/// An image as the HTML shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlImage {
    /// Its `Content-ID`, without angle brackets.
    pub cid: String,
    pub name: String,
}

/// The text as HTML, each image from `image`; an image without one is left out.
pub fn to_html(text: &str, image_of: &dyn Fn(FileId) -> Option<HtmlImage>) -> String {
    let marks = marks(text);
    let mut out = String::from("<div dir=\"auto\">");
    // The list or quote open at the end of the last line.
    let mut open: Option<Style> = None;
    for (start, line) in lines(text) {
        let end = start + line.len();
        let line_mark = marks
            .iter()
            .find(|m| m.style.is_line() && m.open.start == start);
        let group = line_mark
            .map(|m| m.style)
            .filter(|s| matches!(s, Style::Item | Style::Quote));
        if open != group {
            match open {
                Some(Style::Item) => out.push_str("</ul>"),
                Some(Style::Quote) => out.push_str("</blockquote>"),
                _ => {}
            }
            match group {
                Some(Style::Item) => out.push_str("<ul style=\"margin:0;padding-left:1.4em\">"),
                Some(Style::Quote) => out.push_str(
                    "<blockquote style=\"margin:0;padding-left:1ex;border-left:3px solid #d0d7e2;color:#5d6a82\">",
                ),
                _ => {}
            }
            open = group;
        }
        if let Some((file, size)) = image(line) {
            if let Some(img) = image_of(file) {
                let width = match size {
                    ImageSize::Small => "width:40%;",
                    ImageSize::Medium => "width:66%;",
                    ImageSize::Full => "",
                };
                out.push_str(&format!(
                    "<div><img src=\"cid:{}\" alt=\"{}\" style=\"{width}max-width:100%;height:auto\"></div>",
                    html_escape(&img.cid),
                    html_escape(&img.name)
                ));
            }
            continue;
        }
        let (tag, from) = match line_mark.map(|m| m.style) {
            Some(Style::Heading) => ("h3 style=\"margin:0\"", start + 2),
            Some(Style::Item) => ("li", start + 2),
            Some(Style::Quote) => ("div", start + 2),
            _ => ("div", start),
        };
        out.push('<');
        out.push_str(tag);
        out.push('>');
        if text[from..end].trim().is_empty() {
            out.push_str("<br>");
        }
        for piece in pieces(text, from, end, &marks) {
            match piece {
                Piece::Text(t) => out.push_str(&html_escape(t)),
                Piece::Open(m) => match m.style {
                    Style::Bold => out.push_str("<b>"),
                    Style::Italic => out.push_str("<i>"),
                    Style::Strike => out.push_str("<s>"),
                    Style::Code => out.push_str("<code>"),
                    Style::Link => {
                        let url = m.url.as_deref().unwrap_or_default();
                        out.push_str(&format!("<a href=\"{}\">", html_escape(&href(url))));
                    }
                    _ => {}
                },
                Piece::Close(m) => match m.style {
                    Style::Bold => out.push_str("</b>"),
                    Style::Italic => out.push_str("</i>"),
                    Style::Strike => out.push_str("</s>"),
                    Style::Code => out.push_str("</code>"),
                    Style::Link => out.push_str("</a>"),
                    _ => {}
                },
            }
        }
        out.push_str("</");
        out.push_str(tag.split(' ').next().unwrap_or(tag));
        out.push('>');
    }
    match open {
        Some(Style::Item) => out.push_str("</ul>"),
        Some(Style::Quote) => out.push_str("</blockquote>"),
        _ => {}
    }
    out.push_str("</div>");
    out
}

/// A link's address as a link can go: web and mail addresses as they are, a bare
/// `example.com` on the web, anything else nowhere.
fn href(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    if ["https://", "http://", "mailto:", "tel:"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
        url.to_owned()
    } else if lower.contains(':') {
        "#".to_owned()
    } else if url.contains('@') {
        format!("mailto:{url}")
    } else {
        format!("https://{url}")
    }
}

fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Text after a change, with where the caret goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    pub caret: usize,
}

/// Puts `style` on the selection `text[start..end]`, or takes it off where the selection
/// has it already; a line style goes on every line the selection touches. A link goes to
/// `url`, and comes off without one. Returns the text and the selection.
pub fn toggle(
    text: &str,
    start: usize,
    end: usize,
    style: Style,
    url: Option<&str>,
) -> (String, Range<usize>) {
    let (start, end) = (
        start.min(end).min(text.len()),
        end.max(start).min(text.len()),
    );
    if style.is_line() {
        return toggle_lines(text, start, end, style);
    }
    let marks = marks(text);
    let inside = marks
        .iter()
        .find(|m| m.style == style && m.content().start <= start && end <= m.content().end);
    if let Some(m) = inside {
        if style == Style::Link
            && let Some(url) = url.filter(|u| !u.is_empty())
        {
            let mut out = text.to_owned();
            let content = m.content();
            out.replace_range(m.close.clone(), &format!("]({url})"));
            return (out, content);
        }
        let mut out = text.to_owned();
        out.replace_range(m.close.clone(), "");
        out.replace_range(m.open.clone(), "");
        let shift = m.open.len();
        return (out, start - shift..end - shift);
    }
    if style == Style::Link && url.is_none_or(str::is_empty) {
        return (text.to_owned(), start..end);
    }
    // Marks hold no spaces at their ends and no line breaks.
    let chosen = &text[start..end];
    let start = start + (chosen.len() - chosen.trim_start().len());
    let end = (end - (chosen.len() - chosen.trim_end().len())).max(start);
    let end = text[start..end].find('\n').map_or(end, |at| start + at);
    let (open, close) = match style {
        Style::Bold => ("**".to_owned(), "**".to_owned()),
        Style::Italic => ("*".to_owned(), "*".to_owned()),
        Style::Strike => ("~~".to_owned(), "~~".to_owned()),
        Style::Code => ("`".to_owned(), "`".to_owned()),
        _ => ("[".to_owned(), format!("]({})", url.unwrap_or_default())),
    };
    let mut out = String::with_capacity(text.len() + open.len() + close.len() + 8);
    out.push_str(&text[..start]);
    out.push_str(&open);
    if start == end && style == Style::Link {
        // Nothing chosen: the address is the text.
        out.push_str(url.unwrap_or_default());
        out.push_str(&close);
        out.push_str(&text[end..]);
        let at = start + open.len();
        return (out, at..at + url.unwrap_or_default().len());
    }
    out.push_str(&text[start..end]);
    out.push_str(&close);
    out.push_str(&text[end..]);
    (out, start + open.len()..end + open.len())
}

fn prefix(style: Style) -> &'static str {
    match style {
        Style::Heading => "# ",
        Style::Item => "- ",
        _ => "> ",
    }
}

fn toggle_lines(text: &str, start: usize, end: usize, style: Style) -> (String, Range<usize>) {
    let touched =
        |at: usize, line: &str| at <= end && start <= at + line.len() && image(line).is_none();
    let all = lines(text)
        .filter(|&(at, line)| touched(at, line))
        .all(|(_, line)| line.starts_with(prefix(style)));
    let mut out = String::with_capacity(text.len() + 16);
    let (mut new_start, mut new_end) = (start, end);
    for (at, line) in lines(text) {
        if at > 0 {
            out.push('\n');
        }
        let here = out.len();
        let (old, new) = if touched(at, line) {
            let old = ["# ", "- ", "> "]
                .into_iter()
                .find(|p| line.starts_with(p))
                .map_or(0, str::len);
            (old, if all { "" } else { prefix(style) })
        } else {
            (0, "")
        };
        out.push_str(new);
        out.push_str(&line[old..]);
        // A place in this line, where it is now; one in the old mark goes after the new.
        let moved = |p: usize| here + new.len() + p.saturating_sub(at + old);
        if (at..=at + line.len()).contains(&start) {
            new_start = moved(start);
        }
        if (at..=at + line.len()).contains(&end) {
            new_end = moved(end);
        }
    }
    (out, new_start..new_end)
}

/// Fixes up what typing did to the marks, from the text `before` to the text `after`. None
/// when the typing stands as it is.
///
/// - Taking out only hidden marks takes out the character shown before them instead, the
///   one the person sees before the caret.
/// - Taking out a line's mark takes the style off the line.
/// - Taking out part of a mark takes out the rest of it, and a mark left holding nothing
///   goes too.
/// - A line break at the end of a list item or quote starts another; on an empty one it
///   takes the style off instead.
pub fn fix(before: &str, after: &str) -> Option<Edit> {
    let common = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let mut start = common;
    while !before.is_char_boundary(start) || !after.is_char_boundary(start) {
        start -= 1;
    }
    let tail = before[start..]
        .bytes()
        .rev()
        .zip(after[start..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut cut_end = before.len() - tail;
    let mut put_end = after.len() - tail;
    while !before.is_char_boundary(cut_end) || !after.is_char_boundary(put_end) {
        cut_end += 1;
        put_end += 1;
    }
    let cut = start..cut_end;
    let put = &after[start..put_end];
    let marks = marks(before);

    if cut.is_empty() {
        return if put == "\n" {
            line_break(before, start, &marks)
        } else {
            pasted_link(before, start, put, &marks)
        };
    }
    let hidden = |at: usize| {
        marks
            .iter()
            .any(|m| m.open.contains(&at) || m.close.contains(&at))
    };
    // A line's mark taken out: the line goes plain.
    if put.is_empty()
        && let Some(m) = marks
            .iter()
            .find(|m| m.style.is_line() && m.open.start < cut.end && cut.start < m.open.end)
        && cut.start >= m.open.start
    {
        let mut text = before.to_owned();
        text.replace_range(m.open.clone(), "");
        return Some(Edit {
            text,
            caret: m.open.start,
        });
    }
    let mut cut = cut;
    if put.is_empty() && cut.clone().all(hidden) {
        // The character shown before the marks.
        let mut at = cut.start;
        let shown = loop {
            let Some(c) = before[..at].chars().next_back() else {
                break None;
            };
            at -= c.len_utf8();
            if !hidden(at) {
                break Some(at..at + c.len_utf8());
            }
        };
        match shown {
            Some(shown) => cut = shown,
            None => {
                return Some(Edit {
                    text: before.to_owned(),
                    caret: cut.start,
                });
            }
        }
    }
    // What goes: the cut, the rest of any mark it cuts into, and marks it leaves empty.
    let mut gone: Vec<Range<usize>> = vec![cut.clone()];
    let touches = |r: &Range<usize>| r.start < cut.end && cut.start < r.end;
    for m in &marks {
        let content = m.content();
        let emptied = !content.is_empty() && cut.start <= content.start && content.end <= cut.end;
        if touches(&m.open) || touches(&m.close) || emptied {
            gone.push(m.open.clone());
            gone.push(m.close.clone());
        }
    }
    gone.sort_by_key(|r| r.start);
    let mut text = String::with_capacity(before.len() + put.len());
    let mut caret = None;
    let mut at = 0;
    for r in &gone {
        if r.start > at {
            text.push_str(&before[at..r.start]);
        }
        if *r == cut && caret.is_none() {
            text.push_str(put);
            caret = Some(text.len());
        }
        at = at.max(r.end);
    }
    text.push_str(&before[at..]);
    let caret = caret.unwrap_or(text.len());
    if text == after {
        return None;
    }
    Some(Edit { text, caret })
}

/// A web address pasted at `at` in `text` becomes a link, unless it goes into one or into
/// code.
fn pasted_link(text: &str, at: usize, put: &str, marks: &[Mark]) -> Option<Edit> {
    let lower = put.to_ascii_lowercase();
    let address = (lower.starts_with("https://") || lower.starts_with("http://"))
        && put.len() > "https://".len()
        && !put.contains(char::is_whitespace)
        && !put.contains([')', '[', ']']);
    let inside = marks.iter().any(|m| {
        matches!(m.style, Style::Link | Style::Code) && m.open.start < at && at < m.close.end
    });
    if !address || inside {
        return None;
    }
    let link = format!("[{put}]({put})");
    let mut out = text.to_owned();
    out.insert_str(at, &link);
    Some(Edit {
        text: out,
        caret: at + link.len(),
    })
}

/// A line break typed at `at` in `text`: a list item or quote goes on, or ends when empty.
fn line_break(text: &str, at: usize, marks: &[Mark]) -> Option<Edit> {
    let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
    let line_end = text[at..].find('\n').map_or(text.len(), |n| at + n);
    let m = marks
        .iter()
        .find(|m| matches!(m.style, Style::Item | Style::Quote) && m.open.start == line_start)?;
    if at < m.open.end {
        return None;
    }
    let mut out = text.to_owned();
    if text[m.open.end..line_end].trim().is_empty() {
        out.replace_range(m.open.clone(), "");
        return Some(Edit {
            text: out,
            caret: line_start,
        });
    }
    let more = format!("\n{}", prefix(m.style));
    out.insert_str(at, &more);
    Some(Edit {
        text: out,
        caret: at + more.len(),
    })
}

/// The text with `files` shown after the line `at` is on, each on a line of its own.
pub fn insert_images(text: &str, at: usize, files: &[(FileId, ImageSize)]) -> String {
    let at = at.min(text.len());
    let end = text[at..].find('\n').map_or(text.len(), |n| at + n);
    let lines: Vec<String> = files.iter().map(|&(f, s)| image_line(f, s)).collect();
    let mut out = String::with_capacity(text.len() + lines.len() * 24);
    out.push_str(&text[..end]);
    for line in &lines {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    // A line to go on writing after them.
    out.push('\n');
    out.push_str(text[end..].strip_prefix('\n').unwrap_or(&text[end..]));
    out
}

/// The text without the image line of `file`.
pub fn remove_image(text: &str, file: FileId) -> String {
    let kept: Vec<&str> = lines(text)
        .filter(|(_, line)| image(line).is_none_or(|(f, _)| f != file))
        .map(|(_, line)| line)
        .collect();
    kept.join("\n")
}

/// The text with the image of `file` shown at `size`, or as `to` instead.
pub fn change_image(text: &str, file: FileId, to: FileId, size: ImageSize) -> String {
    let changed: Vec<String> = lines(text)
        .map(|(_, line)| match image(line) {
            Some((f, _)) if f == file => image_line(to, size),
            _ => line.to_owned(),
        })
        .collect();
    changed.join("\n")
}

/// The text with the image of `file` moved over the paragraph above or below it: the
/// nearest lines with text, blank lines staying where they are.
pub fn move_image(text: &str, file: FileId, up: bool) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let Some(from) = lines
        .iter()
        .position(|l| image(l).is_some_and(|(f, _)| f == file))
    else {
        return text.to_owned();
    };
    let line = lines.remove(from);
    let blank = |l: &&str| l.trim().is_empty();
    let to = if up {
        // Over blank lines, then over the paragraph.
        let mut i = from;
        while i > 0 && blank(&lines[i - 1]) {
            i -= 1;
        }
        if i > 0 && image(lines[i - 1]).is_some() {
            i - 1
        } else {
            while i > 0 && !blank(&lines[i - 1]) && image(lines[i - 1]).is_none() {
                i -= 1;
            }
            i
        }
    } else {
        let mut i = from;
        while i < lines.len() && blank(&lines[i]) {
            i += 1;
        }
        if i < lines.len() && image(lines[i]).is_some() {
            i + 1
        } else {
            while i < lines.len() && !blank(&lines[i]) && image(lines[i]).is_none() {
                i += 1;
            }
            i
        }
    };
    lines.insert(to.min(lines.len()), line);
    lines.join("\n")
}

#[cfg(test)]
mod tests;
