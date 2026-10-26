//! Against a scripted EWS server on localhost, with the Dovecot test certificate.

use std::sync::Mutex;

use ozero_http::HttpsClient;
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

/// A request as the server saw it: which connection, the Authorization header, the body.
#[derive(Debug, Clone)]
struct Seen {
    conn: usize,
    auth: Option<String>,
    body: String,
}

/// Answers a request: status, extra headers, body.
type Handler = Arc<dyn Fn(&Seen) -> (u16, Vec<String>, String) + Send + Sync>;

async fn serving(handler: Handler) -> (HttpsClient, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let log = seen.clone();
    tokio::spawn(async move {
        for conn in 0.. {
            let (tcp, _) = listener.accept().await.unwrap();
            let (handler, log) = (handler.clone(), log.clone());
            tokio::spawn(async move {
                let mut tls = acceptor().accept(tcp).await.unwrap();
                let mut buf = Vec::new();
                loop {
                    let end = loop {
                        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            break end + 4;
                        }
                        let mut chunk = [0; 4096];
                        let n = tls.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                    };
                    let head = String::from_utf8(buf.drain(..end).collect()).unwrap();
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
                    let auth = head
                        .lines()
                        .find_map(|l| l.strip_prefix("Authorization: "))
                        .map(str::to_owned);
                    let request = Seen { conn, auth, body };
                    let (status, headers, reply) = handler(&request);
                    log.lock().unwrap().push(request);
                    let mut response =
                        format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\n", reply.len());
                    for h in headers {
                        response.push_str(&format!("{h}\r\n"));
                    }
                    response.push_str("\r\n");
                    response.push_str(&reply);
                    tls.write_all(response.as_bytes()).await.unwrap();
                }
            });
        }
    });
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(CA).unwrap())
        .unwrap();
    let tls = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    (HttpsClient::new("localhost", port, Arc::new(tls)), seen)
}

fn acceptor() -> TlsAcceptor {
    let certs = CertificateDer::pem_slice_iter(CERT)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let key = PrivateKeyDer::from_pem_slice(KEY).unwrap();
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    TlsAcceptor::from(Arc::new(config))
}

const BASIC: &str = "Basic YW5uYTpwdw==";

/// A session over Basic whose server answers each operation with `answer`.
async fn session(
    answer: impl Fn(&str) -> String + Send + Sync + 'static,
) -> (EwsSession, Arc<Mutex<Vec<Seen>>>) {
    let handler: Handler = Arc::new(move |r: &Seen| match r.auth.as_deref() {
        Some(BASIC) => (200, vec![], soap(&answer(&r.body))),
        _ => (
            401,
            vec!["WWW-Authenticate: Basic realm=\"x\"".into()],
            String::new(),
        ),
    });
    let (http, seen) = serving(handler).await;
    (
        EwsSession::over(Client::over(http, PATH, "anna", "pw")),
        seen,
    )
}

fn soap(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\">\
<s:Body>{body}</s:Body></s:Envelope>"
    )
}

fn ok(op: &str, inner: &str) -> String {
    format!(
        "<m:{op}ResponseMessage ResponseClass=\"Success\"><m:ResponseCode>NoError</m:ResponseCode>{inner}</m:{op}ResponseMessage>"
    )
}

fn not_found(op: &str) -> String {
    format!(
        "<m:{op}ResponseMessage ResponseClass=\"Error\"><m:MessageText>gone</m:MessageText><m:ResponseCode>ErrorItemNotFound</m:ResponseCode></m:{op}ResponseMessage>"
    )
}

fn response(op: &str, messages: &[String]) -> String {
    format!(
        "<m:{op}Response><m:ResponseMessages>{}</m:ResponseMessages></m:{op}Response>",
        messages.concat()
    )
}

/// A listed message: id, received, read.
fn listed(id: &str, received: &str, read: bool) -> String {
    format!(
        "<t:Message><t:ItemId Id=\"{id}\" ChangeKey=\"c\"/><t:DateTimeReceived>{received}</t:DateTimeReceived>\
<t:IsRead>{read}</t:IsRead><t:IsDraft>false</t:IsDraft></t:Message>"
    )
}

fn found(items: &[String], last: bool) -> String {
    response(
        "FindItem",
        &[ok(
            "FindItem",
            &format!(
                "<m:RootFolder IncludesLastItemInRange=\"{last}\"><t:Items>{}</t:Items></m:RootFolder>",
                items.concat()
            ),
        )],
    )
}

