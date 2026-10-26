//! MD4 (RFC 1320) and MD5 (RFC 1321) with HMAC-MD5 (RFC 2104): broken as hashes, but what
//! NTLM is built on. Nothing else may use them.

/// The message padded as both hashes pad it: a 1 bit, zeros, then the bit length.
fn padded(data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    out.push(0x80);
    while out.len() % 64 != 56 {
        out.push(0);
    }
    out.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_le_bytes());
    out
}

fn words(block: &[u8]) -> [u32; 16] {
    let mut x = [0u32; 16];
    for (i, w) in block.chunks_exact(4).enumerate() {
        x[i] = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
    }
    x
}

fn digest(state: [u32; 4]) -> [u8; 16] {
    let mut out = [0; 16];
    for (i, s) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&s.to_le_bytes());
    }
    out
}

const INIT: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];

pub fn md4(data: &[u8]) -> [u8; 16] {
    let mut state = INIT;
    for block in padded(data).chunks_exact(64) {
        let x = words(block);
        let [mut a, mut b, mut c, mut d] = state;
        let f = |x: u32, y: u32, z: u32| (x & y) | (!x & z);
        let g = |x: u32, y: u32, z: u32| (x & y) | (x & z) | (y & z);
        let h = |x: u32, y: u32, z: u32| x ^ y ^ z;
        for i in [0, 4, 8, 12] {
            a = a.wrapping_add(f(b, c, d)).wrapping_add(x[i]).rotate_left(3);
            d = d
                .wrapping_add(f(a, b, c))
                .wrapping_add(x[i + 1])
                .rotate_left(7);
            c = c
                .wrapping_add(f(d, a, b))
                .wrapping_add(x[i + 2])
                .rotate_left(11);
            b = b
                .wrapping_add(f(c, d, a))
                .wrapping_add(x[i + 3])
                .rotate_left(19);
        }
        for i in [0, 1, 2, 3] {
            let k = 0x5a82_7999u32;
            a = a
                .wrapping_add(g(b, c, d))
                .wrapping_add(x[i])
                .wrapping_add(k)
                .rotate_left(3);
            d = d
                .wrapping_add(g(a, b, c))
                .wrapping_add(x[i + 4])
                .wrapping_add(k)
                .rotate_left(5);
            c = c
                .wrapping_add(g(d, a, b))
                .wrapping_add(x[i + 8])
                .wrapping_add(k)
                .rotate_left(9);
            b = b
                .wrapping_add(g(c, d, a))
                .wrapping_add(x[i + 12])
                .wrapping_add(k)
                .rotate_left(13);
        }
        for i in [0, 2, 1, 3] {
            let k = 0x6ed9_eba1u32;
            a = a
                .wrapping_add(h(b, c, d))
                .wrapping_add(x[i])
                .wrapping_add(k)
                .rotate_left(3);
            d = d
                .wrapping_add(h(a, b, c))
                .wrapping_add(x[i + 8])
                .wrapping_add(k)
                .rotate_left(9);
            c = c
                .wrapping_add(h(d, a, b))
                .wrapping_add(x[i + 4])
                .wrapping_add(k)
                .rotate_left(11);
            b = b
                .wrapping_add(h(c, d, a))
                .wrapping_add(x[i + 12])
                .wrapping_add(k)
                .rotate_left(15);
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d]) {
            *s = s.wrapping_add(v);
        }
    }
    digest(state)
}

/// Per-round shifts of MD5.
const SHIFTS: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];

pub fn md5(data: &[u8]) -> [u8; 16] {
    // floor(abs(sin(i + 1)) * 2^32), as RFC 1321 lists them.
    let k: [u32; 64] =
        std::array::from_fn(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32);
    let mut state = INIT;
    for block in padded(data).chunks_exact(64) {
        let x = words(block);
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let shift = SHIFTS[(i / 16) * 4 + i % 4];
            let sum = a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(x[g]);
            (a, d, c) = (d, c, b);
            b = b.wrapping_add(sum.rotate_left(shift));
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d]) {
            *s = s.wrapping_add(v);
        }
    }
    digest(state)
}

pub fn hmac_md5(key: &[u8], data: &[u8]) -> [u8; 16] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..16].copy_from_slice(&md5(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let inner: Vec<u8> = block
        .iter()
        .map(|b| b ^ 0x36)
        .chain(data.iter().copied())
        .collect();
    let outer: Vec<u8> = block.iter().map(|b| b ^ 0x5c).chain(md5(&inner)).collect();
    md5(&outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn matches_rfc_1320() {
        let cases = [
            ("", "31d6cfe0d16ae931b73c59d7e0c089c0"),
            ("a", "bde52cb31de33e46245e05fbdbd6fb24"),
            ("abc", "a448017aaf21d8525fc10ae87aa6729d"),
            ("message digest", "d9130a8164549fe818874806e1c7014b"),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "e33b4ddc9c38f2199c3e7b164fcc0536",
            ),
        ];
        for (text, want) in cases {
            assert_eq!(hex(&md4(text.as_bytes())), want, "{text}");
        }
    }

    #[test]
    fn matches_rfc_1321() {
        let cases = [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ];
        for (text, want) in cases {
            assert_eq!(hex(&md5(text.as_bytes())), want, "{text}");
        }
    }

    #[test]
    fn matches_rfc_2104() {
        assert_eq!(
            hex(&hmac_md5(b"Jefe", b"what do ya want for nothing?")),
            "750c783e6ab0b503eaa86e310a5db738"
        );
        assert_eq!(
            hex(&hmac_md5(
                &[0xaa; 80],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "6b1ab7fe4bd7bf8f0b62e6ce61b9d0cd"
        );
    }
}
