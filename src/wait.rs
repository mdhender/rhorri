// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `wait` command (`olympia/c1.c:v_wait`, `d_wait`, `i_wait`).
//!
//! `wait` takes a list of conditions and runs until any one of them holds.
//! `not` before a condition inverts it. The conditions are checked when the
//! command starts, every evening (it polls), and after each command that
//! starts during the day (`check_all_waits`).

use crate::check::stack_leader;
use crate::code::{box_code, box_name, comma_num, just_name, nice_num, plural_item_name};
use crate::command::{numargs, parse_arg};
use crate::cstr::i_str_eq;
use crate::entity::{Command, Db, Kind, Subkind, Text, WaitArg};
use crate::game::{Game, player};
use crate::input::rem_value;
use crate::loc::{
    first_character, is_loc_or_ship, is_ship, is_ship_notdone, province, subloc, weather_here,
};
use crate::unit::{ITEM_GOLD, char_here, ferry_horn, has_item};

/// A signal raised by the `flag` command (`struct flag_ent`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlagEnt {
    /// Who raised it.
    pub who: i32,
    pub flag: Text,
}

/// `wait_tags`: the conditions, by tag number.
const WAIT_TAGS: [&str; 23] = [
    "time", "day", "unit", "gold", "item", "flag", "loc", "stack", "top", "ferry", "ship", "rain",
    "fog", "wind", "not", "owner", "raining", "foggy", "windy", "clear", "shiploc", "month",
    "turn",
];

/// `olympia/u.c:lookup`: the index of `s` in `table`, ignoring case.
pub(crate) fn lookup(table: &[&str], s: &[u8]) -> Option<i32> {
    table
        .iter()
        .position(|t| i_str_eq(s, t.as_bytes()))
        .map(|i| i as i32)
}

/// `sout()` for the condition messages.
fn msg(s: impl Into<Vec<u8>>) -> Text {
    s.into()
}

/// Joins byte strings into one message.
fn cat(parts: &[&[u8]]) -> Text {
    parts.concat()
}

/// `olympia/u.c:gold_s`.
fn gold_s(n: i32) -> Text {
    format!("{}~gold", comma_num(n)).into_bytes()
}

/// `olympia/code.c:just_name_qty`.
fn just_name_qty(db: &Db, item: i32, qty: i32) -> Text {
    let mut s = nice_num(qty).into_bytes();
    s.push(b'~');
    s.extend(plural_item_name(db, item, qty));
    s
}

/// `" not"` when the condition is inverted.
fn not_s(not: i32) -> &'static str {
    if not != 0 { " not" } else { "" }
}

/// `parse_wait_args()`: reads the command's conditions. On an error the
/// conditions read so far stay parsed, as in C, and the message says what
/// is wrong.
fn parse_wait_args(db: &Db, c: &Command) -> (Vec<WaitArg>, Option<Text>) {
    assert!(c.wait_parse.is_empty(), "parse_wait_args: already parsed");
    let mut args = Vec::new();
    let n = numargs(c);
    let arg = |i: i32| -> &[u8] { &c.parse[i as usize] };

    let mut i = 1;
    while i <= n {
        let tag_s = arg(i);
        let Some(tag) = lookup(&WAIT_TAGS, tag_s) else {
            let mut m = b"Unknown condition '".to_vec();
            m.extend_from_slice(tag_s);
            m.extend_from_slice(b"'.");
            return (args, Some(m));
        };
        let tag = match tag {
            16 => 11,
            17 => 12,
            18 => 13,
            t => t,
        };
        i += 1;

        let mut new = WaitArg {
            tag,
            ..WaitArg::default()
        };
        let missing = |args: Vec<WaitArg>, new: WaitArg| {
            let mut args = args;
            args.push(new);
            let mut m = b"Argument missing for '".to_vec();
            m.extend_from_slice(tag_s);
            m.extend_from_slice(b"'.");
            (args, Some(m))
        };

        match tag {
            // time, day, unit, gold, loc, stack, ferry, ship, shiploc,
            // month, turn: one argument.
            0 | 1 | 2 | 3 | 6 | 7 | 9 | 10 | 20 | 21 | 22 => {
                if i > n {
                    return missing(args, new);
                }
                new.a1 = parse_arg(db, c.who, arg(i));
                i += 1;
            }
            // item n [q]
            4 => {
                if i > n {
                    return missing(args, new);
                }
                new.a1 = parse_arg(db, c.who, arg(i));
                i += 1;
                if i <= n {
                    new.a2 = parse_arg(db, c.who, arg(i));
                    i += 1;
                } else {
                    new.a2 = 1;
                }
            }
            // flag f [n]
            5 => {
                if i > n {
                    args.push(new);
                    return (args, Some(msg("Flag missing.")));
                }
                new.flag = Some(arg(i).to_vec());
                i += 1;
                new.a1 = player(db, c.who);
                if i <= n
                    && (arg(i).first().is_some_and(u8::is_ascii_digit)
                        || parse_arg(db, c.who, arg(i)) != 0)
                {
                    new.a1 = parse_arg(db, c.who, arg(i));
                    i += 1;
                }
            }
            // top, rain, fog, wind, not, owner, clear: no argument.
            8 | 11 | 12 | 13 | 14 | 15 | 19 => {}
            _ => {
                eprintln!("tag is {tag}");
                panic!("parse_wait_args: bad tag");
            }
        }
        args.push(new);
    }
    (args, None)
}

