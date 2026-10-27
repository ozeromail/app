//! The text of a message being written, for the editors (`ozero_domain::rich`). Offsets
//! are in UTF-16 code units, as Kotlin and `NSString` count.

use ozero_domain::FileId;
use ozero_domain::rich::{self, Block, ImageSize, Style};

/// What a mark does to the text it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RichStyle {
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

impl From<Style> for RichStyle {
    fn from(s: Style) -> Self {
        match s {
            Style::Bold => Self::Bold,
            Style::Italic => Self::Italic,
            Style::Strike => Self::Strike,
            Style::Code => Self::Code,
            Style::Link => Self::Link,
            Style::Heading => Self::Heading,
            Style::Item => Self::Item,
            Style::Quote => Self::Quote,
            Style::Escape => Self::Escape,
        }
    }
}

impl From<RichStyle> for Style {
    fn from(s: RichStyle) -> Self {
        match s {
            RichStyle::Bold => Self::Bold,
            RichStyle::Italic => Self::Italic,
            RichStyle::Strike => Self::Strike,
            RichStyle::Code => Self::Code,
            RichStyle::Link => Self::Link,
            RichStyle::Heading => Self::Heading,
            RichStyle::Item => Self::Item,
            RichStyle::Quote => Self::Quote,
            RichStyle::Escape => Self::Escape,
        }
    }
}

/// How wide an image is shown.
#[uniffi::remote(Enum)]
pub enum ImageSize {
    Small,
    Medium,
    Full,
}

/// A mark: the editors hide `open_start..open_end` and `close_start..close_end`, and style
/// what is between them. A list item's `- ` shows as a bullet instead.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RichMark {
    pub style: RichStyle,
    pub open_start: u32,
    pub open_end: u32,
    pub close_start: u32,
    pub close_end: u32,
    /// Where a link goes.
    pub url: Option<String>,
}

/// Text lines from `start` to `end`, or an image line.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RichBlock {
    Text {
        start: u32,
        end: u32,
    },
    Image {
        start: u32,
        end: u32,
        file: i64,
        size: ImageSize,
    },
}

/// Text with the selection in it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RichText {
    pub text: String,
    pub start: u32,
    pub end: u32,
}

/// UTF-16 offsets of a text's bytes, and back.
struct Units<'a> {
    text: &'a str,
}

impl Units<'_> {
    fn of(&self, byte: usize) -> u32 {
        self.all(&[byte])[0]
    }

    /// The offsets of `bytes`, which may come in any order, in one pass over the text.
    fn all(&self, bytes: &[usize]) -> Vec<u32> {
        let mut order: Vec<usize> = (0..bytes.len()).collect();
        order.sort_unstable_by_key(|&i| bytes[i]);
        let mut offsets = vec![0; bytes.len()];
        let (mut byte, mut unit) = (0, 0usize);
        for i in order {
            let to = bytes[i].min(self.text.len());
            if to > byte {
                unit += self.text[byte..to]
                    .chars()
                    .map(char::len_utf16)
                    .sum::<usize>();
                byte = to;
            }
            offsets[i] = u32::try_from(unit).unwrap_or(u32::MAX);
        }
        offsets
    }

    /// The byte at a UTF-16 offset; one inside a character counts as after it.
    fn byte(&self, unit: u32) -> usize {
        let mut seen = 0;
        for (at, c) in self.text.char_indices() {
            if seen >= unit as usize {
                return at;
            }
            seen += c.len_utf16();
        }
        self.text.len()
    }
}

/// The marks of a text block, outer ones first.
#[uniffi::export]
pub fn rich_marks(text: String) -> Vec<RichMark> {
    let marks = rich::marks(&text);
    let bytes: Vec<usize> = marks
        .iter()
        .flat_map(|m| [m.open.start, m.open.end, m.close.start, m.close.end])
        .collect();
    let units = Units { text: &text }.all(&bytes);
    marks
        .into_iter()
        .zip(units.chunks_exact(4))
        .map(|(m, u)| RichMark {
            style: m.style.into(),
            open_start: u[0],
            open_end: u[1],
            close_start: u[2],
            close_end: u[3],
            url: m.url,
        })
        .collect()
}

