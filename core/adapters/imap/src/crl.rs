//! Revocation by the CA's certificate revocation list (CRL), for certificates that name no
//! OCSP responder, as Google's and Let's Encrypt's no longer do.
//!
//! Android's revocation check only asks OCSP: on a certificate without an OCSP address it
//! fails, and the platform verifier reports that as revoked.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, UnixTime};
use tokio_rustls::rustls::{CertificateError, Error};
use webpki::{
    CertRevocationList, EndEntityCert, ExpirationPolicy, KeyUsage, OwnedCertRevocationList,
    RevocationCheckDepth, RevocationOptionsBuilder, UnknownStatusPolicy, anchor_from_trusted_cert,
};

/// id-pe-authorityInfoAccess, 1.3.6.1.5.5.7.1.1.
const AIA: &[u8] = &[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x01, 0x01];
/// id-ad-ocsp, 1.3.6.1.5.5.7.48.1.
const OCSP: &[u8] = &[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x30, 0x01];
/// id-ce-cRLDistributionPoints, 2.5.29.31.
const CRL_POINTS: &[u8] = &[0x55, 0x1d, 0x1f];

const SEQUENCE: u8 = 0x30;
const OID: u8 = 0x06;
const OCTET_STRING: u8 = 0x04;
const EXTENSIONS: u8 = 0xa3;
/// A GeneralName that is a URI.
const URI: u8 = 0x86;

/// How long a downloaded list is used before it is asked for again.
const KEEP: Duration = Duration::from_secs(60 * 60);
const WAIT: Duration = Duration::from_secs(10);
/// Google's lists are sharded and small; this is room for a large CA's.
const MAX_CRL: u64 = 16 * 1024 * 1024;

/// Whether `certificate` names an OCSP responder.
pub fn has_ocsp(certificate: &[u8]) -> bool {
    extension(certificate, AIA)
        .and_then(tlv)
        .is_some_and(|(_, accesses, _)| {
            children(accesses, SEQUENCE)
                .into_iter()
                .any(|access| child(access, OID) == Some(OCSP))
        })
}

/// The HTTP addresses of the CRLs that cover `certificate`.
pub fn crl_urls(certificate: &[u8]) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(points) = extension(certificate, CRL_POINTS) {
        collect_uris(points, &mut urls);
    }
    urls.retain(|u| u.starts_with("http://"));
    urls
}

/// Checks `end_entity` against its CA's CRL: `Revoked` if it is listed. The CA is one of
/// `intermediates`, whose chain the platform trusts already. A list that cannot be fetched or
/// vouched for leaves the status unknown, which passes as with the platform's own soft-fail
/// check.
pub fn check(
    end_entity: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    now: UnixTime,
) -> Result<(), Error> {
    let lists: Vec<CertRevocationList<'_>> = crl_urls(end_entity)
        .iter()
        .filter_map(|url| match cached(url) {
            Ok(der) => OwnedCertRevocationList::from_der(&der)
                .map_err(|e| log::warn!("{url}: bad revocation list: {e:?}"))
                .ok()
                .map(CertRevocationList::Owned),
            Err(e) => {
                log::warn!("{url}: revocation list not fetched: {e}");
                None
            }
        })
        .collect();
    if lists.is_empty() {
        return Ok(());
    }
    verify(end_entity, intermediates, now, &lists)
}

