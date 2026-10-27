//! Exchange ActiveSync 14.1 (MS-ASCMD) provider, for Exchange servers that open only
//! ActiveSync to phones, as many companies do. It signs in through the EWS client: Basic or
//! NTLM, with the client certificate.
//!
//! The server keeps where each folder's sync stopped, under a sync key the client sends back;
//! keys live in the account's provider state. A folder's checkpoint counts its syncs, so a
//! sync whose answer was not saved asks again with the key before, which the server answers
//! the same way.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use mail_parser::DateTime;
use ozero_domain::{
    Answer, Changes, Checkpoint, Flags, FolderRole, MailProvider, MailSender, Moved, NewFolder,
    Older, ProviderError, ProviderState, RawMessage, Server,
};
use ozero_http::Tls;

use crate::client::Client;
use crate::discover::ACTIVE_SYNC;
use crate::wbxml::{Node, decode, encode};
use crate::{ARCHIVE_NAMES, POLL, dropped, header, path, protocol, random, time};

const VERSION: &str = "14.1";
const CONTENT_TYPE: &str = "application/vnd.ms-sync.wbxml";
/// How the device shows in the company's list of phones.
const DEVICE_TYPE: &str = "OzeroMail";
const POLICY_TYPE: &str = "MS-EAS-Provisioning-WBXML";
/// How much of a new message's MIME comes with a sync: room for its header.
const HEADER_SIZE: &str = "16384";
/// Most messages one sync returns: Exchange takes no more.
const MAX_WINDOW: u32 = 512;
/// Most items one request names.
const MAX_IDS: usize = 100;
/// Mail from the last month (FilterType 5), the longest span synced: older mail is not.
const MONTH: &str = "5";
/// The shortest and longest a Ping may wait, in seconds, as Exchange allows by default.
const MIN_HEARTBEAT: u64 = 60;
const MAX_HEARTBEAT: u64 = 3540;
/// What Outlook calls the junk folder, which ActiveSync does not mark.
const JUNK_NAMES: [&str; 3] = ["junk e-mail", "junk email", "нежелательная почта"];

/// A mailbox over ActiveSync.
pub struct EasSession {
    client: Client,
    user: String,
    device: String,
    /// What follows the command in each request's query.
    query: String,
    /// What the server gave the device once it took its policy; "0" before.
    policy: String,
    /// The FilterType: how far back mail is synced.
    filter: &'static str,
    state: Arc<dyn ProviderState>,
}

impl fmt::Debug for EasSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EasSession")
            .field("user", &self.user)
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

/// Where a folder's sync stopped: the `n`th sync got `key`, asked for with `prev`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Synced {
    n: u64,
    key: String,
    prev: String,
    /// The FilterType the key was synced under.
    filter: String,
}

/// A folder as FolderSync told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    id: String,
    parent: String,
    kind: String,
    name: String,
}

impl EasSession {
    /// For the ActiveSync endpoint at `server`, signed in as `server.user`, keeping its sync
    /// keys in `state`.
    pub fn new(
        tls: impl Into<Tls>,
        server: &Server,
        password: &str,
        state: Arc<dyn ProviderState>,
    ) -> Result<Self, ProviderError> {
        let client = Client::new(
            tls,
            &server.host,
            server.port,
            ACTIVE_SYNC,
            &server.user,
            password,
        );
        Self::over(client, &server.user, state)
    }

    pub(crate) fn over(
        client: Client,
        user: &str,
        state: Arc<dyn ProviderState>,
    ) -> Result<Self, ProviderError> {
        // A new id would make it another phone the company has to let in.
        let device = state
            .get("device")
            .ok_or_else(|| protocol("no device id"))?;
        Ok(Self {
            client,
            user: user.to_owned(),
            query: format!(
                "&User={}&DeviceId={device}&DeviceType={DEVICE_TYPE}",
                query(user)
            ),
            device,
            policy: state.get("policy").unwrap_or_else(|| "0".into()),
            filter: MONTH,
            state,
        })
    }

    /// Syncs only mail of the last `days` days, 0 for all, a month at most.
    pub fn keeping(mut self, days: u32) -> Self {
        self.filter = match days {
            1..=3 => "2",
            4..=7 => "3",
            _ => MONTH,
        };
        self
    }

    /// Gives a new account's device its id, before the first session.
    pub fn register(state: &dyn ProviderState) -> Result<(), ProviderError> {
        state.set("device", Some(&random_hex()?));
        Ok(())
    }

    /// Checks the login and that the server lets the device in: its folders are there.
    pub async fn check(&mut self) -> Result<(), ProviderError> {
        self.folder_list().await.map(drop)
    }

    /// The mailbox owner's name as the server knows it; None when it does not say.
    pub async fn name(&mut self) -> Result<Option<String>, ProviderError> {
        let request = Node::new(
            "Settings:Settings",
            vec![Node::new(
                "Settings:UserInformation",
                vec![Node::new("Settings:Get", vec![])],
            )],
        );
        let answer = self.command("Settings", &request, true).await?;
        let info = answer.at(&["Settings:UserInformation", "Settings:Get"]);
        Ok(info
            .and_then(|i| i.child("Settings:Accounts"))
            .into_iter()
            .flat_map(|a| a.all("Settings:Account"))
            .map(|a| a.text_at(&["Settings:UserDisplayName"]).trim())
            .find(|n| !n.is_empty())
            .map(str::to_owned))
    }

