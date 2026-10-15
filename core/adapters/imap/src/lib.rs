//! `MailProvider` over IMAP4rev1. Parsing is imap-codec's; the session flow is ours.

mod utf7;

use std::sync::Arc;
use std::time::Duration;

use bounded_static::IntoBoundedStatic;
use imap_codec::encode::{Encoder, Fragment};
use imap_codec::fragmentizer::Fragmentizer;
use imap_codec::imap_types::command::{Command, CommandBody};
use imap_codec::imap_types::core::{AString, LiteralMode, Vec1};
use imap_codec::imap_types::fetch::{MessageDataItem, MessageDataItemName, Section};
use imap_codec::imap_types::flag::{Flag, FlagFetch, FlagNameAttribute};
use imap_codec::imap_types::mailbox::Mailbox;
use imap_codec::imap_types::response::{
    Code, Data, GreetingKind, Response, Status, StatusBody, StatusKind,
};
use imap_codec::{CommandCodec, GreetingCodec, ResponseCodec};
use ozero_domain::{Flags, FolderRole, MailProvider, NewFolder, ProviderError, RawMessage, Recent};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

/// Any single read or write that takes longer means the connection is dead.
const TIMEOUT: Duration = Duration::from_secs(30);
/// Upper bound for one server response, literals included.
const MAX_RESPONSE: u32 = 16 * 1024 * 1024;
/// Headers the message list needs.
const HEADER_FIELDS: [&str; 4] = ["FROM", "SUBJECT", "DATE", "MESSAGE-ID"];

type Stream = TlsStream<TcpStream>;

pub struct ImapSession {
    stream: Stream,
    fragments: Fragmentizer,
    next_tag: u32,
}

impl std::fmt::Debug for ImapSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImapSession")
    }
}

pub use tokio_rustls::rustls;

/// TLS settings that trust the same root certificates as the OS.
/// On Android the verifier must be initialized from Kotlin first.
pub fn platform_tls() -> Result<Arc<ClientConfig>, ProviderError> {
    use rustls_platform_verifier::ConfigVerifierExt;
    ClientConfig::with_platform_verifier()
        .map(Arc::new)
        .map_err(|e| ProviderError::Network(e.to_string()))
}

/// The tagged end of a command and the untagged responses before it.
struct Reply {
    kind: StatusKind,
    code: Option<Code<'static>>,
    text: String,
    data: Vec<Data<'static>>,
    /// Codes of untagged OK responses, such as UIDVALIDITY after EXAMINE.
    codes: Vec<Code<'static>>,
}

impl ImapSession {
    /// Connects with implicit TLS (port 993) and logs in. There is no plain-text mode.
    pub async fn connect(
        host: &str,
        port: u16,
        tls: Arc<ClientConfig>,
        user: &str,
        password: &str,
    ) -> Result<Self, ProviderError> {
        let name = ServerName::try_from(host.to_owned())
            .map_err(|_| ProviderError::Network(format!("bad host name {host}")))?;
        let tcp = timed(TcpStream::connect((host, port))).await?;
        let stream = timed(TlsConnector::from(tls).connect(name, tcp)).await?;
        let mut session = Self {
            stream,
            fragments: Fragmentizer::new(MAX_RESPONSE),
            next_tag: 1,
        };
        if session.greeting().await? == GreetingKind::PreAuth {
            return Ok(session);
        }

        let login = CommandBody::login(user, password).map_err(|_| ProviderError::Auth)?;
        let reply = session.send(login).await?;
        match (reply.kind, &reply.code) {
            (StatusKind::Ok, _) => Ok(session),
            // RFC 5530: the server is down or busy, the password may be right.
            (StatusKind::No, Some(code)) if is_temporary(code) => {
                Err(ProviderError::Network(format!("LOGIN: {}", reply.text)))
            }
            (StatusKind::No, _) => Err(ProviderError::Auth),
            (StatusKind::Bad, _) => Err(protocol(format!("LOGIN: {}", reply.text))),
        }
    }

    pub async fn logout(mut self) -> Result<(), ProviderError> {
        self.run(CommandBody::Logout).await.map(|_| ())
    }

    async fn greeting(&mut self) -> Result<GreetingKind, ProviderError> {
        self.read_message().await?;
        let greeting = self
            .fragments
            .decode_message(&GreetingCodec::default())
            .map_err(|e| protocol(format!("bad greeting: {e:?}")))?;
        match greeting.kind {
            // Busy or refusing: say why instead of failing on the closed connection.
            GreetingKind::Bye => Err(ProviderError::Network(format!(
                "server refused: {}",
                greeting.text
            ))),
            kind => Ok(kind),
        }
    }

