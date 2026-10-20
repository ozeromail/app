//! Gmail API provider (`MailProvider`, `MailSender`) and Google sign-in.
//!
//! Folders are Gmail's system labels; the archive is a made-up one, since archived mail is
//! mail without INBOX. A folder's checkpoint is the mailbox's history id.

pub mod oauth;

mod batch;
mod codec;

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ozero_domain::{
    Changes, Checkpoint, Flags, FolderRole, MailProvider, MailSender, NewFolder, ProviderError,
    RawMessage,
};
use ozero_http::{Body, HttpsClient, Response};
use serde_json::{Value, json};
use tokio_rustls::rustls::ClientConfig;

use crate::codec::{base64url, decode_base64, percent};

const HOST: &str = "gmail.googleapis.com";
const BASE: &str = "/gmail/v1/users/me";
/// Archived mail: no label of its own, only not in INBOX.
const ARCHIVE: &str = "ARCHIVE";
/// Most ids one batchModify takes.
const MAX_MODIFY: usize = 1000;
/// Most messages a history page names; more since the last sync and the folder starts over.
const MAX_TOUCHED: usize = 500;
/// Most ids one list call returns.
const MAX_LIST: u32 = 500;
/// Most requests in one batch, and the least time between batches: together they stay under
/// a user's quota of 250 units a second, at 5 units a message.
const MAX_BATCH: usize = 25;
const BATCH_PACE: Duration = Duration::from_millis(500);
/// How often `wait` asks whether the mailbox changed: one cheap call, but each wakes the radio.
const POLL: Duration = Duration::from_secs(60);

/// A Gmail mailbox, signed in with an access token.
#[derive(Debug)]
pub struct GmailSession {
    http: HttpsClient,
    auth: String,
    /// Where the last history read started, what it found and where it ended.
    last_history: Option<(u64, Vec<Touched>, u64)>,
    /// When the last batch went out.
    last_batch: Option<Instant>,
}

impl GmailSession {
    pub fn new(tls: Arc<ClientConfig>, access_token: &str) -> Self {
        Self::over(HttpsClient::new(HOST, 443, tls), access_token)
    }

    fn over(http: HttpsClient, access_token: &str) -> Self {
        Self {
            http,
            auth: format!("Bearer {access_token}"),
            last_history: None,
            last_batch: None,
        }
    }

    /// The mailbox's address.
    pub async fn email(&mut self) -> Result<String, ProviderError> {
        let profile = self.get("/profile").await?;
        profile["emailAddress"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| protocol("no address in the profile"))
    }

    async fn get(&mut self, path: &str) -> Result<Value, ProviderError> {
        self.call("GET", path, None).await
    }

    async fn post(&mut self, path: &str, body: &Value) -> Result<Value, ProviderError> {
        self.call("POST", path, Some(body)).await
    }

