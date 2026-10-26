// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Exits from a province, and the `exdr` stream that orders them for NPC
//! movement (`olympia/dir.c`).
//!
//! `exdr` is one sequential stream per turn, rooted at the turn:
//! `rng_stream_of(rng_stream_of(root, turn, "turn"), 0, "exdr")`. It is
//! seeded the first time a randomized exit list is drawn in a turn
//! (`begin_exitdir()`), and its counter runs across the whole turn: every
//! [`exits_from_loc_nsew_select`](Game::exits_from_loc_nsew_select) with
//! `rand` set shuffles from it in turn. See olympia-g3's
//! `doc/claude/rng-streams.md`.
//!
//! `exits_from_loc` lists every exit, including the ways into and out of
//! sublocations, buildings and ships, and hidden roads; NPC movement takes
//! only a province's exits in the six directions (`province_exits`).

use crate::entity::{Db, Kind, Subkind};
use crate::game::Game;
use crate::loc::{
    LOC_BUILD, LOC_PROVINCE, LOC_SUBLOC, here_list, is_loc_or_ship, is_ship, is_ship_notdone,
    loc_depth, loc_hidden, loc_link_open, province, region,
};
use crate::lore::test_known;
use crate::rng::{Stream, Tag};

/// `EXTAG_TURN`: the per-turn parent of the `exdr` stream.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_EXITDIR`.
const TAG_EXITDIR: Tag = Tag::new(b"exdr");

/// `DIR_N`.
pub const DIR_N: i32 = 1;
/// `DIR_E`.
pub const DIR_E: i32 = 2;
/// `DIR_S`.
pub const DIR_S: i32 = 3;
/// `DIR_W`.
pub const DIR_W: i32 = 4;
/// `DIR_UP`.
pub const DIR_UP: i32 = 5;
/// `DIR_DOWN`.
pub const DIR_DOWN: i32 = 6;
/// `DIR_IN`.
pub const DIR_IN: i32 = 7;
/// `DIR_OUT`.
pub const DIR_OUT: i32 = 8;

/// `olympia/glob.c:full_dir_s`: the directions' names.
pub const FULL_DIR_S: [&str; 9] = [
    "<no dir>", "north", "east", "south", "west", "up", "down", "in", "out",
];

/// `LAND`: select the land exits.
pub const LAND: i32 = 1;
/// `WATER`: select the water exits.
pub const WATER: i32 = 2;

/// An exit from a location (`struct exit_view`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExitView {
    /// Which direction the exit goes (`DIR_xxx`), or 0.
    pub direction: i32,
    /// Where the exit goes.
    pub destination: i32,
    /// The location it leaves from.
    pub orig: i32,
    /// How far, in days.
    pub distance: i32,
    /// Not possible to go there.
    pub impassable: bool,
    /// The destination is hidden.
    pub dest_hidden: bool,
    /// The origin, or the road, is hidden.
    pub orig_hidden: bool,
    /// A hidden destination unknown to the traveller.
    pub hidden: bool,
    /// The region the destination is in, if it is a different one.
    pub inside: i32,
    /// The road, if this is a road.
    pub road: i32,
    /// A water link.
    pub water: bool,
    /// A link to a ship that is moving.
    pub in_transit: bool,
    /// A magical barrier prevents travel.
    pub magic_barrier: bool,
    /// The Gate Spirit of Hades' fee to enter.
    pub hades_cost: i32,
}

/// The `exdr` stream and the turn it was seeded for (`exitdir_rng`,
/// `exitdir_rng_turn`). Unseeded until the first draw.
#[derive(Debug, Clone, Default)]
pub struct ExitDir {
    seeded: Option<(i16, Stream)>,
}

impl ExitDir {
    /// The stream for `turn`, seeding it from `master` if it hasn't been
    /// seeded for this turn (`begin_exitdir()`).
    fn stream(&mut self, master: [u32; 4], turn: i16) -> &mut Stream {
        if self.seeded.as_ref().is_none_or(|&(t, _)| t != turn) {
            let stream = Stream::new(master)
                .child(i32::from(turn), TAG_TURN)
                .child(0, TAG_EXITDIR);
            self.seeded = Some((turn, stream));
        }
        &mut self.seeded.as_mut().expect("seeded above").1
    }
}

