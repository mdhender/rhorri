// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The month's end (`olympia/day.c:post_month`): taxes, claims and noble
//! points, the monthly decays, income and maintenance, the links between
//! places, and the deserting, starving and dying that come of them.
//!
//! The garrisons' tax, maintenance and ranks are in [`crate::garr`], the
//! storms in [`crate::storm`] and the economy in [`crate::buy`].
//!
//! The draws (desertion, starvation, animal deaths, corpses, inn income)
//! are keyed leaves on the turn's `upkp` stream (see [`crate::day`]), so
//! their order doesn't matter.
//!
//! TODO(stage 2): the `wout()` and `log_write()` lines, and the GM
//! report's gold totals (`gold_taxes`, `gold_inn`, `gold_temple`).

use crate::art::{has_auraculum, max_eff_aura};
use crate::check::stack_parent;
use crate::entity::{Kind, Subkind};
use crate::game::{Game, player, stage};
use crate::input::here_pos;
use crate::loc::{
    LOC_PROVINCE, building_owner, count_loc_structures, here_list, loc_depth, loc_pillage,
    province, recent_pillage, subloc, subloc_quest, summerbridge, uldim,
};
use crate::movement::{leave_stack, promote};
use crate::order::queue;
use crate::seed::ITEM_TAX_COOKIE;
use crate::swear::{LOY_NPC, LOY_SUMMON, LOY_UNSWORN, set_loyal};
use crate::turn::Phase;
use crate::unit::{
    ITEM_GOLD, body_old_lord, char_np_total, has_item, has_skill, inventory, is_magician, is_npc,
    is_prisoner, item_animal, item_aura_bonus, item_relic_decay, item_unique,
};

/// `LOY_contract`.
const LOY_CONTRACT: i32 = 1;
/// `LOY_oath`.
const LOY_OATH: i32 = 2;
/// `LOY_fear`.
const LOY_FEAR: i32 = 3;

/// `item_ghost_warrior`.
const ITEM_GHOST_WARRIOR: i32 = 18;
/// `item_corpse`.
const ITEM_CORPSE: i32 = 31;

/// `sub_dead_body`: a dead noble's body.
const SUB_DEAD_BODY: Subkind = Subkind(56);
/// `sub_relic`: the 400-series artifacts.
const SUB_RELIC: Subkind = Subkind(69);

/// `sk_religion`.
const SK_RELIGION: i32 = 750;

/// `NUM_MONTHS`.
const NUM_MONTHS: i32 = 8;

impl Game {
    /// `olympia/day.c:clear_orders_sent`: no faction has sent orders for
    /// the next turn yet.
    pub(crate) fn clear_orders_sent(&mut self) {
        for pl in self.db.boxes_of_kind(Kind::PLAYER) {
            if let Some(p) = self.db.entity_mut(pl).player.as_deref_mut() {
                p.sent_orders = 0;
            }
        }
    }

    /// `olympia/day.c:special_locs_open` (`open`) and `special_locs_close`:
    /// Summerbridge's swamps and Uldim pass open at the end of month 2 and
    /// close at the end of month 6. Their passability is read from the
    /// month whenever someone tries them, so all that changes here is what
    /// the provinces are told.
    pub(crate) fn special_locs(&self, open: bool) {
        let db = &self.db;
        for i in db.boxes_of_kind(Kind::LOC) {
            if loc_depth(db, i) != LOC_PROVINCE {
                continue;
            }
            // TODO(stage 2): log_write(LOG_CODE, "%s open to the north.")
            // and the like, and the message to the province.
            let _told = matches!((summerbridge(db, i), uldim(db, i)), (1 | 2, _) | (_, 3 | 4));
        }
        let _ = open;
    }

    /// `olympia/day.c:collect_taxes`: each castle's owner collects half
    /// the tax base of its province; the rest is lost.
    pub(crate) fn collect_taxes(&mut self) {
        for fort in self.db.boxes_of_subkind(Subkind::CASTLE) {
            let prov = province(&self.db, fort);
            let owner = building_owner(&self.db, fort);
            if owner == 0 {
                continue; // no one to collect taxes
            }
            let mut amount = has_item(&self.db, prov, ITEM_TAX_COOKIE);
            self.consume_item(prov, ITEM_TAX_COOKIE, amount);
            amount /= 2;
            // TODO(stage 2): gold_taxes += amount, and "Collected %s in
            // taxes."
            self.gen_item(owner, ITEM_GOLD, amount);
        }
    }