    /// Sends a command; provisions the device first if the server asks.
    async fn command(
        &mut self,
        cmd: &str,
        request: &Node,
        repeatable: bool,
    ) -> Result<Node, ProviderError> {
        self.command_waiting(cmd, request, repeatable, Duration::ZERO)
            .await
    }

    /// Like `command`, for a long poll: the server may hold its answer up to `wait`.
    async fn command_waiting(
        &mut self,
        cmd: &str,
        request: &Node,
        repeatable: bool,
        wait: Duration,
    ) -> Result<Node, ProviderError> {
        for provisioned in [false, true] {
            let answer = self.request(cmd, request, repeatable, wait).await?;
            let asks = answer
                .as_ref()
                .is_none_or(|a| matches!(status(a), 142..=144));
            if asks && !provisioned {
                self.provision().await?;
                continue;
            }
            let Some(answer) = answer else { break };
            if refused(status(&answer)) {
                return Err(ProviderError::Blocked);
            }
            return Ok(answer);
        }
        Err(protocol("the server keeps asking for the device's policy"))
    }

    /// One request: None if the server asks for the device's policy first. An empty answer
    /// is an empty node.
    async fn request(
        &mut self,
        cmd: &str,
        request: &Node,
        repeatable: bool,
        wait: Duration,
    ) -> Result<Option<Node>, ProviderError> {
        let path = format!("{ACTIVE_SYNC}?Cmd={cmd}{}", self.query);
        let headers = [
            ("MS-ASProtocolVersion", VERSION),
            ("X-MS-PolicyKey", self.policy.as_str()),
        ];
        let response = self
            .client
            .post_to(
                &path,
                &headers,
                CONTENT_TYPE,
                &encode(request),
                repeatable,
                wait,
            )
            .await?;
        match response.status {
            449 => Ok(None),
            403 => Err(ProviderError::Blocked),
            429 => Err(ProviderError::Network("HTTP 429".into())),
            _ if response.ok() && response.body.is_empty() => Ok(Some(Node::default())),
            _ if response.ok() => decode(&response.body)
                .map(Some)
                .ok_or_else(|| protocol("bad WBXML")),
            s if s >= 500 => Err(ProviderError::Network(format!("HTTP {s}"))),
            s => Err(ProviderError::Rejected(format!("HTTP {s}"))),
        }
    }

    /// Takes the server's policy for the device, in two steps: get it, then say it applies.
    async fn provision(&mut self) -> Result<(), ProviderError> {
        self.policy = "0".into();
        let device = Node::new(
            "Settings:DeviceInformation",
            vec![Node::new(
                "Settings:Set",
                vec![
                    Node::text("Settings:Model", "Ozero Mail"),
                    Node::text("Settings:FriendlyName", "Ozero Mail"),
                    Node::text("Settings:OS", std::env::consts::OS),
                    Node::text("Settings:UserAgent", DEVICE_TYPE),
                ],
            )],
        );
        let ask = Node::new("Provision:Provision", vec![device, policies(vec![])]);
        let answer = self.provisioning(&ask).await?;
        let policy = answer
            .at(&["Provision:Policies", "Provision:Policy"])
            .filter(|p| p.text_at(&["Provision:Status"]) == "1");
        // Without a policy for this device, the key stays 0.
        if let Some(policy) = policy {
            let key = policy.text_at(&["Provision:PolicyKey"]);
            self.policy = key.to_owned();
            let ack = Node::new(
                "Provision:Provision",
                vec![policies(vec![
                    Node::text("Provision:PolicyKey", key),
                    Node::text("Provision:Status", "1"),
                ])],
            );
            let answer = self.provisioning(&ack).await?;
            let key = answer.text_at(&[
                "Provision:Policies",
                "Provision:Policy",
                "Provision:PolicyKey",
            ]);
            if key.is_empty() {
                return Err(protocol("no policy key"));
            }
            self.policy = key.to_owned();
        }
        self.state.set("policy", Some(&self.policy));
        Ok(())
    }

    async fn provisioning(&mut self, request: &Node) -> Result<Node, ProviderError> {
        let answer = self
            .request("Provision", request, true, Duration::ZERO)
            .await?
            .ok_or_else(|| protocol("asked for the policy while taking it"))?;
        // A server that wants the phone wiped gets no answer to that.
        if answer.child("Provision:RemoteWipe").is_some() {
            return Err(ProviderError::Blocked);
        }
        match status(&answer) {
            1 => Ok(answer),
            s if refused(s) || s == 139 => Err(ProviderError::Blocked),
            s => Err(protocol(format!("Provision status {s}"))),
        }
    }