    async fn call(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, ProviderError> {
        let data = body.map(Value::to_string);
        let body = data.as_deref().map(|d| Body {
            content_type: "application/json",
            data: d.as_bytes(),
        });
        let path = format!("{BASE}{path}");
        let response = self
            .http
            .send(method, &path, &[("Authorization", &self.auth)], body)
            .await?;
        answer(&response)
    }

    /// GETs `paths` in one round trip; the answer to each, in order.
    async fn batch(
        &mut self,
        paths: &[String],
    ) -> Result<Vec<Result<Value, ProviderError>>, ProviderError> {
        if let Some(last) = self.last_batch {
            tokio::time::sleep_until((last + BATCH_PACE).into()).await;
        }
        self.last_batch = Some(Instant::now());
        let body = batch::request(paths);
        let body = Body {
            content_type: batch::CONTENT_TYPE,
            data: body.as_bytes(),
        };
        // Only GETs inside, so safe to send again.
        let response = self
            .http
            .send_repeatable(
                "POST",
                "/batch/gmail/v1",
                &[("Authorization", &self.auth)],
                Some(body),
            )
            .await?;
        if !response.ok() {
            return Err(error(&response));
        }
        let answers = batch::response(&response.content_type, &response.body, paths.len())
            .ok_or_else(|| protocol("bad batch"))?;
        Ok(answers.iter().map(answer).collect())
    }

    async fn history_id(&mut self) -> Result<u64, ProviderError> {
        let profile = self.get("/profile").await?;
        number(&profile["historyId"]).ok_or_else(|| protocol("no history id"))
    }

    /// The newest `limit` messages with `label`, oldest first.
    async fn newest(&mut self, label: &str, limit: u32) -> Result<Vec<RawMessage>, ProviderError> {
        let list = self
            .get(&format!(
                "/messages?{}&maxResults={}&fields=messages/id",
                listing(label),
                limit.min(MAX_LIST)
            ))
            .await?;
        let ids = each(&list["messages"], |m| &m["id"]);
        let mut messages: Vec<_> = self
            .headers(&ids)
            .await?
            .into_iter()
            .map(|m| m.raw)
            .collect();
        messages.sort_by_key(|m| m.received);
        Ok(messages)
    }

    /// The labels and header of each message still there. What is over a rate limit is
    /// asked again in a later batch, as long as each batch gets something through.
    async fn headers(&mut self, ids: &[String]) -> Result<Vec<Labeled>, ProviderError> {
        let mut found = Vec::with_capacity(ids.len());
        let mut queue: VecDeque<&String> = ids.iter().collect();
        while !queue.is_empty() {
            let chunk: Vec<&String> = queue.drain(..queue.len().min(MAX_BATCH)).collect();
            let paths: Vec<String> = chunk
                .iter()
                .map(|id| {
                    format!(
                        "{BASE}/messages/{}?format=metadata&fields=id,labelIds,internalDate,payload/headers",
                        percent(id)
                    )
                })
                .collect();
            let (mut limited, mut last) = (0, None);
            for (id, answer) in chunk.iter().zip(self.batch(&paths).await?) {
                match answer {
                    Ok(json) => {
                        found.push(raw_message(&json).ok_or_else(|| protocol("bad message"))?)
                    }
                    // Deleted between the list and here.
                    Err(ProviderError::Gone) => {}
                    Err(e @ ProviderError::Network(_)) => {
                        queue.push_back(id);
                        limited += 1;
                        last = Some(e);
                    }
                    Err(e) => return Err(e),
                }
            }
            // Nothing got through: the sync tries again later.
            if limited == chunk.len()
                && let Some(e) = last
            {
                return Err(e);
            }
        }
        Ok(found)
    }

    /// The messages that changed since `start`, in the order they changed, and where the
    /// history is now. `Gone` if `start` is too old or too much changed. The inbox and the
    /// sent folder ask for the same history in one sync, so the last answer is kept.
    async fn history(&mut self, start: u64) -> Result<(Vec<Touched>, u64), ProviderError> {
        if let Some((from, touched, now)) = &self.last_history
            && *from == start
        {
            return Ok((touched.clone(), *now));
        }
        let fresh = self.read_history(start).await?;
        self.last_history = Some((start, fresh.0.clone(), fresh.1));
        Ok(fresh)
    }

    async fn read_history(&mut self, start: u64) -> Result<(Vec<Touched>, u64), ProviderError> {
        let mut touched: Vec<Touched> = Vec::new();
        let mut page = String::new();
        loop {
            let json = self
                .get(&format!(
                    "/history?startHistoryId={start}&maxResults={MAX_TOUCHED}{page}"
                ))
                .await?;
            for record in json["history"].as_array().into_iter().flatten() {
                for kind in [
                    "messagesAdded",
                    "messagesDeleted",
                    "labelsAdded",
                    "labelsRemoved",
                ] {
                    for change in record[kind].as_array().into_iter().flatten() {
                        let message = &change["message"];
                        let Some(id) = message["id"].as_str() else {
                            continue;
                        };
                        // A deleted message is in no folder.
                        let labels = if kind == "messagesDeleted" {
                            vec![]
                        } else {
                            strings(&message["labelIds"])
                        };
                        match touched.iter_mut().find(|t| t.id == id) {
                            Some(t) => t.labels = labels,
                            None => touched.push(Touched {
                                id: id.to_owned(),
                                labels,
                            }),
                        }
                    }
                }
            }
            if touched.len() > MAX_TOUCHED {
                return Err(ProviderError::Gone);
            }
            let now = number(&json["historyId"]).ok_or_else(|| protocol("no history id"))?;
            match json["nextPageToken"].as_str() {
                Some(token) => page = format!("&pageToken={}", percent(token)),
                None => return Ok((touched, now)),
            }
        }
    }

    async fn modify(
        &mut self,
        ids: &[String],
        add: &[&str],
        remove: &[&str],
    ) -> Result<(), ProviderError> {
        for chunk in ids.chunks(MAX_MODIFY) {
            let body = json!({"ids": chunk, "addLabelIds": add, "removeLabelIds": remove});
            match self.post("/messages/batchModify", &body).await {
                // Undone on the device rather than retried forever.
                Err(ProviderError::Rejected(e)) => return Err(ProviderError::Protocol(e)),
                other => other?,
            };
        }
        Ok(())
    }
}

/// A message named in the history, with its labels as of its last change there.
#[derive(Debug, Clone)]
struct Touched {
    id: String,
    labels: Vec<String>,
}

impl MailProvider for GmailSession {
    async fn folders(&mut self) -> Result<Vec<NewFolder>, ProviderError> {
        let folder = |remote_id: &str, name: &str, role| NewFolder {
            remote_id: remote_id.to_owned(),
            name: name.to_owned(),
            role,
        };
        Ok(vec![
            folder("INBOX", "Inbox", FolderRole::Inbox),
            folder("SENT", "Sent", FolderRole::Sent),
            folder("DRAFT", "Drafts", FolderRole::Drafts),
            folder(ARCHIVE, "All Mail", FolderRole::Archive),
            folder("STARRED", "Starred", FolderRole::Flagged),
            folder("SPAM", "Spam", FolderRole::Junk),
            folder("TRASH", "Trash", FolderRole::Trash),
        ])
    }

