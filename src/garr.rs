// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Garrisons (`olympia/garr.c`): their tax and the noble ranks they
//! confer. Also the month's maintenance from `olympia/day.c`
//! (`maint_cost`, `charge_maint_sup`, `men_starve`), which garrisons pay
//! too.
//!
//! TODO(stage 2): the `wout()` lines (maintenance paid, men starving,
//! taxes forwarded) and `gold_provinces`, the GM report's total.

use crate::check::STATE_RUN;
use crate::command::find_command;
use crate::entity::{Db, Kind, Subkind};
use crate::game::{Game, stage};
use crate::loc::{
    LOC_PROVINCE, LOC_REGION, building_owner, garrison_here, here_list, is_ship, loc_depth,
    province_subloc, subloc,
};
use crate::seed::ITEM_TAX_COOKIE;
use crate::turn::Phase;
use crate::unit::{
    ITEM_GOLD, char_pledge, char_rank, default_garrison, has_item, inventory, item_attack,
    item_defense, man_item, noble_item, stack_has_item,
};

/// `item_peasant`.
const ITEM_PEASANT: i32 = 10;
/// `item_worker`.
const ITEM_WORKER: i32 = 11;
/// `item_soldier`.
const ITEM_SOLDIER: i32 = 12;
/// `item_archer`.
const ITEM_ARCHER: i32 = 13;
/// `item_knight`.
const ITEM_KNIGHT: i32 = 14;
/// `item_elite_guard`.
const ITEM_ELITE_GUARD: i32 = 15;
/// `item_pikeman`.
const ITEM_PIKEMAN: i32 = 16;
/// `item_blessed_soldier`.
const ITEM_BLESSED_SOLDIER: i32 = 17;
/// `item_sailor`.
const ITEM_SAILOR: i32 = 19;
/// `item_swordsman`.
const ITEM_SWORDSMAN: i32 = 20;
/// `item_crossbowman`.
const ITEM_CROSSBOWMAN: i32 = 21;
/// `item_elite_arch`.
const ITEM_ELITE_ARCH: i32 = 22;
/// `item_angry_peasant`.
const ITEM_ANGRY_PEASANT: i32 = 23;
/// `item_pirate`.
const ITEM_PIRATE: i32 = 24;

/// `RANK_lord` .. `RANK_king` (`olympia/garr.h`).
const RANK_LORD: i32 = 10;
const RANK_KNIGHT: i32 = 20;
const RANK_BARON: i32 = 30;
const RANK_COUNT: i32 = 40;
const RANK_EARL: i32 = 50;
const RANK_MARQUESS: i32 = 60;
const RANK_DUKE: i32 = 70;
const RANK_KING: i32 = 80;

/// `olympia/day.c:maint_cost`: the monthly upkeep, in gold, of one of
/// `item`.
#[must_use]
pub fn maint_cost(item: i32) -> i32 {
    match item {
        ITEM_PEASANT => 1,
        ITEM_WORKER | ITEM_SOLDIER | ITEM_SAILOR | ITEM_ANGRY_PEASANT | ITEM_CROSSBOWMAN => 2,
        ITEM_BLESSED_SOLDIER | ITEM_PIKEMAN | ITEM_SWORDSMAN | ITEM_PIRATE | ITEM_ARCHER => 3,
        ITEM_KNIGHT | ITEM_ELITE_ARCH => 4,
        ITEM_ELITE_GUARD => 5,
        _ => 0,
    }
}

/// `olympia/garr.c:garrison_strength`: the paid men in a garrison at least
/// as strong as soldiers.
///
/// # Panics
///
/// Panics if `garr` isn't a valid box, as C asserts.
#[must_use]
pub fn garrison_strength(db: &Db, garr: i32) -> i32 {
    assert!(db.valid_box(garr), "loop_inv: invalid box {garr}");
    let (attack, defense) = (
        item_attack(db, ITEM_SOLDIER),
        item_defense(db, ITEM_SOLDIER),
    );
    db.entity(garr)
        .items
        .iter()
        // loop_inv skips invalid items and empty entries.
        .filter(|e| db.valid_box(e.item) && e.qty > 0)
        .filter(|e| {
            man_item(db, e.item)
                && maint_cost(e.item) > 0
                && item_attack(db, e.item) >= attack
                && item_defense(db, e.item) >= defense
        })
        .map(|e| e.qty)
        .sum()
}

