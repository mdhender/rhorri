// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The command table and command-line parsing (`olympia/input.c`).

use crate::code::{GARRISON_MAGIC, scode};
use crate::cstr::{fuzzy_eq, i_str_eq, is_white};
use crate::entity::{Command, Db, Text};
use crate::loc::{garrison_here, subloc};

/// One command's table entry (`struct cmd_tbl_ent`).
///
/// The handlers are named by their C functions (`v_xxx`, `d_xxx`,
/// `i_xxx`); `None` is C's `NULL`. The command driver
/// ([`crate::input`]) dispatches on the name, and a name it has no port for
/// stops the run with "not yet implemented".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmdTblEnt {
    /// Who may issue the command.
    pub allow: &'static str,
    pub name: &'static str,
    /// The start routine, run when the command begins.
    pub start: Option<&'static str>,
    /// The finish routine, run when the command is done waiting, or every
    /// evening if `poll` is set.
    pub finish: Option<&'static str>,
    /// The interrupt routine, run when a running command is interrupted.
    pub interrupt: Option<&'static str>,
    /// Days the command takes; -1 for open-ended.
    pub time: i32,
    /// Whether the finish routine is called every day.
    pub poll: i32,
    /// Priority.
    pub pri: i32,
}

#[allow(clippy::too_many_arguments)]
const fn cmd(
    allow: &'static str,
    name: &'static str,
    start: Option<&'static str>,
    finish: Option<&'static str>,
    interrupt: Option<&'static str>,
    time: i32,
    poll: i32,
    pri: i32,
) -> CmdTblEnt {
    CmdTblEnt {
        allow,
        name,
        start,
        finish,
        interrupt,
        time,
        poll,
        pri,
    }
}