    /// `olympia/day.c:add_claim_gold`: every player faction's claim gains
    /// 25 gold.
    pub(crate) fn add_claim_gold(&mut self) {
        for pl in self.db.boxes_of_kind(Kind::PLAYER) {
            if self.db.subkind(pl) == Subkind::PL_REGULAR {
                self.gen_item(pl, ITEM_GOLD, 25);
            }
        }
    }

    /// `olympia/c1.c:next_np_turn`: how many turns until faction `pl`
    /// gains its next noble point. Every faction gains one on turns 8, 16,
    /// 24 and so on.
    fn next_np_turn(&mut self, pl: i32) -> i32 {
        self.db.p_player(pl); // p_player() allocates
        let ct = 7 - (i32::from(self.system.sysclock.turn) + 1) % NUM_MONTHS;
        let ft = 2; // by Cappinator: p->first_turn % NUM_MONTHS before
        (ft + ct) % NUM_MONTHS
    }

    /// `olympia/u.c:add_np`: faction `pl` gains `num` noble points.
    ///
    /// # Panics
    ///
    /// Panics if `pl` isn't a player, as C asserts.
    pub(crate) fn add_np(&mut self, pl: i32, num: i32) {
        assert_eq!(self.db.kind(pl), Kind::PLAYER, "add_np: {pl}");
        let p = self.db.p_player(pl);
        // short arithmetic, as C's.
        p.noble_points = (i32::from(p.noble_points) + num) as i16;
        p.np_gained = (i32::from(p.np_gained) + num) as i16;
    }

    /// `olympia/day.c:add_noble_points`.
    pub(crate) fn add_noble_points(&mut self) {
        for pl in self.db.boxes_of_kind(Kind::PLAYER) {
            if self.db.subkind(pl) == Subkind::PL_REGULAR && self.next_np_turn(pl) == 0 {
                self.add_np(pl, 1);
            }
        }
    }

    /// `olympia/day.c:add_unformed_sup`: tops faction `pl`'s list of
    /// unformed nobles (numbers its next units will take) up to five.
    pub(crate) fn add_unformed_sup(&mut self, pl: i32) {
        if self.db.get(pl).and_then(|b| b.player.as_ref()).is_none() {
            return;
        }
        while self.db.p_player(pl).unformed.len() < 5 {
            let new = self.new_ent(Kind::UNFORM, Subkind::default());
            if new <= 0 {
                break;
            }
            self.db.p_player(pl).unformed.append(new);
        }
    }

    /// `olympia/day.c:add_unformed`.
    pub(crate) fn add_unformed(&mut self) {
        for pl in self.db.boxes_of_kind(Kind::PLAYER) {
            self.add_unformed_sup(pl);
        }
    }

