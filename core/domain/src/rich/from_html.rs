//! HTML as the text of a message being written: for a draft written elsewhere, to go on
//! writing here with its styles.

use super::{ESCAPED, href as link_target};
use crate::encoding::decode_entities;
use crate::html::{DROPPED, LINE_TAGS, Tag, after_end_tag};

/// The text of an HTML message with its bold, italic, struck and code words, links, lists,
/// quotes and headings as marks. What the marks cannot hold stays as plain text; images go,
/// as the draft keeps them as files.
pub fn from_html(html: &str) -> String {
    let mut writer = Writer::default();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        writer.text(&rest[..lt]);
        rest = &rest[lt..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            rest = rest.find('>').map_or("", |gt| &rest[gt + 1..]);
            continue;
        }
        let Some(tag) = Tag::read(rest) else {
            // Not a tag: its `<` is text.
            writer.text("<");
            rest = &rest[1..];
            continue;
        };
        rest = &rest[tag.len..];
        let name = tag.name.as_str();
        if !tag.closing && (DROPPED.contains(&name) || matches!(name, "head" | "style" | "title")) {
            rest = after_end_tag(rest, &tag.name);
            continue;
        }
        writer.tag(&tag);
    }
    writer.text(rest);
    writer.finish()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Inline {
    Bold,
    Italic,
    Strike,
    Code,
    Link(String),
}

impl Inline {
    fn opening(&self) -> &'static str {
        match self {
            Self::Bold => "**",
            Self::Italic => "*",
            Self::Strike => "~~",
            Self::Code => "`",
            Self::Link(_) => "[",
        }
    }

    fn closing(&self) -> String {
        match self {
            Self::Link(url) => format!("]({url})"),
            other => other.opening().to_owned(),
        }
    }

    /// Whether it has marks: a link that goes nowhere is only its text.
    fn marked(&self) -> bool {
        !matches!(self, Self::Link(url) if url.is_empty())
    }

    fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Default)]
struct Writer {
    out: String,
    /// The line being written, without its line mark.
    line: String,
    /// Styles open now, each with whether its mark is written on this line yet: a mark
    /// waits for text, so no empty mark is written, and goes on after a line break.
    styles: Vec<(Inline, bool)>,
    /// A space seen between words, written before the next word.
    space: bool,
    quote: usize,
    heading: usize,
    pre: usize,
    code: usize,
    /// The next line with text starts a list item.
    item: bool,
}

impl Writer {
    fn text(&mut self, raw: &str) {
        for c in decode_entities(raw).chars() {
            if self.pre > 0 && c == '\n' {
                self.end_line(true);
            } else if self.pre > 0 && c != '\r' {
                // Preformatted: every space stays.
                self.put(c);
            } else if c.is_whitespace() && c != '\u{A0}' {
                self.space = self.space || !self.line.is_empty();
            } else {
                self.put(if c == '\u{A0}' { ' ' } else { c });
            }
        }
    }

    fn put(&mut self, c: char) {
        if self.space {
            self.line.push(' ');
            self.space = false;
        }
        for (style, written) in &mut self.styles {
            if !*written && style.marked() {
                self.line.push_str(style.opening());
            }
            *written = true;
        }
        if self.code > 0 {
            // A backtick would end the code early.
            if c != '`' {
                self.line.push(c);
            }
        } else {
            // As `escape` does.
            if c.is_ascii() && ESCAPED.contains(&(c as u8)) {
                self.line.push('\\');
            }
            self.line.push(c);
        }
    }

