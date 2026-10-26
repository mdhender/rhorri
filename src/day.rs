// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The daily events (`olympia/day.c:daily_events`) and the streams they
//! draw on: `caln`, which picks the days of the month that the curse
//! erosion, the elven hunts and the barking hounds fall on, and `upkp`,
//! the per-turn upkeep stream healing draws from.
//!
//! Both are keyed leaves on a per-turn stream:
//! `rng_stream_of(rng_stream_of(root, turn, "turn"), 0, tag)`, seeded the
//! first time one of them draws in a turn (`begin_calendar()`,
//! `begin_upkeep()`). A keyed leaf never moves a counter, so the stream is
//! derived afresh for each draw here. See olympia-g3's
//! `doc/claude/rng-streams.md`.
//!
//! The month's end draws on `upkp` too (`up_loyal`, `up_starve`,
//! `up_animal`, `up_corpse`, `up_income`); those are keyed leaves on the
//! turn's stream itself, not on a day's child.
//!
//! TODO(stage 2): the `wout()` and `log_write()` lines (calamities,
//! healing, garrison pay, aura) are report output, and so is
//! `gold_garrison`, the garrison pay total in the GM's report.

use crate::art::max_eff_aura;
use crate::check::stack_leader;
use crate::damage::count_loc_char_item;
use crate::entity::{Kind, MONTH_DAYS, Subkind};
use crate::game::Game;
use crate::loc::{all_char_here, building_owner, here_list, is_ship, mine_depth, subloc};
use crate::rng::{Stream, Tag};
use crate::turn::{Phase, not_ported};
use crate::unit::{
    ITEM_GOLD, char_health, char_really_hidden, char_sick, default_garrison, has_item, inventory,
    is_magician, is_prisoner, item_curse, loyal_kind,
};

/// `TAG_TURN`: the per-turn parent of the `caln` and `upkp` streams.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_CALENDAR`.
const TAG_CALENDAR: Tag = Tag::new(b"caln");
/// `CTAG_DAY`: a day-of-month pick.
const CTAG_DAY: Tag = Tag::new(b"day ");
/// `TAG_UPKEEP`.
const TAG_UPKEEP: Tag = Tag::new(b"upkp");
/// `UTAG_HEAL`: sick recovery and the amount healed or lost.
const UTAG_HEAL: Tag = Tag::new(b"heal");
/// `UTAG_LOYAL`: loyalty-decay desertion checks.
const UTAG_LOYAL: Tag = Tag::new(b"loyl");
/// `UTAG_SICKEN`: whether starving men left service or deserted.
const UTAG_SICKEN: Tag = Tag::new(b"sckn");
/// `UTAG_ANIMAL`: whether one animal dies.
const UTAG_ANIMAL: Tag = Tag::new(b"dier");
/// `UTAG_CORPSE`: corpse decomposition.
const UTAG_CORPSE: Tag = Tag::new(b"corp");
/// `UTAG_INCOME`: an inn's income.
const UTAG_INCOME: Tag = Tag::new(b"inco");

/// `item_worker`.
const ITEM_WORKER: i32 = 11;
/// `LOY_oath`.
const LOY_OATH: i32 = 2;

/// The days of the month `daily_events()` picked for its monthly events
/// (its `static` `curse_erode_day`, `faery_day` and `dog_bark_day`): 0
/// until picked, on the first day the run reaches.
#[derive(Debug, Clone, Copy, Default)]
pub struct Calendar {
    curse_erode_day: i32,
    faery_day: i32,
    dog_bark_day: i32,
}

/// The per-turn stream `tag` under the turn root.
fn turn_stream(g: &Game, tag: Tag) -> Stream {
    Stream::new(g.seed.master_seed())
        .child(i32::from(g.system.sysclock.turn), TAG_TURN)
        .child(0, tag)
}

impl Game {
    /// `cal_day()`: day-of-month pick `which` (0 curse erosion, 1 the
    /// elven hunts, 2 the barking hounds) in `low..=high`, on the `caln`
    /// stream.
    #[must_use]
    pub fn cal_day(&self, which: i32, low: i32, high: i32) -> i32 {
        turn_stream(self, TAG_CALENDAR).keyed(which, 0, CTAG_DAY, low, high)
    }

