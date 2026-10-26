// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `move` command: walking and riding from one location to the next
//! (`olympia/move.c`).
//!
//! `v_move` picks the exit (a direction or a destination), works out how
//! many days the stack takes to get there, and starts it moving: the stack
//! leaves whoever it was stacked under and is gone until it arrives. The
//! exit is kept in the command's arguments `b` through `h`
//! (`save_v_array`) so `d_move` can finish the trip when the days are up.
//! Arriving (`move_stack`) moves the stack, marks the place known to every
//! faction in it, and may trigger an ambush in Hades or Faery.
//!
//! Sailing and flying come with the stage 4 movement family. Messages to
//! the players (`wout`, `departure_message`, `arrival_message`) come with
//! the reports in stage 2.

use crate::check::{all_stack, stack_leader, stack_parent};
use crate::command::{cmd_shift, numargs, oly_parse};
use crate::dir::{DIR_IN, DIR_OUT, ExitView, FULL_DIR_S};
use crate::entity::{Db, Kind, Subkind};
use crate::game::{Game, NotYetImplemented, player};
use crate::input::here_pos;
use crate::loc::{
    LOC_BUILD, LOC_PROVINCE, LOC_REGION, all_char_here, building_owner, loc_depth, region, subloc,
};
use crate::lore::set_known;
use crate::order::prepend_order;
use crate::swear::{LOY_NPC, LOY_SUMMON, set_loyal};
use crate::turn::{Phase, not_ported};
use crate::unit::{
    count_stack_figures, count_stack_move_nobles, determine_stack_weights, is_prisoner, loyal_kind,
    restore_stack_actions,
};
use crate::wait::lookup;

/// `olympia/glob.c:short_dir_s`: the directions' one-letter names.
const SHORT_DIR_S: [&str; 9] = ["<no dir>", "n", "e", "s", "w", "u", "d", "i", "o"];

/// `olympia/perm.c:will_admit`: whether `pl`'s faction (`pl` may be any
/// of its units) lets `who` into `targ`.
fn will_admit(db: &Db, pl: i32, who: i32, targ: i32) -> bool {
    // default_garrison(targ)
    let default_garr = db
        .get(targ)
        .and_then(|b| b.char_.as_ref())
        .and_then(|c| c.magic.as_ref())
        .is_some_and(|m| m.default_garr != 0);
    if default_garr {
        return true;
    }

    let pl = player(db, pl);
    if player(db, who) == pl {
        return true;
    }

    // rp_admit()
    assert_eq!(db.kind(pl), Kind::PLAYER, "rp_admit: {pl} is no player");
    let Some(p) = db
        .get(pl)
        .and_then(|b| b.player.as_ref())
        .and_then(|p| p.admits.iter().find(|a| a.targ == targ))
    else {
        return false;
    };

    let found = p.l.contains(who) || p.l.contains(player(db, who));
    if p.sense != 0 { !found } else { found }
}

/// `olympia/move.c:save_v_array`: keeps the exit `v` in the command's
/// arguments `b` through `h` for `d_move`.
fn save_v_array(args: &mut [i32; 8], v: &ExitView) {
    args[1] = v.direction;
    args[2] = v.destination;
    args[3] = v.road;
    args[4] = i32::from(v.dest_hidden);
    args[5] = v.distance;
    args[6] = v.orig;
    args[7] = i32::from(v.orig_hidden);
}

/// `olympia/move.c:restore_v_array`: the exit `save_v_array` kept.
fn restore_v_array(args: &[i32; 8]) -> ExitView {
    ExitView {
        direction: args[1],
        destination: args[2],
        road: args[3],
        dest_hidden: args[4] != 0,
        distance: args[5],
        orig: args[6],
        orig_hidden: args[7] != 0,
        ..ExitView::default()
    }
}

