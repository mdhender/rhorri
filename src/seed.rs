// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Seeding locations (`olympia/seed.c`).
//!
//! Only `seed_taxes`, which runs every month, is here. Seeding a new
//! world (`seed_initial_locations`, `seed_phase_two`) is world creation,
//! which comes with stage 5.

use crate::entity::{Kind, Subkind};
use crate::game::Game;
use crate::loc::{LOC_PROVINCE, loc_civ, loc_depth, loc_opium, loc_pillage, province};
use crate::unit::has_item;

/// `item_tax_cookie`: a location's tax base.
pub const ITEM_TAX_COOKIE: i32 = 96;
/// `item_mage_menial`: a location's magician menial labor cookies.
const ITEM_MAGE_MENIAL: i32 = 277;
/// `item_petty_thief`: a city's petty thief cookie.
const ITEM_PETTY_THIEF: i32 = 294;

impl Game {
    /// `olympia/seed.c:seed_taxes`: resets the month's petty thief, menial
    /// labor and tax cookies in every city and land province, by its
    /// civilization, opium addiction and pillaging.
    ///
    /// # Panics
    ///
    /// Panics if a location's tax base comes to nothing, as C asserts.
    pub fn seed_taxes(&mut self) {
        for where_ in self.db.boxes_of_kind(Kind::LOC) {
            let sk = self.db.subkind(where_);
            if loc_depth(&self.db, where_) != LOC_PROVINCE && sk != Subkind::CITY {
                continue;
            }
            if sk == Subkind::OCEAN {
                continue;
            }

            if sk == Subkind::CITY {
                let n = has_item(&self.db, where_, ITEM_PETTY_THIEF);
                self.consume_item(where_, ITEM_PETTY_THIEF, n);
                self.gen_item(where_, ITEM_PETTY_THIEF, 1);
            }

            let n = has_item(&self.db, where_, ITEM_MAGE_MENIAL);
            self.consume_item(where_, ITEM_MAGE_MENIAL, n);
            let n = has_item(&self.db, where_, ITEM_TAX_COOKIE);
            self.consume_item(where_, ITEM_TAX_COOKIE, n);

            let civ = loc_civ(&self.db, province(&self.db, where_));
            let pil = loc_pillage(&self.db, where_);

            let mut base = civ * 5;
            if pil != 0 {
                base /= pil + 1;
            }
            self.gen_item(where_, ITEM_MAGE_MENIAL, base);

            assert_eq!(has_item(&self.db, where_, ITEM_TAX_COOKIE), 0, "seed_taxes");

            // The tax base of a province is its civilization level.
            let mut base = if sk == Subkind::CITY {
                100
            } else {
                50 + civ * 50
            };
            // Each point of opium addiction takes 10% off.
            base -= base / 10 * loc_opium(&self.db, where_);
            assert!(base > 0, "seed_taxes: {where_} has tax base {base}");
            if pil != 0 {
                base /= pil + 1;
            }
            self.gen_item(where_, ITEM_TAX_COOKIE, base);
        }
    }
}