    /// `olympia/day.c:increment_current_aura`: a magician's aura rises two
    /// points a month, two more with its auraculum, and one for each item
    /// it holds that bonuses aura, up to its maximum.
    pub(crate) fn increment_current_aura(&mut self) {
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if !is_magician(&self.db, who) {
                continue;
            }
            let ac = has_auraculum(&self.db, who);
            let ma = max_eff_aura(&self.db, who);
            let bonuses = inventory(&self.db, who)
                .into_iter()
                .filter(|e| item_aura_bonus(&self.db, e.item) != 0)
                .count();

            let p = self.db.p_magic(who);
            let rises = 2 + if ac != 0 { 2 } else { 0 } + bonuses;
            for _ in 0..rises {
                if p.cur_aura < ma {
                    p.cur_aura += 1;
                }
            }
        }
    }

    /// `olympia/day.c:decrement_ability_shroud`.
    pub(crate) fn decrement_ability_shroud(&mut self) {
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if let Some(p) = self.rp_magic_mut(who)
                && p.ability_shroud > 0
            {
                p.ability_shroud -= 1;
            }
        }
    }

    /// `olympia/day.c:decrement_region_shroud`.
    pub(crate) fn decrement_region_shroud(&mut self) {
        for where_ in self.db.boxes_of_kind(Kind::LOC) {
            if let Some(p) = self.db.entity_mut(where_).loc.as_deref_mut()
                && p.shroud > 0
            {
                p.shroud -= 1;
                // TODO(stage 2): notify_loc_shroud() (scry.c) tells those
                // here with Shroud region how the shroud stands.
            }
        }
    }

    /// `olympia/day.c:decrement_meditation_hinder`.
    pub(crate) fn decrement_meditation_hinder(&mut self) {
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if let Some(p) = self.rp_magic_mut(who)
                && p.hinder_meditation > 0
            {
                p.hinder_meditation -= 1;
            }
        }
    }

    /// `olympia/day.c:decrement_loc_barrier`.
    pub(crate) fn decrement_loc_barrier(&mut self) {
        for where_ in self.db.boxes_of_kind(Kind::LOC) {
            if let Some(p) = self.db.entity_mut(where_).loc.as_deref_mut()
                && p.barrier > 0
            {
                p.barrier -= 1;
                // TODO(stage 2): "The barrier over %s has dissipated." at 0.
            }
        }
    }

    /// `rp_magic(who)` for changing.
    fn rp_magic_mut(&mut self, who: i32) -> Option<&mut crate::entity::CharMagic> {
        self.db
            .get_mut(who)
            .and_then(|b| b.char_.as_deref_mut())
            .and_then(|c| c.magic.as_deref_mut())
    }

    /// `olympia/day.c:loyalty_decay`: contract and fear loyalty wear down
    /// each month, and summoned units' time runs out. A contract unit
    /// below 50 may desert; so may a fear unit with none left.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if a
    /// desertion reaches something not ported.
    ///
    /// # Panics
    ///
    /// Panics on a loyalty kind that doesn't decay, as C asserts.
    pub(crate) fn loyalty_decay(&mut self) -> Phase {
        let indep = self.system.indep_player;
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            let Some(c) = self.db.entity(who).char_.as_deref() else {
                continue;
            };
            if c.fresh_hire != 0 {
                continue;
            }
            let (k, rate) = (i32::from(c.loy_kind), c.loy_rate);
            if k == LOY_UNSWORN || k == LOY_OATH || k == LOY_NPC {
                continue; // no decay
            }

            if rate <= 0 && k == LOY_SUMMON {
                leave_stack(&mut self.db, who);
                set_loyal(&mut self.db, who, LOY_NPC, 0); // redundant
                continue;
            }

            if rate < 50 && k == LOY_CONTRACT && self.up_loyal(who, 0, 1, 2) == 1 {
                // TODO(stage 2): log_write(LOG_DEATH, "%s deserts, %s").
                self.unit_deserts(who, indep, true, LOY_UNSWORN, 0)?;
                continue;
            }

            match k {
                LOY_SUMMON => self.db.p_char(who).loy_rate -= 1,
                LOY_FEAR => {
                    self.db.p_char(who).loy_rate -= 1;
                    if self.db.p_char(who).loy_rate <= 0 && self.up_loyal(who, 1, 1, 2) == 1 {
                        // TODO(stage 2): log_write(LOG_DEATH, "%s deserts,
                        // %s").
                        self.unit_deserts(who, indep, true, LOY_UNSWORN, 0)?;
                    }
                }
                LOY_CONTRACT => {
                    let p = self.db.p_char(who);
                    p.loy_rate -= 50.max(p.loy_rate / 10);
                }
                _ => panic!("loyalty_decay: {who} has loyalty kind {k}"),
            }

            let p = self.db.p_char(who);
            if p.loy_rate < 0 {
                p.loy_rate = 0;
            }
        }
        Ok(())
    }

    /// `olympia/swear.c:unit_deserts`: `who` leaves its faction for
    /// `to_who`'s, swearing to it with loyalty `k` at `lev`. A prisoner
    /// held by its new faction simply goes free; any other unit leaves its
    /// stack. `loy_check` isn't read.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// changing lords reaches something not ported.
    pub(crate) fn unit_deserts(
        &mut self,
        who: i32,
        to_who: i32,
        _loy_check: bool,
        k: i32,
        lev: i32,
    ) -> Phase {
        // TODO(stage 2): "%s renounces loyalty to us." to the old faction
        // and the unit when to_who and the old faction are set.

        if to_who != 0
            && is_prisoner(&self.db, who)
            && player(&self.db, to_who) == player(&self.db, stack_parent(&self.db, who))
        {
            self.db.p_char(who).prisoner = 0;
        } else if !is_prisoner(&self.db, who) {
            self.extract_stacked_unit(who)?;
        }

        self.set_lord(who, to_who, k, lev)?;

        if to_who != 0 {
            // TODO(stage 2): "%s pledges fealty to us." to the unit and
            // to_who.
            self.db.p_char(who).new_lord = 1;
        }
        Ok(())
    }

    /// `olympia/stack.c:extract_stacked_unit`: `who` leaves its stack,
    /// leaving those above and below it behind: the first character
    /// stacked under it takes its place, with the others under that one.
    /// Prisoners left with it escape.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if an
    /// escape reaches something not ported.
    pub(crate) fn extract_stacked_unit(&mut self, who: i32) -> Phase {
        // The first stacked character that isn't a prisoner.
        let first = here_list(&self.db, who)
            .into_iter()
            .find(|&i| self.db.kind(i) == Kind::CHAR && !is_prisoner(&self.db, i))
            .unwrap_or(0);

        // The others go beneath it, and it moves up and out a level, just
        // after who.
        if first != 0 {
            for i in here_list(&self.db, who) {
                if i != first && self.db.kind(i) == Kind::CHAR {
                    self.db.set_where(i, first);
                }
            }
            let out = self.db.loc(who);
            self.db.set_where(first, out);
            let pos = here_pos(&self.db, who) + 1;
            promote(&mut self.db, first, pos);
        }

        // Free any prisoners left.
        for i in here_list(&self.db, who) {
            if self.db.kind(i) == Kind::CHAR && is_prisoner(&self.db, i) {
                self.prisoner_escapes(i)?;
            }
        }

        leave_stack(&mut self.db, who);
        Ok(())
    }

    /// `olympia/day.c:pillage_decay`: a place not pillaged this month
    /// recovers a level of pillage.
    pub(crate) fn pillage_decay(&mut self) {
        for where_ in self.db.boxes_of_kind(Kind::LOC) {
            if loc_pillage(&self.db, where_) != 0 && !recent_pillage(&self.db, where_) {
                self.db.p_subloc(where_).loot -= 1;
            }
        }
    }

    /// `olympia/day.c:relic_decay`: a relic with a lifetime loses a month
    /// of it, and vanishes (to `nowhere_loc`) when it runs out.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// moving the relic reaches something not ported.
    pub(crate) fn relic_decay(&mut self) -> Phase {
        for item in self.db.boxes_of_kind(Kind::ITEM) {
            if self.db.subkind(item) != SUB_RELIC {
                continue;
            }
            if item_relic_decay(&self.db, item) == 0 {
                continue;
            }
            self.db.p_item_magic(item).relic_decay -= 1;
            if item_relic_decay(&self.db, item) > 0 {
                continue;
            }
            let owner = item_unique(&self.db, item);
            // TODO(stage 2): "%s vanishes." to a character owner.
            let nowhere = self.system.nowhere_loc;
            self.move_item(owner, nowhere, item, 1)?;
        }
        Ok(())
    }

    /// `olympia/day.c:hide_mage_decay`.
    pub(crate) fn hide_mage_decay(&mut self) {
        for i in self.db.boxes_of_kind(Kind::CHAR) {
            if let Some(p) = self.rp_magic_mut(i)
                && p.hide_mage > 0
            {
                p.hide_mage -= 1;
            }
        }
    }

    /// `olympia/day.c:inn_income`: an owned inn earns 50 to 75 gold, cut
    /// by pillage and shared among the inns of its place. One time in
    /// eight, an inn in an unpillaged place has a rich traveller who
    /// spends 50 to 130 more.
    pub(crate) fn inn_income(&mut self) {
        for i in self.db.boxes_of_subkind(Subkind::INN) {
            let owner = building_owner(&self.db, i);
            if owner == 0 {
                continue;
            }

            let where_ = subloc(&self.db, i);
            let n_inns = count_loc_structures(&self.db, where_, Subkind::INN, Subkind::default());

            let mut base = self.up_income(i, 0, 50, 75);
            let pil = loc_pillage(&self.db, where_);
            if pil != 0 {
                base /= pil + 1;
            }
            base /= n_inns;

            if pil == 0 && self.up_income(i, 1, 1, 8) == 1 {
                let bonus = self.up_income(i, 2, 5, 13) * 10;
                // TODO(stage 2): "A rich traveller stayed in %s this month,
                // spending %s."
                base += bonus;
            }

            self.gen_item(owner, ITEM_GOLD, base);
            // TODO(stage 2): gold_inn += base, and "%s yielded %s in
            // income."

            if pil != 0 {
                // TODO(stage 2): one of three lines about the looting.
                let _which = self.up_income(i, 3, 1, 3);
            }
        }
    }

    /// `olympia/day.c:temple_income`: a temple owned by a priest collects
    /// 100 gold of offerings, cut by pillage.
    pub(crate) fn temple_income(&mut self) {
        for i in self.db.boxes_of_subkind(Subkind::TEMPLE) {
            let owner = building_owner(&self.db, i);
            if owner == 0 || has_skill(&self.db, owner, SK_RELIGION) == 0 {
                continue;
            }
            let mut amount = 100;
            let where_ = subloc(&self.db, i);
            let pil = loc_pillage(&self.db, where_);
            if pil != 0 {
                amount /= pil + 1;
            }
            self.gen_item(owner, ITEM_GOLD, amount);
            // TODO(stage 2): gold_temple += amount, and "%s collected
            // offerings of %s."
        }
    }

    /// `olympia/day.c:charge_maint_costs`: every unit of a player faction
    /// pays its men.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// men starving reaches something not ported.
    pub(crate) fn charge_maint_costs(&mut self) -> Phase {
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if self.db.subkind(player(&self.db, who)) != Subkind::PL_REGULAR {
                continue;
            }
            self.charge_maint_sup(who)?;
        }
        Ok(())
    }

    /// `olympia/day.c:animal_deaths`: each animal a player's unit holds
    /// dies one time in a hundred.
    pub(crate) fn animal_deaths(&mut self) {
        stage("animal_deaths()");
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if self.db.subkind(player(&self.db, who)) != Subkind::PL_REGULAR {
                continue;
            }
            for e in inventory(&self.db, who) {
                if e.qty > 0 && item_animal(&self.db, e.item) {
                    let dead = (1..=e.qty)
                        .filter(|&i| self.up_animal(who, e.item, i, 1, 1000) < 10)
                        .count() as i32;
                    if dead > 0 {
                        self.consume_item(who, e.item, dead);
                        // TODO(stage 2): "%s %s died."
                    }
                }
            }
        }
    }

    /// `olympia/day.c:ghost_warrior_decay`: one of a unit's ghost warriors
    /// evaporates each month.
    pub(crate) fn ghost_warrior_decay(&mut self) {
        for i in self.db.boxes_of_kind(Kind::CHAR) {
            if is_npc(&self.db, i) {
                continue;
            }
            if has_item(&self.db, i, ITEM_GHOST_WARRIOR) <= 0 {
                continue;
            }
            // TODO(stage 2): "%s evaporated."
            self.consume_item(i, ITEM_GHOST_WARRIOR, 1);
        }
    }

    /// `olympia/day.c:corpse_decay`: none, one or two of a unit's corpses
    /// decompose each month.
    pub(crate) fn corpse_decay(&mut self) {
        for i in self.db.boxes_of_kind(Kind::CHAR) {
            if is_npc(&self.db, i) {
                continue;
            }
            let has = has_item(&self.db, i, ITEM_CORPSE);
            if has <= 0 {
                continue;
            }
            let has = has.min(self.up_corpse(i, 0, 2));
            if has != 0 {
                // TODO(stage 2): "%s decomposed."
                self.consume_item(i, ITEM_CORPSE, has);
            }
        }
    }

    /// `olympia/day.c:dead_body_rot`: a dead noble's body rots away twelve
    /// turns after its death.
    ///
    /// # Panics
    ///
    /// Panics on a body no one holds, as C asserts.
    pub(crate) fn dead_body_rot(&mut self) {
        for i in self.db.boxes_of_subkind(SUB_DEAD_BODY) {
            let owner = item_unique(&self.db, i);
            assert_ne!(owner, 0, "dead_body_rot: no one holds {i}");

            let t =
                i32::from(self.system.sysclock.turn) - i32::from(self.db.p_char(i).death_time.turn);
            if t < 12 {
                continue;
            }
            // TODO(stage 2): "%s decomposed." to a character owner.
            self.destroy_unique_item(owner, i);
        }
    }

    /// `olympia/u.c:destroy_unique_item`: `who` loses unique `item`, which
    /// is gone. A dead noble's body passing on returns the noble's points
    /// to its old faction.
    ///
    /// # Panics
    ///
    /// Panics if `item` isn't a unique item held by `who`, as C asserts.
    pub(crate) fn destroy_unique_item(&mut self, who: i32, item: i32) {
        assert_eq!(self.db.kind(item), Kind::ITEM, "destroy_unique_item");
        assert_ne!(item_unique(&self.db, item), 0, "destroy_unique_item");

        if self.db.subkind(item) == SUB_DEAD_BODY {
            let mut pl = body_old_lord(&self.db, item);
            if pl == self.system.indep_player {
                pl = self.db.p_char(item).prev_lord;
            }
            if self.db.kind(pl) == Kind::PLAYER {
                let nps = char_np_total(&self.db, item);
                // TODO(stage 2): "%s~%s has passed on.  Gained %d NP%s." to
                // the faction.
                self.add_np(pl, nps);
            }
        }

        let ret = self.sub_item(who, item, 1);
        assert!(ret, "destroy_unique_item: {who} hasn't {item}");
        self.db.delete_box(item);
    }

    /// `olympia/day.c:collapsed_mine_decay`: a collapsed mine is cleared
    /// away when its delay runs out.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// moving those inside out reaches something not ported.
    pub(crate) fn collapsed_mine_decay(&mut self) -> Phase {
        for i in self.db.boxes_of_subkind(Subkind::MINE_COLLAPSED) {
            let p = self.db.p_misc(i);
            p.mine_delay = p.mine_delay.wrapping_sub(1);
            if p.mine_delay < 0 {
                p.mine_delay = 0;
            }
            if p.mine_delay == 0 {
                self.get_rid_of_collapsed_mine(i)?;
            }
        }
        Ok(())
    }

    /// `olympia/u.c:get_rid_of_collapsed_mine`: anything inside moves out,
    /// and the mine is gone.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if
    /// moving a stack out reaches something not ported.
    ///
    /// # Panics
    ///
    /// Panics if `fort` isn't a collapsed mine, as C asserts.
    pub(crate) fn get_rid_of_collapsed_mine(&mut self, fort: i32) -> Phase {
        let where_ = subloc(&self.db, fort);
        assert_eq!(
            self.db.subkind(fort),
            Subkind::MINE_COLLAPSED,
            "get_rid_of_collapsed_mine: {fort}"
        );

        // Move anything inside, out, just in case.
        for who in here_list(&self.db, fort) {
            if self.db.kind(who) == Kind::CHAR {
                self.move_stack(who, where_)?;
            } else {
                self.db.set_where(who, where_);
            }
        }

        self.db.set_where(fort, 0);
        self.db.delete_box(fort);
        Ok(())
    }

    /// `olympia/day.c:auto_drop`: a player faction that hasn't sent orders
    /// for `auto_quit_turns` turns is queued to quit.
    pub(crate) fn auto_drop(&mut self) {
        let turn = i32::from(self.system.sysclock.turn);
        let quit_turns = self.system.auto_quit_turns;
        for pl in self.db.boxes_of_subkind(Subkind::PL_REGULAR) {
            let last = self.db.p_player(pl).last_order_turn;
            if turn - last >= quit_turns {
                queue(&mut self.db, pl, b"quit");
                // TODO(stage 2): log_write(LOG_SPECIAL, "Queued drop for
                // %s") with the player's name and email.
            }
        }
    }

    /// `olympia/day.c:link_decay`: an open link between places closes a
    /// month at a time, and opens for two months in its month of the year.
    pub(crate) fn link_decay(&mut self) {
        let month = self.system.sysclock.month();
        for i in self.db.boxes_of_kind(Kind::LOC) {
            let Some(p) = self.db.entity_mut(i).subloc.as_deref_mut() else {
                continue;
            };
            if p.link_to.is_empty() {
                continue;
            }
            if p.link_open > 0 {
                p.link_open -= 1;
            }
            if i32::from(p.link_when) == month && p.link_open < 2 && p.link_open >= 0 {
                p.link_open = 2;
            }
        }
    }

    /// `olympia/day.c:quest_decay`.
    pub(crate) fn quest_decay(&mut self) {
        for where_ in self.db.boxes_of_kind(Kind::LOC) {
            if subloc_quest(&self.db, where_) > 0 {
                self.db.p_subloc(where_).quest_late -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];

    fn game(turn: i16) -> Game {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new(MASTER);
        g.system.sysclock.turn = turn;
        g
    }

    #[test]
    fn noble_points_come_every_eighth_turn() {
        let mut g = game(0);
        g.db.alloc_box(50001, Kind::PLAYER, Subkind::PL_REGULAR);
        let turns: Vec<i16> = (0..30)
            .filter(|&t| {
                g.system.sysclock.turn = t;
                g.next_np_turn(50001) == 0
            })
            .collect();
        assert_eq!(turns, [0, 8, 16, 24]);
    }
}
