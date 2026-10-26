// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Savages: the monthly attacks on structures, and the savages' program
//! (`olympia/savage.c`).
//!
//! Their draws come from the per-location `npcs` stream (see
//! [`crate::npc`]): each structure's spawn check is a keyed draw on the
//! stream seeded for the structure, made even when nothing spawns.
//!
//! The drum commands (`use 98`, `summon savage`, `keep savage`) are stage 4
//! and aren't here.

use crate::check::stack_parent;
use crate::code::int_to_code;
use crate::dir::LAND;
use crate::entity::{Kind, Subkind};
use crate::game::Game;
use crate::loc::{
    LOC_BUILD, LOC_PROVINCE, building_owner, here_list, is_loc_or_ship, loc_depth, subloc,
};
use crate::order::queue;
use crate::swear::{LOY_NPC, LOY_SUMMON, set_loyal};
use crate::turn::Phase;
use crate::unit::{SUB_NI, has_item, noble_item};

/// `MAX_SAVAGES`: the most savage units the world may hold.
const MAX_SAVAGES: i32 = 200;

/// `item_savage`.
pub const ITEM_SAVAGE: i32 = 32;
/// `item_drum`.
const ITEM_DRUM: i32 = 98;

/// `savage_hates()`: an inn, castle, tower, ship or temple, built or not.
fn savage_hates(g: &Game, where_: i32) -> bool {
    matches!(
        g.db.subkind(where_),
        Subkind::INN
            | Subkind::INN_NOTDONE
            | Subkind::CASTLE
            | Subkind::CASTLE_NOTDONE
            | Subkind::TOWER
            | Subkind::TOWER_NOTDONE
            | Subkind::GALLEY
            | Subkind::GALLEY_NOTDONE
            | Subkind::ROUNDSHIP
            | Subkind::ROUNDSHIP_NOTDONE
            | Subkind::TEMPLE
            | Subkind::TEMPLE_NOTDONE
    )
}

/// `savage_hate_here()`: the first structure directly in `where_` that
/// savages hate, or 0.
fn savage_hate_here(g: &Game, where_: i32) -> i32 {
    here_list(&g.db, where_)
        .into_iter()
        .find(|&i| {
            is_loc_or_ship(&g.db, i) && loc_depth(&g.db, i) == LOC_BUILD && savage_hates(g, i)
        })
        .unwrap_or(0)
}

impl Game {
    /// `create_savage()`: a band of 3 to 25 savages with a drum at
    /// `where_`. Returns the unit, or -1.
    fn create_savage(&mut self, where_: i32) -> Result<i32, crate::game::NotYetImplemented> {
        let indep = self.system.indep_player;
        let new = self.new_char(SUB_NI, ITEM_SAVAGE, where_, 100, indep, LOY_NPC, 0, None)?;
        if new < 0 {
            return Ok(-1);
        }

        self.begin_npc(where_);
        self.gen_item(new, ITEM_DRUM, 1);
        let qty = self.npc_qty(where_, new, 3, 25);
        self.gen_item(new, ITEM_SAVAGE, qty);
        Ok(new)
    }

    /// `call_savage()`: unless sworn humans are there, raises savages at
    /// `where_` who march on `to_where` to answer `who`: a battle challenge
    /// (`why` 0), a call to arms (1), or to attack a structure (2).
    /// Returns whether any came.
    fn call_savage(
        &mut self,
        where_: i32,
        to_where: i32,
        who: i32,
        why: i32,
    ) -> Result<bool, crate::game::NotYetImplemented> {
        if self.controlled_humans_here(where_) {
            return Ok(false);
        }

        let new = self.create_savage(where_)?;
        let who_code = int_to_code(who);
        queue(
            &mut self.db,
            new,
            format!("move {}", int_to_code(to_where)).as_bytes(),
        );

        match why {
            0 => {
                // battle challenge
                queue(&mut self.db, new, format!("attack {who_code}").as_bytes());
            }
            1 => {
                // call to arms
                set_loyal(&mut self.db, new, LOY_SUMMON, 3);
                queue(&mut self.db, new, format!("stack {who_code}").as_bytes());
            }
            2 => {
                // move and attack structure; the npcs stream is still
                // seeded for where_ by create_savage()
                queue(&mut self.db, new, b"use 98 1");
                let t = self.npc_behavior(new, 0, 35, 50);
                queue(&mut self.db, new, format!("wait time {t}").as_bytes());
                queue(&mut self.db, new, format!("attack {who_code}").as_bytes());
            }
            _ => {}
        }

        self.init_load_sup(new); // ready to run commands at once
        Ok(true)
    }

