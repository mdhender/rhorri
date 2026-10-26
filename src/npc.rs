// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The independent NPCs' programs and the `npcs` stream
//! (`olympia/npc.c`, and `auto_undead` from `olympia/necro.c`).
//!
//! `npcs` is a per-location stream: `begin_npc(where)` reseeds it as
//! `rng_stream_of(rng_stream_of(root, turn, "turn"), where, "npcs")`, and
//! every draw from it is a keyed leaf (`npc_spawn`, `npc_qty`,
//! `npc_behavior`, and `auto_bandit`'s victim pick), so a draw depends only
//! on the location the stream was last seeded for and its keys. See
//! olympia-g3's `doc/claude/rng-streams.md`.
//!
//! The bandit ambushes (`hades_attack_check`, `faery_attack_check`, with
//! `create_hades_bandit` and `create_faery_bandit`) run when a stack
//! enters a province of Hades or Faery (`move_stack`). The cookie NPCs
//! (`do_cookie_npc`, `create_peasant_mob`) come with the commands that make
//! them.

use crate::code::int_to_code;
use crate::dir::{ExitView, FULL_DIR_S, LAND};
use crate::entity::{Kind, Subkind};
use crate::game::{Game, NotYetImplemented};
use crate::input::STATE_DONE;
use crate::loc::{LOC_BUILD, LOC_PROVINCE, here_list, loc_depth, subloc};
use crate::order::{queue, top_order};
use crate::rng::{Stream, Tag};
use crate::savage::ITEM_SAVAGE;
use crate::storm::npc_summoner;
use crate::swear::{LOY_NPC, LOY_SUMMON};
use crate::turn::Phase;
use crate::unit::{
    ITEM_GOLD, SUB_NI, char_alone, char_really_hidden, has_skill, is_npc, is_prisoner, loyal_kind,
    noble_item, npc_last_dir, npc_program, stack_has_use_key,
};

/// `TAG_TURN`: the per-turn parent of the `npcs` stream.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_NPC`.
const TAG_NPC: Tag = Tag::new(b"npcs");
/// `NTAG_SPAWN`: does an NPC spawn here.
const NTAG_SPAWN: Tag = Tag::new(b"spwn");
/// `NTAG_QTY`: a spawned quantity.
const NTAG_QTY: Tag = Tag::new(b"qnty");
/// `NTAG_BEHAVE`: autonomous NPC behavior.
const NTAG_BEHAVE: Tag = Tag::new(b"bhvr");
/// `NTAG_VICTIM`: `auto_bandit`'s victim pick.
const NTAG_VICTIM: Tag = Tag::new(b"vict");

/// `PROG_bandit`: wilderness spice.
pub const PROG_BANDIT: i32 = 1;
/// `PROG_subloc_monster`.
pub const PROG_SUBLOC_MONSTER: i32 = 2;
/// `PROG_npc_token`.
pub const PROG_NPC_TOKEN: i32 = 3;
/// `PROG_faery_bandit`.
pub const PROG_FAERY_BANDIT: i32 = 4;
/// `PROG_hades_bandit`.
pub const PROG_HADES_BANDIT: i32 = 5;

/// `sub_undead`: a demon lord.
const SUB_UNDEAD: Subkind = Subkind(55);
/// `item_peasant`.
const ITEM_PEASANT: i32 = 10;
/// `item_angry_peasant`.
const ITEM_ANGRY_PEASANT: i32 = 23;
/// `item_elf`.
const ITEM_ELF: i32 = 25;
/// `item_spirit`.
const ITEM_SPIRIT: i32 = 26;
/// `item_corpse`.
const ITEM_CORPSE: i32 = 31;
/// `item_skeleton`.
const ITEM_SKELETON: i32 = 33;
/// `item_gorgon`.
const ITEM_GORGON: i32 = 288;
/// `item_faery`.
const ITEM_FAERY: i32 = 293;
/// `sk_transcend_death`.
pub const SK_TRANSCEND_DEATH: i32 = 911;
/// `use_faery_stone`: the Faery gate opener.
pub const USE_FAERY_STONE: i32 = 8;

/// The `npcs` stream as `begin_npc()` last seeded it (`npc_rng`).
/// Unseeded until the first `begin_npc()`.
#[derive(Debug, Clone, Default)]
pub struct NpcRng {
    stream: Option<Stream>,
}

