//! TLS through the platform's own stack, for a client certificate that a server asks for by
//! renegotiating after the request, which rustls does not do. The core still checks the
//! server's certificate: the platform asks `ServerChecks`.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use ozero_domain::ProviderError;
use ozero_http::{Opened, Stream, Tunnel};
use ozero_imap::rustls::client::danger::ServerCertVerifier;
use ozero_imap::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

/// Bytes moved between the platform and the core at a time.
const CHUNK: usize = 16 * 1024;

/// The platform's TLS, such as Android's: it renegotiates when a server asks.
#[uniffi::export(with_foreign)]
pub trait PlatformTls: Send + Sync {
    /// A connection to `host:port` that offers the client certificate `certificate` and takes
    /// the server only if `checks` passes its certificate chain. Blocks until it is open.
    fn connect(
        &self,
        host: String,
        port: u16,
        certificate: String,
        checks: Arc<ServerChecks>,
    ) -> Result<Arc<dyn PlatformSocket>, PlatformTlsError>;
}

/// An open connection of `PlatformTls`. Its calls block.
#[uniffi::export(with_foreign)]
pub trait PlatformSocket: Send + Sync {
    /// Up to `max` bytes; empty at the end of the stream.
    fn read(&self, max: u32) -> Result<Vec<u8>, PlatformTlsError>;
    fn write(&self, data: Vec<u8>) -> Result<(), PlatformTlsError>;
    /// Ends the connection; a blocked `read` returns.
    fn close(&self);
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
pub enum PlatformTlsError {
    Failed { detail: String },
}

impl fmt::Display for PlatformTlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed { detail } => f.write_str(detail),
        }
    }
}

impl std::error::Error for PlatformTlsError {}

impl From<uniffi::UnexpectedUniFFICallbackError> for PlatformTlsError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { detail: e.reason }
    }
}

/// The core's checks of one server's certificate, for the platform to ask during handshakes.
#[derive(uniffi::Object)]
pub struct ServerChecks {
    checks: Arc<dyn ServerCertVerifier>,
    host: ServerName<'static>,
    /// The last certificate that passed, as DER.
    passed: Mutex<Option<Vec<u8>>>,
}

impl fmt::Debug for ServerChecks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ServerChecks({:?})", self.host)
    }
}

#[uniffi::export]
impl ServerChecks {
    /// Whether the server's certificate chain, leaf first as DER, passes.
    pub fn check(&self, chain: Vec<Vec<u8>>) -> bool {
        let Some((leaf, rest)) = chain.split_first() else {
            return false;
        };
        let intermediates: Vec<_> = rest.iter().map(|c| CertificateDer::from(&c[..])).collect();
        let checked = self.checks.verify_server_cert(
            &CertificateDer::from(&leaf[..]),
            &intermediates,
            &self.host,
            &[],
            UnixTime::now(),
        );
        if let Err(e) = &checked {
            log::warn!("server certificate refused: {e}");
            return false;
        }
        *self.passed.lock().unwrap_or_else(|p| p.into_inner()) = Some(leaf.clone());
        true
    }
}

/// Opens connections through `tls` that offer `certificate` and check servers with `checks`.
pub struct Platform {
    pub tls: Arc<dyn PlatformTls>,
    pub certificate: String,
    pub checks: Arc<dyn ServerCertVerifier>,
}

impl fmt::Debug for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Platform({})", self.certificate)
    }
}

impl Tunnel for Platform {
    fn open<'a>(
        &'a self,
        host: &'a str,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<Opened, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            let name = ServerName::try_from(host.to_owned())
                .map_err(|_| ProviderError::Network(format!("bad host name {host}")))?;
            let checks = Arc::new(ServerChecks {
                checks: self.checks.clone(),
                host: name,
                passed: Mutex::default(),
            });
            let (tls, host, certificate) =
                (self.tls.clone(), host.to_owned(), self.certificate.clone());
            let asked = checks.clone();
            let socket = blocking(move || tls.connect(host, port, certificate, asked)).await?;
            let passed = checks
                .passed
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take();
            let Some(certificate) = passed else {
                socket.close();
                return Err(ProviderError::Network("no server certificate".into()));
            };
            Ok((Box::new(pump(socket)) as Box<dyn Stream>, certificate))
        })
    }
}

/// Runs a blocking call of the platform off the async threads.
async fn blocking<T: Send + 'static>(
    call: impl FnOnce() -> Result<T, PlatformTlsError> + Send + 'static,
) -> Result<T, ProviderError> {
    match tokio::task::spawn_blocking(call).await {
        Ok(result) => result.map_err(|e| ProviderError::Network(e.to_string())),
        Err(e) => Err(ProviderError::Network(e.to_string())),
    }
}

