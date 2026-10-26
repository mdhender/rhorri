// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Lords and loyalty (`olympia/swear.c`).

use crate::game::{Game, player};
use crate::turn::{Phase, not_ported};

/// `LOY_UNCHANGED`: leave the unit's loyalty as it is.
pub const LOY_UNCHANGED: i32 = -1;
/// `LOY_unsworn`.
pub const LOY_UNSWORN: i32 = 0;

impl Game {
    /// Swears `who` to `new_lord`, moving it between factions if their
    /// players differ, and sets its loyalty to `k` at level `lev` unless `k`
    /// is [`LOY_UNCHANGED`] (`olympia/swear.c:set_lord`).
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) when
    /// the move reaches the command driver (`flush_unit_orders` for a unit
    /// leaving a faction, `init_load_sup` for one joining a faction, task 4
    /// of #5) or the noble-point transfer (`char_np_total`).
    pub fn set_lord(&mut self, who: i32, new_lord: i32, k: i32, lev: i32) -> Phase {
        let old_pl = player(&self.db, who);
        let new_pl = if new_lord != 0 {
            player(&self.db, new_lord)
        } else {
            0
        };

        if old_pl != 0 && old_pl != new_pl {
            // flush_unit_orders(), interrupt_order(), clear_all_att(),
            // reswear_all_sworn(), the pledge, and leaving old_pl's units.
            return not_ported("flush_unit_orders"); // task 4 (input.c)
        }

        let c = self.db.p_char(who);
        let mut prev_lord = c.prev_lord;
        if prev_lord == 0 {
            prev_lord = old_pl;
        }
        if new_pl != 0 && old_pl != new_pl {
            c.prev_lord = old_pl;
        }
        c.unit_lord = new_lord;

        if new_lord != 0 && new_pl != old_pl {
            let units = &mut self.db.p_player(new_pl).units;
            units.append(who);
            units.sort();
            // Then touch_loc(who).
            return not_ported("init_load_sup"); // task 4 (input.c)
        }

        let mut old_pl = old_pl;
        if old_pl == crate::check::INDEP_PLAYER {
            old_pl = prev_lord;
        }
        if old_pl != 0 && new_pl != 0 && old_pl != new_pl && new_pl != crate::check::INDEP_PLAYER {
            // add_np() to old_pl and deduct_np() from new_pl, with their
            // report lines.
            return not_ported("char_np_total");
        }

        if k != LOY_UNCHANGED {
            // set_loyal()
            let c = self.db.p_char(who);
            c.loy_kind = k as i8;
            c.loy_rate = lev;
        }
        Ok(())
    }
}
