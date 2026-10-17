//! Fetching images named in messages: from anyone, so with more care than the API calls.

use std::net::IpAddr;
use std::sync::Arc;

use ozero_domain::ProviderError;
use tokio_rustls::rustls::ClientConfig;

use crate::{HttpsClient, Response};

/// Largest image loaded.
pub const MAX_IMAGE: usize = 4 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;

/// Downloads an `https` image: on a public address only, following a few redirects, sending
/// no cookies or anything else that tells who asks. Fails on anything but an image.
pub async fn image(url: &str, tls: Arc<ClientConfig>) -> Result<Response, ProviderError> {
    let (mut host, mut port, mut path) = split(url)?;
    for _ in 0..=MAX_REDIRECTS {
        let mut client = HttpsClient::untrusted(&host, port, tls.clone(), MAX_IMAGE);
        let response = client
            .send("GET", &path, &[("Accept", "image/*")], None)
            .await?;
        if !(300..400).contains(&response.status) {
            let image = response
                .content_type
                .to_ascii_lowercase()
                .starts_with("image/");
            return if response.ok() && image {
                Ok(response)
            } else {
                Err(bad(format!("not an image: {}", response.status)))
            };
        }
        let next = &response.location;
        if next.starts_with('/') && !next.starts_with("//") {
            path = next.clone();
        } else {
            let next = next.strip_prefix("//").map(|n| format!("https://{n}"));
            (host, port, path) = split(next.as_deref().unwrap_or(&response.location))?;
        }
    }
    Err(bad("too many redirects"))
}

/// Host, port and path of an `https` address, without its fragment.
fn split(url: &str) -> Result<(String, u16, String), ProviderError> {
    let rest = url
        .get(..8)
        .filter(|s| s.eq_ignore_ascii_case("https://"))
        .map(|_| &url[8..])
        .ok_or_else(|| bad("not https"))?;
    let rest = rest.split('#').next().unwrap_or_default();
    let (authority, path) = match rest.find(['/', '?']) {
        Some(at) if rest.as_bytes()[at] == b'?' => (&rest[..at], format!("/{}", &rest[at..])),
        Some(at) => (&rest[..at], rest[at..].to_owned()),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().map_err(|_| bad("bad port"))?),
        None => (authority, 443),
    };
    let named = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    if host.is_empty() || !named || path.contains(char::is_whitespace) {
        return Err(bad("bad address"));
    }
    Ok((host.to_ascii_lowercase(), port, path))
}

/// Not loopback, private, link-local or otherwise inside a network.
pub(crate) fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || a == 0
                // Carrier-grade NAT, 100.64.0.0/10.
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                // Unique local fc00::/7 and link-local fe80::/10.
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80)
        }
    }
}

fn bad(msg: impl Into<String>) -> ProviderError {
    ProviderError::Protocol(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_addresses() {
        assert_eq!(
            split("HTTPS://Img.Example.com/a/b.png?x=1#top").unwrap(),
            ("img.example.com".into(), 443, "/a/b.png?x=1".into())
        );
        assert_eq!(
            split("https://x.com:8443?q").unwrap(),
            ("x.com".into(), 8443, "/?q".into())
        );
        for url in [
            "http://x.com/",
            "https://",
            "https://a b/",
            "https://[::1]/",
            "https://u@x/",
        ] {
            assert!(split(url).is_err(), "{url}");
        }
    }

    #[test]
    fn tells_public_addresses() {
        for ip in [
            "10.0.0.1",
            "127.0.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.0.1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["93.184.216.34", "2606:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }
}
