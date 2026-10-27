//! Meeting invitations as mail carries them: iCalendar (RFC 5545) sent the iTIP way
//! (RFC 5546). Only what the reader shows and an answer needs is read.

use crate::{Address, MessageId};

/// What the person says to an invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Answer {
    Yes,
    Maybe,
    No,
}

impl Answer {
    /// As the database keeps it: 1, 2 or 3.
    pub fn code(self) -> i64 {
        match self {
            Self::Yes => 1,
            Self::Maybe => 2,
            Self::No => 3,
        }
    }

    /// None for any other number, 0 among them: no answer.
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            1 => Some(Self::Yes),
            2 => Some(Self::Maybe),
            3 => Some(Self::No),
            _ => None,
        }
    }

    fn partstat(self) -> &'static str {
        match self {
            Self::Yes => "ACCEPTED",
            Self::Maybe => "TENTATIVE",
            Self::No => "DECLINED",
        }
    }

    fn from_partstat(value: &str) -> Option<Self> {
        match value.to_ascii_uppercase().as_str() {
            "ACCEPTED" => Some(Self::Yes),
            "TENTATIVE" => Some(Self::Maybe),
            "DECLINED" => Some(Self::No),
            _ => None,
        }
    }
}

/// How often a meeting comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// A meeting that comes back, as far as the reader tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeat {
    pub every: Frequency,
    /// Every this many days, weeks, months or years: 1 or more.
    pub interval: u32,
    /// The weekdays it falls on, 0 for Monday to 6 for Sunday; empty when the rule names none.
    pub days: Vec<u8>,
}

/// What the invitation asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteKind {
    /// To come: it takes an answer.
    Request,
    /// The meeting is off.
    Cancel,
    /// Anything else, such as a published event: shown, not answered.
    Other,
}

/// A meeting from an invitation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub kind: InviteKind,
    /// The meeting's id; updates and replies keep it.
    pub uid: String,
    /// Which version of the meeting this is; a later update has a higher one.
    pub sequence: u32,
    pub summary: String,
    /// Seconds since the Unix epoch. A day-long meeting starts at 00:00 UTC of its first day.
    pub start: i64,
    /// As `start`, after it or at it.
    pub end: i64,
    /// It takes whole days, from `start` to `end`, not a time of day.
    pub all_day: bool,
    /// The organizer's offset from UTC at the start, in minutes, when the time is given in a
    /// time zone; None for UTC.
    pub offset: Option<i32>,
    pub location: String,
    /// Where to join it online.
    pub link: Option<String>,
    pub organizer: Option<Address>,
    /// Everyone asked but the organizer, with what each said as far as the invitation tells.
    pub attendees: Vec<(Address, Option<Answer>)>,
    pub repeats: Option<Repeat>,
}