/// `olympia/loc.c:mark_loc_stack_known`: every faction in `stack` but the
/// prisoners' has visited `where_`.
fn mark_loc_stack_known(db: &mut Db, stack: i32, where_: i32) {
    if db.kind(stack) == Kind::CHAR {
        set_known(db, stack, where_);
    }
    for i in all_char_here(db, stack) {
        if !is_prisoner(db, i) {
            set_known(db, i, where_);
        }
    }
}

/// `olympia/stealth.c:clear_contacts`: everyone in `stack` forgets whom
/// they contacted.
fn clear_contacts(db: &mut Db, stack: i32) {
    if db.kind(stack) == Kind::CHAR {
        for i in all_stack(db, stack) {
            db.p_char(i).contact.clear();
        }
    }
}

/// `olympia/move.c:suspend_stack_actions`: `who`'s stack is moving, as of
/// today.
fn suspend_stack_actions(g: &mut Game, who: i32) {
    let today = g.system.sysclock.days_since_epoch;
    for i in all_stack(&g.db, who) {
        g.db.p_char(i).moving = today;
    }
}

/// `olympia/move.c:clear_guard_flag`: no one in `who`'s stack is guarding
/// any more.
pub fn clear_guard_flag(db: &mut Db, who: i32) {
    if db.kind(who) == Kind::CHAR {
        db.p_char(who).guard = 0;
    }
    for i in all_char_here(db, who) {
        db.p_char(i).guard = 0;
    }
}

/// `olympia/stack.c:promote`: moves `who` up to place `new_pos` in the
/// here list of where it is.
///
/// # Panics
///
/// Panics if `who` is already above `new_pos`, as C asserts.
fn promote(db: &mut Db, who: i32, new_pos: i32) {
    let who_pos = here_pos(db, who);
    assert!(
        who_pos >= new_pos,
        "promote: {who} at {who_pos} < {new_pos}"
    );
    let l = &mut db.entity_mut(db.loc(who)).loc_info.here_list;
    let mut v = l.as_slice().to_vec();
    let x = v.remove(who_pos as usize);
    v.insert(new_pos as usize, x);
    l.clear();
    for n in v {
        l.append(n);
    }
}

/// `olympia/stack.c:unstack`: `who` leaves its stack, and stands next to
/// its old stack's leader.
///
/// # Panics
///
/// Panics if the leader is inside something other than its location, as
/// C asserts.
fn unstack(db: &mut Db, who: i32) {
    let leader = stack_leader(db, who);
    assert!(db.valid_box(leader), "unstack: invalid leader {leader}");
    assert_eq!(
        subloc(db, leader),
        db.loc(leader),
        "unstack: leader stacked"
    );

    // release_swear()
    let release = db
        .get(who)
        .and_then(|b| b.char_.as_ref())
        .and_then(|c| c.magic.as_ref())
        .is_some_and(|m| m.swear_on_release != 0);
    if release {
        db.p_magic(who).swear_on_release = 0;
    }

    db.set_where(who, subloc(db, leader));
    let pos = here_pos(db, leader) + 1;
    promote(db, who, pos);

    // Unstacking while moving: who has its own actions again.
    restore_stack_actions(db, who);

    if loyal_kind(db, who) == LOY_SUMMON {
        set_loyal(db, who, LOY_NPC, 0);
    }
}

/// `olympia/stack.c:leave_stack`: `who` leaves the character it is
/// stacked under, if any.
pub fn leave_stack(db: &mut Db, who: i32) {
    let leader = stack_parent(db, who);
    if leader <= 0 {
        return;
    }
    // TODO(stage 2): wout(leader, "%s unstacks from us."), wout(who, ...),
    // and the same to those here (vector_char_here).
    unstack(db, who);
}

impl Game {
    /// `in_faery()`.
    fn in_faery(&self, n: i32) -> bool {
        region(&self.db, n) == self.system.faery_region
    }

