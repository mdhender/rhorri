// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Database integrity checks and minor repairs (`olympia/check.c`).
//!
//! `check_db` runs at every load and again before every save. Each repair
//! and each warning is reported on standard error, in C's wording.

use crate::code::{GARRISON_MAGIC, box_code, box_name, eprint_bytes, just_name};
use crate::entity::{Db, Kind, MAX_BOXES, SUB_MAX, Subkind, T_MAX};
use crate::game::{Game, player, stage};
use crate::loc::{LOC_REGION, loc_depth};
use crate::swear::LOY_UNSWORN;
use crate::turn::Phase;

/// `indep_player`: the faction of independent units.
pub const INDEP_PLAYER: i32 = 100;
/// `gm_player`: the Fates.
pub const GM_PLAYER: i32 = 200;
/// `skill_player`: the faction the skill list is reported to.
pub const SKILL_PLAYER: i32 = 202;
/// `eat_pl`: the order scanner.
pub const EAT_PL: i32 = 203;
/// `npc_pl`: the faction of sublocation monsters.
pub const NPC_PL: i32 = 206;
/// `garr_pl`: the faction of garrison units.
pub const GARR_PL: i32 = 207;

/// `sub_pl_system`.
const SUB_PL_SYSTEM: Subkind = Subkind(48);
/// `sub_pl_npc`.
const SUB_PL_NPC: Subkind = Subkind(52);
/// `sub_pl_silent`.
const SUB_PL_SILENT: Subkind = Subkind(62);

/// `STATE_RUN`: a command that is executing.
const STATE_RUN: i8 = 2;

/// Formats a message with `format!`, which may splice in byte-string names
/// as `{}` of [`Bytes`], and writes it to standard error.
macro_rules! warn {
    ($($arg:tt)*) => {
        eprint_bytes(&crate::check::bytes_format(format_args!($($arg)*)))
    };
}

/// Byte strings spliced into a [`warn!`] message. Names in the data files
/// need not be UTF-8, and C prints them as they are, so they travel through
/// the formatter as placeholders and are swapped back in as bytes.
struct Bytes<'a>(&'a [u8]);

thread_local! {
    static SPLICED: std::cell::RefCell<Vec<Vec<u8>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Placeholder byte for a spliced byte string; never in a C format string.
const SPLICE: char = '\u{1}';

impl std::fmt::Display for Bytes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        SPLICED.with(|s| s.borrow_mut().push(self.0.to_vec()));
        write!(f, "{SPLICE}")
    }
}

/// Renders `args`, replacing each placeholder with its byte string.
fn bytes_format(args: std::fmt::Arguments<'_>) -> Vec<u8> {
    SPLICED.with(|s| s.borrow_mut().clear());
    let text = std::fmt::format(args);
    let spliced = SPLICED.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let mut parts = spliced.into_iter();
    let mut out = Vec::with_capacity(text.len());
    for c in text.bytes() {
        if c == SPLICE as u8 {
            out.extend(parts.next().expect("a byte string per placeholder"));
        } else {
            out.push(c);
        }
    }
    out
}

impl Game {
    /// Checks the database and repairs what it can (`check_db()`):
    /// here lists against `where`, faction unit lists against lords, the
    /// system players, skill offerings, unique item holders, movement and
    /// prisoner flags.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`](crate::game::NotYetImplemented) if a
    /// unit sworn to no faction must be sworn to the independent player:
    /// [`set_lord`](Game::set_lord) needs the command driver for that.
    pub fn check_db(&mut self) -> Phase {
        stage("check_db()");

        check_glob();
        check_here(&mut self.db);
        check_swear(&mut self.db);
        self.check_indep()?;
        for &(pl, sk, routine, label, name) in &SYSTEM_PLAYERS {
            check_system_player(&mut self.db, pl, sk, routine, label, name);
        }
        check_nowhere(&self.db);
        check_skills(&mut self.db);
        check_item_counts(&mut self.db);
        check_loc_name_lengths(&self.db);
        check_moving(&mut self.db);
        check_prisoner(&mut self.db);

        if self.db.get(GARRISON_MAGIC).is_some() {
            warn!(
                "\twarning: {} should not be allocated,\n",
                Bytes(&box_name(&self.db, GARRISON_MAGIC))
            );
            warn!("\t\treserved for garrison_magic\n");
        }
        Ok(())
    }

