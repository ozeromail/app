use crate::MessageLocation;

/// A change the user makes on the device first; the server gets it from the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    /// Marks the message read (`true`) or unread.
    Seen(bool),
    /// Moves the message out of its folder into the account's archive.
    Archive,
    /// Moves the message out of its folder into the account's trash.
    Trash,
    /// Moves the message back into the account's inbox, out of the archive, trash, spam or
    /// snoozed folder.
    Inbox,
    /// Moves the message out of its folder into the account's snoozed folder, until it is
    /// time for it to come back.
    Snooze,
    /// Removes the message from the server for good: from the trash only.
    Delete,
    /// Empties the trash or spam the message is in, for good: everything the server has
    /// there but the messages the app still keeps. Queued on each message the person saw go.
    Empty,
}

impl Action {
    /// Whether the message leaves its folder: everything but marking it read or unread.
    pub fn removes(self) -> bool {
        !matches!(self, Action::Seen(_))
    }
}

/// Local id of a queued action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpId(pub i64);

/// A queued action that the message's server does not have yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub id: OpId,
    pub action: Action,
    pub location: MessageLocation,
    /// The folder a move goes to, chosen when it was queued; None if that folder is gone.
    pub to: Option<String>,
}
