//! Against a scripted Gmail API on localhost, with the Dovecot test certificate.

use std::sync::Mutex;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::{RootCertStore, ServerConfig};

use super::*;

const CA: &[u8] = include_bytes!("../../imap/tests/dovecot/ca.crt");
const CERT: &[u8] = include_bytes!("../../imap/tests/dovecot/tls.crt");
const KEY: &[u8] = include_bytes!("../../imap/tests/dovecot/tls.key");

/// Answers by the start of the request path after `BASE`: (prefix, status, JSON body).
type Routes = Vec<(&'static str, u16, Value)>;

/// What the client asked: method, path after `BASE`, body.
type Seen = Arc<Mutex<Vec<(String, String, String)>>>;

async fn session(routes: Routes) -> (GmailSession, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Seen = Arc::default();
    tokio::spawn(serve(listener, routes, seen.clone()));
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(CA).unwrap())
        .unwrap();
    let tls = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let http = HttpsClient::new("localhost", port, Arc::new(tls));
    (GmailSession::over(http, "token"), seen)
}

async fn serve(listener: TcpListener, routes: Routes, seen: Seen) {
    let certs = CertificateDer::pem_slice_iter(CERT)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let key = PrivateKeyDer::from_pem_slice(KEY).unwrap();
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let (tcp, _) = listener.accept().await.unwrap();
    let mut tls = acceptor.accept(tcp).await.unwrap();
    let mut buf = Vec::new();
    loop {
        // One request: head, then a body as long as Content-Length says.
        let end = loop {
            if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break end + 4;
            }
            let mut chunk = [0; 4096];
            let n = tls.read(&mut chunk).await.unwrap();
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8(buf.drain(..end).collect()).unwrap();
        assert!(
            head.contains("\r\nAuthorization: Bearer token\r\n"),
            "{head}"
        );
        let length = head
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .map_or(0, |n| n.parse().unwrap());
        while buf.len() < length {
            let mut chunk = [0; 4096];
            let n = tls.read(&mut chunk).await.unwrap();
            buf.extend_from_slice(&chunk[..n]);
        }
        let body = String::from_utf8(buf.drain(..length).collect()).unwrap();
        let mut words = head.split(' ');
        let method = words.next().unwrap().to_owned();
        let path = words.next().unwrap().to_owned();
        let reply = if path == "/batch/gmail/v1" {
            // One round trip; each part answered from the routes, last part first.
            seen.lock().unwrap().push((method, path, String::new()));
            let boundary = head
                .lines()
                .find_map(|l| l.split_once("boundary="))
                .unwrap()
                .1;
            let mut parts = String::new();
            for (i, part) in body.split(&format!("--{boundary}")).enumerate().skip(1) {
                let Some(get) = part.lines().find_map(|l| l.strip_prefix("GET ")) else {
                    continue;
                };
                let get = get.strip_prefix(BASE).unwrap().to_owned();
                let (status, json) = route(&routes, &get);
                seen.lock()
                    .unwrap()
                    .push(("GET".into(), get, String::new()));
                parts.insert_str(
                    0,
                    &format!(
                        "--b_1\r\nContent-Type: application/http\r\nContent-ID: <response-{}>\r\n\r\nHTTP/1.1 {status} X\r\n\r\n{json}\r\n",
                        i - 1
                    ),
                );
            }
            parts.push_str("--b_1--\r\n");
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: multipart/mixed; boundary=b_1\r\nContent-Length: {}\r\n\r\n{parts}",
                parts.len()
            )
        } else {
            let path = path.strip_prefix(BASE).unwrap().to_owned();
            let (status, json) = route(&routes, &path);
            seen.lock().unwrap().push((method, path, body));
            let json = if status == 204 {
                String::new()
            } else {
                json.to_string()
            };
            format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\n\r\n{json}",
                json.len()
            )
        };
        tls.write_all(reply.as_bytes()).await.unwrap();
    }
}

fn route(routes: &Routes, path: &str) -> (u16, Value) {
    routes
        .iter()
        .find(|(prefix, ..)| path.starts_with(prefix))
        .map_or(
            (404, json!({"error": {"message": "Not Found"}})),
            |(_, s, j)| (*s, j.clone()),
        )
}

fn message(id: &str, labels: &[&str], date_ms: u64, subject: &str) -> Value {
    json!({
        "id": id,
        "labelIds": labels,
        "internalDate": date_ms.to_string(),
        "payload": {"headers": [
            {"name": "Subject", "value": subject},
            {"name": "From", "value": "A <a@example.com>"},
        ]},
    })
}

