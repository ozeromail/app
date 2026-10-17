//! Minimal HTTPS/1.1 client for the providers' APIs: one kept-alive connection to one host,
//! one request at a time. No compression, no plain HTTP; only `fetch` follows redirects.

mod fetch;

pub use fetch::{MAX_IMAGE, image};

use std::sync::Arc;
use std::time::{Duration, Instant};

use ozero_domain::ProviderError;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

/// Any single read or write that takes longer means the connection is dead.
const TIMEOUT: Duration = Duration::from_secs(30);
/// A connection idle this long may have been closed by the server; a new one is opened.
const IDLE: Duration = Duration::from_secs(20);
/// Largest response head we accept.
const MAX_HEAD: usize = 64 * 1024;
/// Largest response body: a 25 MB message in base64 inside JSON fits.
const MAX_BODY: usize = 64 * 1024 * 1024;
const MAX_HEADERS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    /// The Content-Type header, empty without one.
    pub content_type: String,
    /// The Location header, empty without one.
    pub location: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// A request body with its media type.
#[derive(Debug, Clone, Copy)]
pub struct Body<'a> {
    pub content_type: &'a str,
    pub data: &'a [u8],
}

/// Talks to one host over TLS on port 443 or the given one.
#[derive(Debug)]
pub struct HttpsClient {
    host: String,
    port: u16,
    tls: Arc<ClientConfig>,
    conn: Option<(Conn<TlsStream<TcpStream>>, Instant)>,
    /// Largest response body.
    limit: usize,
    /// Refuses hosts that resolve to a private or local address.
    public_only: bool,
}

impl HttpsClient {
    pub fn new(host: &str, port: u16, tls: Arc<ClientConfig>) -> Self {
        Self {
            host: host.to_owned(),
            port,
            tls,
            conn: None,
            limit: MAX_BODY,
            public_only: false,
        }
    }

    /// For a host named by someone else, such as an image in a message: it may not reach
    /// into the local network, and its responses may be at most `limit` bytes.
    pub fn untrusted(host: &str, port: u16, tls: Arc<ClientConfig>, limit: usize) -> Self {
        Self {
            limit,
            public_only: true,
            ..Self::new(host, port, tls)
        }
    }

    /// Sends a request and reads the whole response. `path` holds the query too; `headers`
    /// are added to Host, Accept and Content-Length.
    pub async fn send(
        &mut self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<Body<'_>>,
    ) -> Result<Response, ProviderError> {
        self.exchange(method, path, headers, body, method == "GET")
            .await
    }

    /// Like `send`, for a request that is safe to send twice whatever its method.
    pub async fn send_repeatable(
        &mut self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<Body<'_>>,
    ) -> Result<Response, ProviderError> {
        self.exchange(method, path, headers, body, true).await
    }

    async fn exchange(
        &mut self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<Body<'_>>,
        repeatable: bool,
    ) -> Result<Response, ProviderError> {
        let request = request(method, &self.host, path, headers, body)?;
        let reused = match self.conn.take() {
            Some((conn, used)) if used.elapsed() < IDLE => Some(conn),
            _ => None,
        };
        let (response, conn) = match reused {
            Some(conn) => match exchange(conn, &request).await {
                Ok(done) => done,
                // The server closed the kept-alive connection before answering; a new one
                // asks again, but only what is safe to ask twice: a POST may have gone through.
                Err(Failure::Closed) if repeatable => {
                    exchange(self.connect().await?, &request).await?
                }
                Err(Failure::Closed) => return Err(closed()),
                Err(Failure::Error(e)) => return Err(e),
            },
            None => exchange(self.connect().await?, &request).await?,
        };
        if let Some(conn) = conn {
            self.conn = Some((conn, Instant::now()));
        }
        Ok(response)
    }

    async fn connect(&self) -> Result<Conn<TlsStream<TcpStream>>, ProviderError> {
        let name = ServerName::try_from(self.host.clone())
            .map_err(|_| ProviderError::Network(format!("bad host name {}", self.host)))?;
        let tcp = if self.public_only {
            let addrs = timed(tokio::net::lookup_host((self.host.as_str(), self.port))).await?;
            let public: Vec<_> = addrs.filter(|a| fetch::is_public(a.ip())).collect();
            if public.is_empty() {
                return Err(ProviderError::Network(format!(
                    "{} is not public",
                    self.host
                )));
            }
            timed(TcpStream::connect(&public[..])).await?
        } else {
            timed(TcpStream::connect((self.host.as_str(), self.port))).await?
        };
        let tls = TlsConnector::from(self.tls.clone());
        let mut conn = Conn::new(timed(tls.connect(name, tcp)).await?);
        conn.limit = self.limit;
        Ok(conn)
    }
}