impl Game {
    /// `flag_raised()`: the first signal `flag` raised by `who` or a unit
    /// of faction `who` (anyone's if `who` is 0).
    fn flag_raised(&self, who: i32, flag: &[u8]) -> Option<usize> {
        self.driver.flags.iter().position(|f| {
            (who == 0 || player(&self.db, f.who) == who || f.who == who) && i_str_eq(&f.flag, flag)
        })
    }

    /// `check_wait_conditions()`: the message for the first condition of
    /// `who`'s wait that holds, or `None` to keep waiting. Parses the
    /// conditions first if they aren't yet.
    fn check_wait_conditions(&mut self, who: i32) -> Option<Text> {
        let db = &self.db;
        let mut where_ship = subloc(db, who);
        if is_ship(db, where_ship) || is_ship_notdone(db, where_ship) {
            where_ship = subloc(db, where_ship);
        }

        if self.command(who).wait_parse.is_empty() {
            let (args, err) = parse_wait_args(&self.db, self.rp_command(who).expect("command"));
            self.command(who).wait_parse = args;
            if err.is_some() {
                return err;
            }
            assert!(!self.command(who).wait_parse.is_empty());
        }

        let db = &self.db;
        let c = self.rp_command(who).expect("command");
        let day = i32::from(self.system.sysclock.day);
        let turn = i32::from(self.system.sysclock.turn);

        // C's `cond = not - cond`: true when the condition holds and isn't
        // inverted, or doesn't hold and is. A count (the weather's
        // strength) above 1 reads as holding even when inverted
        // (playbymail/olympia-g3#247).
        let mut not = 0;
        let mut setnot = false;
        for p in &c.wait_parse {
            if setnot {
                setnot = false;
            } else if not != 0 {
                not = 0;
            }
            let held = |cond: i32| not - cond != 0;
            let (a1, a2) = (p.a1, p.a2);

            match p.tag {
                // time n
                0 => {
                    if held(i32::from(c.days_executing >= a1)) {
                        return Some(msg(format!(
                            "{} day{}{} passed.",
                            nice_num(a1),
                            if a1 == 1 { " has" } else { "s have" },
                            not_s(not)
                        )));
                    }
                }
                // day n
                1 => {
                    if held(i32::from(day >= a1)) {
                        return Some(msg(if not != 0 {
                            format!("today is{} day {a1}.", not_s(not))
                        } else {
                            format!("today is day {day}.")
                        }));
                    }
                }
                // month n, turn n
                21 | 22 => {
                    if held(i32::from(turn >= a1)) {
                        return Some(msg(if not != 0 {
                            format!("it is{} turn {a1}.", not_s(not))
                        } else {
                            format!("it is turn {turn}.")
                        }));
                    }
                }
                // unit n
                2 => {
                    if !db.valid_box(a1) {
                        return Some(cat(&[&box_code(a1), b" does not exist."]));
                    }
                    if held(i32::from(char_here(db, who, a1))) {
                        return Some(cat(&[
                            &box_code(a1),
                            b" is",
                            not_s(not).as_bytes(),
                            b" here.",
                        ]));
                    }
                }
                // gold n
                3 => {
                    if held(i32::from(has_item(db, who, ITEM_GOLD) >= a1)) {
                        let name = just_name(db, who);
                        return Some(if not != 0 {
                            cat(&[&name, b" doesn't have ", &gold_s(a1), b"."])
                        } else {
                            cat(&[&name, b" has ", &gold_s(has_item(db, who, ITEM_GOLD)), b"."])
                        });
                    }
                }
                // item n q
                4 => {
                    let cond = db.kind(a1) == Kind::ITEM && has_item(db, who, a1) >= a2;
                    if held(i32::from(cond)) {
                        let name = just_name(db, who);
                        return Some(if not != 0 {
                            cat(&[&name, b" doesn't have ", &just_name_qty(db, a1, a2), b"."])
                        } else {
                            let qty = has_item(db, who, a1);
                            cat(&[&name, b" has ", &just_name_qty(db, a1, qty), b"."])
                        });
                    }
                }
                // flag f [n]
                5 => {
                    if a1 != 0 && !db.valid_box(a1) {
                        return Some(cat(&[&box_code(a1), b" does not exist."]));
                    }
                    let j = self.flag_raised(a1, p.flag.as_deref().unwrap_or(b""));
                    if not != 0 {
                        if j.is_none() {
                            return Some(msg("received no signal"));
                        }
                    } else if let Some(j) = j {
                        let f = &self.driver.flags[j];
                        return Some(cat(&[&box_name(db, f.who), b" signaled '", &f.flag, b"'"]));
                    }
                }
                // loc n
                6 => {
                    if !is_loc_or_ship(db, a1) {
                        return Some(cat(&[&box_code(a1), b" is not a location or ship."]));
                    }
                    if held(i32::from(subloc(db, who) == a1)) {
                        return Some(at_msg(db, not, a1));
                    }
                }
                // shiploc n
                20 => {
                    let ship = subloc(db, who);
                    if !is_ship(db, ship) && !is_ship_notdone(db, ship) {
                        return Some(cat(&[&box_name(db, who), b" is not on a ship."]));
                    }
                    if !is_loc_or_ship(db, a1) {
                        return Some(cat(&[&box_code(a1), b" is not a location or ship."]));
                    }
                    if held(i32::from(subloc(db, ship) == a1)) {
                        return Some(at_msg(db, not, a1));
                    }
                }
                // stack n: a dead or missing unit just hangs.
                7 => {
                    if db.kind(a1) == Kind::CHAR
                        && held(i32::from(stack_leader(db, who) == stack_leader(db, a1)))
                    {
                        return Some(cat(&[
                            &box_name(db, a1),
                            b" is",
                            not_s(not).as_bytes(),
                            b" stacked with us.",
                        ]));
                    }
                }
                // top
                8 => {
                    if held(i32::from(stack_leader(db, who) == who)) {
                        return Some(msg(format!("we are{} the stack leader", not_s(not))));
                    }
                }
                // ferry n
                9 => {
                    if !is_ship(db, a1) {
                        return Some(cat(&[&box_code(a1), b" is not a ship"]));
                    }
                    let cond = subloc(db, a1) == subloc(db, who) && ferry_horn(db, a1);
                    if held(i32::from(cond)) {
                        return Some(msg(format!("the ferry has{} signaled.", not_s(not))));
                    }
                }
                // ship n
                10 => {
                    if db.kind(a1) != Kind::SHIP {
                        return Some(cat(&[&box_code(a1), b" is not a ship."]));
                    }
                    if held(i32::from(where_ship == subloc(db, a1))) {
                        return Some(cat(&[
                            &box_code(a1),
                            b" is",
                            not_s(not).as_bytes(),
                            b" here.",
                        ]));
                    }
                }
                // rain, fog, wind
                11..=13 => {
                    let (sk, word) = match p.tag {
                        11 => (Subkind::RAIN, "raining"),
                        12 => (Subkind::FOG, "foggy"),
                        _ => (Subkind::WIND, "windy"),
                    };
                    if held(weather_here(db, province(db, who), sk)) {
                        return Some(msg(format!("it is{} {word}.", not_s(not))));
                    }
                }
                // not
                14 => {
                    not = 1;
                    setnot = true;
                }
                // owner
                15 => {
                    if held(i32::from(first_character(db, subloc(db, who)) == who)) {
                        return Some(msg(format!(
                            "we are{} the first character here",
                            not_s(not)
                        )));
                    }
                }
                // clear
                19 => {
                    let here = subloc(db, who);
                    let cond = weather_here(db, here, Subkind::FOG) == 0
                        && weather_here(db, here, Subkind::RAIN) == 0
                        && weather_here(db, here, Subkind::WIND) == 0;
                    if held(i32::from(cond)) {
                        return Some(msg(format!("it is{} clear.", not_s(not))));
                    }
                }
                tag => {
                    eprintln!("tag is {tag}");
                    panic!("check_wait_conditions: bad tag");
                }
            }
        }
        None
    }

