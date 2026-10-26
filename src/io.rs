// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Loading and saving the entity files (`olympia/io.c`).
//!
//! Each entity is a header line, `<n> <kind> <subkind>`, then two-letter
//! keyed lines, then a blank line. Substructures are a two-letter section
//! line (`CH`, `LO`, ...) followed by indented keyed lines.

use std::fmt;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::command::oly_parse_cmd;
use crate::cstr::{atoi, is_white, scan_ints};
use crate::entity::{
    Admit, BUY, Char, CharMagic, Command, Db, Entity, Gate, IList, Item, ItemEnt, ItemMagic, Kind,
    Loc, LocInfo, Misc, Player, ReqEnt, SELL, Skill, SkillEnt, Subkind, Subloc, Trade,
};
use crate::lines::LineReader;
use crate::os::{lib_path, make_dir, strerror};
use crate::system::{olytime_print, olytime_scan};

/// The entity files `read_all_boxes()` and `write_all_boxes()` handle, in
/// order, with the kind each holds (`misc` holds whatever is left).
const KIND_FILES: [(&str, Kind); 7] = [
    ("loc", Kind::LOC),
    ("item", Kind::ITEM),
    ("skill", Kind::SKILL),
    ("gate", Kind::GATE),
    ("road", Kind::ROAD),
    ("ship", Kind::SHIP),
    ("unform", Kind::UNFORM),
];

/// Appends formatted text to a byte buffer.
macro_rules! out {
    ($buf:expr, $($arg:tt)*) => {
        put($buf, format_args!($($arg)*))
    };
}

fn put(buf: &mut Vec<u8>, args: fmt::Arguments<'_>) {
    buf.write_fmt(args).expect("writing to a Vec cannot fail");
}

/// `linehash()`: the first two bytes of a line, or `None` if it is shorter.
fn linehash(s: &[u8]) -> Option<[u8; 2]> {
    (s.len() >= 2).then(|| [s[0], s[1]])
}

/// `t_string()`: the text after the key and its separator.
fn t_string(s: &[u8]) -> &[u8] {
    if s.len() >= 4 { &s[3..] } else { b"" }
}

fn lossy(s: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(s)
}

// ---------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------

/// The reader and one-line lookahead the scan routines share (`line` and
/// `advance` in io.c).
struct Scanner {
    r: LineReader,
    line: Option<Vec<u8>>,
    /// The box being loaded (`ext_boxnum`).
    box_num: i32,
}

impl Scanner {
    fn advance(&mut self) {
        self.line = self.r.readlin();
    }

    /// The next line of a `\`-continued list (`readlin_ew()`).
    fn continuation(&mut self) -> Option<Vec<u8>> {
        self.r.readlin_ew()
    }

    /// Runs `f` on each indented line of a section, with the key and text
    /// after the indent, until the first line that isn't indented.
    fn section(&mut self, mut f: impl FnMut(&mut Self, Option<[u8; 2]>, &[u8], &[u8])) {
        self.advance();
        while let Some(line) = self.line.take() {
            if !line.first().is_some_and(|&c| is_white(c)) {
                self.line = Some(line);
                break;
            }
            let rest = &line[1..];
            f(self, linehash(rest), t_string(rest), rest);
            self.advance();
        }
    }

    /// `box_scan()`: a box reference, or 0 if it isn't a valid box.
    fn box_scan(&self, db: &Db, t: &[u8], line: &[u8]) -> i32 {
        let n = atoi(t);
        if db.valid_box(n) {
            n
        } else {
            eprintln!("box_scan({}): bad reference: {}", self.box_num, lossy(line));
            0
        }
    }

    /// Reads a white-separated list of numbers with `\` continuations,
    /// calling `f` on each (the loop shared by `boxlist_scan()`,
    /// `known_scan()` and `admit_scan()`). Negative numbers end the list.
    fn number_list(&mut self, t: &[u8], mut f: impl FnMut(i32)) {
        let mut s = t.to_vec();
        let mut i = 0;
        loop {
            while i < s.len() && is_white(s[i]) {
                i += 1;
            }
            match s.get(i) {
                Some(c) if c.is_ascii_digit() => {
                    f(atoi(&s[i..]));
                    while i < s.len() && s[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                Some(b'\\') => {
                    // C dereferences NULL at end of file; stop instead.
                    let Some(next) = self.continuation() else {
                        break;
                    };
                    s = next;
                    i = 0;
                }
                _ => break,
            }
        }
    }

    /// `boxlist_scan()`: valid box references, appended to `l`.
    fn boxlist(&mut self, db: &Db, t: &[u8], l: &mut IList) {
        let box_num = self.box_num;
        self.number_list(t, |n| {
            if db.valid_box(n) {
                l.append(n);
            } else {
                eprintln!("boxlist_scan({box_num}): bad box reference: {n}");
            }
        });
    }

    /// `boxlist0_scan()`: like [`Self::boxlist`], but 0 is allowed.
    fn boxlist0(&mut self, db: &Db, t: &[u8], l: &mut IList) {
        let box_num = self.box_num;
        self.number_list(t, |n| {
            if n == 0 || db.valid_box(n) {
                l.append(n);
            } else {
                eprintln!("boxlist_scan({box_num}): bad box reference: {n}");
            }
        });
    }

    /// `known_scan()`: valid box references, each added once.
    fn known(&mut self, db: &Db, t: &[u8], kn: &mut IList) {
        let box_num = self.box_num;
        self.number_list(t, |n| {
            if db.valid_box(n) {
                if !kn.contains(n) {
                    kn.append(n);
                }
            } else {
                eprintln!("known_scan({box_num}): bad box reference: {n}");
            }
        });
    }

    /// Reads `\`-continued records of integers, calling `f` with the
    /// integers each record converts (the loop shared by
    /// `skill_list_scan()`, `item_list_scan()`, `trade_list_scan()` and
    /// `req_list_scan()`).
    fn records(&mut self, t: &[u8], n: usize, mut f: impl FnMut(&[i32])) {
        let mut s = t.to_vec();
        loop {
            f(&scan_ints(&s, n));
            // C reads s[-1] for an empty line; treat it as the last record.
            if s.last() != Some(&b'\\') {
                break;
            }
            let Some(next) = self.continuation() else {
                break;
            };
            s = next;
        }
    }
}

/// `v[i]` if `sscanf()` converted it, else `default`.
fn field(v: &[i32], i: usize, default: i32) -> i32 {
    v.get(i).copied().unwrap_or(default)
}

impl Scanner {
    fn skill_list(&mut self, db: &Db, t: &[u8], l: &mut Vec<SkillEnt>) {
        let box_num = self.box_num;
        self.records(t, 5, |v| {
            let e = SkillEnt {
                skill: field(v, 0, 0),
                know: field(v, 1, 0) as i8,
                days_studied: field(v, 2, 0),
                experience: field(v, 3, 0) as i16,
            };
            if db.valid_box(e.skill) {
                l.push(e);
            } else {
                eprintln!("skill_list_scan({box_num}): bad skill {}", e.skill);
            }
        });
    }

    fn item_list(&mut self, db: &Db, t: &[u8], l: &mut Vec<ItemEnt>) {
        let box_num = self.box_num;
        self.records(t, 2, |v| {
            let e = ItemEnt {
                item: field(v, 0, 0),
                qty: field(v, 1, 0),
            };
            if db.valid_box(e.item) {
                l.push(e);
            } else {
                eprintln!("item_list_scan({box_num}): bad item {}", e.item);
            }
        });
    }

    fn trade_list(&mut self, db: &Db, t: &[u8], l: &mut Vec<Trade>) {
        let box_num = self.box_num;
        self.records(t, 8, |v| {
            let e = Trade {
                kind: field(v, 0, 0),
                item: field(v, 1, 0),
                qty: field(v, 2, 0),
                cost: field(v, 3, 0),
                cloak: field(v, 4, 0),
                have_left: field(v, 5, 0),
                month_prod: field(v, 6, 0),
                expire: field(v, 7, 0),
                who: box_num,
            };
            if db.valid_box(e.item) {
                l.push(e);
            } else {
                eprintln!("trade_list_scan({box_num}): bad item {}", e.item);
            }
        });
    }

    fn req_list(&mut self, db: &Db, t: &[u8], l: &mut Vec<ReqEnt>) {
        let box_num = self.box_num;
        self.records(t, 3, |v| {
            let e = ReqEnt {
                item: field(v, 0, 0),
                qty: field(v, 1, 0),
                consume: field(v, 2, 0) as i8,
            };
            if db.valid_box(e.item) {
                l.push(e);
            } else {
                eprintln!("req_list_scan({box_num}): bad item {}", e.item);
            }
        });
    }

    fn admit(&mut self, db: &Db, t: &[u8], p: &mut Player) {
        let box_num = self.box_num;
        let mut a = Admit::default();
        let mut count = 0;
        self.number_list(t, |n| {
            match count {
                0 => a.targ = n,
                1 => a.sense = n,
                _ if db.valid_box(n) => a.l.append(n),
                _ => eprintln!("admit_scan({box_num}): bad box reference: {n}"),
            }
            count += 1;
        });
        if db.valid_box(a.targ) {
            p.admits.push(a);
        } else {
            eprintln!("admit_scan({box_num}): bad targ {}", a.targ);
        }
    }
}

/// Reports a section line with an unknown key.
fn bad_line(func: &str, box_num: i32, line: &[u8]) {
    eprintln!("{func}({box_num}):  bad line: {}", lossy(line));
}

fn scan_loc_info(sc: &mut Scanner, db: &Db, p: &mut LocInfo) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"wh") => p.where_ = sc.box_scan(db, t, line),
        Some(b"hl") => sc.boxlist(db, t, &mut p.here_list),
        _ => bad_line("scan_loc_info", sc.box_num, line),
    });
}