    /// `olympia/move.c:init_ocean_chars`: the characters over the ocean
    /// when the database is loaded.
    pub fn init_ocean_chars(&mut self) {
        for i in self.db.boxes_of_kind(Kind::CHAR) {
            if self.db.subkind(subloc(&self.db, i)) == Subkind::OCEAN {
                self.ocean_chars.append(i);
            }
        }
    }

    /// `olympia/move.c:parse_exit_dir`: the exit from `where_` that the
    /// argument names: the destination `a` if it is an entity (preferring
    /// a passable route), else the direction `word`. `zero_arg` is the
    /// command's name when the player is to be told why there is none.
    ///
    /// `see_all()` (the debugging override of hidden routes, set only in
    /// immediate mode) is never on here.
    fn parse_exit_dir(
        &self,
        who: i32,
        where_: i32,
        a: i32,
        word: &[u8],
        zero_arg: Option<&str>,
    ) -> Option<ExitView> {
        // TODO(stage 2): the zero_arg messages to who.
        let _ = zero_arg;
        let l = self.exits_from_loc(who, where_);

        if self.db.valid_box(a) {
            if where_ == a {
                return None;
            }
            // A secret passable route may parallel a visible impassable
            // one, so passable routes come first. The last of each wins.
            let visible = l.iter().filter(|e| e.destination == a && !e.hidden);
            let (pass, impass): (Vec<_>, Vec<_>) = visible.partition(|e| !e.impassable);
            return pass.last().or(impass.last()).map(|&e| e.clone());
        }

        let dir = lookup(&FULL_DIR_S, word).or_else(|| lookup(&SHORT_DIR_S, word))?;
        // TODO(stage 2): "(assuming '%s %s')" for DIR_IN.
        l.into_iter().find(|e| e.direction == dir && !e.hidden)
    }

    /// `olympia/move.c:land_check`: whether `who` may walk the exit `v`:
    /// not across water, not an impassable route, not onto a ship under
    /// way, and not into a building whose owner won't admit it.
    fn land_check(&self, who: i32, v: &ExitView) -> bool {
        // TODO(stage 2): the messages when show is set.
        if v.water || v.impassable || v.in_transit {
            return false;
        }
        let db = &self.db;
        if loc_depth(db, v.destination) == LOC_BUILD && db.subkind(v.destination) != Subkind::SEWER
        {
            let owner = building_owner(db, v.destination);
            if owner != 0 && !will_admit(db, owner, who, v.destination) && v.direction != DIR_OUT {
                return false;
            }
        }
        true
    }

    /// `olympia/move.c:move_exit_land`: how many days `who`'s stack takes
    /// to walk or ride the exit `v`, or -1 if it is too overloaded to
    /// travel. Riding halves a trip of more than a day, except through a
    /// swamp; otherwise animals are slowed by swamps and excess inventory
    /// slows the stack. An army slows a few nobles down.
    ///
    /// # Panics
    ///
    /// Panics on a stack with no nobles, as C asserts.
    fn move_exit_land(&self, who: i32, v: &ExitView) -> i32 {
        // TODO(stage 2): the delay messages to who (some only with show).
        let db = &self.db;
        let mut delay = v.distance;
        if delay == 0 {
            return 0;
        }

        let terr = db.subkind(v.destination);
        let swamp = terr == Subkind::SWAMP || terr == Subkind::BOG || terr == Subkind::PITS;

        let w = determine_stack_weights(db, who);

        if delay > 1 && w.ride_cap >= w.ride_weight && !swamp {
            delay -= delay / 2;
        } else {
            if w.land_weight > w.land_cap * 2 {
                return -1;
            }
            if swamp && w.animals != 0 {
                delay += 1;
            }
            if w.land_weight > w.land_cap {
                let ratio = (w.land_weight - w.land_cap) * 100 / w.land_cap;
                let additional = delay * ratio / 100;
                delay += additional;
            }
        }

        let nobles = count_stack_move_nobles(db, who);
        let men = count_stack_figures(db, who) - nobles;
        assert!(nobles > 0, "move_exit_land: no nobles");
        let extra = (men / (nobles * self.system.army_slow_factor)).min(v.distance * 2);
        delay + extra
    }

