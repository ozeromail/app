//! Modified UTF-7 of IMAP mailbox names (RFC 3501, section 5.1.3).

/// Decodes a mailbox name; `None` if it is not valid modified UTF-7.
pub fn decode(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let shifted = &rest[start + 1..];
        let end = shifted.find('-')?;
        if end == 0 {
            out.push('&');
        } else {
            for c in char::decode_utf16(utf16_units(&shifted[..end])?) {
                out.push(c.ok()?);
            }
        }
        rest = &shifted[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Base64 with `,` for `/` and no padding, read as UTF-16BE.
fn utf16_units(b64: &str) -> Option<Vec<u16>> {
    let mut units = Vec::with_capacity(b64.len() * 3 / 8);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in b64.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b',' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 16 {
            bits -= 16;
            units.push((acc >> bits) as u16);
            acc &= (1 << bits) - 1;
        }
    }
    // Only zero padding may be left over: less than one base64 digit, all zeros.
    (acc == 0 && bits < 6).then_some(units)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_rfc_and_real_names() {
        assert_eq!(decode("INBOX").as_deref(), Some("INBOX"));
        assert_eq!(decode("Tom &- Jerry").as_deref(), Some("Tom & Jerry"));
        assert_eq!(
            decode("~peter/mail/&U,BTFw-/&ZeVnLIqe-").as_deref(),
            Some("~peter/mail/台北/日本語")
        );
        assert_eq!(decode("&BB4EQgRHBFEEQgRL-").as_deref(), Some("Отчёты"));
        assert_eq!(decode("&2D3eAA-").as_deref(), Some("😀"));
    }

    #[test]
    fn rejects_broken_names() {
        assert_eq!(decode("&BB4"), None);
        assert_eq!(decode("&!-"), None);
        assert_eq!(decode("&2D0-"), None);
    }
}
