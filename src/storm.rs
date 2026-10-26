// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Weather: storms and the `wthr` stream (`olympia/storm.c`), and the
//! month-end storm routines and the weather-day schedule from
//! `olympia/day.c`.
//!
//! `wthr` is one sequential stream per turn, rooted at the turn:
//! `rng_stream_of(rng_stream_of(root, turn, "turn"), 0, "wthr")`. It is
//! seeded the first time weather draws in a turn (`begin_weather()`), and
//! its counter runs across the whole turn: the day 1 weather-day shuffle,
//! then each weather day's province shuffles. Storm strengths and acute
//! damage are keyed draws on the same stream and don't move its counter.
//! See olympia-g3's `doc/claude/rng-streams.md`.
//!
//! The spell commands (`summon rain`, `direct storm`, `dissipate`, `death
//! fog`, ...) are stage 4 magic and aren't here. Neither is
//! `storm_report()`, which is report output (stage 2).
//!
//! TODO(stage 2): the "It has begun to rain." family of `wout()` messages
//! to the locations, and to a storm's summoner, are report output.

use crate::entity::{Db, Kind, MONTH_DAYS, Subkind};
use crate::game::{Game, player};
use crate::loc::{
    LOC_PROVINCE, all_char_here, greater_region, loc_depth, province, subloc, weather_here,
};
use crate::lore::set_known;
use crate::rng::{Stream, Tag};
use crate::turn::{Phase, not_ported};
use crate::unit::is_prisoner;

/// `TAG_TURN`: the per-turn parent of the `wthr` stream.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_WEATHER`.
const TAG_WEATHER: Tag = Tag::new(b"wthr");
/// `WTAG_STORM`: storm-strength rolls.
const WTAG_STORM: Tag = Tag::new(b"stom");
/// `WTAG_WRECK`: acute damage to ships, mines and inns.
const WTAG_WRECK: Tag = Tag::new(b"wrck");

/// How many of the shuffled days of the month are weather days.
const WEATHER_DAYS: usize = 4;

/// The turn's weather state: the `wthr` stream (`weather_rng`,
/// `weather_rng_turn`), the province count `nprovinces()` caches, and
/// `daily_events()`'s weather-day schedule (its `static` `weather_days` and
/// `wday_index`).
#[derive(Debug, Clone, Default)]
pub struct Weather {
    seeded: Option<(i16, Stream)>,
    nprov: i32,
    /// The days of the month, shuffled, with the first four sorted: those
    /// four are the weather days. Empty until the schedule is drawn.
    weather_days: Vec<i32>,
    /// How many weather days have passed.
    wday_index: usize,
}

impl Weather {
    /// The stream for `turn`, seeding it from `master` if it hasn't been
    /// seeded for this turn (`begin_weather()`).
    fn stream(&mut self, master: [u32; 4], turn: i16) -> &mut Stream {
        if self.seeded.as_ref().is_none_or(|&(t, _)| t != turn) {
            let stream = Stream::new(master)
                .child(i32::from(turn), TAG_TURN)
                .child(0, TAG_WEATHER);
            self.seeded = Some((turn, stream));
        }
        &mut self.seeded.as_mut().expect("seeded above").1
    }

    /// The weather days of the month, once the schedule has been drawn.
    #[must_use]
    pub fn weather_days(&self) -> &[i32] {
        let n = self.weather_days.len().min(WEATHER_DAYS);
        &self.weather_days[..n]
    }
}

/// `storm_strength(n)`.
#[must_use]
pub fn storm_strength(db: &Db, n: i32) -> i32 {
    db.get(n)
        .and_then(|b| b.misc.as_ref())
        .map_or(0, |m| i32::from(m.storm_str))
}

/// `npc_summoner(n)`: who summoned a storm or NPC, or 0.
#[must_use]
pub fn npc_summoner(db: &Db, n: i32) -> i32 {
    db.get(n)
        .and_then(|b| b.misc.as_ref())
        .map_or(0, |m| m.summoned_by)
}

/// `storm_bind(n)`: the ship a storm is bound to, or 0.
#[must_use]
pub fn storm_bind(db: &Db, n: i32) -> i32 {
    db.get(n)
        .and_then(|b| b.misc.as_ref())
        .map_or(0, |m| m.bind_storm)
}