fn scan_magic(sc: &mut Scanner, db: &Db, p: &mut CharMagic) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"im") => p.magician = atoi(t) as i8,
        Some(b"ma") => p.max_aura = atoi(t),
        Some(b"ca") => p.cur_aura = atoi(t),
        Some(b"as") => p.ability_shroud = atoi(t) as i16,
        Some(b"hm") => p.hinder_meditation = atoi(t) as i8,
        Some(b"pc") => p.project_cast = sc.box_scan(db, t, line),
        Some(b"qc") => p.quick_cast = atoi(t) as i16,
        Some(b"ot") => p.token = sc.box_scan(db, t, line),
        Some(b"pl") => p.pledge = sc.box_scan(db, t, line),
        Some(b"ar") => p.auraculum = sc.box_scan(db, t, line),
        Some(b"rb") => p.aura_reflect = atoi(t) as i8,
        Some(b"hs") => p.hide_self = atoi(t) as i8,
        Some(b"cm") => p.hide_mage = atoi(t) as i8,
        Some(b"pr") => p.pray = atoi(t) as i8,
        Some(b"sr") => p.swear_on_release = atoi(t) as i8,
        Some(b"kw") => p.knows_weather = atoi(t) as i8,
        Some(b"vp") => p.vis_protect = atoi(t) as i8,
        Some(b"dg") => p.default_garr = atoi(t) as i8,
        Some(b"bf") => p.fee = atoi(t),
        Some(b"vi") => sc.known(db, t, &mut p.visions),
        _ => bad_line("scan_magic", sc.box_num, line),
    });
}

fn scan_char(sc: &mut Scanner, db: &Db, p: &mut Char) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"ni") => p.unit_item = sc.box_scan(db, t, line),
        Some(b"lo") => p.unit_lord = sc.box_scan(db, t, line),
        Some(b"pl") => p.prev_lord = sc.box_scan(db, t, line),
        Some(b"he") => p.health = atoi(t) as i8,
        Some(b"si") => p.sick = atoi(t) as i8,
        Some(b"pr") => p.prisoner = atoi(t) as i8,
        Some(b"mo") => p.moving = atoi(t),
        Some(b"bh") => p.behind = atoi(t) as i8,
        Some(b"lk") => p.loy_kind = atoi(t) as i8,
        Some(b"lr") => p.loy_rate = atoi(t),
        Some(b"gu") => p.guard = atoi(t) as i8,
        Some(b"tf") => p.time_flying = atoi(t) as i8,
        Some(b"bp") => p.break_point = atoi(t) as i8,
        Some(b"ra") => p.rank = atoi(t) as i8,
        Some(b"at") => p.attack = atoi(t) as i16,
        Some(b"df") => p.defense = atoi(t) as i16,
        Some(b"mi") => p.missile = atoi(t) as i16,
        Some(b"po") => p.npc_prog = atoi(t) as i8,
        Some(b"ct") => sc.boxlist(db, t, &mut p.contact),
        Some(b"sl") => sc.skill_list(db, t, &mut p.skills),
        Some(b"dt") => olytime_scan(t, &mut p.death_time),
        _ => bad_line("scan_char", sc.box_num, line),
    });
}

fn scan_loc(sc: &mut Scanner, db: &Db, p: &mut Loc) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"hi") => p.hidden = atoi(t) as i8,
        Some(b"sh") => p.shroud = atoi(t) as i16,
        Some(b"ba") => p.barrier = atoi(t) as i16,
        Some(b"dg") => p.dist_from_gate = atoi(t) as i8,
        Some(b"lc") => p.civ = atoi(t) as i8,
        Some(b"sl") => p.sea_lane = atoi(t) as i8,
        Some(b"pd") => sc.boxlist0(db, t, &mut p.prov_dest),
        _ => bad_line("scan_loc", sc.box_num, line),
    });
}

