//! `MailSender` over SMTP submission (RFC 6409). TLS from the first byte on port 465,
//! STARTTLS on any other port; the password never goes over a plain connection.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use ozero_domain::{MailSender, ProviderError, base64};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

pub use tokio_rustls::rustls;

/// Any single read or write that takes longer means the connection is dead.
const TIMEOUT: Duration = Duration::from_secs(30);
/// Longest reply line we accept; RFC 5321 allows 512 bytes.
const MAX_LINE: usize = 4096;
/// Most lines in one reply; EHLO lists a few dozen at most.
const MAX_LINES: usize = 100;
/// The port where TLS starts before SMTP (RFC 8314).
const IMPLICIT_TLS: u16 = 465;

#[derive(Debug)]
pub struct SmtpSession {
    conn: Conn<TlsStream<TcpStream>>,
}

impl SmtpSession {
    /// Connects, sets up TLS and logs in with AUTH PLAIN or LOGIN.
    pub async fn connect(
        host: &str,
        port: u16,
        tls: Arc<ClientConfig>,
        user: &str,
        password: &str,
    ) -> Result<Self, ProviderError> {
        Self::open(host, port, port == IMPLICIT_TLS, tls, user, password).await
    }

    async fn open(
        host: &str,
        port: u16,
        implicit: bool,
        tls: Arc<ClientConfig>,
        user: &str,
        password: &str,
    ) -> Result<Self, ProviderError> {
        let name = ServerName::try_from(host.to_owned())
            .map_err(|_| ProviderError::Network(format!("bad host name {host}")))?;
        let tcp = timed(TcpStream::connect((host, port))).await?;
        let hello = match tcp.local_addr().map(|a| a.ip()) {
            Ok(IpAddr::V4(ip)) => format!("EHLO [{ip}]"),
            Ok(IpAddr::V6(ip)) => format!("EHLO [IPv6:{ip}]"),
            Err(_) => "EHLO localhost".into(),
        };
        let tls = TlsConnector::from(tls);
        let mut conn = if implicit {
            Conn::new(timed(tls.connect(name, tcp)).await?)
        } else {
            let mut plain = Conn::new(tcp);
            plain.reply().await?.expect(220, "greeting")?;
            let offers = plain.command(&hello).await?.expect(250, "EHLO")?;
            if !offers.has("STARTTLS") {
                return Err(protocol("the server offers no TLS"));
            }
            plain.command("STARTTLS").await?.expect(220, "STARTTLS")?;
            // Anything the server sent before TLS could be an attacker's answer to what we
            // send after it (CVE-2011-0411).
            if !plain.buf.is_empty() {
                return Err(protocol("data after STARTTLS"));
            }
            Conn::new(timed(tls.connect(name, plain.stream)).await?)
        };
        if implicit {
            conn.reply().await?.expect(220, "greeting")?;
        }
        let offers = conn.command(&hello).await?.expect(250, "EHLO")?;
        let mut session = Self { conn };
        session.login(&offers, user, password).await?;
        Ok(session)
    }

    async fn login(
        &mut self,
        offers: &Reply,
        user: &str,
        password: &str,
    ) -> Result<(), ProviderError> {
        let reply = if offers.has_auth("PLAIN") {
            let token = base64(format!("\0{user}\0{password}").as_bytes());
            self.conn.command(&format!("AUTH PLAIN {token}")).await?
        } else if offers.has_auth("LOGIN") {
            self.conn.command("AUTH LOGIN").await?.expect(334, "AUTH")?;
            self.conn
                .command(&base64(user.as_bytes()))
                .await?
                .expect(334, "AUTH")?;
            self.conn.command(&base64(password.as_bytes())).await?
        } else {
            return Err(protocol("the server offers no AUTH PLAIN or LOGIN"));
        };
        match reply.code {
            235 => Ok(()),
            // RFC 4954: bad credentials, or a login the server wants done differently.
            530 | 534 | 535 | 538 => Err(ProviderError::Auth),
            _ => reply.expect(235, "AUTH").map(|_| ()),
        }
    }

    pub async fn quit(mut self) -> Result<(), ProviderError> {
        self.conn.command("QUIT").await.map(|_| ())
    }
}