/// `is_ship_either()`: a ship, finished or not.
fn is_ship_either(db: &Db, n: i32) -> bool {
    is_ship(db, n) || is_ship_notdone(db, n)
}

/// `uldim()`: the Uldim mountains flag of a location.
fn uldim(db: &Db, n: i32) -> i8 {
    db.get(n)
        .and_then(|b| b.subloc.as_ref())
        .map_or(0, |s| s.uldim_flag)
}

/// `summerbridge()`.
fn summerbridge(db: &Db, n: i32) -> i8 {
    db.get(n)
        .and_then(|b| b.subloc.as_ref())
        .map_or(0, |s| s.summer_flag)
}

/// `loc_sea_lane()`.
fn loc_sea_lane(db: &Db, n: i32) -> bool {
    db.get(n)
        .and_then(|b| b.loc.as_ref())
        .is_some_and(|l| l.sea_lane != 0)
}

/// `loc_barrier()`.
fn loc_barrier(db: &Db, n: i32) -> bool {
    db.get(n)
        .and_then(|b| b.loc.as_ref())
        .is_some_and(|l| l.barrier != 0)
}

/// `location_direction()`: the province next to `where_` in direction
/// `dir`, or 0.
#[must_use]
pub fn location_direction(db: &Db, where_: i32, dir: i32) -> i32 {
    let Some(p) = db.get(where_).and_then(|b| b.loc.as_ref()) else {
        return 0;
    };
    usize::try_from(dir - 1)
        .ok()
        .and_then(|i| p.prov_dest.as_slice().get(i))
        .copied()
        .unwrap_or(0)
}

/// `exit_distance()`: how many days it takes to go from `loc1` to `loc2`.
///
/// # Panics
///
/// Panics if the destination's terrain has no travel time, as C asserts
/// (after printing it on standard error).
#[must_use]
pub fn exit_distance(db: &Db, loc1: i32, loc2: i32) -> i32 {
    if db.subkind(loc1) == Subkind::HADES_PIT || db.subkind(loc2) == Subkind::HADES_PIT {
        return 28;
    }

    let (mut loc1, mut loc2) = if loc_depth(db, loc1) > loc_depth(db, loc2) {
        (loc2, loc1)
    } else {
        (loc1, loc2)
    };

    let w_d = loc_depth(db, loc1);
    let d_d = loc_depth(db, loc2);

    if d_d == LOC_BUILD {
        return 0;
    }
    if d_d == LOC_SUBLOC {
        return 1;
    }

    // Water-land links are distance 2.
    let ocean1 = db.subkind(loc1) == Subkind::OCEAN;
    let ocean2 = db.subkind(loc2) == Subkind::OCEAN;
    if ocean1 != ocean2 {
        return 2;
    }

    // Linked sublocations between regions.
    if province(db, loc1) != province(db, loc2) {
        loc1 = province(db, loc1);
        loc2 = province(db, loc2);
    }

    match db.subkind(loc2) {
        Subkind::OCEAN => {
            if loc_sea_lane(db, loc1) && loc_sea_lane(db, loc2) {
                2
            } else {
                3
            }
        }
        Subkind::MOUNTAIN => 10,
        Subkind::FOREST | Subkind::DESERT => 8,
        Subkind::SWAMP => 14,
        Subkind::PLAIN | Subkind::UNDER | Subkind::CLOUD => 7,
        Subkind::TUNNEL | Subkind::CHAMBER => 5,
        sk => {
            eprintln!(
                "exit_distance: subkind={}, loc1={loc1}, loc2={loc2}, w_d={w_d}, d_d={d_d}",
                sk.name()
            );
            panic!("exit_distance: no travel time");
        }
    }
}