impl Invite {
    /// The first meeting of an iCalendar text; None without one, or without a start.
    pub fn parse(calendar: &str) -> Option<Self> {
        let lines = unfold(calendar);
        let method = lines
            .iter()
            .take_while(|l| !l.is("BEGIN", "VEVENT"))
            .find(|l| l.name == "METHOD")
            .map(|l| l.value.to_ascii_uppercase());
        let event = component(&lines, "VEVENT")?;
        let zones = Zones::new(&lines);
        let prop = |name: &str| event.iter().find(|l| l.name == name);
        let text = |name: &str| prop(name).map(|l| unescape(&l.value)).unwrap_or_default();
        let start_line = prop("DTSTART")?;
        let (start, all_day, offset) = zones.time(start_line)?;
        let end = match (prop("DTEND"), prop("DURATION")) {
            (Some(end), _) => zones.time(end).map(|(t, ..)| t),
            (None, Some(duration)) => seconds(&duration.value).and_then(|d| start.checked_add(d)),
            (None, None) => None,
        }
        .unwrap_or(if all_day {
            start.saturating_add(DAY)
        } else {
            start
        })
        .max(start);
        let cancelled = prop("STATUS").is_some_and(|s| s.value.eq_ignore_ascii_case("CANCELLED"));
        let kind = match method.as_deref() {
            _ if cancelled => InviteKind::Cancel,
            Some("REQUEST") => InviteKind::Request,
            Some("CANCEL") => InviteKind::Cancel,
            _ => InviteKind::Other,
        };
        let organizer = prop("ORGANIZER").and_then(Line::address);
        let attendees = event
            .iter()
            .filter(|l| l.name == "ATTENDEE")
            .filter_map(|l| {
                Some((
                    l.address()?,
                    l.param("PARTSTAT").and_then(Answer::from_partstat),
                ))
            })
            .filter(|(a, _)| {
                !organizer
                    .as_ref()
                    .is_some_and(|o| o.email.eq_ignore_ascii_case(&a.email))
            })
            .collect();
        let location = text("LOCATION");
        let link = [
            "URL",
            "X-MICROSOFT-SKYPETEAMSMEETINGURL",
            "X-GOOGLE-CONFERENCE",
        ]
        .iter()
        .filter_map(|name| prop(name))
        .map(|l| l.value.trim().to_owned())
        .find(|v| v.starts_with("https://"))
        .or_else(|| meeting_link(&location))
        .or_else(|| meeting_link(&text("DESCRIPTION")));
        Some(Self {
            kind,
            uid: text("UID"),
            sequence: prop("SEQUENCE")
                .and_then(|l| l.value.trim().parse().ok())
                .unwrap_or(0),
            summary: text("SUMMARY").trim().to_owned(),
            start,
            end,
            all_day,
            offset,
            location: location.trim().to_owned(),
            link,
            organizer,
            attendees,
            repeats: prop("RRULE").and_then(|l| repeat(&l.value)),
        })
    }

    /// What `own` said, as far as the invitation tells.
    pub fn answer_of(&self, own: &str) -> Option<Answer> {
        self.attendees
            .iter()
            .find(|(a, _)| a.email.eq_ignore_ascii_case(own))
            .and_then(|(_, answer)| *answer)
    }
}

/// The iTIP reply of `attendee` to the meeting in `calendar`, as an iCalendar text with CRLF
/// lines: the meeting's own id, times and organizer, with `answer` and `comment`. `stamp` is
/// now, in seconds. None without a meeting in it.
pub fn reply(
    calendar: &str,
    attendee: &Address,
    answer: Answer,
    comment: &str,
    stamp: i64,
) -> Option<String> {
    let lines = unfold(calendar);
    let event = component(&lines, "VEVENT")?;
    let mut out = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "PRODID:-//Ozero Mail//EN".to_owned(),
        "VERSION:2.0".to_owned(),
        "METHOD:REPLY".to_owned(),
    ];
    // The zones the times name, as sent.
    let mut zone = false;
    for line in &lines {
        if line.is("BEGIN", "VTIMEZONE") {
            zone = true;
        }
        if zone {
            out.push(line.raw.clone());
        }
        if line.is("END", "VTIMEZONE") {
            zone = false;
        }
    }
    out.push("BEGIN:VEVENT".to_owned());
    const KEPT: [&str; 7] = [
        "UID",
        "SEQUENCE",
        "RECURRENCE-ID",
        "DTSTART",
        "DTEND",
        "DURATION",
        "ORGANIZER",
    ];
    for line in event.iter().filter(|l| KEPT.contains(&l.name.as_str())) {
        out.push(line.raw.clone());
    }
    if let Some(summary) = event.iter().find(|l| l.name == "SUMMARY") {
        out.push(summary.raw.clone());
    }
    out.push(format!("DTSTAMP:{}", utc_stamp(stamp)));
    let name = attendee
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| format!(";CN=\"{}\"", n.replace(['"', '\r', '\n'], "")))
        .unwrap_or_default();
    out.push(format!(
        "ATTENDEE;PARTSTAT={}{name}:mailto:{}",
        answer.partstat(),
        attendee.email
    ));
    let comment = comment.trim();
    if !comment.is_empty() {
        out.push(format!("COMMENT:{}", escape(comment)));
    }
    out.push("END:VEVENT".to_owned());
    out.push("END:VCALENDAR".to_owned());
    let mut text = String::new();
    for line in out {
        fold(&mut text, &line);
    }
    Some(text)
}

