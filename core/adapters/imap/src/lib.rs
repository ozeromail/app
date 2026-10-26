//! `MailProvider` over IMAP4rev1. Parsing is imap-codec's; the session flow is ours.

mod utf7;
mod verify;

use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use bounded_static::IntoBoundedStatic;
use imap_codec::encode::{Encoder, Fragment};
use imap_codec::fragmentizer::Fragmentizer;
use imap_codec::imap_types::command::{Command, CommandBody};
use imap_codec::imap_types::core::{AString, Charset, LiteralMode, Vec1};
use imap_codec::imap_types::extensions::uidplus::{UidElement, UidSet};
use imap_codec::imap_types::fetch::{MessageDataItem, MessageDataItemName, Section};
use imap_codec::imap_types::flag::{Flag, FlagFetch, FlagNameAttribute, StoreResponse, StoreType};
use imap_codec::imap_types::mailbox::Mailbox;
use imap_codec::imap_types::response::{
    Capability, Code, Data, GreetingKind, Response, Status, StatusBody, StatusKind,
};
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::{SeqOrUid, Sequence, SequenceSet};
use imap_codec::{CommandCodec, GreetingCodec, ResponseCodec};
use ozero_domain::{
    Changes, Checkpoint, Flags, FolderRole, MailProvider, Moved, NewFolder, Older, ProviderError,
    RawMessage,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

/// Any single read or write that takes longer means the connection is dead.
const TIMEOUT: Duration = Duration::from_secs(30);
/// Bytes written in one go, each within `TIMEOUT`.
const WRITE_CHUNK: usize = 64 * 1024;
/// Upper bound for one server response, literals included: a whole message with 25 MB of
/// files, as the big providers allow, in base64.
const MAX_RESPONSE: u32 = 40 * 1024 * 1024;
/// How much of one message `body` downloads; text parts come first, big attachments last.
const MAX_BODY: NonZeroU32 = NonZeroU32::MIN.saturating_add(4 * 1024 * 1024 - 1);
/// How often `wait` looks at a folder on a server without IDLE.
const POLL: Duration = Duration::from_secs(60);
/// UIDs per STORE or MOVE command.
const UID_CHUNK: usize = 500;
/// Headers the message list, conversations, answers and the inbox tabs need.
const HEADER_FIELDS: [&str; 14] = [
    "FROM",
    "REPLY-TO",
    "TO",
    "CC",
    "SUBJECT",
    "DATE",
    "MESSAGE-ID",
    "IN-REPLY-TO",
    "REFERENCES",
    "LIST-ID",
    "LIST-UNSUBSCRIBE",
    "LIST-UNSUBSCRIBE-POST",
    "PRECEDENCE",
    "AUTO-SUBMITTED",
];

type Stream = TlsStream<TcpStream>;

pub struct ImapSession {
    stream: Stream,
    fragments: Fragmentizer,
    next_tag: u32,
    /// What the server can do once logged in, from the LOGIN reply or asked for once.
    capabilities: Option<Vec<Capability<'static>>>,
}

impl std::fmt::Debug for ImapSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImapSession")
    }
}

pub use tokio_rustls::rustls;

pub use verify::{Refused, RefusedCertificates, fingerprint};

