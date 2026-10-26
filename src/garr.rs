// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Garrisons (`olympia/garr.c`), and `maint_cost` from `olympia/day.c`.

use crate::entity::{Db, Subkind};
use crate::game::Game;
use crate::unit::{item_attack, item_defense, man_item};

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

impl Game {
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
