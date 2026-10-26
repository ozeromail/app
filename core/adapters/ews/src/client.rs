//! One HTTPS endpoint of an Exchange server, with the login it asks for: Basic, or NTLM bound
//! to the connection and, for Extended Protection, to the server's certificate.

use std::time::{SystemTime, UNIX_EPOCH};

use ozero_domain::{ProviderError, base64, decode_base64};
use ozero_http::{Body, HttpsClient, Response, Tls};
use ring::rand::{SecureRandom, SystemRandom};

use crate::ntlm;

/// How the server takes the login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    Basic,
    /// NTLM, under the name the server offered it by: `NTLM` or `Negotiate`.
    Ntlm(&'static str),
}

#[derive(Debug)]
pub struct Client {
    http: HttpsClient,
    path: String,
    login: String,
    password: String,
    /// None until the server has said which it takes.
    scheme: Option<Scheme>,
    /// NTLM signed in on the open connection.
    bound: bool,
    /// Asks unsigned first, so the password goes only to a server that asks for a login.
    probe: bool,
}

impl Client {
    pub fn new(
        tls: impl Into<Tls>,
        host: &str,
        port: u16,
        path: &str,
        login: &str,
        password: &str,
    ) -> Self {
        Self::over(HttpsClient::new(host, port, tls), path, login, password)
    }

    pub(crate) fn over(http: HttpsClient, path: &str, login: &str, password: &str) -> Self {
        Self {
            http,
            path: path.to_owned(),
            login: login.to_owned(),
            password: password.to_owned(),
            scheme: None,
            bound: false,
            probe: false,
        }
    }

    /// For a server not known to be the account's yet, as discovery tries.
    pub fn probing(mut self) -> Self {
        self.probe = true;
        self
    }

    /// POSTs `body` signed in. `repeatable` if it is safe to send twice. `Auth` if the server
    /// refuses the login.
    pub async fn post(
        &mut self,
        content_type: &str,
        body: &[u8],
        repeatable: bool,
    ) -> Result<Response, ProviderError> {
        let path = self.path.clone();
        self.post_to(
            &path,
            &[("Accept", "text/xml")],
            content_type,
            body,
            repeatable,
        )
        .await
    }

    /// Like `post`, to `path` on the same server, with these `headers` too.
    pub async fn post_to(
        &mut self,
        path: &str,
        headers: &[(&str, &str)],
        content_type: &str,
        body: &[u8],
        repeatable: bool,
    ) -> Result<Response, ProviderError> {
        // A known server gets Basic first: one without it refuses it unread, and says what it
        // takes instead.
        let mut scheme = self.scheme.or((!self.probe).then_some(Scheme::Basic));
        let mut failure = ProviderError::Auth;
        for _ in 0..3 {
            let fresh = !(self.bound && self.http.is_open());
            let auth = match scheme {
                Some(Scheme::Ntlm(name)) if fresh => {
                    Some(self.ntlm(name, path, headers, content_type).await?)
                }
                None | Some(Scheme::Ntlm(_)) => None,
                Some(Scheme::Basic) => Some(format!(
                    "Basic {}",
                    base64(format!("{}:{}", self.login, self.password).as_bytes())
                )),
            };
            // NTLM's last message holds only on the connection that got the challenge, so it is
            // never resent on a new one: the whole handshake is, if the request may go twice.
            let handshake = fresh && matches!(scheme, Some(Scheme::Ntlm(_)));
            let response = match self
                .send(
                    auth.as_deref(),
                    path,
                    headers,
                    content_type,
                    body,
                    repeatable && !handshake,
                )
                .await
            {
                Err(e @ ProviderError::Network(_)) if handshake && repeatable => {
                    self.bound = false;
                    failure = e;
                    continue;
                }
                response => response?,
            };
            self.bound = response.status != 401 && matches!(scheme, Some(Scheme::Ntlm(_)));
            if response.status != 401 {
                self.scheme = self.scheme.or(scheme);
                return Ok(response);
            }
            failure = ProviderError::Auth;
            match scheme {
                None | Some(Scheme::Basic) if self.scheme.is_none() => {
                    let offered = offered(&response)?;
                    self.scheme = Some(offered);
                    if scheme == Some(offered) {
                        break;
                    }
                    scheme = Some(offered);
                }
                // The connection NTLM signed in on has gone: once more on a new one.
                Some(Scheme::Ntlm(_)) if !fresh => {}
                _ => break,
            }
        }
        Err(failure)
    }

    /// Runs NTLM up to its last message on the open connection, and returns the header that
    /// carries that message with the real request.
    async fn ntlm(
        &mut self,
        name: &str,
        path: &str,
        headers: &[(&str, &str)],
        content_type: &str,
    ) -> Result<String, ProviderError> {
        let hello = format!("{name} {}", base64(&ntlm::negotiate()));
        // Empty, so the body goes only once.
        let response = self
            .send(Some(&hello), path, headers, content_type, &[], true)
            .await?;
        let token = response
            .authenticate
            .iter()
            .find_map(|a| {
                let (scheme, token) = a.split_once(' ')?;
                scheme.eq_ignore_ascii_case(name).then(|| token.trim())
            })
            .filter(|_| response.status == 401)
            .ok_or(ProviderError::Auth)?;
        let challenge = decode_base64(token)
            .as_deref()
            .and_then(ntlm::challenge)
            .ok_or_else(|| ProviderError::Protocol("bad NTLM challenge".into()))?;
        if !self.http.is_open() {
            return Err(ProviderError::Protocol(
                "the server closed the connection during NTLM".into(),
            ));
        }
        let binding = self.http.certificate().map(|c| ntlm::channel_binding(&c));
        let client = random::<8>()?;
        let answer = ntlm::authenticate(
            &challenge,
            &self.login,
            &self.password,
            binding,
            client,
            filetime(),
        );
        Ok(format!("{name} {}", base64(&answer)))
    }

    async fn send(
        &mut self,
        auth: Option<&str>,
        path: &str,
        headers: &[(&str, &str)],
        content_type: &str,
        data: &[u8],
        repeatable: bool,
    ) -> Result<Response, ProviderError> {
        let headers: Vec<(&str, &str)> = auth
            .map(|a| ("Authorization", a))
            .into_iter()
            .chain(headers.iter().copied())
            .collect();
        let body = Some(Body { content_type, data });
        if repeatable {
            self.http
                .send_repeatable("POST", path, &headers, body)
                .await
        } else {
            self.http.send("POST", path, &headers, body).await
        }
    }
}

/// The login a 401 asks for.
fn offered(response: &Response) -> Result<Scheme, ProviderError> {
    let offers = |name: &str| {
        response.authenticate.iter().any(|a| {
            a.split([' ', ','])
                .next()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
        })
    };
    if offers("Basic") {
        Ok(Scheme::Basic)
    } else if offers("NTLM") {
        Ok(Scheme::Ntlm("NTLM"))
    } else if offers("Negotiate") {
        Ok(Scheme::Ntlm("Negotiate"))
    } else {
        Err(ProviderError::Rejected(format!(
            "the server takes no login we know: {}",
            response.authenticate.join("; ")
        )))
    }
}

/// `N` random bytes.
pub(crate) fn random<const N: usize>() -> Result<[u8; N], ProviderError> {
    let mut bytes = [0; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| ProviderError::Protocol("no random numbers".into()))?;
    Ok(bytes)
}

/// Now as a Windows FILETIME: 100 ns steps since 1601.
fn filetime() -> u64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (since.as_secs() + 11_644_473_600) * 10_000_000 + u64::from(since.subsec_nanos() / 100)
}
