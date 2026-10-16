/// Inbox tab a message is sorted into by on-device rules (no AI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    People,
    Notifications,
    Newsletters,
}

/// The headers the rules read. Values are as sent, without the header name.
#[derive(Debug, Clone, Copy, Default)]
pub struct Signals<'a> {
    pub from: &'a str,
    pub list_id: Option<&'a str>,
    pub list_unsubscribe: Option<&'a str>,
    pub list_unsubscribe_post: Option<&'a str>,
    pub precedence: Option<&'a str>,
    pub auto_submitted: Option<&'a str>,
}

/// Sender names used only by machines.
const ROBOTS: [&str; 12] = [
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "do-not-reply",
    "notification",
    "notifications",
    "notify",
    "alert",
    "alerts",
    "mailer-daemon",
    "postmaster",
];

impl Category {
    /// The tabs in the order the apps show them.
    pub const ALL: [Self; 3] = [Self::People, Self::Notifications, Self::Newsletters];

    /// Machines first: services often send through mailing lists too. A discussion list
    /// (a `List-Id` without bulk signs) is people talking, so it stays in People.
    pub fn of(s: &Signals<'_>) -> Self {
        let local = s.from.rsplit_once('@').map_or(s.from, |(local, _)| local);
        let robot = ROBOTS.iter().any(|r| is_robot(local, r));
        // `no (sent by hand)` is still `no`; RFC 3834 allows a comment after the keyword.
        let auto = s.auto_submitted.is_some_and(|v| {
            let keyword = v
                .split(|c: char| c == '(' || c == ';' || c.is_whitespace())
                .find(|k| !k.is_empty())
                .unwrap_or("");
            !keyword.is_empty() && !keyword.eq_ignore_ascii_case("no")
        });
        let precedence = |values: &[&str]| {
            s.precedence
                .is_some_and(|p| values.iter().any(|v| p.trim().eq_ignore_ascii_case(v)))
        };
        if robot || auto || precedence(&["auto_reply"]) {
            Self::Notifications
        } else if precedence(&["bulk", "junk"])
            || s.list_unsubscribe_post.is_some()
            || (s.list_unsubscribe.is_some() && s.list_id.is_none())
        {
            Self::Newsletters
        } else {
            Self::People
        }
    }
}

/// `noreply`, and also `noreply+abc` or `noreply.billing`.
fn is_robot(local: &str, robot: &str) -> bool {
    local
        .get(..robot.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(robot))
        && local
            .as_bytes()
            .get(robot.len())
            .is_none_or(|b| matches!(b, b'+' | b'.' | b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from(email: &str) -> Category {
        Category::of(&Signals {
            from: email,
            ..Signals::default()
        })
    }

    #[test]
    fn a_person_stays_in_people() {
        assert_eq!(from("anna@example.com"), Category::People);
        assert_eq!(from("notifyme@example.com"), Category::People);
        assert_eq!(from(""), Category::People);
    }

    #[test]
    fn machine_senders_are_notifications() {
        assert_eq!(from("noreply@bank.example"), Category::Notifications);
        assert_eq!(from("No-Reply@shop.example"), Category::Notifications);
        assert_eq!(
            from("notifications+abc@github.com"),
            Category::Notifications
        );
        let auto = Signals {
            from: "it@example.com",
            auto_submitted: Some(" auto-generated"),
            ..Signals::default()
        };
        assert_eq!(Category::of(&auto), Category::Notifications);
        for value in ["no", " No (sent by hand)", ""] {
            let human = Signals {
                auto_submitted: Some(value),
                ..auto
            };
            assert_eq!(Category::of(&human), Category::People, "{value:?}");
        }
    }

    #[test]
    fn lists_are_newsletters() {
        let list = Signals {
            from: "news@shop.example",
            list_unsubscribe: Some("<https://shop.example/u>"),
            ..Signals::default()
        };
        assert_eq!(Category::of(&list), Category::Newsletters);
        let bulk = Signals {
            from: "digest@blog.example",
            precedence: Some(" Bulk"),
            ..Signals::default()
        };
        assert_eq!(Category::of(&bulk), Category::Newsletters);
        let one_click = Signals {
            from: "news@shop.example",
            list_id: Some("<news.shop.example>"),
            list_unsubscribe_post: Some("List-Unsubscribe=One-Click"),
            ..Signals::default()
        };
        assert_eq!(Category::of(&one_click), Category::Newsletters);
        // A colleague writing to a team list.
        let discussion = Signals {
            from: "anna@example.com",
            list_id: Some("<team.example.com>"),
            list_unsubscribe: Some("<mailto:team+unsubscribe@example.com>"),
            precedence: Some("list"),
            ..Signals::default()
        };
        assert_eq!(Category::of(&discussion), Category::People);
        // A service that sends through a list is still a notification.
        let service = Signals {
            from: "notifications@github.com",
            ..list
        };
        assert_eq!(Category::of(&service), Category::Notifications);
    }
}
