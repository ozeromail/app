//! Client certificates whose keys the platform keeps: the TLS handshake asks it to sign.

use std::fmt;
use std::sync::Arc;

use ozero_imap::rustls::client::ResolvesClientCert;
use ozero_imap::rustls::pki_types::{AlgorithmIdentifier, CertificateDer, alg_id};
use ozero_imap::rustls::sign::{CertifiedKey, Signer, SigningKey};
use ozero_imap::rustls::{Error, SignatureAlgorithm, SignatureScheme};

/// Client certificates the platform keeps, such as Android's KeyChain. Their keys never leave
/// it: the core only asks it to sign.
#[uniffi::export(with_foreign)]
pub trait ClientCertificates: Send + Sync {
    /// The chain of the certificate called `name`, leaf first, as DER; empty if it is gone.
    fn chain(&self, name: String) -> Vec<Vec<u8>>;
    /// `data` signed with the key of `name`; None if the platform refused.
    fn sign(&self, name: String, signing: Signing, data: Vec<u8>) -> Option<Vec<u8>>;
}

/// How to sign: the key's algorithm, padding and hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Signing {
    RsaPkcs1Sha256,
    RsaPkcs1Sha384,
    RsaPkcs1Sha512,
    RsaPssSha256,
    RsaPssSha384,
    RsaPssSha512,
    EcdsaSha256,
    EcdsaSha384,
    EcdsaSha512,
}

impl Signing {
    fn scheme(self) -> SignatureScheme {
        match self {
            Self::RsaPkcs1Sha256 => SignatureScheme::RSA_PKCS1_SHA256,
            Self::RsaPkcs1Sha384 => SignatureScheme::RSA_PKCS1_SHA384,
            Self::RsaPkcs1Sha512 => SignatureScheme::RSA_PKCS1_SHA512,
            Self::RsaPssSha256 => SignatureScheme::RSA_PSS_SHA256,
            Self::RsaPssSha384 => SignatureScheme::RSA_PSS_SHA384,
            Self::RsaPssSha512 => SignatureScheme::RSA_PSS_SHA512,
            Self::EcdsaSha256 => SignatureScheme::ECDSA_NISTP256_SHA256,
            Self::EcdsaSha384 => SignatureScheme::ECDSA_NISTP384_SHA384,
            Self::EcdsaSha512 => SignatureScheme::ECDSA_NISTP521_SHA512,
        }
    }
}

/// RSA with PKCS #1 first: TLS 1.2 servers take it, and keystores sign it more often than PSS.
const RSA: [Signing; 6] = [
    Signing::RsaPkcs1Sha256,
    Signing::RsaPssSha256,
    Signing::RsaPkcs1Sha384,
    Signing::RsaPssSha384,
    Signing::RsaPkcs1Sha512,
    Signing::RsaPssSha512,
];

/// Offers the platform's certificate `name` when a server asks for one.
pub struct Resolver {
    certificates: Arc<dyn ClientCertificates>,
    name: String,
}

impl Resolver {
    pub fn new(certificates: Arc<dyn ClientCertificates>, name: &str) -> Self {
        Self {
            certificates,
            name: name.to_owned(),
        }
    }
}

impl fmt::Debug for Resolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Resolver({})", self.name)
    }
}

impl ResolvesClientCert for Resolver {
    fn resolve(&self, _: &[&[u8]], _: &[SignatureScheme]) -> Option<Arc<CertifiedKey>> {
        let chain = self.certificates.chain(self.name.clone());
        let (algorithm, signings) = signings(chain.first()?)?;
        let key = Key {
            algorithm,
            signings,
            certificates: self.certificates.clone(),
            name: self.name.clone(),
        };
        let chain = chain.into_iter().map(CertificateDer::from).collect();
        Some(Arc::new(CertifiedKey::new(chain, Arc::new(key))))
    }

    fn has_certs(&self) -> bool {
        true
    }
}

/// The algorithm of a certificate's key and how it can sign, best first; None for a key that is
/// neither RSA nor EC on a NIST curve. An EC key signs with the hash of its curve. The key's
/// algorithm identifier is found by its bytes: the issuer's signature algorithm has other ones.
fn signings(leaf: &[u8]) -> Option<(SignatureAlgorithm, &'static [Signing])> {
    let has = |id: &AlgorithmIdentifier| leaf.windows(id.len()).any(|w| w == id.as_ref());
    Some(if has(&alg_id::RSA_ENCRYPTION) {
        (SignatureAlgorithm::RSA, &RSA)
    } else if has(&alg_id::ECDSA_P256) {
        (SignatureAlgorithm::ECDSA, &[Signing::EcdsaSha256])
    } else if has(&alg_id::ECDSA_P384) {
        (SignatureAlgorithm::ECDSA, &[Signing::EcdsaSha384])
    } else if has(&alg_id::ECDSA_P521) {
        (SignatureAlgorithm::ECDSA, &[Signing::EcdsaSha512])
    } else {
        return None;
    })
}

#[derive(Clone)]
struct Key {
    algorithm: SignatureAlgorithm,
    signings: &'static [Signing],
    certificates: Arc<dyn ClientCertificates>,
    name: String,
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key({})", self.name)
    }
}

impl SigningKey for Key {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        let signing = self
            .signings
            .iter()
            .copied()
            .find(|s| offered.contains(&s.scheme()))?;
        Some(Box::new(Sign {
            key: self.clone(),
            signing,
        }))
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        self.algorithm
    }
}

#[derive(Debug)]
struct Sign {
    key: Key,
    signing: Signing,
}