    /// Every folder, brought up to date with FolderSync.
    async fn folder_list(&mut self) -> Result<Vec<Entry>, ProviderError> {
        // A key is worth only as much as the list it was saved with.
        let saved = self.state.get("folders").map(|f| read_folders(&f));
        let (mut key, mut list) = match (self.state.get("folders.key"), saved) {
            (Some(key), Some(list)) => (key, list),
            _ => ("0".to_owned(), vec![]),
        };
        for _ in 0..2 {
            let request = Node::new(
                "FolderHierarchy:FolderSync",
                vec![Node::text("FolderHierarchy:SyncKey", key.clone())],
            );
            let answer = self.command("FolderSync", &request, true).await?;
            match status(&answer) {
                1 => {}
                // The server forgot the key: start over.
                9 if key != "0" => {
                    key = "0".into();
                    list.clear();
                    continue;
                }
                s => return Err(protocol(format!("FolderSync status {s}"))),
            }
            let new_key = answer.text_at(&["FolderHierarchy:SyncKey"]);
            let changes = answer.child("FolderHierarchy:Changes");
            for change in changes.into_iter().flat_map(|c| &c.children) {
                let id = change.text_at(&["FolderHierarchy:ServerId"]);
                list.retain(|e| e.id != id);
                if change.name != "FolderHierarchy:Delete" {
                    list.push(Entry {
                        id: id.to_owned(),
                        parent: change.text_at(&["FolderHierarchy:ParentId"]).to_owned(),
                        kind: change.text_at(&["FolderHierarchy:Type"]).to_owned(),
                        name: change.text_at(&["FolderHierarchy:DisplayName"]).to_owned(),
                    });
                }
            }
            if new_key != key {
                self.state.set("folders", Some(&write_folders(&list)));
                self.state.set("folders.key", Some(new_key));
            }
            return Ok(list);
        }
        Err(protocol("FolderSync keeps refusing its key"))
    }

    fn synced(&self, folder: &str) -> Option<Synced> {
        let saved = self.state.get(&format!("sync.{folder}"))?;
        let mut parts = saved.splitn(4, '\t');
        Some(Synced {
            n: parts.next()?.parse().ok()?,
            key: parts.next()?.to_owned(),
            prev: parts.next().unwrap_or_default().to_owned(),
            // Saved before there was a choice.
            filter: parts.next().unwrap_or(MONTH).to_owned(),
        })
    }

    fn keep_synced(&self, folder: &str, synced: Option<&Synced>) {
        let value = synced.map(|s| format!("{}\t{}\t{}\t{}", s.n, s.key, s.prev, s.filter));
        self.state.set(&format!("sync.{folder}"), value.as_deref());
    }

    /// The folder's validity: one more each time its sync starts over.
    fn generation(&self, folder: &str) -> u32 {
        self.state
            .get(&format!("gen.{folder}"))
            .and_then(|g| g.parse().ok())
            .unwrap_or(0)
    }

    /// One Sync of a folder: the collection answered, None if nothing changed.
    async fn sync(
        &mut self,
        key: &str,
        folder: &str,
        rest: Vec<Node>,
        repeatable: bool,
    ) -> Result<Option<Node>, ProviderError> {
        let collection = [
            Node::text("AirSync:SyncKey", key),
            Node::text("AirSync:CollectionId", folder),
        ]
        .into_iter()
        .chain(rest)
        .collect();
        let request = Node::new(
            "AirSync:Sync",
            vec![Node::new(
                "AirSync:Collections",
                vec![Node::new("AirSync:Collection", collection)],
            )],
        );
        let mut answer = self.command("Sync", &request, repeatable).await?;
        if answer.name.is_empty() {
            return Ok(None);
        }
        match status(&answer) {
            0 => Ok(answer.take_at(&["AirSync:Collections", "AirSync:Collection"])),
            s => Err(protocol(format!("Sync status {s}"))),
        }
    }

    /// A first key for the folder, which syncs nothing yet.
    async fn first_key(&mut self, folder: &str) -> Result<String, ProviderError> {
        let collection = self
            .sync("0", folder, vec![], true)
            .await?
            .ok_or_else(|| protocol("no first sync key"))?;
        match status(&collection) {
            1 => Ok(collection.text_at(&["AirSync:SyncKey"]).to_owned()),
            s => Err(sync_error(s)),
        }
    }