    /// Runs a command that must succeed.
    async fn run(&mut self, body: CommandBody<'_>) -> Result<Reply, ProviderError> {
        let name = body.name();
        let reply = self.send(body).await?;
        match reply.kind {
            StatusKind::Ok => Ok(reply),
            _ => Err(protocol(format!("{name}: {}", reply.text))),
        }
    }

    /// Sends a command and collects untagged data until its tagged reply.
    async fn send(&mut self, body: CommandBody<'_>) -> Result<Reply, ProviderError> {
        let tag = format!("o{}", self.next_tag);
        self.next_tag += 1;
        let command = Command::new(tag.as_str(), body).map_err(|_| protocol("bad tag"))?;
        for fragment in CommandCodec::default().encode(&command) {
            match fragment {
                Fragment::Line { data } => self.write(&data).await?,
                Fragment::Literal { data, mode } => {
                    // A synchronizing literal waits for the server's "+".
                    // The server may refuse with the tagged reply instead.
                    if mode == LiteralMode::Sync {
                        timed(self.stream.flush()).await?;
                        if let Some(reply) = self.collect(&tag, true).await? {
                            return Ok(reply);
                        }
                    }
                    self.write(&data).await?;
                }
            }
        }
        timed(self.stream.flush()).await?;
        self.collect(&tag, false)
            .await?
            .ok_or_else(|| protocol("unexpected continuation request"))
    }

    /// Reads responses until the tagged reply to `tag`, or until a "+" when `until_continue`.
    async fn collect(
        &mut self,
        tag: &str,
        until_continue: bool,
    ) -> Result<Option<Reply>, ProviderError> {
        let (mut data, mut codes) = (Vec::new(), Vec::new());
        loop {
            // A BYE alone decides nothing: LOGOUT gets one before its OK, and a
            // server that really leaves closes the connection, which fails the read.
            match self.response().await? {
                Response::CommandContinuationRequest(_) if until_continue => return Ok(None),
                Response::Data(d) => data.push(d),
                Response::Status(Status::Untagged(StatusBody {
                    kind: StatusKind::Ok,
                    code: Some(code),
                    ..
                })) => codes.push(code),
                Response::Status(Status::Tagged(t)) if t.tag.inner() == tag => {
                    return Ok(Some(Reply {
                        kind: t.body.kind,
                        code: t.body.code,
                        text: t.body.text.to_string(),
                        data,
                        codes,
                    }));
                }
                _ => {}
            }
        }
    }

    async fn response(&mut self) -> Result<Response<'static>, ProviderError> {
        self.read_message().await?;
        self.fragments
            .decode_message(&ResponseCodec::default())
            .map(IntoBoundedStatic::into_static)
            .map_err(|e| protocol(format!("bad response: {e:?}")))
    }

    /// Reads until the fragmentizer holds one whole message.
    async fn read_message(&mut self) -> Result<(), ProviderError> {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match self.fragments.progress() {
                Some(_) if self.fragments.is_message_complete() => return Ok(()),
                Some(_) => {}
                None => {
                    let n = timed(self.stream.read(&mut buf)).await?;
                    if n == 0 {
                        return Err(ProviderError::Network("connection closed".into()));
                    }
                    self.fragments.enqueue_bytes(&buf[..n]);
                }
            }
        }
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), ProviderError> {
        timed(self.stream.write_all(data)).await
    }
}

impl MailProvider for ImapSession {
    async fn folders(&mut self) -> Result<Vec<NewFolder>, ProviderError> {
        let list = CommandBody::list("", "*").map_err(|_| protocol("bad LIST"))?;
        Ok(self
            .run(list)
            .await?
            .data
            .into_iter()
            .filter_map(|d| match d {
                Data::List { items, mailbox, .. } => folder(&items, &mailbox),
                _ => None,
            })
            .collect())
    }