/// TLS settings that trust the same root certificates as the OS.
/// On Android the verifier must be initialized from Kotlin first. `client` offers a client
/// certificate to a server that asks for one. `trusted` is the SHA-256 of a server
/// certificate the person chose to trust although the checks refuse it; the refused ones
/// go to `refused`.
pub fn platform_tls(
    client: Option<Arc<dyn rustls::client::ResolvesClientCert>>,
    trusted: Option<[u8; 32]>,
    refused: Arc<RefusedCertificates>,
) -> Result<Arc<ClientConfig>, ProviderError> {
    use tokio_rustls::rustls::client::danger::ServerCertVerifier;

    let builder = ClientConfig::builder();
    let os: Arc<dyn ServerCertVerifier> = Arc::new(
        rustls_platform_verifier::Verifier::new(builder.crypto_provider().clone())
            .map_err(|e| ProviderError::Network(e.to_string()))?,
    );
    #[cfg(target_os = "android")]
    let os = Arc::new(verify::RecheckRevoked(os));
    let os = Arc::new(verify::Trusting {
        checks: os,
        trusted,
        refused,
    });
    let builder = builder.dangerous().with_custom_certificate_verifier(os);
    let config = match client {
        Some(client) => builder.with_client_cert_resolver(client),
        None => builder.with_no_client_auth(),
    };
    Ok(Arc::new(config))
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
            capabilities: None,
        };
        if session.greeting().await? == GreetingKind::PreAuth {
            return Ok(session);
        }

        let login = CommandBody::login(user, password).map_err(|_| ProviderError::Auth)?;
        let reply = session.send(login).await?;
        match (reply.kind, &reply.code) {
            (StatusKind::Ok, code) => {
                // In the reply's code or an untagged CAPABILITY, as Gmail sends it.
                let untagged = reply.data.iter().find_map(|d| match d {
                    Data::Capability(caps) => Some(caps),
                    _ => None,
                });
                if let Some(caps) = untagged.or(match code {
                    Some(Code::Capability(caps)) => Some(caps),
                    _ => None,
                }) {
                    session.capabilities = Some(caps.as_ref().to_vec());
                }
                Ok(session)
            }
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
        match (reply.kind, &reply.code) {
            (StatusKind::Ok, _) => Ok(reply),
            // Busy or locked: worth trying again later, unlike a refusal.
            (StatusKind::No, Some(code)) if is_temporary(code) => {
                Err(ProviderError::Network(format!("{name}: {}", reply.text)))
            }
            _ => Err(protocol(format!("{name}: {}", reply.text))),
        }
    }

    /// Sends a command and collects untagged data until its tagged reply.
    async fn send(&mut self, body: CommandBody<'_>) -> Result<Reply, ProviderError> {
        let tag = self.tag();
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

    fn tag(&mut self) -> String {
        let tag = format!("o{}", self.next_tag);
        self.next_tag += 1;
        tag
    }

    /// Whether the server has IDLE.
    async fn can_idle(&mut self) -> Result<bool, ProviderError> {
        Ok(self.capabilities().await?.contains(&Capability::Idle))
    }

    /// Whether the server is Gmail, where folders are labels.
    async fn is_gmail(&mut self) -> Result<bool, ProviderError> {
        Ok(self
            .capabilities()
            .await?
            .iter()
            .any(|c| c.to_string().eq_ignore_ascii_case("X-GM-EXT-1")))
    }

    /// Whether `folder` is Gmail's All Mail, which holds every message whatever its labels.
    async fn is_all_mail(&mut self, folder: &str) -> Result<bool, ProviderError> {
        if !self.is_gmail().await? {
            return Ok(false);
        }
        let list = CommandBody::list("", folder).map_err(|_| protocol("bad LIST"))?;
        Ok(self.run(list).await?.data.iter().any(|d| {
            matches!(d, Data::List { items, .. }
                if items.iter().any(|i| i.to_string().eq_ignore_ascii_case("\\All")))
        }))
    }

    /// What the server can do; asked for at most once a session.
    async fn capabilities(&mut self) -> Result<&[Capability<'static>], ProviderError> {
        if self.capabilities.is_none() {
            let caps = self
                .run(CommandBody::Capability)
                .await?
                .data
                .into_iter()
                .filter_map(|d| match d {
                    Data::Capability(caps) => Some(caps.into_inner()),
                    _ => None,
                })
                .flatten()
                .collect();
            self.capabilities = Some(caps);
        }
        Ok(self.capabilities.as_deref().unwrap_or_default())
    }

    /// Waits in IDLE (RFC 2177) until the server reports a change in the open folder, at
    /// most `wait`: true if it did.
    async fn idle(&mut self, wait: Duration) -> Result<bool, ProviderError> {
        let tag = self.tag();
        self.write(format!("{tag} IDLE\r\n").as_bytes()).await?;
        timed(self.stream.flush()).await?;
        let ours = |r: &Response<'_>| matches!(r, Response::Status(Status::Tagged(t)) if t.tag.inner() == tag);
        // A change may come before the server agrees, too.
        let mut changed = false;
        loop {
            match self.response().await? {
                Response::CommandContinuationRequest(_) => break,
                r if ours(&r) => return Err(protocol("IDLE refused")),
                r => changed |= is_change(&r),
            }
        }
        if !changed {
            let news = async {
                loop {
                    // The outer timeout ends the wait first; this one never does.
                    match self.response_within(wait + TIMEOUT).await? {
                        // The server ended IDLE itself: there is nothing to end.
                        r if ours(&r) => return Ok(None),
                        r if is_change(&r) => return Ok(Some(true)),
                        _ => {}
                    }
                }
            };
            match tokio::time::timeout(wait, news).await {
                Ok(Ok(None)) => return Ok(false),
                Ok(Ok(Some(news))) => changed = news,
                Ok(Err(e)) => return Err(e),
                Err(_) => {}
            }
        }
        self.write(b"DONE\r\n").await?;
        timed(self.stream.flush()).await?;
        self.collect(&tag, false).await?;
        Ok(changed)
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
        self.response_within(TIMEOUT).await
    }

    /// The next response; fails if the server is silent for `limit`.
    async fn response_within(
        &mut self,
        limit: Duration,
    ) -> Result<Response<'static>, ProviderError> {
        self.read_message_within(limit).await?;
        self.fragments
            .decode_message(&ResponseCodec::default())
            .map(IntoBoundedStatic::into_static)
            .map_err(|e| protocol(format!("bad response: {e:?}")))
    }

    /// Reads until the fragmentizer holds one whole message.
    async fn read_message(&mut self) -> Result<(), ProviderError> {
        self.read_message_within(TIMEOUT).await
    }

    async fn read_message_within(&mut self, limit: Duration) -> Result<(), ProviderError> {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match self.fragments.progress() {
                Some(_) if self.fragments.is_message_complete() => return Ok(()),
                Some(_) => {}
                None => {
                    let n = timed_for(limit, self.stream.read(&mut buf)).await?;
                    if n == 0 {
                        return Err(ProviderError::Network("connection closed".into()));
                    }
                    self.fragments.enqueue_bytes(&buf[..n]);
                }
            }
        }
    }

    /// Writes `data` a chunk at a time, each in `TIMEOUT`: a sent copy with large files goes on
    /// a slow link as long as it moves.
    async fn write(&mut self, data: &[u8]) -> Result<(), ProviderError> {
        for chunk in data.chunks(WRITE_CHUNK) {
            timed(self.stream.write_all(chunk)).await?;
        }
        Ok(())
    }

    /// Opens a folder, read-only unless `write`.
    async fn open(&mut self, folder: &str, write: bool) -> Result<Opened, ProviderError> {
        let open = if write {
            CommandBody::select(folder)
        } else {
            CommandBody::examine(folder)
        };
        let reply = self
            .run(open.map_err(|_| protocol("bad folder name"))?)
            .await?;
        let validity = reply
            .codes
            .iter()
            .find_map(|c| match c {
                Code::UidValidity(v) => Some(v.get()),
                _ => None,
            })
            .ok_or_else(|| protocol("no UIDVALIDITY for the folder"))?;
        let next = reply
            .codes
            .iter()
            .find_map(|c| match c {
                Code::UidNext(n) => Some(n.get()),
                _ => None,
            })
            .unwrap_or(0);
        // The last EXISTS is the newest count if mail arrived meanwhile.
        let exists = reply
            .data
            .iter()
            .rev()
            .find_map(|d| match d {
                Data::Exists(n) => Some(*n),
                _ => None,
            })
            .ok_or_else(|| protocol("no EXISTS for the folder"))?;
        Ok(Opened {
            validity,
            next,
            exists,
        })
    }

    /// The newest `limit` of the open folder's messages 1 to `exists`, oldest first.
    async fn newest(&mut self, exists: u32, limit: u32) -> Result<Vec<RawMessage>, ProviderError> {
        if exists == 0 || limit == 0 {
            return Ok(vec![]);
        }
        let first = exists.saturating_sub(limit) + 1;
        self.headers(first..=exists, false).await
    }

    /// Header fetch of the messages in `set`, oldest first.
    async fn headers(
        &mut self,
        set: impl TryInto<SequenceSet>,
        uid: bool,
    ) -> Result<Vec<RawMessage>, ProviderError> {
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
        let fetch = CommandBody::fetch(set, items, uid).map_err(|_| protocol("bad FETCH"))?;
        let mut messages: Vec<_> = self
            .run(fetch)
            .await?
            .data
            .into_iter()
            .filter_map(|d| match d {
                Data::Fetch { items, .. } => message(items),
                _ => None,
            })
            .collect();
        // A server may answer one message twice; keep one. UIDs grow with sequence numbers.
        messages.sort_by_cached_key(|m| m.remote_id.parse::<u32>().unwrap_or(0));
        messages.dedup_by(|a, b| a.remote_id == b.remote_id);
        Ok(messages)
    }

    /// The open folder's messages that match all `keys`: UIDs, or sequence numbers without `uid`.
    async fn search_for(
        &mut self,
        charset: Option<Charset<'_>>,
        keys: Vec1<SearchKey<'_>>,
        uid: bool,
    ) -> Result<Vec<NonZeroU32>, ProviderError> {
        let search = CommandBody::search(charset, keys, uid);
        Ok(self
            .run(search)
            .await?
            .data
            .into_iter()
            .filter_map(|d| match d {
                Data::Search(found, ..) => Some(found),
                _ => None,
            })
            .flatten()
            .collect())
    }

    /// The sequence number of the open folder's message with `uid`, if it is still there.
    async fn number(&mut self, uid: NonZeroU32) -> Result<Option<u32>, ProviderError> {
        let items = vec![MessageDataItemName::Uid];
        let fetch = CommandBody::fetch(uid, items, true).map_err(|_| protocol("bad FETCH"))?;
        // The server may add FETCH replies about other messages, such as flag changes.
        Ok(self
            .run(fetch)
            .await?
            .data
            .into_iter()
            .find_map(|d| match d {
                Data::Fetch { seq, items } => items
                    .as_ref()
                    .iter()
                    .any(|i| matches!(i, MessageDataItem::Uid(u) if *u == uid))
                    .then_some(seq.get()),
                _ => None,
            }))
    }

    /// The UID and flags of every message from UID `from` on.
    async fn flags_from(&mut self, from: NonZeroU32) -> Result<Vec<(u32, Flags)>, ProviderError> {
        let items = vec![MessageDataItemName::Uid, MessageDataItemName::Flags];
        let fetch = CommandBody::fetch(from.., items, true).map_err(|_| protocol("bad FETCH"))?;
        Ok(self
            .run(fetch)
            .await?
            .data
            .into_iter()
            .filter_map(|d| match d {
                Data::Fetch { items, .. } => uid_flags(items),
                _ => None,
            })
            .collect())
    }
}