/// `garrison_castle()`: the castle a garrison holds its province for.
#[must_use]
pub fn garrison_castle(db: &Db, n: i32) -> i32 {
    db.get(n)
        .and_then(|b| b.misc.as_deref())
        .map_or(0, |m| m.garr_castle)
}

/// `olympia/garr.c:province_admin`: who rules province `n` (or the
/// province of garrison `n`): the owner of the castle its garrison holds
/// it for, or 0.
///
/// # Panics
///
/// Panics if `n` is a location but not a province, or neither a location
/// nor a garrison, as C asserts.
#[must_use]
pub fn province_admin(db: &Db, n: i32) -> i32 {
    let garr = if db.kind(n) == Kind::LOC {
        assert_eq!(loc_depth(db, n), LOC_PROVINCE, "province_admin: {n}");
        let garr = garrison_here(db, n);
        if garr == 0 {
            return 0;
        }
        garr
    } else {
        assert_eq!(db.subkind(n), Subkind::GARRISON, "province_admin: {n}");
        n
    };
    let castle = garrison_castle(db, garr);
    if !db.valid_box(castle) {
        return 0;
    }
    building_owner(db, castle)
}

/// `loop_loc_owner`: the chain of rulers over `where_`: its
/// administrator, the noble that one is pledged to, and so on up.
fn loc_owners(db: &Db, where_: i32) -> Vec<i32> {
    let mut l = Vec::new();
    let mut next = province_admin(db, where_);
    while next > 0 {
        l.push(next);
        next = char_pledge(db, next);
    }
    l
}

/// `olympia/garr.c:top_ruler`: the noble at the top of the chain of
/// rulers over `n`, or 0.
#[must_use]
pub fn top_ruler(db: &Db, n: i32) -> i32 {
    loc_owners(db, n).last().copied().unwrap_or(0)
}

/// `olympia/garr.c:nprovs_to_rank`: the rank that ruling `n` provinces
/// gives.
fn nprovs_to_rank(n: i32) -> i32 {
    match n {
        ..1 => 0,
        1..=5 => RANK_LORD,
        6..=12 => RANK_KNIGHT,
        13..=24 => RANK_BARON,
        25..=37 => RANK_COUNT,
        38..=50 => RANK_EARL,
        51..=63 => RANK_MARQUESS,
        _ => RANK_DUKE,
    }
}

/// `olympia/garr.c:lower_noble_rank`: the rank below `rank`.
fn lower_noble_rank(rank: i32) -> i32 {
    match rank {
        RANK_KNIGHT => RANK_LORD,
        RANK_BARON => RANK_KNIGHT,
        RANK_COUNT => RANK_BARON,
        RANK_EARL => RANK_COUNT,
        RANK_MARQUESS => RANK_EARL,
        RANK_DUKE => RANK_MARQUESS,
        RANK_KING => RANK_DUKE,
        _ => 0,
    }
}

/// `olympia/garr.c:det_noble_rank_sup`: a noble's rank is at most one
/// below that of the noble it is pledged to.
fn det_noble_rank_sup(db: &mut Db, who: i32) -> i32 {
    let own_rank = char_rank(db, who);
    let pledged_to = char_pledge(db, who);
    if pledged_to == 0 {
        return own_rank;
    }
    let rank = own_rank.min(lower_noble_rank(det_noble_rank_sup(db, pledged_to)));
    db.p_char(who).rank = rank as i8;
    char_rank(db, who)
}