/// `is_port_city()`: a city, not in the mountains, next to the ocean.
///
/// # Panics
///
/// Panics on a city that isn't a sublocation, as C asserts.
#[must_use]
pub fn is_port_city(db: &Db, where_: i32) -> bool {
    if db.subkind(where_) != Subkind::CITY {
        return false;
    }
    assert_eq!(loc_depth(db, where_), LOC_SUBLOC, "is_port_city: {where_}");

    let p = province(db, where_);
    if db.subkind(p) == Subkind::MOUNTAIN {
        return false;
    }

    [DIR_N, DIR_S, DIR_E, DIR_W].into_iter().any(|dir| {
        let n = location_direction(db, p, dir);
        n != 0 && db.subkind(n) == Subkind::OCEAN
    })
}

/// `province_has_port_city()`: the province's port city, or 0.
///
/// # Panics
///
/// Panics if `where_` isn't a province, as C asserts.
#[must_use]
pub fn province_has_port_city(db: &Db, where_: i32) -> i32 {
    assert_eq!(
        loc_depth(db, where_),
        LOC_PROVINCE,
        "province_has_port_city: {where_}"
    );
    here_list(db, where_)
        .into_iter()
        .find(|&i| db.subkind(i) == Subkind::CITY && is_port_city(db, i))
        .unwrap_or(0)
}

impl Game {
    /// `in_hades()`.
    pub(crate) fn in_hades(&self, n: i32) -> bool {
        region(&self.db, n) == self.system.hades_region
    }

    /// `summer_uldim_open_now()`: whether the Uldim pass and Summerbridge
    /// are open: months 3 to 6, and month 2 once the month is done.
    fn summer_uldim_open_now(&self) -> bool {
        let month = self.system.sysclock.month();
        (3..=6).contains(&month) || (month == 2 && self.month_done)
    }

    /// `ship_gone()`: how long a ship has been away, or 0.
    fn ship_gone(&self, n: i32) -> i32 {
        let moving = self
            .db
            .get(n)
            .and_then(|b| b.subloc.as_ref())
            .map_or(0, |s| s.moving);
        if moving == 0 {
            return 0;
        }
        self.system.sysclock.days_since_epoch - moving + i32::from(self.driver.evening)
    }

