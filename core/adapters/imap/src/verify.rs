//! Certificate checks on top of the OS verifier.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ring::digest::{SHA256, digest};
#[cfg(any(target_os = "android", test))]
use tokio_rustls::rustls::CertificateError;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{DigitallySignedStruct, Error, SignatureScheme};

/// Checks a certificate a second time when the first check calls it revoked.
///
/// Android sometimes reports a valid certificate as revoked for a while, when a
/// revocation answer it fetched could not be used. A second check usually passes;
/// a certificate that is really revoked fails both.
#[cfg(any(target_os = "android", test))]
#[derive(Debug)]
pub struct RecheckRevoked(pub Arc<dyn ServerCertVerifier>);

#[cfg(any(target_os = "android", test))]
impl ServerCertVerifier for RecheckRevoked {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let check = || {
            self.0
                .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
        };
        match check() {
            Err(revoked @ Error::InvalidCertificate(CertificateError::Revoked)) => {
                log::warn!("certificate reported revoked, checking again");
                check().map_err(|_| revoked)
            }
            result => result,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.0.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.0.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_verify_schemes()
    }
}

/// A server certificate the checks refused: what the person may choose to trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// SHA-256 of the certificate.
    pub fingerprint: [u8; 32],
    /// Why the checks refused it.
    pub reason: String,
}

/// The certificates the checks refused lately, by host, so a failed sign-in can say which.
#[derive(Debug, Default)]
pub struct RefusedCertificates(Mutex<HashMap<String, Refused>>);

impl RefusedCertificates {
    /// The certificate `host` showed last time it was refused, if any; forgotten after.
    pub fn take(&self, host: &str) -> Option<Refused> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(host)
    }

    fn note(&self, host: String, refused: Refused) {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(host, refused);
    }
}

/// The OS checks, but also takes the one certificate the person chose to trust for a server
/// although the checks refuse it. Notes any other refused certificate.
#[derive(Debug)]
pub struct Trusting {
    pub checks: Arc<dyn ServerCertVerifier>,
    pub trusted: Option<[u8; 32]>,
    pub refused: Arc<RefusedCertificates>,
}

impl ServerCertVerifier for Trusting {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let result = self.checks.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );
        let Err(Error::InvalidCertificate(reason)) = &result else {
            return result;
        };
        let fingerprint = fingerprint(end_entity);
        if self.trusted == Some(fingerprint) {
            return Ok(ServerCertVerified::assertion());
        }
        let refused = Refused {
            fingerprint,
            reason: format!("{reason:?}"),
        };
        self.refused
            .note(server_name.to_str().into_owned(), refused);
        result
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.checks.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.checks.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.checks.supported_verify_schemes()
    }
}

/// SHA-256 of a certificate.
pub fn fingerprint(certificate: &[u8]) -> [u8; 32] {
    let mut out = [0; 32];
    out.copy_from_slice(digest(&SHA256, certificate).as_ref());
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Answers with the queued results in turn.
    #[derive(Debug)]
    struct Scripted(Mutex<Vec<Result<(), CertificateError>>>);

    impl ServerCertVerifier for Scripted {
        fn verify_server_cert(
            &self,
            _: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: &ServerName<'_>,
            _: &[u8],
            _: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            let next = self.0.lock().unwrap().remove(0);
            next.map(|()| ServerCertVerified::assertion())
                .map_err(Error::InvalidCertificate)
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
            Vec::new()
        }
    }

    /// Runs one check and returns its result and the answers left unused.
    fn verify(script: Vec<Result<(), CertificateError>>) -> (Result<(), Error>, usize) {
        let inner = Arc::new(Scripted(Mutex::new(script)));
        let result = RecheckRevoked(inner.clone())
            .verify_server_cert(
                &CertificateDer::from(Vec::new()),
                &[],
                &ServerName::try_from("imap.example.com").unwrap(),
                &[],
                UnixTime::now(),
            )
            .map(|_| ());
        let left = inner.0.lock().unwrap().len();
        (result, left)
    }

    #[test]
    fn passes_when_the_second_check_does() {
        let (result, left) = verify(vec![Err(CertificateError::Revoked), Ok(())]);
        assert!(result.is_ok());
        assert_eq!(left, 0);
    }

    #[test]
    fn fails_when_both_checks_say_revoked() {
        let (result, left) = verify(vec![
            Err(CertificateError::Revoked),
            Err(CertificateError::Revoked),
            Ok(()),
        ]);
        assert_eq!(
            result,
            Err(Error::InvalidCertificate(CertificateError::Revoked))
        );
        assert_eq!(left, 1);
    }

    #[test]
    fn checks_once_on_other_errors() {
        let (result, left) = verify(vec![Err(CertificateError::Expired), Ok(())]);
        assert_eq!(
            result,
            Err(Error::InvalidCertificate(CertificateError::Expired))
        );
        assert_eq!(left, 1);
    }

    #[test]
    fn checks_once_on_success() {
        let (result, left) = verify(vec![Ok(()), Ok(())]);
        assert!(result.is_ok());
        assert_eq!(left, 1);
    }

    #[test]
    fn keeps_revoked_when_the_second_check_fails_otherwise() {
        let (result, _) = verify(vec![
            Err(CertificateError::Revoked),
            Err(CertificateError::Expired),
        ]);
        assert_eq!(
            result,
            Err(Error::InvalidCertificate(CertificateError::Revoked))
        );
    }

    /// Runs `Trusting` over one scripted check, trusting `trusted`, for certificate `[1]`.
    fn trusting(
        check: Result<(), CertificateError>,
        trusted: Option<[u8; 32]>,
    ) -> (Result<(), Error>, Option<Refused>) {
        let refused = Arc::new(RefusedCertificates::default());
        let verifier = Trusting {
            checks: Arc::new(Scripted(Mutex::new(vec![check]))),
            trusted,
            refused: refused.clone(),
        };
        let result = verifier
            .verify_server_cert(
                &CertificateDer::from(vec![1]),
                &[],
                &ServerName::try_from("mail.example.com").unwrap(),
                &[],
                UnixTime::now(),
            )
            .map(|_| ());
        (result, refused.take("mail.example.com"))
    }

    #[test]
    fn takes_the_trusted_certificate_the_checks_refuse() {
        let (result, refused) = trusting(Err(CertificateError::Revoked), Some(fingerprint(&[1])));
        assert!(result.is_ok());
        assert_eq!(refused, None);
    }

    #[test]
    fn notes_a_refused_certificate_that_is_not_trusted() {
        let (result, refused) = trusting(Err(CertificateError::Revoked), Some(fingerprint(&[2])));
        assert_eq!(
            result,
            Err(Error::InvalidCertificate(CertificateError::Revoked))
        );
        let refused = refused.unwrap();
        assert_eq!(refused.fingerprint, fingerprint(&[1]));
        assert_eq!(refused.reason, "Revoked");
    }

    #[test]
    fn passes_a_good_certificate_as_is() {
        let (result, refused) = trusting(Ok(()), None);
        assert!(result.is_ok());
        assert_eq!(refused, None);
    }
}