enum Failure {
    /// Closed before any byte of a response came.
    Closed,
    Error(ProviderError),
}

impl From<ProviderError> for Failure {
    fn from(e: ProviderError) -> Self {
        Self::Error(e)
    }
}

impl From<Failure> for ProviderError {
    fn from(f: Failure) -> Self {
        match f {
            Failure::Closed => closed(),
            Failure::Error(e) => e,
        }
    }
}

/// Sends one request and reads its response; returns the connection if it stays open.
async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut conn: Conn<S>,
    request: &[u8],
) -> Result<(Response, Option<Conn<S>>), Failure> {
    if conn.write(request).await.is_err() {
        return Err(Failure::Closed);
    }
    let (response, keep) = conn.response().await?;
    Ok((response, keep.then_some(conn)))
}

/// The request as sent. Refuses line breaks that would smuggle in more headers.
fn request(
    method: &str,
    host: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Body<'_>>,
) -> Result<Vec<u8>, ProviderError> {
    let clean = |s: &str| !s.contains(['\r', '\n']);
    let fields = headers.iter().flat_map(|(n, v)| [*n, *v]);
    if !path.starts_with('/') || ![method, host, path].into_iter().chain(fields).all(clean) {
        return Err(ProviderError::Protocol("bad request".into()));
    }
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n");
    if !headers
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("accept"))
    {
        head.push_str("Accept: application/json\r\n");
    }
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        if !clean(body.content_type) {
            return Err(ProviderError::Protocol("bad request".into()));
        }
        head.push_str(&format!(
            "Content-Type: {}\r\nContent-Length: {}\r\n",
            body.content_type,
            body.data.len()
        ));
    }
    head.push_str("\r\n");
    let mut out = head.into_bytes();
    if let Some(body) = body {
        out.extend_from_slice(body.data);
    }
    Ok(out)
}