/// The command table (`olympia/glob.c:cmd_tbl`), 148 entries. Index 0 is
/// the empty entry; a command's `cmd` is its index.
pub const CMD_TBL: [CmdTblEnt; 148] = [
    cmd("", "", None, None, None, 0, 0, 3),
    cmd("cpr", "accept", Some("v_accept"), None, None, 0, 0, 0),
    cmd("cpr", "admit", Some("v_admit"), None, None, 0, 0, 0),
    cmd("cr", "attack", Some("v_attack"), None, None, 1, 0, 3),
    cmd("cr", "banner", Some("v_banner"), None, None, 0, 0, 1),
    cmd("cr", "behind", Some("v_behind"), None, None, 0, 0, 1),
    cmd(
        "c",
        "bind",
        Some("v_bind_storm"),
        Some("d_bind_storm"),
        None,
        7,
        0,
        3,
    ),
    cmd("c", "board", Some("v_board"), None, None, 0, 0, 2),
    cmd(
        "c",
        "breed",
        Some("v_breed"),
        Some("d_breed"),
        None,
        7,
        0,
        3,
    ),
    cmd(
        "c",
        "bribe",
        Some("v_bribe"),
        Some("d_bribe"),
        None,
        7,
        0,
        3,
    ),
    cmd(
        "c",
        "build",
        Some("v_build"),
        Some("d_build"),
        None,
        -1,
        1,
        3,
    ),
    cmd("c", "buy", Some("v_buy"), None, None, 0, 0, 1),
    cmd("c", "catch", Some("v_catch"), None, None, -1, 1, 3),
    cmd("c", "claim", Some("v_claim"), None, None, 0, 0, 1),
    cmd(
        "c",
        "collect",
        Some("v_collect"),
        Some("d_collect"),
        Some("i_collect"),
        -1,
        1,
        3,
    ),
    cmd("cr", "contact", Some("v_contact"), None, None, 0, 0, 0),
    cmd("m", "credit", Some("v_credit"), None, None, 0, 0, 0),
    cmd("c", "decree", Some("v_decree"), None, None, 0, 0, 0),
    cmd("cpr", "default", Some("v_att_clear"), None, None, 0, 0, 0),
    cmd("cpr", "defend", Some("v_defend"), None, None, 0, 0, 0),
    cmd("c", "die", Some("v_die"), None, None, 0, 0, 1),
    cmd("c", "discard", Some("v_discard"), None, None, 0, 0, 1),
    cmd("cr", "drop", Some("v_discard"), None, None, 0, 0, 1),
    cmd("m", "emote", Some("v_emote"), None, None, 0, 0, 1),
    cmd("cr", "execute", Some("v_execute"), None, None, 0, 0, 1),
    cmd(
        "c",
        "explore",
        Some("v_explore"),
        Some("d_explore"),
        None,
        7,
        0,
        3,
    ),
    cmd("c", "fee", Some("v_fee"), None, None, 0, 0, 1),
    cmd("c", "ferry", Some("v_ferry"), None, None, 0, 0, 1),
    cmd("c", "fish", Some("v_fish"), None, None, -1, 1, 3),
    cmd("cr", "flag", Some("v_flag"), None, None, 0, 0, 1),
    cmd("c", "fly", Some("v_fly"), Some("d_fly"), None, -1, 0, 2),
    cmd("c", "forget", Some("v_forget"), None, None, 0, 0, 1),
    cmd("c", "form", Some("v_form"), Some("d_form"), None, 7, 0, 3),
    cmd("cp", "format", Some("v_format"), None, None, 0, 0, 1),
    cmd("c", "garrison", Some("v_garrison"), None, None, 1, 0, 3),
    cmd("cr", "get", Some("v_get"), None, None, 0, 0, 1),
    cmd("cr", "give", Some("v_give"), None, None, 0, 0, 1),
    cmd("cr", "go", Some("v_move"), Some("d_move"), None, -1, 0, 2),
    cmd("c", "guard", Some("v_guard"), None, None, 0, 0, 1),
    cmd("c", "hide", Some("v_hide"), Some("d_hide"), None, 3, 0, 3),
    cmd("c", "honor", Some("v_honor"), None, None, 1, 0, 3),
    cmd("c", "honour", Some("v_honor"), None, None, 1, 0, 3),
    cmd("cpr", "hostile", Some("v_hostile"), None, None, 0, 0, 0),
    cmd(
        "c",
        "improve",
        Some("v_improve"),
        Some("d_improve"),
        None,
        -1,
        1,
        3,
    ),
    cmd("c", "incite", Some("v_incite"), None, None, 7, 0, 3),
    cmd(
        "c",
        "make",
        Some("v_make"),
        Some("d_make"),
        Some("i_make"),
        -1,
        1,
        3,
    ),
    cmd("c", "mallorn", Some("v_mallorn"), None, None, -1, 1, 3),
    cmd("cp", "message", Some("v_message"), None, None, 1, 0, 3),
    cmd("cr", "move", Some("v_move"), Some("d_move"), None, -1, 0, 2),
    cmd("cpr", "name", Some("v_name"), None, None, 0, 0, 1),
    cmd("cpr", "neutral", Some("v_neutral"), None, None, 0, 0, 0),
    cmd("cp", "notab", Some("v_notab"), None, None, 0, 0, 1),
    cmd("c", "oath", Some("v_oath"), None, None, 1, 0, 3),
    cmd("c", "opium", Some("v_opium"), None, None, -1, 1, 3),
    cmd("cr", "pay", Some("v_pay"), None, None, 0, 0, 1),
    cmd(
        "cr",
        "pillage",
        Some("v_pillage"),
        Some("d_pillage"),
        None,
        7,
        0,
        3,
    ),
    cmd("c", "pledge", Some("v_pledge"), None, None, 0, 0, 1),
    cmd("cr", "plugh", Some("v_plugh"), None, None, 0, 0, 3),
    cmd("c", "post", Some("v_post"), None, None, 1, 0, 3),
    cmd("cp", "press", Some("v_press"), None, None, 0, 0, 1),
    cmd("cr", "promote", Some("v_promote"), None, None, 0, 0, 1),
    cmd("cp", "public", Some("v_public"), None, None, 0, 0, 1),
    cmd("c", "quarry", Some("v_quarry"), None, None, -1, 1, 3),
    cmd(
        "c",
        "quest",
        Some("v_quest"),
        Some("d_quest"),
        None,
        7,
        0,
        3,
    ),
    cmd("p", "quit", Some("v_quit"), None, None, 0, 0, 1),
    cmd(
        "c",
        "raise",
        Some("v_raise"),
        Some("d_raise"),
        None,
        7,
        0,
        3,
    ),
    cmd(
        "c",
        "rally",
        Some("v_rally"),
        Some("d_rally"),
        None,
        7,
        0,
        3,
    ),
    cmd("cr", "raze", Some("v_raze"), Some("d_raze"), None, -1, 1, 3),
    cmd("cpr", "realname", Some("v_fullname"), None, None, 0, 0, 1),
    cmd("c", "reclaim", Some("v_reclaim"), None, None, 0, 0, 1),
    cmd("c", "recruit", Some("v_recruit"), None, None, -1, 1, 3),
    cmd(
        "c",
        "repair",
        Some("v_repair"),
        Some("d_repair"),
        Some("i_repair"),
        -1,
        1,
        3,
    ),
    cmd(
        "c",
        "research",
        Some("v_research"),
        Some("d_research"),
        None,
        7,
        0,
        3,
    ),
    cmd("cp", "rumor", Some("v_rumor"), None, None, 0, 0, 1),
    cmd(
        "c",
        "sail",
        Some("v_sail"),
        Some("d_sail"),
        Some("i_sail"),
        -1,
        0,
        4,
    ),
    cmd("c", "sell", Some("v_sell"), None, None, 0, 0, 1),
    cmd("cr", "seek", Some("v_seek"), Some("d_seek"), None, 7, 1, 3),
    cmd(
        "c",
        "sneak",
        Some("v_sneak"),
        Some("d_sneak"),
        None,
        3,
        0,
        3,
    ),
    cmd("cp", "split", Some("v_split"), None, None, 0, 0, 1),
    cmd("cr", "stack", Some("v_stack"), None, None, 0, 0, 1),
    cmd("c", "stone", Some("v_quarry"), None, None, -1, 1, 3),
    cmd(
        "c",
        "study",
        Some("v_study"),
        Some("d_study"),
        None,
        7,
        1,
        3,
    ),
    cmd("c", "surrender", Some("v_surrender"), None, None, 1, 0, 1),
    cmd("c", "swear", Some("v_swear"), None, None, 0, 0, 1),
    cmd("cr", "take", Some("v_get"), None, None, 0, 0, 1),
    cmd("cp", "times", Some("v_times"), None, None, 0, 0, 1),
    cmd(
        "c",
        "train",
        Some("v_make"),
        Some("d_make"),
        Some("i_make"),
        -1,
        1,
        3,
    ),
    cmd(
        "c",
        "trance",
        Some("v_trance"),
        Some("d_trance"),
        None,
        28,
        0,
        3,
    ),
    cmd(
        "cr",
        "terrorize",
        Some("v_terrorize"),
        Some("d_terrorize"),
        None,
        7,
        0,
        3,
    ),
    cmd(
        "c",
        "torture",
        Some("v_torture"),
        Some("d_torture"),
        None,
        7,
        0,
        3,
    ),
    cmd("c", "unload", Some("v_unload"), None, None, 0, 0, 3),
    cmd("c", "ungarrison", Some("v_ungarrison"), None, None, 1, 0, 3),
    cmd("cr", "unstack", Some("v_unstack"), None, None, 0, 0, 1),
    cmd(
        "c",
        "use",
        Some("v_use"),
        Some("d_use"),
        Some("i_use"),
        -1,
        1,
        3,
    ),
    cmd(
        "crm",
        "wait",
        Some("v_wait"),
        Some("d_wait"),
        Some("i_wait"),
        -1,
        1,
        1,
    ),
    cmd("c", "wood", Some("v_wood"), None, None, -1, 1, 3),
    cmd("cr", "xyzzy", Some("v_xyzzy"), None, None, 0, 0, 3),
    cmd("c", "yew", Some("v_yew"), None, None, -1, 1, 3),
    cmd("cr", "north", Some("v_north"), None, None, -1, 0, 2),
    cmd("cr", "n", Some("v_north"), None, None, -1, 0, 2),
    cmd("cr", "s", Some("v_south"), None, None, -1, 0, 2),
    cmd("cr", "south", Some("v_south"), None, None, -1, 0, 2),
    cmd("cr", "east", Some("v_east"), None, None, -1, 0, 2),
    cmd("cr", "e", Some("v_east"), None, None, -1, 0, 2),
    cmd("cr", "west", Some("v_west"), None, None, -1, 0, 2),
    cmd("cr", "w", Some("v_west"), None, None, -1, 0, 2),
    cmd("cr", "enter", Some("v_enter"), None, None, -1, 0, 2),
    cmd("cr", "exit", Some("v_exit"), None, None, -1, 0, 2),
    cmd("cr", "in", Some("v_enter"), None, None, -1, 0, 2),
    cmd("cr", "out", Some("v_exit"), None, None, -1, 0, 2),
    cmd("", "begin", None, None, None, 0, 0, 0),
    cmd("", "unit", None, None, None, 0, 0, 0),
    cmd("", "email", None, None, None, 0, 0, 0),
    cmd("", "vis_email", None, None, None, 0, 0, 0),
    cmd("", "end", None, None, None, 0, 0, 0),
    cmd("", "flush", None, None, None, 0, 0, 0),
    cmd("", "lore", None, None, None, 0, 0, 0),
    cmd("", "passwd", None, None, None, 0, 0, 0),
    cmd("", "password", None, None, None, 0, 0, 0),
    cmd("", "players", None, None, None, 0, 0, 0),
    cmd("", "resend", None, None, None, 0, 0, 0),
    cmd("cpr", "stop", Some("v_stop"), None, None, 0, 0, 0),
    cmd("i", "look", Some("v_look"), None, None, 0, 0, 1),
    cmd("i", "l", Some("v_look"), None, None, 0, 0, 1),
    cmd("i", "ct", Some("v_ct"), None, None, 0, 0, 1),
    cmd("i", "be", Some("v_be"), None, None, 0, 0, 1),
    cmd("i", "additem", Some("v_add_item"), None, None, 0, 0, 1),
    cmd("i", "subitem", Some("v_sub_item"), None, None, 0, 0, 1),
    cmd("i", "h", Some("v_listcmds"), None, None, 0, 0, 1),
    cmd("i", "dump", Some("v_dump"), None, None, 0, 0, 1),
    cmd("i", "i", Some("v_invent"), None, None, 0, 0, 1),
    cmd("i", "fix", Some("v_fix"), None, None, 0, 0, 1),
    cmd("i", "fix2", Some("v_fix2"), None, None, 0, 0, 1),
    cmd("i", "kill", Some("v_kill"), None, None, 0, 0, 1),
    cmd("i", "los", Some("v_los"), None, None, 0, 0, 1),
    cmd("m", "relore", Some("v_relore"), None, None, 0, 0, 1),
    cmd("i", "sk", Some("v_skills"), None, None, 0, 0, 1),
    cmd("i", "know", Some("v_know"), None, None, 0, 0, 1),
    cmd("i", "seed", Some("v_seed"), None, None, 0, 0, 1),
    cmd("i", "seedmarket", Some("v_seedmarket"), None, None, 0, 0, 1),
    cmd("i", "sheet", Some("v_lore"), None, None, 0, 0, 1),
    cmd("i", "poof", Some("v_poof"), None, None, 0, 0, 1),
    cmd("i", "postproc", Some("v_postproc"), None, None, 0, 0, 1),
    cmd("i", "save", Some("v_save"), None, None, 0, 0, 1),
    cmd("i", "seeall", Some("v_see_all"), None, None, 0, 0, 1),
    cmd("i", "tp", Some("v_take_pris"), None, None, 0, 0, 1),
    cmd("i", "makeloc", Some("v_makeloc"), None, None, 0, 0, 1),
    cmd("i", "remail", Some("v_remail"), None, None, 0, 0, 1),
];