/// A message with the properties a header is made of.
fn full(id: &str, received: &str, subject: &str) -> String {
    format!(
        "<t:Message><t:ItemId Id=\"{id}\"/><t:Subject>{subject}</t:Subject>\
<t:DateTimeReceived>{received}</t:DateTimeReceived><t:DateTimeSent>2026-10-26T07:00:00Z</t:DateTimeSent>\
<t:InternetMessageHeaders><t:InternetMessageHeader HeaderName=\"Received\">from x</t:InternetMessageHeader>\
<t:InternetMessageHeader HeaderName=\"List-Id\">&lt;news.corp.ru&gt;</t:InternetMessageHeader>\
<t:InternetMessageHeader HeaderName=\"Subject\">=?utf-8?B?eA==?=</t:InternetMessageHeader></t:InternetMessageHeaders>\
<t:IsRead>false</t:IsRead><t:From><t:Mailbox><t:Name>Boris \"B\"</t:Name><t:EmailAddress>b@corp.ru</t:EmailAddress>\
<t:RoutingType>SMTP</t:RoutingType></t:Mailbox></t:From><t:InternetMessageId>&lt;{id}@corp.ru&gt;</t:InternetMessageId>\
<t:ExtendedProperty><t:ExtendedFieldURI PropertyTag=\"0x1090\" PropertyType=\"Integer\"/><t:Value>2</t:Value></t:ExtendedProperty>\
</t:Message>"
    )
}

fn got(items: &[String]) -> String {
    let messages: Vec<String> = items
        .iter()
        .map(|i| {
            if i.is_empty() {
                not_found("GetItem")
            } else {
                ok("GetItem", &format!("<m:Items>{i}</m:Items>"))
            }
        })
        .collect();
    response("GetItem", &messages)
}

fn op(body: &str) -> &str {
    let start = body.find("<soap:Body><m:").unwrap() + 14;
    let rest = &body[start..];
    &rest[..rest.find([' ', '>']).unwrap()]
}

#[tokio::test]
async fn lists_the_mail_folders() {
    let (mut ews, seen) = session(|body| match op(body) {
        "GetFolder" => {
            let folders: Vec<String> = ["inbox", "sentitems", "drafts", "deleteditems", "junkemail", "outbox"]
                .iter()
                .map(|n| ok("GetFolder", &format!("<m:Folders><t:Folder><t:FolderId Id=\"id-{n}\"/></t:Folder></m:Folders>")))
                .chain(["syncissues", "conversationhistory"].map(|_| not_found("GetFolder")))
                .collect();
            response("GetFolder", &folders)
        }
        "FindFolder" => {
            let folder = |id: &str, parent: &str, name: &str, class: &str, hidden: bool| {
                format!(
                    "<t:Folder><t:FolderId Id=\"{id}\"/><t:ParentFolderId Id=\"{parent}\"/><t:DisplayName>{name}</t:DisplayName>\
<t:FolderClass>{class}</t:FolderClass><t:ExtendedProperty><t:ExtendedFieldURI PropertyTag=\"0x10f4\" PropertyType=\"Boolean\"/>\
<t:Value>{hidden}</t:Value></t:ExtendedProperty></t:Folder>"
                )
            };
            let all = [
                folder("id-inbox", "root", "Входящие", "IPF.Note", false),
                folder("id-sentitems", "root", "Sent Items", "IPF.Note", false),
                folder("id-deleteditems", "root", "Deleted Items", "IPF.Note", false),
                folder("id-outbox", "root", "Outbox", "IPF.Note", false),
                folder("cal", "root", "Calendar", "IPF.Appointment", false),
                folder("hidden", "root", "Hidden", "IPF.Note", true),
                folder("arch", "root", "Архив", "IPF.Note", false),
                folder("proj", "id-inbox", "Projects", "IPF.Note", false),
            ];
            response(
                "FindFolder",
                &[ok("FindFolder", &format!("<m:RootFolder IncludesLastItemInRange=\"true\"><t:Folders>{}</t:Folders></m:RootFolder>", all.concat()))],
            )
        }
        other => panic!("{other}"),
    })
    .await;
    let folders = ews.folders().await.unwrap();
    let got: Vec<_> = folders
        .iter()
        .map(|f| (f.remote_id.as_str(), f.name.as_str(), f.role))
        .collect();
    assert_eq!(
        got,
        [
            ("id-inbox", "Входящие", FolderRole::Inbox),
            ("id-sentitems", "Sent Items", FolderRole::Sent),
            ("id-deleteditems", "Deleted Items", FolderRole::Trash),
            ("arch", "Архив", FolderRole::Archive),
            ("proj", "Входящие/Projects", FolderRole::Custom),
        ]
    );
    // Basic from the start, without asking first.
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|s| s.auth.as_deref() == Some(BASIC)));
}

