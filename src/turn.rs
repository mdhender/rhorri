// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Running a turn (`-r`): `main()`'s run block (`olympia/main.c`),
//! `process_orders()` (`olympia/input.c`), `post_month()`
//! (`olympia/day.c`) and the report phase.
//!
//! The skeleton calls every phase in C order. A phase that isn't ported yet
//! stops the run with [`NotYetImplemented`] naming the routine, rather than
//! being skipped; the comment beside each names the stage 1 task (#5) that
//! ports it. The report phase is the exception: until stage 2 its routines
//! print their `stage()` line and do nothing else.
//!
//! `phase_ran()` and `phase_acted()` (the `--phase-coverage` counters) are
//! left out: `--phase-coverage` isn't ported yet, and stops the run before
//! anything is loaded.

use std::fs::{self, File};

use crate::command::find_command;
use crate::entity::{Kind, MONTH_DAYS};
use crate::game::{Game, NotYetImplemented, player, stage};
use crate::loc::{LOC_PROVINCE, loc_depth, subloc, viewloc};
use crate::lore::scan_char_skill_lore;
use crate::os::strerror;

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
        self.initial_command_load();
        self.queue_npc_orders()?;
        not_ported("ping_garrisons")?; // task 6 (garr.c)
        not_ported("check_token_units")?; // task 6 (npc.c), "temp fix" in C

        self.process_interrupted_units()?; // day 0
        self.process_player_orders()?;
        not_ported("scan_char_item_lore")?; // task 6 (lore.c)

        stage("");

        while self.system.sysclock.day < MONTH_DAYS {
            self.system.sysclock.increment();

            if self.system.sysclock.day == 1 {
                not_ported("match_all_trades")?; // task 9 (buy.c)
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

    /// `olympia/npc.c:queue_npc_orders`: queues the NPCs' orders for the
    /// turn. Task 6: `init_savage_attacks`, `auto_hades`, then the
    /// independent-unit programs.
    fn queue_npc_orders(&mut self) -> Phase {
        stage("queue_npc_orders()");
        not_ported("queue_npc_orders")
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

    /// `olympia/day.c:daily_events`: the day's scheduled events. Task 7
    /// lays it out (the `caln` day picks, the weather days and the daily
    /// routines). The weather pieces are ported (task 8): call
    /// [`schedule_weather_days`](Self::schedule_weather_days) right after
    /// the `caln` day picks, and [`weather_day`](Self::weather_day) after
    /// `dogs_bark_at_hidden_chars()` and before `auto_faery()`.
    fn daily_events(&mut self) -> Phase {
        not_ported("daily_events")
    }

    /// `olympia/day.c:post_month`: end-of-month settlement.
    fn post_month(&mut self) -> Phase {
        stage("post_month()");

        not_ported("clear_orders_sent")?; // task 10

        let month = self.system.sysclock.month();
        if month == 2 {
            not_ported("special_locs_open")?; // task 10
        }
        if month == 6 {
            not_ported("special_locs_close")?; // task 10
        }

        not_ported("move_city_gold")?; // task 9
        not_ported("garrison_gold")?; // task 10
        not_ported("collect_taxes")?; // task 10
        not_ported("add_claim_gold")?; // task 10
        not_ported("add_noble_points")?; // task 10
        not_ported("add_unformed")?; // task 10
        not_ported("increment_current_aura")?; // task 10
        not_ported("decrement_ability_shroud")?; // task 10
        not_ported("decrement_region_shroud")?; // task 10
        not_ported("decrement_meditation_hinder")?; // task 10
        not_ported("decrement_loc_barrier")?; // task 10
        not_ported("loyalty_decay")?; // task 10
        not_ported("pillage_decay")?; // task 10
        not_ported("relic_decay")?; // task 10
        not_ported("hide_mage_decay")?; // task 10
        not_ported("inn_income")?; // task 10
        not_ported("temple_income")?; // task 10
        not_ported("charge_maint_costs")?; // task 10
        self.animal_deaths()?;
        not_ported("ghost_warrior_decay")?; // task 10
        not_ported("corpse_decay")?; // task 10
        not_ported("dead_body_rot")?; // task 10
        self.storm_decay()?;
        self.storm_move();
        not_ported("collapsed_mine_decay")?; // task 10
        self.post_production()?;
        if self.system.auto_quit_turns > 0 {
            not_ported("auto_drop")?; // task 10
        }
        not_ported("link_decay")?; // task 10
        not_ported("quest_decay")?; // task 10
        not_ported("check_token_units")?; // task 6 (npc.c)
        self.determine_noble_ranks()?;

        self.system.post_has_been_run = 1;
        Ok(())
    }

    /// `olympia/day.c:animal_deaths`. Task 10.
    fn animal_deaths(&mut self) -> Phase {
        stage("animal_deaths()");
        not_ported("animal_deaths")
    }

    /// `olympia/day.c:post_production`: the month's production and market
    /// restock. Task 9. Also run once by `load_db()` on a new world.
    fn post_production(&mut self) -> Phase {
        self.compute_civ_levels()?;
        self.location_trades()?;
        not_ported("location_production")?; // task 9 (produce.c)
        not_ported("seed_taxes")?; // task 9 (produce.c)
        self.system.post_has_been_run = 1;
        Ok(())
    }

    /// `olympia/day.c:compute_civ_levels`. Task 9.
    fn compute_civ_levels(&mut self) -> Phase {
        stage("compute_civ_levels()");
        not_ported("compute_civ_levels")
    }

    /// `olympia/buy.c:location_trades`. Task 9.
    fn location_trades(&mut self) -> Phase {
        stage("location_trades()");
        not_ported("location_trades")
    }

    /// `olympia/garr.c:determine_noble_ranks`. Task 10.
    fn determine_noble_ranks(&mut self) -> Phase {
        stage("determine_noble_ranks()");
        not_ported("determine_noble_ranks")
    }

    /// The report phase, in `main()`'s order. TODO(stage 2): each report
    /// routine prints its `stage()` line and does nothing else.
    fn report_phase(&mut self, acct: bool) -> Phase {
        // determine_output_order() (report.c) orders the regions for the
        // reports; nothing saved.
        stage("turn_end_loc_reports()");
        // list_order_templates() (order.c) and player_ent_info()
        // (report.c) print no stage line.
        stage("character_report()");

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