    async fn changes(
        &mut self,
        folder: &str,
        since: Checkpoint,
        known: &[String],
        limit: u32,
    ) -> Result<Changes, ProviderError> {
        let mut validity = since.validity;
        if since.next != 0 {
            match self.history(since.next).await {
                Ok((touched, now)) => {
                    let mut changes = self.apply(folder, touched, known, limit).await?;
                    changes.checkpoint = Checkpoint {
                        validity,
                        next: now,
                    };
                    return Ok(changes);
                }
                // Too much or too long ago: what is saved is dropped and the folder starts over.
                Err(ProviderError::Gone) => validity = validity.wrapping_add(1),
                Err(e) => return Err(e),
            }
        }
        // Read first, so what arrives during the listing is in the next sync.
        let next = self.history_id().await?;
        Ok(Changes {
            checkpoint: Checkpoint { validity, next },
            new: self.newest(folder, limit).await?,
            flags: vec![],
            gone: vec![],
        })
    }

    async fn body(
        &mut self,
        _folder: &str,
        _validity: u32,
        remote_id: &str,
    ) -> Result<Vec<u8>, ProviderError> {
        let json = self
            .get(&format!(
                "/messages/{}?format=raw&fields=raw",
                percent(remote_id)
            ))
            .await?;
        json["raw"]
            .as_str()
            .and_then(decode_base64)
            .ok_or_else(|| protocol("bad raw message"))
    }

    async fn set_seen(
        &mut self,
        _folder: &str,
        _validity: u32,
        remote_ids: &[String],
        seen: bool,
    ) -> Result<(), ProviderError> {
        let (add, remove) = if seen {
            (None, Some("UNREAD"))
        } else {
            (Some("UNREAD"), None)
        };
        self.modify(remote_ids, add.as_slice(), remove.as_slice())
            .await
    }

    async fn move_to(
        &mut self,
        folder: &str,
        _validity: u32,
        remote_ids: &[String],
        to: &str,
    ) -> Result<(), ProviderError> {
        let from = label(folder).filter(|l| !KEPT.contains(l));
        self.modify(remote_ids, label(to).as_slice(), from.as_slice())
            .await
    }

    async fn append(&mut self, folder: &str, message: &[u8]) -> Result<(), ProviderError> {
        let body = json!({"raw": base64url(message), "labelIds": label(folder).as_slice()});
        self.post("/messages", &body).await.map(|_| ())
    }

