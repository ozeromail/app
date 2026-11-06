use crate::{AccountId, Action};

/// Local id of a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderId(pub i64);

/// What a folder means to the user, independent of how a provider names it.
///
/// IMAP special-use flags, Gmail system labels and Graph well-known folder
/// names all map onto these roles, so use cases never branch on provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Trash,
    Junk,
    Flagged,
    /// Where snoozed mail waits until it comes back to the inbox: a folder the app makes,
    /// named `SNOOZED`.
    Snoozed,
    /// A user-created folder or label.
    Custom,
}

/// The names of the folder the app keeps snoozed mail in, the second for a server that takes
/// "Snoozed" for its own, as Gmail does.
pub const SNOOZED: [&str; 2] = ["Snoozed", "Ozero/Snoozed"];

impl FolderRole {
    /// The role of a folder that has no special use, by its full name: one of `SNOOZED`, in
    /// any case, is the snoozed folder.
    pub fn from_name(name: &str) -> Self {
        if SNOOZED.iter().any(|s| name.eq_ignore_ascii_case(s)) {
            Self::Snoozed
        } else {
            Self::Custom
        }
    }

    /// Maps an IMAP special-use attribute (RFC 6154) to a role.
    pub fn from_special_use(attr: &str) -> Option<Self> {
        let role = match attr.to_ascii_lowercase().as_str() {
            "\\sent" => Self::Sent,
            "\\drafts" => Self::Drafts,
            "\\archive" => Self::Archive,
            "\\trash" => Self::Trash,
            "\\junk" => Self::Junk,
            "\\flagged" => Self::Flagged,
            _ => return None,
        };
        Some(role)
    }

    /// Whether `action` applies to a message in a folder with this role: Done archives only
    /// mail that is not filed yet and so does Snooze, nothing goes to the trash from the trash,
    /// a delete in the trash is for good, only the trash and spam are emptied, and mail goes
    /// back to the inbox from the archive, trash, spam or snoozed folder. Mail moves to another
    /// folder from where it is read, not from the sent, drafts or snoozed folder.
    pub fn allows(self, action: Action) -> bool {
        match action {
            Action::Seen(_) | Action::Flagged(_) => true,
            // An invitation thrown away is not asked about; Exchange keeps answered ones there.
            Action::Answer(_) => {
                !matches!(self, Self::Sent | Self::Drafts | Self::Trash | Self::Junk)
            }
            Action::Spam => matches!(self, Self::Inbox | Self::Custom | Self::Archive),
            Action::Archive | Action::Snooze => matches!(self, Self::Inbox | Self::Custom),
            Action::Move(_) => matches!(
                self,
                Self::Inbox | Self::Custom | Self::Archive | Self::Trash | Self::Junk
            ),
            Action::Trash => self != Self::Trash,
            Action::Delete => self == Self::Trash,
            Action::Empty => matches!(self, Self::Trash | Self::Junk),
            Action::Inbox => {
                matches!(
                    self,
                    Self::Archive | Self::Trash | Self::Junk | Self::Snoozed
                )
            }
        }
    }

    /// Whether `Action::Move` can take mail into a folder with this role: the inbox, the
    /// archive and the user's own folders; the rest have actions of their own.
    pub fn takes_moves(self) -> bool {
        matches!(self, Self::Inbox | Self::Archive | Self::Custom)
    }

    /// Messages in these folders take part in Inbox Zero counters.
    pub fn counts_unread(self) -> bool {
        matches!(self, Self::Inbox | Self::Custom)
    }
}

/// A folder as the server reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFolder {
    /// Provider id: the IMAP mailbox name, a Gmail label id or a Graph folder id.
    pub remote_id: String,
    pub name: String,
    pub role: FolderRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub account_id: AccountId,
    pub remote_id: String,
    pub name: String,
    pub role: FolderRole,
    /// The person pinned it, so the apps show it with the account's usual folders.
    pub pinned: bool,
    /// Its place among the account's pinned folders, as the person ordered them; None for
    /// one pinned before they did, which goes after them by name.
    pub rank: Option<u32>,
}