    async fn recent(&mut self, folder: &str, limit: u32) -> Result<Recent, ProviderError> {
        let examine = CommandBody::examine(folder).map_err(|_| protocol("bad folder name"))?;
        let reply = self.run(examine).await?;
        let validity = reply
            .codes
            .iter()
            .find_map(|c| match c {
                Code::UidValidity(v) => Some(v.get()),
                _ => None,
            })
            .ok_or_else(|| protocol("EXAMINE without UIDVALIDITY"))?;
        // The last EXISTS is the newest count if mail arrived meanwhile.
        let exists = reply
            .data
            .iter()
            .rev()
            .find_map(|d| match d {
                Data::Exists(n) => Some(*n),
                _ => None,
            })
            .ok_or_else(|| protocol("EXAMINE without EXISTS"))?;
        if exists == 0 || limit == 0 {
            return Ok(Recent {
                validity,
                messages: vec![],
            });
        }

        let fields = HEADER_FIELDS
            .iter()
            .map(|f| AString::try_from(*f))
            .collect::<Result<Vec<_>, _>>()
            .ok()
            .and_then(|f| Vec1::try_from(f).ok())
            .ok_or_else(|| protocol("bad header fields"))?;
        let items = vec![
            MessageDataItemName::Uid,
            MessageDataItemName::Flags,
            MessageDataItemName::InternalDate,
            MessageDataItemName::BodyExt {
                section: Some(Section::HeaderFields(None, fields)),
                partial: None,
                peek: true,
            },
        ];
        let first = exists.saturating_sub(limit) + 1;
        let fetch =
            CommandBody::fetch(first..=exists, items, false).map_err(|_| protocol("bad FETCH"))?;

        let mut messages: Vec<_> = self
            .run(fetch)
            .await?
            .data
            .into_iter()
            .filter_map(|d| match d {
                Data::Fetch { seq, items } => message(items).map(|m| (seq, m)),
                _ => None,
            })
            .collect();
        // A server may answer one message twice; keep one.
        messages.sort_by_key(|(seq, _)| *seq);
        messages.dedup_by_key(|(seq, _)| *seq);
        Ok(Recent {
            validity,
            messages: messages.into_iter().map(|(_, m)| m).collect(),
        })
    }
}

/// A LIST entry that can hold messages.
fn folder(items: &[FlagNameAttribute<'_>], mailbox: &Mailbox<'_>) -> Option<NewFolder> {
    let mut role = FolderRole::Custom;
    for item in items {
        match item {
            FlagNameAttribute::Noselect => return None,
            // Special-use roles and LIST-EXTENDED's \NonExistent arrive as extensions.
            FlagNameAttribute::Extension(_) => {
                let attr = item.to_string();
                if attr.eq_ignore_ascii_case("\\NonExistent") {
                    return None;
                }
                role = FolderRole::from_special_use(&attr).unwrap_or(role);
            }
            _ => {}
        }
    }
    let remote_id = match mailbox {
        Mailbox::Inbox => {
            role = FolderRole::Inbox;
            "INBOX".to_owned()
        }
        // A raw 8-bit name would not survive a round trip through a String; skip it.
        Mailbox::Other(other) => String::from_utf8(other.as_ref().to_vec()).ok()?,
    };
    let name = utf7::decode(&remote_id).unwrap_or_else(|| remote_id.clone());
    Some(NewFolder {
        remote_id,
        name,
        role,
    })
}

/// A FETCH reply with everything `recent` asked for, or `None` if the server left something out.
fn message<'a>(items: impl IntoIterator<Item = MessageDataItem<'a>>) -> Option<RawMessage> {
    let (mut uid, mut flags, mut received, mut header) = (None, None, None, None);
    for item in items {
        match item {
            MessageDataItem::Uid(u) => uid = Some(u),
            MessageDataItem::Flags(f) => flags = Some(parse_flags(&f)),
            MessageDataItem::InternalDate(d) => received = Some(d.as_ref().timestamp()),
            MessageDataItem::BodyExt { data, .. } => {
                header = Some(
                    data.0
                        .map(|s| s.into_inner().into_owned())
                        .unwrap_or_default(),
                );
            }
            _ => {}
        }
    }
    Some(RawMessage {
        remote_id: uid?.to_string(),
        flags: flags?,
        received: received?,
        header: header?,
    })
}

/// RFC 5530 codes for a refusal that is not about the credentials.
fn is_temporary(code: &Code<'_>) -> bool {
    let Code::Other(other) = code else {
        return false;
    };
    let name = other
        .inner()
        .split(|b| *b == b' ')
        .next()
        .unwrap_or_default();
    [&b"UNAVAILABLE"[..], b"LIMIT", b"INUSE", b"SERVERBUG"]
        .iter()
        .any(|c| name.eq_ignore_ascii_case(c))
}

fn parse_flags(flags: &[FlagFetch<'_>]) -> Flags {
    let mut out = Flags::default();
    for f in flags {
        match f {
            FlagFetch::Flag(Flag::Seen) => out.seen = true,
            FlagFetch::Flag(Flag::Flagged) => out.flagged = true,
            FlagFetch::Flag(Flag::Answered) => out.answered = true,
            FlagFetch::Flag(Flag::Draft) => out.draft = true,
            _ => {}
        }
    }
    out
}

async fn timed<T, E: std::fmt::Display>(
    op: impl Future<Output = Result<T, E>>,
) -> Result<T, ProviderError> {
    match tokio::time::timeout(TIMEOUT, op).await {
        Ok(result) => result.map_err(|e| ProviderError::Network(e.to_string())),
        Err(_) => Err(ProviderError::Network("timed out".into())),
    }
}

fn protocol(msg: impl Into<String>) -> ProviderError {
    ProviderError::Protocol(msg.into())
}