    /// Sends `commands` for messages of a folder, a hundred at a time, without taking the
    /// folder's changes; `options` go before them.
    async fn change(
        &mut self,
        folder: &str,
        options: &[Node],
        commands: Vec<Node>,
        repeatable: bool,
    ) -> Result<(), ProviderError> {
        let mut synced = self.synced(folder);
        for chunk in commands.chunks(MAX_IDS) {
            for tried in [false, true] {
                let current = match synced.take() {
                    Some(s) => s,
                    None => Synced {
                        n: 0,
                        key: self.first_key(folder).await?,
                        prev: String::new(),
                        filter: self.filter.to_owned(),
                    },
                };
                let rest = options
                    .iter()
                    .cloned()
                    .chain([
                        Node::text("AirSync:GetChanges", "0"),
                        Node::new("AirSync:Commands", chunk.to_vec()),
                    ])
                    .collect();
                let answer = self.sync(&current.key, folder, rest, repeatable).await?;
                let Some(collection) = answer else {
                    synced = Some(current);
                    break;
                };
                // 8: the message is no longer there.
                let failed = collection
                    .child("AirSync:Responses")
                    .into_iter()
                    .flat_map(|r| &r.children)
                    .map(status)
                    .find(|s| !matches!(s, 1 | 8));
                match (status(&collection), failed) {
                    (1, Some(s)) => return Err(protocol(format!("Sync change status {s}"))),
                    (1, None) => {
                        // The key before is not the one the folder's checkpoint was saved with.
                        let next = Synced {
                            key: collection.text_at(&["AirSync:SyncKey"]).to_owned(),
                            prev: String::new(),
                            ..current
                        };
                        self.keep_synced(folder, Some(&next));
                        synced = Some(next);
                        break;
                    }
                    // The server forgot the key: the next sync starts over.
                    (3, _) if !tried => self.keep_synced(folder, None),
                    (s, _) => return Err(sync_error(s)),
                }
            }
        }
        Ok(())
    }

    /// Fetches items of a folder with `options`: the properties of each one there.
    async fn fetch(
        &mut self,
        folder: &str,
        ids: &[String],
        options: Vec<Node>,
    ) -> Result<Vec<(String, Node)>, ProviderError> {
        let mut found = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(MAX_IDS) {
            let fetches = chunk
                .iter()
                .map(|id| {
                    Node::new(
                        "ItemOperations:Fetch",
                        vec![
                            Node::text("ItemOperations:Store", "Mailbox"),
                            Node::text("AirSync:CollectionId", folder),
                            Node::text("AirSync:ServerId", id.as_str()),
                            Node::new("ItemOperations:Options", options.clone()),
                        ],
                    )
                })
                .collect();
            let request = Node::new("ItemOperations:ItemOperations", fetches);
            let mut answer = self.command("ItemOperations", &request, true).await?;
            match status(&answer) {
                1 => {}
                s => return Err(protocol(format!("ItemOperations status {s}"))),
            }
            let fetched = answer.take_at(&["ItemOperations:Response"]).into_iter();
            for mut fetch in fetched.flat_map(|r| r.children) {
                // An item no longer there is left out.
                if fetch.name != "ItemOperations:Fetch"
                    || fetch.text_at(&["ItemOperations:Status"]) != "1"
                {
                    continue;
                }
                if let Some(properties) = fetch.take_at(&["ItemOperations:Properties"]) {
                    let id = fetch.text_at(&["AirSync:ServerId"]).to_owned();
                    found.push((id, properties));
                }
            }
        }
        Ok(found)
    }

    /// Adds what a folder's Sync answer tells of to `changes`; flags a change leaves out
    /// are fetched. Known messages count only if the sync is not `fresh`.
    async fn take_changes(
        &mut self,
        folder: &str,
        collection: &Node,
        known: &[String],
        fresh: bool,
        changes: &mut Changes,
    ) -> Result<(), ProviderError> {
        let known: HashSet<&str> = if fresh {
            HashSet::new()
        } else {
            known.iter().map(String::as_str).collect()
        };
        let mut refetch = Vec::new();
        let commands = collection.child("AirSync:Commands");
        for command in commands.into_iter().flat_map(|c| &c.children) {
            let id = command.text_at(&["AirSync:ServerId"]);
            let data = command.child("AirSync:ApplicationData");
            match command.name {
                "AirSync:Add" if !known.contains(id) => {
                    changes.new.extend(data.and_then(|d| raw_message(id, d)));
                }
                "AirSync:Add" | "AirSync:Change" if known.contains(id) => {
                    // A change carries only what changed.
                    let full = |d: &&Node| {
                        ["Email:Read", "Email:Flag", "Email2:LastVerbExecuted"]
                            .iter()
                            .all(|n| d.child(n).is_some())
                    };
                    match data.filter(full).map(flags) {
                        Some(f) => changes.flags.push((id.to_owned(), f)),
                        None => refetch.push(id.to_owned()),
                    }
                }
                "AirSync:Delete" if known.contains(id) => changes.gone.push(id.to_owned()),
                // A soft delete only leaves the month synced; the mail stays.
                _ => {}
            }
        }
        if !refetch.is_empty() {
            let options = vec![Node::new(
                "AirSyncBase:BodyPreference",
                vec![
                    Node::text("AirSyncBase:Type", "1"),
                    Node::text("AirSyncBase:TruncationSize", "0"),
                ],
            )];
            for (id, properties) in self.fetch(folder, &refetch, options).await? {
                changes.flags.push((id, flags(&properties)));
            }
        }
        Ok(())
    }
}