#[tokio::test]
async fn brings_new_mail_flags_and_removals() {
    let (mut ews, seen) = session(|body| match op(body) {
        "GetItem" => got(&[full("new", "2026-10-26T07:30:00Z", "Hi &amp; bye")]),
        "FindItem" => {
            assert!(body.contains("IsGreaterThanOrEqualTo"), "{body}");
            assert!(body.contains("Value=\"2026-10-25T10:00:00Z\""), "{body}");
            found(
                &[
                    listed("new", "2026-10-26T07:30:00Z", false),
                    listed("old", "2026-10-25T10:00:00Z", false),
                ],
                true,
            )
        }
        other => panic!("{other}"),
    })
    .await;
    let since = Checkpoint {
        validity: 0,
        next: 5,
    };
    let known = ["moved".to_owned(), "old".to_owned()];
    let changes = ews
        .changes("id-inbox", since, &known, 1_792_922_400, 50)
        .await
        .unwrap();
    assert_eq!(changes.gone, ["moved"]);
    assert_eq!(changes.flags.len(), 1);
    assert_eq!(changes.flags[0].0, "old");
    assert!(!changes.flags[0].1.seen);
    let [new] = changes.new.as_slice() else {
        panic!("{changes:?}")
    };
    assert_eq!(new.remote_id, "new");
    assert_eq!(new.received, 1_792_999_800);
    assert!(new.flags.flagged && !new.flags.seen);
    let header = String::from_utf8(new.header.clone()).unwrap();
    assert_eq!(
        header,
        "Subject: Hi & bye\r\nFrom: \"Boris B\" <b@corp.ru>\r\nDate: Mon, 26 Oct 2026 07:00:00 +0000\r\n\
Message-ID: <new@corp.ru>\r\nList-Id: <news.corp.ru>\r\n\r\n"
    );
    assert_eq!(changes.checkpoint.next, 1_792_999_801);
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn starts_with_the_newest_mail() {
    let (mut ews, _) = session(|body| match op(body) {
        "FindItem" => {
            assert!(body.contains("MaxEntriesReturned=\"2\""), "{body}");
            assert!(!body.contains("Restriction"), "{body}");
            found(
                &[
                    listed("b", "2026-10-26T07:30:00Z", true),
                    listed("a", "2026-10-26T07:00:00Z", true),
                ],
                false,
            )
        }
        "GetItem" => got(&[
            full("b", "2026-10-26T07:30:00Z", "B"),
            full("a", "2026-10-26T07:00:00Z", "A"),
        ]),
        other => panic!("{other}"),
    })
    .await;
    let changes = ews
        .changes("id-inbox", Checkpoint::default(), &[], i64::MAX, 2)
        .await
        .unwrap();
    let ids: Vec<_> = changes.new.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert!(changes.gone.is_empty() && changes.flags.is_empty());
}

#[tokio::test]
async fn reads_a_message_whole() {
    let (mut ews, _) = session(|body| {
        assert!(body.contains("<t:IncludeMimeContent>true</t:IncludeMimeContent>"));
        got(&["<t:Message><t:ItemId Id=\"a\"/><t:MimeContent CharacterSet=\"UTF-8\">U3ViamVjdDogeA0K\r\nDQpoaQ==</t:MimeContent></t:Message>".into()])
    })
    .await;
    assert_eq!(
        ews.body("f", 0, "a").await.unwrap(),
        b"Subject: x\r\n\r\nhi"
    );
}

#[tokio::test]
async fn moves_and_says_where_mail_went() {
    let (mut ews, seen) = session(|body| {
        assert_eq!(op(body), "MoveItem");
        response(
            "MoveItem",
            &[
                ok(
                    "MoveItem",
                    "<m:Items><t:Message><t:ItemId Id=\"a2\"/></t:Message></m:Items>",
                ),
                not_found("MoveItem"),
            ],
        )
    })
    .await;
    let moved = ews
        .move_to("inbox", 0, &["a".into(), "b".into()], "id-arch")
        .await
        .unwrap();
    assert_eq!(moved.ids, [("a".to_owned(), "a2".to_owned())]);
    assert_eq!(moved.validity, Some(0));
    let body = seen.lock().unwrap().last().unwrap().body.clone();
    assert!(body.contains("<m:ToFolderId><t:FolderId Id=\"id-arch\"/></m:ToFolderId>"));
}

#[tokio::test]
async fn sends_with_a_copy_in_sent() {
    let (mut ews, seen) = session(|_| response("CreateItem", &[ok("CreateItem", "")])).await;
    ews.send("a@corp.ru", &[], b"To: b@corp.ru\r\n\r\nhi")
        .await
        .unwrap();
    let body = seen.lock().unwrap().last().unwrap().body.clone();
    assert!(body.contains("MessageDisposition=\"SendAndSaveCopy\""));
    assert!(body.contains("<t:DistinguishedFolderId Id=\"sentitems\"/>"));
    assert!(body.contains(&format!(
        "<t:MimeContent>{}</t:MimeContent>",
        base64(b"To: b@corp.ru\r\n\r\nhi")
    )));
}

#[tokio::test]
async fn marks_read_without_receipts() {
    let (mut ews, seen) = session(|_| {
        response(
            "UpdateItem",
            &[ok("UpdateItem", ""), not_found("UpdateItem")],
        )
    })
    .await;
    ews.set_seen("f", 0, &["a\"".into(), "b".into()], true)
        .await
        .unwrap();
    let body = seen.lock().unwrap().last().unwrap().body.clone();
    assert!(body.contains("SuppressReadReceipts=\"true\""));
    assert!(body.contains("<t:ItemId Id=\"a&quot;\"/>"));
    assert!(body.contains("<t:IsRead>true</t:IsRead>"));
}

#[tokio::test]
async fn tells_errors_apart() {
    let (mut ews, _) = session(|_| "<x>".to_owned()).await;
    assert!(matches!(ews.check().await, Err(ProviderError::Protocol(_))));
    let busy: Handler = Arc::new(|r: &Seen| match r.auth {
        Some(_) => (
            500,
            vec![],
            soap(
                "<s:Fault><faultcode>s:Client</faultcode><faultstring>busy</faultstring>\
<detail><e:ResponseCode xmlns:e=\"x\">ErrorServerBusy</e:ResponseCode></detail></s:Fault>",
            ),
        ),
        None => (401, vec!["WWW-Authenticate: Basic".into()], String::new()),
    });
    let (http, _) = serving(busy).await;
    let mut ews = EwsSession::over(Client::over(http, PATH, "anna", "pw"));
    assert_eq!(
        ews.check().await,
        Err(ProviderError::Network("ErrorServerBusy".into()))
    );
    let refused: Handler =
        Arc::new(|_| (401, vec!["WWW-Authenticate: Basic".into()], String::new()));
    let (http, _) = serving(refused).await;
    let mut ews = EwsSession::over(Client::over(http, PATH, "anna", "pw"));
    assert_eq!(ews.check().await, Err(ProviderError::Auth));
    let modern: Handler =
        Arc::new(|_| (401, vec!["WWW-Authenticate: Bearer".into()], String::new()));
    let (http, _) = serving(modern).await;
    let mut ews = EwsSession::over(Client::over(http, PATH, "anna", "pw"));
    assert!(matches!(ews.check().await, Err(ProviderError::Rejected(_))));
}

/// A challenge with no target info.
fn challenge() -> String {
    let mut message = b"NTLMSSP\0\x02\0\0\0".to_vec();
    message.extend_from_slice(&[0; 8]);
    message.extend_from_slice(&0x0008_8201u32.to_le_bytes());
    message.extend_from_slice(&[7; 8]);
    message.extend_from_slice(&[0; 16]);
    base64(&message)
}

#[tokio::test]
async fn signs_in_with_ntlm_once_a_connection() {
    // Which connections finished NTLM.
    let signed: Arc<Mutex<HashSet<usize>>> = Arc::default();
    let handler: Handler = Arc::new(move |r: &Seen| {
        let mut signed = signed.lock().unwrap();
        let token = r.auth.as_deref().and_then(|a| a.strip_prefix("Negotiate "));
        match token.and_then(decode_base64) {
            Some(m) if m[8] == 1 => (
                401,
                vec![format!("WWW-Authenticate: Negotiate {}", challenge())],
                String::new(),
            ),
            Some(m) if m[8] == 3 && r.body.contains("GetFolder") => {
                signed.insert(r.conn);
                (200, vec![], soap(&inbox()))
            }
            None if signed.contains(&r.conn) => (200, vec![], soap(&inbox())),
            _ => (
                401,
                vec!["WWW-Authenticate: Negotiate".into()],
                String::new(),
            ),
        }
    });
    let (http, seen) = serving(handler).await;
    let mut ews = EwsSession::over(Client::over(http, PATH, "CORP\\anna", "pw"));
    ews.check().await.unwrap();
    ews.check().await.unwrap();
    let seen = seen.lock().unwrap();
    let kinds: Vec<_> = seen
        .iter()
        .map(|s| {
            s.auth.as_deref().map(|a| {
                a.strip_prefix("Negotiate ")
                    .map(|t| decode_base64(t).unwrap()[8])
            })
        })
        .collect();
    // Basic refused, then hello, then the answer with the real request; then signed in already.
    assert_eq!(kinds, [Some(None), Some(Some(1)), Some(Some(3)), None]);
    assert!(seen[1].body.is_empty());
    assert!(seen.iter().all(|s| s.conn == 0));
}

#[tokio::test]
async fn asks_unsigned_before_sending_the_password_to_a_new_server() {
    let handler: Handler = Arc::new(|r: &Seen| match r.auth.as_deref() {
        Some(BASIC) => (200, vec![], soap(&inbox())),
        _ => (
            401,
            vec!["WWW-Authenticate: Basic realm=\"x\"".into()],
            String::new(),
        ),
    });
    let (http, seen) = serving(handler).await;
    let mut ews = EwsSession::over(Client::over(http, PATH, "anna", "pw").probing());
    ews.check().await.unwrap();
    ews.check().await.unwrap();
    let auth: Vec<_> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.auth.clone())
        .collect();
    assert_eq!(auth, [None, Some(BASIC.into()), Some(BASIC.into())]);
}

