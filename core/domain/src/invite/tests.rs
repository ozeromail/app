use super::*;

/// As Exchange sends it: a Windows zone with summer time, a folded description and an alarm.
const EXCHANGE: &str = "BEGIN:VCALENDAR\r
METHOD:REQUEST\r
PRODID:Microsoft Exchange Server 2016\r
VERSION:2.0\r
BEGIN:VTIMEZONE\r
TZID:W. Europe Standard Time\r
BEGIN:STANDARD\r
DTSTART:16010101T030000\r
TZOFFSETFROM:+0200\r
TZOFFSETTO:+0100\r
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=10\r
END:STANDARD\r
BEGIN:DAYLIGHT\r
DTSTART:16010101T020000\r
TZOFFSETFROM:+0100\r
TZOFFSETTO:+0200\r
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=3\r
END:DAYLIGHT\r
END:VTIMEZONE\r
BEGIN:VEVENT\r
ORGANIZER;CN=\"Смирнова Ольга\":mailto:o.smirnova@tbank.ru\r
ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=Khizri:mailto:k.\r
 makhmudov@tbank.ru\r
ATTENDEE;ROLE=OPT-PARTICIPANT;PARTSTAT=ACCEPTED;CN=Ivan:mailto:ivan@tbank.ru\r
DESCRIPTION;LANGUAGE=ru-RU:Обсудим цели\\, хвосты.\\nСсылка: https://telemost.\r
 yandex.ru/j/123456.\r
UID:040000008200E00074C5B7101A82E008000000\r
SUMMARY;LANGUAGE=ru-RU:Планирование спринта 46\r
DTSTART;TZID=W. Europe Standard Time:20261029T110000\r
DTEND;TZID=W. Europe Standard Time:20261029T120000\r
SEQUENCE:2\r
LOCATION;LANGUAGE=ru-RU:Переговорка «Байкал»\\, 7 этаж\r
STATUS:CONFIRMED\r
BEGIN:VALARM\r
TRIGGER:-PT15M\r
END:VALARM\r
END:VEVENT\r
END:VCALENDAR\r
";

#[test]
fn reads_an_exchange_invitation() {
    let invite = Invite::parse(EXCHANGE).unwrap();
    assert_eq!(invite.kind, InviteKind::Request);
    assert_eq!(invite.uid, "040000008200E00074C5B7101A82E008000000");
    assert_eq!(invite.sequence, 2);
    assert_eq!(invite.summary, "Планирование спринта 46");
    // 29 October 2026 is after the last Sunday of October: winter time, UTC+1.
    assert_eq!(
        invite.start,
        days_from_civil(2026, 10, 29) * DAY + 10 * 3_600
    );
    assert_eq!(invite.end - invite.start, 3_600);
    assert_eq!(invite.offset, Some(60));
    assert!(!invite.all_day);
    assert_eq!(invite.location, "Переговорка «Байкал», 7 этаж");
    assert_eq!(
        invite.link.as_deref(),
        Some("https://telemost.yandex.ru/j/123456")
    );
    let organizer = invite.organizer.clone().unwrap();
    assert_eq!(organizer.name.as_deref(), Some("Смирнова Ольга"));
    assert_eq!(organizer.email, "o.smirnova@tbank.ru");
    assert_eq!(invite.attendees.len(), 2);
    assert_eq!(invite.answer_of("K.Makhmudov@tbank.ru"), None);
    assert_eq!(invite.answer_of("ivan@tbank.ru"), Some(Answer::Yes));
}

#[test]
fn keeps_summer_time_apart() {
    let summer = EXCHANGE.replace("20261029T", "20260701T");
    let invite = Invite::parse(&summer).unwrap();
    assert_eq!(invite.offset, Some(120));
    assert_eq!(invite.start, days_from_civil(2026, 7, 1) * DAY + 9 * 3_600);
}

#[test]
fn reads_utc_dates_durations_and_repeats() {
    let calendar = "BEGIN:VCALENDAR\nMETHOD:REQUEST\nBEGIN:VEVENT\nUID:a@google.com\n\
DTSTART:20261103T090000Z\nDURATION:PT1H30M\nRRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=TU,TH\n\
ORGANIZER:mailto:anna@gmail.com\nATTENDEE;PARTSTAT=TENTATIVE:mailto:me@example.com\n\
END:VEVENT\nEND:VCALENDAR\n";
    let invite = Invite::parse(calendar).unwrap();
    assert_eq!(invite.start, days_from_civil(2026, 11, 3) * DAY + 9 * 3_600);
    assert_eq!(invite.end - invite.start, 5_400);
    assert_eq!(invite.offset, None);
    assert_eq!(invite.organizer.as_ref().unwrap().name, None);
    assert_eq!(invite.answer_of("me@example.com"), Some(Answer::Maybe));
    assert_eq!(
        invite.repeats,
        Some(Repeat {
            every: Frequency::Weekly,
            interval: 2,
            days: vec![1, 3],
        })
    );
}