/// What EXAMINE or SELECT says about a folder.
struct Opened {
    validity: u32,
    /// UIDNEXT, or 0 if the server left it out.
    next: u32,
    exists: u32,
}

impl MailProvider for ImapSession {
    async fn folders(&mut self) -> Result<Vec<NewFolder>, ProviderError> {
        let gmail = self.is_gmail().await?;
        let list = CommandBody::list("", "*").map_err(|_| protocol("bad LIST"))?;
        Ok(self
            .run(list)
            .await?
            .data
            .into_iter()
            .filter_map(|d| match d {
                Data::List { items, mailbox, .. } => folder(&items, &mailbox, gmail),
                _ => None,
            })
            .collect())
    }

    async fn changes(
        &mut self,
        folder: &str,
        since: Checkpoint,
        known: &[String],
        _oldest: i64,
        limit: u32,
    ) -> Result<Changes, ProviderError> {
        let opened = self.open(folder, false).await?;
        let mut known: HashSet<u32> = known.iter().filter_map(|id| id.parse().ok()).collect();
        let mut changes = Changes {
            checkpoint: Checkpoint {
                validity: opened.validity,
                next: opened.next.into(),
            },
            new: vec![],
            flags: vec![],
            gone: vec![],
        };
        // Nothing saved can be trusted, or there is no checkpoint yet: start from the newest.
        if since.validity != opened.validity || since.next == 0 {
            changes.new = self.newest(opened.exists, limit).await?;
            if let Some(last) = changes.new.last() {
                // Without UIDNEXT, mail after the newest one fetched is new.
                let uid = last.remote_id.parse::<u64>().unwrap_or(0);
                changes.checkpoint.next = changes.checkpoint.next.max(uid + 1);
            }
            return Ok(changes);
        }
        // UIDs end at u32::MAX, so a checkpoint past it has no new mail left.
        let next = NonZeroU32::new(u32::try_from(since.next).unwrap_or(u32::MAX))
            .unwrap_or(NonZeroU32::MIN);
        changes.checkpoint.next = changes.checkpoint.next.max(since.next);
        if known.is_empty() && u64::from(opened.next) == since.next {
            return Ok(changes);
        }

        // One FETCH of UIDs and flags from the oldest known message on tells what is still
        // there, what changed and what is new: a few bytes per message instead of headers.
        let from = known
            .iter()
            .filter_map(|uid| NonZeroU32::new(*uid))
            .fold(next, NonZeroU32::min);
        let mut fresh = vec![];
        for (uid, flags) in self.flags_from(from).await? {
            if known.remove(&uid) {
                changes.flags.push((uid.to_string(), flags));
            } else if uid >= next.get() {
                fresh.push(uid);
            }
        }
        // What is left of the known mail is no longer in the folder.
        changes.gone = known.iter().map(u32::to_string).collect();

        fresh.sort_unstable();
        fresh.dedup();
        let newest = &fresh[fresh.len().saturating_sub(limit as usize)..];
        if let (Some(&first), Some(&last)) = (newest.first(), newest.last()) {
            changes.new = self.headers(first..=last, true).await?;
            let fetched: HashSet<&str> = changes.new.iter().map(|m| m.remote_id.as_str()).collect();
            // Mail that arrived after EXAMINE was fetched too. Mail the server left out of
            // the reply comes again next time.
            changes.checkpoint.next = match newest
                .iter()
                .find(|uid| !fetched.contains(uid.to_string().as_str()))
            {
                Some(missed) => u64::from(*missed),
                None => changes.checkpoint.next.max(u64::from(last) + 1),
            };
        }
        Ok(changes)
    }

