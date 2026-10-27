//! Against a scripted ActiveSync server on localhost.

use std::sync::Mutex;

use super::*;
use crate::tests::{RawHandler, Seen, serving_raw};

#[derive(Default)]
struct Kept(Mutex<HashMap<String, String>>);

impl ProviderState for Kept {
    fn get(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap().get(key).cloned()
    }

    fn set(&self, key: &str, value: Option<&str>) {
        let mut kept = self.0.lock().unwrap();
        match value {
            Some(v) => kept.insert(key.to_owned(), v.to_owned()),
            None => kept.remove(key),
        };
    }
}

fn n(name: &'static str, children: Vec<Node>) -> Node {
    Node::new(name, children)
}

fn t(name: &'static str, text: &str) -> Node {
    Node::text(name, text)
}

/// A request the server took: its command and body.
type Took = Arc<Mutex<Vec<(String, Node)>>>;

/// A session whose server answers each command with `script`: an HTTP status and a body.
async fn server(
    script: impl Fn(&str, &Node, &Seen) -> (u16, Option<Node>) + Send + Sync + 'static,
) -> (EasSession, Arc<Kept>, Took, Arc<Mutex<Vec<Seen>>>) {
    let took: Took = Arc::default();
    let log = took.clone();
    let handler: RawHandler = Arc::new(move |r: &Seen| {
        let cmd = r
            .line
            .split("Cmd=")
            .nth(1)
            .and_then(|c| c.split('&').next())
            .unwrap_or_default();
        let request = decode(&r.raw).unwrap_or_default();
        let (status, answer) = script(cmd, &request, r);
        log.lock().unwrap().push((cmd.to_owned(), request));
        (status, vec![], answer.map_or_else(Vec::new, |a| encode(&a)))
    });
    let (http, seen) = serving_raw(handler).await;
    let state = Arc::new(Kept::default());
    EasSession::register(&*state).unwrap();
    let client = Client::over(http, ACTIVE_SYNC, "corp\\anna", "pw");
    let session = EasSession::over(client, "corp\\anna", state.clone()).unwrap();
    (session, state, took, seen)
}

fn folder(id: &str, parent: &str, name: &str, kind: &str) -> Node {
    n(
        "FolderHierarchy:Add",
        vec![
            t("FolderHierarchy:ServerId", id),
            t("FolderHierarchy:ParentId", parent),
            t("FolderHierarchy:DisplayName", name),
            t("FolderHierarchy:Type", kind),
        ],
    )
}

fn sync_key(request: &Node) -> &str {
    request.text_at(&[
        "AirSync:Collections",
        "AirSync:Collection",
        "AirSync:SyncKey",
    ])
}

/// A Sync answer for folder 1.
fn collection(key: &str, status: &str, commands: Vec<Node>) -> Option<Node> {
    let mut c = vec![
        t("AirSync:SyncKey", key),
        t("AirSync:CollectionId", "1"),
        t("AirSync:Status", status),
    ];
    if !commands.is_empty() {
        c.push(n("AirSync:Commands", commands));
    }
    Some(n(
        "AirSync:Sync",
        vec![n("AirSync:Collections", vec![n("AirSync:Collection", c)])],
    ))
}

fn item(command: &'static str, id: &str, data: Vec<Node>) -> Node {
    n(
        command,
        vec![
            t("AirSync:ServerId", id),
            n("AirSync:ApplicationData", data),
        ],
    )
}

fn mime(data: &str) -> Node {
    n(
        "AirSyncBase:Body",
        vec![t("AirSyncBase:Type", "4"), t("AirSyncBase:Data", data)],
    )
}

