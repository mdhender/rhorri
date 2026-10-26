// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The entity catch-all stream, `enty` (`olympia/u.c`, the `ent_*`
//! helpers).
//!
//! The one-off draws with no subsystem of their own (prisoner escapes,
//! barking hounds, finding land for a swimmer, ...) are keyed leaves on one
//! per-turn stream: `rng_stream_of(rng_stream_of(root, turn, "turn"), 0,
//! "enty")`, seeded the first time one of them draws in a turn
//! (`begin_entity()`). A keyed leaf never moves a counter, so the stream is
//! derived afresh for each draw here. The leaves that can repeat within a
//! turn draw from a per-day child of it instead (`ent_today()`). See
//! olympia-g3's `doc/claude/rng-streams.md`.
//!
//! Only the helpers something ported calls are here so far: `ent_drop`,
//! `ent_take`, `ent_sick`, `ent_build` and `ent_menial` come with their
//! commands.

use crate::game::Game;
use crate::rng::{Stream, Tag};

/// `ETAG_TURN`: the per-turn parent of the `enty` stream.
const ETAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_ENTITY`.
const TAG_ENTITY: Tag = Tag::new(b"enty");
/// `ENTAG_PRISON`: `check_prisoner_escape`'s roll.
const ENTAG_PRISON: Tag = Tag::new(b"pris");
/// `ENTAG_LANDDIR`: `find_nearest_land`'s direction.
const ENTAG_LANDDIR: Tag = Tag::new(b"ldir");
/// `ENTAG_LANDPICK`: `find_nearest_land`'s province pick.
const ENTAG_LANDPICK: Tag = Tag::new(b"lpkk");
/// `ENTAG_BARK`: `bark_dogs`' per-hound flavor gate.
const ENTAG_BARK: Tag = Tag::new(b"bark");

/// `ESC_WEEKLY`: `check_prisoner_escape`'s weekly check.
pub const ESC_WEEKLY: i32 = 0;
/// `ESC_MOVE`: `check_prisoner_escape` as the captor moves.
pub const ESC_MOVE: i32 = 1;

impl Game {
    /// The `enty` turn stream (`begin_entity()`).
    fn entity_stream(&self) -> Stream {
        Stream::new(self.seed.master_seed())
            .child(i32::from(self.system.sysclock.turn), ETAG_TURN)
            .child(0, TAG_ENTITY)
    }

    /// `ent_today()`: a keyed draw on today's child of the `enty` stream,
    /// for the leaves that can repeat within a turn
    /// (playbymail/olympia-g3#186).
    fn ent_today(&self, k1: i32, k2: i32, tag: Tag, low: i32, high: i32) -> i32 {
        self.entity_stream()
            .child(i32::from(self.system.sysclock.day), tag)
            .keyed(k1, k2, tag, low, high)
    }

    /// `ent_prisoner()`: the escape roll in `1..=1000` for prisoner `who`
    /// in check `sub` (`ESC_WEEKLY`, `ESC_MOVE`, or a give).
    #[must_use]
    pub fn ent_prisoner(&self, who: i32, sub: i32) -> i32 {
        self.ent_today(who, sub, ENTAG_PRISON, 1, 1000)
    }

    /// `ent_bark()`: whether hound number `hound` of `who`'s barks (1 in
    /// 2).
    #[must_use]
    pub fn ent_bark(&self, who: i32, hound: i32) -> i32 {
        self.ent_today(who, hound, ENTAG_BARK, 1, 2)
    }

    /// `ent_land_dir()`: the direction `find_nearest_land` searches from
    /// `orig_where` on retry `retry`.
    #[must_use]
    pub fn ent_land_dir(&self, orig_where: i32, retry: i32) -> i32 {
        self.entity_stream()
            .keyed(orig_where, retry, ENTAG_LANDDIR, 1, 4)
    }

    /// `ent_land_pick()`: which of `len` provinces `find_nearest_land`
    /// falls back to from `orig_where`.
    #[must_use]
    pub fn ent_land_pick(&self, orig_where: i32, len: i32) -> i32 {
        self.entity_stream()
            .keyed(orig_where, 0, ENTAG_LANDPICK, 0, len - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];

    fn game(turn: i16, day: i16) -> Game {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new(MASTER);
        g.system.sysclock.turn = turn;
        g.system.sysclock.day = day;
        g
    }

    /// The `enty` draws for turn 3 under `MASTER`, from u.c's helpers run
    /// over lib/rng.c.
    #[test]
    fn enty_draws_match_c() {
        let g = game(3, 7);
        assert_eq!(
            [
                g.ent_prisoner(1234, ESC_WEEKLY),
                g.ent_prisoner(1234, ESC_MOVE)
            ],
            [415, 81]
        );
        assert_eq!(game(3, 14).ent_prisoner(1234, ESC_WEEKLY), 204);
        let g = game(3, 9);
        assert_eq!(
            [
                g.ent_bark(1234, 1),
                g.ent_bark(1234, 2),
                g.ent_bark(1234, 3)
            ],
            [1, 2, 1]
        );
        assert_eq!(
            [
                g.ent_land_dir(20101, 99),
                g.ent_land_dir(20101, 98),
                g.ent_land_pick(20101, 10),
                g.ent_land_pick(20101, 100)
            ],
            [2, 3, 2, 34]
        );
    }
}
