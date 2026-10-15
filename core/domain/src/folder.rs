use crate::AccountId;

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
    /// A user-created folder or label.
    Custom,
}

impl FolderRole {
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
}