fn scan_subloc(sc: &mut Scanner, db: &Db, p: &mut Subloc) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"da") => p.damage = atoi(t) as u8,
        Some(b"de") => p.defense = atoi(t),
        Some(b"ca") => p.capacity = atoi(t),
        Some(b"er") => p.effort_required = atoi(t),
        Some(b"eg") => p.effort_given = atoi(t),
        Some(b"bm") => p.build_materials = atoi(t) as i8,
        Some(b"mo") => p.moving = atoi(t),
        Some(b"gr") => p.galley_ram = atoi(t) as i8,
        Some(b"sd") => p.shaft_depth = atoi(t) as i16,
        Some(b"sh") => p.safe = atoi(t) as i8,
        Some(b"mc") => p.major = atoi(t) as i8,
        Some(b"op") => p.opium_econ = atoi(t),
        Some(b"lo") => p.loot = atoi(t) as i8,
        Some(b"cp") => p.prominence = atoi(t) as i8,
        Some(b"lw") => p.link_when = atoi(t) as i8,
        Some(b"lp") => p.link_open = atoi(t) as i8,
        Some(b"uf") => p.uldim_flag = atoi(t) as i8,
        Some(b"sf") => p.summer_flag = atoi(t) as i8,
        Some(b"ql") => p.quest_late = atoi(t) as i8,
        Some(b"td") => p.tunnel_level = atoi(t) as i8,
        Some(b"cl") => p.castle_lev = atoi(t) as i8,
        Some(b"lt") => sc.boxlist(db, t, &mut p.link_to),
        Some(b"lf") => sc.boxlist(db, t, &mut p.link_from),
        Some(b"te") => sc.boxlist(db, t, &mut p.teaches),
        Some(b"nc") => sc.boxlist(db, t, &mut p.near_cities),
        Some(b"bs") => sc.boxlist(db, t, &mut p.bound_storms),
        _ => bad_line("scan_subloc", sc.box_num, line),
    });
}

fn scan_item(sc: &mut Scanner, db: &Db, p: &mut Item) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"pl") => p.plural_name = Some(t.to_vec()),
        Some(b"wt") => p.weight = atoi(t) as i16,
        Some(b"lc") => p.land_cap = atoi(t) as i16,
        Some(b"rc") => p.ride_cap = atoi(t) as i16,
        Some(b"fc") => p.fly_cap = atoi(t) as i16,
        Some(b"mu") => p.is_man_item = atoi(t) as i8,
        Some(b"pr") => p.prominent = atoi(t) as i8,
        Some(b"an") => p.animal = atoi(t) as i8,
        Some(b"un") => p.who_has = sc.box_scan(db, t, line),
        Some(b"at") => p.attack = atoi(t) as i16,
        Some(b"df") => p.defense = atoi(t) as i16,
        Some(b"mi") => p.missile = atoi(t) as i16,
        Some(b"bp") => p.base_price = atoi(t),
        Some(b"ca") => p.capturable = atoi(t) as i8,
        _ => bad_line("scan_item", sc.box_num, line),
    });
}

fn scan_item_magic(sc: &mut Scanner, db: &Db, p: &mut ItemMagic) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"au") => p.aura = atoi(t) as i16,
        Some(b"cl") => p.curse_loyalty = atoi(t) as i8,
        Some(b"cr") => p.cloak_region = atoi(t) as i8,
        Some(b"cc") => p.cloak_creator = atoi(t) as i8,
        Some(b"uk") => p.use_key = atoi(t) as i8,
        Some(b"rc") => p.region_created = sc.box_scan(db, t, line),
        Some(b"pc") => p.project_cast = sc.box_scan(db, t, line),
        Some(b"ct") => p.creator = sc.box_scan(db, t, line),
        Some(b"lo") => p.lore = sc.box_scan(db, t, line),
        Some(b"qc") => p.quick_cast = atoi(t) as i16,
        Some(b"ab") => p.attack_bonus = atoi(t) as i8,
        Some(b"db") => p.defense_bonus = atoi(t) as i8,
        Some(b"mb") => p.missile_bonus = atoi(t) as i8,
        Some(b"ba") => p.aura_bonus = atoi(t) as i16,
        Some(b"rd") => p.relic_decay = atoi(t) as i16,
        Some(b"tn") => p.token_num = atoi(t) as i8,
        Some(b"ti") => p.token_ni = atoi(t),
        Some(b"oc") => p.orb_use_count = atoi(t) as i8,
        Some(b"mu") => sc.boxlist(db, t, &mut p.may_use),
        Some(b"ms") => sc.boxlist(db, t, &mut p.may_study),
        _ => bad_line("scan_item_magic", sc.box_num, line),
    });
}

fn scan_player(sc: &mut Scanner, db: &Db, p: &mut Player) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"fn") => p.full_name = Some(t.to_vec()),
        Some(b"em") => p.email = Some(t.to_vec()),
        Some(b"ve") => p.vis_email = Some(t.to_vec()),
        Some(b"le") => p.last_email = Some(t.to_vec()),
        Some(b"pw") => p.password = Some(t.to_vec()),
        Some(b"np") => p.noble_points = atoi(t) as i16,
        Some(b"fs") => p.fast_study = atoi(t) as i16,
        Some(b"ft") => p.first_turn = atoi(t),
        Some(b"fo") => p.format = atoi(t) as i8,
        Some(b"nt") => p.notab = atoi(t) as i8,
        Some(b"tf") => p.first_tower = atoi(t) as i8,
        Some(b"so") => p.sent_orders = atoi(t) as i8,
        Some(b"lt") => p.last_order_turn = atoi(t),
        Some(b"sl") => p.split_lines = atoi(t),
        Some(b"sb") => p.split_bytes = atoi(t),
        Some(b"ci") => p.compuserve = atoi(t) as i8,
        Some(b"bm") => p.broken_mailer = atoi(t) as i8,
        Some(b"dr") => p.dont_remind = atoi(t) as i8,
        Some(b"kn") => sc.known(db, t, &mut p.known),
        Some(b"un") => sc.boxlist(db, t, &mut p.units),
        Some(b"uf") => sc.boxlist(db, t, &mut p.unformed),
        Some(b"am") => sc.admit(db, t, p),
        _ => bad_line("scan_player", sc.box_num, line),
    });
}

fn scan_skill(sc: &mut Scanner, db: &Db, p: &mut Skill) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"tl") => p.time_to_learn = atoi(t),
        Some(b"ne") => p.no_exp = atoi(t),
        Some(b"np") => p.np_req = atoi(t),
        Some(b"rs") => p.required_skill = sc.box_scan(db, t, line),
        Some(b"pr") => p.produced = sc.box_scan(db, t, line),
        Some(b"of") => sc.boxlist(db, t, &mut p.offered),
        Some(b"re") => sc.boxlist(db, t, &mut p.research),
        Some(b"rq") => sc.req_list(db, t, &mut p.req),
        _ => bad_line("scan_skill", sc.box_num, line),
    });
}

fn scan_command(sc: &mut Scanner, db: &Db, p: &mut Command) {
    p.who = sc.box_num;
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"li") => {
            if !oly_parse_cmd(p, t) {
                eprintln!("scan_command({}): bad cmd {}", sc.box_num, lossy(t));
            }
        }
        Some(b"cs") => p.state = atoi(t) as i8,
        Some(b"wa") => p.wait = atoi(t),
        Some(b"st") => p.status = atoi(t) as i8,
        Some(b"de") => p.days_executing = atoi(t),
        Some(b"po") => p.poll = atoi(t) as i8,
        Some(b"pr") => p.pri = atoi(t) as i8,
        Some(b"if") => p.inhibit_finish = atoi(t) as i8,
        Some(b"us") => p.use_skill = sc.box_scan(db, t, line),
        Some(b"ue") => p.use_exp = atoi(t),
        Some(b"ar") => {
            for (arg, v) in p.args.iter_mut().zip(scan_ints(t, 8)) {
                *arg = v;
            }
        }
        _ => bad_line("scan_command", sc.box_num, line),
    });
}

