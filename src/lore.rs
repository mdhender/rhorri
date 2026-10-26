// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! What a faction knows, and the lore sheets it is owed
//! (`olympia/lore.c`, `olympia/u.c:test_known`, `set_known`).

use crate::entity::{Db, Kind};
use crate::game::player;
use crate::unit::item_lore;

/// `test_known()`: whether `who`'s faction knows of box `i`.
///
/// # Panics
///
/// Panics if `who` (when not 0) or `i` isn't a valid box, as C asserts.
#[must_use]
pub fn test_known(db: &Db, who: i32, i: i32) -> bool {
    if who == 0 {
        return false;
    }
    assert!(db.valid_box(who), "test_known: invalid box {who}");
    assert!(db.valid_box(i), "test_known: invalid box {i}");
    let pl = player(db, who);
    db.get(pl)
        .and_then(|b| b.player.as_ref())
        .is_some_and(|p| p.known.contains(i))
}

/// `set_known()`: records that `who`'s faction knows of box `i`.
pub fn set_known(db: &mut Db, who: i32, i: i32) {
    if !db.valid_box(who) || !db.valid_box(i) {
        return;
    }
    let pl = player(db, who);
    if !db.valid_box(pl) {
        return;
    }
    let known = &mut db.p_player(pl).known;
    if !known.contains(i) {
        known.append(i);
    }
}

/// `queue_lore()`: owes `who`'s faction the lore sheet for `num`, unless it
/// already knows `num` and `anyway` is false.
///
/// # Panics
///
/// Panics if `who` has no faction, or `num` isn't a valid box, as C
/// asserts.
pub fn queue_lore(db: &mut Db, who: i32, num: i32, anyway: bool) {
    let pl = player(db, who);
    assert!(
        db.kind(pl) == Kind::PLAYER,
        "queue_lore: {who} has no faction"
    );
    if !anyway && test_known(db, pl, num) {
        return;
    }
    db.p_player(pl).deliver_lore.append(num);
    set_known(db, pl, num);
}

/// `scan_char_skill_lore()`: owes each faction the lore for every skill its
/// characters have.
pub fn scan_char_skill_lore(db: &mut Db) {
    for who in db.boxes_of_kind(Kind::CHAR) {
        // loop_char_skill walks a copy of the list.
        let skills: Vec<i32> = db
            .entity(who)
            .char_
            .as_ref()
            .map(|c| c.skills.iter().map(|e| e.skill).collect())
            .unwrap_or_default();
        for skill in skills {
            queue_lore(db, who, skill, false);
        }
    }
}

/// `scan_char_item_lore()`: owes each faction the lore for every item with
/// a lore sheet its characters hold.
pub fn scan_char_item_lore(db: &mut Db) {
    for who in db.boxes_of_kind(Kind::CHAR) {
        // loop_inv walks a copy, skipping invalid items and empty entries.
        let items: Vec<i32> = db
            .entity(who)
            .items
            .iter()
            .filter(|e| db.valid_box(e.item) && e.qty > 0)
            .map(|e| e.item)
            .collect();
        for item in items {
            if item_lore(db, item) != 0 && !test_known(db, who, item) {
                queue_lore(db, who, item, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{SkillEnt, Subkind};

    #[test]
    fn skill_lore_is_queued_once_per_faction() {
        let mut db = Db::new();
        db.alloc_box(100, Kind::PLAYER, Subkind::PL_REGULAR);
        db.alloc_box(600, Kind::SKILL, Subkind::default());
        db.alloc_box(610, Kind::SKILL, Subkind::default());
        for who in [1001, 1002] {
            db.alloc_box(who, Kind::CHAR, Subkind::default());
            db.p_char(who).unit_lord = 100;
        }
        db.p_char(1001).skills = vec![SkillEnt {
            skill: 600,
            ..SkillEnt::default()
        }];
        db.p_char(1002).skills = vec![
            SkillEnt {
                skill: 610,
                ..SkillEnt::default()
            },
            SkillEnt {
                skill: 600,
                ..SkillEnt::default()
            },
        ];
        db.p_player(100).known.append(610);

        scan_char_skill_lore(&mut db);

        let p = db.p_player(100);
        assert_eq!(p.deliver_lore.as_slice(), [600]);
        assert_eq!(p.known.as_slice(), [610, 600]);
    }
}
