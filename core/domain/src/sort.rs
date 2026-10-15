/// Inbox tab a message is sorted into by on-device rules (no AI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    People,
    Notifications,
    Newsletters,
}