/// An invitation saved with its message's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedInvite {
    /// The iCalendar text as sent.
    pub calendar: String,
    /// What the person answered in the app, if they did.
    pub answer: Option<Answer>,
    /// When the meeting was before this update moved it: the start and end an earlier
    /// invitation to it says, if one is saved and says another.
    pub before: Option<(i64, i64)>,
}

/// A meeting of a day, from an invitation in the mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayEvent {
    /// The meeting's id.
    pub uid: String,
    /// The message with the newest invitation to it.
    pub message: MessageId,
    pub summary: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// What the person answered in the app, if they did.
    pub answer: Option<Answer>,
}

/// What a message list shows of an invitation in place of its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteRow {
    pub start: i64,
    pub all_day: bool,
    pub cancelled: bool,
    pub location: String,
}

const DAY: i64 = 86_400;

/// One content line, unfolded: `NAME;PARAM=value:value`.
#[derive(Debug, Clone)]
struct Line {
    /// Upper case.
    name: String,
    params: Vec<(String, String)>,
    value: String,
    /// The whole line as sent, unfolded.
    raw: String,
}

impl Line {
    fn parse(raw: &str) -> Option<Self> {
        // The value starts at the first colon outside quotes.
        let mut quoted = false;
        let colon = raw.char_indices().find_map(|(i, c)| {
            match c {
                '"' => quoted = !quoted,
                ':' if !quoted => return Some(i),
                _ => {}
            }
            None
        })?;
        let (head, value) = (&raw[..colon], &raw[colon + 1..]);
        let mut parts = split_unquoted(head, ';').into_iter();
        let name = parts.next()?.trim().to_ascii_uppercase();
        let params = parts
            .filter_map(|p| {
                let (k, v) = p.split_once('=')?;
                Some((
                    k.trim().to_ascii_uppercase(),
                    v.trim().trim_matches('"').to_owned(),
                ))
            })
            .collect();
        Some(Self {
            name,
            params,
            value: value.to_owned(),
            raw: raw.to_owned(),
        })
    }

    fn is(&self, name: &str, value: &str) -> bool {
        self.name == name && self.value.trim().eq_ignore_ascii_case(value)
    }

    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// A `mailto:` value with its `CN`.
    fn address(&self) -> Option<Address> {
        let value = self.value.trim();
        let email = value
            .get(..7)
            .filter(|s| s.eq_ignore_ascii_case("mailto:"))
            .map_or(value, |_| &value[7..])
            .trim();
        if !email.contains('@') {
            return None;
        }
        Some(Address {
            name: self
                .param("CN")
                .map(str::trim)
                .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(email))
                .map(str::to_owned),
            email: email.to_owned(),
        })
    }
}

fn split_unquoted(text: &str, at: char) -> Vec<&str> {
    let mut parts = vec![];
    let (mut start, mut quoted) = (0, false);
    for (i, c) in text.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == at && !quoted {
            parts.push(&text[start..i]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&text[start..]);
    parts
}

/// The content lines, folded lines joined back.
fn unfold(calendar: &str) -> Vec<Line> {
    let mut joined: Vec<String> = vec![];
    for line in calendar.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match (line.strip_prefix([' ', '\t']), joined.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ if line.trim().is_empty() => {}
            _ => joined.push(line.to_owned()),
        }
    }
    joined.iter().filter_map(|l| Line::parse(l)).collect()
}

