// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Location depth and containment (`olympia/loc.c`, `olympia/u.c:loc_depth`).

use crate::entity::{Db, Kind, Subkind};
use crate::system::System;

/// `LOC_region`: a top-level continent or island group.
pub const LOC_REGION: i32 = 1;
/// `LOC_province`: a main location area.
pub const LOC_PROVINCE: i32 = 2;
/// `LOC_subloc`: an inner sublocation.
pub const LOC_SUBLOC: i32 = 3;
/// `LOC_build`: a building, structure or ship.
pub const LOC_BUILD: i32 = 4;

/// `loc_depth()`: how deep in the location tree a location of this subkind
/// sits.
///
/// # Panics
///
/// Panics for a subkind that isn't a location, as C asserts (after printing
/// the subkind on standard error).
#[must_use]
pub fn loc_depth(db: &Db, n: i32) -> i32 {
    let sk = db.subkind(n);
    match sk {
        Subkind::REGION => LOC_REGION,
        Subkind::OCEAN
        | Subkind::FOREST
        | Subkind::PLAIN
        | Subkind::MOUNTAIN
        | Subkind::DESERT
        | Subkind::SWAMP
        | Subkind::UNDER
        | Subkind::CLOUD
        | Subkind::TUNNEL
        | Subkind::CHAMBER => LOC_PROVINCE,
        Subkind::ISLAND
        | Subkind::STONE_CIR
        | Subkind::MALLORN_GROVE
        | Subkind::BOG
        | Subkind::CAVE
        | Subkind::CITY
        | Subkind::LAIR
        | Subkind::GRAVEYARD
        | Subkind::RUINS
        | Subkind::BATTLEFIELD
        | Subkind::ENCH_FOREST
        | Subkind::ROCKY_HILL
        | Subkind::TREE_CIRCLE
        | Subkind::PITS
        | Subkind::PASTURE
        | Subkind::OASIS
        | Subkind::YEW_GROVE
        | Subkind::SAND_PIT
        | Subkind::SACRED_GROVE
        | Subkind::POPPY_FIELD
        | Subkind::FAERY_HILL
        | Subkind::HADES_PIT => LOC_SUBLOC,
        Subkind::TEMPLE
        | Subkind::GALLEY
        | Subkind::ROUNDSHIP
        | Subkind::CASTLE
        | Subkind::GALLEY_NOTDONE
        | Subkind::ROUNDSHIP_NOTDONE
        | Subkind::GHOST_SHIP
        | Subkind::TEMPLE_NOTDONE
        | Subkind::INN
        | Subkind::INN_NOTDONE
        | Subkind::CASTLE_NOTDONE
        | Subkind::MINE
        | Subkind::MINE_NOTDONE
        | Subkind::MINE_COLLAPSED
        | Subkind::TOWER
        | Subkind::TOWER_NOTDONE
        | Subkind::SEWER => LOC_BUILD,
        _ => {
            eprintln!("subkind is {}", sk.0);
            panic!("loc_depth: {n} has no location subkind");
        }
    }
}

/// `subloc()`: the innermost location or ship that holds `who`.
#[must_use]
pub fn subloc(db: &Db, mut who: i32) -> i32 {
    loop {
        who = db.loc(who);
        if !(who > 0 && db.kind(who) != Kind::LOC && db.kind(who) != Kind::SHIP) {
            return who;
        }
    }
}

/// `viewloc()`: steps out from `who` to the level it is seen from.
/// Provinces see into everything except cities, graveyards, sewers and
/// faery hills.
#[must_use]
pub fn viewloc(db: &Db, mut who: i32) -> i32 {
    while who > 0
        && loc_depth(db, who) != LOC_PROVINCE
        && !matches!(
            db.subkind(who),
            Subkind::CITY | Subkind::GRAVEYARD | Subkind::SEWER | Subkind::FAERY_HILL
        )
    {
        who = db.loc(who);
    }
    who
}

/// `region()`: the region that holds `who`, or `who` if it is one.
#[must_use]
pub fn region(db: &Db, mut who: i32) -> i32 {
    while who > 0 && (db.kind(who) != Kind::LOC || loc_depth(db, who) != LOC_REGION) {
        who = db.loc(who);
    }
    who
}

/// `olympia/u.c:greater_region`: the special region (Faery, Hades,
/// Nowhere, the Cloudlands, the tunnels or the Subworld) that holds `who`,
/// or 0 if it is in the ordinary world.
#[must_use]
pub fn greater_region(db: &Db, sys: &System, who: i32) -> i32 {
    let reg = region(db, who);
    let special = [
        sys.faery_region,
        sys.hades_region,
        sys.nowhere_region,
        sys.cloud_region,
        sys.tunnel_region,
        sys.under_region,
    ];
    if special.contains(&reg) { reg } else { 0 }
}

/// `province()`: the province that holds `who`, or `who` if it is one.
#[must_use]
pub fn province(db: &Db, mut who: i32) -> i32 {
    while who > 0 && (db.kind(who) != Kind::LOC || loc_depth(db, who) != LOC_PROVINCE) {
        who = db.loc(who);
    }
    who
}

/// `is_ship()`: a finished ship.
#[must_use]
pub fn is_ship(db: &Db, n: i32) -> bool {
    matches!(
        db.subkind(n),
        Subkind::GALLEY | Subkind::ROUNDSHIP | Subkind::RAFT
    )
}

/// `is_ship_notdone()`: a ship still being built.
#[must_use]
pub fn is_ship_notdone(db: &Db, n: i32) -> bool {
    matches!(
        db.subkind(n),
        Subkind::GALLEY_NOTDONE | Subkind::ROUNDSHIP_NOTDONE | Subkind::RAFT_NOTDONE
    )
}

