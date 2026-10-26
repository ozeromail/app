//! NTLMv2 (MS-NLMP) as HTTP authentication uses it, with the TLS channel binding Exchange's
//! Extended Protection asks for. No signing or sealing: TLS protects the connection.

use ring::digest::{Algorithm, SHA256, SHA384, SHA512, digest};

use crate::md::{hmac_md5, md4, md5};

const SIGNATURE: &[u8; 8] = b"NTLMSSP\0";
const UNICODE: u32 = 0x0000_0001;
const REQUEST_TARGET: u32 = 0x0000_0004;
const NTLM: u32 = 0x0000_0200;
const ALWAYS_SIGN: u32 = 0x0000_8000;
const EXTENDED_SECURITY: u32 = 0x0008_0000;
const TARGET_INFO: u32 = 0x0080_0000;
const KEY_128: u32 = 0x2000_0000;
const KEY_56: u32 = 0x8000_0000;
const FLAGS: u32 = UNICODE
    | REQUEST_TARGET
    | NTLM
    | ALWAYS_SIGN
    | EXTENDED_SECURITY
    | TARGET_INFO
    | KEY_128
    | KEY_56;

/// Attribute ids of the target information.
const AV_EOL: u16 = 0;
const AV_TIMESTAMP: u16 = 7;
const AV_CHANNEL_BINDINGS: u16 = 10;

/// The first message: what the client can do.
pub fn negotiate() -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&FLAGS.to_le_bytes());
    // No domain or workstation named.
    out.extend_from_slice(&[0; 16]);
    out
}

/// What the server's challenge carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub nonce: [u8; 8],
    pub flags: u32,
    pub target_info: Vec<u8>,
}

/// Reads the server's challenge message.
pub fn challenge(message: &[u8]) -> Option<Challenge> {
    if message.get(..8)? != SIGNATURE || u32le(message, 8)? != 2 {
        return None;
    }
    let flags = u32le(message, 20)?;
    let nonce = message.get(24..32)?.try_into().ok()?;
    let target_info = if flags & TARGET_INFO != 0 {
        field(message, 40)?.to_vec()
    } else {
        Vec::new()
    };
    Some(Challenge {
        nonce,
        flags,
        target_info,
    })
}

/// The last message, answering `challenge` for `login` (`DOMAIN\user`, or a name the server
/// knows alone, such as `user@domain`). `binding` is `channel_binding` of the server's
/// certificate; `client` a random nonce; `now` the time as a Windows FILETIME.
pub fn authenticate(
    challenge: &Challenge,
    login: &str,
    password: &str,
    binding: Option<[u8; 16]>,
    client: [u8; 8],
    now: u64,
) -> Vec<u8> {
    let (domain, user) = login.split_once('\\').unwrap_or(("", login));
    let key = ntowf_v2(user, domain, password);
    let pairs = pairs(&challenge.target_info);
    let timestamp = pairs
        .iter()
        .find(|(id, v)| *id == AV_TIMESTAMP && v.len() == 8)
        .and_then(|(_, v)| v.as_slice().try_into().ok())
        .map(u64::from_le_bytes);
    let mut info = Vec::new();
    for (id, value) in pairs
        .iter()
        .filter(|(id, _)| *id != AV_EOL && *id != AV_CHANNEL_BINDINGS)
    {
        push_pair(&mut info, *id, value);
    }
    if let Some(hash) = binding {
        push_pair(&mut info, AV_CHANNEL_BINDINGS, &hash);
    }
    push_pair(&mut info, AV_EOL, &[]);
    let nt = nt_response(
        &key,
        &challenge.nonce,
        &client,
        timestamp.unwrap_or(now),
        &info,
    );
    // With a server time the LM answer must be empty zeros (MS-NLMP 3.1.5.1.2).
    let lm = match timestamp {
        Some(_) => vec![0; 24],
        None => [
            &hmac_md5(&key, &[challenge.nonce, client].concat())[..],
            &client,
        ]
        .concat(),
    };
    let flags = (challenge.flags & FLAGS) | UNICODE | NTLM;
    let fields = [lm, nt, utf16(domain), utf16(user), Vec::new(), Vec::new()];
    let mut head = SIGNATURE.to_vec();
    head.extend_from_slice(&3u32.to_le_bytes());
    let mut payload = Vec::new();
    let mut offset = 64u32;
    for f in &fields {
        let len = f.len() as u16;
        head.extend_from_slice(&len.to_le_bytes());
        head.extend_from_slice(&len.to_le_bytes());
        head.extend_from_slice(&offset.to_le_bytes());
        offset += u32::from(len);
        payload.extend_from_slice(f);
    }
    head.extend_from_slice(&flags.to_le_bytes());
    head.extend(payload);
    head
}