/// The lines inside the first `name` component, nested ones aside.
fn component<'a>(lines: &'a [Line], name: &str) -> Option<&'a [Line]> {
    let start = lines.iter().position(|l| l.is("BEGIN", name))? + 1;
    let len = lines[start..].iter().position(|l| l.is("END", name))?;
    let inside = &lines[start..start + len];
    // A VALARM inside comes after the properties that matter.
    let end = inside
        .iter()
        .position(|l| l.name == "BEGIN")
        .unwrap_or(inside.len());
    Some(&inside[..end])
}

/// Text values: `\n`, `\,`, `\;` and `\\` as written.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(c) => out.push(c),
            None => {}
        }
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

/// Appends `line` folded at 75 octets, on a character boundary, with CRLF.
fn fold(out: &mut String, line: &str) {
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out.push_str("\r\n");
}

/// The link of a known meeting service in a text, such as a location or description.
fn meeting_link(text: &str) -> Option<String> {
    const HOSTS: [&str; 9] = [
        "telemost.yandex.",
        "telemost.360.yandex.",
        "zoom.us/",
        "teams.microsoft.com/",
        "teams.live.com/",
        "meet.google.com/",
        "webex.com/",
        "ktalk.ru/",
        "jazz.sber.ru/",
    ];
    text.match_indices("https://").find_map(|(at, _)| {
        let link = &text[at..];
        let end = link
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | ')'))
            .unwrap_or(link.len());
        let link = link[..end].trim_end_matches(['.', ',', ';']);
        let rest = &link[8..];
        HOSTS
            .iter()
            .any(|h| rest.contains(h))
            .then(|| link.to_owned())
    })
}

/// A `DURATION` value in seconds: `PT1H30M`, `P1D`, `P2W`.
fn seconds(value: &str) -> Option<i64> {
    let value = value.trim();
    let (sign, value) = match value.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, value.strip_prefix('+').unwrap_or(value)),
    };
    let mut rest = value.strip_prefix('P')?;
    let mut total = 0i64;
    let mut number = String::new();
    let mut time = false;
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            'T' => time = true,
            '0'..='9' => number.push(c),
            _ => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                let unit = match (c, time) {
                    ('W', _) => 7 * DAY,
                    ('D', _) => DAY,
                    ('H', true) => 3_600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
                // A length past any calendar is no length.
                total = total.checked_add(n.checked_mul(unit)?)?;
            }
        }
    }
    Some(sign * total)
}

/// What an `RRULE` says, as far as the reader tells it.
fn repeat(rule: &str) -> Option<Repeat> {
    let mut every = None;
    let mut interval = 1;
    let mut days = vec![];
    for part in rule.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                every = match value.trim().to_ascii_uppercase().as_str() {
                    "DAILY" => Some(Frequency::Daily),
                    "WEEKLY" => Some(Frequency::Weekly),
                    "MONTHLY" => Some(Frequency::Monthly),
                    "YEARLY" => Some(Frequency::Yearly),
                    _ => None,
                }
            }
            "INTERVAL" => interval = value.trim().parse().unwrap_or(1).max(1),
            "BYDAY" => {
                days = value
                    .split(',')
                    .filter_map(|d| {
                        weekday(d.trim_start_matches([
                            '+', '-', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
                        ]))
                    })
                    .collect();
                days.sort_unstable();
                days.dedup();
            }
            _ => {}
        }
    }
    Some(Repeat {
        every: every?,
        interval,
        days,
    })
}

/// 0 for `MO` to 6 for `SU`.
fn weekday(code: &str) -> Option<u8> {
    let at = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"]
        .iter()
        .position(|d| d.eq_ignore_ascii_case(code.trim()))?;
    u8::try_from(at).ok()
}

/// A date and time as written, without its zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Local {
    days: i64,
    seconds: i64,
}