#[tokio::test]
async fn a_refused_password_is_tried_once() {
    let handler: Handler = Arc::new(|_: &Seen| {
        (
            401,
            vec!["WWW-Authenticate: Basic realm=\"x\"".into()],
            String::new(),
        )
    });
    let (http, seen) = serving(handler).await;
    let mut ews = EwsSession::over(Client::over(http, PATH, "anna", "pw").probing());
    assert!(matches!(ews.check().await, Err(ProviderError::Auth)));
    let signed = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.auth.is_some())
        .count();
    assert_eq!(signed, 1);
}

fn inbox() -> String {
    response(
        "GetFolder",
        &[ok(
            "GetFolder",
            "<m:Folders><t:Folder><t:FolderId Id=\"i\"/></t:Folder></m:Folders>",
        )],
    )
}

#[test]
fn takes_unresolved_addresses_from_the_header() {
    let item = xml::parse(
        b"<Message><ItemId Id=\"a\"/><Subject></Subject>\
<InternetMessageHeaders><InternetMessageHeader HeaderName=\"From\">Anna &lt;a@corp.ru&gt;</InternetMessageHeader>\
<InternetMessageHeader HeaderName=\"Subject\">Re: x</InternetMessageHeader>\
<InternetMessageHeader HeaderName=\"X-MS-Exchange-Organization-Network-Message-Id\">1</InternetMessageHeader>\
<InternetMessageHeader HeaderName=\"Bad: name\">1</InternetMessageHeader></InternetMessageHeaders>\
<From><Mailbox><Name>Anna</Name><EmailAddress>/O=CORP/OU=EXCHANGE/CN=ANNA</EmailAddress><RoutingType>EX</RoutingType></Mailbox></From>\
<ToRecipients><Mailbox><EmailAddress>b@corp.ru</EmailAddress></Mailbox><Mailbox><Name>C</Name><EmailAddress>c@corp.ru</EmailAddress></Mailbox></ToRecipients>\
</Message>",
    )
    .unwrap();
    let raw = raw_message(&item).unwrap();
    assert_eq!(
        String::from_utf8(raw.header).unwrap(),
        "To: <b@corp.ru>, \"C\" <c@corp.ru>\r\nFrom: Anna <a@corp.ru>\r\nSubject: Re: x\r\n\r\n"
    );
    assert!(raw.flags.seen);
}