/// `olympia/garr.c:find_kings`: whoever rules every province of a region
/// of at least 15 is its king.
fn find_kings(db: &mut Db) {
    for reg in db.boxes_of_kind(Kind::LOC) {
        if loc_depth(db, reg) != LOC_REGION {
            continue;
        }
        let mut ruler = -1;
        let mut nprovs = 0;
        for where_ in here_list(db, reg) {
            if db.kind(where_) != Kind::LOC {
                continue;
            }
            nprovs += 1;
            if ruler == -1 {
                ruler = top_ruler(db, where_);
                if ruler == 0 {
                    break; // fail
                }
            } else if ruler != top_ruler(db, where_) {
                ruler = 0;
                break; // fail
            }
        }
        if ruler != 0 && nprovs >= 15 {
            db.p_char(ruler).rank = RANK_KING as i8;
        }
    }
}

/// `olympia/day.c:unit_maint_cost`: what `who`'s men cost a month. A
/// noble that stands for an item doesn't pay for itself.
fn unit_maint_cost(db: &Db, who: i32) -> i32 {
    let ni = noble_item(db, who);
    inventory(db, who)
        .into_iter()
        .filter(|e| e.item != ni)
        .map(|e| maint_cost(e.item) * e.qty)
        .sum()
}

impl Game {
    /// `olympia/day.c:charge_maint_sup`: `who`'s stack pays its men's
    /// maintenance. Returns 0 if it owes nothing, 1 if it paid in full and
    /// 2 if men starved.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// losing men reaches something not ported.
    pub(crate) fn charge_maint_sup(
        &mut self,
        who: i32,
    ) -> Result<i32, crate::game::NotYetImplemented> {
        let cost = unit_maint_cost(&self.db, who);
        if cost < 1 {
            return Ok(0);
        }
        if self.autocharge(who, cost) {
            // TODO(stage 2): "Paid maintenance of %s."
            return Ok(1);
        }
        let have = stack_has_item(&self.db, who, ITEM_GOLD);
        // TODO(stage 2): "Maintenance costs are %s, can afford %s."
        self.men_starve(who, have)?;
        Ok(2)
    }

    /// `olympia/day.c:men_starve`: `who` pays what it can, a man at a time
    /// round its kinds of men, and a third of the men it can't pay leave,
    /// taken a man at a time round the kinds from the first.
    ///
    /// # Panics
    ///
    /// Panics if no man goes unpaid, as C asserts.
    fn men_starve(&mut self, who: i32, have: i32) -> Phase {
        let mut item = Vec::new();
        let mut qty = Vec::new();
        let mut cost = Vec::new();
        let mut nmen = 0;
        for e in inventory(&self.db, who) {
            let n = maint_cost(e.item);
            if n != 0 {
                item.push(e.item);
                qty.push(e.qty);
                cost.push(n);
                nmen += e.qty;
            }
        }
        let mut starve = vec![0; item.len()];

        let mut have = have;
        let mut gold = have;
        let mut npaid = 0;
        loop {
            let mut hit_one = false;
            for i in 0..item.len() {
                if qty[i] > 0 && have >= cost[i] {
                    have -= cost[i];
                    qty[i] -= 1;
                    npaid += 1;
                    hit_one = true;
                }
            }
            if !(hit_one && have > 0) {
                break;
            }
        }

        gold -= have;
        let mut nstarve = nmen - npaid;
        nstarve = (nstarve + 2) / 3;
        assert!(nstarve > 0, "men_starve: {who} starves no one");

        let mut i = 0;
        let mut failcheck = 0;
        while nstarve > 0 {
            assert!(failcheck < 10000, "men_starve: failcheck");
            failcheck += 1;
            if qty[i] != 0 {
                nstarve -= 1;
                starve[i] += 1;
                qty[i] -= 1;
            }
            i += 1;
            if i >= item.len() {
                i = 0;
            }
        }

        self.autocharge(who, gold);

        for i in 0..item.len() {
            if starve[i] == 0 {
                continue;
            }
            // Peasants starve; the others left service or deserted.
            if item[i] != ITEM_PEASANT {
                let _left_service = self.up_starve(who, item[i], 1, 2) == 1;
            }
            // TODO(stage 2): "%s %s." with the men and how they went.
            self.consume_item(who, item[i], starve[i]);
            if item[i] == ITEM_SAILOR || item[i] == ITEM_PIRATE {
                self.check_captain_loses_sailors(starve[i], who, 0);
            }
        }
        Ok(())
    }