impl Local {
    /// `20261029T110000`, with or without a trailing `Z`, or the date alone.
    fn parse(value: &str) -> Option<(Self, bool)> {
        let value = value.trim().trim_end_matches(['Z', 'z']);
        let (date, time) = match value.split_once(['T', 't']) {
            Some((date, time)) => (date, Some(time)),
            None => (value, None),
        };
        let digits = |s: &str, from: usize, len: usize| -> Option<i64> {
            let part = s.get(from..from + len)?;
            part.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| part.parse().ok())
                .flatten()
        };
        if date.len() != 8 {
            return None;
        }
        let (y, m, d) = (
            digits(date, 0, 4)?,
            digits(date, 4, 2)?,
            digits(date, 6, 2)?,
        );
        if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        let seconds = match time {
            Some(t) if t.len() >= 6 => {
                digits(t, 0, 2)? * 3_600 + digits(t, 2, 2)? * 60 + digits(t, 4, 2)?
            }
            Some(_) => return None,
            None => 0,
        };
        Some((
            Self {
                days: days_from_civil(y, m, d),
                seconds,
            },
            time.is_none(),
        ))
    }

    fn timestamp(self) -> i64 {
        self.days * DAY + self.seconds
    }
}

/// Days since 1970-01-01 of a date of the proleptic Gregorian calendar.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The year, month and day of a day since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

/// `20261029T080000Z` for a time in seconds.
fn utc_stamp(at: i64) -> String {
    let (y, m, d) = civil_from_days(at.div_euclid(DAY));
    let s = at.rem_euclid(DAY);
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        s / 3_600,
        s / 60 % 60,
        s % 60
    )
}

/// The time zones a calendar defines, by `TZID`.
struct Zones {
    zones: Vec<(String, Vec<Onset>)>,
}

/// When a zone starts keeping an offset: once, or every year by a rule.
#[derive(Debug, Clone)]
struct Onset {
    /// As written, in the offset before it.
    first: Local,
    /// Minutes from UTC from then on.
    offset: i32,
    /// The month and the nth (from the end, if negative) weekday it comes back every year.
    yearly: Option<(i64, i64, u8)>,
    /// The last time it comes back, for a rule a later one replaced.
    until: Option<Local>,
}

impl Onset {
    /// From the lines of a `STANDARD` or `DAYLIGHT` part of a zone.
    fn new(lines: &[&Line]) -> Option<Self> {
        let value = |name: &str| {
            lines
                .iter()
                .find(|l| l.name == name)
                .map(|l| l.value.as_str())
        };
        Some(Self {
            first: Local::parse(value("DTSTART")?)?.0,
            offset: offset_minutes(value("TZOFFSETTO")?)?,
            yearly: value("RRULE").and_then(yearly_rule),
            until: value("RRULE").and_then(|rule| {
                let (_, until) = rule.split(';').find_map(|p| {
                    p.split_once('=')
                        .filter(|(k, _)| k.trim().eq_ignore_ascii_case("UNTIL"))
                })?;
                Some(Local::parse(until)?.0)
            }),
        })
    }
}

impl Zones {
    fn new(lines: &[Line]) -> Self {
        let mut zones: Vec<(String, Vec<Onset>)> = vec![];
        let mut iter = lines.iter();
        while let Some(line) = iter.next() {
            if !line.is("BEGIN", "VTIMEZONE") {
                continue;
            }
            let mut id = String::new();
            let mut onsets = vec![];
            let mut current: Option<Vec<&Line>> = None;
            for line in iter.by_ref() {
                if line.is("END", "VTIMEZONE") {
                    break;
                }
                match (line.name.as_str(), current.as_mut()) {
                    ("TZID", None) => id = line.value.trim().to_owned(),
                    ("BEGIN", _) => current = Some(vec![]),
                    ("END", Some(_)) => onsets.extend(current.take().and_then(|c| Onset::new(&c))),
                    (_, Some(part)) => part.push(line),
                    _ => {}
                }
            }
            zones.push((id, onsets));
        }
        Self { zones }
    }

