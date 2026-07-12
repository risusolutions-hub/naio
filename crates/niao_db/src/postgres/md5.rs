//! Minimal MD5 for PostgreSQL auth.

pub fn md5_hex(data: &[u8]) -> String {
    let mut a: u32 = 0x67452301;
    let mut b: u32 = 0xefcdab89;
    let mut c: u32 = 0x98badcfe;
    let mut d: u32 = 0x10325476;
    let mut buf = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    buf.push(0x80);
    while (buf.len() % 64) != 56 {
        buf.push(0);
    }
    buf.extend_from_slice(&bit_len.to_le_bytes());
    for chunk in buf.chunks(64) {
        let mut m = [0u32; 16];
        for (i, word) in chunk.chunks(4).enumerate() {
            m[i] = u32::from_le_bytes(word.try_into().unwrap());
        }
        let (aa, bb, cc, dd) = transform(a, b, c, d, &m);
        a = aa.wrapping_add(a);
        b = bb.wrapping_add(b);
        c = cc.wrapping_add(c);
        d = dd.wrapping_add(d);
    }
    let out = [a.to_le_bytes(), b.to_le_bytes(), c.to_le_bytes(), d.to_le_bytes()].concat();
    niao_codec::hex::encode(&out)
}

fn transform(a: u32, b: u32, c: u32, d: u32, m: &[u32; 16]) -> (u32, u32, u32, u32) {
    let mut aa = a;
    let mut bb = b;
    let mut cc = c;
    let mut dd = d;
    macro_rules! rnd {
        ($f:expr, $x:expr, $s:expr) => {{
            aa = bb.wrapping_add(
                aa.wrapping_add(($f).wrapping_add($x)).rotate_left($s),
            );
            let t = dd;
            dd = cc;
            cc = bb;
            bb = t;
        }};
    }
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21,
        6, 10, 15, 21, 6, 10, 15, 21,
    ];
    for i in 0..64 {
        let (f, g) = match i {
            0..=15 => ((bb & cc) | (!bb & dd), i),
            16..=31 => ((dd & bb) | (!dd & cc), (5 * i + 1) % 16),
            32..=47 => (bb ^ cc ^ dd, (3 * i + 5) % 16),
            _ => (cc ^ (bb | !dd), (7 * i) % 16),
        };
        rnd!(f, dd.wrapping_add(K[i]).wrapping_add(m[g as usize]), S[i]);
    }
    (aa, bb, cc, dd)
}

pub fn pg_md5_password(user: &str, password: &str, salt: &[u8]) -> String {
    let inner = md5_hex(format!("{password}{user}").as_bytes());
    let inner_bytes = niao_codec::hex::decode(&inner).unwrap_or_default();
    let mut buf = inner_bytes;
    buf.extend_from_slice(salt);
    format!("md5{}", md5_hex(&buf))
}