    /// `olympia/move.c:can_move_here`: whether `who` could walk from
    /// `where_` by the direction `word`.
    fn can_move_here(&self, who: i32, where_: i32, word: &[u8]) -> bool {
        // c->a is 0: every argument has been shifted off.
        self.parse_exit_dir(who, where_, 0, word, None)
            .is_some_and(|v| {
                v.direction != DIR_IN
                    && self.land_check(who, &v)
                    && self.move_exit_land(who, &v) >= 0
            })
    }

    /// `olympia/move.c:can_move_at_outer_level`: how many levels out from
    /// `where_` `who` would have to go to walk by the direction `word`, or
    /// 0 if it can't from any of them.
    fn can_move_at_outer_level(&self, who: i32, where_: i32, word: &[u8]) -> i32 {
        let mut outer = subloc(&self.db, where_);
        while loc_depth(&self.db, outer) > LOC_REGION {
            if self.can_move_here(who, outer, word) {
                return loc_depth(&self.db, outer) - loc_depth(&self.db, where_);
            }
            outer = subloc(&self.db, outer);
        }
        0
    }

    /// Runs `f` on `who`'s command, lifted out of the database so `f` can
    /// read the database too.
    fn with_command<T>(
        &mut self,
        who: i32,
        f: impl FnOnce(&Db, &mut crate::entity::Command) -> T,
    ) -> T {
        let mut c = self.db.entity_mut(who).cmd.take().expect("command");
        let r = f(&self.db, &mut c);
        self.db.entity_mut(who).cmd = Some(c);
        r
    }

    /// `olympia/move.c:v_move`: starts `who`'s stack moving by the first
    /// of the command's directions or destinations it can walk. If none
    /// names an exit here but one is an exit from a location `who` is in,
    /// it moves out first, and the move is queued again.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] for a prisoner in a stack setting out
    /// on a trip of more than a day (`check_prisoner_escape`).
    ///
    /// # Panics
    ///
    /// Panics if the way out found by `can_move_at_outer_level` isn't
    /// there, as C asserts.
    pub(crate) fn v_move(&mut self, who: i32) -> Result<i8, NotYetImplemented> {
        let where_ = subloc(&self.db, who);
        let mut check_outer = true;

        let c = self.command(who);
        if numargs(c) < 1 {
            // TODO(stage 2): wout(who, "Specify direction or destination to MOVE.")
            return Ok(0);
        }
        // After every argument has been shifted off, C's c->parse[1] still
        // points at the last of them (cstrings_delete leaves the slot;
        // playbymail/olympia-g3#253).
        let last_word = c.parse.last().cloned().unwrap_or_default();

        let mut v = None;
        while numargs(self.command(who)) > 0 {
            let c = self.command(who);
            let (a, word) = (c.args[0], c.parse[1].clone());
            v = self.parse_exit_dir(who, where_, a, &word, Some("move"));
            if let Some(e) = &v {
                check_outer = false;
                if self.land_check(who, e) {
                    break;
                }
                v = None;
            }
            self.with_command(who, cmd_shift);
        }

        if v.is_none() && check_outer && self.can_move_at_outer_level(who, where_, &last_word) != 0
        {
            let a = subloc(&self.db, where_);
            self.command(who).args[0] = a;
            v = self.parse_exit_dir(who, where_, a, &last_word, None);
            let e = v.as_ref().expect("v_move: no way out");
            assert!(self.move_exit_land(who, e) >= 0, "v_move: can't move out");
            // TODO(stage 2): wout(who, "(assuming 'move out' first)")
            let line = self.command(who).line.clone().unwrap_or_default();
            let pl = player(&self.db, who);
            prepend_order(&mut self.db, pl, who, &line);
        }

        let Some(mut v) = v else {
            return Ok(0);
        };

        let delay = self.move_exit_land(who, &v);
        if delay < 0 {
            return Ok(0);
        }

        if v.hades_cost != 0 {
            let cost = v.hades_cost * count_stack_figures(&self.db, who);
            // TODO(stage 2): log_write(LOG_SPECIAL, "%s (%s) tries to enter Hades")
            if !self.autocharge(who, cost) {
                // TODO(stage 2): wout(who, "Can't afford %s to enter Hades.")
                return Ok(0);
            }
            // TODO(stage 2): wout(who, "The Gatekeeper Spirit of Hades took %s from us.")
        }

        v.distance = delay;
        let c = self.command(who);
        c.wait = delay;
        save_v_array(&mut c.args, &v);
        leave_stack(&mut self.db, who);

        // TODO(stage 2): if delay > 0, wout(VECT, "Travel to %s will take
        // %s day%s.") to the stack (vector_stack).

        suspend_stack_actions(self, who);
        clear_guard_flag(&mut self.db, who);

        if delay > 1 {
            self.prisoner_movement_escape_check(who)?;
        }

        // TODO(stage 2): departure_message(who, v)
        Ok(1)
    }