    /// A `DTSTART` or `DTEND` line in seconds since the epoch, whether it is a date alone, and
    /// the offset of the zone it names, if this calendar defines it.
    fn time(&self, line: &Line) -> Option<(i64, bool, Option<i32>)> {
        let utc = line.value.trim().ends_with(['Z', 'z']);
        let (local, date) = Local::parse(&line.value)?;
        if date || utc {
            return Some((local.timestamp(), date, None));
        }
        let offset = self.offset_at(line.param("TZID"), local);
        Some((
            local.timestamp() - i64::from(offset.unwrap_or(0)) * 60,
            false,
            offset,
        ))
    }

    fn offset_at(&self, zone: Option<&str>, at: Local) -> Option<i32> {
        let zone = zone?.trim();
        let (_, onsets) = self.zones.iter().find(|(id, _)| id == zone)?;
        let year = civil_from_days(at.days).0;
        let mut latest: Option<(i64, i32)> = None;
        for onset in onsets {
            let times: Vec<Local> = match onset.yearly {
                Some((month, nth, day)) => [year - 1, year]
                    .iter()
                    .filter_map(|&y| nth_weekday(y, month, nth, day))
                    .map(|days| Local {
                        days,
                        seconds: onset.first.seconds,
                    })
                    .filter(|l| l.timestamp() >= onset.first.timestamp())
                    .filter(|l| onset.until.is_none_or(|u| l.timestamp() <= u.timestamp()))
                    .collect(),
                None => vec![onset.first],
            };
            for t in times {
                let t = t.timestamp();
                if t <= at.timestamp() && latest.is_none_or(|(l, _)| t > l) {
                    latest = Some((t, onset.offset));
                }
            }
        }
        latest
            .map(|(_, offset)| offset)
            .or_else(|| onsets.first().map(|o| o.offset))
    }
}

/// `+0300` or `-0130` in minutes.
fn offset_minutes(value: &str) -> Option<i32> {
    let value = value.trim();
    let (sign, digits) = match value.split_at_checked(1)? {
        ("+", d) => (1, d),
        ("-", d) => (-1, d),
        _ => return None,
    };
    let hours: i32 = digits.get(..2)?.parse().ok()?;
    let minutes: i32 = digits.get(2..4)?.parse().ok()?;
    Some(sign * (hours * 60 + minutes))
}

/// `FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU` as the month, the nth weekday and the weekday.
fn yearly_rule(rule: &str) -> Option<(i64, i64, u8)> {
    let mut month = None;
    let mut by_day = None;
    for part in rule.split(';') {
        let (key, value) = part.split_once('=')?;
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" if !value.trim().eq_ignore_ascii_case("YEARLY") => return None,
            "BYMONTH" => month = value.trim().parse::<i64>().ok(),
            "BYDAY" => {
                let value = value.trim();
                let split = value.len().checked_sub(2)?;
                let (nth, day) = value.split_at_checked(split)?;
                let nth = match nth.trim_start_matches('+') {
                    "" => 1,
                    n => n.parse::<i64>().ok()?,
                };
                by_day = Some((nth, weekday(day)?));
            }
            _ => {}
        }
    }
    let (nth, day) = by_day?;
    Some((month?, nth, day))
}

/// The day (since the epoch) of the `nth` `weekday` (0 for Monday) of a month; a negative
/// `nth` counts from the month's end.
fn nth_weekday(year: i64, month: i64, nth: i64, weekday: u8) -> Option<i64> {
    if !(1..=12).contains(&month) || nth == 0 {
        return None;
    }
    let weekday = i64::from(weekday);
    let first = days_from_civil(year, month, 1);
    let next_month = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    // 1970-01-01 was a Thursday: 3 when Monday is 0.
    let day_of = |days: i64| (days + 3).rem_euclid(7);
    let day = if nth > 0 {
        first + (weekday - day_of(first)).rem_euclid(7) + (nth - 1) * 7
    } else {
        let last = next_month - 1;
        last - (day_of(last) - weekday).rem_euclid(7) + (nth + 1) * 7
    };
    (first..next_month).contains(&day).then_some(day)
}

#[cfg(test)]
mod tests;