fn scan_gate(sc: &mut Scanner, db: &Db, p: &mut Gate) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"tl") => p.to_loc = sc.box_scan(db, t, line),
        Some(b"nj") => p.notify_jumps = sc.box_scan(db, t, line),
        Some(b"nu") => p.notify_unseal = sc.box_scan(db, t, line),
        Some(b"sk") => p.seal_key = atoi(t) as i16,
        Some(b"rh") => p.road_hidden = atoi(t) as i8,
        _ => bad_line("scan_gate", sc.box_num, line),
    });
}

fn scan_misc(sc: &mut Scanner, db: &Db, p: &mut Misc) {
    sc.section(|sc, key, t, line| match key.as_ref().map(|k| &k[..]) {
        Some(b"di") => p.npc_dir = atoi(t) as i8,
        Some(b"mc") => p.npc_created = atoi(t),
        Some(b"md") => p.mine_delay = atoi(t) as i8,
        Some(b"ss") => p.storm_str = atoi(t) as i16,
        Some(b"mh") => p.npc_home = sc.box_scan(db, t, line),
        Some(b"gc") => p.garr_castle = sc.box_scan(db, t, line),
        Some(b"sb") => p.summoned_by = sc.box_scan(db, t, line),
        Some(b"co") => p.npc_cookie = sc.box_scan(db, t, line),
        Some(b"ov") => p.only_vulnerable = sc.box_scan(db, t, line),
        Some(b"bs") => p.bind_storm = sc.box_scan(db, t, line),
        Some(b"ol") => p.old_lord = sc.box_scan(db, t, line),
        Some(b"sn") => p.save_name = Some(t.to_vec()),
        Some(b"ds") => p.display = Some(t.to_vec()),
        Some(b"ca") => p.cmd_allow = t.first().copied().unwrap_or(0),
        Some(b"nm") => sc.known(db, t, &mut p.npc_memory),
        _ => bad_line("scan_misc", sc.box_num, line),
    });
}

/// Scans a substructure section: takes it out of box `n` (allocating it on
/// demand, as `p_xxx()` does), scans into it, and puts it back.
fn scan_part<T: Default>(
    sc: &mut Scanner,
    db: &mut Db,
    n: i32,
    slot: fn(&mut Entity) -> &mut Option<Box<T>>,
    scan: fn(&mut Scanner, &Db, &mut T),
) {
    let mut part = slot(db.entity_mut(n)).take().unwrap_or_default();
    scan(sc, db, &mut part);
    *slot(db.entity_mut(n)) = Some(part);
}

/// Loads one entity's lines (`load_box()`).
///
/// # Panics
///
/// Panics if box `n` wasn't allocated by the scan of `master`, as C asserts.
fn load_box(sc: &mut Scanner, db: &mut Db, n: i32, libdir: &Path) {
    if !db.valid_box(n) {
        eprintln!("Unforseen box {n} found in load phase.");
        eprintln!("Remove {}/master and retry.", libdir.display());
        panic!("box {n} was not allocated by the master scan");
    }
    sc.box_num = n;

    sc.advance();
    while let Some(line) = sc.line.clone() {
        if line.is_empty() {
            break;
        }
        if line[0] == b'#' {
            sc.advance();
            continue;
        }
        let t = t_string(&line);
        match linehash(&line).as_ref().map(|k| &k[..]) {
            Some(b"na") => {
                db.set_name(n, t);
                sc.advance();
            }
            Some(b"il") => {
                let mut items = std::mem::take(&mut db.entity_mut(n).items);
                sc.item_list(db, t, &mut items);
                db.entity_mut(n).items = items;
                sc.advance();
            }
            Some(b"tl") => {
                let mut trades = std::mem::take(&mut db.entity_mut(n).trades);
                sc.trade_list(db, t, &mut trades);
                db.entity_mut(n).trades = trades;
                sc.advance();
            }
            Some(k @ (b"an" | b"ad" | b"ah")) => {
                let mut disp = db.entity_mut(n).disp.take().unwrap_or_default();
                let list = match k {
                    b"an" => &mut disp.neutral,
                    b"ad" => &mut disp.defend,
                    _ => &mut disp.hostile,
                };
                sc.boxlist(db, t, list);
                db.entity_mut(n).disp = Some(disp);
                sc.advance();
            }
            Some(b"CH") => scan_part(sc, db, n, |e| &mut e.char_, scan_char),
            Some(b"CM") => {
                let mut ch: Box<Char> = db.entity_mut(n).char_.take().unwrap_or_default();
                let mut magic = ch.magic.take().unwrap_or_default();
                scan_magic(sc, db, &mut magic);
                ch.magic = Some(magic);
                db.entity_mut(n).char_ = Some(ch);
            }
            Some(b"LI") => {
                let mut li = std::mem::take(&mut db.entity_mut(n).loc_info);
                scan_loc_info(sc, db, &mut li);
                db.entity_mut(n).loc_info = li;
            }
            Some(b"LO") => scan_part(sc, db, n, |e| &mut e.loc, scan_loc),
            Some(b"SL") => scan_part(sc, db, n, |e| &mut e.subloc, scan_subloc),
            Some(b"IT") => scan_part(sc, db, n, |e| &mut e.item, scan_item),
            Some(b"IM") => {
                let mut item: Box<Item> = db.entity_mut(n).item.take().unwrap_or_default();
                let mut magic = item.magic.take().unwrap_or_default();
                scan_item_magic(sc, db, &mut magic);
                item.magic = Some(magic);
                db.entity_mut(n).item = Some(item);
            }
            Some(b"PL") => scan_part(sc, db, n, |e| &mut e.player, scan_player),
            Some(b"SK") => scan_part(sc, db, n, |e| &mut e.skill, scan_skill),
            Some(b"GA") => scan_part(sc, db, n, |e| &mut e.gate, scan_gate),
            Some(b"MI") => scan_part(sc, db, n, |e| &mut e.misc, scan_misc),
            Some(b"CO") => scan_part(sc, db, n, |e| &mut e.cmd, scan_command),
            _ => {
                eprintln!("load_box({n}):  bad line: {}", lossy(&line));
                sc.advance();
                while sc
                    .line
                    .as_ref()
                    .is_some_and(|l| l.first().is_some_and(|&c| is_white(c)))
                {
                    sc.advance();
                }
            }
        }
    }
    // Step over the blank line that ends the entity.
    if sc.line.is_some() {
        sc.advance();
    }
}