/// Looks a command up by name, exactly and then fuzzily (`find_command()`).
///
/// Returns the table index and whether the match was fuzzy.
#[must_use]
pub fn find_command(s: &[u8]) -> Option<(i32, bool)> {
    if s.is_empty() {
        return None;
    }
    let commands = CMD_TBL.iter().enumerate().skip(1);
    if let Some((i, _)) = commands
        .clone()
        .find(|(_, c)| i_str_eq(c.name.as_bytes(), s))
    {
        return Some((i as i32, false));
    }
    commands
        .clone()
        .find(|(_, c)| fuzzy_eq(c.name.as_bytes(), s))
        .map(|(i, _)| (i as i32, true))
}

/// Splits a command line into arguments (`parse_line()`).
///
/// Arguments are separated by white space; `"..."` and `'...'` quote an
/// argument, and each argument is trimmed of white space.
#[must_use]
pub fn parse_line(s: &[u8]) -> Vec<Text> {
    let mut args = Vec::new();
    let mut i = 0;
    while i < s.len() {
        while i < s.len() && is_white(s[i]) {
            i += 1;
        }
        if i >= s.len() {
            break;
        }
        let (start, end) = match s[i] {
            q @ (b'"' | b'\'') => {
                let start = i + 1;
                let end = s[start..]
                    .iter()
                    .position(|&c| c == q)
                    .map_or(s.len(), |p| start + p);
                (start, end)
            }
            _ => {
                let start = i;
                let end = s[start..]
                    .iter()
                    .position(|&c| is_white(c))
                    .map_or(s.len(), |p| start + p);
                (start, end)
            }
        };
        // The terminator, if any, is overwritten and skipped.
        i = if end < s.len() { end + 1 } else { end };

        let mut arg = &s[start..end];
        while arg.first().is_some_and(|&c| is_white(c)) {
            arg = &arg[1..];
        }
        while arg.last().is_some_and(|&c| is_white(c)) {
            arg = &arg[..arg.len() - 1];
        }
        args.push(arg.to_vec());
    }
    args
}