    async fn older(
        &mut self,
        folder: &str,
        validity: u32,
        known: &[String],
        _oldest: i64,
        limit: u32,
    ) -> Result<Older, ProviderError> {
        let opened = self.open(folder, false).await?;
        if opened.validity != validity {
            return Err(ProviderError::Gone);
        }
        let lowest = known
            .iter()
            .filter_map(|id| id.parse::<NonZeroU32>().ok())
            .min();
        // Sequence numbers grow with UIDs, so the mail below the lowest saved UID is numbered
        // 1 to one less than that message's number.
        let below = match lowest {
            None => opened.exists,
            Some(lowest) => match self.number(lowest).await? {
                Some(number) => number - 1,
                // Gone since the last sync: the first number of the saved mail after it, a
                // longer answer. Not `n:*`, see `empty`.
                None => {
                    let from = SequenceSet::try_from(lowest.get()..=i32::MAX as u32)
                        .map_err(|_| protocol("bad UID set"))?;
                    let keys = Vec1::from(SearchKey::Uid(from));
                    self.search_for(None, keys, false)
                        .await?
                        .into_iter()
                        .min()
                        .map_or(opened.exists, |first| first.get() - 1)
                }
            },
        };
        let mut new = self.newest(below, limit).await?;
        // Mail expunged meanwhile moves the numbers down: only what is older still.
        if let Some(lowest) = lowest {
            new.retain(|m| {
                m.remote_id
                    .parse::<u32>()
                    .is_ok_and(|uid| uid < lowest.get())
            });
        }
        Ok(Older {
            new,
            more: below > limit,
        })
    }

