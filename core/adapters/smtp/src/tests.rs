//! Against a scripted server on localhost, with the Dovecot test certificate.

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::{RootCertStore, ServerConfig};

use super::*;

const CA: &[u8] = include_bytes!("../../imap/tests/dovecot/ca.crt");
const CERT: &[u8] = include_bytes!("../../imap/tests/dovecot/tls.crt");
const KEY: &[u8] = include_bytes!("../../imap/tests/dovecot/tls.key");

fn client_tls() -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(CA).unwrap())
        .unwrap();
    Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
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

/// How the scripted server behaves.
#[derive(Clone, Copy)]
struct Script {
    implicit: bool,
    auth: &'static str,
    password_ok: bool,
    starttls: bool,
}

const SERVER: Script = Script {
    implicit: false,
    auth: "AUTH PLAIN LOGIN",
    password_ok: true,
    starttls: true,
};

/// Serves one session and returns every line the client sent, the message included.
async fn serve(listener: TcpListener, script: Script) -> Vec<String> {
    let (tcp, _) = listener.accept().await.unwrap();
    let mut seen = Vec::new();
    if script.implicit {
        let tls = acceptor().accept(tcp).await.unwrap();
        session(BufReader::new(tls), script, &mut seen).await;
        return seen;
    }
    let mut plain = BufReader::new(tcp);
    plain
        .get_mut()
        .write_all(b"220 test ESMTP\r\n")
        .await
        .unwrap();
    loop {
        let line = read(&mut plain).await;
        seen.push(line.clone());
        if line.starts_with("EHLO") {
            let tls = if script.starttls {
                "250-STARTTLS\r\n"
            } else {
                ""
            };
            let reply = format!("250-test\r\n{tls}250 SIZE 1000000\r\n");
            plain.get_mut().write_all(reply.as_bytes()).await.unwrap();
        } else if line == "STARTTLS" {
            plain
                .get_mut()
                .write_all(b"220 go ahead\r\n")
                .await
                .unwrap();
            break;
        } else {
            plain.get_mut().write_all(b"500 no\r\n").await.unwrap();
            return seen;
        }
    }
    let tls = acceptor().accept(plain.into_inner()).await.unwrap();
    session(BufReader::new(tls), script, &mut seen).await;
    seen
}

async fn session<S: AsyncRead + AsyncWrite + Unpin>(
    mut conn: BufReader<S>,
    script: Script,
    seen: &mut Vec<String>,
) {
    if script.implicit {
        conn.get_mut()
            .write_all(b"220 test ESMTP\r\n")
            .await
            .unwrap();
    }
    loop {
        let line = read(&mut conn).await;
        seen.push(line.clone());
        let reply: String = if line.starts_with("EHLO") {
            format!(
                "250-test greets you\r\n250-{}\r\n250 8BITMIME\r\n",
                script.auth
            )
        } else if line.starts_with("AUTH PLAIN") || (line == "AUTH LOGIN") {
            if line == "AUTH LOGIN" {
                conn.get_mut()
                    .write_all(b"334 VXNlcm5hbWU6\r\n")
                    .await
                    .unwrap();
                seen.push(read(&mut conn).await);
                conn.get_mut()
                    .write_all(b"334 UGFzc3dvcmQ6\r\n")
                    .await
                    .unwrap();
                seen.push(read(&mut conn).await);
            }
            if script.password_ok {
                "235 ok\r\n"
            } else {
                "535 5.7.8 bad\r\n"
            }
            .into()
        } else if line.starts_with("MAIL FROM")
            || line.starts_with("RCPT TO:<bob")
            || line == "RSET"
        {
            "250 ok\r\n".into()
        } else if line.starts_with("RCPT TO") {
            "550 5.1.1 no such user\r\n".into()
        } else if line == "DATA" {
            conn.get_mut().write_all(b"354 go\r\n").await.unwrap();
            loop {
                let data = read(&mut conn).await;
                seen.push(data.clone());
                if data == "." {
                    break;
                }
            }
            "250 2.0.0 queued\r\n".into()
        } else if line == "QUIT" {
            conn.get_mut().write_all(b"221 bye\r\n").await.unwrap();
            return;
        } else {
            "500 what\r\n".into()
        };
        conn.get_mut().write_all(reply.as_bytes()).await.unwrap();
    }
}

