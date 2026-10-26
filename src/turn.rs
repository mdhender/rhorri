// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Running a turn (`-r`): `main()`'s run block (`olympia/main.c`),
//! `process_orders()` (`olympia/input.c`), `post_month()`
//! (`olympia/day.c`) and the report phase.
//!
//! Every phase runs in C order; a routine a phase reaches that isn't
//! ported yet stops the run with [`NotYetImplemented`] naming it, rather
//! than being skipped. The month's end routines are in [`crate::month`].
//!
//! The report phase is the exception: until stage 2 its routines print
//! their `stage()` line and write no report. What they change in the
//! database is ported, since it is saved: `determine_output_order()`
//! sorts the factions' unit lists, printing sorts inventories and
//! attitude and admit lists in place, the character reports collect the
//! pledge backlinks, and queuing lore marks it known.
//!
//! `phase_ran()` and `phase_acted()` (the `--phase-coverage` counters) are
//! left out: `--phase-coverage` isn't ported yet, and stops the run before
//! anything is loaded.

use std::fs::{self, File};

use crate::command::find_command;
use crate::entity::{Db, Subkind};
use crate::entity::{Kind, MONTH_DAYS};
use crate::game::{Game, NotYetImplemented, player, stage};
use crate::loc::{
    LOC_PROVINCE, LOC_REGION, all_here, castle_level, loc_depth, major_city, province, subloc,
    viewloc,
};
use crate::lore::{scan_char_item_lore, scan_char_skill_lore};
use crate::os::strerror;
use crate::seed::ITEM_TAX_COOKIE;
use crate::unit::{char_pledge, has_item, is_prisoner, item_unique};

/// `sub_pl_silent`: a faction that gets no reports.
const SUB_PL_SILENT: Subkind = Subkind(62);

/// `olympia/perm.c:print_att`: `n`'s attitudes. TODO(stage 2): the report
/// lines. `print_att_sup()` sorts each list it prints in place.
fn print_att(db: &mut Db, n: i32) {
    if let Some(d) = db.get_mut(n).and_then(|b| b.disp.as_deref_mut()) {
        for l in [&mut d.hostile, &mut d.neutral, &mut d.defend] {
            if !l.is_empty() {
                l.sort();
            }
        }
    }
}

/// `RELIC_CROWN`: the Crown of Prosperity.
const RELIC_CROWN: i32 = 402;

/// The result of a phase: done, or stopped at a routine not ported yet.
pub type Phase = Result<(), NotYetImplemented>;

/// Stops the run at `routine`, which isn't ported yet.
pub(crate) fn not_ported(routine: &str) -> Phase {
    Err(NotYetImplemented(routine.to_owned()))
}

impl Game {
    /// Runs a turn: `main()`'s `if (run_flag)` block. `acct` is `-A`.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] at the first routine the turn reaches
    /// that isn't ported yet.
    pub fn run_turn(&mut self, acct: bool) -> Phase {
        // TODO(stage 2): open_logfile() empties lib/log, and open_times()
        // starts lib/times_press and makes lib/log. Both are report output.

        // show_day = TRUE: only the report output reads it.
        self.process_orders()?;
        self.post_month()?;
        // show_day = FALSE

        self.report_phase(acct)
    }

    /// `olympia/input.c:process_orders`: day 0, then each day of the month.
    fn process_orders(&mut self) -> Phase {
        stage("process_orders()");

        self.cmd_wait = find_command(b"wait").map_or(-1, |(i, _)| i);
        assert!(self.cmd_wait > 0, "process_orders: no wait command");

        self.init_locs_touched();
        self.init_weather_views();
        self.system.sysclock.turn_change();

        self.init_wait_list();
        self.init_collect_list();
        self.initial_command_load()?;
        self.queue_npc_orders()?;
        self.ping_garrisons();
        self.check_token_units()?; // "XXX/NOTYET -- temp fix" in C

        self.process_interrupted_units()?; // day 0
        self.process_player_orders()?;
        scan_char_item_lore(&mut self.db);

        stage("");

        while self.system.sysclock.day < MONTH_DAYS {
            self.system.sysclock.increment();

            if self.system.sysclock.day == 1 {
                self.match_all_trades()?;
            }

            self.print_dot(b'.');

            self.daily_command_loop()?;
            self.daily_events()?;
        }
        eprintln!();

        self.month_done = true;
        Ok(())
    }