#[tokio::test]
async fn starts_with_the_newest_messages() {
    let (mut gmail, seen) = session(vec![
        (
            "/profile",
            200,
            json!({"emailAddress": "me@gmail.com", "historyId": "100"}),
        ),
        (
            "/messages?labelIds=INBOX",
            200,
            json!({"messages": [{"id": "b"}, {"id": "a"}, {"id": "gone"}]}),
        ),
        (
            "/messages/a?",
            200,
            message("a", &["INBOX"], 1_000_000, "First"),
        ),
        (
            "/messages/b?",
            200,
            message("b", &["INBOX", "UNREAD", "STARRED"], 2_000_000, "Second"),
        ),
    ])
    .await;
    let changes = gmail
        .changes("INBOX", Checkpoint::default(), &[], 10)
        .await
        .unwrap();
    assert_eq!(
        changes.checkpoint,
        Checkpoint {
            validity: 0,
            next: 100
        }
    );
    let ids: Vec<_> = changes.new.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    let b = &changes.new[1];
    assert_eq!(
        (b.received, b.flags.seen, b.flags.flagged),
        (2_000, false, true)
    );
    assert_eq!(
        b.header,
        b"Subject: Second\r\nFrom: A <a@example.com>\r\n\r\n"
    );
    assert!(seen.lock().unwrap()[1].1.contains("maxResults=10"));
}

#[tokio::test]
async fn loads_mail_older_than_the_saved_one() {
    let (mut gmail, seen) = session(vec![
        (
            "/messages?labelIds=INBOX&q=before%3A1001",
            200,
            json!({"messages": [{"id": "saved"}, {"id": "old"}], "nextPageToken": "x"}),
        ),
        (
            "/messages/old?",
            200,
            message("old", &["INBOX"], 900_000, "Old"),
        ),
    ])
    .await;
    let older = gmail
        .older("INBOX", 0, &["saved".into()], 1_000, 10)
        .await
        .unwrap();
    let ids: Vec<_> = older.new.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(ids, ["old"]);
    assert!(older.more);
    // What is saved costs no request.
    let paths: Vec<_> = seen.lock().unwrap().iter().map(|s| s.1.clone()).collect();
    assert_eq!(paths.len(), 3, "{paths:?}");
    assert!(paths[0].contains("maxResults=10"));
}

#[tokio::test]
async fn searches_a_folder_for_plain_words() {
    let (mut gmail, seen) = session(vec![
        (
            "/messages?q=-in%3Ainbox%20-in%3Asent%20-in%3Adrafts%20%22from%3Aanna%22%20%22invoice%22&",
            200,
            json!({"messages": [{"id": "a"}]}),
        ),
        ("/messages/a?", 200, message("a", &[], 1_000, "Invoice")),
    ])
    .await;
    let found = gmail
        .search(ARCHIVE, 0, " from:anna  invoice", 5)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert!(seen.lock().unwrap()[0].1.contains("maxResults=5"));
}

#[tokio::test]
async fn follows_the_history() {
    let history = json!({
        "historyId": "120",
        "history": [
            {"messagesAdded": [{"message": {"id": "new", "labelIds": ["INBOX", "UNREAD"]}}]},
            {"messagesAdded": [{"message": {"id": "sent", "labelIds": ["SENT"]}}]},
            {"labelsRemoved": [{"message": {"id": "archived", "labelIds": []}, "labelIds": ["INBOX"]}]},
            {"labelsAdded": [{"message": {"id": "read", "labelIds": ["INBOX", "STARRED"]}, "labelIds": ["STARRED"]}]},
            {"messagesDeleted": [{"message": {"id": "deleted", "labelIds": ["INBOX"]}}]},
        ],
    });
    let (mut gmail, seen) = session(vec![
        ("/history?startHistoryId=100", 200, history),
        (
            "/messages/new?",
            200,
            message("new", &["INBOX", "UNREAD"], 5_000, "New"),
        ),
    ])
    .await;
    let known = ["archived", "read", "deleted"].map(String::from);
    let since = Checkpoint {
        validity: 3,
        next: 100,
    };
    let changes = gmail.changes("INBOX", since, &known, 10).await.unwrap();
    assert_eq!(
        changes.checkpoint,
        Checkpoint {
            validity: 3,
            next: 120
        }
    );
    assert_eq!(changes.new.len(), 1);
    assert_eq!(changes.new[0].remote_id, "new");
    let read = Flags {
        seen: true,
        flagged: true,
        ..Flags::default()
    };
    assert_eq!(changes.flags, [("read".to_owned(), read)]);
    assert_eq!(changes.gone, ["archived", "deleted"]);
    // Known mail and mail only in Sent cost no request, and Sent reads the same history.
    let sent = gmail.changes("SENT", since, &[], 10).await.unwrap();
    assert_eq!(sent.checkpoint.next, 120);
    let paths: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|(m, ..)| m == "GET")
        .map(|(_, p, _)| p.clone())
        .collect();
    assert_eq!(paths.len(), 3, "{paths:?}");
    assert!(paths[0].starts_with("/history") && paths[1].starts_with("/messages/new"));
    assert!(paths[2].starts_with("/messages/sent"));
}