    async fn search(
        &mut self,
        folder: &str,
        validity: u32,
        text: &str,
        limit: u32,
    ) -> Result<Vec<RawMessage>, ProviderError> {
        let words = text
            .split_whitespace()
            .map(|w| AString::try_from(w).map(SearchKey::Text))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| protocol("bad search"))?;
        let Ok(words) = Vec1::try_from(words) else {
            return Ok(vec![]);
        };
        // 0: a folder never synced, so any validity will do.
        let opened = self.open(folder, false).await?.validity;
        if validity != 0 && opened != validity {
            return Err(ProviderError::Gone);
        }
        // US-ASCII is every server's default; other text needs the charset named.
        let charset = (!text.is_ascii())
            .then(|| Charset::try_from("UTF-8"))
            .transpose()
            .map_err(|_| protocol("bad charset"))?;
        let mut uids = self.search_for(charset, words, true).await?;
        uids.sort_unstable();
        uids.dedup();
        let newest = &uids[uids.len().saturating_sub(limit as usize)..];
        if newest.is_empty() {
            return Ok(vec![]);
        }
        let set = SequenceSet::try_from(newest).map_err(|_| protocol("bad UID set"))?;
        self.headers(set, true).await
    }

    async fn body(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> Result<Vec<u8>, ProviderError> {
        self.fetch_message(folder, validity, remote_id, Some((0, MAX_BODY)))
            .await
    }

    async fn message(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
    ) -> Result<Vec<u8>, ProviderError> {
        self.fetch_message(folder, validity, remote_id, None).await
    }

    async fn set_seen(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        seen: bool,
    ) -> Result<(), ProviderError> {
        let kind = if seen {
            StoreType::Add
        } else {
            StoreType::Remove
        };
        self.each_chunk(folder, validity, remote_ids, |uids| {
            CommandBody::store(uids, kind, StoreResponse::Silent, vec![Flag::Seen], true)
                .map_err(|_| protocol("bad STORE"))
        })
        .await
        .map(drop)
    }

    async fn move_to(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> Result<Moved, ProviderError> {
        let to = Mailbox::try_from(to).map_err(|_| protocol("bad folder name"))?;
        // Mail taken out of Gmail's All Mail goes to the trash; a copy only adds the label.
        let copy = self.is_all_mail(folder).await?;
        // RFC 6851 MOVE; every server we target has it.
        let replies = self
            .each_chunk(folder, validity, remote_ids, |uids| {
                let sequence_set = uids.try_into().map_err(|_| protocol("bad UID set"))?;
                let mailbox = to.clone();
                Ok(if copy {
                    CommandBody::Copy {
                        sequence_set,
                        mailbox,
                        uid: true,
                    }
                } else {
                    CommandBody::Move {
                        sequence_set,
                        mailbox,
                        uid: true,
                    }
                })
            })
            .await?;
        let mut moved = Moved::default();
        for reply in &replies {
            // UIDPLUS (RFC 4315) names the new UIDs, before the expunges or in the end.
            for code in reply.codes.iter().chain(&reply.code) {
                if let Code::CopyUid {
                    uid_validity,
                    source,
                    destination,
                } = code
                {
                    moved.validity = Some(uid_validity.get());
                    // As many as were moved: a server cannot make us count to u32::MAX.
                    let ids = uid_list(source, remote_ids.len())
                        .zip(uid_list(destination, remote_ids.len()));
                    moved
                        .ids
                        .extend(ids.map(|(old, new)| (old.to_string(), new.to_string())));
                }
            }
        }
        Ok(moved)
    }

    async fn delete(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
    ) -> Result<(), ProviderError> {
        let uids = uids(remote_ids)?;
        if uids.is_empty() {
            return Ok(());
        }
        if self.open(folder, true).await?.validity != validity {
            return Err(ProviderError::Gone);
        }
        let sets = uids
            .chunks(UID_CHUNK)
            .map(SequenceSet::try_from)
            .collect::<Result<_, _>>()
            .map_err(|_| protocol("bad UID set"))?;
        self.purge(sets).await
    }

    async fn empty(
        &mut self,
        folder: &str,
        validity: u32,
        keep: &[String],
    ) -> Result<(), ProviderError> {
        if self.open(folder, true).await?.validity != validity {
            return Err(ProviderError::Gone);
        }
        let mut keep = uids(keep)?;
        keep.sort();
        // Every UID but those to keep: the gaps between them, the last up to a UID far past
        // any real one. Not `n:*`, which takes the newest message even when it is below `n`;
        // imap-codec writes numbers near `u32::MAX` as `*`.
        let mut gaps = vec![];
        let mut from = 1;
        for uid in keep
            .into_iter()
            .map(NonZeroU32::get)
            .chain([i32::MAX as u32])
        {
            if let (Some(a), Some(b)) = (NonZeroU32::new(from), NonZeroU32::new(uid - 1))
                && a <= b
            {
                gaps.push(Sequence::Range(SeqOrUid::Value(a), SeqOrUid::Value(b)));
            }
            from = uid.saturating_add(1);
        }
        let sets = gaps
            .chunks(UID_CHUNK)
            .map(|chunk| SequenceSet::try_from(chunk.to_vec()))
            .collect::<Result<_, _>>()
            .map_err(|_| protocol("bad UID set"))?;
        self.purge(sets).await
    }

    async fn append(&mut self, folder: &str, message: &[u8]) -> Result<(), ProviderError> {
        let mailbox = Mailbox::try_from(folder).map_err(|_| protocol("bad folder name"))?;
        let append = CommandBody::append(mailbox, vec![Flag::Seen], None, message.to_vec())
            .map_err(|_| protocol("bad APPEND"))?;
        self.run(append).await.map(|_| ())
    }

    async fn wait(
        &mut self,
        folder: &str,
        since: Checkpoint,
        wait: Duration,
    ) -> Result<bool, ProviderError> {
        let idles = self.can_idle().await?;
        let opened = self.open(folder, false).await?;
        // Mail that came after the last sync. A server without UIDNEXT says nothing here.
        if opened.validity != since.validity || u64::from(opened.next) > since.next {
            return Ok(true);
        }
        if idles {
            return self.idle(wait).await;
        }
        let until = tokio::time::Instant::now() + wait;
        loop {
            let left = until.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            tokio::time::sleep(left.min(POLL)).await;
            let now = self.open(folder, false).await?;
            if (now.validity, now.next, now.exists) != (opened.validity, opened.next, opened.exists)
            {
                return Ok(true);
            }
        }
    }
}

impl ImapSession {
    /// The raw message with `remote_id`, without marking it read: the `partial` bytes of it,
    /// or all of it up to `MAX_RESPONSE`.
    async fn fetch_message(
        &mut self,
        folder: &str,
        validity: u32,
        remote_id: &str,
        partial: Option<(u32, NonZeroU32)>,
    ) -> Result<Vec<u8>, ProviderError> {
        let uid: u32 = remote_id
            .parse()
            .map_err(|_| protocol(format!("bad UID {remote_id}")))?;
        // A new UIDVALIDITY means the UID may name another message now.
        if self.open(folder, false).await?.validity != validity {
            return Err(ProviderError::Gone);
        }
        let items = vec![
            MessageDataItemName::Uid,
            MessageDataItemName::BodyExt {
                section: None,
                partial,
                peek: true,
            },
        ];
        let fetch =
            CommandBody::fetch(uid..=uid, items, true).map_err(|_| protocol("bad FETCH"))?;
        // The server may add FETCH replies about other messages, such as flag changes.
        // No reply for the UID means the message was expunged.
        self.run(fetch)
            .await?
            .data
            .into_iter()
            .find_map(|d| match d {
                Data::Fetch { items, .. } => message_bytes(items, uid),
                _ => None,
            })
            .ok_or(ProviderError::Gone)
    }

    /// Removes the messages in `sets` of the open folder for good.
    async fn purge(&mut self, sets: Vec<SequenceSet>) -> Result<(), ProviderError> {
        if sets.is_empty() {
            return Ok(());
        }
        for set in &sets {
            let store = CommandBody::store(
                set.clone(),
                StoreType::Add,
                StoreResponse::Silent,
                vec![Flag::Deleted],
                true,
            )
            .map_err(|_| protocol("bad STORE"))?;
            self.run(store).await?;
        }
        // RFC 4315 UID EXPUNGE removes only these; without it, EXPUNGE takes whatever else in
        // the folder is marked deleted, which in the trash is thrown away already.
        if !self.capabilities().await?.contains(&Capability::UidPlus) {
            return self.run(CommandBody::Expunge).await.map(|_| ());
        }
        for sequence_set in sets {
            self.run(CommandBody::ExpungeUid { sequence_set }).await?;
        }
        Ok(())
    }

    /// Opens `folder` for writing and runs one command per chunk of its UIDs.
    /// A UID that was expunged meanwhile is skipped by the server.
    async fn each_chunk<'a>(
        &mut self,
        folder: &str,
        validity: u32,
        remote_ids: &[String],
        command: impl Fn(&[NonZeroU32]) -> Result<CommandBody<'a>, ProviderError>,
    ) -> Result<Vec<Reply>, ProviderError> {
        let uids = uids(remote_ids)?;
        if uids.is_empty() {
            return Ok(vec![]);
        }
        if self.open(folder, true).await?.validity != validity {
            return Err(ProviderError::Gone);
        }
        // Chunks keep each command line well under server limits.
        let mut replies = vec![];
        for chunk in uids.chunks(UID_CHUNK) {
            replies.push(self.run(command(chunk)?).await?);
        }
        Ok(replies)
    }
}