    /// `add_province_exit()`: appends the exit from `where_` to `dest` in
    /// direction `dir`, as `who` sees it, to `l`. From the ocean to a
    /// province with a port city, the city's exit is appended first.
    ///
    /// # Panics
    ///
    /// Panics if `dest` isn't a valid box, as C asserts.
    fn add_province_exit(&self, who: i32, where_: i32, dest: i32, dir: i32, l: &mut Vec<ExitView>) {
        let db = &self.db;
        assert!(db.valid_box(dest), "add_province_exit: invalid box {dest}");

        let mut v = ExitView::default();
        let where_sk = db.subkind(where_);
        let dest_sk = db.subkind(dest);

        if (is_ship_either(db, where_) && self.ship_gone(where_) != 0)
            || (is_ship_either(db, dest) && self.ship_gone(dest) != 0)
        {
            v.in_transit = true;
        }

        if is_ship_either(db, where_) && dest_sk == Subkind::OCEAN {
            v.impassable = true;
        }

        if where_sk == Subkind::OCEAN && !is_ship_either(db, dest) {
            v.water = true;
        }

        if dest_sk == Subkind::OCEAN {
            v.water = true;
        }

        // Land to water, from a province with a port city: impassable.
        if loc_depth(db, where_) == LOC_PROVINCE
            && dest_sk == Subkind::OCEAN
            && province_has_port_city(db, where_) != 0
        {
            v.impassable = true;
        }

        // Can't go into collapsed mines.
        if dest_sk == Subkind::MINE_COLLAPSED {
            v.impassable = true;
        }

        // Water to land, where the land has a port city: the city is the
        // way in.
        if where_sk == Subkind::OCEAN
            && dest_sk != Subkind::OCEAN
            && dest_sk != Subkind::MOUNTAIN
            && loc_depth(db, dest) == LOC_PROVINCE
        {
            let n = province_has_port_city(db, dest);
            if n != 0 {
                v.impassable = true;
                self.add_province_exit(who, where_, n, dir, l);
            }
        }

        // Water and mountains don't meet.
        if (where_sk == Subkind::MOUNTAIN && dest_sk == Subkind::OCEAN)
            || (where_sk == Subkind::OCEAN && dest_sk == Subkind::MOUNTAIN)
        {
            v.impassable = true;
        }

        // Surface to cloud: impassable, except by flying.
        if (dir == DIR_UP || dir == DIR_DOWN)
            && (where_sk == Subkind::CLOUD || dest_sk == Subkind::CLOUD)
        {
            v.impassable = true;
        }

        // The Uldim mountains.
        if (dir == DIR_N && uldim(db, where_) == 1) || (dir == DIR_S && uldim(db, where_) == 2) {
            v.impassable = true;
        }

        // The Uldim pass and Summerbridge are open part of the year.
        if ((dir == DIR_N && (uldim(db, where_) == 4 || summerbridge(db, where_) == 1))
            || (dir == DIR_S && (uldim(db, where_) == 3 || summerbridge(db, where_) == 2)))
            && !self.summer_uldim_open_now()
        {
            v.impassable = true;
        }

        v.orig = where_;
        v.destination = dest;
        v.direction = dir;
        v.distance = exit_distance(db, where_, dest);

        if loc_hidden(db, where_) {
            v.orig_hidden = true;
        }
        if loc_hidden(db, dest) {
            v.dest_hidden = true;
        }

        // Out routes are never hidden: a character may have poofed into a
        // building, and must be able to leave.
        if loc_hidden(db, dest) && !test_known(db, who, dest) && dir != DIR_OUT {
            v.hidden = true;
        }

        if region(db, where_) != region(db, dest) {
            v.inside = region(db, dest);
            if !self.in_hades(where_) && self.in_hades(dest) {
                v.hades_cost = 100;
            }
        }

        // A magical barrier keeps travellers out.
        if loc_barrier(db, dest) && dir != DIR_OUT {
            v.impassable = true;
            v.magic_barrier = true;
        }

        l.push(v);
    }

    /// `province_exits()`: the exits from a province in each direction,
    /// north through down.
    fn province_exits(&self, who: i32, where_: i32, l: &mut Vec<ExitView>) {
        for dir in 1..=DIR_DOWN {
            let n = location_direction(&self.db, where_, dir);
            if n != 0 {
                self.add_province_exit(who, where_, n, dir, l);
            }
        }
    }

    /// `province_sub_exits()`: the ways into what is inside a province,
    /// and the open links into it.
    fn province_sub_exits(&self, who: i32, where_: i32, l: &mut Vec<ExitView>) {
        for i in here_list(&self.db, where_) {
            if is_loc_or_ship(&self.db, i) {
                self.add_province_exit(who, where_, i, DIR_IN, l);
            }
        }
        let link_from = self
            .db
            .entity(where_)
            .subloc
            .as_ref()
            .map(|p| p.link_from.as_slice().to_vec());
        for i in link_from.unwrap_or_default() {
            if loc_link_open(&self.db, i) != 0 {
                self.add_province_exit(who, where_, i, DIR_IN, l);
            }
        }
    }

    /// `subloc_exits()`: from a sublocation, out to sea from a port city,
    /// in to what it holds, out to where it is, and along its open links.
    fn subloc_exits(&self, who: i32, where_: i32, l: &mut Vec<ExitView>) {
        let db = &self.db;
        if is_port_city(db, where_) {
            let p = province(db, where_);
            for dir in DIR_N..=DIR_W {
                let n = location_direction(db, p, dir);
                if n != 0 && db.subkind(n) == Subkind::OCEAN {
                    self.add_province_exit(who, where_, n, dir, l);
                }
            }
        }
        for i in here_list(db, where_) {
            if is_loc_or_ship(db, i) {
                self.add_province_exit(who, where_, i, DIR_IN, l);
            }
        }
        self.add_province_exit(who, where_, db.loc(where_), DIR_OUT, l);

        if loc_link_open(db, where_) != 0 {
            let link_to = db
                .entity(where_)
                .subloc
                .as_ref()
                .map(|p| p.link_to.as_slice().to_vec());
            for i in link_to.unwrap_or_default() {
                self.add_province_exit(who, where_, i, 0, l);
            }
        }
    }

