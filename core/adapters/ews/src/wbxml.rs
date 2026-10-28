//! WAP Binary XML as ActiveSync speaks it (MS-ASWBXML): tags from code pages, inline
//! strings and opaque data, no attributes and no string table.

/// One element: its tag as `Page:Name`, its children, its content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    /// Empty for a tag this crate does not know.
    pub name: &'static str,
    pub children: Vec<Node>,
    /// The text or opaque data in it, as bytes: a message as MIME need not be UTF-8.
    pub data: Vec<u8>,
}

impl Node {
    pub fn new(name: &'static str, children: Vec<Node>) -> Self {
        Self {
            name,
            children,
            data: vec![],
        }
    }

    pub fn text(name: &'static str, data: impl Into<Vec<u8>>) -> Self {
        Self {
            name,
            children: vec![],
            data: data.into(),
        }
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn at(&self, path: &[&str]) -> Option<&Node> {
        path.iter().try_fold(self, |node, name| node.child(name))
    }

    /// Takes the element at `path` out of this one.
    pub fn take_at(&mut self, path: &[&str]) -> Option<Node> {
        let (last, path) = path.split_last()?;
        let mut node = self;
        for name in path {
            node = node.children.iter_mut().find(|c| c.name == *name)?;
        }
        let at = node.children.iter().position(|c| c.name == *last)?;
        Some(node.children.remove(at))
    }

    /// The text at `path`, empty if there is none.
    pub fn text_at(&self, path: &[&str]) -> &str {
        self.at(path)
            .and_then(|n| std::str::from_utf8(&n.data).ok())
            .unwrap_or("")
    }
}

const SWITCH_PAGE: u8 = 0x00;
const END: u8 = 0x01;
const ENTITY: u8 = 0x02;
const STR_I: u8 = 0x03;
const LITERAL: u8 = 0x04;
const OPAQUE: u8 = 0xc3;
const CONTENT: u8 = 0x40;
const ATTRIBUTES: u8 = 0x80;
/// Deepest nesting read: ActiveSync answers go a few levels down.
const MAX_DEPTH: usize = 32;
/// Tags the server takes only as opaque data.
const OPAQUE_TAGS: [&str; 1] = ["ComposeMail:Mime"];

/// `node` as a WBXML document: version 1.3, unknown public id, UTF-8. A tag this crate does
/// not know is left out.
pub fn encode(node: &Node) -> Vec<u8> {
    let mut out = vec![0x03, 0x01, 0x6a, 0x00];
    let mut page = 0;
    write(node, &mut page, &mut out);
    out
}

fn write(node: &Node, page: &mut u8, out: &mut Vec<u8>) {
    let Some((at, tag)) = token(node.name) else {
        return;
    };
    if at != *page {
        out.extend([SWITCH_PAGE, at]);
        *page = at;
    }
    if node.children.is_empty() && node.data.is_empty() {
        out.push(tag);
        return;
    }
    out.push(tag | CONTENT);
    if !node.data.is_empty() {
        if OPAQUE_TAGS.contains(&node.name)
            || node.data.contains(&0)
            || std::str::from_utf8(&node.data).is_err()
        {
            out.push(OPAQUE);
            multibyte(node.data.len(), out);
            out.extend_from_slice(&node.data);
        } else {
            out.push(STR_I);
            out.extend_from_slice(&node.data);
            out.push(0);
        }
    }
    for child in &node.children {
        write(child, page, out);
    }
    out.push(END);
}

/// A WBXML document's root; None for one that is not well formed or uses what ActiveSync
/// does not.
pub fn decode(data: &[u8]) -> Option<Node> {
    let mut reader = Reader {
        data,
        at: 0,
        page: 0,
    };
    reader.byte()?;
    // A public id of 0 is followed by its index in the string table.
    if reader.number()? == 0 {
        reader.number()?;
    }
    reader.number()?;
    let table = reader.number()?;
    reader.at = reader.at.checked_add(table)?;
    reader.node(0)
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    page: u8,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let b = *self.data.get(self.at)?;
        self.at += 1;
        Some(b)
    }