/// Loads every entity in one file (`read_boxes()`).
///
/// # Panics
///
/// Panics on a comment or a line that doesn't start with a box number
/// where an entity should begin: the C engine loops forever there.
fn read_boxes(db: &mut Db, libdir: &Path, name: &str) {
    let path = lib_path(libdir, name);
    let Some(r) = LineReader::open(&path) else {
        return;
    };
    let mut sc = Scanner {
        r,
        line: None,
        box_num: 0,
    };
    sc.advance();
    while let Some(line) = sc.line.clone() {
        assert!(
            line.first() != Some(&b'#'),
            "read_boxes: comment between entities in {} hangs the C engine",
            path.display()
        );
        let box_num = atoi(&line);
        if box_num > 0 {
            load_box(&mut sc, db, box_num, libdir);
        } else {
            eprintln!("read_boxes: unexpected line {}", lossy(&line));
            panic!(
                "read_boxes: unexpected line in {} hangs the C engine",
                path.display()
            );
        }
    }
}

/// The names of the faction files in `fact/`.
fn fact_files(libdir: &Path, func: &str) -> Vec<String> {
    let dir = lib_path(libdir, "fact");
    match fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.as_bytes().first().is_some_and(u8::is_ascii_digit))
            .collect(),
        Err(err) => {
            eprintln!("{func}: can't open {}: {}", dir.display(), strerror(&err));
            Vec::new()
        }
    }
}

/// Loads every entity's attributes (`read_all_boxes()`).
pub fn read_all_boxes(db: &mut Db, libdir: &Path) {
    for (name, _) in KIND_FILES {
        read_boxes(db, libdir, name);
    }
    read_boxes(db, libdir, "misc");
    for name in fact_files(libdir, "read_chars") {
        read_boxes(db, libdir, &format!("fact/{name}"));
    }
}

/// Allocates every box listed in `master` (`fast_scan()`). Returns false if
/// there is no `master`.
pub fn fast_scan(db: &mut Db, libdir: &Path) -> bool {
    let Some(mut r) = LineReader::open(&lib_path(libdir, "master")) else {
        return false;
    };
    while let Some(s) = r.readlin() {
        let num = atoi(&s);
        let mut p = s
            .iter()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(s.len());
        while p < s.len() && is_white(s[p]) {
            p += 1;
        }
        let (kind, sk) = match s[p..].iter().position(|&c| c == b'.') {
            Some(dot) => (atoi(&s[p..p + dot]), atoi(&s[p + dot + 1..])),
            None => (atoi(&s[p..]), 0),
        };
        db.alloc_box(num, Kind(kind as i8), Subkind(sk as i8));
    }
    true
}

/// Allocates every box by scanning the entity files' header lines
/// (`scan_boxes()`), for when there is no `master`.
fn scan_boxes(db: &mut Db, libdir: &Path, name: &str) {
    let Some(mut r) = LineReader::open(&lib_path(libdir, name)) else {
        return;
    };
    while let Some(s) = r.readlin() {
        if s.first() == Some(&b'#') {
            continue;
        }
        let box_num = atoi(&s);
        let mut rest = &s[s.iter().position(|&c| c == b' ').unwrap_or(s.len())..];
        if rest.first() == Some(&b' ') {
            rest = &rest[1..];
        }
        let (kind_s, sub_s) = match rest.iter().position(|&c| c == b' ') {
            Some(sp) => (&rest[..sp], &rest[sp + 1..]),
            None => (rest, &rest[rest.len()..]),
        };
        let kind = Kind::from_name(kind_s).unwrap_or_else(|| {
            eprintln!("read_boxes({box_num}): bad kind: {}", lossy(kind_s));
            Kind::DELETED
        });
        let sk = if sub_s.first() == Some(&b'0') {
            Subkind::default()
        } else {
            Subkind::from_name(sub_s).unwrap_or_else(|| {
                eprintln!("read_boxes({box_num}): bad subkind: {}", lossy(sub_s));
                Subkind::default()
            })
        };
        db.alloc_box(box_num, kind, sk);

        // Skip the entity's body, up to its blank line.
        while let Some(s) = r.readlin() {
            if s.first() == Some(&b'#') {
                continue;
            }
            if s.iter().all(|&c| is_white(c)) {
                break;
            }
        }
    }
}

/// Allocates every box from the entity files (`scan_all_boxes()`).
pub fn scan_all_boxes(db: &mut Db, libdir: &Path) {
    eprintln!("fast_scan failed, scan_all_boxes()");
    for (name, _) in KIND_FILES {
        scan_boxes(db, libdir, name);
    }
    scan_boxes(db, libdir, "misc");
    for name in fact_files(libdir, "scan_chars") {
        scan_boxes(db, libdir, &format!("fact/{name}"));
    }
}

// ---------------------------------------------------------------------
// Saving
// ---------------------------------------------------------------------

/// What the printers need to know about other boxes.
struct Printer<'a> {
    db: &'a Db,
    /// `-p`: follow box references with the box's name.
    pretty: bool,
}

impl Printer<'_> {
    /// `if_name()`: ` (<name>)` when pretty-printing a named valid box.
    fn if_name(&self, out: &mut Vec<u8>, n: i32) {
        if self.pretty && self.db.valid_box(n) {
            let name = self.db.name(n);
            if !name.is_empty() {
                out.extend_from_slice(b" (");
                out.extend_from_slice(name);
                out.push(b')');
            }
        }
    }

    /// `box_print()`: the header and box number, if it is a valid box.
    fn box_ref(&self, out: &mut Vec<u8>, header: &str, n: i32) {
        if self.db.valid_box(n) {
            out!(out, "{header}{n}");
            self.if_name(out, n);
            out.push(b'\n');
        }
    }

    /// Prints numbers eleven to a line, continuing with `\`, after a
    /// header printed before the first one; `count` is where the count
    /// starts.
    fn numbers(out: &mut Vec<u8>, header: &str, values: impl Iterator<Item = i32>, start: i32) {
        let mut count = start;
        for v in values {
            count += 1;
            if count == start + 1 {
                out.extend_from_slice(header.as_bytes());
            }
            if count % 11 == 10 {
                out.extend_from_slice(b"\\\n\t");
            }
            out!(out, "{v} ");
        }
        if count > start {
            out.push(b'\n');
        }
    }

    /// `boxlist_print()`: the valid boxes in `l`.
    fn boxlist(&self, out: &mut Vec<u8>, header: &str, l: &IList) {
        let valid = l
            .as_slice()
            .iter()
            .copied()
            .filter(|&n| self.db.valid_box(n));
        Self::numbers(out, header, valid, 0);
    }

    /// `boxlist0_print()`: like `boxlist_print()`, but 0 is kept.
    fn boxlist0(&self, out: &mut Vec<u8>, header: &str, l: &IList) {
        let valid = l
            .as_slice()
            .iter()
            .copied()
            .filter(|&n| n == 0 || self.db.valid_box(n));
        Self::numbers(out, header, valid, 0);
    }

    /// `known_print()`: the valid boxes in a sparse set, which the caller
    /// has sorted (`loop_known` sorts in place).
    fn known(&self, out: &mut Vec<u8>, header: &str, kn: &IList) {
        let mut first = true;
        for (count, n) in kn
            .as_slice()
            .iter()
            .copied()
            .filter(|&n| self.db.valid_box(n))
            .enumerate()
        {
            if first {
                out.extend_from_slice(header.as_bytes());
                first = false;
            }
            if count % 11 == 10 {
                out.extend_from_slice(b"\\\n\t");
            }
            out!(out, "{n} ");
        }
        if !first {
            out.push(b'\n');
        }
    }

    /// Prints records joined by ` \` continuations after a header
    /// (`skill_list_print()` and friends).
    fn records(out: &mut Vec<u8>, header: &str, records: impl Iterator<Item = String>) {
        let mut count = 0;
        for r in records {
            count += 1;
            if count == 1 {
                out.extend_from_slice(header.as_bytes());
            }
            if count > 1 {
                out.extend_from_slice(b" \\\n\t");
            }
            out.extend_from_slice(r.as_bytes());
        }
        if count > 0 {
            out.push(b'\n');
        }
    }

    fn admit(&self, out: &mut Vec<u8>, p: &Admit) {
        if !self.db.valid_box(p.targ) || (p.sense == 0 && p.l.is_empty()) {
            return;
        }
        out!(out, " am {} {} ", p.targ, p.sense);
        let mut count = 2;
        for &n in p.l.as_slice() {
            if self.db.valid_box(n) {
                count += 1;
                if count % 11 == 10 {
                    out.extend_from_slice(b"\\\n\t");
                }
                out!(out, "{n} ");
            }
        }
        out.push(b'\n');
    }
}