impl NpcRng {
    /// The stream, which `begin_npc()` must have seeded.
    fn stream(&self) -> &Stream {
        self.stream
            .as_ref()
            .expect("npcs stream drawn before begin_npc()")
    }
}

/// `subloc_here()` for `city_here()`: the first city directly in
/// `where_`, or 0.
fn city_here(g: &Game, where_: i32) -> i32 {
    here_list(&g.db, where_)
        .into_iter()
        .find(|&i| g.db.kind(i) == Kind::LOC && g.db.subkind(i) == Subkind::CITY)
        .unwrap_or(0)
}

/// `olympia/loc.c:all_here`: everything in `who`, at any depth, each
/// followed by what is in it.
fn all_here(g: &Game, who: i32) -> Vec<i32> {
    fn add_here(g: &Game, who: i32, l: &mut Vec<i32>) {
        assert!(g.db.valid_box(who), "add_here: invalid box {who}");
        l.push(who);
        for i in here_list(&g.db, who) {
            add_here(g, i, l);
        }
    }

    assert!(g.db.valid_box(who), "all_here: invalid box {who}");
    let mut l = Vec::new();
    for i in here_list(&g.db, who) {
        add_here(g, i, &mut l);
    }
    l
}

/// `get_exit_dir()`: the exit in direction `dir`, if there is one.
fn get_exit_dir(l: &[ExitView], dir: i32) -> Option<&ExitView> {
    l.iter().find(|e| e.direction == dir)
}

impl Game {
    /// `begin_npc()`: seeds the `npcs` stream for location `where_`.
    pub fn begin_npc(&mut self, where_: i32) {
        let turn = i32::from(self.system.sysclock.turn);
        self.npc_rng.stream = Some(
            Stream::new(self.seed.master_seed())
                .child(turn, TAG_TURN)
                .child(where_, TAG_NPC),
        );
    }

    /// `npc_spawn()`: a keyed spawn roll on the `npcs` stream.
    #[must_use]
    pub fn npc_spawn(&self, k1: i32, k2: i32, low: i32, high: i32) -> i32 {
        self.npc_rng.stream().keyed(k1, k2, NTAG_SPAWN, low, high)
    }

    /// `npc_qty()`: a keyed spawn-quantity roll on the `npcs` stream.
    #[must_use]
    pub fn npc_qty(&self, k1: i32, k2: i32, low: i32, high: i32) -> i32 {
        self.npc_rng.stream().keyed(k1, k2, NTAG_QTY, low, high)
    }

    /// `npc_behavior()`: a keyed behavior roll on the `npcs` stream.
    #[must_use]
    pub fn npc_behavior(&self, k1: i32, k2: i32, low: i32, high: i32) -> i32 {
        self.npc_rng.stream().keyed(k1, k2, NTAG_BEHAVE, low, high)
    }

    /// `controlled_humans_here()`: whether anything in `where_`, at any
    /// depth, is a sworn character of no subkind.
    pub(crate) fn controlled_humans_here(&self, where_: i32) -> bool {
        all_here(self, where_).into_iter().any(|i| {
            self.db.kind(i) == Kind::CHAR
                && self.db.subkind(i) == Subkind::default()
                && loyal_kind(&self.db, i) != crate::swear::LOY_UNSWORN
        })
    }

    /// `choose_npc_direction()`: a land exit from `where_` for `who` to
    /// take: nine times in ten the way it last went (`dir`), if it can,
    /// else the first of the exits as the `exdr` stream shuffled them.
    fn choose_npc_direction(&mut self, who: i32, where_: i32, dir: i32) -> Option<ExitView> {
        let l = self.exits_from_loc_nsew_select(who, where_, LAND, true);
        if l.is_empty() {
            return None;
        }

        // A 90% chance an NPC keeps going in the same direction.
        self.begin_npc(where_);
        if dir != 0 && self.npc_behavior(who, dir, 1, 10) < 10 {
            if let Some(e) = get_exit_dir(&l, dir) {
                return Some(e.clone());
            }
        }

        l.into_iter().next()
    }