#[test]
fn reads_days_and_cancellations() {
    let calendar = "BEGIN:VCALENDAR\r\nMETHOD:CANCEL\r\nBEGIN:VEVENT\r\nUID:x\r\n\
DTSTART;VALUE=DATE:20261231\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let invite = Invite::parse(calendar).unwrap();
    assert_eq!(invite.kind, InviteKind::Cancel);
    assert!(invite.all_day);
    assert_eq!(invite.start, days_from_civil(2026, 12, 31) * DAY);
    assert_eq!(invite.end, invite.start + DAY);
    assert_eq!(Invite::parse("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"), None);
}

#[test]
fn replies_with_the_meeting_as_sent() {
    let me = Address {
        name: Some("Khizri".into()),
        email: "k.makhmudov@tbank.ru".into(),
    };
    let stamp = days_from_civil(2026, 10, 27) * DAY + 17 * 3_600;
    let text = reply(
        EXCHANGE,
        &me,
        Answer::No,
        "Буду в отпуске, напишу заметки.",
        stamp,
    )
    .unwrap();
    assert!(text.lines().all(|l| l.len() <= 76));
    assert!(text.contains("METHOD:REPLY\r\n"));
    assert!(text.contains("BEGIN:VTIMEZONE\r\nTZID:W. Europe Standard Time\r\n"));
    assert!(text.contains("DTSTART;TZID=W. Europe Standard Time:20261029T110000\r\n"));
    assert!(text.contains("SEQUENCE:2\r\n"));
    assert!(text.contains("DTSTAMP:20261027T170000Z\r\n"));
    assert!(
        text.contains("ATTENDEE;PARTSTAT=DECLINED;CN=\"Khizri\":mailto:k.makhmudov@tbank.ru\r\n")
    );
    assert!(!text.contains("VALARM"));
    // Read back, the reply is the same meeting.
    let back = Invite::parse(&text).unwrap();
    assert_eq!(back.kind, InviteKind::Other);
    assert_eq!(back.start, Invite::parse(EXCHANGE).unwrap().start);
    assert_eq!(back.answer_of("k.makhmudov@tbank.ru"), Some(Answer::No));
    let comment = text.replace("\r\n ", "");
    assert!(comment.contains("COMMENT:Буду в отпуске\\, напишу заметки."));
}

#[test]
fn finds_the_nth_weekday() {
    // The last Sunday of March 2026 is the 29th; the first Monday of June the 1st.
    assert_eq!(
        nth_weekday(2026, 3, -1, 6),
        Some(days_from_civil(2026, 3, 29))
    );
    assert_eq!(
        nth_weekday(2026, 6, 1, 0),
        Some(days_from_civil(2026, 6, 1))
    );
    assert_eq!(nth_weekday(2026, 2, 5, 0), None);
    assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
}

#[test]
fn reads_durations() {
    assert_eq!(seconds("PT15M"), Some(900));
    assert_eq!(seconds("P1DT1H"), Some(DAY + 3_600));
    assert_eq!(seconds("-PT5M"), Some(-300));
    assert_eq!(seconds("1H"), None);
}

#[test]
fn a_zone_rule_ended_by_until_no_longer_applies() {
    // As Apple writes New York: the rules before 2007 next to today's.
    let calendar = "BEGIN:VCALENDAR\r
METHOD:REQUEST\r
BEGIN:VTIMEZONE\r
TZID:America/New_York\r
BEGIN:DAYLIGHT\r
DTSTART:19870405T020000\r
TZOFFSETFROM:-0500\r
TZOFFSETTO:-0400\r
RRULE:FREQ=YEARLY;UNTIL=20060402T070000Z;BYMONTH=4;BYDAY=1SU\r
END:DAYLIGHT\r
BEGIN:STANDARD\r
DTSTART:19671029T020000\r
TZOFFSETFROM:-0400\r
TZOFFSETTO:-0500\r
RRULE:FREQ=YEARLY;UNTIL=20061029T060000Z;BYMONTH=10;BYDAY=-1SU\r
END:STANDARD\r
BEGIN:DAYLIGHT\r
DTSTART:20070311T020000\r
TZOFFSETFROM:-0500\r
TZOFFSETTO:-0400\r
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU\r
END:DAYLIGHT\r
BEGIN:STANDARD\r
DTSTART:20071104T020000\r
TZOFFSETFROM:-0400\r
TZOFFSETTO:-0500\r
RRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU\r
END:STANDARD\r
END:VTIMEZONE\r
BEGIN:VEVENT\r
UID:ny\r
DTSTART;TZID=America/New_York:20261028T100000\r
DTEND;TZID=America/New_York:20261028T110000\r
SUMMARY:Sync\r
END:VEVENT\r
END:VCALENDAR\r
";
    let invite = Invite::parse(calendar).unwrap();
    // Still summer time on 28 October: 10:00 is 14:00 UTC.
    assert_eq!(invite.offset, Some(-240));
    assert_eq!(invite.start, utc_seconds(2026, 10, 28, 14));
}

#[test]
fn a_length_past_any_calendar_is_no_length() {
    assert_eq!(seconds("P999999999999999W"), None);
    assert_eq!(seconds("PT1H30M"), Some(5_400));
}

fn utc_seconds(year: i64, month: i64, day: i64, hour: i64) -> i64 {
    days_from_civil(year, month, day) * DAY + hour * 3_600
}