/// Prints `key value` if `value` is nonzero.
fn nonzero(out: &mut Vec<u8>, key: &str, value: impl Into<i32>) {
    let value = value.into();
    if value != 0 {
        out!(out, "{key}{value}\n");
    }
}

/// Prints `key text` if `text` is set and not empty.
fn text(out: &mut Vec<u8>, key: &str, value: Option<&[u8]>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(v);
        out.push(b'\n');
    }
}

impl Printer<'_> {
    fn loc_info(&self, out: &mut Vec<u8>, p: &LocInfo) {
        if *p == LocInfo::default() {
            return;
        }
        out.extend_from_slice(b"LI\n");
        self.box_ref(out, " wh ", p.where_);
        self.boxlist(out, " hl ", &p.here_list);
    }

    fn magic(&self, out: &mut Vec<u8>, p: &CharMagic) {
        if *p == CharMagic::default() {
            return;
        }
        out.extend_from_slice(b"CM\n");
        nonzero(out, " im ", p.magician);
        nonzero(out, " ma ", p.max_aura);
        nonzero(out, " ca ", p.cur_aura);
        nonzero(out, " as ", p.ability_shroud);
        nonzero(out, " hm ", p.hinder_meditation);
        nonzero(out, " qc ", p.quick_cast);
        nonzero(out, " rb ", p.aura_reflect);
        nonzero(out, " hs ", p.hide_self);
        nonzero(out, " cm ", p.hide_mage);
        nonzero(out, " pr ", p.pray);
        nonzero(out, " kw ", p.knows_weather);
        nonzero(out, " dg ", p.default_garr);
        nonzero(out, " sr ", p.swear_on_release);
        nonzero(out, " bf ", p.fee);
        nonzero(out, " vp ", p.vis_protect);
        self.box_ref(out, " pl ", p.pledge);
        self.box_ref(out, " pc ", p.project_cast);
        self.box_ref(out, " ar ", p.auraculum);
        self.box_ref(out, " ot ", p.token);
        self.known(out, " vi ", &p.visions);
    }

    fn char_(&self, out: &mut Vec<u8>, p: &Char) {
        out.extend_from_slice(b"CH\n");
        self.box_ref(out, " ni ", p.unit_item);
        self.box_ref(out, " lo ", p.unit_lord);
        self.box_ref(out, " pl ", p.prev_lord);
        nonzero(out, " he ", p.health);
        nonzero(out, " si ", p.sick);
        nonzero(out, " lk ", p.loy_kind);
        nonzero(out, " lr ", p.loy_rate);
        let skills = p
            .skills
            .iter()
            .filter(|s| self.db.valid_box(s.skill))
            .map(|s| {
                format!(
                    "{} {} {} {} 0",
                    s.skill, s.know, s.days_studied, s.experience
                )
            });
        Self::records(out, " sl\t", skills);
        nonzero(out, " pr ", p.prisoner);
        nonzero(out, " mo ", p.moving);
        nonzero(out, " bh ", p.behind);
        nonzero(out, " gu ", p.guard);
        nonzero(out, " tf ", p.time_flying);
        nonzero(out, " bp ", p.break_point);
        nonzero(out, " ra ", p.rank);
        nonzero(out, " at ", p.attack);
        nonzero(out, " df ", p.defense);
        nonzero(out, " mi ", p.missile);
        nonzero(out, " po ", p.npc_prog);
        self.boxlist(out, " ct ", &p.contact);
        olytime_print(out, " dt ", &p.death_time);
    }

    fn loc(&self, out: &mut Vec<u8>, p: &Loc) {
        if *p == Loc::default() {
            return;
        }
        out.extend_from_slice(b"LO\n");
        self.boxlist0(out, " pd ", &p.prov_dest);
        nonzero(out, " hi ", p.hidden);
        nonzero(out, " sh ", p.shroud);
        nonzero(out, " ba ", p.barrier);
        nonzero(out, " dg ", p.dist_from_gate);
        nonzero(out, " sl ", p.sea_lane);
        nonzero(out, " lc ", p.civ);
    }

    fn subloc(&self, out: &mut Vec<u8>, p: &Subloc) {
        out.extend_from_slice(b"SL\n");
        self.boxlist(out, " te ", &p.teaches);
        nonzero(out, " da ", p.damage);
        nonzero(out, " de ", p.defense);
        nonzero(out, " ca ", p.capacity);
        nonzero(out, " bm ", p.build_materials);
        nonzero(out, " er ", p.effort_required);
        nonzero(out, " eg ", p.effort_given);
        nonzero(out, " mo ", p.moving);
        nonzero(out, " gr ", p.galley_ram);
        nonzero(out, " sd ", p.shaft_depth);
        nonzero(out, " cl ", p.castle_lev);
        nonzero(out, " sh ", p.safe);
        nonzero(out, " mc ", p.major);
        nonzero(out, " op ", p.opium_econ);
        nonzero(out, " lo ", p.loot);
        nonzero(out, " cp ", p.prominence);
        nonzero(out, " uf ", p.uldim_flag);
        nonzero(out, " sf ", p.summer_flag);
        nonzero(out, " ql ", p.quest_late);
        nonzero(out, " td ", p.tunnel_level);
        self.boxlist(out, " nc ", &p.near_cities);
        self.boxlist(out, " lt ", &p.link_to);
        self.boxlist(out, " lf ", &p.link_from);
        self.boxlist(out, " bs ", &p.bound_storms);
        nonzero(out, " lw ", p.link_when);
        nonzero(out, " lp ", p.link_open);
    }

    fn item(&self, out: &mut Vec<u8>, p: &Item) {
        if *p == Item::default() {
            return;
        }
        out.extend_from_slice(b"IT\n");
        text(out, " pl ", p.plural_name.as_deref());
        nonzero(out, " wt ", p.weight);
        nonzero(out, " lc ", p.land_cap);
        nonzero(out, " rc ", p.ride_cap);
        // C tests ride_cap here, not fly_cap.
        if p.ride_cap != 0 {
            out!(out, " fc {}\n", p.fly_cap);
        }
        nonzero(out, " mu ", p.is_man_item);
        nonzero(out, " pr ", p.prominent);
        nonzero(out, " an ", p.animal);
        nonzero(out, " at ", p.attack);
        nonzero(out, " df ", p.defense);
        nonzero(out, " mi ", p.missile);
        nonzero(out, " bp ", p.base_price);
        nonzero(out, " ca ", p.capturable);
        self.box_ref(out, " un ", p.who_has);
    }

    fn item_magic(&self, out: &mut Vec<u8>, p: &ItemMagic) {
        if *p == ItemMagic::default() {
            return;
        }
        out.extend_from_slice(b"IM\n");
        nonzero(out, " au ", p.aura);
        nonzero(out, " cl ", p.curse_loyalty);
        nonzero(out, " cr ", p.cloak_region);
        nonzero(out, " cc ", p.cloak_creator);
        nonzero(out, " uk ", p.use_key);
        nonzero(out, " qc ", p.quick_cast);
        nonzero(out, " ab ", p.attack_bonus);
        nonzero(out, " db ", p.defense_bonus);
        nonzero(out, " mb ", p.missile_bonus);
        nonzero(out, " ba ", p.aura_bonus);
        nonzero(out, " rd ", p.relic_decay);
        nonzero(out, " tn ", p.token_num);
        nonzero(out, " oc ", p.orb_use_count);
        self.box_ref(out, " ti ", p.token_ni);
        self.box_ref(out, " rc ", p.region_created);
        self.box_ref(out, " pc ", p.project_cast);
        self.box_ref(out, " ct ", p.creator);
        self.box_ref(out, " lo ", p.lore);
        self.boxlist(out, " mu ", &p.may_use);
        self.boxlist(out, " ms ", &p.may_study);
    }

    fn player(&self, out: &mut Vec<u8>, p: &Player) {
        out.extend_from_slice(b"PL\n");
        text(out, " fn ", p.full_name.as_deref());
        text(out, " em ", p.email.as_deref());
        text(out, " ve ", p.vis_email.as_deref());
        text(out, " le ", p.last_email.as_deref());
        text(out, " pw ", p.password.as_deref());
        nonzero(out, " np ", p.noble_points);
        nonzero(out, " fs ", p.fast_study);
        nonzero(out, " ft ", p.first_turn);
        nonzero(out, " fo ", p.format);
        nonzero(out, " nt ", p.notab);
        nonzero(out, " tf ", p.first_tower);
        nonzero(out, " sl ", p.split_lines);
        nonzero(out, " sb ", p.split_bytes);
        nonzero(out, " so ", p.sent_orders);
        nonzero(out, " dr ", p.dont_remind);
        nonzero(out, " ci ", p.compuserve);
        nonzero(out, " bm ", p.broken_mailer);
        nonzero(out, " lt ", p.last_order_turn);
        self.known(out, " kn ", &p.known);
        self.boxlist(out, " un ", &p.units);
        self.boxlist(out, " uf ", &p.unformed);
        for a in &p.admits {
            self.admit(out, a);
        }
    }

    fn skill(&self, out: &mut Vec<u8>, p: &Skill) {
        out.extend_from_slice(b"SK\n");
        out!(out, " tl {}\n", p.time_to_learn);
        self.box_ref(out, " rs ", p.required_skill);
        self.boxlist(out, " of ", &p.offered);
        self.boxlist(out, " re ", &p.research);
        let reqs = p
            .req
            .iter()
            .filter(|r| self.db.valid_box(r.item))
            .map(|r| format!("{} {} {}", r.item, r.qty, r.consume));
        Self::records(out, " rq\t", reqs);
        self.box_ref(out, " pr ", p.produced);
        nonzero(out, " np ", p.np_req);
        nonzero(out, " ne ", p.no_exp);
    }

    fn command(&self, out: &mut Vec<u8>, p: &Command) {
        if p.cmd == 0 {
            return;
        }
        out.extend_from_slice(b"CO\n li ");
        // printf("%s", NULL) prints "(null)".
        out.extend_from_slice(p.line.as_deref().unwrap_or(b"(null)"));
        out.push(b'\n');
        let [a, b, c, d, e, f, g, h] = p.args;
        out!(out, " ar {a} {b} {c} {d} {e} {f} {g} {h}\n");
        nonzero(out, " cs ", p.state);
        nonzero(out, " wa ", p.wait);
        nonzero(out, " st ", p.status);
        if p.use_skill != 0 {
            self.box_ref(out, " us ", p.use_skill);
        }
        nonzero(out, " ue ", p.use_exp);
        nonzero(out, " de ", p.days_executing);
        nonzero(out, " po ", p.poll);
        nonzero(out, " pr ", p.pri);
        nonzero(out, " if ", p.inhibit_finish);
    }

    fn gate(&self, out: &mut Vec<u8>, p: &Gate) {
        out.extend_from_slice(b"GA\n");
        self.box_ref(out, " tl ", p.to_loc);
        if p.notify_jumps != 0 {
            self.box_ref(out, " nj ", p.notify_jumps);
        }
        if p.notify_unseal != 0 {
            self.box_ref(out, " nu ", p.notify_unseal);
        }
        nonzero(out, " sk ", p.seal_key);
        nonzero(out, " rh ", p.road_hidden);
    }

    fn misc(&self, out: &mut Vec<u8>, p: &Misc) {
        if *p == Misc::default() {
            return;
        }
        out.extend_from_slice(b"MI\n");
        self.box_ref(out, " sb ", p.summoned_by);
        nonzero(out, " di ", p.npc_dir);
        nonzero(out, " mc ", p.npc_created);
        nonzero(out, " md ", p.mine_delay);
        nonzero(out, " ss ", p.storm_str);
        if p.cmd_allow != 0 {
            out.extend_from_slice(b" ca ");
            out.push(p.cmd_allow);
            out.push(b'\n');
        }
        self.box_ref(out, " gc ", p.garr_castle);
        self.box_ref(out, " mh ", p.npc_home);
        self.box_ref(out, " co ", p.npc_cookie);
        self.box_ref(out, " ov ", p.only_vulnerable);
        self.box_ref(out, " ol ", p.old_lord);
        self.box_ref(out, " bs ", p.bind_storm);
        text(out, " sn ", p.save_name.as_deref());
        text(out, " ds ", p.display.as_deref());
        self.known(out, " nm ", &p.npc_memory);
    }

    /// Prints box `n` (`save_box()`, less the marking).
    fn entity(&self, out: &mut Vec<u8>, n: i32) {
        let b = self.db.entity(n);
        if b.skind.0 != 0 {
            out!(out, "{n} {} {}\n", b.kind.name(), b.skind.name());
        } else {
            out!(out, "{n} {} 0\n", b.kind.name());
        }
        text(out, "na ", b.name.as_deref());

        let items = b
            .items
            .iter()
            .filter(|i| self.db.valid_box(i.item) && i.qty > 0)
            .map(|i| format!("{} {}", i.item, i.qty));
        Self::records(out, "il\t", items);

        // Completed or cleared BUY and SELL trades are dropped; PRODUCE and
        // CONSUME trades are kept even at zero.
        let trades = b
            .trades
            .iter()
            .filter(|t| self.db.valid_box(t.item))
            .filter(|t| !((t.kind == BUY || t.kind == SELL) && t.qty <= 0))
            .map(|t| {
                format!(
                    "{} {} {} {} {} {} {} {}",
                    t.kind, t.item, t.qty, t.cost, t.cloak, t.have_left, t.month_prod, t.expire
                )
            });
        Self::records(out, "tl\t", trades);

        if let Some(d) = &b.disp {
            self.boxlist(out, "an ", &d.neutral);
            self.boxlist(out, "ad ", &d.defend);
            self.boxlist(out, "ah ", &d.hostile);
        }

        self.loc_info(out, &b.loc_info);
        if let Some(p) = &b.char_ {
            self.char_(out, p);
            if let Some(m) = &p.magic {
                self.magic(out, m);
            }
        }
        if let Some(p) = &b.loc {
            self.loc(out, p);
        }
        if let Some(p) = &b.subloc {
            self.subloc(out, p);
        }
        if let Some(p) = &b.item {
            self.item(out, p);
            if let Some(m) = &p.magic {
                self.item_magic(out, m);
            }
        }
        if let Some(p) = &b.player {
            self.player(out, p);
        }
        if let Some(p) = &b.skill {
            self.skill(out, p);
        }
        if let Some(p) = &b.gate {
            self.gate(out, p);
        }
        if let Some(p) = &b.misc {
            self.misc(out, p);
        }
        if let Some(p) = &b.cmd {
            self.command(out, p);
        }
        out.push(b'\n');
    }
}

