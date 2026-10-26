// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Faery's turn-time draws: the `faer` stream (`olympia/faery.c`).
//!
//! Faery's turn-time draws are keyed leaves on one per-turn stream:
//! `rng_stream_of(rng_stream_of(root, turn, "turn"), 0, "faer")`, seeded
//! the first time Faery draws in a turn (`begin_faery()`). The leaves that
//! can repeat within a turn (the bandit ambushes) draw from a per-day
//! child of it instead (`faer_today()`). See olympia-g3's
//! `doc/claude/rng-streams.md`.
//!
//! The elven hunts (`auto_faery`, `create_elven_hunt`) come with the daily
//! events. Building Faery (`create_faery`, the world-init half of the
//! stream) is world creation, which isn't ported yet.

use crate::game::Game;
use crate::rng::{Stream, Tag};

/// `TAG_TURN`: the per-turn parent of the `faer` stream.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_FAERY`.
const TAG_FAERY: Tag = Tag::new(b"faer");
/// `FTAG_BKND`: `create_faery_bandit`'s kind.
const FTAG_BKND: Tag = Tag::new(b"bknd");
/// `FTAG_BQTY`: `create_faery_bandit`'s troop count.
const FTAG_BQTY: Tag = Tag::new(b"bqty");
/// `FTAG_BGLD`: `create_faery_bandit`'s gold.
const FTAG_BGLD: Tag = Tag::new(b"bgld");
/// `FTAG_AMBS`: `faery_attack_check`'s ambush trigger.
const FTAG_AMBS: Tag = Tag::new(b"ambs");
/// `FTAG_RTAL`: `faery_attack_check`'s retaliation.
const FTAG_RTAL: Tag = Tag::new(b"rtal");

/// The `faer` turn stream and the turn it was seeded for (`faery_rng`,
/// `faery_rng_turn`). Unseeded until the first draw.
#[derive(Debug, Clone, Default)]
pub struct FaeryRng {
    seeded: Option<(i16, Stream)>,
}

impl FaeryRng {
    /// The stream for `turn`, seeding it from `master` if it hasn't been
    /// seeded for this turn (`begin_faery()`).
    fn stream(&mut self, master: [u32; 4], turn: i16) -> &Stream {
        if self.seeded.as_ref().is_none_or(|&(t, _)| t != turn) {
            let stream = Stream::new(master)
                .child(i32::from(turn), TAG_TURN)
                .child(0, TAG_FAERY);
            self.seeded = Some((turn, stream));
        }
        &self.seeded.as_ref().expect("seeded above").1
    }
}

impl Game {
    /// The `faer` turn stream, seeded for this turn (`begin_faery()`).
    pub(crate) fn faery_stream(&mut self) -> Stream {
        let master = self.seed.master_seed();
        let turn = self.system.sysclock.turn;
        *self.faery.stream(master, turn)
    }

    /// `faer_today()`: a keyed draw on today's child of the `faer` stream,
    /// for the leaves a unit can repeat within a turn
    /// (playbymail/olympia-g3#186).
    fn faer_today(&mut self, k1: i32, k2: i32, tag: Tag, low: i32, high: i32) -> i32 {
        let day = i32::from(self.system.sysclock.day);
        self.faery_stream()
            .child(day, tag)
            .keyed(k1, k2, tag, low, high)
    }

    /// `faer_bandit_kind()`: which bandits ambush `who` entering `where_`.
    pub fn faer_bandit_kind(&mut self, who: i32, where_: i32) -> i32 {
        self.faer_today(who, where_, FTAG_BKND, 1, 3)
    }

    /// `faer_bandit_qty()`: how many bandits are in `unit`.
    pub fn faer_bandit_qty(&mut self, unit: i32) -> i32 {
        self.faery_stream().keyed(unit, 0, FTAG_BQTY, 4, 24)
    }

    /// `faer_bandit_gold()`: the gold `unit` carries.
    pub fn faer_bandit_gold(&mut self, unit: i32) -> i32 {
        self.faery_stream().keyed(unit, 0, FTAG_BGLD, 1, 25)
    }

    /// `faer_ambush()`: the ambush trigger roll for `who` entering
    /// `where_`.
    pub fn faer_ambush(&mut self, who: i32, where_: i32) -> i32 {
        self.faer_today(who, where_, FTAG_AMBS, 1, 100)
    }

    /// `faer_retal()`: whether ambushers attack `who` at once.
    pub fn faer_retal(&mut self, who: i32, where_: i32) -> i32 {
        self.faer_today(who, where_, FTAG_RTAL, 1, 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];

    /// The `faer` draws for turn 3 under `MASTER`, from faery.c's helpers
    /// run over lib/rng.c.
    #[test]
    fn faer_draws_match_c() {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new(MASTER);
        g.system.sysclock.turn = 3;
        assert_eq!(g.faer_bandit_qty(1234), 6);
        assert_eq!(g.faer_bandit_gold(1234), 8);
        g.system.sysclock.day = 12;
        assert_eq!(
            [
                g.faer_ambush(1234, 20101),
                g.faer_retal(1234, 20101),
                g.faer_bandit_kind(1234, 20101)
            ],
            [13, 1, 1]
        );
        g.system.sysclock.day = 13;
        assert_eq!(g.faer_ambush(1234, 20101), 95);
    }
}