/// `weather_mage(n)`: whether a character knows the weather.
#[must_use]
pub fn weather_mage(db: &Db, n: i32) -> bool {
    db.get(n)
        .and_then(|b| b.char_.as_ref())
        .and_then(|c| c.magic.as_ref())
        .is_some_and(|m| m.knows_weather != 0)
}

/// The `default: assert(FALSE)` of the weather-message switches: a storm
/// must be rain, wind or fog.
fn assert_storm_subkind(sk: Subkind) {
    assert!(
        matches!(sk, Subkind::RAIN | Subkind::WIND | Subkind::FOG),
        "storm of subkind {}",
        sk.0
    );
}

impl Game {
    /// Shuffles `l` with sequential draws on the `wthr` stream
    /// (`olympia/storm.c:wthr_shuffle`).
    pub fn wthr_shuffle(&mut self, l: &mut [i32]) {
        let master = self.seed.master_seed();
        let turn = self.system.sysclock.turn;
        self.weather.stream(master, turn).shuffle(l);
    }

    /// A storm-strength roll for province `where_`, keyed on `k2`
    /// (`olympia/storm.c:wthr_storm`). Doesn't move the stream's counter.
    pub fn wthr_storm(&mut self, where_: i32, k2: i32, low: i32, high: i32) -> i32 {
        let master = self.seed.master_seed();
        let turn = self.system.sysclock.turn;
        self.weather
            .stream(master, turn)
            .keyed(where_, k2, WTAG_STORM, low, high)
    }

    /// An acute-damage roll for a ship, mine or inn, keyed on `where_` and
    /// `sub` (`olympia/storm.c:wthr_wreck`). Doesn't move the stream's
    /// counter.
    pub fn wthr_wreck(&mut self, where_: i32, sub: i32, low: i32, high: i32) -> i32 {
        let master = self.seed.master_seed();
        let turn = self.system.sysclock.turn;
        self.weather
            .stream(master, turn)
            .keyed(where_, sub, WTAG_WRECK, low, high)
    }

    /// `olympia/u.c:nprovinces`: how many provinces there are, counted the
    /// first time it is asked and cached for the rest of the run.
    pub fn nprovinces(&mut self) -> i32 {
        if self.weather.nprov != 0 {
            return self.weather.nprov;
        }
        let db = &self.db;
        let n = db
            .boxes_of_kind(Kind::LOC)
            .into_iter()
            .filter(|&i| loc_depth(db, i) == LOC_PROVINCE)
            .count();
        self.weather.nprov = i32::try_from(n).expect("province count fits an int");
        self.weather.nprov
    }

    /// Draws the month's weather days, the first time it is called
    /// (`olympia/day.c:daily_events`, the `weather_days` block): the days
    /// 1 to `MONTH_DAYS` shuffled on the `wthr` stream, with the first four
    /// sorted.
    ///
    /// `daily_events` calls this each day, after the `caln` day picks and
    /// before `default_garrison_pay()`.
    pub fn schedule_weather_days(&mut self) {
        if !self.weather.weather_days.is_empty() {
            return;
        }
        let mut days: Vec<i32> = (1..=i32::from(MONTH_DAYS)).collect();
        self.wthr_shuffle(&mut days);
        // safe_qsort(weather_days, 4, sizeof(int), int_comp)
        days[..WEATHER_DAYS].sort_unstable();
        self.weather.weather_days = days;
    }

    /// Raises the natural weather if today is the next weather day
    /// (`olympia/day.c:daily_events`, the `wday_index` block). Only the four
    /// sorted days count (playbymail/olympia-g3#229).
    ///
    /// `daily_events` calls this each day, after
    /// `dogs_bark_at_hidden_chars()` and before `auto_faery()`.
    ///
    /// # Panics
    ///
    /// Panics if the weather days haven't been drawn
    /// ([`schedule_weather_days`](Self::schedule_weather_days)), where C
    /// would read a null list.
    pub fn weather_day(&mut self) {
        assert!(
            !self.weather.weather_days.is_empty(),
            "weather_day: no weather days drawn"
        );
        let i = self.weather.wday_index;
        if i < WEATHER_DAYS && i32::from(self.system.sysclock.day) == self.weather.weather_days[i] {
            self.weather.wday_index += 1;
            self.natural_weather();
        }
    }