impl MailProvider for EasSession {
    async fn folders(&mut self) -> Result<Vec<NewFolder>, ProviderError> {
        let list = self.folder_list().await?;
        let by_id: HashMap<&str, &Entry> = list.iter().map(|e| (e.id.as_str(), e)).collect();
        let (mut archived, mut junked) = (false, false);
        let mut folders = Vec::new();
        for e in &list {
            let Some(role) = role(&e.kind) else {
                continue;
            };
            let (name, top) = path(&e.name, &e.parent, |id| {
                by_id.get(id).map(|p| (p.name.as_str(), p.parent.as_str()))
            });
            let lower = e.name.to_lowercase();
            let role = match role {
                FolderRole::Custom
                    if top && !archived && ARCHIVE_NAMES.contains(&lower.as_str()) =>
                {
                    archived = true;
                    FolderRole::Archive
                }
                FolderRole::Custom if top && !junked && JUNK_NAMES.contains(&lower.as_str()) => {
                    junked = true;
                    FolderRole::Junk
                }
                FolderRole::Custom if top => FolderRole::from_name(&e.name),
                role => role,
            };
            folders.push(NewFolder {
                remote_id: e.id.clone(),
                name,
                role,
            });
        }
        Ok(folders)
    }

    async fn changes(
        &mut self,
        folder: &str,
        since: Checkpoint,
        known: &[String],
        _oldest: i64,
        limit: u32,
    ) -> Result<Changes, ProviderError> {
        let mut changes = Changes {
            checkpoint: since,
            new: vec![],
            flags: vec![],
            gone: vec![],
        };
        for _ in 0..2 {
            let generation = self.generation(folder);
            // Under another span the folder starts over.
            let saved = self.synced(folder).filter(|s| {
                since.validity == generation && since.next > 0 && s.filter == self.filter
            });
            // The key the checkpoint was saved after, or the one before if that sync's
            // answer was not saved.
            let key = saved.and_then(|s| {
                if since.next == s.n {
                    Some(s.key)
                } else if since.next + 1 == s.n && !s.prev.is_empty() {
                    Some(s.prev)
                } else {
                    None
                }
            });
            let fresh = key.is_none();
            let (key, base, validity) = match key {
                Some(key) => (key, since.next, generation),
                None => {
                    // Saved messages may be ones the server now names otherwise.
                    let validity = if since == Checkpoint::default() {
                        generation
                    } else {
                        generation.wrapping_add(1)
                    };
                    self.state
                        .set(&format!("gen.{folder}"), Some(&validity.to_string()));
                    (self.first_key(folder).await?, 0, validity)
                }
            };
            let rest = vec![
                Node::new("AirSync:GetChanges", vec![]),
                Node::text("AirSync:WindowSize", limit.clamp(1, MAX_WINDOW).to_string()),
                Node::new(
                    "AirSync:Options",
                    vec![
                        Node::text("AirSync:FilterType", self.filter),
                        Node::text("AirSync:MIMESupport", "2"),
                        Node::new(
                            "AirSyncBase:BodyPreference",
                            vec![
                                Node::text("AirSyncBase:Type", "4"),
                                Node::text("AirSyncBase:TruncationSize", HEADER_SIZE),
                            ],
                        ),
                    ],
                ),
            ];
            // Nothing changed when it does not answer: the key stays.
            let mut new_key = key.clone();
            if let Some(collection) = self.sync(&key, folder, rest, true).await? {
                match status(&collection) {
                    1 => {}
                    // The server forgot the key: start over.
                    3 if !fresh => {
                        self.keep_synced(folder, None);
                        continue;
                    }
                    s => return Err(sync_error(s)),
                }
                self.take_changes(folder, &collection, known, fresh, &mut changes)
                    .await?;
                let answered = collection.text_at(&["AirSync:SyncKey"]);
                if !answered.is_empty() {
                    answered.clone_into(&mut new_key);
                }
            }
            let next = Synced {
                n: base + 1,
                key: new_key,
                prev: key,
                filter: self.filter.to_owned(),
            };
            self.keep_synced(folder, Some(&next));
            changes.checkpoint = Checkpoint {
                validity,
                next: next.n,
            };
            changes.new.sort_by_key(|m| m.received);
            return Ok(changes);
        }
        Err(protocol("Sync keeps refusing its key"))
    }

    async fn older(
        &mut self,
        _folder: &str,
        _validity: u32,
        _known: &[String],
        _oldest: i64,
        _limit: u32,
    ) -> Result<Older, ProviderError> {
        // Only the last month syncs.
        Ok(Older {
            new: vec![],
            more: false,
        })
    }

    async fn search(
        &mut self,
        _folder: &str,
        _validity: u32,
        _text: &str,
        _limit: u32,
    ) -> Result<Vec<RawMessage>, ProviderError> {
        Ok(vec![])
    }