    /// `ship_exits()`: from a ship, out to where it is, and over to every
    /// other ship there, even one that has left (combat needs the link).
    fn ship_exits(&self, who: i32, ship: i32, l: &mut Vec<ExitView>) {
        let db = &self.db;
        assert!(is_ship_either(db, ship), "ship_exits: {ship} is not a ship");
        let outer_loc = db.loc(ship);
        self.add_province_exit(who, ship, outer_loc, DIR_OUT, l);
        for i in here_list(db, outer_loc) {
            if i != ship && is_ship_either(db, i) {
                self.add_province_exit(who, ship, i, 0, l);
            }
        }
    }

    /// `building_exits()`: out of a building or ship, and in to what it
    /// holds.
    fn building_exits(&self, who: i32, where_: i32, l: &mut Vec<ExitView>) {
        let db = &self.db;
        if is_ship_either(db, where_) {
            self.ship_exits(who, where_, l);
        } else {
            self.add_province_exit(who, where_, db.loc(where_), DIR_OUT, l);
        }
        for i in here_list(db, where_) {
            if is_loc_or_ship(db, i) {
                self.add_province_exit(who, where_, i, DIR_IN, l);
            }
        }
    }

    /// `extra_routes()`: the roads out of `where_`, hidden ones included.
    fn extra_routes(&self, who: i32, where_: i32, l: &mut Vec<ExitView>) {
        let db = &self.db;
        for i in here_list(db, where_) {
            if db.kind(i) != Kind::ROAD {
                continue;
            }
            let gate = db.entity(i).gate.as_deref();
            let dest = gate.map_or(0, |g| g.to_loc);
            assert!(db.valid_box(dest), "extra_routes: road {i} to {dest}");
            let road_hidden = gate.is_some_and(|g| g.road_hidden != 0);
            let (where_sk, dest_sk) = (db.subkind(where_), db.subkind(dest));

            let mut v = ExitView {
                orig: where_,
                destination: dest,
                distance: exit_distance(db, where_, dest),
                road: i,
                ..ExitView::default()
            };
            // Surface-cloud links are impassable (except by flying).
            if (where_sk == Subkind::MOUNTAIN && dest_sk == Subkind::CLOUD)
                || (where_sk == Subkind::CLOUD && dest_sk == Subkind::MOUNTAIN)
            {
                v.impassable = true;
            }
            if road_hidden {
                v.orig_hidden = true;
                v.dest_hidden = true;
                if !test_known(db, who, i) {
                    v.hidden = true;
                }
            }
            if region(db, where_) != region(db, dest) {
                v.inside = region(db, dest);
            }
            if where_sk == Subkind::OCEAN || dest_sk == Subkind::OCEAN {
                v.water = true;
            }
            l.push(v);
        }
    }

    /// `exits_from_loc()`: every exit from `where_`, as `who` sees it.
    ///
    /// # Panics
    ///
    /// Panics if `where_` is a region, as C asserts.
    #[must_use]
    pub fn exits_from_loc(&self, who: i32, where_: i32) -> Vec<ExitView> {
        let mut l = Vec::new();
        match loc_depth(&self.db, where_) {
            LOC_PROVINCE => {
                self.province_exits(who, where_, &mut l);
                self.province_sub_exits(who, where_, &mut l);
            }
            LOC_SUBLOC => self.subloc_exits(who, where_, &mut l),
            LOC_BUILD => {
                self.province_exits(who, where_, &mut l);
                self.building_exits(who, where_, &mut l);
            }
            depth => {
                eprintln!("where={where_}, depth={depth}");
                panic!("exits_from_loc: {where_} is at depth {depth}");
            }
        }
        self.extra_routes(who, where_, &mut l);
        l
    }