    /// `olympia/npc.c:npc_move`: queues a move for `who`: out, from below
    /// a province, or on to a neighboring province.
    pub fn npc_move(&mut self, who: i32) {
        let where_ = subloc(&self.db, who);

        if loc_depth(&self.db, where_) != LOC_PROVINCE {
            queue(&mut self.db, who, b"move out");
            return;
        }

        let last = npc_last_dir(&self.db, who);
        if let Some(e) = self.choose_npc_direction(who, where_, last) {
            self.db.p_misc(who).npc_dir = e.direction as i8;
            let dir = FULL_DIR_S[usize::try_from(e.direction).expect("exit direction")];
            queue(&mut self.db, who, format!("move {dir}").as_bytes());
        }
    }

    /// `auto_unsworn()`: half the time an unsworn character moves: into a
    /// city here, half the time there is one, else on.
    fn auto_unsworn(&mut self, who: i32) {
        let where_ = subloc(&self.db, who);

        if loc_depth(&self.db, where_) == LOC_BUILD {
            return;
        }

        self.begin_npc(where_);
        if self.npc_behavior(who, 0, 1, 2) == 1 {
            let n = city_here(self, where_);
            if n != 0 && self.npc_behavior(who, 1, 1, 2) == 1 {
                queue(
                    &mut self.db,
                    who,
                    format!("move {}", int_to_code(n)).as_bytes(),
                );
            } else {
                self.npc_move(who);
            }
        }
    }

    /// `auto_mob()`: a mob away from home disperses; at home it may, half
    /// the time, once it has stood five turns.
    fn auto_mob(&mut self, who: i32) {
        let Some((home, created)) = self
            .db
            .get(who)
            .and_then(|b| b.misc.as_ref())
            .map(|m| (m.npc_home, m.npc_created))
        else {
            eprintln!("warning: mob's rp_misc is NULL, who={who}");
            return;
        };

        // A mob left unstacked shows in the end-of-turn location report,
        // so someone may still rally it before it disperses.
        let where_ = subloc(&self.db, who);
        self.begin_npc(where_);
        if where_ != home
            || (i32::from(self.system.sysclock.turn) - created >= 5
                && self.npc_behavior(who, 0, 1, 2) == 1)
        {
            let t = self.npc_behavior(who, 1, 10, 20);
            queue(&mut self.db, who, format!("wait time {t}").as_bytes());
            queue(&mut self.db, who, b"reclaim \"disperses.\"");
        }
    }

    /// `auto_bandit()`: a bandit unstacks whoever is under it, then
    /// attacks a visible character here that isn't an NPC, or moves on.
    fn auto_bandit(&mut self, who: i32, prog: i32) {
        let where_ = subloc(&self.db, who);

        for i in here_list(&self.db, who) {
            queue(
                &mut self.db,
                who,
                format!("unstack {}", int_to_code(i)).as_bytes(),
            );
        }

        let db = &self.db;
        let targets: Vec<i32> = here_list(db, where_)
            .into_iter()
            .filter(|&i| db.kind(i) == Kind::CHAR && !is_npc(db, i) && !char_really_hidden(db, i))
            .filter(|&i| {
                !(prog == PROG_FAERY_BANDIT && stack_has_use_key(db, i, USE_FAERY_STONE) != 0)
            })
            .filter(|&i| {
                !(prog == PROG_HADES_BANDIT
                    && has_skill(db, i, SK_TRANSCEND_DEATH) != 0
                    && char_alone(db, i))
            })
            .collect();

        let mut victim = 0;
        if !targets.is_empty() {
            // A keyed uniform pick on the npcs stream, keyed on the bandit.
            self.begin_npc(where_);
            let last = i32::try_from(targets.len()).expect("target count") - 1;
            let k = self.npc_rng.stream().keyed(who, 0, NTAG_VICTIM, 0, last);
            victim = targets[usize::try_from(k).expect("victim index")];
        }

        if victim != 0 {
            queue(
                &mut self.db,
                who,
                format!("attack {}", int_to_code(victim)).as_bytes(),
            );
        } else {
            self.npc_move(who);
        }
    }