    /// `olympia/storm.c:natural_weather`: raises the month's natural storms
    /// over the ordinary world. Called on each of the four weather days.
    ///
    /// # Panics
    ///
    /// Panics at turn 0, whose month is -1, as C asserts.
    pub fn natural_weather(&mut self) {
        // One natural storm per 4 provinces; half made each month, over
        // four calls.
        let n = self.nprovinces() / 4 / 2 / 4;

        match self.system.sysclock.month() {
            // Fierce winds
            0 => {
                self.create_some_storms(n, Subkind::FOG);
                self.create_some_storms(n, Subkind::WIND);
            }
            // Snowmelt
            1 => {
                self.create_some_storms(n, Subkind::FOG);
                self.create_some_storms(n, Subkind::RAIN);
            }
            // Blossom bloom, Harvest
            2 | 5 => {}
            // Sunsear
            3 => self.create_some_storms(n, Subkind::RAIN),
            // Thunder and rain
            4 => self.create_some_storms(n * 2, Subkind::RAIN),
            // Waning days
            6 => {
                self.create_some_storms(n, Subkind::RAIN);
                self.create_some_storms(n, Subkind::FOG);
                self.create_some_storms(n, Subkind::RAIN);
            }
            // Dark night
            7 => self.create_some_storms(n, Subkind::WIND),
            m => panic!("natural_weather: month {m}"),
        }
    }

    /// `olympia/storm.c:create_some_storms`: raises up to `num` storms of
    /// subkind `sk`, of strength 2 or 3, in provinces of the ordinary world
    /// that have none of that weather, chosen by a shuffle on the `wthr`
    /// stream.
    fn create_some_storms(&mut self, num: i32, sk: Subkind) {
        let db = &self.db;
        let mut l: Vec<i32> = db
            .boxes_of_kind(Kind::LOC)
            .into_iter()
            .filter(|&i| {
                loc_depth(db, i) == LOC_PROVINCE
                    && greater_region(db, &self.system, i) == 0
                    && weather_here(db, i, sk) == 0
            })
            .collect();

        self.wthr_shuffle(&mut l);

        let k2 = i32::from(self.system.sysclock.day) * 256 + i32::from(sk.0);
        for &where_ in l.iter().take(usize::try_from(num).unwrap_or(0)) {
            let aura = self.wthr_storm(where_, k2, 2, 3);
            self.new_storm(0, sk, aura, where_);
        }
    }

    /// `olympia/storm.c:new_storm`: makes a storm of subkind `sk` and
    /// strength `aura` in province `where_`. `new` is the storm's box, or 0
    /// to allocate one. Returns 0, or -1 if no box could be allocated.
    ///
    /// # Panics
    ///
    /// Panics unless `sk` is rain, wind or fog and `where_` is a province,
    /// as C asserts.
    pub fn new_storm(&mut self, new: i32, sk: Subkind, aura: i32, where_: i32) -> i32 {
        assert_storm_subkind(sk);
        assert_eq!(
            loc_depth(&self.db, where_),
            LOC_PROVINCE,
            "new_storm: {where_} is not a province"
        );

        // TODO(stage 2): `before = weather_here(where, sk)` decides whether
        // to wout() that the weather has begun.

        let new = if new == 0 {
            let n = self.new_ent(Kind::STORM, sk);
            if n <= 0 {
                return -1;
            }
            n
        } else {
            new
        };

        // C stores it in a short.
        self.db.p_misc(new).storm_str = aura as i16;
        self.db.set_where(new, where_);
        0
    }

    /// `olympia/storm.c:move_storm`: moves a storm to `dest`, showing its
    /// summoner's faction the new location.
    ///
    /// # Panics
    ///
    /// Panics if the storm leaves no weather of its kind behind and isn't
    /// rain, wind or fog, as C asserts.
    pub fn move_storm(&mut self, storm: i32, dest: i32) {
        let orig = subloc(&self.db, storm);
        let sk = self.db.subkind(storm);

        // TODO(stage 2): `before = weather_here(dest, sk)` decides whether
        // to wout() that the weather has begun at dest.

        self.db.set_where(storm, dest);

        let owner = npc_summoner(&self.db, storm);
        if self.db.valid_box(owner) && self.db.valid_box(player(&self.db, owner)) {
            self.touch_loc_pl(player(&self.db, owner), dest);
        }

        if weather_here(&self.db, orig, sk) == 0 {
            // TODO(stage 2): wout(orig, "It has stopped raining.") and kin.
            assert_storm_subkind(sk);
        }
    }