    /// `has_ocean_access()`: 0 if `where_` has no water exit, 1 if its
    /// only water exits are impassable, and 2 if one is passable (though
    /// perhaps hidden).
    #[must_use]
    pub fn has_ocean_access(&self, where_: i32) -> i32 {
        let mut ret = 0;
        for v in self.exits_from_loc(0, where_).iter().filter(|v| v.water) {
            if !v.impassable {
                ret = 2;
            } else if ret == 0 {
                ret = 1;
            }
        }
        ret
    }

    /// `exits_from_loc_nsew()`: a province's exits in the six directions,
    /// as `who` sees them; none from anywhere else.
    #[must_use]
    pub fn exits_from_loc_nsew(&self, who: i32, where_: i32) -> Vec<ExitView> {
        let mut l = Vec::new();
        if loc_depth(&self.db, where_) == LOC_PROVINCE {
            self.province_exits(who, where_, &mut l);
        }
        l
    }

    /// `exits_from_loc_nsew_select()`: the province exits that are land
    /// (`land & LAND`) or water (`land & WATER`) links, shuffled on the
    /// `exdr` stream if `rand` is set. None from anywhere but a province.
    pub fn exits_from_loc_nsew_select(
        &mut self,
        who: i32,
        where_: i32,
        land: i32,
        rand: bool,
    ) -> Vec<ExitView> {
        if loc_depth(&self.db, where_) != LOC_PROVINCE {
            return Vec::new();
        }
        let mut ret: Vec<ExitView> = self
            .exits_from_loc_nsew(who, where_)
            .into_iter()
            .filter(|e| ((land & LAND) != 0 && !e.water) || ((land & WATER) != 0 && e.water))
            .collect();
        if rand {
            let master = self.seed.master_seed();
            let turn = self.system.sysclock.turn;
            self.exitdir.stream(master, turn).shuffle(&mut ret);
        }
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Kind;
    use std::path::PathBuf;

    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];

    #[test]
    fn location_direction_reads_prov_dest() {
        let mut db = Db::new();
        db.alloc_box(10101, Kind::LOC, Subkind::PLAIN);
        for n in [10001, 10102, 10201, 10100] {
            db.alloc_box(n, Kind::LOC, Subkind::FOREST);
        }
        let p = &mut db.entity_mut(10101).loc;
        let p = p.get_or_insert_with(Box::default);
        for n in [10001, 10102, 10201, 0] {
            p.prov_dest.append(n);
        }
        assert_eq!(location_direction(&db, 10101, DIR_N), 10001);
        assert_eq!(location_direction(&db, 10101, DIR_S), 10201);
        assert_eq!(location_direction(&db, 10101, DIR_W), 0);
        assert_eq!(location_direction(&db, 10101, DIR_UP), 0);
        assert_eq!(location_direction(&db, 10102, DIR_N), 0);
    }

    /// The `exdr` stream for turn 3 under `MASTER`, as C's
    /// `begin_exitdir()` seeds it, shuffling a five-exit list and then a
    /// three-exit one: the values come from `exit_views_shuffle_rng` over
    /// lib/rng.c.
    #[test]
    fn exitdir_shuffles_run_on_across_the_turn() {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new(MASTER);
        g.system.sysclock.turn = 3;
        let mut a: Vec<i32> = (1..=5).collect();
        let mut b: Vec<i32> = (1..=3).collect();
        let (master, turn) = (g.seed.master_seed(), g.system.sysclock.turn);
        g.exitdir.stream(master, turn).shuffle(&mut a);
        g.exitdir.stream(master, turn).shuffle(&mut b);
        assert_eq!(a, EXDR_A);
        assert_eq!(b, EXDR_B);
    }

    const EXDR_A: [i32; 5] = [1, 5, 2, 4, 3];
    const EXDR_B: [i32; 3] = [1, 2, 3];
}