    /// `olympia/savage.c:auto_savage`: a free savage razes the structure
    /// it is in; enters and razes an empty one here, or drums up help to
    /// attack a manned one; otherwise unstacks its followers and moves on,
    /// or, with no sworn humans about, disbands.
    pub(crate) fn auto_savage(&mut self, who: i32) {
        let where_ = subloc(&self.db, who);

        // Stacked under someone: do nothing.
        if stack_parent(&self.db, who) != 0 {
            return;
        }

        // In a structure: raze it.
        if savage_hates(self, where_) {
            queue(&mut self.db, who, b"raze");
            return;
        }

        let target = savage_hate_here(self, where_);
        if target != 0 {
            if building_owner(&self.db, target) == 0 {
                queue(
                    &mut self.db,
                    who,
                    format!("enter {}", int_to_code(target)).as_bytes(),
                );
                queue(&mut self.db, who, b"raze");
                return;
            }

            if self.controlled_humans_here(target) {
                if has_item(&self.db, who, ITEM_DRUM) < 1 {
                    self.gen_item(who, ITEM_DRUM, 1);
                }

                self.begin_npc(where_);
                queue(&mut self.db, who, format!("use {ITEM_DRUM} 1").as_bytes());
                let t = self.npc_behavior(who, 0, 35, 50);
                queue(&mut self.db, who, format!("wait time {t}").as_bytes());
                queue(
                    &mut self.db,
                    who,
                    format!("attack {}", int_to_code(target)).as_bytes(),
                );
                return;
            }
        }

        // Unstack any savages under us.
        for i in here_list(&self.db, who) {
            if self.db.kind(i) == Kind::CHAR {
                queue(&mut self.db, who, format!("unstack {i}").as_bytes());
            }
        }

        if !self.controlled_humans_here(where_) {
            queue(&mut self.db, who, b"die");
            return;
        }

        self.npc_move(who);
    }

    /// `olympia/savage.c:init_savage_attacks`: unless the world is full of
    /// savages, each structure has a 1% chance a band rises in a land
    /// province next to its own to march on it. The chance is a keyed draw
    /// on the `npcs` stream seeded for the structure, one per structure
    /// even when none rise.
    pub(crate) fn init_savage_attacks(&mut self) -> Phase {
        let db = &self.db;
        self.num_savages = db
            .boxes_of_kind(Kind::CHAR)
            .into_iter()
            .filter(|&i| noble_item(db, i) == ITEM_SAVAGE)
            .count() as i32;

        if self.num_savages >= MAX_SAVAGES {
            return Ok(());
        }

        // loop_loc: the body makes characters, never locations.
        for fort in self.db.boxes_of_kind(Kind::LOC) {
            if loc_depth(&self.db, fort) != LOC_BUILD {
                continue;
            }

            self.begin_npc(fort);
            if self.npc_spawn(fort, 0, 1, 100) != 1 {
                continue;
            }

            let where_ = subloc(&self.db, fort);
            if loc_depth(&self.db, where_) != LOC_PROVINCE {
                continue;
            }

            let l = self.exits_from_loc_nsew_select(0, where_, LAND, true);
            if l.is_empty() {
                eprintln!("init_savage_attacks: no exits?");
                continue; // probably shouldn't happen
            }

            for e in &l {
                if self.call_savage(e.destination, where_, fort, 2)? {
                    break;
                }
            }
        }
        Ok(())
    }
}