#[derive(Debug)]
struct Conn<S> {
    stream: S,
    /// Read but not yet used.
    buf: Vec<u8>,
    /// Largest body.
    limit: usize,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Conn<S> {
    fn new(stream: S) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            limit: MAX_BODY,
        }
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), ProviderError> {
        timed(self.stream.write_all(data)).await?;
        timed(self.stream.flush()).await
    }

    /// Reads more into `buf`; false at the end of the stream.
    async fn fill(&mut self) -> Result<bool, ProviderError> {
        let mut chunk = [0; 16 * 1024];
        let n = timed(self.stream.read(&mut chunk)).await?;
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(n > 0)
    }

    /// The response and whether the connection can carry another request.
    async fn response(&mut self) -> Result<(Response, bool), Failure> {
        let (status, content_type, location, length, close) = loop {
            if let Some(head) = self.head()? {
                break head;
            }
            if self.buf.len() > MAX_HEAD {
                return Err(protocol("response head too long").into());
            }
            if !self.fill().await? {
                return Err(if self.buf.is_empty() {
                    Failure::Closed
                } else {
                    closed().into()
                });
            }
        };
        let body = match length {
            Length::None => Vec::new(),
            Length::Fixed(n) => self.fixed(n).await?,
            Length::Chunked => self.chunked().await?,
            Length::ToEnd => self.until_closed().await?,
        };
        let keep = !close && !matches!(length, Length::ToEnd);
        Ok((
            Response {
                status,
                content_type,
                location,
                body,
            },
            keep,
        ))
    }

    /// Parses and takes the head from `buf` once it is all there.
    #[allow(clippy::type_complexity)]
    fn head(&mut self) -> Result<Option<(u16, String, String, Length, bool)>, ProviderError> {
        let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
        let mut parsed = httparse::Response::new(&mut headers);
        let size = match parsed.parse(&self.buf) {
            Ok(httparse::Status::Complete(size)) => size,
            Ok(httparse::Status::Partial) => return Ok(None),
            Err(e) => return Err(protocol(format!("bad response: {e}"))),
        };
        let status = parsed.code.ok_or_else(|| protocol("no status"))?;
        if (100..200).contains(&status) {
            // We send no Expect, so nothing should wait for one; skip it all the same.
            self.buf.drain(..size);
            return self.head();
        }
        let header = |name| values(parsed.headers, name);
        let close = header("connection").iter().any(|v| v == "close");
        let chunked = header("transfer-encoding")
            .iter()
            .any(|v| v.rsplit(',').next().is_some_and(|t| t.trim() == "chunked"));
        let lengths = header("content-length");
        let length = if status == 204 || status == 304 {
            Length::None
        } else if chunked {
            Length::Chunked
        } else if let [length] = lengths.as_slice() {
            let n = length
                .parse::<usize>()
                .map_err(|_| protocol("bad content length"))?;
            Length::Fixed(n)
        } else if lengths.is_empty() {
            Length::ToEnd
        } else {
            return Err(protocol("more than one content length"));
        };
        let content_type = content_type(parsed.headers);
        let location = raw(parsed.headers, "location");
        self.buf.drain(..size);
        Ok(Some((status, content_type, location, length, close)))
    }

    async fn fixed(&mut self, n: usize) -> Result<Vec<u8>, ProviderError> {
        if n > self.limit {
            return Err(protocol("response too large"));
        }
        while self.buf.len() < n {
            if !self.fill().await? {
                return Err(closed());
            }
        }
        // Hands over the body without copying it; what follows stays in `buf`.
        let rest = self.buf.split_off(n);
        Ok(std::mem::replace(&mut self.buf, rest))
    }

    async fn chunked(&mut self) -> Result<Vec<u8>, ProviderError> {
        let mut body = Vec::new();
        loop {
            let line = self.line().await?;
            let size = line.split(';').next().unwrap_or_default().trim();
            let size = usize::from_str_radix(size, 16).map_err(|_| protocol("bad chunk size"))?;
            if size == 0 {
                // Trailers, then an empty line.
                while !self.line().await?.is_empty() {}
                return Ok(body);
            }
            if size > self.limit - body.len() {
                return Err(protocol("response too large"));
            }
            body.extend(self.fixed(size).await?);
            if !self.line().await?.is_empty() {
                return Err(protocol("bad chunk end"));
            }
        }
    }

    async fn until_closed(&mut self) -> Result<Vec<u8>, ProviderError> {
        while self.fill().await? {
            if self.buf.len() > self.limit {
                return Err(protocol("response too large"));
            }
        }
        Ok(std::mem::take(&mut self.buf))
    }

    async fn line(&mut self) -> Result<String, ProviderError> {
        loop {
            if let Some(end) = self.buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=end).collect();
                let line = String::from_utf8_lossy(&line);
                return Ok(line.trim_end_matches(['\r', '\n']).to_owned());
            }
            if self.buf.len() > MAX_HEAD {
                return Err(protocol("line too long"));
            }
            if !self.fill().await? {
                return Err(closed());
            }
        }
    }
}

/// A whole response held in `data`, as a part of a multipart body carries one: its head,
/// then everything after it as the body. None if the head is not a complete response head.
pub fn parse(data: &[u8]) -> Option<Response> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Response::new(&mut headers);
    let httparse::Status::Complete(size) = parsed.parse(data).ok()? else {
        return None;
    };
    Some(Response {
        status: parsed.code?,
        content_type: content_type(parsed.headers),
        location: raw(parsed.headers, "location"),
        body: data[size..].to_vec(),
    })
}

/// The values of every header called `name`, trimmed and lowercased.
fn values(headers: &[httparse::Header<'_>], name: &str) -> Vec<String> {
    headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| String::from_utf8_lossy(h.value).trim().to_ascii_lowercase())
        .collect()
}

/// As sent: a multipart boundary in it is case-sensitive.
fn content_type(headers: &[httparse::Header<'_>]) -> String {
    raw(headers, "content-type")
}

/// The first header called `name` as sent, trimmed; empty without one.
fn raw(headers: &[httparse::Header<'_>], name: &str) -> String {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| String::from_utf8_lossy(h.value).trim().to_owned())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy)]
enum Length {
    None,
    Fixed(usize),
    Chunked,
    /// Until the server closes the connection.
    ToEnd,
}

async fn timed<T, E: std::fmt::Display>(
    op: impl Future<Output = Result<T, E>>,
) -> Result<T, ProviderError> {
    match tokio::time::timeout(TIMEOUT, op).await {
        Ok(result) => result.map_err(|e| ProviderError::Network(e.to_string())),
        Err(_) => Err(ProviderError::Network("timed out".into())),
    }
}

fn closed() -> ProviderError {
    ProviderError::Network("the server closed the connection".into())
}

fn protocol(msg: impl Into<String>) -> ProviderError {
    ProviderError::Protocol(msg.into())
}

#[cfg(test)]
mod tests;