    async fn body(
        &mut self,
        folder: &str,
        _validity: u32,
        remote_id: &str,
    ) -> Result<Vec<u8>, ProviderError> {
        let options = vec![
            Node::text("AirSync:MIMESupport", "2"),
            Node::new(
                "AirSyncBase:BodyPreference",
                vec![Node::text("AirSyncBase:Type", "4")],
            ),
        ];
        let fetched = self.fetch(folder, &[remote_id.to_owned()], options).await?;
        let (_, mut properties) = fetched.into_iter().next().ok_or(ProviderError::Gone)?;
        properties
            .take_at(&["AirSyncBase:Body", "AirSyncBase:Data"])
            .map(|d| d.data)
            .filter(|m| !m.is_empty())
            .ok_or_else(|| protocol("no MIME content"))
    }

    async fn set_seen(
        &mut self,
        folder: &str,
        _validity: u32,
        remote_ids: &[String],
        seen: bool,
    ) -> Result<(), ProviderError> {
        let read = if seen { "1" } else { "0" };
        let commands = remote_ids
            .iter()
            .map(|id| {
                Node::new(
                    "AirSync:Change",
                    vec![
                        Node::text("AirSync:ServerId", id.as_str()),
                        Node::new(
                            "AirSync:ApplicationData",
                            vec![Node::text("Email:Read", read)],
                        ),
                    ],
                )
            })
            .collect();
        self.change(folder, &[], commands, true).await
    }

    async fn set_flagged(
        &mut self,
        folder: &str,
        _validity: u32,
        remote_ids: &[String],
        flagged: bool,
    ) -> Result<(), ProviderError> {
        // Status 2 is flagged; an empty flag takes it off.
        let flag = || {
            if flagged {
                Node::new(
                    "Email:Flag",
                    vec![
                        Node::text("Email:Status", "2"),
                        Node::text("Email:FlagType", "Flag for follow up"),
                    ],
                )
            } else {
                Node::new("Email:Flag", vec![])
            }
        };
        let commands = remote_ids
            .iter()
            .map(|id| {
                Node::new(
                    "AirSync:Change",
                    vec![
                        Node::text("AirSync:ServerId", id.as_str()),
                        Node::new("AirSync:ApplicationData", vec![flag()]),
                    ],
                )
            })
            .collect();
        self.change(folder, &[], commands, true).await
    }

