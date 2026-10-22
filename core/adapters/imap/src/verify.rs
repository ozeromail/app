//! Certificate checks on top of the OS verifier.

use std::sync::Arc;

use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{CertificateError, DigitallySignedStruct, Error, SignatureScheme};

/// Checks a certificate a second time when the first check calls it revoked.
///
/// Android sometimes reports a valid certificate as revoked for a while, when a
/// revocation answer it fetched could not be used. A second check usually passes;
/// a certificate that is really revoked fails both.
#[derive(Debug)]
pub struct RecheckRevoked(pub Arc<dyn ServerCertVerifier>);

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
}
