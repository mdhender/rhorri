// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The game state and the database load and save flows
//! (`olympia/io.c:load_db`, `save_db`).

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::dir::ExitDir;
use crate::entity::{Db, Kind, MAX_BOXES};
use crate::hades::HadesRng;
use crate::input::Driver;
use crate::io;
use crate::mint::Mint;
use crate::npc::NpcRng;
use crate::order;
use crate::os::lib_path;
use crate::rnd::{LegacyRng, SaveSeedError};
use crate::storm::Weather;
use crate::system::System;
use crate::trade_route::TradeRoutes;

/// The relics `create_relics()` makes if they don't exist yet.
const RELICS: [i32; 3] = [401, 402, 403];

/// A step of the C engine that rhorri can't run yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotYetImplemented(pub String);

impl fmt::Display for NotYetImplemented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not yet implemented: {}", self.0)
    }
}

impl Error for NotYetImplemented {}

/// Everything a run of the engine works on.
#[derive(Debug)]
pub struct Game {
    /// The database directory (`-l`, default `lib`).
    pub libdir: PathBuf,
    /// `-p`: follow box references in the data files with names.
    pub pretty_data_files: bool,
    /// `test-use-const-report-date`: a fixed Times date.
    pub test_use_const_report_date: bool,
    /// `-G`: a trade-route seed for a new game.
    pub trade_route_seed: Option<u64>,
    pub db: Db,
    pub system: System,
    pub seed: LegacyRng,
    pub trade_routes: TradeRoutes,
    /// The `wait` command's index in the command table (`cmd_wait`).
    pub cmd_wait: i32,
    /// How many day dots `print_dot()` has printed (`dot_count`).
    pub dot_count: i32,
    /// Set once the turn's days have run (`month_done`).
    pub month_done: bool,
    /// The `mint` stream entity numbers are drawn from; see
    /// [`new_ent`](Self::new_ent).
    pub mint: Mint,
    /// The command driver's queues and lists; see [`crate::input`].
    pub driver: Driver,
    /// The `wthr` stream and the weather-day schedule; see
    /// [`crate::storm`].
    pub weather: Weather,
    /// The `exdr` stream; see [`crate::dir`].
    pub exitdir: ExitDir,
    /// The `npcs` stream as last seeded; see [`crate::npc`].
    pub npc_rng: NpcRng,
    /// The `hads` turn stream; see [`crate::hades`].
    pub hades: HadesRng,
    /// The savage units in the world, as `init_savage_attacks` last counted
    /// them (`olympia/savage.c:num_savages`).
    pub num_savages: i32,
}

impl Game {
    /// A game with nothing loaded, reading from `libdir`.
    #[must_use]
    pub fn new(libdir: PathBuf) -> Self {
        Self {
            libdir,
            pretty_data_files: false,
            test_use_const_report_date: false,
            trade_route_seed: None,
            db: Db::new(),
            system: System::default(),
            seed: LegacyRng::new([0; 4]),
            trade_routes: TradeRoutes::default(),
            cmd_wait: 0,
            dot_count: 0,
            month_done: false,
            mint: Mint::default(),
            driver: Driver::default(),
            weather: Weather::default(),
            exitdir: ExitDir::default(),
            npc_rng: NpcRng::default(),
            hades: HadesRng::default(),
            num_savages: 0,
        }
    }

    /// `<libdir>/<name>`, joined as C's `sout("%s/%s")` does.
    #[must_use]
    pub fn lib_path(&self, name: &str) -> PathBuf {
        lib_path(&self.libdir, name)
    }