    /// `olympia/storm.c:move_bound_storms`: the storms bound to `ship`
    /// follow it to the province of `where_`.
    ///
    /// # Panics
    ///
    /// Panics if a bound storm is no longer a storm. C means to drop it from
    /// the list, but passes the storm's number to `ilist_delete()` as the
    /// index, which asserts unless the number happens to be a valid index.
    /// That is copied: a storm in range is removed by that index.
    pub fn move_bound_storms(&mut self, ship: i32, where_: i32) {
        if self.db.get(ship).and_then(|b| b.subloc.as_ref()).is_none() {
            return;
        }
        let bound = |db: &Db| {
            db.entity(ship)
                .subloc
                .as_ref()
                .map_or(&[][..], |s| s.bound_storms.as_slice())
                .to_vec()
        };

        let mut i: i32 = 0;
        loop {
            let l = bound(&self.db);
            let Some(&storm) = usize::try_from(i).ok().and_then(|ix| l.get(ix)) else {
                break;
            };
            if self.db.kind(storm) != Kind::STORM {
                let s = self.db.entity_mut(ship).subloc.as_mut().expect("checked");
                // C passes the storm, not its index, so this panics for any
                // real storm (playbymail/olympia-g3#250).
                s.bound_storms.delete(storm);
                i -= 1;
            } else {
                let dest = province(&self.db, where_);
                self.move_storm(storm, dest);
            }
            i += 1;
        }
    }