/// The hash of the TLS channel binding of a server certificate (RFC 5929
/// `tls-server-end-point`), as MsvAvChannelBindings carries it.
pub fn channel_binding(certificate: &[u8]) -> [u8; 16] {
    let hash = digest(signature_hash(certificate), certificate);
    let data = [b"tls-server-end-point:", hash.as_ref()].concat();
    // gss_channel_bindings_struct: no addresses, then the application data.
    let mut bindings = vec![0u8; 16];
    bindings.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bindings.extend(data);
    md5(&bindings)
}

fn ntowf_v2(user: &str, domain: &str, password: &str) -> [u8; 16] {
    let identity = utf16(&format!("{}{domain}", user.to_uppercase()));
    hmac_md5(&md4(&utf16(password)), &identity)
}

fn nt_response(
    key: &[u8; 16],
    nonce: &[u8; 8],
    client: &[u8; 8],
    time: u64,
    info: &[u8],
) -> Vec<u8> {
    let mut blob = vec![1, 1, 0, 0, 0, 0, 0, 0];
    blob.extend_from_slice(&time.to_le_bytes());
    blob.extend_from_slice(client);
    blob.extend_from_slice(&[0; 4]);
    blob.extend_from_slice(info);
    blob.extend_from_slice(&[0; 4]);
    let proof = hmac_md5(key, &[&nonce[..], &blob].concat());
    [&proof[..], &blob].concat()
}

/// The attribute-value pairs of target information, up to its end.
fn pairs(info: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let mut out = Vec::new();
    let mut rest = info;
    while let (Some(id), Some(len)) = (u16le(rest, 0), u16le(rest, 2)) {
        let Some(value) = rest.get(4..4 + usize::from(len)) else {
            break;
        };
        if id == AV_EOL {
            break;
        }
        out.push((id, value.to_vec()));
        rest = &rest[4 + usize::from(len)..];
    }
    out
}

fn push_pair(out: &mut Vec<u8>, id: u16, value: &[u8]) {
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&(value.len() as u16).to_le_bytes());
    out.extend_from_slice(value);
}

/// The bytes a security buffer at `at` points to.
fn field(message: &[u8], at: usize) -> Option<&[u8]> {
    let len = usize::from(u16le(message, at)?);
    let offset = u32le(message, at + 4)? as usize;
    message.get(offset..offset.checked_add(len)?)
}