    /// `v_wait()`: starts a wait. If a condition already holds, the
    /// command is done at once.
    pub(crate) fn v_wait(&mut self, who: i32) -> i8 {
        if numargs(self.command(who)) < 1 {
            // TODO(stage 2): wout(who, "Must say what condition to wait
            // for.")
            return 0;
        }

        // clear_wait_parse()
        self.command(who).wait_parse.clear();

        if let Some(_s) = self.check_wait_conditions(who) {
            // TODO(stage 2): wout(who, "Wait finished: %s", s)
            let c = self.command(who);
            c.wait = 0;
            c.inhibit_finish = 1; // don't call d_wait
            return 1;
        }

        self.driver.wait_list.push(who);
        1
    }

    /// `d_wait()`: the daily check; ends the wait when a condition holds.
    pub(crate) fn d_wait(&mut self, who: i32) -> i8 {
        if let Some(_s) = self.check_wait_conditions(who) {
            // TODO(stage 2): wout(who, "Wait finished: %s", s)
            rem_value(&mut self.driver.wait_list, who);
            let c = self.command(who);
            c.wait = 0;
            c.inhibit_finish = 1;
        }
        1
    }

    /// `i_wait()`: an interrupted wait leaves the wait list.
    pub(crate) fn i_wait(&mut self, who: i32) -> i8 {
        rem_value(&mut self.driver.wait_list, who);
        1
    }
}

