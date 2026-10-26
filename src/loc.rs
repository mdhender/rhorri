// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Location depth and containment (`olympia/loc.c`, `olympia/u.c:loc_depth`).

use crate::entity::{Db, Kind, Subkind};

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
