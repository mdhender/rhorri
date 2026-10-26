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
//! The elven hunts (`auto_faery`) roam Faery and drive out humans who
//! come without a Faery stone. Building Faery (`create_faery`, the
//! world-init half of the stream) is world creation, which isn't ported
//! yet.
//!
//! TODO(stage 2): `warn_human`'s `log_write(LOG_SPECIAL, "Faery hunt warned
//! %s.")` is report output.

use crate::code::int_to_code;
use crate::entity::{Kind, Subkind};
use crate::game::{Game, player};
use crate::loc::{here_list, subloc};
use crate::npc::USE_FAERY_STONE;
use crate::order::queue;
use crate::rng::{Stream, Tag};
use crate::swear::LOY_NPC;
use crate::turn::Phase;
use crate::unit::{SUB_NI, stack_has_use_key};

/// `TAG_TURN`: the per-turn parent of the `faer` stream.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_FAERY`.
const TAG_FAERY: Tag = Tag::new(b"faer");
/// `FTAG_HUNT`: `create_elven_hunt`'s place, keyed on its slot and retry.
const FTAG_HUNT: Tag = Tag::new(b"hunt");
/// `FTAG_HQTY`: `create_elven_hunt`'s elf count.
const FTAG_HQTY: Tag = Tag::new(b"hqty");
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

/// How many elven hunts Faery keeps.
const FAERY_UNITS: i32 = 15;

/// `item_elf`.
const ITEM_ELF: i32 = 25;

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

    /// `faer_hunt_loc()`: `create_elven_hunt`'s place for spawn slot
    /// `slot`, try `retry`.
    pub fn faer_hunt_loc(&mut self, slot: i32, retry: i32, low: i32, high: i32) -> i32 {
        self.faery_stream().keyed(slot, retry, FTAG_HUNT, low, high)
    }

    /// `faer_hunt_qty()`: how many elves are in the hunt for spawn slot
    /// `slot`.
    pub fn faer_hunt_qty(&mut self, slot: i32) -> i32 {
        self.faery_stream().keyed(slot, 0, FTAG_HQTY, 25, 100)
    }

    /// `olympia/faery.c:create_elven_hunt`: a hunt of 25 to 100 elves for
    /// spawn slot `slot`, in a random land place in Faery, made ready to
    /// run its orders at once.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// swearing the hunt to Faery reaches something not ported.
    ///
    /// # Panics
    ///
    /// Panics if Faery has no location info, as C asserts.
    fn create_elven_hunt(&mut self, slot: i32) -> Phase {
        let region = self.system.faery_region;
        assert!(
            self.db.get(region).is_some(),
            "create_elven_hunt: no Faery region {region}"
        );
        let places = here_list(&self.db, region);
        let last = i32::try_from(places.len()).expect("Faery size") - 1;
        let mut retry = 0;
        let where_ = loop {
            let r = self.faer_hunt_loc(slot, retry, 0, last);
            retry += 1;
            let where_ = places[usize::try_from(r).expect("index")];
            if self.db.subkind(where_) != Subkind::OCEAN {
                break where_;
            }
        };

        let pl = self.system.faery_player;
        let new = self.new_char(
            SUB_NI,
            ITEM_ELF,
            where_,
            100,
            pl,
            LOY_NPC,
            0,
            Some(b"Faery Hunt"),
        )?;
        if new < 0 {
            return Ok(());
        }

        let qty = self.faer_hunt_qty(slot);
        self.gen_item(new, ITEM_ELF, qty);

        queue(&mut self.db, new, b"wait time 0");
        self.init_load_sup(new); // ready to run commands at once
        Ok(())
    }

    /// `olympia/faery.c:warn_human`: hunt `who` tells `targ` to leave.
    fn warn_human(&mut self, who: i32, targ: i32) {
        queue(
            &mut self.db,
            who,
            format!("message 1 {}", int_to_code(targ)).as_bytes(),
        );
        queue(
            &mut self.db,
            who,
            b"You are not welcome in Faery.  Leave, or you will be killed.",
        );
    }

    /// `olympia/faery.c:auto_faery_sup`: hunt `who` warns each character
    /// of a regular faction where it is without a Faery stone, and attacks
    /// those it has warned before; with no one to drive out, it moves on.
    fn auto_faery_sup(&mut self, who: i32) {
        let where_ = subloc(&self.db, who);
        let pl = player(&self.db, who);
        let mut queued_something = false;

        for i in here_list(&self.db, where_) {
            if self.db.kind(i) != Kind::CHAR
                || self.db.subkind(player(&self.db, i)) != Subkind::PL_REGULAR
            {
                continue;
            }
            if stack_has_use_key(&self.db, i, USE_FAERY_STONE) != 0 {
                continue;
            }
            queued_something = true;

            // p_misc(player(who)): the faction remembers whom it warned.
            if !self.db.p_misc(pl).npc_memory.contains(i) {
                self.warn_human(who, i);
                self.db.p_misc(pl).npc_memory.append(i); // set_bit()
                continue;
            }

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

    /// `olympia/faery.c:auto_faery`: tops Faery up to 15 elven hunts, then
    /// queues each one's orders.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// making a hunt reaches something not ported.
    pub(crate) fn auto_faery(&mut self) -> Phase {
        let pl = self.system.faery_player;
        let units = |g: &Self| {
            // loop_units walks a copy of the list.
            g.db.get(pl)
                .and_then(|b| b.player.as_ref())
                .map(|p| p.units.as_slice().to_vec())
                .unwrap_or_default()
        };

        let mut n_faery = i32::try_from(units(self).len()).expect("unit count");
        while n_faery < FAERY_UNITS {
            self.create_elven_hunt(n_faery)?;
            n_faery += 1;
        }

        for i in units(self) {
            self.auto_faery_sup(i);
        }
        Ok(())
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
        assert_eq!(
            [
                g.faer_hunt_loc(0, 0, 0, 99),
                g.faer_hunt_loc(0, 1, 0, 99),
                g.faer_hunt_loc(7, 0, 0, 99)
            ],
            [78, 86, 9]
        );
        assert_eq!([g.faer_hunt_qty(0), g.faer_hunt_qty(7)], [62, 86]);
    }
}