impl Folder {
    /// Whether its server has it: not one made on the device, see [`on_server`].
    pub fn on_server(&self) -> bool {
        on_server(&self.remote_id)
    }
}

/// Whether a folder's `remote_id` is the server's: `Store::create_folder` gives one made on
/// the device an id starting with `:new:` until the server gives it one.
pub fn on_server(remote_id: &str) -> bool {
    !remote_id.starts_with(":new:")
}

/// What the account's folder names split their paths with: `.` on servers that keep folders
/// under INBOX, `/` on the rest and for labels. Only one of them, so a name such as
/// "Dr. Who" stays whole.
pub fn folder_separator<'a>(mut names: impl Iterator<Item = &'a str>) -> char {
    if names.any(|n| n.starts_with("INBOX.")) {
        '.'
    } else {
        '/'
    }
}

/// What the person did to one of their own folders on the device that its server does not
/// have yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderEdit {
    /// Made it.
    Create,
    /// Renamed it or put it in another folder.
    Change,
    /// Deleted it, with its mail.
    Delete,
}

/// A folder change waiting for the server, with what the server needs to make it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderChange {
    pub folder: FolderId,
    pub edit: FolderEdit,
    /// The folder's id on the server; none yet for one being made.
    pub remote_id: String,
    /// The id of the folder it goes inside; None at the top level.
    pub parent: Option<String>,
    /// Its own name, without the folders it is inside.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_use_is_case_insensitive() {
        assert_eq!(
            FolderRole::from_special_use("\\Sent"),
            Some(FolderRole::Sent)
        );
        assert_eq!(
            FolderRole::from_special_use("\\ARCHIVE"),
            Some(FolderRole::Archive)
        );
        // \All holds every message, so it is not an archive.
        assert_eq!(FolderRole::from_special_use("\\All"), None);
        assert_eq!(FolderRole::from_special_use("\\Noselect"), None);
    }

    #[test]
    fn moves_only_where_they_mean_something() {
        assert!(FolderRole::Inbox.allows(Action::Archive));
        assert!(!FolderRole::Archive.allows(Action::Archive));
        assert!(FolderRole::Archive.allows(Action::Trash));
        assert!(!FolderRole::Trash.allows(Action::Trash));
        assert!(FolderRole::Trash.allows(Action::Delete));
        assert!(!FolderRole::Junk.allows(Action::Delete));
        assert!(FolderRole::Junk.allows(Action::Empty));
        assert!(!FolderRole::Inbox.allows(Action::Empty));
        assert!(!FolderRole::Sent.allows(Action::Archive));
        assert!(FolderRole::Archive.allows(Action::Move(FolderId(1))));
        assert!(!FolderRole::Sent.allows(Action::Move(FolderId(1))));
        assert!(FolderRole::Inbox.allows(Action::Snooze));
        assert!(!FolderRole::Snoozed.allows(Action::Snooze));
        assert!(!FolderRole::Sent.allows(Action::Snooze));
        for role in [
            FolderRole::Archive,
            FolderRole::Trash,
            FolderRole::Junk,
            FolderRole::Snoozed,
        ] {
            assert!(role.allows(Action::Inbox));
        }
        for role in [
            FolderRole::Inbox,
            FolderRole::Sent,
            FolderRole::Drafts,
            FolderRole::Custom,
        ] {
            assert!(!role.allows(Action::Inbox));
        }
        assert!(FolderRole::Sent.allows(Action::Seen(true)));
    }

    #[test]
    fn the_snoozed_folder_is_found_by_name() {
        assert_eq!(FolderRole::from_name("Snoozed"), FolderRole::Snoozed);
        assert_eq!(FolderRole::from_name("SNOOZED"), FolderRole::Snoozed);
        assert_eq!(FolderRole::from_name("Ozero/Snoozed"), FolderRole::Snoozed);
        assert_eq!(FolderRole::from_name("Work/Snoozed"), FolderRole::Custom);
        assert_eq!(FolderRole::from_name("Snoozed mail"), FolderRole::Custom);
    }
}
