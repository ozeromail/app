//! Typesetting for reading, never for sending: quotes, dashes and spaces that do not break,
//! by the rules of the mail's language. What is stored and replied to stays as written.

/// Whose rules the text follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    /// Russian when most of the letters are Cyrillic.
    pub fn of(text: &str) -> Self {
        Self::of_chars(text.chars())
    }

    /// As `of`, for text in pieces.
    pub fn of_chars(text: impl Iterator<Item = char>) -> Self {
        let (mut cyrillic, mut latin) = (0usize, 0usize);
        for c in text {
            if matches!(c, '\u{400}'..='\u{4FF}') {
                cyrillic += 1;
            } else if c.is_ascii_alphabetic() {
                latin += 1;
            }
        }
        if cyrillic > latin { Self::Ru } else { Self::En }
    }

    /// The outer and the inner pair of double quotes.
    fn quotes(self) -> [(char, char); 2] {
        match self {
            Self::Ru => [('«', '»'), ('„', '“')],
            Self::En => [('“', '”'), ('‘', '’')],
        }
    }
}

pub const NBSP: char = '\u{A0}';

/// Typesets text that runs on across pieces, as the text between a page's tags does: a quote
/// opened in one piece closes in the next, and a short word keeps the word in the next piece.
#[derive(Debug, Clone)]
pub struct Typesetter {
    lang: Lang,
    /// The last character written; `\n` at the start of a line.
    prev: char,
    /// The word written last, up to the space before it.
    word: String,
    /// Double quotes open now.
    open: u8,
}

impl Typesetter {
    pub fn new(lang: Lang) -> Self {
        Self {
            lang,
            prev: '\n',
            word: String::new(),
            open: 0,
        }
    }

    /// The next piece starts a new line: nothing before it joins it.
    pub fn break_line(&mut self) {
        self.prev = '\n';
        self.word.clear();
    }

    /// Whether nothing has been written on this line yet.
    pub fn at_line_start(&self) -> bool {
        self.prev == '\n'
    }

    /// `piece` typeset.
    pub fn set(&mut self, piece: &str) -> String {
        let chars: Vec<char> = piece.chars().collect();
        let mut out = String::with_capacity(piece.len() + 8);
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let mut skip = 0;
            let shown = match c {
                '"' => {
                    let [outer, inner] = self.lang.quotes();
                    if opens(self.prev) {
                        self.open = self.open.saturating_add(1);
                        if self.open == 1 { outer.0 } else { inner.0 }
                    } else {
                        self.open = self.open.saturating_sub(1);
                        if self.open == 0 { outer.1 } else { inner.1 }
                    }
                }
                // An apostrophe in a word; single quotes around words only in English.
                '\'' if self.prev.is_alphanumeric() => '’',
                '\'' if self.lang == Lang::En && opens(self.prev) => '‘',
                '.' if chars.get(i + 1..i + 3) == Some(&['.', '.']) => {
                    skip = 2;
                    '…'
                }
                // A hyphen or two between spaces is a dash, kept with the word before it.
                ' ' if dash_at(&chars, i + 1) > 0 => {
                    skip = dash_at(&chars, i + 1);
                    self.push(&mut out, NBSP);
                    '—'
                }
                ' ' if self.binds(&chars[i + 1..]) => NBSP,
                _ => c,
            };
            self.push(&mut out, shown);
            i += 1 + skip;
        }
        out
    }

    fn push(&mut self, out: &mut String, c: char) {
        out.push(c);
        self.prev = c;
        if c.is_whitespace() {
            self.word.clear();
        } else {
            self.word.push(c);
        }
    }

    /// Whether the space before `rest` had better not break: after a short word, an initial,
    /// `№` or a number before its word, and between groups of digits.
    fn binds(&self, rest: &[char]) -> bool {
        let Some(&next) = rest.first() else {
            return false;
        };
        // Without the quote or bracket that opens it.
        let word = self
            .word
            .trim_start_matches(|c: char| !c.is_alphanumeric() && c != '№');
        if next.is_whitespace() || word.is_empty() {
            return false;
        }
        // в доме, to go
        if word.chars().count() <= 2 && word.chars().all(char::is_alphabetic) {
            return true;
        }
        if matches!(word, "№" | "§") {
            return true;
        }
        // Х. Р. Махмудов
        let mut letters = word.chars();
        let initial = matches!(
            (letters.next(), letters.next(), letters.next()),
            (Some(l), Some('.'), None) if l.is_uppercase()
        );
        if initial && next.is_uppercase() {
            return true;
        }
        let number = word.starts_with(|c: char| c.is_ascii_digit())
            && word.ends_with(|c: char| c.is_ascii_digit())
            && word
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, ',' | '.'));
        if !number {
            return false;
        }
        // 53 713,00: a group of three digits after a short number belongs to it.
        let group = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        let ends = rest.get(group).is_none_or(|c| !c.is_alphanumeric());
        if group == 3 && ends && word.len() <= 3 && word.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        // 30 октября, 5 minutes, 38 000 руб., 15 %
        next.is_alphabetic() || next == '%'
    }
}

/// Whether a straight quote after `prev` opens.
fn opens(prev: char) -> bool {
    prev.is_whitespace()
        || matches!(
            prev,
            '(' | '[' | '{' | '«' | '„' | '“' | '‘' | '—' | '–' | '/'
        )
}

/// How long the `-` or `--` at `at` is when it stands alone before a space; 0 for none.
fn dash_at(chars: &[char], at: usize) -> usize {
    match chars.get(at..) {
        Some(['-', ' ', ..]) => 1,
        Some(['-', '-', ' ', ..]) => 2,
        _ => 0,
    }
}

/// Plain text typeset line by line.
pub fn text(text: &str, lang: Lang) -> String {
    let mut setter = Typesetter::new(lang);
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        setter.break_line();
        out.push_str(&setter.set(line));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_the_language() {
        assert_eq!(Lang::of("Привет, Anna"), Lang::Ru);
        assert_eq!(Lang::of("Hello, Анна"), Lang::En);
    }

    #[test]
    fn sets_russian() {
        assert_eq!(
            text(
                "Письмо \"Выезд \"срочно\" специалиста\" - от 30 октября...",
                Lang::Ru
            ),
            "Письмо «Выезд „срочно“ специалиста»\u{A0}— от\u{A0}30\u{A0}октября…"
        );
        assert_eq!(
            text("Долг 53 713,00 руб., № 20, Махмудов Х. Р. и я", Lang::Ru),
            "Долг 53\u{A0}713,00\u{A0}руб., №\u{A0}20, Махмудов Х.\u{A0}Р. и\u{A0}я"
        );
    }

    #[test]
    fn sets_english() {
        assert_eq!(
            text("I said \"it's 'fine'\" -- see you", Lang::En),
            "I\u{A0}said “it’s ‘fine’”\u{A0}— see you"
        );
    }

    #[test]
    fn keeps_a_word_across_pieces() {
        let mut setter = Typesetter::new(Lang::Ru);
        assert_eq!(setter.set("Ответ \"в"), "Ответ «в");
        assert_eq!(setter.set(" доме\""), "\u{A0}доме»");
    }

    #[test]
    fn leaves_phones_and_dates_alone() {
        assert_eq!(
            text("Call +7 928 505-55-36 by 2026-10-30", Lang::En),
            "Call +7\u{A0}928\u{A0}505-55-36 by\u{A0}2026-10-30"
        );
        assert_eq!(text("a-b - c", Lang::En), "a-b\u{A0}— c");
    }
}