/// The UIDs of a UIDPLUS set in its order, at most `limit` of them.
fn uid_list(set: &UidSet, limit: usize) -> impl Iterator<Item = u32> + '_ {
    set.0
        .as_ref()
        .iter()
        .flat_map(|element| match *element {
            UidElement::Single(uid) => uid.get()..=uid.get(),
            UidElement::Range(a, b) => a.get().min(b.get())..=a.get().max(b.get()),
        })
        .take(limit)
}

/// Message ids as IMAP UIDs.
fn uids(remote_ids: &[String]) -> Result<Vec<NonZeroU32>, ProviderError> {
    remote_ids
        .iter()
        .map(|id| id.parse::<NonZeroU32>())
        .collect::<Result<_, _>>()
        .map_err(|_| protocol("bad UID"))
}

/// The message bytes of a FETCH reply for `uid`.
fn message_bytes<'a>(
    items: impl IntoIterator<Item = MessageDataItem<'a>>,
    uid: u32,
) -> Option<Vec<u8>> {
    let (mut found, mut data) = (None, None);
    for item in items {
        match item {
            MessageDataItem::Uid(u) => found = Some(u.get()),
            MessageDataItem::BodyExt { data: d, .. } => {
                data = d.0.map(|s| s.into_inner().into_owned());
            }
            _ => {}
        }
    }
    data.filter(|_| found == Some(uid))
}