    /// `up_heal()`: healing roll `sub` (0 recovering from sickness, 1 the
    /// amount healed or lost) for `who`, on today's child of the `upkp`
    /// stream (`up_today()`, playbymail/olympia-g3#186).
    #[must_use]
    pub fn up_heal(&self, who: i32, sub: i32, low: i32, high: i32) -> i32 {
        turn_stream(self, TAG_UPKEEP)
            .child(i32::from(self.system.sysclock.day), UTAG_HEAL)
            .keyed(who, sub, UTAG_HEAL, low, high)
    }

    /// `up_loyal()`: loyalty-decay desertion check `sub` (0 a contract
    /// running out, 1 fear wearing off) for `who`, on the `upkp` stream.
    #[must_use]
    pub fn up_loyal(&self, who: i32, sub: i32, low: i32, high: i32) -> i32 {
        turn_stream(self, TAG_UPKEEP).keyed(who, sub, UTAG_LOYAL, low, high)
    }

    /// `up_starve()`: whether `who`'s starving men of kind `item` left
    /// service (1) or deserted, on the `upkp` stream.
    #[must_use]
    pub fn up_starve(&self, who: i32, item: i32, low: i32, high: i32) -> i32 {
        turn_stream(self, TAG_UPKEEP).keyed(who, item, UTAG_SICKEN, low, high)
    }

    /// `up_animal()`: the death roll for animal `idx` (from 1) of kind
    /// `item` held by `who`, on the `upkp` stream. The item is folded into
    /// the purpose tag.
    #[must_use]
    pub fn up_animal(&self, who: i32, item: i32, idx: i32, low: i32, high: i32) -> i32 {
        turn_stream(self, TAG_UPKEEP).keyed(who, idx, UTAG_ANIMAL.xor(item as u32), low, high)
    }

    /// `up_corpse()`: how many of `who`'s corpses decompose, on the `upkp`
    /// stream.
    #[must_use]
    pub fn up_corpse(&self, who: i32, low: i32, high: i32) -> i32 {
        turn_stream(self, TAG_UPKEEP).keyed(who, 0, UTAG_CORPSE, low, high)
    }

    /// `up_income()`: inn `inn`'s income roll `sub` (0 the base income, 1
    /// the rich traveller's chance, 2 his spending, 3 the pillage message),
    /// on the `upkp` stream.
    #[must_use]
    pub fn up_income(&self, inn: i32, sub: i32, low: i32, high: i32) -> i32 {
        turn_stream(self, TAG_UPKEEP).keyed(inn, sub, UTAG_INCOME, low, high)
    }

    /// `olympia/day.c:daily_events`: the day's scheduled events.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) at the
    /// first routine reached that isn't ported.
    pub(crate) fn daily_events(&mut self) -> Phase {
        let days = i32::from(MONTH_DAYS);
        if self.calendar.curse_erode_day == 0 {
            self.calendar.curse_erode_day = self.cal_day(0, 1, days);
        }
        if self.calendar.faery_day == 0 {
            self.calendar.faery_day = self.cal_day(1, days / 2, days);
        }
        if self.calendar.dog_bark_day == 0 {
            self.calendar.dog_bark_day = self.cal_day(2, 1, days);
        }

        self.schedule_weather_days();

        self.default_garrison_pay();
        self.ship_coastal_damage()?;
        self.random_loc_damage()?;
        self.check_ocean_chars()?;

        let day = i32::from(self.system.sysclock.day);
        if day % 7 == 0 {
            self.heal_characters()?;
            self.weekly_prisoner_escape_check()?;
        }

        if day == self.calendar.curse_erode_day {
            self.noncreator_curse_erode();
        }

        if day == 15 {
            self.increment_stone_ring_aura();
        }

        if day == self.calendar.dog_bark_day {
            self.dogs_bark_at_hidden_chars();
        }

        self.weather_day();

        if day == self.calendar.faery_day {
            self.auto_faery()?;
        }
        Ok(())
    }

