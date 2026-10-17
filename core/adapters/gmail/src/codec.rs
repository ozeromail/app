//! The encodings Google's APIs use: base64url and URL percent-encoding.

use ozero_domain::base64;

/// Base64url without padding (RFC 4648 §5), as PKCE and the Gmail `raw` field use it.
pub fn base64url(bytes: &[u8]) -> String {
    base64(bytes)
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            c => c,
        })
        .collect()
}

/// Decodes base64url or standard base64, with or without padding. None on other characters.
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0u32);
    for c in text.bytes().filter(|c| *c != b'=') {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        bits = bits << 6 | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    Some(out)
}

/// Percent-encodes everything but the unreserved characters (RFC 3986), for query strings
/// and form bodies.
pub fn percent(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Pairs joined as `a=1&b=2`, each part percent-encoded.
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", percent(k), percent(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The value of `key` in a URL's query, decoded; `+` is a space.
pub fn query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or_default();
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (unpercent(k) == key).then(|| unpercent(v))
    })
}

fn unpercent(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_base64url() {
        for len in 0..20 {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 250) as u8).collect();
            let text = base64url(&bytes);
            assert!(!text.contains(['+', '/', '=']));
            assert_eq!(decode_base64(&text).unwrap(), bytes);
        }
        assert_eq!(decode_base64("//4=").unwrap(), [0xff, 0xfe]);
        assert_eq!(decode_base64("a b"), None);
    }

    #[test]
    fn encodes_and_reads_queries() {
        assert_eq!(percent("a b/ä~"), "a%20b%2F%C3%A4~");
        assert_eq!(form(&[("k", "v w"), ("x", "=")]), "k=v%20w&x=%3D");
        let url = "app:/r?state=s%2B1&code=4%2F0A+b&empty#frag";
        assert_eq!(query_value(url, "code").as_deref(), Some("4/0A b"));
        assert_eq!(query_value(url, "state").as_deref(), Some("s+1"));
        assert_eq!(query_value(url, "empty").as_deref(), Some(""));
        assert_eq!(query_value(url, "frag"), None);
        assert_eq!(query_value("app:/r", "code"), None);
    }
}