/// Appends box `n` to `out` and marks it written (`save_box()`). Deleted
/// boxes are skipped.
fn save_box(db: &mut Db, pretty: bool, out: &mut Vec<u8>, n: i32) {
    if db.kind(n) == Kind::DELETED {
        return;
    }
    // loop_known sorts each sparse set in place as it prints it.
    let b = db.entity_mut(n);
    if let Some(m) = b.char_.as_mut().and_then(|c| c.magic.as_mut()) {
        m.visions.sort();
    }
    if let Some(p) = &mut b.player {
        p.known.sort();
    }
    if let Some(m) = &mut b.misc {
        m.npc_memory.sort();
    }
    Printer { db, pretty }.entity(out, n);
    db.entity_mut(n).temp = 1;
}

/// Creates `<libdir>/<name>` and writes what `f` produces into it
/// (`open_write_fp()`); if the file can't be created, reports it and
/// doesn't run `f`.
fn write_file(libdir: &Path, name: &str, f: impl FnOnce(&mut Vec<u8>)) {
    let path = lib_path(libdir, name);
    match File::create(&path) {
        Ok(mut file) => {
            let mut out = Vec::new();
            f(&mut out);
            // fprintf() is unchecked in C.
            let _ = file.write_all(&out);
        }
        Err(err) => eprintln!(
            "open_write_fp: can't open {}: {}",
            path.display(),
            strerror(&err)
        ),
    }
}