    /// `olympia/necro.c:auto_undead`: a demon lord attacks its summoner
    /// if they meet, and is free of it; otherwise it moves on, from below
    /// a province or half the time, or pillages.
    fn auto_undead(&mut self, who: i32) {
        let where_ = subloc(&self.db, who);
        let master = npc_summoner(&self.db, who);

        if master != 0 && subloc(&self.db, who) == subloc(&self.db, master) {
            queue(
                &mut self.db,
                who,
                format!("attack {}", int_to_code(master)).as_bytes(),
            );
            self.db.p_misc(who).summoned_by = 0;
            return;
        }

        self.begin_npc(where_);
        if loc_depth(&self.db, where_) != LOC_PROVINCE || self.npc_behavior(who, 0, 1, 2) == 1 {
            self.npc_move(who);
            return;
        }

        queue(&mut self.db, who, b"pillage 1");
    }

    /// `create_hades_bandit()`: bandits of a random kind that ambush `who`
    /// entering `where_`. Returns the new unit, or -1 if there is no
    /// entity number free.
    fn create_hades_bandit(&mut self, who: i32, where_: i32) -> Result<i32, NotYetImplemented> {
        let kind = self.hads_bandit_kind(who, where_);
        let item = match kind {
            1 => ITEM_SPIRIT,
            2 => ITEM_CORPSE,
            3 => ITEM_SAVAGE,
            4 => ITEM_SKELETON,
            5 => ITEM_GORGON,
            _ => unreachable!("hads_bandit_kind {kind}"),
        };
        self.create_bandit(item, where_, PROG_HADES_BANDIT)
    }

    /// `create_faery_bandit()`: faeries or elves that ambush `who`
    /// entering `where_`, with some gold. Returns the new unit, or -1 if
    /// there is no entity number free.
    fn create_faery_bandit(&mut self, who: i32, where_: i32) -> Result<i32, NotYetImplemented> {
        let kind = self.faer_bandit_kind(who, where_);
        let item = match kind {
            1 | 2 => ITEM_FAERY,
            3 => ITEM_ELF,
            _ => unreachable!("faer_bandit_kind {kind}"),
        };
        let new = self.create_bandit(item, where_, PROG_FAERY_BANDIT)?;
        if new >= 0 {
            // TODO(stage 2): wout(where, "%s appear.")
            let gold = self.faer_bandit_gold(new);
            self.gen_item(new, ITEM_GOLD, gold);
        }
        Ok(new)
    }

    /// The common part of `create_hades_bandit()` and
    /// `create_faery_bandit()`: a unit of `item` running program `prog`,
    /// with its troops.
    fn create_bandit(
        &mut self,
        item: i32,
        where_: i32,
        prog: i32,
    ) -> Result<i32, NotYetImplemented> {
        let pl = self.system.indep_player;
        let new = self.new_char(SUB_NI, item, where_, -1, pl, LOY_NPC, 0, None)?;
        // C sets the break point before checking for -1.
        if new < 0 {
            return Ok(-1);
        }
        let c = self.db.p_char(new);
        c.break_point = 0;
        c.npc_prog = prog as i8;

        let qty = if prog == PROG_HADES_BANDIT {
            self.hads_bandit_qty(new)
        } else {
            self.faer_bandit_qty(new)
        };
        self.gen_item(new, item, qty);
        // TODO(stage 2): wout(where, "%s appear.") (for Faery, after the
        // troops and before the gold)
        Ok(new)
    }

    /// The common tail of the ambush checks: the new bandits wait, ready
    /// to run their orders at once, and if `retal` they attack `who`.
    fn ambush(&mut self, new: i32, who: i32, retal: impl FnOnce(&mut Self) -> bool) -> Phase {
        queue(&mut self.db, new, b"wait time 0");
        self.init_load_sup(new)?; // ready to run commands at once
        if retal(self) {
            queue(
                &mut self.db,
                new,
                format!("attack {}", int_to_code(who)).as_bytes(),
            );
        }
        Ok(())
    }

    /// `olympia/npc.c:hades_attack_check`: a stack entering a province of
    /// Hades may be ambushed: six times in a hundred, unless it is an NPC,
    /// a hidden lone character, or a lone transcender of death.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if making the bandits reaches
    /// something not ported.
    pub fn hades_attack_check(&mut self, who: i32, where_: i32) -> Phase {
        if self.hads_ambush(who, where_) > 6 {
            return Ok(());
        }
        let db = &self.db;
        if is_npc(db, who)
            || db.kind(who) != Kind::CHAR
            || char_really_hidden(db, who)
            || (has_skill(db, who, SK_TRANSCEND_DEATH) != 0 && char_alone(db, who))
        {
            return Ok(());
        }

        let new = self.create_hades_bandit(who, where_)?;
        if new < 0 {
            return Ok(());
        }
        self.ambush(new, who, |g| g.hads_retal(who, where_) == 1)
    }