    /// `olympia/day.c:near_rocky_coast`: 0 if `where_` isn't ocean, 1 if
    /// it borders land, 2 if a dangerous (mountain) coast, 3 if open sea.
    /// The first mountain among the NSEW neighbors returns 2 at once.
    fn near_rocky_coast(&self, where_: i32) -> i32 {
        if self.db.subkind(where_) != Subkind::OCEAN {
            return 0;
        }
        let mut ret = 3;
        for e in self.exits_from_loc_nsew(0, where_) {
            let sk = self.db.subkind(e.destination);
            if sk != Subkind::OCEAN {
                if sk == Subkind::MOUNTAIN {
                    return 2;
                }
                ret = 1;
            }
        }
        ret
    }

    /// `olympia/day.c:ship_coastal_damage`: a ship near a coast may strike
    /// a reef or rocks, on the `wthr` stream.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if the damage sinks a ship with
    /// someone aboard.
    fn ship_coastal_damage(&mut self) -> Phase {
        let mut ship = self.db.kind_first(Kind::SHIP);
        while ship > 0 {
            let next = self.db.kind_next(ship);
            if is_ship(&self.db, ship) {
                let n = self.near_rocky_coast(subloc(&self.db, ship));
                let sub = i32::from(self.system.sysclock.day) * 16;
                match n {
                    0 => {}
                    1 => {
                        if self.wthr_wreck(ship, sub, 1, 75) == 1 {
                            // TODO(stage 2): "%s struck a coastal reef.  There
                            // is minor damage to the ship."
                            let dam = self.wthr_wreck(ship, sub + 1, 3, 5);
                            self.add_structure_damage(ship, dam, true)?;
                        }
                    }
                    2 => {
                        if self.wthr_wreck(ship, sub + 2, 1, 50) == 1 {
                            // TODO(stage 2): "%s struck some submerged rocks.
                            // There is minor damage to the ship."
                            let dam = self.wthr_wreck(ship, sub + 3, 6, 9);
                            self.add_structure_damage(ship, dam, true)?;
                        }
                    }
                    3 => {
                        // TODO(stage 2): 1 or 2, "Hungry looking birds circle
                        // overhead."; 3, "Sharks circle in the water a short
                        // distance from the ship."
                        let _omen = self.wthr_wreck(ship, sub + 4, 1, 200);
                    }
                    _ => unreachable!("near_rocky_coast: {n}"),
                }
            }
            ship = next;
        }
        Ok(())
    }

    /// `olympia/day.c:mine_calamity`: an accident in `mine` kills up to five
    /// of the workers in it and damages it.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if the collapse reaches something not
    /// ported.
    fn mine_calamity(&mut self, mine: i32) -> Phase {
        let sub = i32::from(self.system.sysclock.day) * 16;

        // TODO(stage 2): which accident (cave-in, roof collapse, gas,
        // explosion) to everyone in the mine.
        let _what = self.wthr_wreck(mine, sub, 1, 4);

        let mut to_kill = self
            .wthr_wreck(mine, sub + 1, 1, 5)
            .min(count_loc_char_item(&self.db, mine, ITEM_WORKER));
        if to_kill > 0 {
            // TODO(stage 2): "%s miner%s %s killed in the accident.", and
            // "%s lost %s worker%s." to each.
            for i in all_char_here(&self.db, mine) {
                let m = has_item(&self.db, i, ITEM_WORKER);
                if m != 0 {
                    let n = m.min(to_kill);
                    self.consume_item(i, ITEM_WORKER, n);
                    to_kill -= n;
                    if to_kill <= 0 {
                        break;
                    }
                }
            }
        }

        let dam = self.wthr_wreck(mine, sub + 2, 1, 15);
        self.add_structure_damage(mine, dam, true)?;
        Ok(())
    }

    /// `olympia/day.c:inn_calamity`: rowdy customers damage the inn at
    /// `where_`.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if the collapse reaches something not
    /// ported.
    fn inn_calamity(&mut self, where_: i32) -> Phase {
        let sub = i32::from(self.system.sysclock.day) * 16;
        let _own = building_owner(&self.db, where_);
        let dam = self.wthr_wreck(where_, sub, 5, 15);
        // TODO(stage 2): who, what and the damage done, "%s:  %s, causing
        // %d points of damage." to the owner.
        let _who = self.wthr_wreck(where_, sub + 1, 1, 6);
        let _did = self.wthr_wreck(where_, sub + 2, 1, 5);
        let _broke = self.wthr_wreck(where_, sub + 3, 1, 7);
        self.add_structure_damage(where_, dam, true)?;
        Ok(())
    }

