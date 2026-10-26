// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The command table and command-line parsing (`olympia/input.c`).

use crate::cstr::{fuzzy_eq, i_str_eq, is_white};
use crate::entity::{Command, Text};

/// One command's table entry (`struct cmd_tbl_ent`), without its handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmdTblEnt {
    /// Who may issue the command.
    pub allow: &'static str,
    pub name: &'static str,
    /// Days the command takes; -1 for open-ended.
    pub time: i32,
    /// Whether the finish routine is called every day.
    pub poll: i32,
    /// Priority.
    pub pri: i32,
}

const fn cmd(allow: &'static str, name: &'static str, time: i32, poll: i32, pri: i32) -> CmdTblEnt {
    CmdTblEnt {
        allow,
        name,
        time,
        poll,
        pri,
    }
}

/// The command table (`olympia/glob.c:cmd_tbl`), 148 entries. Index 0 is
/// the empty entry; a command's `cmd` is its index.
pub const CMD_TBL: [CmdTblEnt; 148] = [
    cmd("", "", 0, 0, 3),
    cmd("cpr", "accept", 0, 0, 0),
    cmd("cpr", "admit", 0, 0, 0),
    cmd("cr", "attack", 1, 0, 3),
    cmd("cr", "banner", 0, 0, 1),
    cmd("cr", "behind", 0, 0, 1),
    cmd("c", "bind", 7, 0, 3),
    cmd("c", "board", 0, 0, 2),
    cmd("c", "breed", 7, 0, 3),
    cmd("c", "bribe", 7, 0, 3),
    cmd("c", "build", -1, 1, 3),
    cmd("c", "buy", 0, 0, 1),
    cmd("c", "catch", -1, 1, 3),
    cmd("c", "claim", 0, 0, 1),
    cmd("c", "collect", -1, 1, 3),
    cmd("cr", "contact", 0, 0, 0),
    cmd("m", "credit", 0, 0, 0),
    cmd("c", "decree", 0, 0, 0),
    cmd("cpr", "default", 0, 0, 0),
    cmd("cpr", "defend", 0, 0, 0),
    cmd("c", "die", 0, 0, 1),
    cmd("c", "discard", 0, 0, 1),
    cmd("cr", "drop", 0, 0, 1),
    cmd("m", "emote", 0, 0, 1),
    cmd("cr", "execute", 0, 0, 1),
    cmd("c", "explore", 7, 0, 3),
    cmd("c", "fee", 0, 0, 1),
    cmd("c", "ferry", 0, 0, 1),
    cmd("c", "fish", -1, 1, 3),
    cmd("cr", "flag", 0, 0, 1),
    cmd("c", "fly", -1, 0, 2),
    cmd("c", "forget", 0, 0, 1),
    cmd("c", "form", 7, 0, 3),
    cmd("cp", "format", 0, 0, 1),
    cmd("c", "garrison", 1, 0, 3),
    cmd("cr", "get", 0, 0, 1),
    cmd("cr", "give", 0, 0, 1),
    cmd("cr", "go", -1, 0, 2),
    cmd("c", "guard", 0, 0, 1),
    cmd("c", "hide", 3, 0, 3),
    cmd("c", "honor", 1, 0, 3),
    cmd("c", "honour", 1, 0, 3),
    cmd("cpr", "hostile", 0, 0, 0),
    cmd("c", "improve", -1, 1, 3),
    cmd("c", "incite", 7, 0, 3),
    cmd("c", "make", -1, 1, 3),
    cmd("c", "mallorn", -1, 1, 3),
    cmd("cp", "message", 1, 0, 3),
    cmd("cr", "move", -1, 0, 2),
    cmd("cpr", "name", 0, 0, 1),
    cmd("cpr", "neutral", 0, 0, 0),
    cmd("cp", "notab", 0, 0, 1),
    cmd("c", "oath", 1, 0, 3),
    cmd("c", "opium", -1, 1, 3),
    cmd("cr", "pay", 0, 0, 1),
    cmd("cr", "pillage", 7, 0, 3),
    cmd("c", "pledge", 0, 0, 1),
    cmd("cr", "plugh", 0, 0, 3),
    cmd("c", "post", 1, 0, 3),
    cmd("cp", "press", 0, 0, 1),
    cmd("cr", "promote", 0, 0, 1),
    cmd("cp", "public", 0, 0, 1),
    cmd("c", "quarry", -1, 1, 3),
    cmd("c", "quest", 7, 0, 3),
    cmd("p", "quit", 0, 0, 1),
    cmd("c", "raise", 7, 0, 3),
    cmd("c", "rally", 7, 0, 3),
    cmd("cr", "raze", -1, 1, 3),
    cmd("cpr", "realname", 0, 0, 1),
    cmd("c", "reclaim", 0, 0, 1),
    cmd("c", "recruit", -1, 1, 3),
    cmd("c", "repair", -1, 1, 3),
    cmd("c", "research", 7, 0, 3),
    cmd("cp", "rumor", 0, 0, 1),
    cmd("c", "sail", -1, 0, 4),
    cmd("c", "sell", 0, 0, 1),
    cmd("cr", "seek", 7, 1, 3),
    cmd("c", "sneak", 3, 0, 3),
    cmd("cp", "split", 0, 0, 1),
    cmd("cr", "stack", 0, 0, 1),
    cmd("c", "stone", -1, 1, 3),
    cmd("c", "study", 7, 1, 3),
    cmd("c", "surrender", 1, 0, 1),
    cmd("c", "swear", 0, 0, 1),
    cmd("cr", "take", 0, 0, 1),
    cmd("cp", "times", 0, 0, 1),
    cmd("c", "train", -1, 1, 3),
    cmd("c", "trance", 28, 0, 3),
    cmd("cr", "terrorize", 7, 0, 3),
    cmd("c", "torture", 7, 0, 3),
    cmd("c", "unload", 0, 0, 3),
    cmd("c", "ungarrison", 1, 0, 3),
    cmd("cr", "unstack", 0, 0, 1),
    cmd("c", "use", -1, 1, 3),
    cmd("crm", "wait", -1, 1, 1),
    cmd("c", "wood", -1, 1, 3),
    cmd("cr", "xyzzy", 0, 0, 3),
    cmd("c", "yew", -1, 1, 3),
    cmd("cr", "north", -1, 0, 2),
    cmd("cr", "n", -1, 0, 2),
    cmd("cr", "s", -1, 0, 2),
    cmd("cr", "south", -1, 0, 2),
    cmd("cr", "east", -1, 0, 2),
    cmd("cr", "e", -1, 0, 2),
    cmd("cr", "west", -1, 0, 2),
    cmd("cr", "w", -1, 0, 2),
    cmd("cr", "enter", -1, 0, 2),
    cmd("cr", "exit", -1, 0, 2),
    cmd("cr", "in", -1, 0, 2),
    cmd("cr", "out", -1, 0, 2),
    cmd("", "begin", 0, 0, 0),
    cmd("", "unit", 0, 0, 0),
    cmd("", "email", 0, 0, 0),
    cmd("", "vis_email", 0, 0, 0),
    cmd("", "end", 0, 0, 0),
    cmd("", "flush", 0, 0, 0),
    cmd("", "lore", 0, 0, 0),
    cmd("", "passwd", 0, 0, 0),
    cmd("", "password", 0, 0, 0),
    cmd("", "players", 0, 0, 0),
    cmd("", "resend", 0, 0, 0),
    cmd("cpr", "stop", 0, 0, 0),
    cmd("i", "look", 0, 0, 1),
    cmd("i", "l", 0, 0, 1),
    cmd("i", "ct", 0, 0, 1),
    cmd("i", "be", 0, 0, 1),
    cmd("i", "additem", 0, 0, 1),
    cmd("i", "subitem", 0, 0, 1),
    cmd("i", "h", 0, 0, 1),
    cmd("i", "dump", 0, 0, 1),
    cmd("i", "i", 0, 0, 1),
    cmd("i", "fix", 0, 0, 1),
    cmd("i", "fix2", 0, 0, 1),
    cmd("i", "kill", 0, 0, 1),
    cmd("i", "los", 0, 0, 1),
    cmd("m", "relore", 0, 0, 1),
    cmd("i", "sk", 0, 0, 1),
    cmd("i", "know", 0, 0, 1),
    cmd("i", "seed", 0, 0, 1),
    cmd("i", "seedmarket", 0, 0, 1),
    cmd("i", "sheet", 0, 0, 1),
    cmd("i", "poof", 0, 0, 1),
    cmd("i", "postproc", 0, 0, 1),
    cmd("i", "save", 0, 0, 1),
    cmd("i", "seeall", 0, 0, 1),
    cmd("i", "tp", 0, 0, 1),
    cmd("i", "makeloc", 0, 0, 1),
    cmd("i", "remail", 0, 0, 1),
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