/// Cuts a line at its first `#` (`remove_comment()`).
///
/// The quote handling in C only steps over the quote characters
/// themselves, so a `#` inside quotes still starts a comment.
fn remove_comment(s: &mut Text) {
    let mut i = 0;
    while i < s.len() {
        while s.get(i) == Some(&b'"') {
            i += 1;
        }
        while s.get(i) == Some(&b'\'') {
            i += 1;
        }
        if s.get(i) == Some(&b'#') {
            s.truncate(i);
            break;
        }
        i += 1;
    }
}

/// Clears the high bit of every byte, then turns controls into spaces
/// (`remove_ctrl_chars()`).
fn remove_ctrl_chars(s: &mut [u8]) {
    for c in s {
        *c &= 0x7f;
        if *c < 32 {
            *c = b' ';
        }
    }
}

/// Parses a command line into `c` (`oly_parse_cmd()`): stores the cleaned
/// line, its conditional prefix and its arguments, and looks the command up.
///
/// Returns false if the first word is not a command; `c.cmd` is then 0.
pub fn oly_parse_cmd(c: &mut Command, s: &[u8]) -> bool {
    c.cmd = 0;
    c.fuzzy = false;
    c.use_skill = 0;

    let start = s.iter().position(|&ch| !is_white(ch)).unwrap_or(s.len());
    let mut line = s[start..].to_vec();
    remove_comment(&mut line);
    remove_ctrl_chars(&mut line);
    c.line = Some(line.clone());

    let rest = match line.first() {
        Some(b'&') => {
            c.conditional = 1;
            &line[1..]
        }
        Some(b'?') => {
            c.conditional = 2;
            &line[1..]
        }
        _ => {
            c.conditional = 0;
            &line[..]
        }
    };
    c.parsed_line = Some(rest.to_vec());
    c.parse = parse_line(rest);

    if let Some(first) = c.parse.first() {
        let Some((i, fuzzy)) = find_command(first) else {
            return false;
        };
        c.cmd = i;
        c.fuzzy = fuzzy;
    }
    true
}

