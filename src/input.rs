// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The command driver (`olympia/input.c`).
//!
//! Every character and player has a command structure. Its state moves
//! `STATE_LOAD → STATE_RUN → STATE_DONE`: `load_command` pops the
//! unit's next order and parses it into the structure, queuing it on the
//! load queue of its priority; `do_command` runs the start routine and moves
//! it to the run queue; `finish_command` counts down its days, calls the
//! finish routine and, when it is done, loads the next order. A unit with no
//! orders left is `STATE_DONE`, and one whose order doesn't parse is
//! `STATE_ERROR`, which no queue holds (playbymail/olympia-g3#248).
//!
//! Each day `start_phase` drains the load queues, lowest priority number
//! first, and `evening_phase` gives each running command its daily finish
//! check. The queues, the wait and collect lists and the rest of the driver
//! state live in [`Driver`].
//!
//! The command table names each command's handlers (see
//! [`CmdTblEnt`](crate::command::CmdTblEnt)). A handler that isn't ported
//! yet stops the run with "not yet implemented", naming the command.
//!
//! TODO(stage 2): the driver's `out()` and `wout()` messages to the units
//! (`> line`, "Unrecognized command.", "Wait finished: ...") are report
//! output and are dropped; each is marked where C writes it.

use crate::check::{GM_PLAYER, stack_leader, stack_parent};
use crate::code::box_name;
use crate::command::{CMD_TBL, find_command, oly_parse};
use crate::entity::{Command, Kind, Text};
use crate::game::{Game, NotYetImplemented, player};
use crate::order::{pop_order, stop_order, top_order};
use crate::turn::Phase;
use crate::unit::{char_gone, char_moving, is_prisoner, restore_stack_actions, restricted_control};
use crate::wait::FlagEnt;

/// `STATE_DONE`: no orders remain in the unit's queue.
pub const STATE_DONE: i8 = 0;
/// `STATE_LOAD`: a command is loaded and ready to run.
pub const STATE_LOAD: i8 = 1;
/// `STATE_RUN`: the command is running.
pub const STATE_RUN: i8 = 2;
/// `STATE_ERROR`: the order didn't parse.
pub const STATE_ERROR: i8 = 3;

/// `MAX_PRI`: the number of command priorities.
const MAX_PRI: usize = 5;

/// `min_pri_ready()`'s answer when no command is ready.
const NONE_READY: i32 = 99;

/// `LEN`: C's generic string buffer length; an order must be shorter.
const LEN: usize = 2048;

/// The driver's state, kept for the whole run as C's file-scope statics
/// are: `check_db()` can load commands (through `set_lord`) before the turn
/// starts.
#[derive(Debug, Default, Clone)]
pub struct Driver {
    /// Loaded commands, by priority (`load_q`).
    load_q: [Vec<i32>; MAX_PRI],
    /// Running commands (`run_q`).
    run_q: Vec<i32>,
    /// The priority `start_phase` is draining (`cur_pri`).
    cur_pri: i32,
    /// Set during `evening_phase` (`evening`).
    pub evening: bool,
    /// Whether today's auto-attacks are still to be checked
    /// (`auto_attack_flag`).
    auto_attack_flag: bool,
    /// Units running `wait` (`olympia/c1.c:wait_list`).
    pub wait_list: Vec<i32>,
    /// Units running `collect` (`olympia/produce.c:collectors`).
    pub collectors: Vec<i32>,
    /// Units held back a day by an auto-attack
    /// (`olympia/combat.c:second_wait_list`).
    pub second_wait_list: Vec<i32>,
    /// Units whose trades want matching (`olympia/buy.c:trades_to_check`).
    /// `buy`, `sell` and `add_item` add to it.
    pub trades_to_check: Vec<i32>,
    /// The signals raised by `flag` this turn (`olympia/c1.c:flags`).
    pub flags: Vec<FlagEnt>,
}

/// `ilist_rem_value_uniq()`: removes the last `n`.
fn rem_value_uniq(l: &mut Vec<i32>, n: i32) {
    if let Some(i) = l.iter().rposition(|&x| x == n) {
        l.remove(i);
    }
}

/// `ilist_rem_value()`: removes every `n`.
pub fn rem_value(l: &mut Vec<i32>, n: i32) {
    l.retain(|&x| x != n);
}

/// The table entry for command `cmd`.
fn tbl(cmd: i32) -> &'static crate::command::CmdTblEnt {
    &CMD_TBL[usize::try_from(cmd).expect("command index")]
}