    /// `olympia/day.c:random_loc_damage`: mines may have an accident, the
    /// deeper the likelier, and inns a brawl, on the `wthr` stream.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if the damage reaches something not
    /// ported.
    fn random_loc_damage(&mut self) -> Phase {
        let sub = i32::from(self.system.sysclock.day) * 16;
        let mut where_ = self.db.kind_first(Kind::LOC);
        while where_ > 0 {
            let next = self.db.kind_next(where_);
            match self.db.subkind(where_) {
                Subkind::MINE => {
                    let depth = mine_depth(&self.db, where_);
                    if self.wthr_wreck(where_, sub + 8, 1, 90) <= depth {
                        self.mine_calamity(where_)?;
                    }
                }
                Subkind::INN if self.wthr_wreck(where_, sub + 9, 1, 100) == 1 => {
                    self.inn_calamity(where_)?;
                }
                _ => {}
            }
            where_ = next;
        }
        Ok(())
    }

    /// `olympia/day.c:dogs_bark_at_hidden_chars`: hounds may bark wherever
    /// a hidden character is.
    fn dogs_bark_at_hidden_chars(&self) {
        let db = &self.db;
        for where_ in db.boxes_of_kind(Kind::LOC) {
            let flag = here_list(db, where_)
                .into_iter()
                .any(|i| db.kind(i) == Kind::CHAR && char_really_hidden(db, i));
            if flag {
                self.bark_dogs(where_);
            }
        }
    }

    /// `olympia/day.c:heal_char_sup`: a wounded character heals 3 to 15
    /// points, or loses them while sick; the sick recover one time in 20,
    /// or 10 at an inn.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if the character dies of it
    /// (`kill_char`).
    fn heal_char_sup(&mut self, who: i32) -> Phase {
        let h = char_health(&self.db, who);
        if h >= 100 {
            return Ok(());
        }

        let inn = self.db.subkind(subloc(&self.db, who)) == Subkind::INN;
        let chance = if inn { 10 } else { 5 };

        if char_sick(&self.db, who) && self.up_heal(who, 0, 1, 100) <= chance {
            // TODO(stage 2): "%s defeated illness and is now recovering."
            self.db.p_char(who).sick = 0;
        }

        let mut amount = self.up_heal(who, 1, 3, 15);

        if char_sick(&self.db, who) {
            if h - amount < 0 {
                amount = h;
            }
            // TODO(stage 2): "%s lost %s health."
            let p = self.db.p_char(who);
            // schar: health stays within 0..=100 here.
            p.health = (i32::from(p.health) - amount) as i8;
            if char_health(&self.db, who) <= 0 {
                return not_ported("kill_char");
            }
        } else {
            if h + amount > 100 {
                amount = 100 - h;
            }
            // TODO(stage 2): "%s gained %s health.", and "%s is fully
            // healed." at 100.
            let p = self.db.p_char(who);
            p.health = (i32::from(p.health) + amount) as i8;
        }
        Ok(())
    }