    /// `olympia/day.c:init_locs_touched`: notes where every free
    /// character, and every summoned storm, is at the start of the turn.
    fn init_locs_touched(&mut self) {
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            let prisoner = self
                .db
                .entity(who)
                .char_
                .as_ref()
                .is_some_and(|c| c.prisoner != 0);
            if !prisoner {
                self.touch_loc(who);
            }
        }
        self.storm_owner_touch_loc();
        // touch_garrison_locs() is compiled out (#if 0) in C.
    }

    /// `olympia/day.c:touch_loc`.
    pub(crate) fn touch_loc(&mut self, who: i32) {
        let where_ = subloc(&self.db, who);
        self.touch_loc_pl(player(&self.db, who), where_);
    }

    /// `olympia/day.c:touch_loc_pl`: records that faction `pl` saw
    /// `where_`, and what holds it if that is below a province.
    pub(crate) fn touch_loc_pl(&mut self, pl: i32, where_: i32) {
        let where_ = viewloc(&self.db, where_);
        let inside = (loc_depth(&self.db, where_) > LOC_PROVINCE).then(|| self.db.loc(where_));
        let locs = &mut self.db.p_player(pl).locs;
        for n in std::iter::once(where_).chain(inside) {
            // set_bit()
            if !locs.contains(n) {
                locs.append(n);
            }
        }
    }

    /// `olympia/day.c:storm_owner_touch_loc`: a summoned storm shows its
    /// summoner's faction where it is.
    fn storm_owner_touch_loc(&mut self) {
        for i in self.db.boxes_of_kind(Kind::STORM) {
            let owner = self.db.entity(i).misc.as_ref().map_or(0, |m| m.summoned_by);
            if owner != 0 {
                let where_ = subloc(&self.db, i);
                let pl = player(&self.db, owner);
                if pl != 0 {
                    self.touch_loc_pl(pl, where_);
                }
            }
        }
    }

    /// `olympia/u.c:print_dot`: the day dots on standard error, wrapped
    /// every 60.
    pub fn print_dot(&mut self, c: u8) {
        if self.dot_count == 0 {
            eprint!("   ");
            self.dot_count += 1;
        }
        self.dot_count += 1;
        if self.dot_count % 60 == 0 {
            eprint!("\n   ");
        }
        eprint!("{}", char::from(c));
    }

    /// `olympia/day.c:post_month`: end-of-month settlement.
    fn post_month(&mut self) -> Phase {
        stage("post_month()");

        self.clear_orders_sent();

        let month = self.system.sysclock.month();
        if month == 2 {
            self.special_locs(true); // special_locs_open()
        }
        if month == 6 {
            self.special_locs(false); // special_locs_close()
        }

        self.move_city_gold()?;
        self.garrison_gold()?;
        self.collect_taxes();
        self.add_claim_gold();
        self.add_noble_points();
        self.add_unformed();
        self.increment_current_aura();
        self.decrement_ability_shroud();
        self.decrement_region_shroud();
        self.decrement_meditation_hinder();
        self.decrement_loc_barrier();
        self.loyalty_decay()?;
        self.pillage_decay();
        self.relic_decay()?;
        self.hide_mage_decay();
        self.inn_income();
        self.temple_income();
        self.charge_maint_costs()?;
        self.animal_deaths();
        self.ghost_warrior_decay();
        self.corpse_decay();
        self.dead_body_rot();
        self.storm_decay()?;
        self.storm_move();
        self.collapsed_mine_decay()?;
        self.post_production();
        if self.system.auto_quit_turns > 0 {
            self.auto_drop();
        }
        self.link_decay();
        self.quest_decay();
        self.check_token_units()?;
        self.determine_noble_ranks();

        self.system.post_has_been_run = 1;
        Ok(())
    }

    /// `olympia/day.c:post_production`: the month's civilization levels,
    /// market restock, production and tax base. Also run once by
    /// `load_db()` on a new world.
    pub(crate) fn post_production(&mut self) {
        self.compute_civ_levels();
        self.location_trades();
        self.location_production();
        self.seed_taxes();
        self.system.post_has_been_run = 1;
    }

    /// `olympia/day.c:compute_civ_levels`: each province's civilization
    /// level. A province scores for the first castle (by its level),
    /// tower, temple, inn, mine and city (double for a major city) inside
    /// it, and the Crown of Prosperity adds where it is. A province then
    /// rises to half of any neighbour's score, until nothing changes; the
    /// level is a quarter of the score.
    fn compute_civ_levels(&mut self) {
        let db = &mut self.db;
        let crown_loc = if db.get(RELIC_CROWN).is_some() {
            province(db, item_unique(db, RELIC_CROWN))
        } else {
            -1
        };
        stage("compute_civ_levels()");

        // clear_temps(T_loc)
        for i in db.boxes_of_kind(Kind::LOC) {
            db.entity_mut(i).temp = 0;
        }
        if db.valid_box(crown_loc) {
            db.entity_mut(crown_loc).temp += 8; // the crown gives +2 civ
        }

        let provinces: Vec<i32> = db
            .boxes_of_kind(Kind::LOC)
            .into_iter()
            .filter(|&i| loc_depth(db, i) == LOC_PROVINCE)
            .collect();

        for &where_ in &provinces {
            // Each kind of place scores once per province.
            let mut scored: Vec<Subkind> = Vec::new();
            for i in all_here(db, where_) {
                let sk = db.subkind(i);
                let points = match sk {
                    Subkind::CASTLE => 6 + castle_level(db, i),
                    Subkind::TOWER | Subkind::TEMPLE | Subkind::INN | Subkind::MINE => 4,
                    Subkind::CITY if major_city(db, i) => 8,
                    Subkind::CITY => 4,
                    _ => continue,
                };
                if !scored.contains(&sk) {
                    scored.push(sk);
                    let p = province(db, i);
                    db.entity_mut(p).temp += points;
                }
            }
        }

        loop {
            let mut flag = false;
            for &where_ in &provinces {
                if db.subkind(where_) == Subkind::OCEAN {
                    continue;
                }
                // loop_prov_dest
                let dests = db
                    .entity(where_)
                    .loc
                    .as_ref()
                    .map(|p| p.prov_dest.as_slice().to_vec())
                    .unwrap_or_default();
                for i in dests {
                    if i == 0 {
                        continue;
                    }
                    let dest_civ = db.entity(i).temp / 2;
                    if dest_civ > db.entity(where_).temp {
                        db.entity_mut(where_).temp = dest_civ;
                        flag = true;
                    }
                }
            }
            if !flag {
                break;
            }
        }

        for &where_ in &provinces {
            let civ = db.entity(where_).temp / 4;
            db.p_loc(where_).civ = civ as i8;
        }
    }

    /// `olympia/day.c:move_city_gold`: moves each city's tax base out to
    /// its province, where it can be collected.
    fn move_city_gold(&mut self) -> Phase {
        for i in self.db.boxes_of_subkind(Subkind::CITY) {
            let prov = province(&self.db, i);
            let has = has_item(&self.db, i, ITEM_TAX_COOKIE);
            self.move_item(i, prov, ITEM_TAX_COOKIE, has)?;
        }
        Ok(())
    }

    /// `olympia/report.c:determine_output_order`: numbers each region and
    /// everything in it in report order, and sorts every faction's unit
    /// list into that order (`sort_for_output()`), which is saved.
    fn determine_output_order(&mut self) {
        let db = &mut self.db;
        let mut count = 0;
        for reg in db.boxes_of_kind(Kind::LOC) {
            if loc_depth(db, reg) != LOC_REGION {
                continue;
            }
            db.entity_mut(reg).output_order = count;
            count += 1;
            for i in all_here(db, reg) {
                db.entity_mut(i).output_order = count;
                count += 1;
            }
        }

        // Sort all player unit lists.
        for pl in db.boxes_of_kind(Kind::PLAYER) {
            let Some(mut units) = db
                .entity_mut(pl)
                .player
                .as_deref_mut()
                .map(|p| std::mem::take(&mut p.units))
            else {
                continue;
            };
            // output_order_comp(): by output order, then by number.
            units.sort_by_key(|&n| (db.entity(n).output_order, n));
            db.p_player(pl).units = units;
        }
    }

    /// `olympia/report.c:player_ent_info`: each faction's admit
    /// permissions, attitudes and unclaimed items. TODO(stage 2): the
    /// report lines. Printing sorts the lists it prints, in place, and
    /// they are saved that way.
    fn player_ent_info(&mut self) {
        for pl in self.db.boxes_of_kind(Kind::PLAYER) {
            if self.db.subkind(pl) == SUB_PL_SILENT {
                continue;
            }
            // print_admit() (perm.c): admit_comp orders by target.
            self.db.p_player(pl).admits.sort_unstable_by_key(|a| a.targ);
            print_att(&mut self.db, pl);
            // show_unclaimed(): inv_item_comp orders by item.
            self.db
                .entity_mut(pl)
                .items
                .sort_unstable_by_key(|e| e.item);
        }
    }

    /// `olympia/report.c:character_report`: each character's report to
    /// its faction. TODO(stage 2): the report. What it changes is saved:
    /// `char_rep_sup()` sorts the attitude lists and the inventory it
    /// prints in place, and the first report collects the pledge backlinks
    /// (`show_pledged()`).
    fn character_report(&mut self) {
        stage("character_report()");
        for who in self.db.boxes_of_kind(Kind::CHAR) {
            if self.db.subkind(player(&self.db, who)) == SUB_PL_SILENT {
                continue;
            }
            // A prisoner's report is a line saying so; its prisoner flag
            // is lowered for it and raised again (to TRUE).
            if is_prisoner(&self.db, who) {
                self.db.p_char(who).prisoner = 1;
                continue;
            }
            // char_rep_sup()
            if !self.pledge_backlinks {
                self.collect_pledge_backlinks();
            }
            print_att(&mut self.db, who);
            // show_char_inventory(): inv_item_comp orders by item.
            self.db
                .entity_mut(who)
                .items
                .sort_unstable_by_key(|e| e.item);
        }
    }

    /// `olympia/report.c:collect_pledge_backlinks`: lists, on each
    /// character pledged to, who is pledged to it. Every character gets a
    /// magic struct.
    fn collect_pledge_backlinks(&mut self) {
        self.pledge_backlinks = true;
        let chars = self.db.boxes_of_kind(Kind::CHAR);
        for &i in &chars {
            self.db.p_magic(i).pledged_to_us.clear();
        }
        for &i in &chars {
            let n = char_pledge(&self.db, i);
            if n != 0 {
                self.db.p_magic(n).pledged_to_us.append(i);
            }
        }
    }

    /// The report phase, in `main()`'s order. TODO(stage 2): each report
    /// routine prints its `stage()` line and does nothing else.
    fn report_phase(&mut self, acct: bool) -> Phase {
        self.determine_output_order();
        stage("turn_end_loc_reports()");
        // list_order_templates() (order.c) prints no stage line, and
        // changes nothing.
        self.player_ent_info();
        self.character_report();

        stage("player_banner()");
        if acct {
            stage("charge_account()");
        }
        stage("report_account()");
        stage("summary_report()");
        stage("player_report()");

        // Not a stub: queuing lore marks it known in the faction (`kn` in
        // fact/), which is saved.
        scan_char_skill_lore(&mut self.db);
        stage("show_lore_sheets()");
        stage("gm_report()");

        // gm_show_all_skills() (lore.c) prints no stage line.

        self.add_new_players()?;
        // gen_include_section() (report.c, "must be last") and
        // close_logfile() (sout.c) print no stage line.

        stage("write_player_list()");
        // write_email(), write_totimes(), write_forwards() and
        // write_factions() (main.c) print no stage line.
        Ok(())
    }

    /// `olympia/add.c:add_new_players`: adds the players who joined since
    /// the last turn, and tells the others about them.
    fn add_new_players(&mut self) -> Phase {
        stage("add_new_players()");

        make_new_players()?;
        // TODO(stage 2): show_new_char_locs(), new_char_report(),
        // new_player_banners(), new_player_report() and
        // new_order_templates() report on the players just added, and
        // there are none until make_new_players() is ported.
        stage("new_player_list()");
        Ok(())
    }
}