    /// `olympia/move.c:check_captain_loses_sailors`: a ship under sail that
    /// loses `qty` of its crew below what it needs takes a day longer for
    /// each sailor short. `inform` is told too.
    ///
    /// # Panics
    ///
    /// Panics on a ship that isn't a galley or a roundship, as C asserts.
    pub(crate) fn check_captain_loses_sailors(&mut self, qty: i32, target: i32, inform: i32) {
        let cmd_sail = find_command(b"sail").map_or(-1, |(i, _)| i);
        assert!(cmd_sail > 0, "check_captain_loses_sailors: no sail command");
        let where_ = subloc(&self.db, target);

        if !is_ship(&self.db, subloc(&self.db, target)) {
            return;
        }
        match self.rp_command(target) {
            Some(c) if c.state == STATE_RUN && c.cmd == cmd_sail => {}
            _ => return,
        }

        let now = has_item(&self.db, target, ITEM_SAILOR) + has_item(&self.db, target, ITEM_PIRATE);
        let mut before = now + qty;

        let should_have = match self.db.subkind(where_) {
            Subkind::GALLEY => 14,
            Subkind::ROUNDSHIP => 8,
            sk => {
                eprintln!("kind is {}", sk.0);
                panic!("check_captain_loses_sailors: kind is {}", sk.0);
            }
        };

        if now >= should_have {
            return; // still have enough sailors
        }
        if before > should_have {
            before = should_have;
        }
        let penalty = before - now;
        assert!(
            penalty > 0,
            "check_captain_loses_sailors: penalty {penalty}"
        );

        // TODO(stage 2): "Loss of crew will cause travel to take ..." to
        // target and inform, and log_write(LOG_SPECIAL, "Loss of sailors
        // incurs penalty for %s.").
        let _ = inform;
        let c = self.command(target);
        assert!(c.wait > 0, "check_captain_loses_sailors: wait {}", c.wait);
        c.wait += penalty;
    }

    /// `olympia/garr.c:garrison_gold`: each garrison (but a default one)
    /// takes its province's tax base, pays its men from it, and sends on
    /// what is left: all of it to a castle in the province, or else half to
    /// the owner of the castle it holds the province for. A garrison of
    /// fewer than ten soldiers sends nothing.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// men starving reaches something not ported.
    ///
    /// # Panics
    ///
    /// Panics if a garrison isn't in a province, as C asserts.
    pub(crate) fn garrison_gold(&mut self) -> Phase {
        // clear_temps(T_loc)
        for i in self.db.boxes_of_kind(Kind::LOC) {
            self.db.entity_mut(i).temp = 0;
        }

        for garr in self.db.boxes_of_subkind(Subkind::GARRISON) {
            if default_garrison(&self.db, garr) {
                continue;
            }

            // The garrisoned province's tax base, as gold for the
            // maintenance charge.
            let where_ = subloc(&self.db, garr);
            assert_eq!(
                loc_depth(&self.db, where_),
                LOC_PROVINCE,
                "garrison_gold: {garr} not in a province"
            );
            let base = has_item(&self.db, where_, ITEM_TAX_COOKIE);
            self.consume_item(where_, ITEM_TAX_COOKIE, base);
            self.gen_item(garr, ITEM_GOLD, base);
            self.db.p_misc(garr).garr_tax = base;

            // What maintenance took; the garrison may have dipped into
            // gold of its own.
            let has_before = has_item(&self.db, garr, ITEM_GOLD);
            self.charge_maint_sup(garr)?;
            let has_now = has_item(&self.db, garr, ITEM_GOLD);
            let spent = has_before - has_now;
            if spent >= base {
                continue; // spent the entire tax base, or more
            }

            let mut remain = base - spent;
            self.consume_item(garr, ITEM_GOLD, remain);

            // Under-strength garrisons don't forward tax.
            if garrison_strength(&self.db, garr) < 10 {
                continue;
            }

            // A castle in the province gets the rest of the tax base.
            if province_subloc(&self.db, where_, Subkind::CASTLE) != 0 {
                self.gen_item(where_, ITEM_TAX_COOKIE, remain);
                self.db.p_misc(garr).garr_forward = -1;
                continue;
            }

            remain /= 2; // 50% of what remains goes to the land's owner
            let castle = garrison_castle(&self.db, garr);
            if castle != 0 && remain > 0 {
                self.db.entity_mut(castle).temp += remain;
                self.db.p_misc(garr).garr_forward = remain;
            }
        }

        for i in self.db.boxes_of_kind(Kind::LOC) {
            if self.db.subkind(i) != Subkind::CASTLE {
                continue;
            }
            let owner = building_owner(&self.db, i);
            let temp = self.db.entity(i).temp;
            if owner == 0 || temp == 0 {
                continue;
            }
            // TODO(stage 2): "Collected %s from owned provinces.", and
            // gold_provinces += temp.
            self.gen_item(owner, ITEM_GOLD, temp);
        }
        Ok(())
    }