    async fn wait(
        &mut self,
        _folder: &str,
        since: Checkpoint,
        wait: Duration,
    ) -> Result<bool, ProviderError> {
        let until = Instant::now() + wait;
        loop {
            // Any change in the mailbox moves its history id on.
            if self.history_id().await? > since.next {
                return Ok(true);
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            tokio::time::sleep(left.min(POLL)).await;
        }
    }
}

impl GmailSession {
    /// Turns the history into changes of `folder`: known messages get their flags or are gone,
    /// new ones in it come with their header.
    async fn apply(
        &mut self,
        folder: &str,
        touched: Vec<Touched>,
        known: &[String],
        limit: u32,
    ) -> Result<Changes, ProviderError> {
        let known: HashSet<&str> = known.iter().map(String::as_str).collect();
        let mut changes = Changes {
            checkpoint: Checkpoint::default(),
            new: vec![],
            flags: vec![],
            gone: vec![],
        };
        let mut added = Vec::new();
        for t in touched {
            if known.contains(t.id.as_str()) {
                if has(&t.labels, folder) {
                    changes.flags.push((t.id, flags(&t.labels)));
                } else {
                    changes.gone.push(t.id);
                }
            } else if has(&t.labels, folder) {
                added.push(t.id);
            }
        }
        for m in self.headers(&added).await? {
            if has(&m.labels, folder) {
                changes.new.push(m.raw);
            }
        }
        changes.new.sort_by_key(|m| m.received);
        let extra = changes.new.len().saturating_sub(limit as usize);
        changes.new.drain(..extra);
        Ok(changes)
    }
}

impl MailSender for GmailSession {
    async fn send(
        &mut self,
        _from: &str,
        _to: &[String],
        message: &[u8],
    ) -> Result<(), ProviderError> {
        // Gmail reads the recipients from the message and keeps a copy in Sent.
        let body = json!({"raw": base64url(message)});
        self.post("/messages/send", &body).await.map(|_| ())
    }
}

/// The label that puts a message in `folder`; none for the archive.
fn label(folder: &str) -> Option<&str> {
    (folder != ARCHIVE).then_some(folder)
}

/// Labels whose mail shows in no other folder.
const APART: [&str; 2] = ["SPAM", "TRASH"];
/// The other system labels, with their search names: mail in none of them is archived.
const FILED: [(&str, &str); 3] = [("INBOX", "inbox"), ("SENT", "sent"), ("DRAFT", "drafts")];
/// Labels a message keeps when it moves: Gmail does not let them be taken off.
const KEPT: [&str; 2] = ["SENT", "DRAFT"];

/// What lists a folder's messages: its label, or for the archive a search, which leaves spam
/// and trash out as every list does unless asked.
fn listing(folder: &str) -> String {
    if folder == ARCHIVE {
        let filed: Vec<_> = FILED.iter().map(|(_, n)| format!("-in:{n}")).collect();
        format!("q={}", percent(&filed.join(" ")))
    } else if APART.contains(&folder) {
        format!("labelIds={}&includeSpamTrash=true", percent(folder))
    } else {
        format!("labelIds={}", percent(folder))
    }
}

fn has(labels: &[String], folder: &str) -> bool {
    let is = |l: &str| labels.iter().any(|x| x == l);
    if APART.contains(&folder) {
        is(folder)
    } else if APART.iter().any(|l| is(l)) {
        false
    } else if folder == ARCHIVE {
        !FILED.iter().any(|(l, _)| is(l))
    } else {
        is(folder)
    }
}

fn flags(labels: &[String]) -> Flags {
    let has = |l: &str| labels.iter().any(|x| x == l);
    Flags {
        seen: !has("UNREAD"),
        flagged: has("STARRED"),
        answered: false,
        draft: has("DRAFT"),
    }
}

/// A metadata message as the provider port returns it, with its labels.
struct Labeled {
    raw: RawMessage,
    labels: Vec<String>,
}

fn raw_message(json: &Value) -> Option<Labeled> {
    let labels = strings(&json["labelIds"]);
    let mut header = Vec::new();
    for h in json["payload"]["headers"].as_array().into_iter().flatten() {
        let (Some(name), Some(value)) = (h["name"].as_str(), h["value"].as_str()) else {
            continue;
        };
        // One header per line, whatever line breaks the value holds.
        let value: String = value
            .chars()
            .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
            .collect();
        header.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    header.extend_from_slice(b"\r\n");
    Some(Labeled {
        raw: RawMessage {
            remote_id: json["id"].as_str()?.to_owned(),
            flags: flags(&labels),
            received: number(&json["internalDate"])
                .map_or(0, |ms| i64::try_from(ms / 1000).unwrap_or(0)),
            header,
        },
        labels,
    })
}

/// The JSON of a response, or the error its status means.
fn answer(response: &Response) -> Result<Value, ProviderError> {
    if !response.ok() {
        return Err(error(response));
    }
    if response.body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&response.body).map_err(|e| protocol(e.to_string()))
}

/// The error a failed response means.
fn error(response: &Response) -> ProviderError {
    let Response { status, body, .. } = response;
    // A front end in the way may answer an error with HTML.
    let json: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let error = &json["error"];
    let reason = error["message"].as_str().unwrap_or_default();
    let limited = each(&error["errors"], |e| &e["reason"])
        .iter()
        .any(|r| r.ends_with("LimitExceeded") || r == "quotaExceeded");
    match status {
        401 => ProviderError::Auth,
        404 => ProviderError::Gone,
        // Over a rate or quota limit: later it goes through.
        403 if limited => ProviderError::Network(format!("403 {reason}")),
        429 | 500.. => ProviderError::Network(format!("{status} {reason}")),
        status => ProviderError::Rejected(format!("{status} {reason}")),
    }
}

/// Google sends 64-bit numbers as strings.
fn number(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| value.as_u64())
}

/// The strings in an array, each picked out by `pick`.
fn each(value: &Value, pick: impl Fn(&Value) -> &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| pick(v).as_str().map(str::to_owned))
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    each(value, |v| v)
}

fn protocol(msg: impl Into<String>) -> ProviderError {
    ProviderError::Protocol(msg.into())
}

#[cfg(test)]
mod tests;
