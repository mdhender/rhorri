// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The MD5 message digest (RFC 1321), the primitive under every random draw.
//!
//! Ported from Colin Plumb's public-domain implementation in `lib/rnd.c`
//! (`xMD5Init`/`xMD5Update`/`xMD5Final`/`xMD5Transform`), which computes
//! standard MD5.

/// Per-step additive constants: `floor(abs(sin(i + 1)) * 2^32)`.
const K: [u32; 64] = [
    0xd76a_a478,
    0xe8c7_b756,
    0x2420_70db,
    0xc1bd_ceee,
    0xf57c_0faf,
    0x4787_c62a,
    0xa830_4613,
    0xfd46_9501,
    0x6980_98d8,
    0x8b44_f7af,
    0xffff_5bb1,
    0x895c_d7be,
    0x6b90_1122,
    0xfd98_7193,
    0xa679_438e,
    0x49b4_0821,
    0xf61e_2562,
    0xc040_b340,
    0x265e_5a51,
    0xe9b6_c7aa,
    0xd62f_105d,
    0x0244_1453,
    0xd8a1_e681,
    0xe7d3_fbc8,
    0x21e1_cde6,
    0xc337_07d6,
    0xf4d5_0d87,
    0x455a_14ed,
    0xa9e3_e905,
    0xfcef_a3f8,
    0x676f_02d9,
    0x8d2a_4c8a,
    0xfffa_3942,
    0x8771_f681,
    0x6d9d_6122,
    0xfde5_380c,
    0xa4be_ea44,
    0x4bde_cfa9,
    0xf6bb_4b60,
    0xbebf_bc70,
    0x289b_7ec6,
    0xeaa1_27fa,
    0xd4ef_3085,
    0x0488_1d05,
    0xd9d4_d039,
    0xe6db_99e5,
    0x1fa2_7cf8,
    0xc4ac_5665,
    0xf429_2244,
    0x432a_ff97,
    0xab94_23a7,
    0xfc93_a039,
    0x655b_59c3,
    0x8f0c_cc92,
    0xffef_f47d,
    0x8584_5dd1,
    0x6fa8_7e4f,
    0xfe2c_e6e0,
    0xa301_4314,
    0x4e08_11a1,
    0xf753_7e82,
    0xbd3a_f235,
    0x2ad7_d2bb,
    0xeb86_d391,
];

/// Left-rotate amounts, four per round.
const SHIFTS: [[u32; 4]; 4] = [
    [7, 12, 17, 22],
    [5, 9, 14, 20],
    [4, 11, 16, 23],
    [6, 10, 15, 21],
];

const INIT: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];

/// Returns the MD5 digest of `input`.
#[must_use]
pub fn digest(input: &[u8]) -> [u8; 16] {
    let mut state = INIT;

    let mut blocks = input.chunks_exact(64);
    for block in &mut blocks {
        transform(&mut state, block);
    }

    // Pad with 0x80, zeros to 56 mod 64, then the bit count, little-endian.
    let tail = blocks.remainder();
    let mut last = [0u8; 128];
    last[..tail.len()].copy_from_slice(tail);
    last[tail.len()] = 0x80;
    let padded = if tail.len() < 56 { 64 } else { 128 };
    let bits = (input.len() as u64).wrapping_mul(8);
    last[padded - 8..padded].copy_from_slice(&bits.to_le_bytes());
    for block in last[..padded].chunks_exact(64) {
        transform(&mut state, block);
    }

    let mut out = [0u8; 16];
    for (bytes, word) in out.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    out
}

/// Hashes 32-bit words the way the C engine does when it passes an array
/// of `uint32_t` to `MD5()`: each word as four little-endian bytes in, and
/// the digest read back as four little-endian words.
///
/// The C code uses the host's byte order. Little-endian matches every host
/// the fixtures are built on, so this is fixed rather than host-dependent.
#[must_use]
pub(crate) fn hash_words(words: &[u32]) -> [u32; 4] {
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let d = digest(&bytes);
    let mut out = [0u32; 4];
    for (word, chunk) in out.iter_mut().zip(d.chunks_exact(4)) {
        *word = u32::from_le_bytes(chunk.try_into().expect("chunk is 4 bytes"));
    }
    out
}

/// Folds one 64-byte block into `state`.
fn transform(state: &mut [u32; 4], block: &[u8]) {
    let mut m = [0u32; 16];
    for (word, chunk) in m.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_le_bytes(chunk.try_into().expect("chunk is 4 bytes"));
    }

    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let round = i / 16;
        let (f, g) = match round {
            0 => (d ^ (b & (c ^ d)), i),
            1 => (c ^ (d & (b ^ c)), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let sum = a
            .wrapping_add(f)
            .wrapping_add(K[i])
            .wrapping_add(m[g])
            .rotate_left(SHIFTS[round][i % 4]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(sum);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(d: [u8; 16]) -> String {
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn rfc_1321_vectors() {
        let cases = [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(hex(digest(input.as_bytes())), want, "input {input:?}");
        }
    }

    /// Lengths around the 56- and 64-byte padding boundaries, checked
    /// against the C `MD5()`.
    #[test]
    fn padding_boundaries_match_c() {
        let ramp: Vec<u8> = (0..130u32).map(|i| (i * 7 + 3) as u8).collect();
        let cases = [
            (55, "52c0e574e1198de5fe3f8f11440dcb1b"),
            (56, "46c9907fc908ee68b1e7b8e71286a518"),
            (57, "1c805dd236c35cab25fcb1bc73802c51"),
            (58, "bca54f0684135200890418305d71acac"),
            (59, "01344f4f53fcb77c6f85b7c808411ee8"),
            (60, "88cedcdc923e0dc7346f92cc4cee4179"),
            (61, "0fae1a833824da26a06af17d24b5e783"),
            (62, "8eece4c814dc5eb0d6722df349794fe0"),
            (63, "a62f6d59e837867693f042f5b8f5a236"),
            (64, "7160b8fb5e9e4023d549c3971fbaeead"),
            (65, "70bd662e7aefbda85a0f7244167b7897"),
            (130, "4298951876af905376a025f7279651c2"),
        ];
        for (len, want) in cases {
            assert_eq!(hex(digest(&ramp[..len])), want, "length {len}");
        }
    }
}