/// The error for a handler that isn't ported yet.
fn handler_not_ported(cmd: i32, handler: &str) -> NotYetImplemented {
    NotYetImplemented(format!("{} command ({handler})", tbl(cmd).name))
}

impl Game {
    /// `rp_command(who)`.
    #[must_use]
    pub fn rp_command(&self, who: i32) -> Option<&Command> {
        self.db.get(who).and_then(|b| b.cmd.as_deref())
    }

    /// `rp_command(who)` for changing; the unit must have a command.
    ///
    /// # Panics
    ///
    /// Panics if `who` has no command structure, where C would follow a
    /// null pointer.
    pub fn command(&mut self, who: i32) -> &mut Command {
        self.db
            .entity_mut(who)
            .cmd
            .as_deref_mut()
            .unwrap_or_else(|| panic!("{who} has no command"))
    }

    /// `set_state()`: moves the command to the queue for `state`, taking it
    /// off the one for its old state. A loaded command goes on the load
    /// queue for `new_pri`.
    fn set_state(&mut self, who: i32, state: i8, new_pri: i32) {
        let c = self.command(who);
        let (old_state, old_pri) = (c.state, c.pri);
        c.state = state;
        let d = &mut self.driver;
        match old_state {
            STATE_RUN => rem_value_uniq(&mut d.run_q, who),
            STATE_LOAD => rem_value_uniq(&mut d.load_q[old_pri as usize], who),
            _ => {}
        }
        match state {
            STATE_RUN => d.run_q.push(who),
            STATE_LOAD => d.load_q[new_pri as usize].push(who),
            _ => {}
        }
    }

    /// `get_command()`: pops `who`'s next order, if it has a faction and an
    /// order queued, and notes that the faction played this turn.
    fn get_command(&mut self, who: i32) -> Option<Text> {
        if !self.db.valid_box(who) || self.db.kind(who) == Kind::DEADCHAR {
            return None;
        }
        let fact = player(&self.db, who);
        if fact == 0 {
            return None;
        }
        let order = top_order(&self.db, fact, who)?.to_vec();

        self.db.p_player(fact).last_order_turn = i32::from(self.system.sysclock.turn);

        assert!(order.len() < LEN, "get_command: order too long");
        pop_order(&mut self.db, fact, who);
        Some(order)
    }

    /// `load_command()`: loads `who`'s next order into its command and
    /// sets its state: `STATE_LOAD`, `STATE_ERROR` if the order doesn't
    /// parse, or `STATE_DONE` if there is none. Returns false for none.
    fn load_command(&mut self, who: i32) -> bool {
        let Some(buf) = self.get_command(who) else {
            self.set_state(who, STATE_DONE, 0);
            return false;
        };

        // oly_parse() reads the database (for `garrison`), never this
        // command, so the command is lifted out while it is filled in.
        let mut c = self.db.entity_mut(who).cmd.take().expect("command");
        let parsed = oly_parse(&self.db, &mut c, &buf);
        self.db.entity_mut(who).cmd = Some(c);

        if parsed {
            let ent = tbl(self.command(who).cmd);
            self.set_state(who, STATE_LOAD, ent.pri);
            let c = self.command(who);
            c.pri = ent.pri as i8;
            c.wait = ent.time;
            c.poll = ent.poll as i8;
            c.days_executing = 0;
        } else {
            self.set_state(who, STATE_ERROR, 0);
        }
        true
    }

    /// `command_done()`: the command is over; loads the next one. If that
    /// one has a lower priority number, `start_phase` goes back for it.
    fn command_done(&mut self, who: i32) {
        // `immediate` mode isn't ported.
        if self.load_command(who) {
            let pri = i32::from(self.command(who).pri);
            if pri < self.driver.cur_pri {
                self.driver.cur_pri = pri;
            }
        }
    }

