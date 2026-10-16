use crate::MessageLocation;

/// A change the user makes on the device first; the server gets it from the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    /// Marks the message read (`true`) or unread.
    Seen(bool),
}

/// Local id of a queued action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpId(pub i64);

/// A queued action that the message's server does not have yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub id: OpId,
    pub action: Action,
    pub location: MessageLocation,
}