    fn tag(&mut self, tag: &Tag<'_>) {
        let closing = tag.closing;
        match tag.name.as_str() {
            "br" => self.end_line(true),
            "b" | "strong" => self.style(Inline::Bold, closing),
            "i" | "em" => self.style(Inline::Italic, closing),
            "s" | "strike" | "del" => self.style(Inline::Strike, closing),
            "code" | "tt" => {
                count(&mut self.code, closing);
                self.style(Inline::Code, closing);
            }
            // One that goes nowhere is kept open with no marks, so its end tag ends nothing else.
            "a" => self.style(
                Inline::Link(tag.attr("href").and_then(href).unwrap_or_default()),
                closing,
            ),
            "p" => {
                self.end_line(false);
                if closing {
                    self.blank_line();
                }
            }
            "li" => {
                self.end_line(false);
                self.item = !closing;
            }
            "blockquote" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" => {
                self.end_line(false);
                let depth = match tag.name.as_str() {
                    "blockquote" => &mut self.quote,
                    "pre" => &mut self.pre,
                    _ => &mut self.heading,
                };
                count(depth, closing);
            }
            "td" | "th" => self.space = self.space || !self.line.is_empty(),
            name if LINE_TAGS.contains(&name) => self.end_line(false),
            _ => {}
        }
    }

    fn style(&mut self, style: Inline, closing: bool) {
        if !closing {
            self.styles.push((style, false));
            return;
        }
        let Some(at) = self.styles.iter().rposition(|(s, _)| s.same_kind(&style)) else {
            return;
        };
        // Styles opened inside it close with it and open again after it.
        let closed = self.styles.split_off(at);
        for (s, written) in closed.iter().rev() {
            if *written && s.marked() {
                self.line.push_str(&s.closing());
            }
        }
        self.styles
            .extend(closed.into_iter().skip(1).map(|(s, _)| (s, false)));
    }

    /// Ends the line; with `always`, an empty one too.
    fn end_line(&mut self, always: bool) {
        self.space = false;
        if self.line.is_empty() && !always {
            return;
        }
        for (s, written) in self.styles.iter_mut().rev() {
            if *written && s.marked() {
                self.line.push_str(&s.closing());
            }
            *written = false;
        }
        if !self.line.is_empty() {
            if self.quote > 0 {
                self.out.push_str("> ");
            } else if self.heading > 0 {
                self.out.push_str("# ");
            } else if self.item {
                self.out.push_str("- ");
            }
            self.item = false;
        }
        self.out.push_str(&self.line);
        self.out.push('\n');
        self.line.clear();
    }

    fn blank_line(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    fn finish(mut self) -> String {
        self.end_line(false);
        self.out.trim_matches('\n').to_owned()
    }
}

/// Where a link goes, if it goes somewhere a link here can: where sending lets it go, with
/// no spaces or brackets, which would end it.
fn href(raw: &str) -> Option<String> {
    let url = decode_entities(raw).trim().to_owned();
    let usable = !url.is_empty()
        && !url.starts_with('#')
        && link_target(&url) != "#"
        && !url.contains(|c: char| c.is_whitespace() || c == ')' || c == '(');
    usable.then_some(url)
}

/// One more of what `closing` ends, or one less.
fn count(depth: &mut usize, closing: bool) {
    *depth = if closing {
        depth.saturating_sub(1)
    } else {
        *depth + 1
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_styles_and_links() {
        let html = "<div dir=\"ltr\">Hi <b>there</b>, <i>see</i> <a href=\"https://example.com/a\">this</a> \
                    and <s>not</s> <code>x_y</code>.</div>";
        assert_eq!(
            from_html(html),
            "Hi **there**, *see* [this](https://example.com/a) and ~~not~~ `x_y`."
        );
    }

    #[test]
    fn keeps_lines_lists_quotes_and_headings() {
        let html = "<h2>Plan</h2><div>One</div><div><br></div><div>Two&nbsp;&amp; more</div>\
                    <ul><li>first</li><li>second</li></ul><blockquote>quoted<br>twice</blockquote><p>End</p>";
        assert_eq!(
            from_html(html),
            "# Plan\nOne\n\nTwo & more\n- first\n- second\n> quoted\n> twice\nEnd"
        );
    }

    #[test]
    fn keeps_marks_as_written_and_leaves_out_what_is_not_text() {
        let html = "<!DOCTYPE html><html><head><style>p{color:red}</style><title>T</title></head>\
                    <body><!-- note --><p>5*3 = 15 # [x]</p><img src=\"cid:1\"></body></html>";
        assert_eq!(from_html(html), super::super::escape("5*3 = 15 # [x]"));
    }

    #[test]
    fn styles_go_on_after_a_line_break_and_never_hold_spaces() {
        assert_eq!(
            from_html("<b>one <br>two </b>three"),
            "**one**\n**two** three"
        );
        assert_eq!(from_html("<b></b>x<i> </i>y"), "x y");
    }

    #[test]
    fn a_link_that_cannot_go_anywhere_is_its_text() {
        assert_eq!(
            from_html("<a href=\"javascript:x()\">a</a> <a href='#top'>b</a> <a>c</a>"),
            "a b c"
        );
    }

    #[test]
    fn keeps_the_spaces_of_preformatted_text() {
        assert_eq!(
            from_html("<pre>if x:\n    y  = 1</pre>"),
            "if x:\n    y  = 1"
        );
    }

    #[test]
    fn a_lone_angle_bracket_is_text() {
        assert_eq!(
            from_html("1 < 2 &lt; 3 &#x41;&#66; &bogus; <"),
            "1 < 2 < 3 AB &bogus; <"
        );
    }

    #[test]
    fn reads_back_what_sending_writes() {
        let text = "# Notes\nHi **bold** *it* ~~x~~ `c` [l](https://e.com)\n\n- one\n- two\n> said\nend 5\\*3";
        assert_eq!(from_html(&super::super::to_html(text, &|_| None)), text);
    }
}
