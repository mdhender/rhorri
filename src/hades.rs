// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Hades' monsters and the `hads` stream (`olympia/hades.c`).
//!
//! Hades' turn-time draws are keyed leaves on one per-turn stream:
//! `rng_stream_of(rng_stream_of(root, turn, "turn"), 0, "hads")`, seeded
//! the first time Hades draws in a turn (`begin_hades()`). The leaves that
//! can repeat within a turn (the bandit ambushes) draw from a per-day
//! child of it instead (`hads_today()`). See olympia-g3's
//! `doc/claude/rng-streams.md`.
//!
//! Building Hades (`create_hades`, the world-init half of the stream) is
//! world creation, which isn't ported yet. `random_hades_loc` belongs to
//! the transcend-death magic of stage 4.

use crate::code::int_to_code;
use crate::entity::{Kind, Subkind};
use crate::game::{Game, NotYetImplemented, player};
use crate::loc::{here_list, subloc};
use crate::npc::SK_TRANSCEND_DEATH;
use crate::order::queue;
use crate::rng::{Stream, Tag};
use crate::swear::LOY_NPC;
use crate::turn::Phase;
use crate::unit::{SUB_NI, char_alone, has_skill};

/// `TAG_TURN`: the per-turn parent of the `hads` stream.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_HADES`.
const TAG_HADES: Tag = Tag::new(b"hads");
/// `HTAG_NAST`: `create_hades_nasty`'s spawn, keyed on its slot.
const HTAG_NAST: Tag = Tag::new(b"nast");
/// `HTAG_BKND`: `create_hades_bandit`'s kind.
const HTAG_BKND: Tag = Tag::new(b"bknd");
/// `HTAG_BQTY`: `create_hades_bandit`'s troop count.
const HTAG_BQTY: Tag = Tag::new(b"bqty");
/// `HTAG_AMBS`: `hades_attack_check`'s ambush trigger.
const HTAG_AMBS: Tag = Tag::new(b"ambs");
/// `HTAG_RTAL`: `hades_attack_check`'s retaliation.
const HTAG_RTAL: Tag = Tag::new(b"rtal");

/// How many monsters Hades keeps.
const HADES_UNITS: i32 = 25;

/// `item_spirit`.
const ITEM_SPIRIT: i32 = 26;

/// The `hads` turn stream and the turn it was seeded for (`hades_rng`,
/// `hades_rng_turn`). Unseeded until the first draw.
#[derive(Debug, Clone, Default)]
pub struct HadesRng {
    seeded: Option<(i16, Stream)>,
}

impl HadesRng {
    /// The stream for `turn`, seeding it from `master` if it hasn't been
    /// seeded for this turn (`begin_hades()`).
    fn stream(&mut self, master: [u32; 4], turn: i16) -> &Stream {
        if self.seeded.as_ref().is_none_or(|&(t, _)| t != turn) {
            let stream = Stream::new(master)
                .child(i32::from(turn), TAG_TURN)
                .child(0, TAG_HADES);
            self.seeded = Some((turn, stream));
        }
        &self.seeded.as_ref().expect("seeded above").1
    }
}

impl Game {
    /// The `hads` turn stream, seeded for this turn (`begin_hades()`).
    fn hades_stream(&mut self) -> Stream {
        let master = self.seed.master_seed();
        let turn = self.system.sysclock.turn;
        *self.hades.stream(master, turn)
    }

    /// `hads_nasty()`: `create_hades_nasty`'s draw `sub` (0 where, 1 kind,
    /// 2 troop count) for spawn slot `slot`.
    fn hads_nasty(&mut self, slot: i32, sub: i32, low: i32, high: i32) -> i32 {
        self.hades_stream().keyed(slot, sub, HTAG_NAST, low, high)
    }

    /// `hads_today()`: a keyed draw on today's child of the `hads` stream,
    /// for the leaves a unit can repeat within a turn
    /// (playbymail/olympia-g3#186).
    fn hads_today(&mut self, k1: i32, k2: i32, tag: Tag, low: i32, high: i32) -> i32 {
        let day = i32::from(self.system.sysclock.day);
        self.hades_stream()
            .child(day, tag)
            .keyed(k1, k2, tag, low, high)
    }

    /// `hads_bandit_kind()`: which bandits ambush `who` entering `where_`.
    pub fn hads_bandit_kind(&mut self, who: i32, where_: i32) -> i32 {
        self.hads_today(who, where_, HTAG_BKND, 1, 5)
    }

    /// `hads_bandit_qty()`: how many bandits are in `unit`.
    pub fn hads_bandit_qty(&mut self, unit: i32) -> i32 {
        self.hades_stream().keyed(unit, 0, HTAG_BQTY, 4, 24)
    }

    /// `hads_ambush()`: the ambush trigger roll for `who` entering
    /// `where_`.
    pub fn hads_ambush(&mut self, who: i32, where_: i32) -> i32 {
        self.hads_today(who, where_, HTAG_AMBS, 1, 100)
    }

    /// `hads_retal()`: whether ambushers attack `who` at once.
    pub fn hads_retal(&mut self, who: i32, where_: i32) -> i32 {
        self.hads_today(who, where_, HTAG_RTAL, 1, 2)
    }

