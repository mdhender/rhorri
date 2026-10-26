// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! What locations produce each month (`olympia/produce.c`).
//!
//! The production commands (MINE, HARVEST and the like) come with stage 4.

use crate::entity::{Kind, Subkind};
use crate::game::Game;
use crate::loc::{LOC_PROVINCE, loc_depth, mine_depth};
use crate::unit::{ITEM_GOLD, has_item};

/// `MOUNTAIN_STONE`.
const MOUNTAIN_STONE: i32 = 50;
/// `POPPY_OPIUM`: what a poppy field yields untended.
const POPPY_OPIUM: i32 = 25;

/// `item_opium`.
const ITEM_OPIUM: i32 = 93;
/// `item_flotsam`.
const ITEM_FLOTSAM: i32 = 59;
/// `item_iron`.
const ITEM_IRON: i32 = 79;
/// `item_mithril`.
const ITEM_MITHRIL: i32 = 82;

/// `terr_prod[]`: what each terrain replenishes to, as (terrain, item,
/// quantity), in C's order.
const TERR_PROD: [(Subkind, i32, i32); 25] = [
    (Subkind::FOREST, 77, 30),                 // item_lumber
    (Subkind::SACRED_GROVE, 77, 5),            // item_lumber
    (Subkind::TREE_CIRCLE, 77, 5),             // item_lumber
    (Subkind::MOUNTAIN, 78, MOUNTAIN_STONE),   // item_stone
    (Subkind::ROCKY_HILL, 78, MOUNTAIN_STONE), // item_stone
    (Subkind::DESERT, 78, 10),                 // item_stone
    (Subkind::CAVE, 67, 2),                    // item_farrenstone
    (Subkind::PLAIN, 51, 5),                   // item_wild_horse
    (Subkind::PASTURE, 51, 5),                 // item_wild_horse
    (Subkind::OCEAN, 87, 50),                  // item_fish
    (Subkind::MALLORN_GROVE, 65, 2),           // item_avinia_leaf
    (Subkind::MALLORN_GROVE, 70, 2),           // item_mallorn_wood
    (Subkind::BOG, 66, 4),                     // item_spiny_root
    (Subkind::PITS, 66, 4),                    // item_spiny_root
    (Subkind::SWAMP, 66, 1),                   // item_spiny_root
    (Subkind::YEW_GROVE, 68, 5),               // item_yew
    (Subkind::GRAVEYARD, 31, 15),              // item_corpse
    (Subkind::TREE_CIRCLE, 64, 3),             // item_lana_bark
    (Subkind::SAND_PIT, 71, 1),                // item_pretus_bones
    (Subkind::SWAMP, ITEM_OPIUM, POPPY_OPIUM),
    (Subkind::POPPY_FIELD, ITEM_OPIUM, POPPY_OPIUM),
    (Subkind::FOREST, 10, 10),   // item_peasant
    (Subkind::MOUNTAIN, 10, 10), // item_peasant
    (Subkind::PLAIN, 10, 10),    // item_peasant
    (Subkind::CITY, 10, 10),     // item_peasant
];

/// `mine_prod[]`: what a mine replenishes to at each depth, as (iron,
/// gold, mithril).
const MINE_PROD: [(i32, i32, i32); 21] = [
    (0, 0, 0),
    (15, 25, 0),
    (12, 100, 0),
    (10, 200, 0),
    (8, 500, 1),
    (5, 500, 0),
    (3, 400, 0),
    (0, 150, 0),
    (0, 50, 1),
    (0, 0, 2),
    (0, 0, 8),
    (0, 0, 5),
    (0, 0, 0),
    (10, 10, 0),
    (10, 50, 2),
    (0, 10, 1),
    (0, 0, 0),
    (10, 0, 1),
    (0, 500, 0),
    (0, 0, 0),
    (0, 0, 0),
];

/// `MINE_MAX`: the deepest row of [`MINE_PROD`].
const MINE_MAX: i32 = 20;

impl Game {
    /// `replenish()`: tops `where_`'s stock of `item` up to `qty`.
    fn replenish(&mut self, where_: i32, item: i32, qty: i32) {
        let n = has_item(&self.db, where_, item);
        if n < qty {
            self.gen_item(where_, item, qty - n);
        }
    }

    /// `olympia/produce.c:mine_production`: a mine's iron, gold and
    /// mithril, by its depth.
    ///
    /// # Panics
    ///
    /// Panics if the mine has no depth, as C asserts.
    pub fn mine_production(&mut self, where_: i32) {
        let depth = mine_depth(&self.db, where_);
        assert!(depth > 0, "mine_production: {where_} has depth {depth}");
        let (iron, gold, mithril) = MINE_PROD[depth.min(MINE_MAX) as usize];
        self.replenish(where_, ITEM_IRON, iron);
        self.replenish(where_, ITEM_GOLD, gold);
        self.replenish(where_, ITEM_MITHRIL, mithril);
    }

    /// `olympia/produce.c:location_production`: each location's month of
    /// production, by terrain. A poppy field is first cut back to its
    /// normal yield, then doubled if it was tended; islands and provinces
    /// on the sea wash up flotsam.
    pub fn location_production(&mut self) {
        for where_ in self.db.boxes_of_kind(Kind::LOC) {
            let terr = self.db.subkind(where_);

            if terr == Subkind::MINE {
                self.mine_production(where_);
            } else {
                for &(t, item, qty) in &TERR_PROD {
                    if t == terr {
                        self.replenish(where_, item, qty);
                    }
                }
            }

            if terr == Subkind::POPPY_FIELD {
                let n = has_item(&self.db, where_, ITEM_OPIUM);
                if n > POPPY_OPIUM {
                    self.consume_item(where_, ITEM_OPIUM, n - POPPY_OPIUM);
                }
                let tended = self
                    .db
                    .entity_mut(where_)
                    .misc
                    .as_mut()
                    .filter(|m| m.opium_double != 0)
                    .map(|m| m.opium_double = 0)
                    .is_some();
                if tended {
                    let n = has_item(&self.db, where_, ITEM_OPIUM);
                    self.gen_item(where_, ITEM_OPIUM, n);
                }
            }

            if terr == Subkind::ISLAND
                || (loc_depth(&self.db, where_) == LOC_PROVINCE
                    && self.has_ocean_access(where_) != 0)
            {
                self.replenish(where_, ITEM_FLOTSAM, 30);
            }
        }
    }
}