    /// A multi-byte unsigned integer: seven bits a byte, high bit set on all but the last.
    fn number(&mut self) -> Option<usize> {
        let mut n: usize = 0;
        for _ in 0..5 {
            let b = self.byte()?;
            n = n.checked_mul(128)? | usize::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Some(n);
            }
        }
        None
    }

    fn node(&mut self, depth: usize) -> Option<Node> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut b = self.byte()?;
        while b == SWITCH_PAGE {
            self.page = self.byte()?;
            b = self.byte()?;
        }
        if b & ATTRIBUTES != 0 || matches!(b & 0x3f, END | ENTITY | STR_I | LITERAL) {
            return None;
        }
        let mut node = Node::new(name(self.page, b & 0x3f), vec![]);
        if b & CONTENT == 0 {
            return Some(node);
        }
        loop {
            match *self.data.get(self.at)? {
                END => {
                    self.at += 1;
                    return Some(node);
                }
                STR_I => {
                    self.at += 1;
                    let rest = self.data.get(self.at..)?;
                    let len = rest.iter().position(|&b| b == 0)?;
                    node.data.extend_from_slice(&rest[..len]);
                    self.at += len + 1;
                }
                OPAQUE => {
                    self.at += 1;
                    let len = self.number()?;
                    let end = self.at.checked_add(len)?;
                    node.data.extend_from_slice(self.data.get(self.at..end)?);
                    self.at = end;
                }
                _ => node.children.push(self.node(depth + 1)?),
            }
        }
    }
}

fn multibyte(mut n: usize, out: &mut Vec<u8>) {
    let mut bytes = vec![(n & 0x7f) as u8];
    n >>= 7;
    while n > 0 {
        bytes.push((n & 0x7f) as u8 | 0x80);
        n >>= 7;
    }
    out.extend(bytes.iter().rev());
}

/// The code page and tag of a `Page:Name`.
fn token(name: &str) -> Option<(u8, u8)> {
    PAGES.iter().find_map(|(page, tags)| {
        let (token, _) = tags.iter().find(|(_, n)| *n == name)?;
        Some((*page, *token))
    })
}

/// The `Page:Name` of a tag; empty for one this crate does not know.
fn name(page: u8, token: u8) -> &'static str {
    PAGES
        .iter()
        .find(|(p, _)| *p == page)
        .and_then(|(_, tags)| tags.iter().find(|(t, _)| *t == token))
        .map_or("", |(_, name)| name)
}