#[tokio::test]
async fn takes_the_policy_then_lists_mail_folders() {
    let (mut eas, state, took, seen) = server(|cmd, request, r| match cmd {
        "FolderSync" if !r.head.contains("X-MS-PolicyKey: P2") => (449, None),
        "FolderSync" if request.text_at(&["FolderHierarchy:SyncKey"]) == "F1" => (
            200,
            Some(n(
                "FolderHierarchy:FolderSync",
                vec![
                    t("FolderHierarchy:Status", "1"),
                    t("FolderHierarchy:SyncKey", "F2"),
                    n(
                        "FolderHierarchy:Changes",
                        vec![n(
                            "FolderHierarchy:Delete",
                            vec![t("FolderHierarchy:ServerId", "6")],
                        )],
                    ),
                ],
            )),
        ),
        "FolderSync" => (
            200,
            Some(n(
                "FolderHierarchy:FolderSync",
                vec![
                    t("FolderHierarchy:Status", "1"),
                    t("FolderHierarchy:SyncKey", "F1"),
                    n(
                        "FolderHierarchy:Changes",
                        vec![
                            folder("1", "0", "Inbox", "2"),
                            folder("2", "0", "Sent Items", "5"),
                            folder("3", "0", "Calendar", "8"),
                            folder("4", "1", "Projects", "12"),
                            folder("5", "0", "Junk E-mail", "12"),
                            folder("6", "0", "Archive", "12"),
                        ],
                    ),
                ],
            )),
        ),
        "Provision" => {
            let acked = request.text_at(&[
                "Provision:Policies",
                "Provision:Policy",
                "Provision:PolicyKey",
            ]) == "P1";
            let key = if acked { "P2" } else { "P1" };
            (
                200,
                Some(n(
                    "Provision:Provision",
                    vec![
                        t("Provision:Status", "1"),
                        n(
                            "Provision:Policies",
                            vec![n(
                                "Provision:Policy",
                                vec![
                                    t("Provision:PolicyType", POLICY_TYPE),
                                    t("Provision:Status", "1"),
                                    t("Provision:PolicyKey", key),
                                ],
                            )],
                        ),
                    ],
                )),
            )
        }
        _ => (500, None),
    })
    .await;
    let folders = eas.folders().await.unwrap();
    let named: Vec<(&str, FolderRole)> =
        folders.iter().map(|f| (f.name.as_str(), f.role)).collect();
    assert_eq!(
        named,
        [
            ("Inbox", FolderRole::Inbox),
            ("Sent Items", FolderRole::Sent),
            ("Inbox/Projects", FolderRole::Custom),
            ("Junk E-mail", FolderRole::Junk),
            ("Archive", FolderRole::Archive),
        ]
    );
    let took: Vec<String> = took
        .lock()
        .unwrap()
        .iter()
        .map(|(c, _)| c.clone())
        .collect();
    assert_eq!(took, ["FolderSync", "Provision", "Provision", "FolderSync"]);
    let line = seen.lock().unwrap()[0].line.clone();
    assert!(line.contains("User=corp%5Canna&DeviceId="), "{line}");
    assert!(line.contains("&DeviceType=OzeroMail "), "{line}");
    assert_eq!(state.get("policy").as_deref(), Some("P2"));

    // The next folder sync goes on from its key.
    let folders = eas.folders().await.unwrap();
    assert!(folders.iter().all(|f| f.name != "Archive"));
    assert_eq!(folders.len(), 4);
    assert_eq!(state.get("folders.key").as_deref(), Some("F2"));
}