async fn read<S: AsyncRead + Unpin>(conn: &mut BufReader<S>) -> String {
    let mut line = String::new();
    conn.read_line(&mut line).await.unwrap();
    line.trim_end_matches(['\r', '\n']).to_owned()
}

async fn run(script: Script, to: &str, message: &[u8]) -> (Result<(), ProviderError>, Vec<String>) {
    run_all(script, &[to], message).await
}

/// Sends one message to each address in turn; the result is the first failure, if any.
async fn run_all(
    script: Script,
    to: &[&str],
    message: &[u8],
) -> (Result<(), ProviderError>, Vec<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(serve(listener, script));
    let result = async {
        let mut smtp = SmtpSession::open(
            "localhost",
            port,
            script.implicit,
            client_tls(),
            "anna",
            "pa ss",
        )
        .await?;
        let mut first = Ok(());
        for to in to {
            let sent = smtp
                .send("anna@example.com", &[(*to).to_owned()], message)
                .await;
            first = first.and(sent);
        }
        smtp.quit().await?;
        first
    }
    .await;
    // A failed client leaves the server waiting for a line that never comes.
    if matches!(result, Err(ref e) if !matches!(e, ProviderError::Rejected(_))) {
        server.abort();
    }
    let seen = server.await.unwrap_or_default();
    (result, seen)
}

#[tokio::test(flavor = "current_thread")]
async fn sends_after_starttls_with_auth_plain() {
    let (result, seen) = run(
        SERVER,
        "bob@example.org",
        b"Subject: Hi\r\n\r\n.dot\r\nend\r\n",
    )
    .await;
    result.unwrap();
    assert!(seen[0].starts_with("EHLO [127.0.0.1]"), "{seen:?}");
    assert_eq!(seen[1], "STARTTLS");
    // "\0anna\0pa ss"
    assert_eq!(seen[3], "AUTH PLAIN AGFubmEAcGEgc3M=");
    assert_eq!(
        &seen[4..],
        [
            "MAIL FROM:<anna@example.com>",
            "RCPT TO:<bob@example.org>",
            "DATA",
            "Subject: Hi",
            "",
            "..dot",
            "end",
            ".",
            "QUIT",
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn uses_implicit_tls_and_auth_login() {
    let script = Script {
        implicit: true,
        auth: "AUTH LOGIN XOAUTH2",
        ..SERVER
    };
    let (result, seen) = run(script, "bob@example.org", b"x").await;
    result.unwrap();
    assert_eq!(seen[1..4], ["AUTH LOGIN", "YW5uYQ==", "cGEgc3M="]);
}

#[tokio::test(flavor = "current_thread")]
async fn a_wrong_password_is_auth() {
    let script = Script {
        password_ok: false,
        ..SERVER
    };
    assert_eq!(
        run(script, "bob@example.org", b"x").await.0,
        Err(ProviderError::Auth)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn refuses_a_server_without_tls() {
    let script = Script {
        starttls: false,
        ..SERVER
    };
    let (result, seen) = run(script, "bob@example.org", b"x").await;
    assert!(
        matches!(result, Err(ProviderError::Protocol(_))),
        "{result:?}"
    );
    // The login never went out.
    assert!(!seen.iter().any(|l| l.starts_with("AUTH")), "{seen:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn a_refused_recipient_is_permanent() {
    let (result, _) = run(SERVER, "nobody@example.org", b"x").await;
    match result {
        Err(ProviderError::Rejected(text)) => assert!(text.contains("550"), "{text}"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_refused_message_does_not_block_the_next() {
    let (result, seen) = run_all(SERVER, &["nobody@example.org", "bob@example.org"], b"x").await;
    assert!(
        matches!(result, Err(ProviderError::Rejected(_))),
        "{result:?}"
    );
    let rset = seen.iter().position(|l| l == "RSET").unwrap();
    assert_eq!(seen[rset + 1], "MAIL FROM:<anna@example.com>");
    assert!(seen.iter().any(|l| l == "DATA"), "{seen:?}");
}
