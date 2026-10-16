/// Inbox tab a message is sorted into by on-device rules (no AI).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Category {
    #[default]
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
    /// Machines first: services often send through mailing lists too.
    pub fn of(s: &Signals<'_>) -> Self {
        let local = s.from.rsplit_once('@').map_or(s.from, |(local, _)| local);
        let robot = ROBOTS
            .iter()
            .any(|r| local.eq_ignore_ascii_case(r) || starts_with(local, r));
        let auto = s
            .auto_submitted
            .is_some_and(|v| !v.trim().eq_ignore_ascii_case("no"));
        let precedence = s.precedence.map(|v| v.trim().to_ascii_lowercase());
        if robot || auto || precedence.as_deref() == Some("auto_reply") {
            Self::Notifications
        } else if s.list_id.is_some()
            || s.list_unsubscribe.is_some()
            || matches!(precedence.as_deref(), Some("bulk" | "list" | "junk"))
        {
            Self::Newsletters
        } else {
            Self::People
        }
    }
}

/// `noreply+abc` and `noreply.billing` count as `noreply`.
fn starts_with(local: &str, robot: &str) -> bool {
    local.len() > robot.len()
        && local.is_char_boundary(robot.len())
        && local[..robot.len()].eq_ignore_ascii_case(robot)
        && matches!(local.as_bytes()[robot.len()], b'+' | b'.' | b'-' | b'_')
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
        let human = Signals {
            auto_submitted: Some("no"),
            ..auto
        };
        assert_eq!(Category::of(&human), Category::People);
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
        // A service that sends through a list is still a notification.
        let service = Signals {
            from: "notifications@github.com",
            ..list
        };
        assert_eq!(Category::of(&service), Category::Notifications);
    }
}