    /// `finish_command()`: a running command's daily check. Counts down its
    /// wait, calls its finish routine when the wait is over (or every day
    /// if it polls), and loads the next command when it is done or failed.
    /// Returns the command's status.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] for a finish routine not ported yet.
    pub fn finish_command(&mut self, who: i32) -> Result<i8, NotYetImplemented> {
        assert!(self.db.valid_box(who), "finish_command: invalid box {who}");

        if self.db.kind(who) == Kind::DEADCHAR {
            self.command_done(who);
            return Ok(0);
        }

        // A character stacked under a moving unit is suspended until it
        // arrives, except for wait completion checks.
        let cmd = self.command(who).cmd;
        if char_gone(&self.db, who) && stack_leader(&self.db, who) != who && cmd != self.cmd_wait {
            return Ok(1);
        }

        let day = self.system.sysclock.day;
        let c = self.command(who);
        if c.wait > 0 {
            c.wait -= 1;
        }
        // TODO(stage 2): out(who, "finish_command called twice, wait=%d")
        // when c.wait > 0 and c.debug == day.
        c.debug = day as i8;

        if c.wait <= 0 || c.poll != 0 {
            if let Some(finish) = tbl(c.cmd).finish {
                if c.inhibit_finish == 0 {
                    let status = self.call_finish(who, finish)?;
                    self.command(who).status = status;
                }
            }
        }

        let c = self.command(who);
        if c.state == STATE_RUN && (c.status == 0 || c.wait == 0) {
            self.command_done(who);
        }
        Ok(self.command(who).status)
    }

    /// `check_allow()`: whether `who` may issue a command allowed to
    /// `allow`: `p` players, `c` characters, `r` restricted characters
    /// (whose letter is `cmd_allow`), `m` the gamemaster's units.
    ///
    /// # Panics
    ///
    /// Panics if `who` is neither a player nor a character, as C asserts.
    fn check_allow(&self, who: i32, allow: &str) -> bool {
        // `immediate` mode, which also allows `i`, isn't ported.
        let t = match self.db.kind(who) {
            Kind::PLAYER => b'p',
            Kind::CHAR => match restricted_control(&self.db, who) {
                0 => b'c',
                t => t,
            },
            _ => {
                let mut msg = b"check_allow: bad kind: ".to_vec();
                msg.extend(box_name(&self.db, who));
                msg.push(b'\n');
                crate::code::eprint_bytes(&msg);
                panic!("check_allow: bad kind");
            }
        };
        if allow.contains('m') && player(&self.db, who) == GM_PLAYER {
            return true;
        }
        if !allow.as_bytes().contains(&t) {
            // TODO(stage 2): wout(who, "%s may not issue that order.")
            return false;
        }
        true
    }

    /// `do_command()`: starts a loaded command. An error, a command `who`
    /// may not issue, or one with no start routine fails; otherwise the
    /// start routine runs, and a command that takes no time finishes at
    /// once. A failed command loads the next one.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] for a handler not ported yet.
    pub fn do_command(&mut self, who: i32) -> Phase {
        // TODO(stage 2): out(who, "> %s", c->line), and if the command
        // matched fuzzily, out(who, "(assuming you meant '%s')").
        let (state, cmd) = {
            let c = self.command(who);
            (c.state, c.cmd)
        };
        let ent = tbl(cmd);

        if state == STATE_ERROR {
            // TODO(stage 2): out(who, "Unrecognized command.")
            self.command(who).status = 0;
        } else if !self.check_allow(who, ent.allow) {
            self.command(who).status = 0;
        } else if let Some(start) = ent.start {
            let pl = player(&self.db, who);
            let p = self.db.p_player(pl);
            p.cmd_count = p.cmd_count.wrapping_add(1);

            self.set_state(who, STATE_RUN, 0);

            let c = self.command(who);
            assert_eq!(c.days_executing, 0, "do_command: days_executing");
            c.debug = 0;
            c.inhibit_finish = 0;
            // The start routine may re-parse the command.
            let status = self.call_start(who, cmd, start)?;
            self.command(who).status = status;
        } else {
            // TODO(stage 2): out(who, "Unimplemented command.")
            self.command(who).status = 0;
        }

        let c = self.command(who);
        if c.status == 0 {
            self.command_done(who);
        } else if c.wait == 0 && c.state == STATE_RUN {
            let status = self.finish_command(who)?;
            self.command(who).status = status;
        }

        self.check_trades_to_check()
    }

    /// `if (ilist_len(trades_to_check) > 0) check_validated_trades()`.
    fn check_trades_to_check(&mut self) -> Phase {
        if self.driver.trades_to_check.is_empty() {
            return Ok(());
        }
        self.check_validated_trades()
    }