    /// `olympia/npc.c:faery_attack_check`: a stack entering a province of
    /// Faery may be ambushed: six times in a hundred, unless it is an NPC,
    /// a hidden lone character, or carries a Faery stone.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if making the bandits reaches
    /// something not ported.
    pub fn faery_attack_check(&mut self, who: i32, where_: i32) -> Phase {
        if self.faer_ambush(who, where_) > 6 {
            return Ok(());
        }
        let db = &self.db;
        if is_npc(db, who)
            || db.kind(who) != Kind::CHAR
            || char_really_hidden(db, who)
            || stack_has_use_key(db, who, USE_FAERY_STONE) != 0
        {
            return Ok(());
        }

        let new = self.create_faery_bandit(who, where_)?;
        if new < 0 {
            return Ok(());
        }
        self.ambush(new, who, |g| g.faer_retal(who, where_) == 1)
    }

    /// `olympia/npc.c:queue_npc_orders`: queues the NPCs' orders for the
    /// turn: the savage attacks, the Hades monsters, then a program for
    /// each independent unit that is free and has nothing queued.
    ///
    /// # Panics
    ///
    /// Panics on a unit with an unknown NPC program, as C asserts.
    pub(crate) fn queue_npc_orders(&mut self) -> Phase {
        crate::game::stage("queue_npc_orders()");

        self.init_savage_attacks()?;
        self.auto_hades()?;

        let indep = self.system.indep_player;
        // loop_units walks a copy of the list.
        let units = self
            .db
            .get(indep)
            .and_then(|b| b.player.as_ref())
            .map(|p| p.units.as_slice().to_vec())
            .unwrap_or_default();
        for who in units {
            if loyal_kind(&self.db, who) == LOY_SUMMON {
                continue;
            }
            if is_prisoner(&self.db, who) {
                continue;
            }
            if self.rp_command(who).is_some_and(|c| c.state != STATE_DONE) {
                continue; // running an order
            }
            if top_order(&self.db, indep, who).is_some() {
                continue; // orders already queued
            }

            match npc_program(&self.db, who) {
                0 => match self.db.subkind(who) {
                    Subkind(0) => self.auto_unsworn(who),
                    SUB_UNDEAD => self.auto_undead(who),
                    SUB_NI => match noble_item(&self.db, who) {
                        ITEM_SAVAGE => self.auto_savage(who),
                        ITEM_PEASANT | ITEM_ANGRY_PEASANT => self.auto_mob(who),
                        _ => {}
                    },
                    _ => {}
                },
                PROG_BANDIT | PROG_HADES_BANDIT | PROG_FAERY_BANDIT => {
                    let prog = npc_program(&self.db, who);
                    self.auto_bandit(who, prog);
                }
                PROG_SUBLOC_MONSTER => {}
                PROG_NPC_TOKEN => self.npc_move(who),
                prog => panic!("queue_npc_orders: {who} has NPC program {prog}"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The `npcs` draws for turn 3 under this master seed, from npc.c's
    /// helpers run over lib/rng.c.
    #[test]
    fn npc_draws_match_c() {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new([0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210]);
        g.system.sysclock.turn = 3;

        g.begin_npc(20101);
        assert_eq!(g.npc_spawn(20101, 0, 1, 100), 13);
        assert_eq!(g.npc_qty(20101, 80000, 3, 25), 17);
        assert_eq!(g.npc_behavior(1234, 0, 1, 2), 1);
        assert_eq!(g.npc_behavior(1234, 2, 1, 10), 9);
        assert_eq!(g.npc_rng.stream().keyed(1234, 0, NTAG_VICTIM, 0, 6), 2);

        g.begin_npc(63000);
        assert_eq!(g.npc_spawn(63000, 0, 1, 100), 69);
        assert_eq!(g.npc_behavior(1234, 0, 35, 50), 49);
    }
}