    /// `check_indep()`: makes sure the independent player exists, and swears
    /// every unit with no faction to it.
    fn check_indep(&mut self) -> Phase {
        let db = &mut self.db;
        if db.get(INDEP_PLAYER).is_none() {
            warn!("\tcheck_indep: creating independent player [{INDEP_PLAYER}]\n");
            db.alloc_box(INDEP_PLAYER, Kind::PLAYER, SUB_PL_NPC);
        }
        assert_eq!(db.kind(INDEP_PLAYER), Kind::PLAYER);
        if db.name(INDEP_PLAYER).is_empty() {
            db.set_name(INDEP_PLAYER, b"Independent player");
        }

        let mut next = self.db.kind_first(Kind::CHAR);
        while next > 0 {
            let i = next;
            next = self.db.kind_next(i);
            if player(&self.db, i) == 0 {
                warn!(
                    "\tcheck_indep: swearing unit [{i}] to {}\n",
                    Bytes(&box_name(&self.db, INDEP_PLAYER))
                );
                self.set_lord(i, INDEP_PLAYER, LOY_UNSWORN, 0)?;
            }
        }
        Ok(())
    }
}

/// `check_glob()`: the kind and subkind name tables cover every kind and
/// subkind.
fn check_glob() {
    const _: () = assert!(T_MAX == 14 && SUB_MAX == 74);
}

/// The valid boxes in ascending order (`loop_boxes`).
fn valid_boxes(db: &Db) -> impl Iterator<Item = i32> + '_ {
    (1..MAX_BOXES).filter(|&i| db.valid_box(i))
}

/// `in_here_list()`.
fn in_here_list(db: &Db, loc: i32, who: i32) -> bool {
    db.entity(loc).loc_info.here_list.contains(who)
}

/// `check_here()`: a box that says it is somewhere is added to that
/// location's here list, and a box in a here list that says it is elsewhere
/// is taken out. Where a box says it is wins.
fn check_here(db: &mut Db) {
    for i in valid_boxes(db).collect::<Vec<_>>() {
        let where_ = db.loc(i);
        if where_ > 0 && !in_here_list(db, where_, i) {
            warn!("\tcheck_here: adding [{i}] to here list of [{where_}]\n");
            // add_to_here_list()
            db.entity_mut(where_).loc_info.here_list.append(i);
        }
    }

    for i in valid_boxes(db).collect::<Vec<_>>() {
        // loop_here() walks a copy of the list.
        let here = db.entity(i).loc_info.here_list.as_slice().to_vec();
        for j in here {
            if db.loc(j) != i {
                warn!("\tcheck_here: removing [{j}] from here list of [{i}]\n");
                // remove_from_here_list()
                assert!(in_here_list(db, i, j), "{j} not in here list of {i}");
                db.entity_mut(i).loc_info.here_list.rem_value(j);
            }
        }
    }
}

/// `check_swear()`: a character sworn into a faction is added to the
/// faction's unit list, and a unit in a faction's list that belongs to
/// another is taken out. The unit's lord wins.
fn check_swear(db: &mut Db) {
    for i in db.boxes_of_kind(Kind::CHAR) {
        let over = player(db, i);
        // is_unit() allocates the player struct, through p_player().
        if over > 0 && !db.p_player(over).units.contains(i) {
            warn!("\tcheck_swear: adding [{i}] to player [{over}]\n");
            let units = &mut db.p_player(over).units;
            units.append(i);
            units.sort();
        }
    }

    for i in db.boxes_of_kind(Kind::PLAYER) {
        // loop_units() walks a copy of the list.
        let units = db
            .entity(i)
            .player
            .as_ref()
            .map(|p| p.units.as_slice().to_vec())
            .unwrap_or_default();
        for j in units {
            if player(db, j) != i {
                warn!("\tcheck_swear: removing [{j}] from player list of [{i}]\n");
                db.p_player(i).units.rem_value(j);
            }
        }
    }
}