    /// `create_hades_nasty()`: a monster for spawn slot `slot` in a random
    /// place in Hades: tortured spirits, a ghostly presence, or a lesser
    /// or greater demon, made ready to run its orders at once.
    ///
    /// # Panics
    ///
    /// Panics if Hades has no location info, as C asserts.
    fn create_hades_nasty(&mut self, slot: i32) -> Result<(), NotYetImplemented> {
        let region = self.system.hades_region;
        let pl = self.system.hades_player;
        let places = here_list(&self.db, region);
        let last = i32::try_from(places.len()).expect("Hades size") - 1;
        let where_ = places[usize::try_from(self.hads_nasty(slot, 0, 0, last)).expect("index")];

        let kind = self.hads_nasty(slot, 1, 1, 4);
        let (sk, name): (Subkind, &[u8]) = match kind {
            1 => (SUB_NI, b"Tortured spirits"),
            2 => (Subkind::default(), b"Ghostly presence"),
            3 => (Subkind::default(), b"Lesser Demon"),
            4 => (Subkind::default(), b"Greater Demon"),
            _ => unreachable!("hads_nasty kind {kind}"),
        };
        let ni = if kind == 1 { ITEM_SPIRIT } else { 0 };
        let new = self.new_char(sk, ni, where_, 100, pl, LOY_NPC, 0, Some(name))?;
        if new < 0 {
            return Ok(());
        }

        let strength = match kind {
            2 => 100,
            3 => 250,
            4 => 500,
            _ => 0,
        };
        if strength != 0 {
            let c = self.db.p_char(new);
            c.attack = strength;
            c.defense = strength;
        }
        let spirits = match kind {
            1 => Some((25, 75)),
            3 => Some((50, 150)),
            4 => Some((100, 250)),
            _ => None,
        };
        if let Some((low, high)) = spirits {
            let qty = self.hads_nasty(slot, 2, low, high);
            self.gen_item(new, ITEM_SPIRIT, qty);
        }

        queue(&mut self.db, new, b"wait time 0");
        self.init_load_sup(new)?; // ready to run commands at once
        Ok(())
    }

    /// `auto_hades_sup()`: a Hades monster attacks every character of a
    /// regular faction where it is (but a lone transcender of death), or
    /// moves on if there are none.
    fn auto_hades_sup(&mut self, who: i32) {
        let where_ = subloc(&self.db, who);
        let mut queued_something = false;

        for i in here_list(&self.db, where_) {
            if self.db.kind(i) != Kind::CHAR
                || self.db.subkind(player(&self.db, i)) != Subkind::PL_REGULAR
            {
                continue;
            }
            if has_skill(&self.db, i, SK_TRANSCEND_DEATH) != 0 && char_alone(&self.db, i) {
                continue;
            }
            queued_something = true;
            queue(
                &mut self.db,
                who,
                format!("attack {}", int_to_code(i)).as_bytes(),
            );
        }

        if !queued_something {
            self.npc_move(who);
        }
    }

    /// `olympia/hades.c:auto_hades`: tops Hades up to 25 monsters, then
    /// queues each one's orders.
    pub(crate) fn auto_hades(&mut self) -> Phase {
        let pl = self.system.hades_player;
        let units = |g: &Self| {
            // loop_units walks a copy of the list.
            g.db.get(pl)
                .and_then(|b| b.player.as_ref())
                .map(|p| p.units.as_slice().to_vec())
                .unwrap_or_default()
        };

        let mut n_hades = i32::try_from(units(self).len()).expect("unit count");
        while n_hades < HADES_UNITS {
            self.create_hades_nasty(n_hades)?;
            n_hades += 1;
        }

        for i in units(self) {
            self.auto_hades_sup(i);
        }
        Ok(())
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

    /// The `hads` draws for turn 3 under `MASTER`, from hades.c's helpers
    /// run over lib/rng.c.
    #[test]
    fn hads_draws_match_c() {
        let mut g = game(3, 0);
        let nasty: Vec<i32> = (0..5)
            .flat_map(|slot| [g.hads_nasty(slot, 0, 0, 9), g.hads_nasty(slot, 1, 1, 4)])
            .collect();
        assert_eq!(nasty, HADS_NASTY);
        assert_eq!(g.hads_nasty(7, 2, 100, 250), HADS_NASTY_QTY);
        assert_eq!(g.hads_bandit_qty(1234), HADS_BQTY);
        g.system.sysclock.day = 12;
        assert_eq!(
            [
                g.hads_ambush(1234, 20101),
                g.hads_retal(1234, 20101),
                g.hads_bandit_kind(1234, 20101)
            ],
            HADS_DAY12
        );
        g.system.sysclock.day = 13;
        assert_eq!(g.hads_ambush(1234, 20101), HADS_AMBUSH_DAY13);
    }

    const HADS_NASTY: [i32; 10] = [0, 4, 6, 3, 0, 1, 3, 1, 2, 3];
    const HADS_NASTY_QTY: i32 = 155;
    const HADS_BQTY: i32 = 18;
    const HADS_DAY12: [i32; 3] = [76, 1, 5];
    const HADS_AMBUSH_DAY13: i32 = 46;
}