#[tokio::test]
async fn syncs_mail_and_asks_again_for_an_answer_not_saved() {
    let (mut eas, _, took, _) = server(|cmd, request, _| match (cmd, sync_key(request)) {
        ("Sync", "0") => (200, collection("S1", "1", vec![])),
        ("Sync", "S1") => (
            200,
            collection(
                "S2",
                "1",
                vec![
                    item(
                        "AirSync:Add",
                        "1:1",
                        vec![
                            t("Email:DateReceived", "2026-10-26T10:00:00.000Z"),
                            t("Email:Read", "1"),
                            mime("From: a@b\r\nReceived: x\r\n y\r\nSubject: Hi\r\n\r\nbody"),
                        ],
                    ),
                    item(
                        "AirSync:Add",
                        "1:2",
                        vec![
                            t("Email:DateReceived", "2026-10-26T09:00:00.000Z"),
                            t("Email:From", "\"Bob\" <bob@x.org>"),
                            t("Email:Subject", "Cut"),
                            n("Email:Flag", vec![t("Email:Status", "2")]),
                            mime("From: bob@x.org\r\nX-Long: aaaa"),
                        ],
                    ),
                ],
            ),
        ),
        ("Sync", "S2") => (
            200,
            collection(
                "S3",
                "1",
                vec![
                    item(
                        "AirSync:Change",
                        "1:1",
                        vec![
                            t("Email:Read", "0"),
                            n("Email:Flag", vec![]),
                            t("Email2:LastVerbExecuted", "0"),
                        ],
                    ),
                    item(
                        "AirSync:Change",
                        "1:2",
                        vec![n("Email:Flag", vec![t("Email:Status", "2")])],
                    ),
                    n("AirSync:Delete", vec![t("AirSync:ServerId", "1:3")]),
                    n("AirSync:SoftDelete", vec![t("AirSync:ServerId", "1:1")]),
                ],
            ),
        ),
        ("Sync", "S3") => (200, collection("S3", "3", vec![])),
        ("ItemOperations", _) => (
            200,
            Some(n(
                "ItemOperations:ItemOperations",
                vec![
                    t("ItemOperations:Status", "1"),
                    n(
                        "ItemOperations:Response",
                        vec![n(
                            "ItemOperations:Fetch",
                            vec![
                                t("ItemOperations:Status", "1"),
                                t("AirSync:ServerId", "1:2"),
                                n(
                                    "ItemOperations:Properties",
                                    vec![
                                        t("Email:Read", "1"),
                                        n("Email:Flag", vec![t("Email:Status", "2")]),
                                        t("Email2:LastVerbExecuted", "1"),
                                    ],
                                ),
                            ],
                        )],
                    ),
                ],
            )),
        ),
        _ => (500, None),
    })
    .await;
    let first = eas
        .changes("1", Checkpoint::default(), &[], i64::MAX, 50)
        .await
        .unwrap();
    assert_eq!(
        first.checkpoint,
        Checkpoint {
            validity: 0,
            next: 1
        }
    );
    let ids: Vec<&str> = first.new.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(ids, ["1:2", "1:1"]);
    assert_eq!(
        String::from_utf8_lossy(&first.new[1].header),
        "From: a@b\r\nSubject: Hi\r\n\r\n"
    );
    assert!(first.new[1].flags.seen);
    // Its header did not fit what came: made from what the server says of it.
    let header = String::from_utf8_lossy(&first.new[0].header).into_owned();
    assert!(
        header.starts_with("From: \"Bob\" <bob@x.org>\r\n"),
        "{header}"
    );
    assert!(header.contains("Subject: Cut\r\n"), "{header}");
    assert!(first.new[0].flags.flagged);
    assert_eq!(first.new[0].received, 1_793_005_200);

    let known = ["1:1".to_owned(), "1:2".to_owned(), "1:3".to_owned()];
    for _ in 0..2 {
        // The second time as if the first answer was not saved.
        let next = eas
            .changes("1", first.checkpoint, &known, 0, 50)
            .await
            .unwrap();
        assert_eq!(next.checkpoint.next, 2);
        assert!(next.new.is_empty());
        assert_eq!(next.gone, ["1:3"]);
        let seen = Flags::default();
        let answered = Flags {
            seen: true,
            flagged: true,
            answered: true,
            draft: false,
        };
        assert_eq!(
            next.flags,
            [("1:1".to_owned(), seen), ("1:2".to_owned(), answered)]
        );
        let (_, last) = took
            .lock()
            .unwrap()
            .iter()
            .rfind(|(c, _)| c == "Sync")
            .cloned()
            .unwrap();
        assert_eq!(sync_key(&last), "S2");
    }

    // A key the server forgot starts the folder over, under a new validity.
    let over = eas
        .changes(
            "1",
            Checkpoint {
                validity: 0,
                next: 2,
            },
            &known,
            0,
            50,
        )
        .await
        .unwrap();
    assert_eq!(
        over.checkpoint,
        Checkpoint {
            validity: 1,
            next: 1
        }
    );
    assert_eq!(over.new.len(), 2);
    assert!(over.flags.is_empty() && over.gone.is_empty());
}