#[tokio::test]
async fn starts_over_when_the_history_is_gone() {
    let (mut gmail, _) = session(vec![
        ("/history", 404, json!({"error": {"message": "Not Found"}})),
        ("/profile", 200, json!({"historyId": "500"})),
        ("/messages?", 200, json!({})),
    ])
    .await;
    let since = Checkpoint {
        validity: 7,
        next: 100,
    };
    let changes = gmail.changes("INBOX", since, &[], 10).await.unwrap();
    assert_eq!(
        changes.checkpoint,
        Checkpoint {
            validity: 8,
            next: 500
        }
    );
}

#[tokio::test]
async fn the_archive_is_mail_in_no_other_system_folder() {
    let history = json!({
        "historyId": "120",
        "history": [
            {"messagesAdded": [{"message": {"id": "in", "labelIds": ["INBOX"]}}]},
            {"messagesAdded": [{"message": {"id": "mine", "labelIds": ["SENT"]}}]},
            {"labelsAdded": [{"message": {"id": "binned", "labelIds": ["TRASH"]}, "labelIds": ["TRASH"]}]},
            {"labelsRemoved": [{"message": {"id": "done", "labelIds": []}, "labelIds": ["INBOX"]}]},
        ],
    });
    let (mut gmail, seen) = session(vec![
        ("/profile", 200, json!({"historyId": "100"})),
        (
            "/messages?q=-in%3Ainbox%20-in%3Asent%20-in%3Adrafts&",
            200,
            json!({}),
        ),
        ("/history?startHistoryId=100", 200, history),
        ("/messages/done?", 200, message("done", &[], 5_000, "Done")),
    ])
    .await;
    let first = gmail
        .changes(ARCHIVE, Checkpoint::default(), &[], 10)
        .await
        .unwrap();
    assert!(first.new.is_empty());
    let known = ["binned".to_owned()];
    let changes = gmail
        .changes(ARCHIVE, first.checkpoint, &known, 10)
        .await
        .unwrap();
    assert_eq!(changes.new[0].remote_id, "done");
    assert_eq!(changes.gone, ["binned"]);
    let paths: Vec<String> = seen.lock().unwrap().iter().map(|s| s.1.clone()).collect();
    assert!(
        !paths
            .iter()
            .any(|p| p.starts_with("/messages/in?") || p.starts_with("/messages/mine?"))
    );
}

#[tokio::test]
async fn archives_by_taking_the_inbox_label_off() {
    let (mut gmail, seen) = session(vec![("/messages/batchModify", 204, Value::Null)]).await;
    let ids = ["a".to_owned(), "b".to_owned()];
    gmail.move_to("INBOX", 0, &ids, ARCHIVE).await.unwrap();
    gmail.set_seen("INBOX", 0, &ids, true).await.unwrap();
    // Into the trash a message keeps its labels.
    gmail.move_to("SENT", 0, &ids, "TRASH").await.unwrap();
    let seen = seen.lock().unwrap();
    let bodies: Vec<Value> = seen
        .iter()
        .map(|(_, _, b)| serde_json::from_str(b).unwrap())
        .collect();
    assert_eq!(
        bodies[0],
        json!({"ids": ["a", "b"], "addLabelIds": [], "removeLabelIds": ["INBOX"]})
    );
    assert_eq!(
        bodies[1],
        json!({"ids": ["a", "b"], "addLabelIds": [], "removeLabelIds": ["UNREAD"]})
    );
    assert_eq!(
        bodies[2],
        json!({"ids": ["a", "b"], "addLabelIds": ["TRASH"], "removeLabelIds": []})
    );
}

#[tokio::test]
async fn deletes_for_good_only_what_is_still_in_the_folder() {
    let (mut gmail, seen) = session(vec![
        (
            "/messages?labelIds=TRASH&includeSpamTrash=true",
            200,
            json!({"messages": [{"id": "a"}, {"id": "c"}]}),
        ),
        ("/messages/batchDelete", 204, Value::Null),
    ])
    .await;
    // "b" was taken out of the trash elsewhere since the last sync.
    gmail
        .delete("TRASH", 0, &["a".to_owned(), "b".to_owned()])
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    let body: Value = serde_json::from_str(&seen[1].2).unwrap();
    assert_eq!(body, json!({"ids": ["a"]}));
}