    /// Re-parses `who`'s command as `line` and runs it as a `move`: the
    /// compass verbs, `enter` and `exit` (`olympia/move.c:v_north`, ...).
    ///
    /// # Errors
    ///
    /// As [`v_move`](Self::v_move).
    ///
    /// # Panics
    ///
    /// Panics if `line` doesn't parse, as C asserts.
    pub(crate) fn v_move_as(&mut self, who: i32, line: &[u8]) -> Result<i8, NotYetImplemented> {
        let ok = self.with_command(who, |db, c| oly_parse(db, c, line));
        assert!(ok, "v_move_as: can't parse the move");
        self.v_move(who)
    }

    /// `olympia/move.c:v_enter`: `move in`, or `move` to the argument.
    ///
    /// # Errors
    ///
    /// As [`v_move`](Self::v_move).
    pub(crate) fn v_enter(&mut self, who: i32) -> Result<i8, NotYetImplemented> {
        let c = self.command(who);
        let line = if numargs(c) < 1 {
            b"move in".to_vec()
        } else {
            [&b"move "[..], &c.parse[1]].concat()
        };
        self.v_move_as(who, &line)
    }

    /// `olympia/stack.c:prisoner_movement_escape_check`: each prisoner in
    /// `who`'s stack may escape as it sets out.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if there is a prisoner: the escape
    /// roll (`check_prisoner_escape`) comes with the daily events.
    fn prisoner_movement_escape_check(&mut self, who: i32) -> Phase {
        if all_char_here(&self.db, who)
            .into_iter()
            .any(|i| is_prisoner(&self.db, i))
        {
            return not_ported("check_prisoner_escape");
        }
        Ok(())
    }

    /// `olympia/move.c:discover_road`: `who`'s stack learns both ends of
    /// the hidden road `v` it is taking from `where_`.
    fn discover_road(&mut self, who: i32, where_: i32, v: &ExitView) {
        let back: Vec<i32> = self
            .exits_from_loc(who, v.destination)
            .into_iter()
            .filter(|e| e.road != 0 && e.destination == where_)
            .map(|e| e.road)
            .collect();
        for road in back {
            set_known(&mut self.db, who, road);
            set_known(&mut self.db, who, v.road);
            for j in all_char_here(&self.db, who) {
                set_known(&mut self.db, j, road);
                set_known(&mut self.db, j, v.road);
            }
        }
    }