/// Writes player `pl` and its units to `fact/<pl>` (`write_player()`).
///
/// # Panics
///
/// Panics if a unit is not a character, as C asserts.
fn write_player(db: &mut Db, pretty: bool, libdir: &Path, pl: i32) {
    write_file(libdir, &format!("fact/{pl}"), |out| {
        save_box(db, pretty, out, pl);
        let units = db
            .entity(pl)
            .player
            .as_ref()
            .map(|p| p.units.as_slice().to_vec())
            .unwrap_or_default();
        for who in units {
            let kind = db.kind(who);
            assert!(
                kind == Kind::CHAR || kind == Kind::DELETED,
                "unit {who} of player {pl} is not a character"
            );
            save_box(db, pretty, out, who);
        }
    });
}

/// Rewrites every entity file and `fact/` (`write_all_boxes()`).
pub fn write_all_boxes(db: &mut Db, pretty: bool, libdir: &Path) {
    let allocated: Vec<i32> = db.allocated().collect();
    for &n in &allocated {
        db.entity_mut(n).temp = 0;
    }

    let fact = lib_path(libdir, "fact");
    let _ = fs::remove_dir_all(&fact);
    make_dir(&fact);

    for (name, kind) in KIND_FILES {
        write_file(libdir, name, |out| {
            for n in db.boxes_of_kind(kind) {
                save_box(db, pretty, out, n);
            }
        });
    }
    for pl in db.boxes_of_kind(Kind::PLAYER) {
        write_player(db, pretty, libdir, pl);
    }
    // write_leftovers(): everything not yet written goes to misc.
    write_file(libdir, "misc", |out| {
        for &n in &allocated {
            if db.entity(n).temp == 0 && db.kind(n) != Kind::DELETED {
                save_box(db, pretty, out, n);
            }
        }
    });
}

/// Writes the index of every box (`write_master()`).
pub fn write_master(db: &mut Db, libdir: &Path) {
    let path = lib_path(libdir, "master");
    let mut file = match File::create(&path) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("can't write {}: {}", path.display(), strerror(&err));
            return;
        }
    };
    let allocated: Vec<i32> = db.allocated().collect();
    for &n in &allocated {
        db.entity_mut(n).temp = 0;
    }

    let mut out = Vec::new();
    for &n in &allocated {
        let kind = db.kind(n);
        let b = db.entity(n);
        let (k, sk) = (b.kind.0, b.skind.0);
        let name = db.name(n);
        let place = match kind {
            Kind::LOC | Kind::ITEM | Kind::SKILL | Kind::GATE | Kind::SHIP | Kind::UNFORM => {
                format!("{}\t", kind.name())
            }
            Kind::PLAYER => format!("fact/{n}"),
            Kind::CHAR => format!("fact/{}", crate::game::player(db, n)),
            _ => continue,
        };
        out!(&mut out, "{n}\t{k}.{sk}\t{place}\t");
        out.extend_from_slice(name);
        out.push(b'\n');
        db.entity_mut(n).temp = 1;
    }
    for &n in &allocated {
        if db.kind(n) != Kind::DELETED && db.entity(n).temp == 0 {
            let b = db.entity(n);
            out!(&mut out, "{n}\t{}.{}\tmisc\t", b.kind.0, b.skind.0);
            out.extend_from_slice(db.name(n));
            out.push(b'\n');
        }
    }
    // fprintf() is unchecked in C.
    let _ = file.write_all(&out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_continue_every_eleven() {
        let mut out = Vec::new();
        Printer::numbers(&mut out, " hl ", 1..=12, 0);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            " hl 1 2 3 4 5 6 7 8 9 \\\n\t10 11 12 \n"
        );
    }

    #[test]
    fn linehash_and_t_string() {
        assert_eq!(linehash(b"na Name"), Some(*b"na"));
        assert_eq!(linehash(b"n"), None);
        assert_eq!(t_string(b"na Name"), b"Name");
        assert_eq!(t_string(b"na "), b"");
    }
}