fn u16le(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32le(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The hash `tls-server-end-point` takes for a certificate: that of its own signature
/// algorithm, but SHA-256 for MD5, SHA-1 and SHA-256.
fn signature_hash(certificate: &[u8]) -> &'static Algorithm {
    // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signature }
    let oid = der(certificate, 0x30)
        .and_then(|(_, cert)| der_any(cert))
        .and_then(|(rest, _)| der(rest, 0x30))
        .and_then(|(_, algorithm)| der(algorithm, 0x06))
        .map(|(_, oid)| oid);
    match oid {
        // sha384WithRSAEncryption, ecdsa-with-SHA384
        Some(
            [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0c]
            | [0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x03],
        ) => &SHA384,
        // sha512WithRSAEncryption, ecdsa-with-SHA512
        Some(
            [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0d]
            | [0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x04],
        ) => &SHA512,
        _ => &SHA256,
    }
}

/// A DER value with tag `tag` at the start of `data`: what follows it, and its contents.
fn der(data: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    if *data.first()? != tag {
        return None;
    }
    der_any(data)
}

fn der_any(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let first = *data.get(1)?;
    let (len, start) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 {
            return None;
        }
        let len = data
            .get(2..2 + n)?
            .iter()
            .fold(0usize, |l, b| l << 8 | usize::from(*b));
        (len, 2 + n)
    };
    let end = start.checked_add(len)?;
    Some((data.get(end..)?, data.get(start..end)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    /// MS-NLMP 4.2.4: NTLMv2 with User, Domain and Password.
    #[test]
    fn matches_the_ntlm_v2_example() {
        let key = ntowf_v2("User", "Domain", "Password");
        assert_eq!(key.to_vec(), hex("0c868a403bfd7a93a3001ef22ef02e3f"));
        let info =
            hex("02000c0044006f006d00610069006e0001000c005300650072007600650072000000000000000000");
        // The example's target info is followed by four zeros; ours adds them itself.
        let info = &info[..info.len() - 4];
        let nonce = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
        let nt = nt_response(&key, &nonce, &[0xaa; 8], 0, info);
        assert_eq!(nt[..16].to_vec(), hex("68cd0ab851e51c96aabc927bebef6a1c"));
        let lm = hmac_md5(&key, &[nonce, [0xaa; 8]].concat());
        assert_eq!(lm.to_vec(), hex("86c35097ac9cec102554764a57cccc19"));
    }

    #[test]
    fn answers_a_challenge() {
        let mut message = SIGNATURE.to_vec();
        message.extend_from_slice(&2u32.to_le_bytes());
        message.extend_from_slice(&[0; 8]);
        message.extend_from_slice(&(FLAGS | 0x0100_0000).to_le_bytes());
        message.extend_from_slice(&[7; 8]);
        message.extend_from_slice(&[0; 8]);
        let mut info = Vec::new();
        push_pair(&mut info, 2, &utf16("CORP"));
        push_pair(&mut info, AV_TIMESTAMP, &5u64.to_le_bytes());
        push_pair(&mut info, AV_EOL, &[]);
        message.extend_from_slice(&(info.len() as u16).to_le_bytes());
        message.extend_from_slice(&(info.len() as u16).to_le_bytes());
        message.extend_from_slice(&48u32.to_le_bytes());
        message.extend_from_slice(&info);
        let challenge = challenge(&message).unwrap();
        assert_eq!(challenge.nonce, [7; 8]);
        assert_eq!(challenge.target_info, info);

        let answer = authenticate(&challenge, "CORP\\anna", "pw", Some([9; 16]), [1; 8], 99);
        assert_eq!(&answer[..12], b"NTLMSSP\0\x03\0\0\0");
        let lm = field(&answer, 12).unwrap();
        assert_eq!(lm, [0; 24]);
        let nt = field(&answer, 20).unwrap();
        // The server's time, not ours, then the client nonce.
        assert_eq!(&nt[24..32], &5u64.to_le_bytes());
        assert_eq!(&nt[32..40], &[1; 8]);
        let tail = &nt[44..];
        assert_eq!(
            pairs(tail).last().unwrap(),
            &(AV_CHANNEL_BINDINGS, vec![9; 16])
        );
        assert_eq!(field(&answer, 28).unwrap(), utf16("CORP"));
        assert_eq!(field(&answer, 36).unwrap(), utf16("anna"));
        let key = ntowf_v2("anna", "CORP", "pw");
        let proof = hmac_md5(&key, &[&[7u8; 8][..], &nt[16..]].concat());
        assert_eq!(&nt[..16], &proof);
    }

    #[test]
    fn refuses_what_is_no_challenge() {
        assert_eq!(challenge(&negotiate()), None);
        assert_eq!(challenge(b"NTLMSSP\0\x02\0\0\0"), None);
    }

    #[test]
    fn binds_to_the_certificate() {
        // Not a certificate: hashed with SHA-256 all the same.
        let data = [b"tls-server-end-point:", digest(&SHA256, b"x").as_ref()].concat();
        let mut bindings = vec![0u8; 16];
        bindings.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bindings.extend(data);
        assert_eq!(channel_binding(b"x"), md5(&bindings));
        // SEQUENCE { SEQUENCE {}, SEQUENCE { OID ecdsa-with-SHA384 } }
        let cert = hex("300e3000300a06082a8648ce3d040303");
        assert_eq!(signature_hash(&cert), &SHA384);
    }
}