    /// `olympia/move.c:d_move`: `who`'s stack arrives.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if arriving reaches something not
    /// ported.
    pub(crate) fn d_move(&mut self, who: i32) -> Result<i8, NotYetImplemented> {
        let v = restore_v_array(&self.command(who).args);

        if !self.db.valid_box(v.destination) {
            // TODO(stage 2): wout(who, "Your destination no longer exists!")
            return Ok(0);
        }

        if v.road != 0 {
            let where_ = subloc(&self.db, who);
            self.discover_road(who, where_, &v);
        }

        // TODO(stage 2): wout(VECT, "Arrival at %s.") to the stack, and
        // "The province is blanketed in fog." when it is.

        restore_stack_actions(&mut self.db, who);
        self.move_stack(who, v.destination)?;

        // TODO(stage 2): arrival_message(who, v) when the viewlocs differ.
        Ok(1)
    }

    /// `olympia/move.c:touch_loc_after_move`: every faction in `who`'s
    /// stack but the prisoners' sees where it is now.
    fn touch_loc_after_move(&mut self, who: i32) {
        if self.db.kind(who) == Kind::CHAR {
            self.touch_loc(who);
        }
        for i in all_char_here(&self.db, who) {
            if !is_prisoner(&self.db, i) {
                self.touch_loc(i);
            }
        }
    }

    /// `olympia/move.c:move_stack`: puts `who`'s stack in `where_`. Its
    /// factions learn and see the place, its weather mages see the
    /// weather, its contacts are forgotten, it trades in a city, it is
    /// over the ocean or not, and it may be ambushed in Hades or Faery.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if arriving reaches something not
    /// ported.
    ///
    /// # Panics
    ///
    /// Panics if `who` isn't a character, as C asserts.
    pub fn move_stack(&mut self, who: i32, where_: i32) -> Phase {
        assert_eq!(self.db.kind(who), Kind::CHAR, "move_stack: {who}");

        // TODO(stage 2): log_write(LOG_SPECIAL, "%s enters Faery at %s.")
        // when entering Faery.

        self.db.set_where(who, where_);
        mark_loc_stack_known(&mut self.db, who, where_);
        self.touch_loc_after_move(who);
        self.update_weather_view_locs(who, where_);
        clear_contacts(&mut self.db, who);

        let sk = self.db.subkind(where_);
        if sk == Subkind::CITY {
            for i in all_stack(&self.db, who) {
                self.match_trades(i)?;
            }
        }

        if sk == Subkind::OCEAN {
            let p = self.db.p_char(who);
            if p.time_flying == 0 {
                p.time_flying += 1;
                self.ocean_chars.append(who);
            }
        } else {
            let flying = self
                .db
                .get(who)
                .and_then(|b| b.char_.as_ref())
                .is_some_and(|c| c.time_flying != 0);
            if flying {
                self.db.p_char(who).time_flying = 0;
                self.ocean_chars.rem_value(who);
            }
        }

        let land_province = loc_depth(&self.db, where_) == LOC_PROVINCE && sk != Subkind::OCEAN;
        if land_province && self.in_faery(where_) {
            self.faery_attack_check(who, where_)?;
        }
        if land_province && self.in_hades(where_) {
            self.hades_attack_check(who, where_)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v_array_round_trips_the_exit() {
        let v = ExitView {
            direction: 3,
            destination: 10_203,
            road: 7_001,
            dest_hidden: true,
            distance: 4,
            orig: 10_103,
            orig_hidden: false,
            impassable: true,
            ..ExitView::default()
        };
        let mut args = [9; 8];
        save_v_array(&mut args, &v);
        assert_eq!(args, [9, 3, 10_203, 7_001, 1, 4, 10_103, 0]);
        let back = restore_v_array(&args);
        assert_eq!(
            back,
            ExitView {
                impassable: false,
                ..v
            }
        );
    }

    #[test]
    fn short_and_full_direction_names_line_up() {
        assert_eq!(lookup(&SHORT_DIR_S, b"S"), Some(3));
        assert_eq!(lookup(&FULL_DIR_S, b"Out"), Some(DIR_OUT));
        assert_eq!(lookup(&FULL_DIR_S, b"sideways"), None);
    }
}