    /// Runs command `cmd`'s start routine `start` for `who`.
    fn call_start(&mut self, who: i32, cmd: i32, start: &str) -> Result<i8, NotYetImplemented> {
        match start {
            "v_wait" => Ok(self.v_wait(who)),
            // olympia/c2.c:v_stop: a stop order that wasn't at the head of
            // the queue on day 0 does nothing.
            "v_stop" => Ok(1),
            _ => Err(handler_not_ported(cmd, start)),
        }
    }

    /// Runs the finish routine `finish` of `who`'s command.
    fn call_finish(&mut self, who: i32, finish: &str) -> Result<i8, NotYetImplemented> {
        match finish {
            "d_wait" => Ok(self.d_wait(who)),
            _ => Err(handler_not_ported(self.command(who).cmd, finish)),
        }
    }

    /// Runs the interrupt routine `interrupt` of `who`'s command.
    fn call_interrupt(&mut self, who: i32, interrupt: &str) -> Result<i8, NotYetImplemented> {
        match interrupt {
            "i_wait" => Ok(self.i_wait(who)),
            _ => Err(handler_not_ported(self.command(who).cmd, interrupt)),
        }
    }

    /// `init_load_sup()`: puts `who`'s command back on its queue, or loads
    /// its first order, giving it a command structure if it has none.
    ///
    /// # Panics
    ///
    /// Panics on a command in `STATE_ERROR` or an unknown state, as C
    /// asserts.
    pub fn init_load_sup(&mut self, who: i32) {
        let c = self.db.entity_mut(who).cmd.get_or_insert_with(|| {
            Box::new(Command {
                who,
                state: STATE_DONE,
                ..Command::default()
            })
        });
        assert_eq!(who, c.who, "init_load_sup: command of {}", c.who);
        let (state, pri) = (c.state, c.pri);

        match state {
            STATE_LOAD => self.driver.load_q[pri as usize].push(who),
            STATE_RUN => self.driver.run_q.push(who),
            STATE_DONE => {
                self.load_command(who);
            }
            _ => panic!("init_load_sup: {who} has command state {state}"),
        }
    }

    /// `initial_command_load()`: queues every character's command, then
    /// every player's.
    pub(crate) fn initial_command_load(&mut self) {
        for kind in [Kind::CHAR, Kind::PLAYER] {
            let mut i = self.db.kind_first(kind);
            while i > 0 {
                let next = self.db.kind_next(i);
                self.init_load_sup(i);
                i = next;
            }
        }
    }

    /// The characters, in kind-chain order, read as `loop_char` does.
    fn loop_char(&self) -> Vec<i32> {
        self.db.boxes_of_kind(Kind::CHAR)
    }

    /// `min_pri_ready()`: the lowest priority with a command ready to run,
    /// or 99. Prisoners, moving characters and those held back by an
    /// auto-attack wait.
    fn min_pri_ready(&self) -> i32 {
        for (pri, q) in self.driver.load_q.iter().enumerate() {
            for &who in q {
                let c = self.rp_command(who).expect("queued command");
                assert_eq!(c.state, STATE_LOAD, "min_pri_ready: {who} not loaded");
                assert_eq!(c.pri as usize, pri, "min_pri_ready: {who} priority");
                if !is_prisoner(&self.db, c.who)
                    && char_moving(&self.db, c.who) == 0
                    && !c.second_wait
                {
                    return pri as i32;
                }
            }
        }
        NONE_READY
    }

    /// `init_wait_list()`: the characters already running `wait`.
    pub(crate) fn init_wait_list(&mut self) {
        for i in self.loop_char() {
            if let Some(c) = self.rp_command(i) {
                if c.state == STATE_RUN && c.cmd == self.cmd_wait {
                    self.driver.wait_list.push(i);
                }
            }
        }
    }

    /// `olympia/produce.c:init_collect_list`: the characters already
    /// running `collect`.
    pub(crate) fn init_collect_list(&mut self) {
        let cmd_collect = find_command(b"collect").map_or(-1, |(i, _)| i);
        assert!(cmd_collect > 0, "init_collect_list: no collect command");
        for i in self.loop_char() {
            if let Some(c) = self.rp_command(i) {
                if c.state == STATE_RUN && c.cmd == cmd_collect {
                    self.driver.collectors.push(i);
                }
            }
        }
    }