/// The text as runs of text lines between image lines; joined with line breaks they are
/// the text again.
#[uniffi::export]
pub fn rich_blocks(text: String) -> Vec<RichBlock> {
    let blocks = rich::blocks(&text);
    let bytes: Vec<usize> = blocks
        .iter()
        .flat_map(|block| match block {
            Block::Text(r) => [r.start, r.end],
            Block::Image { line, .. } => [line.start, line.end],
        })
        .collect();
    let units = Units { text: &text }.all(&bytes);
    blocks
        .into_iter()
        .zip(units.chunks_exact(2))
        .map(|(block, u)| match block {
            Block::Text(_) => RichBlock::Text {
                start: u[0],
                end: u[1],
            },
            Block::Image { file, size, .. } => RichBlock::Image {
                start: u[0],
                end: u[1],
                file: file.0,
                size,
            },
        })
        .collect()
}

/// Puts `style` on the selection, or takes it off where it has it; a link goes to `url`,
/// and comes off without one.
#[uniffi::export]
pub fn rich_toggle(
    text: String,
    start: u32,
    end: u32,
    style: RichStyle,
    url: Option<String>,
) -> RichText {
    let units = Units { text: &text };
    let (changed, chosen) = rich::toggle(
        &text,
        units.byte(start),
        units.byte(end),
        style.into(),
        url.as_deref(),
    );
    let units = Units { text: &changed }.all(&[chosen.start, chosen.end]);
    let (start, end) = (units[0], units[1]);
    RichText {
        text: changed,
        start,
        end,
    }
}

/// What typing did to the marks, fixed up: None when `after` stands as it is, else the text
/// with the caret at `start`, which is `end`.
#[uniffi::export]
pub fn rich_fix(before: String, after: String) -> Option<RichText> {
    let edit = rich::fix(&before, &after)?;
    let caret = Units { text: &edit.text }.of(edit.caret);
    Some(RichText {
        text: edit.text,
        start: caret,
        end: caret,
    })
}

/// The text with `files` shown after the line `at` is on, each on a line of its own; more
/// than one at a time are shown smaller.
#[uniffi::export]
pub fn rich_insert_images(text: String, at: u32, files: Vec<i64>) -> String {
    let files: Vec<FileId> = files.into_iter().map(FileId).collect();
    let at = Units { text: &text }.byte(at);
    rich::insert_images(&text, at, &files)
}

/// The address of the link the selection is in, if it is in one.
#[uniffi::export]
pub fn rich_link_at(text: String, start: u32, end: u32) -> Option<String> {
    let units = Units { text: &text };
    rich::link_at(&text, units.byte(start), units.byte(end))
}

/// The text without the image of `file`.
#[uniffi::export]
pub fn rich_remove_image(text: String, file: i64) -> String {
    rich::remove_image(&text, FileId(file))
}

/// The text with the image of `file` shown at `size`, as the file `to`.
#[uniffi::export]
pub fn rich_change_image(text: String, file: i64, to: i64, size: ImageSize) -> String {
    rich::change_image(&text, FileId(file), FileId(to), size)
}

/// The text with the image of `file` moved over the paragraph above or below it.
#[uniffi::export]
pub fn rich_move_image(text: String, file: i64, up: bool) -> String {
    rich::move_image(&text, FileId(file), up)
}

/// Plain text as text with styles, its marks kept as they are.
#[uniffi::export]
pub fn rich_escape(plain: String) -> String {
    rich::escape(&plain)
}

/// What the text shows, without marks and images: for a list of drafts.
#[uniffi::export]
pub fn rich_shown(text: String) -> String {
    rich::shown(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_in_utf16() {
        let text = "😀 **жир**".to_owned();
        let marks = rich_marks(text.clone());
        assert_eq!(
            marks,
            vec![RichMark {
                style: RichStyle::Bold,
                open_start: 3,
                open_end: 5,
                close_start: 8,
                close_end: 10,
                url: None,
            }]
        );
        let toggled = rich_toggle("😀 жир".into(), 3, 6, RichStyle::Italic, None);
        assert_eq!(toggled.text, "😀 *жир*");
        assert_eq!((toggled.start, toggled.end), (4, 7));
        let fixed = rich_fix(text, "😀 **жир*".into()).unwrap();
        assert_eq!((fixed.text.as_str(), fixed.start), ("😀 **жи**", 7));
        assert_eq!(
            rich_blocks("😀\n![](file:3)\nб".into()),
            vec![
                RichBlock::Text { start: 0, end: 2 },
                RichBlock::Image {
                    start: 3,
                    end: 14,
                    file: 3,
                    size: ImageSize::Full
                },
                RichBlock::Text { start: 15, end: 16 },
            ]
        );
    }
}