/// `"%sat %s."`: where the unit is, or isn't.
fn at_msg(db: &Db, not: i32, a1: i32) -> Text {
    cat(&[
        if not != 0 { b"not " } else { b"" },
        b"at ",
        &box_name(db, a1),
        b".",
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Subkind;
    use std::path::PathBuf;

    const PL: i32 = 50001;
    const PROV: i32 = 10101;
    const WHO: i32 = 1001;

    fn game() -> Game {
        let mut g = Game::new(PathBuf::from("lib"));
        g.db.alloc_box(PL, Kind::PLAYER, Subkind::PL_REGULAR);
        g.db.alloc_box(PROV, Kind::LOC, Subkind::PLAIN);
        g.db.alloc_box(WHO, Kind::CHAR, Subkind::default());
        g.db.p_char(WHO).unit_lord = PL;
        g.db.set_where(WHO, PROV);
        g
    }

    /// Checks `line` as a fresh wait for [`WHO`].
    fn check(g: &mut Game, line: &[u8]) -> Option<String> {
        let mut c = Command {
            who: WHO,
            ..Command::default()
        };
        assert!(crate::command::oly_parse(&g.db, &mut c, line));
        g.db.entity_mut(WHO).cmd = Some(Box::new(c));
        g.check_wait_conditions(WHO)
            .map(|m| String::from_utf8(m).unwrap())
    }

    #[test]
    fn conditions_and_their_messages() {
        let mut g = game();
        g.system.sysclock.day = 4;
        assert_eq!(check(&mut g, b"wait time 1"), None);
        assert_eq!(
            check(&mut g, b"wait time 0").as_deref(),
            Some("zero days have passed.")
        );
        assert_eq!(
            check(&mut g, b"wait day 4").as_deref(),
            Some("today is day 4.")
        );
        assert_eq!(
            check(&mut g, b"wait day 3").as_deref(),
            Some("today is day 4.")
        );
        assert_eq!(
            check(&mut g, b"wait top").as_deref(),
            Some("we are the stack leader")
        );
        assert_eq!(
            check(&mut g, b"wait owner time 3").as_deref(),
            Some("we are the first character here")
        );
        assert_eq!(
            check(&mut g, b"wait clear").as_deref(),
            Some("it is clear.")
        );
        assert_eq!(check(&mut g, b"wait rain"), None);
        assert_eq!(
            check(&mut g, b"wait loc 10101").as_deref(),
            Some("at Plain~[ab01].")
        );
        assert_eq!(
            check(&mut g, b"wait unit 5").as_deref(),
            Some("[5] does not exist.")
        );
        assert_eq!(
            check(&mut g, b"wait time").as_deref(),
            Some("Argument missing for 'time'.")
        );
        assert_eq!(
            check(&mut g, b"wait time 3 bogus").as_deref(),
            Some("Unknown condition 'bogus'.")
        );
        // The conditions read before the error stay parsed.
        assert_eq!(g.rp_command(WHO).unwrap().wait_parse.len(), 1);
    }

    #[test]
    fn not_inverts_only_the_next_condition() {
        let mut g = game();
        g.system.sysclock.day = 4;
        assert_eq!(check(&mut g, b"wait not day 3 time 9"), None);
        assert_eq!(
            check(&mut g, b"wait not day 9").as_deref(),
            Some("today is not day 9.")
        );
        assert_eq!(
            check(&mut g, b"wait not day 9 day 9"),
            Some("today is not day 9.".to_owned())
        );
        assert_eq!(check(&mut g, b"wait not top day 9"), None);
        assert_eq!(
            check(&mut g, b"wait not raining").as_deref(),
            Some("it is not raining.")
        );
    }

    #[test]
    fn inverted_weather_of_strength_two_still_reads_as_holding() {
        // `cond = not - cond` with a storm strength of 2 is -1: C reports
        // "it is not raining" while it rains.
        let mut g = game();
        g.db.alloc_box(80001, Kind::STORM, Subkind::RAIN);
        g.db.set_where(80001, PROV);
        g.db.entity_mut(80001).misc = Some(Box::default());
        g.db.entity_mut(80001).misc.as_mut().unwrap().storm_str = 2;
        assert_eq!(
            check(&mut g, b"wait rain").as_deref(),
            Some("it is raining.")
        );
        assert_eq!(
            check(&mut g, b"wait not rain").as_deref(),
            Some("it is not raining.")
        );
        g.db.entity_mut(80001).misc.as_mut().unwrap().storm_str = 1;
        assert_eq!(check(&mut g, b"wait not rain"), None);
    }

    #[test]
    fn flags_and_gold() {
        let mut g = game();
        g.db.alloc_box(ITEM_GOLD, Kind::ITEM, Subkind::default());
        g.db.set_name(WHO, b"Osswid");
        g.db.entity_mut(WHO).items.push(crate::entity::ItemEnt {
            item: ITEM_GOLD,
            qty: 1500,
        });
        assert_eq!(
            check(&mut g, b"wait gold 1000").as_deref(),
            Some("Osswid has 1,500~gold.")
        );
        assert_eq!(check(&mut g, b"wait flag go"), None);
        g.driver.flags.push(FlagEnt {
            who: WHO,
            flag: b"GO".to_vec(),
        });
        assert_eq!(
            check(&mut g, b"wait flag go").as_deref(),
            Some("Osswid~[1001] signaled 'GO'")
        );
        assert_eq!(check(&mut g, b"wait not flag go"), None);
    }
}