/// `is_loc_or_ship()`.
#[must_use]
pub fn is_loc_or_ship(db: &Db, n: i32) -> bool {
    matches!(db.kind(n), Kind::LOC | Kind::SHIP)
}

/// `safe_haven()`.
fn safe_haven(db: &Db, n: i32) -> bool {
    db.get(n)
        .and_then(|b| b.subloc.as_ref())
        .is_some_and(|s| s.safe != 0)
}

/// `in_safe_now()`: whether `who` is anywhere inside a safe haven, where
/// no combat is permitted.
#[must_use]
pub fn in_safe_now(db: &Db, mut who: i32) -> bool {
    loop {
        if safe_haven(db, who) {
            return true;
        }
        who = db.loc(who);
        if who <= 0 {
            return false;
        }
    }
}

/// `loop_here`: a copy of what is directly in `where_`.
///
/// # Panics
///
/// Panics if `where_` isn't a valid box, as C asserts.
#[must_use]
pub fn here_list(db: &Db, where_: i32) -> Vec<i32> {
    assert!(db.valid_box(where_), "loop_here: invalid box {where_}");
    db.entity(where_).loc_info.here_list.as_slice().to_vec()
}

/// `loop_char_here` (`olympia/loc.c:all_char_here`): every character in
/// `who`, depth first: each character directly in it, followed by the
/// characters stacked under that one.
///
/// # Panics
///
/// Panics if `who` isn't a valid box, as C asserts.
#[must_use]
pub fn all_char_here(db: &Db, who: i32) -> Vec<i32> {
    fn add_char_here(db: &Db, who: i32, l: &mut Vec<i32>) {
        l.push(who);
        for &i in db.entity(who).loc_info.here_list.as_slice() {
            if db.kind(i) == Kind::CHAR {
                add_char_here(db, i, l);
            }
        }
    }

    assert!(db.valid_box(who), "all_char_here: invalid box {who}");
    let mut l = Vec::new();
    for &i in db.entity(who).loc_info.here_list.as_slice() {
        if db.kind(i) == Kind::CHAR {
            add_char_here(db, i, &mut l);
        }
    }
    l
}

/// `first_character()`: the first character directly in `where_`, or 0.
#[must_use]
pub fn first_character(db: &Db, where_: i32) -> i32 {
    here_list(db, where_)
        .into_iter()
        .find(|&i| db.kind(i) == Kind::CHAR)
        .unwrap_or(0)
}

/// `olympia/garr.c:garrison_here`: the garrison of `where_`, which is
/// always its first character, or 0.
#[must_use]
pub fn garrison_here(db: &Db, where_: i32) -> i32 {
    let n = first_character(db, where_);
    if n != 0 && db.subkind(n) == Subkind::GARRISON {
        n
    } else {
        0
    }
}

/// `building_owner()`: the first character in a building.
///
/// # Panics
///
/// Panics if `where_` isn't a building, as C asserts.
#[must_use]
pub fn building_owner(db: &Db, where_: i32) -> i32 {
    assert_eq!(loc_depth(db, where_), LOC_BUILD, "building_owner: {where_}");
    first_character(db, where_)
}

/// `olympia/u.c:loc_hidden`.
#[must_use]
pub fn loc_hidden(db: &Db, n: i32) -> bool {
    db.get(n)
        .and_then(|b| b.loc.as_ref())
        .is_some_and(|l| l.hidden != 0)
}

/// `olympia/storm.c:weather_here`: the summed strength of the storms of
/// subkind `sk` over the province holding `where_`; 0 inside a building.
#[must_use]
pub fn weather_here(db: &Db, where_: i32, sk: Subkind) -> i32 {
    if loc_depth(db, where_) == LOC_BUILD {
        return 0;
    }
    let where_ = province(db, where_);
    here_list(db, where_)
        .into_iter()
        .filter(|&i| db.kind(i) == Kind::STORM && db.subkind(i) == sk)
        .map(|i| {
            // storm_strength()
            db.entity(i)
                .misc
                .as_ref()
                .map_or(0, |m| i32::from(m.storm_str))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A region 10 holding province 20, which holds city 30, which holds
    /// castle 40, which holds character 50.
    fn nested() -> Db {
        let mut db = Db::new();
        db.alloc_box(10, Kind::LOC, Subkind::REGION);
        db.alloc_box(20, Kind::LOC, Subkind::FOREST);
        db.alloc_box(30, Kind::LOC, Subkind::CITY);
        db.alloc_box(40, Kind::LOC, Subkind::CASTLE);
        db.alloc_box(50, Kind::CHAR, Subkind::default());
        db.set_where(20, 10);
        db.set_where(30, 20);
        db.set_where(40, 30);
        db.set_where(50, 40);
        db
    }

    #[test]
    fn depths() {
        let db = nested();
        assert_eq!(loc_depth(&db, 10), LOC_REGION);
        assert_eq!(loc_depth(&db, 20), LOC_PROVINCE);
        assert_eq!(loc_depth(&db, 30), LOC_SUBLOC);
        assert_eq!(loc_depth(&db, 40), LOC_BUILD);
    }

    #[test]
    fn subloc_and_viewloc() {
        let db = nested();
        assert_eq!(subloc(&db, 50), 40);
        // A castle is seen from the city it is in, which is seen from itself.
        assert_eq!(viewloc(&db, 40), 30);
        assert_eq!(viewloc(&db, 30), 30);
        assert_eq!(viewloc(&db, 20), 20);
    }
}
