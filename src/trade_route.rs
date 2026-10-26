// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The per-game trade-route buyer secret (`olympia/buy.c`,
//! playbymail/olympia-g3#46).

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::os::strerror;

/// The secret when no per-game seed is set.
const DEFAULT_SECRET: u32 = 0x00b0_5c0e;

/// The buyer secret and the seed it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeRoutes {
    /// The original seed, when a custom one is in effect.
    seed: Option<u64>,
    secret: u32,
}

impl Default for TradeRoutes {
    fn default() -> Self {
        Self {
            seed: None,
            secret: DEFAULT_SECRET,
        }
    }
}

impl TradeRoutes {
    /// Establishes the secret (`init_trade_routes()`). A saved
    /// `lib/trade_routes` wins; then `-G` (`pending`); then the
    /// `lib/trade-route-seed` input file; then the default.
    #[must_use]
    pub fn init(trade_routes: &Path, seed_input: &Path, pending: Option<u64>) -> Self {
        if let Ok(text) = fs::read(trade_routes) {
            let mut routes = Self::default();
            if let Some(seed) = scan_u64(&text) {
                routes.apply(seed);
            }
            if pending.is_some() {
                eprintln!(
                    "trade-route seed: {} already exists; ignoring -G/lib/trade-route-seed",
                    trade_routes.display()
                );
            }
            return routes;
        }
        let mut routes = Self::default();
        if let Some(seed) = pending {
            routes.apply(seed);
        } else if let Some(seed) = fs::read(seed_input).ok().and_then(|t| scan_u64(&t)) {
            routes.apply(seed);
        }
        routes
    }

    /// The buyer secret.
    #[must_use]
    pub fn secret(&self) -> u32 {
        self.secret
    }

    /// Saves a custom seed to `lib/trade_routes` (`save_trade_routes()`).
    /// With the default secret, no file is written.
    pub fn save(&self, path: &Path) {
        let Some(seed) = self.seed else {
            return;
        };
        match File::create(path) {
            Ok(mut file) => {
                // fprintf() is unchecked in C.
                let _ = writeln!(file, "{seed}");
            }
            Err(err) => eprintln!("couldn't write {}: {}", path.display(), strerror(&err)),
        }
    }

    fn apply(&mut self, seed: u64) {
        let s = splitmix64(seed);
        self.seed = Some(seed);
        self.secret = (s ^ (s >> 32)) as u32;
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// `fscanf("%llu")`: a decimal number after white space, or `None`.
fn scan_u64(text: &[u8]) -> Option<u64> {
    let s = text.trim_ascii_start();
    let (negative, digits) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let end = digits
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    let value = digits[..end].iter().fold(0u64, |v, &c| {
        v.saturating_mul(10).saturating_add(u64::from(c - b'0'))
    });
    Some(if negative {
        value.wrapping_neg()
    } else {
        value
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_secret_is_the_legacy_constant() {
        assert_eq!(TradeRoutes::default().secret(), 0xb05c0e);
    }

    #[test]
    fn scan_u64_like_fscanf() {
        assert_eq!(scan_u64(b"  12345\n"), Some(12345));
        assert_eq!(scan_u64(b"x"), None);
    }

    #[test]
    fn splitmix64_reference_value() {
        // splitmix64(0) from the reference implementation.
        assert_eq!(splitmix64(0), 0xe220_a839_7b1d_cdaf);
    }
}