    /// Loads the database (`load_db()`).
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if the database needs an
    /// initialization step that isn't ported yet.
    pub fn load_db(&mut self) -> Result<(), NotYetImplemented> {
        stage("load_db()");

        self.seed = LegacyRng::load(&self.lib_path("randseed"));
        self.trade_routes = TradeRoutes::init(
            &self.lib_path("trade_routes"),
            &self.lib_path("trade-route-seed"),
            self.trade_route_seed,
        );
        self.system.load(&self.lib_path("system"));

        let libdir = Path::new(&self.libdir);
        if !io::fast_scan(&mut self.db, libdir) {
            io::scan_all_boxes(&mut self.db, libdir);
        }
        io::read_all_boxes(&mut self.db, libdir);
        let orders = self.lib_path("orders");
        order::load_orders(&mut self.db, &orders);

        self.check_db()?;
        // determine_map_edges() only computes map routines' bounds; nothing
        // ported uses them yet.

        let s = &self.system;
        let missing = [
            (s.post_has_been_run == 0, "post_production"),
            (s.seed_has_been_run == 0, "seed_initial_locations"),
            (s.faery_region == 0, "create_faery"),
            (s.hades_region == 0, "create_hades"),
            (s.nowhere_region == 0, "create_nowhere"),
            (s.cloud_region == 0, "create_cloudlands"),
            (s.tunnel_region == 0, "create_tunnels"),
            (s.combat_pl == 0, "create combat player"),
            (
                RELICS.iter().any(|&n| self.db.get(n).is_none()),
                "create_relics",
            ),
        ];
        if let Some((_, step)) = missing.iter().find(|(needed, _)| *needed) {
            return Err(NotYetImplemented((*step).to_owned()));
        }
        // init_ocean_chars() builds an in-memory list nothing ported uses.

        self.delete_deadchars();
        self.print_box_usage();
        Ok(())
    }

    /// Deletes dead characters left from the last turn
    /// (`delete_deadchars()`).
    fn delete_deadchars(&mut self) {
        for i in self.db.boxes_of_kind(Kind::DEADCHAR) {
            if self.db.loc(i) != 0 {
                self.db.set_where(i, 0);
            }
            self.db.delete_box(i);
        }
    }

    /// Prints how full each range of entity numbers is
    /// (`print_box_usage()`).
    fn print_box_usage(&self) {
        eprintln!("entity space usage:");
        let ranges = [
            (1000, 9999, "char"),
            (20000, 59999, "locs"),
            (50000, 56759, "play"),
            (56760, 58759, "lloc"),
            (59000, 78999, "subl"),
            (79900, MAX_BOXES - 1, "rest"),
        ];
        for (low, high, s) in ranges {
            let used = (low..=high).filter(|&i| self.db.get(i).is_some()).count() as i32;
            eprintln!(
                "\t{low:5} - {high:5}  {s:>4}  {used}/{} used ({}%)",
                high - low,
                used * 100 / (high - low)
            );
        }
    }

    /// Saves the database (`save_db()`).
    ///
    /// # Errors
    ///
    /// Returns [`SaveSeedError`] if `randseed` can't be written. That is
    /// fatal: `save_seed()` is the first write, so nothing has been saved,
    /// and the caller must report it and exit 1.
    pub fn save_db(&mut self) -> Result<(), SaveSeedError> {
        stage("save_db()");
        self.cleanup_posts();
        self.seed.save(&self.lib_path("randseed"))?;
        self.trade_routes.save(&self.lib_path("trade_routes"));
        self.system.save(&self.lib_path("system"));
        let libdir = PathBuf::from(&self.libdir);
        io::write_all_boxes(&mut self.db, self.pretty_data_files, &libdir);
        io::write_master(&mut self.db, &libdir);
        order::save_orders(&self.db, &self.lib_path("orders"));
        // rename_act_join_files() renames the join files of players added
        // this run; adding players (-a) isn't ported, so there are none.
        Ok(())
    }

    /// Removes this turn's posted signs (`cleanup_posts()`).
    fn cleanup_posts(&mut self) {
        for i in self.db.boxes_of_kind(Kind::POST) {
            self.db.set_where(i, 0);
            self.db.delete_box(i);
        }
    }
}

/// `stage()`: names the phase on standard error.
pub fn stage(s: &str) {
    eprintln!("{s}");
}

/// `lord()`: who a unit answers to; a player is its own lord.
#[must_use]
pub fn lord(db: &Db, n: i32) -> i32 {
    if db.kind(n) == Kind::PLAYER {
        return n;
    }
    db.get(n)
        .and_then(|b| b.char_.as_ref())
        .map_or(0, |c| c.unit_lord)
}

/// `player()`: the faction a unit belongs to, following lords up.
///
/// # Panics
///
/// Panics on a loop of lords, as C asserts.
#[must_use]
pub fn player(db: &Db, mut n: i32) -> i32 {
    let mut loop_check = 5000;
    while n != 0 && db.kind(n) != Kind::PLAYER {
        n = lord(db, n);
        assert!(loop_check > 0, "player(): loop of lords");
        loop_check -= 1;
    }
    n
}