    /// `olympia/storm.c:dissipate_storm`: removes a storm from the world
    /// and deletes it. A summoned storm's cookie goes back to the NPC home
    /// it came from. `show` is whether to tell the location the weather has
    /// cleared (report output).
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) for a
    /// storm with a cookie to return: `gen_item()` isn't ported yet.
    ///
    /// # Panics
    ///
    /// Panics if `storm` isn't a storm, or, with `show`, if it leaves no
    /// weather of its kind behind and isn't rain, wind or fog, as C asserts.
    pub fn dissipate_storm(&mut self, storm: i32, show: bool) -> Phase {
        assert_eq!(self.db.kind(storm), Kind::STORM, "dissipate_storm: {storm}");
        let where_ = subloc(&self.db, storm);

        // TODO(stage 2): wout(owner, "%s has dissipated.") to a character
        // summoner.

        if show {
            let sk = self.db.subkind(storm);
            if weather_here(&self.db, where_, sk) == 0 {
                // TODO(stage 2): wout(where, "It has stopped raining.") and
                // kin.
                assert_storm_subkind(sk);
            }
        }

        self.db.set_where(storm, 0);

        let p = self.db.p_misc(storm);
        if p.npc_home != 0 && p.npc_cookie != 0 {
            // gen_item(p->npc_home, p->npc_cookie, 1)
            return not_ported("gen_item");
        }

        let ship = storm_bind(&self.db, storm);
        if ship != 0 {
            if let Some(s) = self.db.entity_mut(ship).subloc.as_mut() {
                s.bound_storms.rem_value(storm);
            }
            self.db.p_misc(storm).bind_storm = 0;
        }

        self.db.delete_box(storm);
        Ok(())
    }

    /// `olympia/storm.c:update_weather_view_loc_sup`: `who`'s faction sees
    /// the weather in `where_`.
    ///
    /// # Panics
    ///
    /// Panics if `who` has no faction, as C asserts.
    fn update_weather_view_loc_sup(&mut self, who: i32, where_: i32) {
        let pl = player(&self.db, who);
        assert!(self.db.valid_box(pl), "{who} has no faction");
        let seen = &mut self.db.p_player(pl).weather_seen;
        // set_bit()
        if !seen.contains(where_) {
            seen.append(where_);
        }
    }

    /// `olympia/storm.c:update_weather_view_locs`: the weather mages in
    /// `stack` see the weather in the province of `where_`. Called when a
    /// stack arrives somewhere.
    pub fn update_weather_view_locs(&mut self, stack: i32, where_: i32) {
        let where_ = province(&self.db, where_);

        if self.db.kind(stack) == Kind::CHAR && weather_mage(&self.db, stack) {
            self.update_weather_view_loc_sup(stack, where_);
        }

        for i in all_char_here(&self.db, stack) {
            if !is_prisoner(&self.db, i) && weather_mage(&self.db, i) {
                self.update_weather_view_loc_sup(i, where_);
            }
        }
    }

    /// `olympia/storm.c:init_weather_views`: each weather mage's faction
    /// sees the weather where the mage starts the turn.
    pub fn init_weather_views(&mut self) {
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if weather_mage(&self.db, who) {
                let where_ = province(&self.db, who);
                self.update_weather_view_loc_sup(who, where_);
            }
        }
    }

    /// `olympia/storm.c:can_see_weather_here`: whether `who`'s faction has
    /// seen the weather in the province of `where_` this turn.
    ///
    /// # Panics
    ///
    /// Panics if `who` has no faction, as C asserts.
    #[must_use]
    pub fn can_see_weather_here(&self, who: i32, where_: i32) -> bool {
        let pl = player(&self.db, who);
        assert!(self.db.valid_box(pl), "{who} has no faction");
        let where_ = province(&self.db, where_);
        self.db
            .get(pl)
            .and_then(|b| b.player.as_ref())
            .is_some_and(|p| p.weather_seen.contains(where_))
    }

    /// `olympia/day.c:storm_decay`: each storm weakens by one at month end,
    /// and dissipates when it is spent.
    ///
    /// # Errors
    ///
    /// As [`dissipate_storm`](Self::dissipate_storm).
    pub fn storm_decay(&mut self) -> Phase {
        // loop_storm: read the next storm before the body deletes this one.
        let mut next = self.db.kind_first(Kind::STORM);
        while next > 0 {
            let i = next;
            next = self.db.kind_next(i);

            let p = self.db.p_misc(i);
            // C's short decrement truncates on the store.
            p.storm_str = p.storm_str.wrapping_sub(1);
            if p.storm_str > 0 {
                continue;
            }
            p.storm_str = 0;

            self.dissipate_storm(i, true)?;
        }
        Ok(())
    }

    /// `olympia/day.c:storm_move`: each storm its summoner directed this
    /// month moves to the province it was sent to.
    ///
    /// # Panics
    ///
    /// Panics if a directed storm's destination isn't a province, as C
    /// asserts.
    pub fn storm_move(&mut self) {
        for i in self.db.boxes_of_kind(Kind::STORM) {
            let p = self.db.p_misc(i);
            if p.npc_dir == 0 {
                continue;
            }
            let dest = p.storm_move;

            assert_eq!(
                loc_depth(&self.db, dest),
                LOC_PROVINCE,
                "storm_move: {i} sent to {dest}, not a province"
            );

            self.db.set_where(i, dest);

            let owner = npc_summoner(&self.db, i);
            if owner != 0 && self.db.valid_box(owner) {
                set_known(&mut self.db, owner, dest);
            }

            let p = self.db.p_misc(i);
            p.npc_dir = 0;
            p.storm_move = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // The expected values come from olympia-g3's lib/rng.c and
    // lib/ilist.c (ilist_shuffle_rng) run with this master seed: the
    // daily_events weather_days block, then a shuffle of provinces 20000 to
    // 20009 on the same stream, then wthr_storm and wthr_wreck rolls.
    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];

    fn game(turn: i16) -> Game {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new(MASTER);
        g.system.sysclock.turn = turn;
        g
    }

    /// Region 58760 holding provinces 20000 to 20009.
    fn world(turn: i16) -> Game {
        let mut g = game(turn);
        g.db.alloc_box(58760, Kind::LOC, Subkind::REGION);
        for p in 20000..20010 {
            g.db.alloc_box(p, Kind::LOC, Subkind::FOREST);
            g.db.set_where(p, 58760);
        }
        g
    }

    #[test]
    fn weather_days_match_c() {
        let mut g = game(3);
        g.schedule_weather_days();
        assert_eq!(
            g.weather.weather_days,
            [
                4, 6, 12, 24, 19, 18, 9, 22, 8, 15, 21, 2, 3, 1, 13, 7, 23, 16, 14, 28, 5, 17, 10,
                29, 20, 25, 26, 11, 30, 27
            ]
        );
        assert_eq!(g.weather.weather_days(), [4, 6, 12, 24]);
        // Drawn once: a second call draws nothing.
        g.schedule_weather_days();
        let mut l: Vec<i32> = (20000..20010).collect();
        g.wthr_shuffle(&mut l);
        assert_eq!(
            l,
            [
                20000, 20009, 20005, 20002, 20008, 20007, 20006, 20003, 20004, 20001
            ]
        );
    }

    #[test]
    fn keyed_rolls_match_c() {
        let mut g = game(3);
        let k2 = 5 * 256 + i32::from(Subkind::WIND.0);
        let rolls: Vec<i32> = (20000..20010).map(|p| g.wthr_storm(p, k2, 2, 3)).collect();
        assert_eq!(rolls, [3, 3, 2, 2, 2, 2, 2, 2, 3, 2]);
        assert_eq!(g.wthr_wreck(30000, 7, 1, 75), 7);
        assert_eq!(g.wthr_wreck(30001, 8, 1, 200), 2);
    }

    #[test]
    fn storms_go_to_the_first_shuffled_provinces() {
        let mut g = world(3);
        g.schedule_weather_days();
        g.system.sysclock.day = 5;
        g.create_some_storms(3, Subkind::WIND);
        let storms: Vec<(i32, i32, i32)> =
            g.db.boxes_of_kind(Kind::STORM)
                .into_iter()
                .map(|s| {
                    (
                        g.db.loc(s),
                        storm_strength(&g.db, s),
                        g.db.subkind(s).0.into(),
                    )
                })
                .collect();
        let mut got: Vec<(i32, i32)> = storms.iter().map(|&(w, st, _)| (w, st)).collect();
        got.sort_unstable();
        assert_eq!(got, [(20000, 3), (20005, 2), (20009, 2)]);
        assert!(
            storms
                .iter()
                .all(|&(_, _, sk)| sk == i32::from(Subkind::WIND.0))
        );
        assert_eq!(weather_here(&g.db, 20000, Subkind::WIND), 3);
        assert_eq!(weather_here(&g.db, 20000, Subkind::RAIN), 0);
    }

    #[test]
    fn provinces_with_that_weather_or_in_a_special_region_are_skipped() {
        let mut g = world(3);
        // A second region, made Faery, holding province 20010.
        g.db.alloc_box(58761, Kind::LOC, Subkind::REGION);
        g.db.alloc_box(20010, Kind::LOC, Subkind::FOREST);
        g.db.set_where(20010, 58761);
        g.system.faery_region = 58761;
        for p in 20000..20009 {
            assert_eq!(g.new_storm(0, Subkind::RAIN, 2, p), 0);
        }
        g.create_some_storms(5, Subkind::RAIN);
        let rain = g.db.boxes_of_kind(Kind::STORM);
        assert_eq!(rain.len(), 10);
        // Only 20009 was clear of rain, so the one new storm is there.
        assert!(matches!(weather_here(&g.db, 20009, Subkind::RAIN), 2 | 3));
        assert_eq!(weather_here(&g.db, 20010, Subkind::RAIN), 0);
    }

    #[test]
    fn natural_weather_makes_an_eighth_of_a_quarter_of_the_provinces() {
        let mut g = world(1);
        // 10 provinces make no storms: 10 / 4 / 2 / 4 == 0.
        g.natural_weather();
        assert!(g.db.boxes_of_kind(Kind::STORM).is_empty());
        assert_eq!(g.nprovinces(), 10);
    }

    #[test]
    fn weather_day_runs_on_the_four_sorted_days_only() {
        let mut g = world(3);
        g.schedule_weather_days();
        let mut fired = Vec::new();
        for day in 1..=MONTH_DAYS {
            g.system.sysclock.day = day;
            let before = g.weather.wday_index;
            g.weather_day();
            if g.weather.wday_index != before {
                fired.push(day);
            }
        }
        assert_eq!(fired, [4, 6, 12, 24]);
    }

    #[test]
    fn storms_decay_and_dissipate() {
        let mut g = world(3);
        g.new_storm(0, Subkind::FOG, 2, 20001);
        g.new_storm(0, Subkind::FOG, 1, 20002);
        g.new_storm(0, Subkind::RAIN, 3, 20002);
        g.storm_decay().unwrap();
        let left: Vec<(i32, i32)> =
            g.db.boxes_of_kind(Kind::STORM)
                .into_iter()
                .map(|s| (g.db.loc(s), storm_strength(&g.db, s)))
                .collect();
        let mut left = left;
        left.sort_unstable();
        assert_eq!(left, [(20001, 1), (20002, 2)]);
        assert_eq!(weather_here(&g.db, 20002, Subkind::FOG), 0);
        assert!(g.db.entity(20002).loc_info.here_list.len() == 1);
    }

    #[test]
    fn dissipating_unbinds_the_storm() {
        let mut g = world(3);
        g.db.alloc_box(60000, Kind::SHIP, Subkind::GALLEY);
        g.db.set_where(60000, 20003);
        let s = g.db.kind_first(Kind::STORM);
        assert_eq!(s, 0);
        g.new_storm(0, Subkind::WIND, 1, 20003);
        let s = g.db.kind_first(Kind::STORM);
        g.db.p_misc(s).bind_storm = 60000;
        g.db.entity_mut(60000)
            .subloc
            .get_or_insert_with(Default::default)
            .bound_storms
            .append(s);
        g.storm_decay().unwrap();
        assert_eq!(g.db.kind(s), Kind::DELETED);
        assert!(
            g.db.entity(60000)
                .subloc
                .as_ref()
                .unwrap()
                .bound_storms
                .is_empty()
        );
    }

    #[test]
    fn a_storm_with_a_cookie_stops_at_gen_item() {
        let mut g = world(3);
        g.new_storm(0, Subkind::RAIN, 1, 20003);
        let s = g.db.kind_first(Kind::STORM);
        let m = g.db.p_misc(s);
        m.npc_home = 20003;
        m.npc_cookie = 300;
        assert!(g.storm_decay().is_err());
    }

    #[test]
    fn directed_storms_move_and_their_summoner_learns_where() {
        let mut g = world(3);
        g.db.alloc_box(50001, Kind::PLAYER, Subkind::PL_REGULAR);
        g.db.alloc_box(1001, Kind::CHAR, Subkind::default());
        g.db.p_char(1001).unit_lord = 50001;
        g.db.set_where(1001, 20000);
        g.new_storm(0, Subkind::RAIN, 2, 20000);
        g.new_storm(0, Subkind::RAIN, 2, 20000);
        let storms = g.db.boxes_of_kind(Kind::STORM);
        let m = g.db.p_misc(storms[0]);
        m.summoned_by = 1001;
        m.npc_dir = 2;
        m.storm_move = 20004;
        g.storm_move();
        assert_eq!(g.db.loc(storms[0]), 20004);
        assert_eq!(g.db.loc(storms[1]), 20000);
        let m = g.db.entity(storms[0]).misc.as_ref().unwrap();
        assert_eq!((m.npc_dir, m.storm_move), (0, 0));
        assert!(
            g.db.entity(50001)
                .player
                .as_ref()
                .unwrap()
                .known
                .contains(20004)
        );
    }

    #[test]
    fn weather_mages_see_where_they_start() {
        let mut g = world(3);
        g.db.alloc_box(50001, Kind::PLAYER, Subkind::PL_REGULAR);
        for (who, mage, at) in [
            (1001, true, 20000),
            (1002, false, 20001),
            (1003, true, 1001),
        ] {
            g.db.alloc_box(who, Kind::CHAR, Subkind::default());
            g.db.p_char(who).unit_lord = 50001;
            if mage {
                g.db.p_magic(who).knows_weather = 1;
            }
            g.db.set_where(who, at);
        }
        g.init_weather_views();
        assert!(g.can_see_weather_here(1002, 20000));
        assert!(!g.can_see_weather_here(1002, 20001));
        g.update_weather_view_locs(1001, 20005);
        assert!(g.can_see_weather_here(1001, 20005));
        assert_eq!(
            g.db.entity(50001)
                .player
                .as_ref()
                .unwrap()
                .weather_seen
                .as_slice(),
            [20000, 20005]
        );
    }
}
