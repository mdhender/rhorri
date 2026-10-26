// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The legacy serial generator and the `randseed` file (`olympia/rnd.c`).
//!
//! No gameplay draw uses [`LegacyRng::rnd`] any more (playbymail/olympia-g3#25):
//! every subsystem draws from an [`rng::Stream`](crate::rng::Stream) rooted at
//! the master seed. What remains is the seed file itself, the master seed
//! captured from it, and [`md5_int`], which the trade-route buyer test uses.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use crate::md5;

/// The 128-bit seed loaded from `lib/randseed`, and the serial digest that
/// legacy draws advance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyRng {
    digest: [u32; 4],
    master: [u32; 4],
}

impl LegacyRng {
    /// Starts from a seed, which also becomes the master seed.
    #[must_use]
    pub fn new(seed: [u32; 4]) -> Self {
        Self {
            digest: seed,
            master: seed,
        }
    }

    /// Loads the seed from a `randseed` file.
    ///
    /// Like `load_seed()`, a file that cannot be opened is not an error: it
    /// prints `<path> could not be opened.` to standard output and leaves the
    /// seed all zeros. A file shorter than 16 bytes fills the seed from the
    /// front and leaves the rest zero.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let mut bytes = [0u8; 16];
        match File::open(path) {
            Ok(file) => {
                let mut buf = Vec::with_capacity(16);
                // fread() errors are ignored in C too; keep what was read.
                let _ = file.take(16).read_to_end(&mut buf);
                bytes[..buf.len()].copy_from_slice(&buf);
            }
            Err(_) => println!("{} could not be opened.", path.display()),
        }
        Self::new(words_from_le(&bytes))
    }

    /// Writes the current digest to a `randseed` file, as `save_seed()` does.
    ///
    /// # Errors
    ///
    /// Returns the error if the file cannot be created or written.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut bytes = [0u8; 16];
        for (chunk, word) in bytes.chunks_exact_mut(4).zip(self.digest) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        File::create(path)?.write_all(&bytes)
    }

    /// The seed as loaded, before any draw advanced it (`rng_master_seed()`).
    #[must_use]
    pub fn master_seed(&self) -> [u32; 4] {
        self.master
    }

    /// Draws from the serial digest, uniform in `low..=high` (`rnd()`).
    ///
    /// # Panics
    ///
    /// Panics unless `0 <= high - low <= i32::MAX`. The C code hangs or
    /// overflows outside that range.
    pub fn rnd(&mut self, low: i32, high: i32) -> i32 {
        reduce_range(&mut self.digest, low, high)
    }
}

/// Hashes four integers to one (`md5_int()`).
#[must_use]
pub fn md5_int(a: i32, b: i32, c: i32, d: i32) -> i32 {
    md5::hash_words(&[a as u32, b as u32, c as u32, d as u32])[0] as i32
}

/// Reduces a working digest to a draw in `low..=high`, re-hashing it in place
/// until a masked draw lands in range. Shared by `rnd()` and the stream layer
/// (`lib/rng.c:reduce_range`).
///
/// # Panics
///
/// Panics unless `0 <= high - low <= i32::MAX`.
pub(crate) fn reduce_range(h: &mut [u32; 4], low: i32, high: i32) -> i32 {
    let range = high
        .checked_sub(low)
        .filter(|r| *r >= 0)
        .unwrap_or_else(|| panic!("invalid draw range {low}..={high}")) as u32;
    let mask = u32::MAX.checked_shr(range.leading_zeros()).unwrap_or(0);
    loop {
        *h = md5::hash_words(h);
        let num = h[0] & mask;
        if num <= range {
            return num.wrapping_add(low as u32) as i32;
        }
    }
}

fn words_from_le(bytes: &[u8; 16]) -> [u32; 4] {
    let mut words = [0u32; 4];
    for (word, chunk) in words.iter_mut().zip(bytes.chunks_exact(4)) {
        *word = u32::from_le_bytes(chunk.try_into().expect("chunk is 4 bytes"));
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes the C reference wrote as its seed file.
    fn seed_bytes() -> [u8; 16] {
        std::array::from_fn(|i| 0xa5 ^ (i as u8).wrapping_mul(37))
    }

    fn rng() -> LegacyRng {
        LegacyRng::new(words_from_le(&seed_bytes()))
    }

    #[test]
    fn rnd_matches_c() {
        let mut r = rng();
        let rolls: Vec<i32> = (0..12).map(|_| r.rnd(1, 6)).collect();
        assert_eq!(rolls, [2, 2, 3, 2, 5, 5, 1, 4, 5, 1, 3, 6]);
        assert_eq!(r.rnd(0, 0), 0);
        assert_eq!(r.rnd(-10, -3), -4);
        assert_eq!(r.rnd(0, i32::MAX - 1), 1_872_564_680);
        assert_eq!(r.rnd(1000, 9999), 8389);
    }

    #[test]
    fn master_seed_survives_draws() {
        let mut r = rng();
        r.rnd(1, 100);
        assert_eq!(
            r.master_seed(),
            [0xcaef_80a5, 0xa67b_1c31, 0x32d7_e88d, 0x8ea3_4419]
        );
    }

    #[test]
    fn md5_int_matches_c() {
        assert_eq!(md5_int(1, 2, 3, 4), 1_794_852_427);
        assert_eq!(md5_int(-1, 0, 12345, -99999), -2_080_497_132);
    }

    #[test]
    fn load_and_save_round_trip_like_c() {
        let dir = std::env::temp_dir().join(format!("rhorri-rnd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let seed = dir.join("randseed");
        std::fs::write(&seed, seed_bytes()).expect("write seed");

        let mut r = LegacyRng::load(&seed);
        assert_eq!(r, rng());
        for _ in 0..12 {
            r.rnd(1, 6);
        }
        for (low, high) in [(0, 0), (-10, -3), (0, i32::MAX - 1), (1000, 9999)] {
            r.rnd(low, high);
        }
        r.save(&seed).expect("save seed");
        let saved = std::fs::read(&seed).expect("read seed");
        let hex: String = saved.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "dddc1ef2eef2a1f9dc3e82af02f7ec32");

        std::fs::remove_dir_all(&dir).expect("remove temp dir");
    }

    #[test]
    fn short_seed_file_pads_with_zeros() {
        let dir = std::env::temp_dir().join(format!("rhorri-short-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let seed = dir.join("randseed");
        std::fs::write(&seed, [1, 2, 3, 4, 5]).expect("write seed");

        assert_eq!(LegacyRng::load(&seed).master_seed(), [0x0403_0201, 5, 0, 0]);

        std::fs::remove_dir_all(&dir).expect("remove temp dir");
    }

    #[test]
    #[should_panic(expected = "invalid draw range")]
    fn reversed_range_panics() {
        rng().rnd(5, 4);
    }
}