    /// `olympia/day.c:default_garrison_pay`: each unit stacked under a
    /// default garrison unit (but another one, or a prisoner) is paid
    /// `garrison_pay` gold.
    fn default_garrison_pay(&mut self) {
        let pay = self.system.garrison_pay;
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if is_prisoner(&self.db, who) {
                continue;
            }
            let parent = stack_leader(&self.db, who);
            if parent == 0 || !default_garrison(&self.db, parent) {
                continue;
            }
            if default_garrison(&self.db, who) {
                continue;
            }
            self.gen_item(who, ITEM_GOLD, pay);
            // TODO(stage 2): gold_garrison += pay unless who is an NPC, and
            // "Received %s from %s." to who.
        }
    }

    /// `olympia/day.c:heal_characters`: the weekly healing of every
    /// wounded character.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if a character dies of sickness.
    fn heal_characters(&mut self) -> Phase {
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            let n = char_health(&self.db, who);
            if (0..100).contains(&n) {
                self.heal_char_sup(who)?;
            }
        }
        Ok(())
    }

    /// `olympia/day.c:increment_stone_ring_aura`: a magician in a stone
    /// circle gains a point of aura, up to its maximum.
    fn increment_stone_ring_aura(&mut self) {
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if !is_magician(&self.db, who)
                || self.db.subkind(subloc(&self.db, who)) != Subkind::STONE_CIR
            {
                continue;
            }
            let ma = max_eff_aura(&self.db, who);
            let p = self.db.p_magic(who);
            if p.cur_aura < ma {
                p.cur_aura += 1; // one point bonus
                // TODO(stage 2): "Current aura is now %d."
            }
        }
    }

    /// `olympia/day.c:noncreator_curse_erode`: a cursed item would wear
    /// down the loyalty of whoever holds it but its creator. C only logs
    /// "NOTYET!" for each: nothing changes.
    fn noncreator_curse_erode(&self) {
        let db = &self.db;
        for who in db.boxes_of_kind(Kind::CHAR) {
            for e in inventory(db, who) {
                let Some((curse, creator)) = item_curse(db, e.item) else {
                    continue;
                };
                let erodes = curse != 0 && creator != who && loyal_kind(db, who) != LOY_OATH;
                // TODO(stage 2): log_write(LOG_CODE, "noncreator_curse_erode:
                // NOTYET!") when it erodes.
                let _ = erodes;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];

    fn game(turn: i16, day: i16) -> Game {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new(MASTER);
        g.system.sysclock.turn = turn;
        g.system.sysclock.day = day;
        g
    }

    /// The `caln` and `upkp` draws for turn 3 under `MASTER`, from day.c's
    /// helpers run over lib/rng.c.
    #[test]
    fn calendar_and_upkeep_draws_match_c() {
        let g = game(3, 7);
        assert_eq!(
            [
                g.cal_day(0, 1, 30),
                g.cal_day(1, 15, 30),
                g.cal_day(2, 1, 30)
            ],
            [20, 19, 24]
        );
        assert_eq!(
            [g.up_heal(1234, 0, 1, 100), g.up_heal(1234, 1, 3, 15)],
            [60, 10]
        );
        let g = game(3, 14);
        assert_eq!(
            [g.up_heal(1234, 0, 1, 100), g.up_heal(1234, 1, 3, 15)],
            [59, 15]
        );
    }

    /// The month-end `upkp` leaves for turn 3 under `MASTER`, from
    /// `rng_keyed` over lib/rng.c with day.c's tags (the animal is item
    /// 52).
    #[test]
    fn month_end_upkeep_draws_match_c() {
        let g = game(3, 30);
        assert_eq!(
            [
                g.up_loyal(1234, 0, 1, 2),
                g.up_loyal(1234, 1, 1, 2),
                g.up_loyal(5678, 0, 1, 2),
                g.up_loyal(5678, 1, 1, 2)
            ],
            [2, 1, 1, 2]
        );
        assert_eq!(
            [
                g.up_starve(1234, 12, 1, 2),
                g.up_starve(1234, 19, 1, 2),
                g.up_starve(5678, 12, 1, 2)
            ],
            [1, 1, 2]
        );
        let dier: Vec<i32> = (1..=6).map(|i| g.up_animal(1234, 52, i, 1, 1000)).collect();
        assert_eq!(dier, [923, 945, 191, 357, 465, 857]);
        assert_eq!(
            [
                g.up_corpse(1234, 0, 2),
                g.up_corpse(5678, 0, 2),
                g.up_corpse(9012, 0, 2)
            ],
            [2, 1, 1]
        );
        assert_eq!(
            [
                g.up_income(61234, 0, 50, 75),
                g.up_income(61234, 1, 1, 8),
                g.up_income(61234, 2, 5, 13),
                g.up_income(61234, 3, 1, 3)
            ],
            [69, 5, 7, 2]
        );
    }
}