/// The system players `check_db` makes sure of after the independent
/// player: number, subkind, the C routine, how its message names the
/// player, and the name it is given if it has none.
const SYSTEM_PLAYERS: [(i32, Subkind, &str, &str, &str); 5] = [
    (GM_PLAYER, SUB_PL_SYSTEM, "check_gm", "gm", "Gamemaster"),
    (
        SKILL_PLAYER,
        SUB_PL_SYSTEM,
        "check_skill_player",
        "skill",
        "Skill list",
    ),
    (
        EAT_PL,
        SUB_PL_SYSTEM,
        "check_eat_player",
        "eat",
        "Order eater",
    ),
    (
        NPC_PL,
        SUB_PL_SILENT,
        "check_npc_player",
        "npc",
        "NPC control",
    ),
    (
        GARR_PL,
        SUB_PL_SILENT,
        "check_garr_player",
        "garrison",
        "Garrison units",
    ),
];

/// `check_gm()`, `check_skill_player()`, `check_eat_player()`,
/// `check_npc_player()` and `check_garr_player()`: makes sure system player
/// `pl` exists, and has a name.
fn check_system_player(db: &mut Db, pl: i32, sk: Subkind, routine: &str, label: &str, name: &str) {
    if db.get(pl).is_none() {
        warn!("\t{routine}: creating {label} player [{pl}]\n");
        db.alloc_box(pl, Kind::PLAYER, sk);
    }
    assert_eq!(db.kind(pl), Kind::PLAYER);
    if db.name(pl).is_empty() {
        db.set_name(pl, name.as_bytes());
    }
}

/// `check_nowhere()`: warns about characters, and locations below the
/// region level, that are nowhere.
fn check_nowhere(db: &Db) {
    for i in db.boxes_of_kind(Kind::CHAR) {
        if db.loc(i) == 0 {
            warn!("\twarning: unit {} is nowhere\n", Bytes(&box_code(i)));
        }
    }

    // loop_loc_or_ship: the ships, then the locations.
    let ships = db.boxes_of_kind(Kind::SHIP);
    for i in ships.into_iter().chain(db.boxes_of_kind(Kind::LOC)) {
        if loc_depth(db, i) > LOC_REGION && db.loc(i) == 0 {
            warn!("\twarning: loc {} is nowhere\n", Bytes(&box_code(i)));
        }
    }
}

/// `req_skill()`.
fn req_skill(db: &Db, n: i32) -> i32 {
    db.entity(n).skill.as_ref().map_or(0, |s| s.required_skill)
}

/// `olympia/use.c:skill_school`: the top-level skill a skill descends from.
///
/// # Panics
///
/// Panics on a loop of required skills, as C asserts.
fn skill_school(db: &Db, mut sk: i32) -> i32 {
    for _ in 0..1000 {
        let n = req_skill(db, sk);
        if n == 0 {
            return sk;
        }
        sk = n;
    }
    panic!("skill_school: loop of required skills");
}

