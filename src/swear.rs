// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Lords and loyalty (`olympia/swear.c`).

use crate::game::{Game, lord, player};
use crate::turn::{Phase, not_ported};

/// `LOY_UNCHANGED`: leave the unit's loyalty as it is.
pub const LOY_UNCHANGED: i32 = -1;
/// `LOY_unsworn`.
pub const LOY_UNSWORN: i32 = 0;

impl Game {
    /// `char_pledge()`: whom `who` is pledged to, or 0.
    fn char_pledge(&self, who: i32) -> i32 {
        self.db
            .get(who)
            .and_then(|b| b.char_.as_ref())
            .and_then(|c| c.magic.as_ref())
            .map_or(0, |m| m.pledge)
    }

    /// `reswear_all_sworn()`: the units sworn to `who` swear to `who`'s
    /// lord instead.
    fn reswear_all_sworn(&mut self, who: i32) -> Phase {
        let pl = player(&self.db, who);
        let new_lord = lord(&self.db, who);
        // loop_units
        let units = self
            .db
            .get(pl)
            .and_then(|b| b.player.as_ref())
            .map(|p| p.units.as_slice().to_vec())
            .unwrap_or_default();
        for i in units {
            if lord(&self.db, i) == who {
                self.set_lord(i, new_lord, LOY_UNCHANGED, 0)?;
            }
        }
        Ok(())
    }

    /// Swears `who` to `new_lord`, moving it between factions if their
    /// players differ, and sets its loyalty to `k` at level `lev` unless `k`
    /// is [`LOY_UNCHANGED`] (`olympia/swear.c:set_lord`).
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) when
    /// the move interrupts a command whose interrupt routine isn't ported,
    /// or reaches the noble-point transfer (`char_np_total`).
    pub fn set_lord(&mut self, who: i32, new_lord: i32, k: i32, lev: i32) -> Phase {
        let old_pl = player(&self.db, who);
        let new_pl = if new_lord != 0 {
            player(&self.db, new_lord)
        } else {
            0
        };

        if old_pl != 0 && old_pl != new_pl {
            self.flush_unit_orders(old_pl, who);
            self.interrupt_order(who)?;
            // clear_all_att()
            if let Some(d) = self.db.get_mut(who).and_then(|b| b.disp.as_deref_mut()) {
                d.neutral.clear();
                d.hostile.clear();
                d.defend.clear();
            }
            self.reswear_all_sworn(who)?;

            if self.char_pledge(who) != 0 {
                self.db.p_magic(who).pledge = 0;
                // TODO(stage 2): pledge_backlinks = FALSE, report.c's cache
                // of who is pledged to whom.
            }

            self.db.p_player(old_pl).units.rem_value(who);
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

            self.init_load_sup(who); // load the command from the new owner
            self.touch_loc(who);
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