#[tokio::test]
async fn empties_everything_but_what_to_keep() {
    let (mut gmail, seen) = session(vec![
        (
            "/messages?labelIds=SPAM&includeSpamTrash=true",
            200,
            json!({"messages": [{"id": "a"}, {"id": "b"}, {"id": "c"}]}),
        ),
        ("/messages/batchDelete", 204, Value::Null),
    ])
    .await;
    gmail.empty("SPAM", 0, &["b".to_owned()]).await.unwrap();
    let seen = seen.lock().unwrap();
    let body: Value = serde_json::from_str(&seen[1].2).unwrap();
    assert_eq!(body, json!({"ids": ["a", "c"]}));
}

#[tokio::test]
async fn sends_and_reads_raw_messages() {
    let raw = b"Subject: Hi\r\n\r\n\xff\xfe body";
    let (mut gmail, seen) = session(vec![
        ("/messages/send", 200, json!({"id": "x"})),
        (
            "/messages/x?format=raw",
            200,
            json!({"raw": base64url(raw)}),
        ),
    ])
    .await;
    gmail
        .send("me@gmail.com", &["you@example.com".into()], raw)
        .await
        .unwrap();
    assert_eq!(gmail.body("INBOX", 0, "x").await.unwrap(), raw);
    let sent: Value = serde_json::from_str(&seen.lock().unwrap()[0].2).unwrap();
    assert_eq!(sent, json!({"raw": base64url(raw)}));
}

#[tokio::test]
async fn maps_errors() {
    let (mut gmail, _) = session(vec![
        (
            "/messages/send",
            400,
            json!({"error": {"message": "Invalid To header"}}),
        ),
        (
            "/messages/batchModify",
            400,
            json!({"error": {"message": "Bad"}}),
        ),
        ("/profile", 401, json!({})),
        ("/messages/busy", 503, json!("<html>Unavailable</html>")),
        (
            "/messages/limited",
            403,
            json!({"error": {"message": "Slow down", "errors": [{"reason": "userRateLimitExceeded"}]}}),
        ),
    ])
    .await;
    assert_eq!(
        gmail.send("me", &[], b"x").await,
        Err(ProviderError::Rejected("400 Invalid To header".into()))
    );
    // A refused action is undone on the device, not stuck in the queue.
    assert!(matches!(
        gmail.set_seen("INBOX", 0, &["a".into()], true).await,
        Err(ProviderError::Protocol(_))
    ));
    assert_eq!(gmail.email().await, Err(ProviderError::Auth));
    assert!(matches!(
        gmail.body("INBOX", 0, "busy").await,
        Err(ProviderError::Network(_))
    ));
    // Over a rate limit: tried again later, not refused for good.
    assert!(matches!(
        gmail.body("INBOX", 0, "limited").await,
        Err(ProviderError::Network(_))
    ));
    assert_eq!(
        gmail.body("INBOX", 0, "nothere").await,
        Err(ProviderError::Gone)
    );
}

#[tokio::test]
async fn reads_headers_in_batches() {
    let (mut gmail, seen) = session(vec![
        ("/messages/a?", 200, message("a", &["INBOX"], 1_000, "A")),
        (
            "/messages/busy?",
            429,
            json!({"error": {"message": "Slow down"}}),
        ),
    ])
    .await;
    let mut ids: Vec<String> = (0..MAX_BATCH).map(|_| "a".into()).collect();
    ids.push("gone".into());
    assert_eq!(gmail.headers(&ids).await.unwrap().len(), MAX_BATCH);
    let batches = |seen: &Seen| {
        seen.lock()
            .unwrap()
            .iter()
            .filter(|(m, ..)| m == "POST")
            .count()
    };
    assert_eq!(batches(&seen), 2);
    // Asked again once on its own; still over the limit, the sync tries again later.
    assert!(matches!(
        gmail.headers(&["a".into(), "busy".into()]).await,
        Err(ProviderError::Network(_))
    ));
    let busy = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, p, _)| p.starts_with("/messages/busy"))
        .count();
    assert_eq!(busy, 2);
}

#[tokio::test]
async fn waits_until_the_history_moves_on() {
    let routes = vec![("/profile", 200, json!({"historyId": "12"}))];
    let since = |next| Checkpoint { validity: 0, next };
    let wait = Duration::from_millis(50);

    let (mut gmail, _) = session(routes.clone()).await;
    assert!(gmail.wait("INBOX", since(10), wait).await.unwrap());

    let (mut gmail, seen) = session(routes).await;
    assert!(!gmail.wait("INBOX", since(12), wait).await.unwrap());
    assert!(seen.lock().unwrap().len() >= 2);
}
