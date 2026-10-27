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
        self.of_range(0, byte)
    }

    fn of_range(&self, from: usize, to: usize) -> u32 {
        let to = to.min(self.text.len());
        let units: usize = self.text[from.min(to)..to]
            .chars()
            .map(char::len_utf16)
            .sum();
        u32::try_from(units).unwrap_or(u32::MAX)
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
    let units = Units { text: &text };
    // One pass for all of them: the offsets of every byte where a mark starts or ends.
    let marks = rich::marks(&text);
    let mut at: Vec<usize> = marks
        .iter()
        .flat_map(|m| [m.open.start, m.open.end, m.close.start, m.close.end])
        .collect();
    at.sort_unstable();
    at.dedup();
    let mut offsets = Vec::with_capacity(at.len());
    let (mut byte, mut unit) = (0, 0u32);
    for &target in &at {
        unit += units.of_range(byte, target);
        byte = target;
        offsets.push(unit);
    }
    let of = |b: usize| offsets[at.binary_search(&b).unwrap_or(0)];
    marks
        .into_iter()
        .map(|m| RichMark {
            style: m.style.into(),
            open_start: of(m.open.start),
            open_end: of(m.open.end),
            close_start: of(m.close.start),
            close_end: of(m.close.end),
            url: m.url,
        })
        .collect()
}

/// The text as runs of text lines between image lines; joined with line breaks they are
/// the text again.
#[uniffi::export]
pub fn rich_blocks(text: String) -> Vec<RichBlock> {
    let units = Units { text: &text };
    rich::blocks(&text)
        .into_iter()
        .map(|block| match block {
            Block::Text(r) => RichBlock::Text {
                start: units.of(r.start),
                end: units.of(r.end),
            },
            Block::Image { line, file, size } => RichBlock::Image {
                start: units.of(line.start),
                end: units.of(line.end),
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
    let units = Units { text: &changed };
    let (start, end) = (units.of(chosen.start), units.of(chosen.end));
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
    let size = if files.len() > 1 {
        ImageSize::Medium
    } else {
        ImageSize::Full
    };
    let files: Vec<(FileId, ImageSize)> = files.into_iter().map(|f| (FileId(f), size)).collect();
    let at = Units { text: &text }.byte(at);
    rich::insert_images(&text, at, &files)
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
    }
}