    async fn move_to(
        &mut self,
        folder: &str,
        _validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> Result<Moved, ProviderError> {
        let mut ids = Vec::new();
        for chunk in remote_ids.chunks(MAX_IDS) {
            let moves = chunk
                .iter()
                .map(|id| {
                    Node::new(
                        "Move:Move",
                        vec![
                            Node::text("Move:SrcMsgId", id.as_str()),
                            Node::text("Move:SrcFldId", folder),
                            Node::text("Move:DstFldId", to),
                        ],
                    )
                })
                .collect();
            let request = Node::new("Move:MoveItems", moves);
            // Not repeatable: a second move finds the mail gone and loses its new ids.
            let answer = self.command("MoveItems", &request, false).await?;
            for response in answer.all("Move:Response") {
                let new = response.text_at(&["Move:DstMsgId"]);
                // 3 is done; a message no longer there is skipped.
                if response.text_at(&["Move:Status"]) == "3" && !new.is_empty() {
                    let old = response.text_at(&["Move:SrcMsgId"]);
                    ids.push((old.to_owned(), new.to_owned()));
                }
            }
        }
        Ok(Moved {
            validity: Some(self.generation(to)),
            ids,
        })
    }

    async fn delete(
        &mut self,
        folder: &str,
        _validity: u32,
        remote_ids: &[String],
    ) -> Result<(), ProviderError> {
        let commands = remote_ids
            .iter()
            .map(|id| {
                Node::new(
                    "AirSync:Delete",
                    vec![Node::text("AirSync:ServerId", id.as_str())],
                )
            })
            .collect();
        // For good, not into Deleted Items.
        let options = [Node::text("AirSync:DeletesAsMoves", "0")];
        self.change(folder, &options, commands, true).await
    }

    async fn empty(
        &mut self,
        folder: &str,
        _validity: u32,
        keep: &[String],
    ) -> Result<(), ProviderError> {
        if !keep.is_empty() {
            return Err(ProviderError::Rejected(
                "ActiveSync empties only a whole folder".into(),
            ));
        }
        let request = Node::new(
            "ItemOperations:ItemOperations",
            vec![Node::new(
                "ItemOperations:EmptyFolderContents",
                vec![Node::text("AirSync:CollectionId", folder)],
            )],
        );
        let answer = self.command("ItemOperations", &request, true).await?;
        let done = answer
            .at(&[
                "ItemOperations:Response",
                "ItemOperations:EmptyFolderContents",
            ])
            .map_or("", |e| e.text_at(&["ItemOperations:Status"]));
        match (status(&answer), done) {
            (1, "1") => Ok(()),
            (s, e) => Err(ProviderError::Rejected(format!(
                "EmptyFolderContents status {s}, {e}"
            ))),
        }
    }

    async fn create_snoozed(&mut self, name: &str) -> Result<NewFolder, ProviderError> {
        // FolderCreate takes the key of a list that is up to date.
        let mut list = self.folder_list().await?;
        let key = self.state.get("folders.key").unwrap_or_else(|| "0".into());
        let request = Node::new(
            "FolderHierarchy:FolderCreate",
            vec![
                Node::text("FolderHierarchy:SyncKey", key),
                Node::text("FolderHierarchy:ParentId", "0"),
                Node::text("FolderHierarchy:DisplayName", name),
                Node::text("FolderHierarchy:Type", "12"),
            ],
        );
        let answer = self.command("FolderCreate", &request, false).await?;
        match status(&answer) {
            1 => {
                let id = answer.text_at(&["FolderHierarchy:ServerId"]).to_owned();
                list.push(Entry {
                    id: id.clone(),
                    parent: "0".into(),
                    kind: "12".into(),
                    name: name.to_owned(),
                });
                self.state.set("folders", Some(&write_folders(&list)));
                let new_key = answer.text_at(&["FolderHierarchy:SyncKey"]);
                self.state.set("folders.key", Some(new_key));
                Ok(NewFolder {
                    remote_id: id,
                    name: name.to_owned(),
                    role: FolderRole::Snoozed,
                })
            }
            // There already, made meanwhile by another device.
            2 => self
                .folders()
                .await?
                .into_iter()
                .find(|f| f.role == FolderRole::Snoozed)
                .ok_or_else(|| ProviderError::Rejected(format!("{name} is taken"))),
            s => Err(ProviderError::Rejected(format!("FolderCreate status {s}"))),
        }
    }

    async fn append(&mut self, _folder: &str, _message: &[u8]) -> Result<(), ProviderError> {
        Err(ProviderError::Rejected(
            "ActiveSync saves no message into a folder".into(),
        ))
    }

    async fn answer(
        &mut self,
        folder: &str,
        _validity: u32,
        remote_id: &str,
        answer: Answer,
    ) -> Result<(), ProviderError> {
        // ActiveSync's UserResponse numbers the answers as `Answer::code` does.
        let response = answer.code().to_string();
        // The server puts the meeting in the calendar; the organizer hears by mail, which
        // the app sends itself, as ActiveSync before 16.0 leaves it to the device.
        let request = Node::new(
            "MeetingResponse:MeetingResponse",
            vec![Node::new(
                "MeetingResponse:Request",
                vec![
                    Node::text("MeetingResponse:UserResponse", response),
                    Node::text("MeetingResponse:CollectionId", folder),
                    Node::text("MeetingResponse:RequestId", remote_id),
                ],
            )],
        );
        let answer = self.command("MeetingResponse", &request, true).await?;
        match answer.text_at(&["MeetingResponse:Result", "MeetingResponse:Status"]) {
            "1" => Ok(()),
            // Not a meeting request, or no longer there.
            "2" => Err(ProviderError::Gone),
            "4" => Err(ProviderError::Network("MeetingResponse status 4".into())),
            status => Err(ProviderError::Rejected(format!(
                "MeetingResponse status {status}"
            ))),
        }
    }

    async fn wait(
        &mut self,
        folder: &str,
        _since: Checkpoint,
        wait: Duration,
    ) -> Result<bool, ProviderError> {
        // Ping answers once the folder has changed since its last sync, or when the heartbeat
        // is over: mail comes the moment the server has it.
        let mut heartbeat = wait.as_secs().clamp(MIN_HEARTBEAT, MAX_HEARTBEAT);
        for _ in 0..2 {
            let request = Node::new(
                "Ping:Ping",
                vec![
                    Node::text("Ping:HeartbeatInterval", heartbeat.to_string()),
                    Node::new(
                        "Ping:Folders",
                        vec![Node::new(
                            "Ping:Folder",
                            vec![
                                Node::text("Ping:Id", folder),
                                Node::text("Ping:Class", "Email"),
                            ],
                        )],
                    ),
                ],
            );
            let answer = self
                .command_waiting("Ping", &request, true, Duration::from_secs(heartbeat))
                .await?;
            match status(&answer) {
                1 => return Ok(false),
                // The folder changed, or the folder list did: a sync finds out what.
                2 | 7 => return Ok(true),
                // Out of the server's range: it says the nearest it takes.
                5 => match answer.text_at(&["Ping:HeartbeatInterval"]).trim().parse() {
                    Ok(taken) if taken != heartbeat => heartbeat = taken,
                    _ => break,
                },
                _ => break,
            }
        }
        // A server that does not ping is looked at now and then; a sync with nothing new costs
        // one small request.
        tokio::time::sleep(wait.min(POLL)).await;
        Ok(wait >= POLL)
    }
}

impl MailSender for EasSession {
    async fn send(
        &mut self,
        _from: &str,
        _to: &[String],
        message: &[u8],
    ) -> Result<(), ProviderError> {
        // Exchange reads the recipients from the message and keeps a copy in Sent.
        let request = Node::new(
            "ComposeMail:SendMail",
            vec![
                Node::text("ComposeMail:ClientId", random_hex()?),
                Node::new("ComposeMail:SaveInSentItems", vec![]),
                Node::text("ComposeMail:Mime", message),
            ],
        );
        let answer = self.command("SendMail", &request, false).await?;
        match status(&answer) {
            0 | 1 => Ok(()),
            s => Err(ProviderError::Rejected(format!("SendMail status {s}"))),
        }
    }
}

fn policies(more: Vec<Node>) -> Node {
    let policy = [Node::text("Provision:PolicyType", POLICY_TYPE)]
        .into_iter()
        .chain(more)
        .collect();
    Node::new(
        "Provision:Policies",
        vec![Node::new("Provision:Policy", policy)],
    )
}

/// The status a command answered with; 0 without one.
fn status(answer: &Node) -> u32 {
    answer
        .children
        .iter()
        .find(|c| c.name.ends_with(":Status"))
        .and_then(|s| s.text_at(&[]).trim().parse().ok())
        .unwrap_or(0)
}

fn sync_error(status: u32) -> ProviderError {
    match status {
        // The folder is gone, or the hierarchy changed: the next folder sync sees it.
        8 | 12 => ProviderError::Network(format!("Sync status {status}")),
        s => protocol(format!("Sync status {s}")),
    }
}

/// The company refused the device or the user (MS-ASCMD common status codes).
fn refused(status: u32) -> bool {
    matches!(status, 126 | 129..=131 | 140 | 141 | 145 | 177)
}

/// The role of a folder of this FolderSync type; None for one that holds no mail.
fn role(kind: &str) -> Option<FolderRole> {
    match kind {
        "2" => Some(FolderRole::Inbox),
        "3" => Some(FolderRole::Drafts),
        "4" => Some(FolderRole::Trash),
        "5" => Some(FolderRole::Sent),
        "1" | "12" => Some(FolderRole::Custom),
        _ => None,
    }
}

fn read_folders(saved: &str) -> Vec<Entry> {
    saved
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            Some(Entry {
                id: parts.next()?.to_owned(),
                parent: parts.next()?.to_owned(),
                kind: parts.next()?.to_owned(),
                name: parts.next()?.to_owned(),
            })
        })
        .collect()
}