impl Signer for Sign {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        self.key
            .certificates
            .sign(self.key.name.clone(), self.signing, message.to_vec())
            .ok_or_else(|| Error::General("the platform did not sign with the certificate".into()))
    }

    fn scheme(&self) -> SignatureScheme {
        self.signing.scheme()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use ozero_imap::rustls::client::danger::HandshakeSignatureValid;
    use ozero_imap::rustls::crypto::{ring, verify_tls12_signature, verify_tls13_signature};
    use ozero_imap::rustls::pki_types::pem::PemObject;
    use ozero_imap::rustls::pki_types::{PrivateKeyDer, UnixTime};
    use ozero_imap::rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
    use ozero_imap::rustls::{
        ClientConfig, ClientConnection, DigitallySignedStruct, DistinguishedName, RootCertStore,
        ServerConfig, ServerConnection,
    };

    use super::*;

    const CA: &[u8] = include_bytes!("../../adapters/imap/tests/dovecot/ca.crt");
    const CERT: &[u8] = include_bytes!("../../adapters/imap/tests/dovecot/tls.crt");
    const KEY: &[u8] = include_bytes!("../../adapters/imap/tests/dovecot/tls.key");

    /// The test certificate, with an EC P-256 key, kept as a platform would; it signs only
    /// while `works`.
    struct Platform {
        works: bool,
        signed: Mutex<Vec<Signing>>,
    }

    impl ClientCertificates for Platform {
        fn chain(&self, name: String) -> Vec<Vec<u8>> {
            if name != "T-Bank" {
                return vec![];
            }
            vec![CertificateDer::from_pem_slice(CERT).unwrap().to_vec()]
        }

        fn sign(&self, name: String, signing: Signing, data: Vec<u8>) -> Option<Vec<u8>> {
            assert_eq!(name, "T-Bank");
            self.signed.lock().unwrap().push(signing);
            let key = ring::sign::any_supported_type(&PrivateKeyDer::from_pem_slice(KEY).unwrap())
                .unwrap();
            let signer = key.choose_scheme(&[signing.scheme()]).unwrap();
            self.works.then(|| signer.sign(&data).unwrap())
        }
    }

    /// Asks for a client certificate and takes any whose key signed the handshake.
    #[derive(Debug)]
    struct AnyClient;

    impl ClientCertVerifier for AnyClient {
        fn root_hint_subjects(&self) -> &[DistinguishedName] {
            &[]
        }

        fn verify_client_cert(
            &self,
            _: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: UnixTime,
        ) -> Result<ClientCertVerified, Error> {
            Ok(ClientCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            let algorithms = ring::default_provider().signature_verification_algorithms;
            verify_tls12_signature(message, cert, dss, &algorithms)
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            let algorithms = ring::default_provider().signature_verification_algorithms;
            verify_tls13_signature(message, cert, dss, &algorithms)
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    /// Runs a handshake in memory with `name` as the client certificate.
    fn handshake(platform: Arc<Platform>, name: &str) -> Result<(), Error> {
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from_pem_slice(CA).unwrap())
            .unwrap();
        let client = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_client_cert_resolver(Arc::new(Resolver::new(platform, name)));
        let server = ServerConfig::builder()
            .with_client_cert_verifier(Arc::new(AnyClient))
            .with_single_cert(
                vec![CertificateDer::from_pem_slice(CERT).unwrap()],
                PrivateKeyDer::from_pem_slice(KEY).unwrap(),
            )
            .unwrap();
        let mut client =
            ClientConnection::new(Arc::new(client), "localhost".try_into().unwrap()).unwrap();
        let mut server = ServerConnection::new(Arc::new(server)).unwrap();
        for _ in 0..10 {
            let mut wire = Vec::new();
            client.write_tls(&mut wire).unwrap();
            server.read_tls(&mut wire.as_slice()).unwrap();
            server.process_new_packets()?;
            wire.clear();
            server.write_tls(&mut wire).unwrap();
            client.read_tls(&mut wire.as_slice()).unwrap();
            client.process_new_packets()?;
            if !client.is_handshaking() && !server.is_handshaking() {
                assert!(server.peer_certificates().is_some());
                return Ok(());
            }
        }
        Err(Error::General("the handshake did not end".into()))
    }

    #[test]
    fn signs_the_handshake_through_the_platform() {
        let platform = Arc::new(Platform {
            works: true,
            signed: Mutex::default(),
        });
        handshake(platform.clone(), "T-Bank").unwrap();
        assert_eq!(*platform.signed.lock().unwrap(), [Signing::EcdsaSha256]);
    }

    #[test]
    fn fails_when_the_platform_does_not_sign_or_has_no_certificate() {
        let refusing = Arc::new(Platform {
            works: false,
            signed: Mutex::default(),
        });
        assert!(handshake(refusing.clone(), "T-Bank").is_err());
        assert!(handshake(refusing, "gone").is_err());
    }

    #[test]
    fn knows_the_key_from_the_certificate() {
        let leaf = CertificateDer::from_pem_slice(CERT).unwrap();
        let ec: &[Signing] = &[Signing::EcdsaSha256];
        assert_eq!(signings(&leaf), Some((SignatureAlgorithm::ECDSA, ec)));
        let rsa = [&[0x30, 0][..], alg_id::RSA_ENCRYPTION.as_ref()].concat();
        assert_eq!(signings(&rsa), Some((SignatureAlgorithm::RSA, &RSA[..])));
        assert_eq!(signings(&[0x30, 0]), None);
    }
}