/// Reads a command argument as an entity number (`parse_arg()`): a number
/// or an entity code, else 0. `garrison` (or a near miss) is the garrison
/// where `who` is, or `garrison_magic` if there is none.
#[must_use]
pub fn parse_arg(db: &Db, who: i32, s: &[u8]) -> i32 {
    let mut n = scode(s).max(0);
    if n == 0 && who != 0 && subloc(db, who) != 0 && fuzzy_eq(s, b"garrison") {
        n = garrison_here(db, subloc(db, who));
        if n == 0 {
            n = GARRISON_MAGIC;
        }
    }
    n
}

/// Parses a command line into `c` and reads its first eight arguments as
/// entity numbers (`oly_parse()`).
///
/// Returns false if the first word is not a command.
pub fn oly_parse(db: &Db, c: &mut Command, s: &[u8]) -> bool {
    c.args = [0; 8];
    if !oly_parse_cmd(c, s) {
        return false;
    }
    for (arg, word) in c.args.iter_mut().zip(c.parse.iter().skip(1)) {
        *arg = parse_arg(db, c.who, word);
    }
    true
}

/// `numargs()`: the arguments after the command word.
#[must_use]
pub fn numargs(c: &Command) -> i32 {
    c.parse.len() as i32 - 1
}

/// `olympia/input.c:cmd_shift`: drops the first argument, shifting the
/// rest (and their entity numbers) down one, and reads the new eighth.
pub fn cmd_shift(db: &Db, c: &mut Command) {
    if c.parse.len() > 1 {
        c.parse.remove(1);
    }
    c.args.copy_within(1.., 0);
    c.args[7] = if numargs(c) >= 8 {
        parse_arg(db, c.who, &c.parse[8])
    } else {
        0
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_command_exact_and_fuzzy() {
        assert_eq!(find_command(b"accept"), Some((1, false)));
        assert_eq!(find_command(b"ACCEPT"), Some((1, false)));
        assert_eq!(find_command(b"acept"), Some((1, true)));
        assert_eq!(find_command(b"zzzzzz"), None);
        assert_eq!(find_command(b""), None);
    }

    #[test]
    fn parse_line_quotes_and_trims() {
        assert_eq!(
            parse_line(b"move  \"a b\" 'c d'  e"),
            [&b"move"[..], b"a b", b"c d", b"e"]
        );
        assert_eq!(parse_line(b"name \" x "), [&b"name"[..], b"x"]);
    }

    #[test]
    fn remove_comment_ignores_quotes_like_c() {
        let mut s = b"say \"a # b\"".to_vec();
        remove_comment(&mut s);
        assert_eq!(s, b"say \"a ");
    }

    #[test]
    fn oly_parse_cmd_sets_line_and_conditional() {
        let mut c = Command::default();
        assert!(oly_parse_cmd(&mut c, b"  &move n # go\x01"));
        assert_eq!(c.line.as_deref(), Some(&b"&move n "[..]));
        assert_eq!(c.conditional, 1);
        assert_eq!(c.parse, [&b"move"[..], b"n"]);
        assert!(c.cmd > 0);

        assert!(!oly_parse_cmd(&mut c, b"bogus"));
        assert_eq!(c.cmd, 0);
    }
}
