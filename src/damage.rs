// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Damage to structures, ships going down, and the hounds that bark at
//! hidden characters (`olympia/u.c`).
//!
//! TODO(stage 2): the `wout()` and `log_write()` lines (the barking, "has
//! sunk!", "collapses!") are report output.

use crate::entity::{Kind, Subkind};
use crate::game::Game;
use crate::loc::{LOC_PROVINCE, all_char_here, here_list, is_ship, loc_depth, region, subloc};
use crate::turn::{Phase, not_ported};
use crate::unit::{has_item, stack_has_item};

/// `item_hound`.
pub const ITEM_HOUND: i32 = 295;

/// `olympia/u.c:count_loc_char_item`: how much of `item` the characters in
/// `where_` hold between them.
#[must_use]
pub fn count_loc_char_item(db: &crate::entity::Db, where_: i32, item: i32) -> i32 {
    all_char_here(db, where_)
        .into_iter()
        .map(|i| has_item(db, i, item))
        .sum()
}

impl Game {
    /// `olympia/u.c:add_structure_damage`: adds `damage` points to `fort`,
    /// up to 100. At 100 it is destroyed if `can_destroy`, else left at 99.
    /// Returns whether it was destroyed.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if a ship sinks at sea with someone
    /// aboard (the drowning is `kill_char`).
    ///
    /// # Panics
    ///
    /// Panics if `damage` is negative, as C asserts.
    pub fn add_structure_damage(
        &mut self,
        fort: i32,
        damage: i32,
        can_destroy: bool,
    ) -> Result<bool, crate::game::NotYetImplemented> {
        assert!(damage >= 0, "add_structure_damage: damage {damage}");
        let p = self.db.p_subloc(fort);
        let total = i32::from(p.damage) + damage;
        // Both branches fit: damage is at most 100.
        p.damage = total.min(100) as u8;
        if p.damage < 100 {
            return Ok(false);
        }
        if !can_destroy {
            p.damage = 99;
            return Ok(false);
        }

        // Completely destroyed.
        if is_ship(&self.db, fort) {
            self.sink_ship(fort)?;
        } else {
            self.building_collapses(fort)?;
        }
        Ok(true)
    }

    /// `olympia/u.c:sink_ship`: `ship` goes down. At sea its passengers
    /// drown; in port they and its cargo go ashore. Storms bound to it are
    /// freed, and it is deleted.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if someone is aboard at sea
    /// ([`kill_stack_ocean`](Self::kill_stack_ocean)).
    fn sink_ship(&mut self, ship: i32) -> Phase {
        let where_ = subloc(&self.db, ship);

        // TODO(stage 2): log_write(LOG_SPECIAL, "%s has sunk in %s."), and
        // "%s has sunk!" to the ship and to where it was.

        if self.db.subkind(where_) == Subkind::OCEAN {
            for who in here_list(&self.db, ship) {
                if self.db.kind(who) == Kind::CHAR {
                    self.kill_stack_ocean(who)?;
                }
            }
        } else {
            for who in here_list(&self.db, ship) {
                if self.db.kind(who) == Kind::CHAR {
                    self.move_stack(who, where_)?;
                } else {
                    self.db.set_where(who, where_);
                }
            }
        }

        // Unbind any storms bound to this ship.
        if let Some(p) = self.db.get(ship).and_then(|b| b.subloc.as_ref()) {
            let storms = p.bound_storms.as_slice().to_vec();
            for storm in storms {
                if self.db.kind(storm) == Kind::STORM {
                    self.db.p_misc(storm).bind_storm = 0;
                }
            }
            self.db.p_subloc(ship).bound_storms.clear();
        }

        self.db.set_where(ship, 0);
        self.db.delete_box(ship);
        Ok(())
    }