fn write_folders(list: &[Entry]) -> String {
    let plain = |s: &str| s.replace(['\t', '\r', '\n'], " ");
    list.iter()
        .map(|e| {
            format!(
                "{}\t{}\t{}\t{}\n",
                plain(&e.id),
                plain(&e.parent),
                plain(&e.kind),
                plain(&e.name)
            )
        })
        .collect()
}

/// The flags of a message's properties.
fn flags(data: &Node) -> Flags {
    Flags {
        seen: data.text_at(&["Email:Read"]) == "1",
        flagged: data.text_at(&["Email:Flag", "Email:Status"]) == "2",
        // 1 replied, 2 replied to all.
        answered: matches!(data.text_at(&["Email2:LastVerbExecuted"]), "1" | "2"),
        draft: false,
    }
}

/// A new message: its header from the start of its MIME, or from its properties when the
/// header is longer than what came.
fn raw_message(id: &str, data: &Node) -> Option<RawMessage> {
    if id.is_empty() {
        return None;
    }
    let received = time(data.text_at(&["Email:DateReceived"]));
    let header = data
        .at(&["AirSyncBase:Body", "AirSyncBase:Data"])
        .and_then(|d| header_of(&d.data))
        .unwrap_or_else(|| header_from(data, received));
    Some(RawMessage {
        remote_id: id.to_owned(),
        flags: flags(data),
        received: received.unwrap_or(0),
        header,
    })
}

/// The header block of a MIME message, without what servers add on the way; None if it is
/// cut short.
fn header_of(mime: &[u8]) -> Option<Vec<u8>> {
    let end = mime
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 2)
        .or_else(|| mime.windows(2).position(|w| w == b"\n\n").map(|p| p + 1))?;
    let mut header = Vec::with_capacity(end + 2);
    let mut keep = false;
    for line in mime[..end].split_inclusive(|&b| b == b'\n') {
        if !line.starts_with(b" ") && !line.starts_with(b"\t") {
            let name = line.split(|&b| b == b':').next().unwrap_or_default();
            keep = !dropped(&String::from_utf8_lossy(name));
        }
        if keep {
            header.extend_from_slice(line);
        }
    }
    header.extend_from_slice(b"\r\n");
    Some(header)
}

/// A header made from a message's properties.
fn header_from(data: &Node, received: Option<i64>) -> Vec<u8> {
    let date = received.map(|t| DateTime::from_timestamp(t).to_rfc822());
    header([
        ("From", data.text_at(&["Email:From"])),
        ("To", data.text_at(&["Email:To"])),
        ("Cc", data.text_at(&["Email:Cc"])),
        ("Reply-To", data.text_at(&["Email:ReplyTo"])),
        ("Subject", data.text_at(&["Email:Subject"])),
        ("Date", date.as_deref().unwrap_or_default()),
    ])
}

/// A random id of 16 hex digits.
fn random_hex() -> Result<String, ProviderError> {
    Ok(random::<8>()?.iter().map(|b| format!("{b:02X}")).collect())
}

/// `value` for a URL query.
fn query(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(b).to_string()
            }
            b => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests;