/// `check_skills()`: warns about orphaned subskills, skills that take no
/// time to learn, skills offered twice or by a school they aren't in, and
/// subskills no school offers. Leaves in each skill's `temp` the school
/// that offers it.
fn check_skills(db: &mut Db) {
    let skills = db.boxes_of_kind(Kind::SKILL);

    for &sk in &skills {
        if sk >= 9000 && skill_school(db, sk) == sk {
            warn!("\twarning: orphaned subskill {}\n", Bytes(&box_code(sk)));
        }
        db.entity_mut(sk).temp = 0;
    }

    for &sk in &skills {
        // learn_time()
        let learn_time = db.entity(sk).skill.as_ref().map_or(0, |s| s.time_to_learn);
        if learn_time == 0 {
            warn!(
                "\twarning: learn time of {} is 0\n",
                Bytes(&box_name(db, sk))
            );
        }
        let Some(p) = db.entity(sk).skill.as_ref() else {
            continue;
        };
        let lists = [
            p.offered.as_slice().to_vec(),
            p.research.as_slice().to_vec(),
        ];
        for off in lists.into_iter().flatten() {
            let temp = db.entity(off).temp;
            if temp != 0 {
                warn!(
                    "\twarning: both {} and {} offer skill {off}\n",
                    Bytes(&box_name(db, sk)),
                    Bytes(&box_name(db, temp))
                );
            } else {
                db.entity_mut(off).temp = sk;
            }

            let school = skill_school(db, off);
            if school != sk {
                warn!(
                    "\twarning: {} offers {off}, but {off} is in school {school}\n",
                    Bytes(&box_name(db, sk))
                );
            }
        }
    }

    for &sk in &skills {
        if skill_school(db, sk) == sk {
            continue;
        }
        if db.entity(sk).temp == 0 {
            warn!(
                "\twarning: non-offered skill {}\n",
                Bytes(&box_name(db, sk))
            );
        }
    }
}

/// `item_unique()`: who holds a unique item, or 0 for an ordinary one.
fn item_unique(db: &Db, n: i32) -> i32 {
    db.entity(n).item.as_ref().map_or(0, |i| i.who_has)
}

/// `check_item_counts()`: every unique item is held once, by the box its
/// `who_has` names. A holder that disagrees is corrected; the rest are
/// warnings. Leaves each item's total held in its `temp`.
fn check_item_counts(db: &mut Db) {
    // clear_temps(T_item)
    for i in db.boxes_of_kind(Kind::ITEM) {
        db.entity_mut(i).temp = 0;
    }

    for i in valid_boxes(db).collect::<Vec<_>>() {
        // loop_inv() walks a copy, skipping invalid items and empty slots.
        let inv = db.entity(i).items.clone();
        for e in inv {
            if !(db.valid_box(e.item) && e.qty > 0) {
                continue;
            }
            if db.kind(e.item) != Kind::ITEM {
                warn!(
                    "\t{} has non-item {}\n",
                    Bytes(&box_name(db, i)),
                    Bytes(&box_name(db, e.item))
                );
                continue;
            }

            let unique = item_unique(db, e.item);
            if unique == 0 {
                continue;
            }
            if unique != i {
                warn!(
                    "\tunique item {}: whohas={}, actual={}\n",
                    Bytes(&box_name(db, e.item)),
                    Bytes(&box_name(db, unique)),
                    Bytes(&box_name(db, i))
                );
                db.p_item(e.item).who_has = i;
            }
            if e.qty != 1 {
                warn!(
                    "\t{} has qty {} of unique item {}\n",
                    Bytes(&box_name(db, i)),
                    e.qty,
                    Bytes(&box_name(db, e.item))
                );
            }
            db.entity_mut(e.item).temp += e.qty;
        }
    }

    for i in db.boxes_of_kind(Kind::ITEM) {
        if item_unique(db, i) != 0 && db.entity(i).temp != 1 {
            warn!(
                "\tunique item {} count {}\n",
                Bytes(&box_name(db, i)),
                db.entity(i).temp
            );
        }
    }
}

/// `check_loc_name_lengths()`: warns about location names over 25 bytes.
fn check_loc_name_lengths(db: &Db) {
    for i in db.boxes_of_kind(Kind::LOC) {
        if just_name(db, i).len() > 25 {
            warn!("\twarning: {} name too long\n", Bytes(&box_name(db, i)));
        }
    }
}