/// Checks `end_entity` against `lists`, signed by one of `intermediates`. Only a listed
/// certificate fails: a list that is stale, foreign or signed by no known CA says nothing.
fn verify(
    end_entity: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    now: UnixTime,
    lists: &[CertRevocationList<'_>],
) -> Result<(), Error> {
    let refs: Vec<&CertRevocationList<'_>> = lists.iter().collect();
    let revocation = RevocationOptionsBuilder::new(&refs)
        .map_err(|_| Error::General("no revocation list".into()))?
        .with_depth(RevocationCheckDepth::EndEntity)
        .with_status_policy(UnknownStatusPolicy::Deny)
        .with_expiration_policy(ExpirationPolicy::Enforce)
        .build();
    // The platform trusts the issuer already; here it only vouches for the list.
    let anchors: Vec<_> = intermediates
        .iter()
        .filter_map(|c| anchor_from_trusted_cert(c).ok())
        .collect();
    let leaf = EndEntityCert::try_from(end_entity).map_err(|_| bad())?;
    let algorithms = default_provider().signature_verification_algorithms.all;
    leaf.verify_for_usage(
        algorithms,
        &anchors,
        &[],
        now,
        KeyUsage::server_auth(),
        Some(revocation),
        None,
    )
    .map_or_else(
        |e| match e {
            webpki::Error::CertRevoked => Err(Error::InvalidCertificate(CertificateError::Revoked)),
            other => {
                log::warn!("revocation status unknown: {other:?}");
                Ok(())
            }
        },
        drop,
    )
}

fn bad() -> Error {
    Error::InvalidCertificate(CertificateError::BadEncoding)
}

/// The list at `url`, downloaded at most once in `KEEP`.
fn cached(url: &str) -> Result<Vec<u8>, String> {
    /// Each list by its address, with when it came.
    type Lists = Mutex<HashMap<String, (Vec<u8>, Instant)>>;
    static LISTS: OnceLock<Lists> = OnceLock::new();
    let lists = LISTS.get_or_init(Mutex::default);
    if let Some((der, at)) = lists.lock().unwrap_or_else(|p| p.into_inner()).get(url)
        && at.elapsed() < KEEP
    {
        return Ok(der.clone());
    }
    let der = download(url)?;
    lists
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(url.to_owned(), (der.clone(), Instant::now()));
    Ok(der)
}

/// GETs a plain-HTTP `url`: a CRL is signed, so it needs no TLS, and CAs serve it so. Blocks,
/// as the platform's own revocation check does inside a handshake.
fn download(url: &str) -> Result<Vec<u8>, String> {
    let rest = url.strip_prefix("http://").ok_or("not http")?;
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let path = if path.is_empty() { "/" } else { path };
    let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
    let address = if authority.contains(':') {
        authority.to_owned()
    } else {
        format!("{authority}:80")
    };
    let addresses = address.to_socket_addrs().map_err(|e| e.to_string())?;
    let mut stream = addresses
        .into_iter()
        .find_map(|a| TcpStream::connect_timeout(&a, WAIT).ok())
        .ok_or("no connection")?;
    stream
        .set_read_timeout(Some(WAIT))
        .and_then(|()| stream.set_write_timeout(Some(WAIT)))
        .map_err(|e| e.to_string())?;
    // In one write: a server may read the request once.
    let request = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    stream
        .take(MAX_CRL)
        .read_to_end(&mut response)
        .map_err(|e| e.to_string())?;
    let end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("no header end")?;
    let status = response.split(|&b| b == b' ').nth(1).unwrap_or_default();
    if status != b"200" {
        return Err(format!("status {}", String::from_utf8_lossy(status)));
    }
    Ok(response.split_off(end + 4))
}

/// The value of the certificate extension with `oid`, inside its OCTET STRING.
fn extension<'a>(certificate: &'a [u8], oid: &[u8]) -> Option<&'a [u8]> {
    let (_, certificate, _) = tlv(certificate)?;
    let (_, tbs, _) = tlv(certificate)?;
    let mut fields = tbs;
    while let Some((tag, value, rest)) = tlv(fields) {
        if tag == EXTENSIONS {
            let (_, list, _) = tlv(value)?;
            return children(list, SEQUENCE).into_iter().find_map(|ext| {
                (child(ext, OID) == Some(oid)).then(|| child(ext, OCTET_STRING))?
            });
        }
        fields = rest;
    }
    None
}