#[tokio::test]
async fn marks_moves_reads_and_sends() {
    let (mut eas, state, took, seen) = server(|cmd, request, _| match (cmd, sync_key(request)) {
        ("Sync", "0") => (200, collection("S1", "1", vec![])),
        ("Sync", "S1") => (200, collection("S2", "1", vec![])),
        ("MoveItems", _) => (
            200,
            Some(n(
                "Move:MoveItems",
                vec![
                    n(
                        "Move:Response",
                        vec![
                            t("Move:SrcMsgId", "1:1"),
                            t("Move:Status", "3"),
                            t("Move:DstMsgId", "2:7"),
                        ],
                    ),
                    n(
                        "Move:Response",
                        vec![t("Move:SrcMsgId", "1:2"), t("Move:Status", "1")],
                    ),
                ],
            )),
        ),
        ("ItemOperations", _) => (
            200,
            Some(n(
                "ItemOperations:ItemOperations",
                vec![
                    t("ItemOperations:Status", "1"),
                    n(
                        "ItemOperations:Response",
                        vec![n(
                            "ItemOperations:Fetch",
                            vec![
                                t("ItemOperations:Status", "1"),
                                t("AirSync:ServerId", "1:1"),
                                n(
                                    "ItemOperations:Properties",
                                    vec![mime("Subject: Hi\r\n\r\nall")],
                                ),
                            ],
                        )],
                    ),
                ],
            )),
        ),
        ("SendMail", _) => (200, None),
        _ => (500, None),
    })
    .await;
    eas.set_seen("1", 0, &["1:1".to_owned()], true)
        .await
        .unwrap();
    let (_, change) = took.lock().unwrap()[1].clone();
    let collection = change
        .at(&["AirSync:Collections", "AirSync:Collection"])
        .unwrap();
    assert_eq!(collection.text_at(&["AirSync:GetChanges"]), "0");
    assert_eq!(
        collection.text_at(&[
            "AirSync:Commands",
            "AirSync:Change",
            "AirSync:ApplicationData",
            "Email:Read"
        ]),
        "1"
    );
    // The key after it, under the count the folder's checkpoint has.
    assert_eq!(state.get("sync.1").as_deref(), Some("0\tS2\t\t5"));

    let moved = eas
        .move_to("1", 0, &["1:1".to_owned(), "1:2".to_owned()], "2")
        .await
        .unwrap();
    assert_eq!(
        moved,
        Moved {
            validity: Some(0),
            ids: vec![("1:1".to_owned(), "2:7".to_owned())]
        }
    );

    let body = eas.body("1", 0, "1:1").await.unwrap();
    assert_eq!(body, b"Subject: Hi\r\n\r\nall");

    let message = b"To: b@x.org\r\nSubject: Yo\r\n\r\nhi";
    eas.send("a@x.org", &[], message).await.unwrap();
    let raw = seen.lock().unwrap().last().unwrap().raw.clone();
    // Opaque: its length, then the message.
    let at = raw
        .windows(message.len())
        .position(|w| w == message)
        .unwrap();
    assert_eq!(raw[at - 2..at], [0xc3, message.len() as u8]);
    let (_, sent) = took.lock().unwrap().last().cloned().unwrap();
    assert!(sent.child("ComposeMail:SaveInSentItems").is_some());
}

#[tokio::test]
async fn tells_a_device_the_company_has_not_let_in() {
    let (mut eas, ..) = server(|_, _, _| {
        (
            200,
            Some(n(
                "FolderHierarchy:FolderSync",
                vec![t("FolderHierarchy:Status", "129")],
            )),
        )
    })
    .await;
    assert_eq!(eas.check().await, Err(ProviderError::Blocked));
    let (mut eas, ..) = server(|_, _, _| (403, None)).await;
    assert_eq!(eas.check().await, Err(ProviderError::Blocked));
}

#[tokio::test]
async fn pings_until_the_inbox_changes() {
    let (mut eas, _, took, _) = server(|cmd, request, _| {
        let heartbeat = request.text_at(&["Ping:HeartbeatInterval"]);
        let answer = match (cmd, heartbeat) {
            // Longer than this server takes: it says how long it does.
            ("Ping", "300") => vec![t("Ping:Status", "5"), t("Ping:HeartbeatInterval", "240")],
            ("Ping", _) => vec![t("Ping:Status", "2")],
            _ => vec![],
        };
        (200, Some(n("Ping:Ping", answer)))
    })
    .await;
    let waited = eas
        .wait("1", Checkpoint::default(), Duration::from_secs(300))
        .await;
    assert_eq!(waited, Ok(true));
    let took = took.lock().unwrap();
    let asked: Vec<_> = took
        .iter()
        .map(|(cmd, r)| {
            let folder = r.text_at(&["Ping:Folders", "Ping:Folder", "Ping:Id"]);
            (cmd.as_str(), r.text_at(&["Ping:HeartbeatInterval"]), folder)
        })
        .collect();
    assert_eq!(asked, [("Ping", "300", "1"), ("Ping", "240", "1")]);
}

#[tokio::test]
async fn looks_now_and_then_when_the_server_does_not_ping() {
    let (mut eas, ..) =
        server(|_, _, _| (200, Some(n("Ping:Ping", vec![t("Ping:Status", "8")])))).await;
    let waited = eas
        .wait("1", Checkpoint::default(), Duration::from_millis(10))
        .await;
    assert_eq!(waited, Ok(false));
}

#[test]
fn reads_times_with_and_without_fractions() {
    assert_eq!(time("2026-10-26T09:00:00.000Z"), Some(1_793_005_200));
    assert_eq!(time("2026-10-26T09:00:00Z"), Some(1_793_005_200));
    assert_eq!(time("soon"), None);
}