/// `char_moving()`: the day a character's stack started moving, or 0.
fn char_moving(db: &Db, n: i32) -> i32 {
    db.entity(n).char_.as_ref().map_or(0, |c| c.moving)
}

/// `olympia/stack.c:stack_parent`: the character `who` is stacked under,
/// or 0.
pub fn stack_parent(db: &Db, who: i32) -> i32 {
    let n = db.loc(who);
    if db.kind(n) == Kind::CHAR { n } else { 0 }
}

/// `olympia/stack.c:stack_leader`: the character at the top of `who`'s
/// stack.
///
/// # Panics
///
/// Panics if `who` isn't a character, or on a stack 1000 deep, as C
/// asserts.
#[must_use]
pub fn stack_leader(db: &Db, mut who: i32) -> i32 {
    assert_eq!(
        db.kind(who),
        Kind::CHAR,
        "stack_leader: {who} is no character"
    );
    let mut n = who;
    let mut count = 0;
    while db.kind(n) == Kind::CHAR {
        who = n;
        n = stack_parent(db, n);
        assert!(count < 1000, "stack_leader: loop of stacks");
        count += 1;
    }
    who
}

/// `olympia/loc.c:all_stack`: `who` and every character stacked under it,
/// depth first.
#[must_use]
pub fn all_stack(db: &Db, who: i32) -> Vec<i32> {
    fn add_char_here(db: &Db, who: i32, l: &mut Vec<i32>) {
        assert!(db.valid_box(who));
        l.push(who);
        for &i in db.entity(who).loc_info.here_list.as_slice() {
            if db.kind(i) == Kind::CHAR {
                add_char_here(db, i, l);
            }
        }
    }

    assert!(db.valid_box(who));
    let mut l = vec![who];
    for &i in db.entity(who).loc_info.here_list.as_slice() {
        if db.kind(i) == Kind::CHAR {
            add_char_here(db, i, &mut l);
        }
    }
    l
}

/// `check_moving()`: a stack leader marked moving with no running command
/// stops, with its stack (`restore_stack_actions()`), and a follower takes
/// its leader's moving flag.
fn check_moving(db: &mut Db) {
    for i in db.boxes_of_kind(Kind::CHAR) {
        if stack_leader(db, i) != i || char_moving(db, i) == 0 {
            continue;
        }
        let running = db
            .entity(i)
            .cmd
            .as_ref()
            .is_some_and(|c| c.state == STATE_RUN);
        if !running {
            warn!("\t{} moving but no command\n", Bytes(&box_name(db, i)));
            // olympia/move.c:restore_stack_actions
            for j in all_stack(db, i) {
                db.p_char(j).moving = 0;
            }
        }
    }

    for i in db.boxes_of_kind(Kind::CHAR) {
        let leader = stack_leader(db, i);
        if leader == i || char_moving(db, i) == char_moving(db, leader) {
            continue;
        }
        warn!(
            "\t{} moving disagrees with leader\n",
            Bytes(&box_name(db, i))
        );
        db.p_char(i).moving = char_moving(db, leader);
    }
}

/// `check_prisoner()`: a prisoner nobody holds is freed.
fn check_prisoner(db: &mut Db) {
    for who in db.boxes_of_kind(Kind::CHAR) {
        // is_prisoner()
        let prisoner = db
            .entity(who)
            .char_
            .as_ref()
            .is_some_and(|c| c.prisoner != 0);
        if !prisoner {
            continue;
        }
        if stack_parent(db, who) == 0 {
            warn!("\t{} prisoner but unstacked\n", Bytes(&box_name(db, who)));
            db.p_char(who).prisoner = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_names_are_spliced_into_messages() {
        let name = b"caf\xe9";
        assert_eq!(
            bytes_format(format_args!("a {} b {} c", Bytes(name), 7)),
            b"a caf\xe9 b 7 c"
        );
    }
}