/// Every URI GeneralName anywhere in `der`.
fn collect_uris(der: &[u8], urls: &mut Vec<String>) {
    let mut rest = der;
    while let Some((tag, value, next)) = tlv(rest) {
        if tag == URI {
            urls.extend(std::str::from_utf8(value).ok().map(str::to_owned));
        } else if tag & 0x20 != 0 {
            // Constructed: look inside.
            collect_uris(value, urls);
        }
        rest = next;
    }
}

/// The values of the elements of `der` tagged `tag`.
fn children(der: &[u8], tag: u8) -> Vec<&[u8]> {
    let mut found = Vec::new();
    let mut rest = der;
    while let Some((t, value, next)) = tlv(rest) {
        if t == tag {
            found.push(value);
        }
        rest = next;
    }
    found
}

/// The value of the first element tagged `tag` in `content`, a SEQUENCE's.
fn child(content: &[u8], tag: u8) -> Option<&[u8]> {
    children(content, tag).into_iter().next()
}

/// One DER element: its tag, its value and what follows it.
fn tlv(der: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = der.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (length, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 || rest.len() < count {
            return None;
        }
        let (bytes, rest) = rest.split_at(count);
        let length = bytes.iter().fold(0usize, |n, &b| (n << 8) | usize::from(b));
        (length, rest)
    };
    (rest.len() >= length).then(|| {
        let (value, rest) = rest.split_at(length);
        (tag, value, rest)
    })
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    const CA: &[u8] = include_bytes!("../tests/crl/ca.der");
    const GOOD: &[u8] = include_bytes!("../tests/crl/good.der");
    const REVOKED: &[u8] = include_bytes!("../tests/crl/revoked.der");
    const WITH_OCSP: &[u8] = include_bytes!("../tests/crl/ocsp.der");
    const CRL: &[u8] = include_bytes!("../tests/crl/ca.crl");

    fn check_listed(leaf: &[u8]) -> Result<(), Error> {
        let list = OwnedCertRevocationList::from_der(CRL).unwrap();
        verify(
            &CertificateDer::from(leaf),
            &[CertificateDer::from(CA)],
            UnixTime::now(),
            &[CertRevocationList::Owned(list)],
        )
    }

    #[test]
    fn reads_the_revocation_addresses() {
        assert_eq!(crl_urls(GOOD), ["http://c.example.com/ca.crl"]);
        assert!(!has_ocsp(GOOD));
        assert!(has_ocsp(WITH_OCSP));
        assert!(crl_urls(CA).is_empty());
        assert!(crl_urls(b"not a certificate").is_empty());
    }

    #[test]
    fn passes_a_certificate_its_list_leaves_out() {
        assert_eq!(check_listed(GOOD), Ok(()));
    }

    #[test]
    fn refuses_a_listed_certificate() {
        assert_eq!(
            check_listed(REVOKED),
            Err(Error::InvalidCertificate(CertificateError::Revoked))
        );
    }

    #[test]
    fn ignores_a_list_from_another_issuer() {
        let list = OwnedCertRevocationList::from_der(CRL).unwrap();
        // Listed, but by a list no known CA vouches for.
        let result = verify(
            &CertificateDer::from(REVOKED),
            &[CertificateDer::from(GOOD)],
            UnixTime::now(),
            &[CertRevocationList::Owned(list)],
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn downloads_a_list_over_http() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let n = stream.read(&mut request).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc")
                .unwrap();
            String::from_utf8_lossy(&request[..n]).into_owned()
        });
        let got = download(&format!("http://127.0.0.1:{port}/we2/x.crl")).unwrap();
        assert_eq!(got, b"abc");
        let request = served.join().unwrap();
        assert!(request.starts_with("GET /we2/x.crl HTTP/1.0\r\nHost: 127.0.0.1\r\n"));
        assert!(download("https://c.example.com/ca.crl").is_err());
    }
}