impl MailSender for SmtpSession {
    async fn send(
        &mut self,
        from: &str,
        to: &[String],
        message: &[u8],
    ) -> Result<(), ProviderError> {
        // Addresses come checked, but a line break here would be a second command.
        if std::iter::once(from)
            .chain(to.iter().map(String::as_str))
            .any(|a| a.chars().any(|c| c.is_control() || c == '<' || c == '>'))
        {
            return Err(protocol("bad address"));
        }
        self.conn
            .command(&format!("MAIL FROM:<{from}>"))
            .await?
            .expect(250, "MAIL")?;
        for address in to {
            let reply = self.conn.command(&format!("RCPT TO:<{address}>")).await?;
            // 251: the server forwards it on.
            if reply.code != 251 {
                reply.expect(250, "RCPT")?;
            }
        }
        self.conn.command("DATA").await?.expect(354, "DATA")?;
        self.conn.write(&dot_stuffed(message)).await?;
        self.conn.reply().await?.expect(250, "DATA")?;
        Ok(())
    }
}

/// The message as DATA sends it: a dot doubled at the start of a line, ending in `.` alone.
fn dot_stuffed(message: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(message.len() + 8);
    for line in message.split_inclusive(|b| *b == b'\n') {
        if line.first() == Some(&b'.') {
            out.push(b'.');
        }
        out.extend_from_slice(line);
    }
    if !out.ends_with(b"\r\n") {
        if out.ends_with(b"\n") {
            out.pop();
        }
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    out
}

/// One SMTP reply: a code and its text lines.
#[derive(Debug)]
struct Reply {
    code: u16,
    lines: Vec<String>,
}

impl Reply {
    /// Ok if the server answered `code`; otherwise temporary (4xx) or permanent.
    fn expect(self, code: u16, what: &str) -> Result<Self, ProviderError> {
        if self.code == code {
            return Ok(self);
        }
        let text = format!("{what}: {} {}", self.code, self.lines.join(" "));
        if (400..500).contains(&self.code) {
            Err(ProviderError::Network(text))
        } else {
            Err(protocol(text))
        }
    }

    /// An EHLO keyword, such as `STARTTLS`; the first line is the greeting.
    fn has(&self, keyword: &str) -> bool {
        self.lines.iter().skip(1).any(|l| {
            l.split_whitespace()
                .next()
                .is_some_and(|k| k.eq_ignore_ascii_case(keyword))
        })
    }

    fn has_auth(&self, mechanism: &str) -> bool {
        self.lines.iter().skip(1).any(|l| {
            let mut words = l.split_whitespace();
            words.next().is_some_and(|k| k.eq_ignore_ascii_case("AUTH"))
                && words.any(|m| m.eq_ignore_ascii_case(mechanism))
        })
    }
}

/// A connection that reads replies line by line.
#[derive(Debug)]
struct Conn<S> {
    stream: S,
    /// Read but not yet used.
    buf: Vec<u8>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Conn<S> {
    fn new(stream: S) -> Self {
        Self {
            stream,
            buf: Vec::new(),
        }
    }

    async fn command(&mut self, line: &str) -> Result<Reply, ProviderError> {
        self.write(format!("{line}\r\n").as_bytes()).await?;
        self.reply().await
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), ProviderError> {
        timed(self.stream.write_all(data)).await?;
        timed(self.stream.flush()).await
    }

    /// Reads `250-first`, `250-second`, `250 last` into one reply.
    async fn reply(&mut self) -> Result<Reply, ProviderError> {
        let mut lines = Vec::new();
        loop {
            let line = self.line().await?;
            let code = line
                .get(..3)
                .and_then(|c| c.parse::<u16>().ok())
                .filter(|c| (200..600).contains(c))
                .ok_or_else(|| protocol("bad reply"))?;
            let more = line.as_bytes().get(3) == Some(&b'-');
            lines.push(line.get(4..).unwrap_or_default().to_owned());
            if !more {
                return Ok(Reply { code, lines });
            }
            if lines.len() >= MAX_LINES {
                return Err(protocol("reply too long"));
            }
        }
    }

    async fn line(&mut self) -> Result<String, ProviderError> {
        loop {
            if let Some(end) = self.buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=end).collect();
                let line = String::from_utf8_lossy(&line);
                return Ok(line.trim_end_matches(['\r', '\n']).to_owned());
            }
            if self.buf.len() > MAX_LINE {
                return Err(protocol("reply line too long"));
            }
            let mut chunk = [0; 1024];
            let n = timed(self.stream.read(&mut chunk)).await?;
            if n == 0 {
                return Err(ProviderError::Network(
                    "the server closed the connection".into(),
                ));
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn stuffs_dots_and_ends_the_data() {
        assert_eq!(
            dot_stuffed(b"a\r\n.b\r\n..c\r\n"),
            b"a\r\n..b\r\n...c\r\n.\r\n"
        );
        assert_eq!(dot_stuffed(b"no break"), b"no break\r\n.\r\n");
        assert_eq!(dot_stuffed(b"lf only\n"), b"lf only\r\n.\r\n");
        assert_eq!(dot_stuffed(b""), b"\r\n.\r\n");
    }
}