/// The tags of each code page used, as `Page:Name`; others read as an empty name.
const PAGES: &[(u8, &[(u8, &str)])] = &[
    (
        0,
        &[
            (0x05, "AirSync:Sync"),
            (0x06, "AirSync:Responses"),
            (0x07, "AirSync:Add"),
            (0x08, "AirSync:Change"),
            (0x09, "AirSync:Delete"),
            (0x0a, "AirSync:Fetch"),
            (0x0b, "AirSync:SyncKey"),
            (0x0c, "AirSync:ClientId"),
            (0x0d, "AirSync:ServerId"),
            (0x0e, "AirSync:Status"),
            (0x0f, "AirSync:Collection"),
            (0x10, "AirSync:Class"),
            (0x12, "AirSync:CollectionId"),
            (0x13, "AirSync:GetChanges"),
            (0x14, "AirSync:MoreAvailable"),
            (0x15, "AirSync:WindowSize"),
            (0x16, "AirSync:Commands"),
            (0x17, "AirSync:Options"),
            (0x18, "AirSync:FilterType"),
            (0x19, "AirSync:Truncation"),
            (0x1b, "AirSync:Conflict"),
            (0x1c, "AirSync:Collections"),
            (0x1d, "AirSync:ApplicationData"),
            (0x1e, "AirSync:DeletesAsMoves"),
            (0x20, "AirSync:Supported"),
            (0x21, "AirSync:SoftDelete"),
            (0x22, "AirSync:MIMESupport"),
            (0x23, "AirSync:MIMETruncation"),
            (0x24, "AirSync:Wait"),
            (0x25, "AirSync:Limit"),
            (0x26, "AirSync:Partial"),
            (0x27, "AirSync:ConversationMode"),
            (0x28, "AirSync:MaxItems"),
            (0x29, "AirSync:HeartbeatInterval"),
        ],
    ),
    (
        2,
        &[
            (0x0c, "Email:Body"),
            (0x0d, "Email:BodySize"),
            (0x0e, "Email:BodyTruncated"),
            (0x0f, "Email:DateReceived"),
            (0x11, "Email:DisplayTo"),
            (0x12, "Email:Importance"),
            (0x13, "Email:MessageClass"),
            (0x14, "Email:Subject"),
            (0x15, "Email:Read"),
            (0x16, "Email:To"),
            (0x17, "Email:Cc"),
            (0x18, "Email:From"),
            (0x19, "Email:ReplyTo"),
            (0x35, "Email:ThreadTopic"),
            (0x36, "Email:MIMEData"),
            (0x37, "Email:MIMETruncated"),
            (0x38, "Email:MIMESize"),
            (0x39, "Email:InternetCPID"),
            (0x3a, "Email:Flag"),
            (0x3b, "Email:Status"),
            (0x3c, "Email:ContentClass"),
            (0x3d, "Email:FlagType"),
            (0x3e, "Email:CompleteTime"),
        ],
    ),
    (
        5,
        &[
            (0x05, "Move:MoveItems"),
            (0x06, "Move:Move"),
            (0x07, "Move:SrcMsgId"),
            (0x08, "Move:SrcFldId"),
            (0x09, "Move:DstFldId"),
            (0x0a, "Move:Response"),
            (0x0b, "Move:Status"),
            (0x0c, "Move:DstMsgId"),
        ],
    ),
    (
        7,
        &[
            (0x05, "FolderHierarchy:Folders"),
            (0x06, "FolderHierarchy:Folder"),
            (0x07, "FolderHierarchy:DisplayName"),
            (0x08, "FolderHierarchy:ServerId"),
            (0x09, "FolderHierarchy:ParentId"),
            (0x0a, "FolderHierarchy:Type"),
            (0x0c, "FolderHierarchy:Status"),
            (0x0e, "FolderHierarchy:Changes"),
            (0x0f, "FolderHierarchy:Add"),
            (0x10, "FolderHierarchy:Delete"),
            (0x11, "FolderHierarchy:Update"),
            (0x12, "FolderHierarchy:SyncKey"),
            (0x13, "FolderHierarchy:FolderCreate"),
            (0x14, "FolderHierarchy:FolderDelete"),
            (0x15, "FolderHierarchy:FolderUpdate"),
            (0x16, "FolderHierarchy:FolderSync"),
            (0x17, "FolderHierarchy:Count"),
        ],
    ),
    (
        8,
        &[
            (0x05, "MeetingResponse:CalendarId"),
            (0x06, "MeetingResponse:CollectionId"),
            (0x07, "MeetingResponse:MeetingResponse"),
            (0x08, "MeetingResponse:RequestId"),
            (0x09, "MeetingResponse:Request"),
            (0x0a, "MeetingResponse:Result"),
            (0x0b, "MeetingResponse:Status"),
            (0x0c, "MeetingResponse:UserResponse"),
        ],
    ),
    (
        13,
        &[
            (0x05, "Ping:Ping"),
            (0x07, "Ping:Status"),
            (0x08, "Ping:HeartbeatInterval"),
            (0x09, "Ping:Folders"),
            (0x0a, "Ping:Folder"),
            (0x0b, "Ping:Id"),
            (0x0c, "Ping:Class"),
            (0x0d, "Ping:MaxFolders"),
        ],
    ),
    (
        14,
        &[
            (0x05, "Provision:Provision"),
            (0x06, "Provision:Policies"),
            (0x07, "Provision:Policy"),
            (0x08, "Provision:PolicyType"),
            (0x09, "Provision:PolicyKey"),
            (0x0a, "Provision:Data"),
            (0x0b, "Provision:Status"),
            (0x0c, "Provision:RemoteWipe"),
            (0x0d, "Provision:EASProvisionDoc"),
        ],
    ),
    (
        15,
        &[
            (0x05, "Search:Search"),
            (0x07, "Search:Store"),
            (0x08, "Search:Name"),
            (0x09, "Search:Query"),
            (0x0a, "Search:Options"),
            (0x0b, "Search:Range"),
            (0x0c, "Search:Status"),
            (0x0d, "Search:Response"),
            (0x0e, "Search:Result"),
            (0x0f, "Search:Properties"),
            (0x10, "Search:Total"),
        ],
    ),
    (
        16,
        &[
            (0x05, "GAL:DisplayName"),
            (0x07, "GAL:Office"),
            (0x08, "GAL:Title"),
            (0x09, "GAL:Company"),
            (0x0f, "GAL:EmailAddress"),
        ],
    ),
    (
        17,
        &[
            (0x05, "AirSyncBase:BodyPreference"),
            (0x06, "AirSyncBase:Type"),
            (0x07, "AirSyncBase:TruncationSize"),
            (0x0a, "AirSyncBase:Body"),
            (0x0b, "AirSyncBase:Data"),
            (0x0c, "AirSyncBase:EstimatedDataSize"),
            (0x0d, "AirSyncBase:Truncated"),
            (0x16, "AirSyncBase:NativeBodyType"),
            (0x18, "AirSyncBase:Preview"),
        ],
    ),
    (
        18,
        &[
            (0x05, "Settings:Settings"),
            (0x06, "Settings:Status"),
            (0x07, "Settings:Get"),
            (0x08, "Settings:Set"),
            (0x16, "Settings:DeviceInformation"),
            (0x17, "Settings:Model"),
            (0x19, "Settings:FriendlyName"),
            (0x1a, "Settings:OS"),
            (0x1b, "Settings:OSLanguage"),
            (0x1d, "Settings:UserInformation"),
            (0x1e, "Settings:EmailAddresses"),
            (0x1f, "Settings:SMTPAddress"),
            (0x20, "Settings:UserAgent"),
            (0x23, "Settings:PrimarySmtpAddress"),
            (0x24, "Settings:Accounts"),
            (0x25, "Settings:Account"),
            (0x28, "Settings:UserDisplayName"),
        ],
    ),
    (
        20,
        &[
            (0x05, "ItemOperations:ItemOperations"),
            (0x06, "ItemOperations:Fetch"),
            (0x07, "ItemOperations:Store"),
            (0x08, "ItemOperations:Options"),
            (0x0b, "ItemOperations:Properties"),
            (0x0c, "ItemOperations:Data"),
            (0x0d, "ItemOperations:Status"),
            (0x0e, "ItemOperations:Response"),
            (0x12, "ItemOperations:EmptyFolderContents"),
            (0x13, "ItemOperations:DeleteSubFolders"),
        ],
    ),
    (
        21,
        &[
            (0x05, "ComposeMail:SendMail"),
            (0x08, "ComposeMail:SaveInSentItems"),
            (0x10, "ComposeMail:Mime"),
            (0x11, "ComposeMail:ClientId"),
            (0x12, "ComposeMail:Status"),
        ],
    ),
    (
        22,
        &[
            (0x09, "Email2:ConversationId"),
            (0x0a, "Email2:ConversationIndex"),
            (0x0b, "Email2:LastVerbExecuted"),
            (0x0c, "Email2:LastVerbExecutionTime"),
            (0x0d, "Email2:ReceivedAsBcc"),
            (0x0e, "Email2:Sender"),
            (0x15, "Email2:IsDraft"),
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_a_sync_request() {
        let mime = b"Subject: \xff\r\n\r\nhi\0".to_vec();
        let request = Node::new(
            "AirSync:Sync",
            vec![Node::new(
                "AirSync:Collections",
                vec![Node::new(
                    "AirSync:Collection",
                    vec![
                        Node::text("AirSync:SyncKey", "0"),
                        Node::text("AirSync:CollectionId", "5"),
                        Node::new("AirSync:GetChanges", vec![]),
                        Node::new(
                            "AirSync:Options",
                            vec![Node::new(
                                "AirSyncBase:BodyPreference",
                                vec![Node::text("AirSyncBase:Type", "4")],
                            )],
                        ),
                        Node::text("ComposeMail:Mime", mime),
                    ],
                )],
            )],
        );
        let bytes = encode(&request);
        // Sync with content, Collections, Collection, SyncKey "0".
        assert_eq!(
            bytes[..13],
            [3, 1, 0x6a, 0, 0x45, 0x5c, 0x4f, 0x4b, 3, b'0', 0, 1, 0x52]
        );
        assert_eq!(decode(&bytes), Some(request));
    }

    #[test]
    fn reads_unknown_tags_and_refuses_broken_ones() {
        // Contacts page 1, tag 5, empty; then a tag with content that never ends.
        assert_eq!(
            decode(&[3, 1, 0x6a, 0, 0x45, 0, 1, 0x05, 1]),
            Some(Node::new("AirSync:Sync", vec![Node::new("", vec![])]))
        );
        assert_eq!(decode(&[3, 1, 0x6a, 0, 0x45, 0x4b, 3, b'1']), None);
        assert_eq!(decode(&[3, 1, 0x6a, 0, 0xc5]), None);
    }
}