    /// `olympia/u.c:building_collapses`: `fort` falls down. Everyone and
    /// everything inside goes out to where it stood. A mine becomes a
    /// collapsed mine, gone in eight months; anything else is deleted, and
    /// a castle's garrisons lose it.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if moving someone out reaches
    /// something not ported.
    fn building_collapses(&mut self, fort: i32) -> Phase {
        let where_ = subloc(&self.db, fort);

        // TODO(stage 2): log_write(LOG_SPECIAL, "%s collapsed in %s."), and
        // "%s collapses!" to those inside and where it stood.

        for who in here_list(&self.db, fort) {
            if self.db.kind(who) == Kind::CHAR {
                self.move_stack(who, where_)?;
            } else {
                self.db.set_where(who, where_);
            }
        }

        if self.db.subkind(fort) == Subkind::MINE {
            self.db.change_box_subkind(fort, Subkind::MINE_COLLAPSED);
            self.db.p_misc(fort).mine_delay = 8;
            return Ok(());
        }

        if self.db.subkind(fort) == Subkind::CASTLE {
            for i in self.db.boxes_of_subkind(Subkind::GARRISON) {
                // garrison_castle()
                let castle = self
                    .db
                    .get(i)
                    .and_then(|b| b.misc.as_ref())
                    .map_or(0, |m| m.garr_castle);
                if castle == fort {
                    self.db.p_misc(i).garr_castle = 0;
                }
            }
        }

        self.db.set_where(fort, 0);
        self.db.delete_box(fort);
        Ok(())
    }

    /// `olympia/u.c:kill_stack_ocean`: `who`'s stack falls into the sea.
    /// Each member dies, unless it survives and swims ashore.
    ///
    /// # Errors
    ///
    /// Always returns [`NotYetImplemented`]: character death
    /// (`kill_char`) isn't ported.
    pub fn kill_stack_ocean(&mut self, _who: i32) -> Phase {
        not_ported("kill_char")
    }

    /// `olympia/u.c:find_nearest_land`: the nearest land to `where_`: an
    /// island in it, or the first land province walking from it in a
    /// direction drawn on the `enty` stream, else a province of its region
    /// picked at random. 0 if there is none.
    ///
    /// # Panics
    ///
    /// Panics if a province has no neighbor in any of the four directions,
    /// or an island isn't a location, as C asserts.
    #[must_use]
    pub fn find_nearest_land(&self, mut where_: i32) -> i32 {
        let db = &self.db;
        let orig_where = where_;

        let mut try_two = 100;
        while try_two > 0 {
            try_two -= 1;
            let mut dir = self.ent_land_dir(orig_where, try_two);

            let mut try_one = 1000;
            while try_one > 0 {
                try_one -= 1;
                if db.subkind(where_) != Subkind::OCEAN {
                    return where_;
                }

                if let Some(&i) = here_list(db, where_)
                    .iter()
                    .find(|&&i| db.subkind(i) == Subkind::ISLAND)
                {
                    assert_eq!(db.kind(i), Kind::LOC, "find_nearest_land: island {i}");
                    return i;
                }

                where_ = crate::dir::location_direction(db, where_, dir);

                let mut check = 0;
                while where_ == 0 {
                    where_ = orig_where;
                    dir = (dir % 4) + 1;
                    check += 1;
                    assert!(check <= 4, "find_nearest_land: no way out of {orig_where}");
                    where_ = crate::dir::location_direction(db, where_, dir);
                }
            }

            // TODO(stage 2): log_write(LOG_CODE, "find_nearest_land: Plan
            // B") when try_two == 99.
        }

        // TODO(stage 2): log_write(LOG_CODE, "find_nearest_land: Plan C").
        let reg = region(db, orig_where);
        let l: Vec<i32> = db
            .boxes_of_kind(Kind::LOC)
            .into_iter()
            .filter(|&i| {
                region(db, i) == reg
                    && loc_depth(db, i) == LOC_PROVINCE
                    && db.subkind(i) != Subkind::OCEAN
            })
            .collect();
        if l.is_empty() {
            return 0;
        }
        let len = i32::try_from(l.len()).expect("province count fits an int");
        l[usize::try_from(self.ent_land_pick(orig_where, len)).expect("pick in range")]
    }

    /// `olympia/u.c:bark_dogs`: the hounds of the characters in `where_`
    /// may bark, each one in two.
    pub fn bark_dogs(&self, where_: i32) {
        let db = &self.db;
        let mut sum = 0;
        let mut bark = 0;
        for who in here_list(db, where_) {
            if db.kind(who) != Kind::CHAR {
                continue;
            }
            let n = stack_has_item(db, who, ITEM_HOUND);
            sum += n;
            for i in 1..=n {
                if self.ent_bark(who, i) == 1 {
                    bark += 1;
                }
            }
        }
        // TODO(stage 2): to everyone in where_, "The hound is barking."
        // (bark == 1 && sum == 1), "A hound is barking." (bark == 1) or
        // "The hounds are barking." (bark > 1).
        let _ = (sum, bark);
    }
}