    /// `olympia/garr.c:determine_noble_ranks`: a noble's rank comes from
    /// the provinces it rules through garrisons of ten or more soldiers,
    /// and its liege's: a vassal ranks at least one below its lord.
    pub(crate) fn determine_noble_ranks(&mut self) {
        stage("determine_noble_ranks()");
        let db = &mut self.db;

        // clear_temps(T_char)
        for i in db.boxes_of_kind(Kind::CHAR) {
            db.entity_mut(i).temp = 0;
        }

        for garr in db.boxes_of_subkind(Subkind::GARRISON) {
            if garrison_strength(db, garr) < 10 {
                continue;
            }
            for owner in loc_owners(db, garr) {
                db.entity_mut(owner).temp += 1;
            }
        }

        for who in db.boxes_of_kind(Kind::CHAR) {
            let rank = nprovs_to_rank(db.entity(who).temp);
            db.p_char(who).rank = rank as i8;
        }

        find_kings(db);

        for who in db.boxes_of_kind(Kind::CHAR) {
            if char_rank(db, who) == 0 {
                continue;
            }
            let rank = det_noble_rank_sup(db, who);
            db.p_char(who).rank = rank as i8;
        }
    }

    /// `olympia/garr.c:ping_garrisons`: a garrison of ten or more soldiers
    /// stands guard this turn; the rest don't.
    ///
    /// TODO(stage 2): `show_to_garrison` and the "%s guards %s." line to
    /// the location.
    ///
    /// # Panics
    ///
    /// Panics on a garrison with no character struct, as C asserts.
    pub(crate) fn ping_garrisons(&mut self) {
        for garr in self.db.boxes_of_subkind(Subkind::GARRISON) {
            assert!(
                self.db.entity(garr).char_.is_some(),
                "ping_garrisons: {garr} has no character struct"
            );
            let guard = garrison_strength(&self.db, garr) >= 10;
            self.db.p_char(garr).guard = i8::from(guard);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{ItemEnt, Kind};

    #[test]
    fn strength_counts_paid_men_as_strong_as_soldiers() {
        let mut db = Db::new();
        for (item, man, attack, defense) in [
            (ITEM_PEASANT, 1, 1, 1),
            (ITEM_SOLDIER, 1, 5, 5),
            (ITEM_KNIGHT, 1, 50, 50),
            (ITEM_PIKEMAN, 1, 5, 30),
            (ITEM_ARCHER, 1, 0, 0),
            (31, 1, 50, 50), // a corpse: no upkeep
        ] {
            db.alloc_box(item, Kind::ITEM, Subkind::default());
            let i = db.p_item(item);
            i.is_man_item = man;
            i.attack = attack;
            i.defense = defense;
        }
        db.alloc_box(5000, Kind::CHAR, Subkind::GARRISON);
        db.entity_mut(5000).items = [
            (ITEM_PEASANT, 100),
            (ITEM_SOLDIER, 4),
            (ITEM_KNIGHT, 3),
            (ITEM_PIKEMAN, 2),
            (ITEM_ARCHER, 9),
            (31, 7),
        ]
        .map(|(item, qty)| ItemEnt { item, qty })
        .to_vec();
        assert_eq!(garrison_strength(&db, 5000), 9);
    }
}