/// The socket as an async stream: one blocking thread moves what it reads in, another what is
/// written out. Dropping the stream closes the socket.
fn pump(socket: Arc<dyn PlatformSocket>) -> DuplexStream {
    let (ours, theirs) = tokio::io::duplex(4 * CHUNK);
    let (mut from_core, mut to_core) = tokio::io::split(theirs);
    let runtime = tokio::runtime::Handle::current();
    let reader = socket.clone();
    let reading = runtime.clone();
    tokio::task::spawn_blocking(move || {
        while let Ok(data) = reader.read(CHUNK as u32) {
            if data.is_empty() || reading.block_on(to_core.write_all(&data)).is_err() {
                break;
            }
        }
        // The core reads the end of the stream.
        let _ = reading.block_on(to_core.shutdown());
    });
    tokio::task::spawn_blocking(move || {
        let mut chunk = vec![0; CHUNK];
        while let Ok(n @ 1..) = runtime.block_on(from_core.read(&mut chunk)) {
            if socket.write(chunk[..n].to_vec()).is_err() {
                break;
            }
        }
        socket.close();
    });
    ours
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Condvar;

    use ozero_imap::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified};
    use ozero_imap::rustls::{CertificateError, DigitallySignedStruct, Error, SignatureScheme};

    use super::*;

    /// Takes only the certificate `good`.
    #[derive(Debug)]
    struct OnlyGood;

    impl ServerCertVerifier for OnlyGood {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            name: &ServerName<'_>,
            _: &[u8],
            _: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            assert_eq!(name.to_str(), "mail.example.com");
            if end_entity.as_ref() == b"good" {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(Error::InvalidCertificate(CertificateError::Revoked))
            }
        }

        fn verify_tls12_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            unreachable!()
        }

        fn verify_tls13_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            unreachable!()
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![]
        }
    }

    /// A server that shows `certificate` and answers each write with it reversed.
    struct Server {
        certificate: &'static [u8],
        socket: Arc<Echo>,
    }

    #[derive(Default)]
    struct Echo {
        state: Mutex<(VecDeque<Vec<u8>>, bool)>,
        changed: Condvar,
    }

    impl PlatformTls for Server {
        fn connect(
            &self,
            host: String,
            port: u16,
            certificate: String,
            checks: Arc<ServerChecks>,
        ) -> Result<Arc<dyn PlatformSocket>, PlatformTlsError> {
            assert_eq!(
                (&*host, port, &*certificate),
                ("mail.example.com", 443, "T-Bank")
            );
            if !checks.check(vec![self.certificate.to_vec(), b"issuer".to_vec()]) {
                return Err(PlatformTlsError::Failed {
                    detail: "handshake failed".into(),
                });
            }
            Ok(self.socket.clone())
        }
    }

    impl PlatformSocket for Echo {
        fn read(&self, _: u32) -> Result<Vec<u8>, PlatformTlsError> {
            let mut state = self.state.lock().unwrap();
            loop {
                if let Some(data) = state.0.pop_front() {
                    return Ok(data);
                }
                if state.1 {
                    return Ok(vec![]);
                }
                state = self.changed.wait(state).unwrap();
            }
        }

        fn write(&self, mut data: Vec<u8>) -> Result<(), PlatformTlsError> {
            data.reverse();
            self.state.lock().unwrap().0.push_back(data);
            self.changed.notify_all();
            Ok(())
        }

        fn close(&self) {
            self.state.lock().unwrap().1 = true;
            self.changed.notify_all();
        }
    }

    fn platform(certificate: &'static [u8]) -> (Platform, Arc<Echo>) {
        let socket = Arc::new(Echo::default());
        let tls = Arc::new(Server {
            certificate,
            socket: socket.clone(),
        });
        let platform = Platform {
            tls,
            certificate: "T-Bank".into(),
            checks: Arc::new(OnlyGood),
        };
        (platform, socket)
    }

    #[tokio::test]
    async fn carries_bytes_over_the_platform_and_closes_with_the_stream() {
        let (platform, socket) = platform(b"good");
        let (mut stream, certificate) = platform.open("mail.example.com", 443).await.unwrap();
        assert_eq!(certificate, b"good");
        stream.write_all(b"ping").await.unwrap();
        let mut answer = [0; 4];
        stream.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"gnip");
        drop(stream);
        for _ in 0..100 {
            if socket.state.lock().unwrap().1 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("the socket stayed open");
    }

    #[tokio::test]
    async fn opens_nothing_to_a_server_the_checks_refuse() {
        let (platform, _) = platform(b"revoked");
        let opened = platform.open("mail.example.com", 443).await;
        assert!(matches!(opened, Err(ProviderError::Network(d)) if d == "handshake failed"));
    }
}