    /// `check_all_waits()`: checks every waiting unit's conditions.
    ///
    /// C walks the live list, so a unit whose wait ends (and leaves the
    /// list) makes the walk skip the unit after it; kept as C has it.
    fn check_all_waits(&mut self) -> Phase {
        let mut i = 0;
        while i < self.driver.wait_list.len() {
            let who = self.driver.wait_list[i];
            if let Some(c) = self.rp_command(who) {
                if c.state == STATE_RUN && c.cmd == self.cmd_wait {
                    assert_eq!(c.wait, -1, "check_all_waits: {who} wait");
                    self.finish_command(who)?;
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// `exec_precedence()`: a character's place in the running order:
    /// stack leaders first, then by place in the here list.
    fn exec_precedence(&self, who: i32) -> i32 {
        if self.db.kind(who) != Kind::CHAR {
            return 0;
        }
        let mut stack_depth = 0;
        let mut n = who;
        while n != 0 {
            n = stack_parent(&self.db, n);
            stack_depth += 1;
        }
        let pos = here_pos(&self.db, who);
        assert!(stack_depth < 100, "exec_precedence: stack depth");
        assert!(pos < 10000, "exec_precedence: here position");
        stack_depth * 10000 + pos
    }

    /// Sorts `l` by each box's `temp`, then by number (`exec_comp`).
    fn sort_by_temp(&self, l: &mut [i32]) {
        l.sort_by_key(|&i| (self.db.entity(i).temp, i));
    }

    /// `sort_load_queue()`: puts load queue `pri` in running order.
    fn sort_load_queue(&mut self, pri: usize) {
        let mut l = std::mem::take(&mut self.driver.load_q[pri]);
        for &i in &l {
            self.db.entity_mut(i).temp = self.exec_precedence(i);
        }
        self.sort_by_temp(&mut l);
        self.driver.load_q[pri] = l;
    }

    /// `sort_run_queue()`: puts the run queue in running order, lower
    /// priority numbers first.
    fn sort_run_queue(&mut self) {
        let mut l = std::mem::take(&mut self.driver.run_q);
        for &i in &l {
            let pri = i32::from(self.rp_command(i).expect("running command").pri);
            self.db.entity_mut(i).temp = pri * 1_000_000 + self.exec_precedence(i);
        }
        self.sort_by_temp(&mut l);
        self.driver.run_q = l;
    }

    /// `start_phase()`: starts the day's ready commands, lowest priority
    /// number first. Auto-attacks are checked once a day, before the first
    /// command of priority 3 or more.
    fn start_phase(&mut self) -> Phase {
        loop {
            let pri = self.min_pri_ready();
            self.driver.cur_pri = pri;

            if self.driver.auto_attack_flag && pri >= 3 {
                self.check_all_auto_attacks()?;
                self.driver.auto_attack_flag = false;
            }

            if pri == NONE_READY {
                return Ok(());
            }

            let pri_ix = pri as usize;
            self.sort_load_queue(pri_ix);
            let l = self.driver.load_q[pri_ix].clone();

            for i in l {
                let c = self.rp_command(i).expect("queued command");
                if c.state == STATE_LOAD
                    && i32::from(c.pri) == pri
                    && !is_prisoner(&self.db, i)
                    && char_moving(&self.db, i) == 0
                    && !c.second_wait
                {
                    self.do_command(i)?;
                    self.check_all_waits()?;

                    if pri != self.driver.cur_pri {
                        break;
                    }
                }
            }
        }
    }

    /// `evening_phase()`: the day's completion check for every running
    /// command.
    fn evening_phase(&mut self) -> Phase {
        self.driver.evening = true;

        self.sort_run_queue();
        let l = self.driver.run_q.clone();
        for i in l {
            let c = self.rp_command(i).expect("running command");
            if c.state != STATE_RUN || c.second_wait {
                continue;
            }
            self.command(i).days_executing += 1;
            self.finish_command(i)?;
            self.check_trades_to_check()?;
        }

        self.check_all_waits()?;

        self.driver.evening = false;
        Ok(())
    }

    /// `daily_command_loop()`: the day's commands.
    pub(crate) fn daily_command_loop(&mut self) -> Phase {
        self.driver.auto_attack_flag = true;
        self.start_phase()?;
        self.evening_phase()?;
        self.clear_second_waits();
        Ok(())
    }

    /// `process_player_orders()`: runs each player's orders, all at once
    /// on day 0.
    pub(crate) fn process_player_orders(&mut self) -> Phase {
        let mut pl = self.db.kind_first(Kind::PLAYER);
        while pl > 0 {
            let next = self.db.kind_next(pl);
            if self.rp_command(pl).is_some() {
                // A quit order can delete the player.
                while self.db.kind(pl) == Kind::PLAYER && self.command(pl).state == STATE_LOAD {
                    self.do_command(pl)?;
                }
            }
            pl = next;
        }
        Ok(())
    }

    /// `interrupt_order()`: stops `who`'s running command, if any, and
    /// loads its next one.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] for an interrupt routine not ported
    /// yet.
    pub fn interrupt_order(&mut self, who: i32) -> Phase {
        if stack_leader(&self.db, who) == who {
            restore_stack_actions(&mut self.db, who); // not moving anymore
        }

        let Some(c) = self.rp_command(who) else {
            return Ok(());
        };
        if c.state == STATE_RUN {
            if let Some(interrupt) = tbl(c.cmd).interrupt {
                let status = self.call_interrupt(who, interrupt)?;
                self.command(who).status = status;
            }
            self.command_done(who);
            assert_ne!(self.command(who).state, STATE_RUN);
        }
        Ok(())
    }

    /// `process_interrupted_units()`: carries out the `stop` orders at the
    /// head of the units' queues, on day 0.
    pub(crate) fn process_interrupted_units(&mut self) -> Phase {
        let mut who = self.db.kind_first(Kind::CHAR);
        while who > 0 {
            let next = self.db.kind_next(who);
            if stop_order(&self.db, player(&self.db, who), who) {
                let pl = player(&self.db, who);
                pop_order(&mut self.db, pl, who);
                // TODO(stage 2): out(who, "> stop")

                let running = self
                    .rp_command(who)
                    .is_some_and(|c| c.cmd != 0 && c.wait != 0);
                if running {
                    // TODO(stage 2): out(who, "Interrupt current order.")
                    self.interrupt_order(who)?;
                } else {
                    // TODO(stage 2): out(who, "No order is currently
                    // executing.")
                }
            }
            who = next;
        }
        Ok(())
    }

    /// `olympia/order.c:flush_unit_orders`: drops every order queued for
    /// `who` in player `pl`'s orders, and a loaded command of `pl`'s unit.
    pub fn flush_unit_orders(&mut self, pl: i32, who: i32) {
        if self.db.get(who).is_none() {
            return;
        }
        while top_order(&self.db, pl, who).is_some() {
            pop_order(&mut self.db, pl, who);
        }
        if player(&self.db, who) == pl
            && self.rp_command(who).is_some_and(|c| c.state == STATE_LOAD)
        {
            self.command_done(who);
        }
    }
}

/// `olympia/stack.c:here_pos`: `who`'s place in the here list of where it
/// is.
///
/// # Panics
///
/// Panics if `who` isn't in that here list, as C asserts.
fn here_pos(db: &crate::entity::Db, who: i32) -> i32 {
    let l = &db.entity(db.loc(who)).loc_info.here_list;
    l.as_slice()
        .iter()
        .position(|&x| x == who)
        .map(|p| p as i32)
        .unwrap_or_else(|| panic!("here_pos: {who} not in its here list"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Subkind;
    use crate::order::queue_order;
    use std::path::PathBuf;

    const PL: i32 = 50001;
    const PROV: i32 = 10101;

    /// A game with player [`PL`], a plain province and characters `chars`
    /// sworn to the player, standing in the province in that order.
    fn game(chars: &[i32]) -> Game {
        let mut g = Game::new(PathBuf::from("lib"));
        g.cmd_wait = find_command(b"wait").unwrap().0;
        g.system.sysclock.turn = 3;
        g.db.alloc_box(PL, Kind::PLAYER, Subkind::PL_REGULAR);
        g.db.alloc_box(PROV, Kind::LOC, Subkind::PLAIN);
        for &c in chars {
            g.db.alloc_box(c, Kind::CHAR, Subkind::default());
            g.db.p_char(c).unit_lord = PL;
            g.db.set_where(c, PROV);
        }
        g
    }

    fn state(g: &Game, who: i32) -> (i8, i32, i32) {
        let c = g.rp_command(who).unwrap();
        (c.state, c.cmd, c.wait)
    }

    fn orders_left(g: &Game, who: i32) -> usize {
        g.db.entity(PL)
            .player
            .as_ref()
            .unwrap()
            .orders
            .iter()
            .find(|o| o.unit == who)
            .map_or(0, |o| o.l.len())
    }

    /// Runs the days `from..=to` of the month.
    fn run_days(g: &mut Game, from: i16, to: i16) {
        for day in from..=to {
            g.system.sysclock.day = day;
            g.daily_command_loop().unwrap();
        }
    }

    #[test]
    fn initial_load_pops_an_order_and_queues_it_by_priority() {
        let mut g = game(&[1001, 1002]);
        queue_order(&mut g.db, PL, 1001, b"wait time 2");
        queue_order(&mut g.db, PL, 1001, b"move n");
        g.initial_command_load();

        assert_eq!(state(&g, 1001), (STATE_LOAD, g.cmd_wait, -1));
        assert_eq!(orders_left(&g, 1001), 1);
        // wait has priority 1.
        assert_eq!(g.driver.load_q[1], [1001]);
        // No orders: done, and on no queue.
        assert_eq!(state(&g, 1002).0, STATE_DONE);
        assert_eq!(g.rp_command(PL).unwrap().state, STATE_DONE);
        // Loading an order marks the faction as playing this turn.
        assert_eq!(g.db.entity(PL).player.as_ref().unwrap().last_order_turn, 3);
    }

    #[test]
    fn wait_time_runs_its_days_then_loads_the_next_order() {
        let mut g = game(&[1001]);
        queue_order(&mut g.db, PL, 1001, b"wait time 2");
        queue_order(&mut g.db, PL, 1001, b"wait time 1");
        g.initial_command_load();

        run_days(&mut g, 1, 1);
        let c = g.rp_command(1001).unwrap();
        assert_eq!((c.state, c.days_executing), (STATE_RUN, 1));
        assert_eq!(g.driver.wait_list, [1001]);
        assert_eq!(g.driver.run_q, [1001]);
        assert_eq!(g.db.entity(PL).player.as_ref().unwrap().cmd_count, 1);

        // Day 2's evening ends the first wait and loads the second, which
        // starts on day 3 and ends that evening.
        run_days(&mut g, 2, 2);
        let c = g.rp_command(1001).unwrap();
        assert_eq!(c.state, STATE_LOAD);
        assert_eq!(c.parse[2], b"1");
        assert!(g.driver.wait_list.is_empty());

        run_days(&mut g, 3, 3);
        let c = g.rp_command(1001).unwrap();
        assert_eq!(
            (c.state, c.days_executing, c.inhibit_finish),
            (STATE_DONE, 1, 1)
        );
        assert!(g.driver.run_q.is_empty());
    }

    #[test]
    fn a_wait_that_already_holds_finishes_when_it_starts() {
        let mut g = game(&[1001]);
        queue_order(&mut g.db, PL, 1001, b"wait day 1");
        queue_order(&mut g.db, PL, 1001, b"wait");
        queue_order(&mut g.db, PL, 1001, b"wait bogus");
        queue_order(&mut g.db, PL, 1001, b"wait time 9");
        g.initial_command_load();
        run_days(&mut g, 1, 1);
        // `wait day 1` holds, `wait` fails for want of a condition, and
        // `wait bogus` ends on its unknown condition: all on day 1.
        assert_eq!(state(&g, 1001), (STATE_RUN, g.cmd_wait, -1));
        assert_eq!(g.rp_command(1001).unwrap().parse[2], b"9");
        assert_eq!(orders_left(&g, 1001), 0);
    }

    #[test]
    fn an_unrecognized_order_leaves_the_unit_stuck() {
        let mut g = game(&[1001]);
        queue_order(&mut g.db, PL, 1001, b"bogus");
        queue_order(&mut g.db, PL, 1001, b"wait time 1");
        g.initial_command_load();
        assert_eq!(state(&g, 1001), (STATE_ERROR, 0, 0));
        assert!(g.driver.load_q.iter().all(Vec::is_empty));
        run_days(&mut g, 1, 30);
        // No queue holds a STATE_ERROR command, so the next order never
        // loads, as in C.
        assert_eq!(state(&g, 1001).0, STATE_ERROR);
        assert_eq!(orders_left(&g, 1001), 1);
    }

    #[test]
    fn a_command_with_no_ported_handler_stops_the_run() {
        let mut g = game(&[1001]);
        queue_order(&mut g.db, PL, 1001, b"move n");
        g.initial_command_load();
        g.system.sysclock.day = 1;
        assert_eq!(
            g.daily_command_loop(),
            Err(NotYetImplemented("move command (v_move)".to_owned()))
        );
    }

    #[test]
    fn players_may_not_wait_and_unimplemented_commands_fail() {
        let mut g = game(&[]);
        queue_order(&mut g.db, PL, PL, b"begin");
        queue_order(&mut g.db, PL, PL, b"wait time 1");
        queue_order(&mut g.db, PL, PL, b"stop");
        queue_order(&mut g.db, PL, PL, b"wait time 2");
        g.initial_command_load();
        g.process_player_orders().unwrap();
        // begin has no start routine and wait isn't allowed to a player;
        // stop is, and finishes at once. All of them ran on day 0.
        assert_eq!(state(&g, PL).0, STATE_DONE);
        assert_eq!(orders_left(&g, PL), 0);
        assert_eq!(g.db.entity(PL).player.as_ref().unwrap().cmd_count, 1);
    }

    #[test]
    fn a_stop_at_the_head_of_the_queue_interrupts_a_running_wait() {
        let mut g = game(&[1001]);
        g.db.entity_mut(1001).cmd = Some(Box::new(Command {
            who: 1001,
            cmd: g.cmd_wait,
            state: STATE_RUN,
            wait: -1,
            poll: 1,
            pri: 1,
            parse: vec![b"wait".to_vec(), b"time".to_vec(), b"5".to_vec()],
            ..Command::default()
        }));
        g.db.p_char(1001).moving = 7;
        queue_order(&mut g.db, PL, 1001, b"stop");
        queue_order(&mut g.db, PL, 1001, b"wait time 1");
        g.init_wait_list();
        g.initial_command_load();
        assert_eq!(g.driver.wait_list, [1001]);
        assert_eq!(g.driver.run_q, [1001]);

        g.process_interrupted_units().unwrap();
        // i_wait took it off the wait list, the stack stopped moving and
        // the next order is loaded.
        assert!(g.driver.wait_list.is_empty());
        assert!(g.driver.run_q.is_empty());
        assert_eq!(g.db.entity(1001).char_.as_ref().unwrap().moving, 0);
        assert_eq!(state(&g, 1001), (STATE_LOAD, g.cmd_wait, -1));
        assert_eq!(orders_left(&g, 1001), 0);
    }

    #[test]
    fn commands_run_in_priority_then_stack_then_here_order() {
        let mut g = game(&[1003, 1001, 1002]);
        // 1004 is stacked under 1003.
        g.db.alloc_box(1004, Kind::CHAR, Subkind::default());
        g.db.p_char(1004).unit_lord = PL;
        g.db.set_where(1004, 1003);
        for who in [1001, 1002, 1003, 1004] {
            g.init_load_sup(who);
            g.set_state(who, STATE_LOAD, 1);
            g.command(who).pri = 1;
        }
        g.sort_load_queue(1);
        assert_eq!(g.driver.load_q[1], [1003, 1001, 1002, 1004]);
        assert_eq!(g.db.entity(1004).temp, 20000);

        g.command(1002).pri = 1;
        for who in [1001, 1002, 1003, 1004] {
            g.set_state(who, STATE_RUN, 0);
        }
        g.command(1003).pri = 3;
        g.sort_run_queue();
        assert_eq!(g.driver.run_q, [1001, 1002, 1004, 1003]);
    }

    #[test]
    fn set_state_moves_between_queues() {
        let mut g = game(&[1001, 1002]);
        for who in [1001, 1002] {
            g.init_load_sup(who);
        }
        g.set_state(1001, STATE_LOAD, 3);
        g.command(1001).pri = 3;
        g.set_state(1002, STATE_LOAD, 3);
        g.command(1002).pri = 3;
        assert_eq!(g.driver.load_q[3], [1001, 1002]);
        g.set_state(1001, STATE_RUN, 0);
        assert_eq!(g.driver.load_q[3], [1002]);
        assert_eq!(g.driver.run_q, [1001]);
        g.set_state(1001, STATE_DONE, 0);
        assert!(g.driver.run_q.is_empty());
        assert_eq!(g.min_pri_ready(), 3);
        g.db.p_char(1002).prisoner = 1;
        assert_eq!(g.min_pri_ready(), NONE_READY);
    }

    #[test]
    fn rem_value_uniq_removes_the_last_match() {
        let mut l = vec![1, 2, 1, 3];
        rem_value_uniq(&mut l, 1);
        assert_eq!(l, [1, 2, 3]);
        rem_value(&mut l, 9);
        assert_eq!(l, [1, 2, 3]);
    }
}