#[tokio::test]
async fn makes_the_snoozed_folder_at_the_top() {
    let (mut eas, state, took, _) = server(|cmd, _, _| match cmd {
        "FolderSync" => (
            200,
            Some(n(
                "FolderHierarchy:FolderSync",
                vec![
                    t("FolderHierarchy:Status", "1"),
                    t("FolderHierarchy:SyncKey", "F1"),
                    n(
                        "FolderHierarchy:Changes",
                        vec![folder("1", "0", "Inbox", "2")],
                    ),
                ],
            )),
        ),
        "FolderCreate" => (
            200,
            Some(n(
                "FolderHierarchy:FolderCreate",
                vec![
                    t("FolderHierarchy:Status", "1"),
                    t("FolderHierarchy:SyncKey", "F2"),
                    t("FolderHierarchy:ServerId", "9"),
                ],
            )),
        ),
        _ => (500, None),
    })
    .await;
    let made = eas.create_snoozed("Snoozed").await.unwrap();
    assert_eq!(
        made,
        NewFolder {
            remote_id: "9".into(),
            name: "Snoozed".into(),
            role: FolderRole::Snoozed,
        }
    );
    let create = took.lock().unwrap().last().unwrap().1.clone();
    assert_eq!(create.text_at(&["FolderHierarchy:SyncKey"]), "F1");
    assert_eq!(create.text_at(&["FolderHierarchy:ParentId"]), "0");
    assert_eq!(state.get("folders.key").as_deref(), Some("F2"));
    // The saved list has it, so the next listing finds it by its name.
    let folders = eas.folders().await.unwrap();
    assert!(
        folders
            .iter()
            .any(|f| f.remote_id == "9" && f.role == FolderRole::Snoozed)
    );
}

#[tokio::test]
async fn starts_a_folder_over_when_it_keeps_another_span() {
    let (eas, _, took, _) = server(|cmd, request, _| match (cmd, sync_key(request)) {
        ("Sync", "0") => (200, collection("S1", "1", vec![])),
        ("Sync", "S1") => (200, collection("S2", "1", vec![])),
        _ => (500, None),
    })
    .await;
    let filters = || -> Vec<(String, String)> {
        let took = took.lock().unwrap();
        let syncs = took.iter().filter(|(c, _)| c == "Sync");
        let path = [
            "AirSync:Collections",
            "AirSync:Collection",
            "AirSync:Options",
            "AirSync:FilterType",
        ];
        syncs
            .map(|(_, r)| (sync_key(r).to_owned(), r.text_at(&path).to_owned()))
            .filter(|(_, filter)| !filter.is_empty())
            .collect()
    };
    let mut eas = eas;
    let month = eas
        .changes("1", Checkpoint::default(), &[], i64::MAX, 50)
        .await
        .unwrap();
    assert_eq!(filters(), [("S1".to_owned(), "5".to_owned())]);

    let mut eas = eas.keeping(3);
    let days = eas
        .changes("1", month.checkpoint, &[], i64::MAX, 50)
        .await
        .unwrap();
    assert_eq!(days.checkpoint.validity, month.checkpoint.validity + 1);
    assert_eq!(filters()[1], ("S1".to_owned(), "2".to_owned()));
}

#[tokio::test]
async fn answers_an_invitation_in_the_calendar() {
    let (mut eas, _, took, _) = server(|_, request, _| {
        let id = request.text_at(&["MeetingResponse:Request", "MeetingResponse:RequestId"]);
        let status = if id == "1:9" { "2" } else { "1" };
        let result = n(
            "MeetingResponse:Result",
            vec![
                t("MeetingResponse:RequestId", id),
                t("MeetingResponse:Status", status),
            ],
        );
        (
            200,
            Some(n("MeetingResponse:MeetingResponse", vec![result])),
        )
    })
    .await;
    eas.answer("1", 0, "1:5", Answer::No).await.unwrap();
    assert_eq!(
        eas.answer("1", 0, "1:9", Answer::Yes).await,
        Err(ProviderError::Gone)
    );
    let took = took.lock().unwrap();
    let (cmd, request) = &took[0];
    assert_eq!(cmd, "MeetingResponse");
    let asked = |name| request.text_at(&["MeetingResponse:Request", name]);
    assert_eq!(asked("MeetingResponse:UserResponse"), "3");
    assert_eq!(asked("MeetingResponse:CollectionId"), "1");
    assert_eq!(asked("MeetingResponse:RequestId"), "1:5");
}