/// A LIST entry that can hold messages. On Gmail, archiving is moving to All Mail (`\All`),
/// and Important (`\Important`) is left out, as it only repeats mail from other folders.
fn folder(
    items: &[FlagNameAttribute<'_>],
    mailbox: &Mailbox<'_>,
    gmail: bool,
) -> Option<NewFolder> {
    let mut role = FolderRole::Custom;
    for item in items {
        match item {
            FlagNameAttribute::Noselect => return None,
            // Special-use roles and LIST-EXTENDED's \NonExistent arrive as extensions.
            FlagNameAttribute::Extension(_) => {
                let attr = item.to_string();
                if attr.eq_ignore_ascii_case("\\NonExistent")
                    || gmail && attr.eq_ignore_ascii_case("\\Important")
                {
                    return None;
                }
                role = match FolderRole::from_special_use(&attr) {
                    Some(special) => special,
                    None if gmail && attr.eq_ignore_ascii_case("\\All") => FolderRole::Archive,
                    None => role,
                };
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

/// A FETCH reply with everything `headers` asked for, or `None` if the server left something out.
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

/// The UID and flags of a FETCH reply, if it has both.
fn uid_flags<'a>(items: impl IntoIterator<Item = MessageDataItem<'a>>) -> Option<(u32, Flags)> {
    let (mut uid, mut flags) = (None, None);
    for item in items {
        match item {
            MessageDataItem::Uid(u) => uid = Some(u.get()),
            MessageDataItem::Flags(f) => flags = Some(parse_flags(&f)),
            _ => {}
        }
    }
    uid.zip(flags)
}

/// An untagged response saying a message came, went or changed in the open folder.
fn is_change(response: &Response<'_>) -> bool {
    matches!(
        response,
        Response::Data(Data::Exists(_) | Data::Expunge(_) | Data::Fetch { .. })
    )
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
    timed_for(TIMEOUT, op).await
}

async fn timed_for<T, E: std::fmt::Display>(
    limit: Duration,
    op: impl Future<Output = Result<T, E>>,
) -> Result<T, ProviderError> {
    match tokio::time::timeout(limit, op).await {
        Ok(result) => result.map_err(|e| ProviderError::Network(e.to_string())),
        Err(_) => Err(ProviderError::Network("timed out".into())),
    }
}

fn protocol(msg: impl Into<String>) -> ProviderError {
    ProviderError::Protocol(msg.into())
}

#[cfg(test)]
mod tests {
    use imap_codec::imap_types::core::Atom;

    use super::*;

    #[test]
    fn lists_a_uid_set_in_order_up_to_a_limit() {
        let n = |v| NonZeroU32::new(v).unwrap();
        let set = UidSet(
            Vec1::try_from(vec![
                UidElement::Single(n(7)),
                UidElement::Range(n(3), n(1)),
                UidElement::Range(n(10), n(u32::MAX)),
            ])
            .unwrap(),
        );
        assert_eq!(uid_list(&set, 6).collect::<Vec<_>>(), [7, 1, 2, 3, 10, 11]);
    }

    #[test]
    fn archives_to_all_mail_on_gmail_only() {
        let listed = |attr: &str, gmail| {
            let items = [FlagNameAttribute::from(Atom::try_from(attr).unwrap())];
            let mailbox = Mailbox::try_from("[Gmail]/Folder").unwrap();
            folder(&items, &mailbox, gmail).map(|f| f.role)
        };
        assert_eq!(listed("All", true), Some(FolderRole::Archive));
        assert_eq!(listed("All", false), Some(FolderRole::Custom));
        assert_eq!(listed("Important", true), None);
        assert_eq!(listed("Important", false), Some(FolderRole::Custom));
        assert_eq!(listed("Trash", true), Some(FolderRole::Trash));
        assert_eq!(listed("Sent", true), Some(FolderRole::Sent));
    }
}