/// `olympia/add.c:make_new_players`: makes a player for each
/// `act/<account>/Join-g3` file. `act` is read from the current directory,
/// not the library.
fn make_new_players() -> Phase {
    let acct_dir = "act";
    let entries = match fs::read_dir(acct_dir) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!(
                "make_new_players: can't open {acct_dir}: {}",
                strerror(&err)
            );
            return Ok(());
        }
    };
    for e in entries.flatten() {
        let name = e.file_name();
        if name.as_encoded_bytes().first() == Some(&b'.') {
            continue;
        }
        if File::open(e.path().join("Join-g3")).is_ok() {
            return not_ported("make_new_players (adding a player)");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::entity::{MONTH_DAYS, OlyTime};

    /// The C engine's clock across a turn, from `process_orders()`: the
    /// turn change readies day 0 of the next turn, and each increment then
    /// advances a day, through day `MONTH_DAYS`.
    #[test]
    fn a_turn_advances_the_clock_a_month() {
        // world.tgz's lib/system has no clock (0 0 0); C saves
        // `sysclock: 1 30 30` after a turn on it.
        let mut t = OlyTime::default();
        t.turn_change();
        assert_eq!(
            t,
            OlyTime {
                turn: 1,
                day: 0,
                days_since_epoch: 0
            }
        );
        let mut days = 0;
        while t.day < MONTH_DAYS {
            t.increment();
            days += 1;
        }
        assert_eq!(days, 30);
        assert_eq!(
            t,
            OlyTime {
                turn: 1,
                day: 30,
                days_since_epoch: 30
            }
        );
        // And the next turn.
        t.turn_change();
        while t.day < MONTH_DAYS {
            t.increment();
        }
        assert_eq!(
            t,
            OlyTime {
                turn: 2,
                day: 30,
                days_since_epoch: 60
            }
        );
    }

    #[test]
    fn increment_advances_the_day_and_the_epoch() {
        let mut t = OlyTime {
            turn: 7,
            day: 4,
            days_since_epoch: 184,
        };
        t.increment();
        assert_eq!(
            t,
            OlyTime {
                turn: 7,
                day: 5,
                days_since_epoch: 185
            }
        );
    }

    #[test]
    fn month_is_oly_month() {
        let month = |turn| {
            OlyTime {
                turn,
                ..OlyTime::default()
            }
            .month()
        };
        // C's % truncates toward zero, so turn 0 is month -1.
        assert_eq!(month(0), -1);
        assert_eq!(month(1), 0);
        assert_eq!(month(3), 2);
        assert_eq!(month(8), 7);
        assert_eq!(month(9), 0);
    }
}
