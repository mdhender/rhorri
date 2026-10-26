// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! NPC tokens: artifacts that command a band of NPC units for whoever
//! holds them (`olympia/art.c`).
//!
//! Moving a token between holders (`move_token`, `swear_token_units`) comes
//! with the commands that move items. TODO(stage 2): the `log_write()`
//! lines to `lib/log` and the "appears" and "melts into the ground" lines
//! to the locations are report output.

use crate::entity::{Kind, Subkind};
use crate::game::{Game, NotYetImplemented, player};
use crate::loc::{province, subloc};
use crate::swear::LOY_NPC;
use crate::turn::{Phase, not_ported};
use crate::unit::{SUB_NI, beast_capturable, item_token_ni, item_token_num, item_unique};

/// `sub_npc_token`.
const SUB_NPC_TOKEN: Subkind = Subkind(63);

impl Game {
    /// `token_player()`: the regular faction a token's holder belongs to,
    /// or the independent player.
    fn token_player(&self, owner: i32) -> i32 {
        if self.db.kind(owner) != Kind::CHAR {
            return self.system.indep_player;
        }
        let pl = player(&self.db, owner);
        if self.db.subkind(pl) != Subkind::PL_REGULAR {
            return self.system.indep_player;
        }
        pl
    }

    /// The units a token commands (`loop_units(item)`), copied.
    fn token_units(&self, item: i32) -> Vec<i32> {
        self.db
            .get(item)
            .and_then(|b| b.player.as_ref())
            .map(|p| p.units.as_slice().to_vec())
            .unwrap_or_default()
    }

    /// `melt_token_units()`: a token's units vanish while no player holds
    /// it, and it falls back to commanding one.
    ///
    /// # Panics
    ///
    /// Panics if `item` isn't an NPC token, as C asserts.
    fn melt_token_units(&mut self, item: i32) -> Phase {
        assert_eq!(self.db.subkind(item), SUB_NPC_TOKEN, "melt_token_units");
        if self
            .token_units(item)
            .into_iter()
            .any(|who| self.db.kind(who) == Kind::CHAR)
        {
            // Each one is reclaimed (char_reclaim), which kills it.
            return not_ported("char_reclaim");
        }
        if item_token_num(&self.db, item) > 1 {
            self.db.p_item_magic(item).token_num = 1;
        }
        Ok(())
    }

    /// `add_token_unit_sup()`: a new unit for a token, in its holder's
    /// province, or where the holder is if that is at sea.
    fn add_token_unit_sup(&mut self, item: i32) -> Result<(), NotYetImplemented> {
        let owner = item_unique(&self.db, item);
        assert_ne!(owner, 0, "add_token_unit_sup: token {item} has no holder");

        let mut where_ = province(&self.db, owner);
        if self.db.subkind(where_) == Subkind::OCEAN {
            where_ = subloc(&self.db, owner);
        }

        let ni = item_token_ni(&self.db, item);
        let pl = self.token_player(owner);
        let new = self.new_char(SUB_NI, ni, where_, -1, pl, LOY_NPC, 0, None)?;
        if new < 0 {
            return Ok(());
        }

        if beast_capturable(&self.db, new) {
            self.db.p_char(new).break_point = 0;
        }
        self.db.p_misc(new).cmd_allow = b'r';
        self.db.p_magic(new).token = item;
        self.db.p_player(item).units.append(new);
        Ok(())
    }

    /// `add_token_units()`: makes up a token's units to its number.
    fn add_token_units(&mut self, item: i32) -> Phase {
        assert_eq!(self.db.subkind(item), SUB_NPC_TOKEN, "add_token_units");
        let mut l = i32::try_from(self.token_units(item).len()).expect("unit count");
        while l < item_token_num(&self.db, item) {
            self.add_token_unit_sup(item)?;
            l += 1;
        }
        Ok(())
    }

    /// `olympia/art.c:check_token_units`: an NPC token held by a player has
    /// its units made up; one that isn't has them melt away. A unit sworn
    /// to someone else returns to the holder's faction if it can afford
    /// the unit.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] when units melt (`char_reclaim`) or
    /// a unit is sworn back (`char_np_total`).
    pub(crate) fn check_token_units(&mut self) -> Phase {
        for item in self.db.boxes_of_subkind(SUB_NPC_TOKEN) {
            let owner = item_unique(&self.db, item);
            assert_ne!(owner, 0, "check_token_units: token {item} has no holder");

            let pl = self.token_player(owner);
            if pl == self.system.indep_player {
                self.melt_token_units(item)?;
            } else {
                self.add_token_units(item)?;
            }

            for unit in self.token_units(item) {
                if self.db.kind(unit) != Kind::CHAR {
                    // TODO(stage 2): log_write(LOG_CODE, "%s holds unit %s
                    // which is %s, ...").
                    continue;
                }
                if player(&self.db, unit) != pl {
                    // player_np(pl) >= char_np_total(unit), then set_lord().
                    return not_ported("char_np_total");
                }
            }
        }
        Ok(())
    }
}
