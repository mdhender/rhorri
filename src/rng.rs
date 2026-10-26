// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Addressable random streams (`lib/rng.{c,h}`).
//!
//! Every random draw in the engine comes from a [`Stream`]: a 128-bit seed
//! derived from the master seed through a chain of `(key, tag)` steps, plus a
//! counter for sequential draws. A child stream depends only on its parent,
//! key and tag, so subsystems cannot shift each other's draws.
//!
//! The streams, their tags and their keying are listed in olympia-g3's
//! `doc/claude/rng-streams.md`.

use crate::md5;
use crate::rnd::reduce_range;

/// A stream's purpose tag: four ASCII bytes packed big-endian, so
/// `Tag::new(b"turn")` is the C `0x7475726eu /* "turn" */`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tag(u32);

impl Tag {
    /// Packs a four-character tag name.
    #[must_use]
    pub const fn new(name: &[u8; 4]) -> Self {
        Self(u32::from_be_bytes(*name))
    }
}

/// A 128-bit seed and a draw counter (`rng_stream`).
///
/// Streams are plain values with no shared state: copy one to fork it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stream {
    seed: [u32; 4],
    counter: u32,
}

impl Stream {
    /// Roots a stream at a master seed (`rng_seed()`).
    #[must_use]
    pub fn new(master: [u32; 4]) -> Self {
        Self {
            seed: master,
            counter: 0,
        }
    }

    /// Derives the child stream for `key` and `tag` (`rng_stream_of()`).
    ///
    /// The child depends only on this stream's seed, `key` and `tag`, not on
    /// its counter or on which siblings were derived first.
    #[must_use]
    pub fn child(&self, key: i32, tag: Tag) -> Self {
        let [s0, s1, s2, s3] = self.seed;
        Self {
            seed: md5::hash_words(&[s0, s1, s2, s3, key as u32, tag.0]),
            counter: 0,
        }
    }

    /// Draws the next value in `low..=high` and advances the counter
    /// (`rng_draw()`).
    ///
    /// # Panics
    ///
    /// Panics unless `0 <= high - low <= i32::MAX`.
    pub fn draw(&mut self, low: i32, high: i32) -> i32 {
        let [s0, s1, s2, s3] = self.seed;
        let mut h = md5::hash_words(&[s0, s1, s2, s3, self.counter]);
        self.counter = self.counter.wrapping_add(1);
        reduce_range(&mut h, low, high)
    }

    /// Draws a value in `low..=high` that depends only on this stream's seed,
    /// `k1`, `k2` and `tag`, not on any other draw (`rng_keyed()`).
    ///
    /// # Panics
    ///
    /// Panics unless `0 <= high - low <= i32::MAX`.
    #[must_use]
    pub fn keyed(&self, k1: i32, k2: i32, tag: Tag, low: i32, high: i32) -> i32 {
        let [s0, s1, s2, s3] = self.seed;
        let mut h = md5::hash_words(&[s0, s1, s2, s3, k1 as u32, k2 as u32, tag.0]);
        reduce_range(&mut h, low, high)
    }

    /// Shuffles `items` in place with sequential draws
    /// (`lib/ilist.c:ilist_shuffle_rng`, `lib/exit_views.c:exit_views_shuffle_rng`).
    ///
    /// Each slot but the last swaps with a slot drawn from itself to the end,
    /// so a list of `n` items takes `n - 1` draws.
    ///
    /// # Panics
    ///
    /// Panics if `items` has more than `i32::MAX` elements.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        let last = i32::try_from(items.len())
            .unwrap_or_else(|_| panic!("cannot shuffle {} items", items.len()))
            - 1;
        for i in 0..last {
            let r = self.draw(i, last);
            items.swap(i as usize, r as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];
    const TURN: Tag = Tag::new(b"turn");
    const COMBAT: Tag = Tag::new(b"comb");
    const MARKET: Tag = Tag::new(b"mrkt");
    const HIT: Tag = Tag::new(b"hit ");
    const DMG: Tag = Tag::new(b"dmg ");

    fn turn() -> Stream {
        Stream::new(MASTER).child(42, TURN)
    }

    #[test]
    fn tag_packs_big_endian() {
        assert_eq!(TURN, Tag(0x7475_726e));
        assert_eq!(HIT, Tag(0x6869_7420));
    }

    #[test]
    fn child_matches_c() {
        let t = turn();
        assert_eq!(t.seed, [0x2159_8470, 0x5792_16c2, 0x5906_297d, 0x0300_42be]);
        let c = t.child(-7, COMBAT);
        assert_eq!(c.seed, [0x9ca8_feda, 0xf9f1_9146, 0x4894_ef90, 0xac13_f745]);
    }

    #[test]
    fn draw_matches_c() {
        let mut c = turn().child(-7, COMBAT);
        let rolls: Vec<i32> = (0..10).map(|_| c.draw(1, 10)).collect();
        assert_eq!(rolls, [6, 9, 4, 6, 6, 6, 3, 10, 6, 9]);
        assert_eq!(c.draw(-16, -3), -10);
        assert_eq!(c.draw(1000, 9999), 6826);
        assert_eq!(c.draw(5, 5), 5);
        assert_eq!(c.draw(0, i32::MAX - 1), 915_273_372);
        assert_eq!(c.counter, 14);
    }

    #[test]
    fn keyed_matches_c() {
        let root = Stream::new(MASTER);
        let t = turn();
        assert_eq!(t.keyed(501, 17, HIT, 1, 100), 73);
        assert_eq!(t.keyed(-1, 0, DMG, 1, 6), 3);
        assert_eq!(root.keyed(0, 0, Tag(0), 0, 1), 1);
        assert_eq!(
            root.keyed(i32::MAX, i32::MIN, Tag(u32::MAX), -1000, 1000),
            -144
        );
    }

    #[test]
    fn shuffle_matches_c() {
        let mut s = Stream::new(MASTER).child(3, Tag::new(b"exdr"));
        let mut items: Vec<i32> = (10..19).collect();
        s.shuffle(&mut items);
        assert_eq!(items, [15, 11, 13, 18, 17, 10, 14, 16, 12]);
        assert_eq!(s.counter, 8);
    }

    #[test]
    fn shuffle_of_zero_or_one_item_draws_nothing() {
        let mut s = turn();
        s.shuffle::<i32>(&mut []);
        s.shuffle(&mut [1]);
        assert_eq!(s.counter, 0);
    }

    #[test]
    fn keyed_draw_ignores_other_draws() {
        let mut battle = turn().child(501, COMBAT);
        let hit = battle.keyed(1, 0, HIT, 1, 20);
        battle.draw(1, 10);
        let _ = battle.keyed(1, 0, DMG, 1, 6);
        assert_eq!(battle.keyed(1, 0, HIT, 1, 20), hit);
    }

    #[test]
    fn child_ignores_sibling_order_and_counter() {
        let mut root = Stream::new(MASTER);
        let combat = root.child(0, COMBAT);
        let market = root.child(0, MARKET);
        root.draw(1, 10);
        assert_eq!(root.child(0, MARKET), market);
        assert_eq!(root.child(0, COMBAT), combat);
        assert_ne!(combat, market);
    }
}
