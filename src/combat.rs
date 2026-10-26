// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The parts of combat the command driver reaches every day
//! (`olympia/combat.c`): the auto-attack check and the second waits.
//!
//! Combat itself is stage 4. The auto-attack check is ported whole, so it
//! finds the same targets C does, and a unit that would attack stops the
//! run with "not yet implemented".

use crate::check::{stack_leader, stack_parent};
use crate::entity::Kind;
use crate::game::{Game, player};
use crate::loc::{
    LOC_BUILD, building_owner, first_character, here_list, in_safe_now, is_loc_or_ship, is_ship,
    loc_depth, loc_hidden, subloc,
};
use crate::lore::test_known;
use crate::turn::{Phase, not_ported};
use crate::unit::{
    char_gone, char_health, char_here, has_item, is_npc, is_prisoner, npc_program, only_defeatable,
};

/// `PROG_subloc_monster`: an NPC that attacks everyone who enters.
const PROG_SUBLOC_MONSTER: i32 = 2;

impl Game {
    /// `clear_second_waits()`: releases the units an auto-attack held back
    /// today.
    pub(crate) fn clear_second_waits(&mut self) {
        for who in std::mem::take(&mut self.driver.second_wait_list) {
            self.command(who).second_wait = false;
        }
    }

    /// `check_all_auto_attacks()`: each healthy stack leader attacks the
    /// first hostile target where it is.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if a
    /// unit would attack: combat is stage 4.
    pub(crate) fn check_all_auto_attacks(&mut self) -> Phase {
        let mut i = self.db.kind_first(Kind::CHAR);
        while i > 0 {
            let next = self.db.kind_next(i);
            // Only a stack leader may start an auto-attack.
            let health = char_health(&self.db, i);
            if stack_parent(&self.db, i) == 0 && (health == 100 || health == -1) {
                self.check_auto_attack_sup(i)?;
            }
            i = next;
        }
        Ok(())
    }

    /// `check_auto_attack_sup()`: finds `who`'s first auto-attack target:
    /// a hostile character it can see, or a hostile location or ship with
    /// someone of another faction in charge; then, aboard a ship, a hostile
    /// ship alongside.
    fn check_auto_attack_sup(&mut self, who: i32) -> Phase {
        let db = &self.db;
        let where_ = subloc(db, who);

        if in_safe_now(db, who) || char_gone(db, who) {
            return Ok(());
        }

        for i in here_list(db, where_) {
            if !is_hostile(self, who, i) {
                continue;
            }
            let target = if db.kind(i) == Kind::CHAR {
                if is_prisoner(db, i) || !char_here(db, who, i) {
                    continue;
                }
                let target = stack_leader(db, i);
                if stack_leader(db, who) == target {
                    continue;
                }
                let n = only_defeatable(db, target);
                if n != 0 && has_item(db, who, n) == 0 {
                    continue;
                }
                target
            } else if is_loc_or_ship(db, i) {
                if db.kind(i) == Kind::LOC && loc_hidden(db, i) && !test_known(db, who, i) {
                    continue;
                }
                let targ_who = if loc_depth(db, i) == LOC_BUILD {
                    building_owner(db, i)
                } else {
                    first_character(db, i)
                };
                if targ_who == 0 || player(db, who) == player(db, targ_who) {
                    continue;
                }
                i
            } else {
                continue;
            };
            return auto_attack(who, target);
        }

        if is_ship(db, where_) {
            let outer = subloc(db, where_);
            for i in here_list(db, outer) {
                if i == where_ || !is_ship(db, i) || !is_hostile(self, who, i) {
                    continue;
                }
                if building_owner(db, i) == 0 {
                    continue;
                }
                return auto_attack(who, i);
            }
        }
        Ok(())
    }
}

/// `auto_attack()`: `who` attacks `target`. Stage 4.
fn auto_attack(_who: i32, _target: i32) -> Phase {
    // out(who, "> [auto-attack %s]") and regular_combat().
    not_ported("auto_attack (regular_combat)")
}

/// `olympia/perm.c:is_hostile`: whether `who` is hostile to `targ`: a
/// subloc monster to every non-NPC (unless only an item defeats it), a
/// garrison to those on its hostile list, and anyone to those on its own or
/// its faction's hostile list. Never to its own faction.
#[must_use]
pub fn is_hostile(g: &Game, who: i32, targ: i32) -> bool {
    let db = &g.db;
    if player(db, who) == player(db, targ) {
        return false;
    }
    if npc_program(db, who) == PROG_SUBLOC_MONSTER
        && !is_npc(db, targ)
        && only_defeatable(db, who) == 0
    {
        return true;
    }
    if db.subkind(who) == crate::entity::Subkind::GARRISON {
        let host = db.get(who).and_then(|b| b.misc.as_ref());
        if host.is_some_and(|m| m.garr_host.contains(targ)) {
            return true;
        }
    }
    let hostile = |n: i32| {
        db.get(n)
            .and_then(|b| b.disp.as_ref())
            .is_some_and(|d| d.hostile.contains(targ))
    };
    hostile(who) || hostile(player(db, who))
}
