// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Allocating entities at numbers drawn from the `mint` stream
//! (`olympia/code.c:new_ent`, `rnd_alloc_num`, `begin_mint`).
//!
//! `mint` is a sequential stream, rooted at the turn:
//! `rng_stream_of(rng_stream_of(root, turn, "turn"), 0, "mint")`. It is
//! seeded the first time it is drawn from in a turn and re-seeded, with its
//! counter back at zero, when the turn number changes. Every allocation
//! draws once per range it tries, so the order of `new_ent` calls fixes
//! every number they hand out. See olympia-g3's `doc/claude/rng-streams.md`.

use crate::entity::{Db, Kind, MAX_BOXES, Subkind};
use crate::game::Game;
use crate::rng::{Stream, Tag};

/// `MTAG_TURN`: the per-turn parent of the `mint` stream.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_MINT`.
const TAG_MINT: Tag = Tag::new(b"mint");

/// `sub_ni`: a noble that stands for an item (`unit_item`).
const SUB_NI: Subkind = Subkind(54);

/// The `mint` stream and the turn it was seeded for (`mint_rng`,
/// `mint_rng_turn`). Unseeded until the first draw.
#[derive(Debug, Clone, Default)]
pub struct Mint {
    seeded: Option<(i16, Stream)>,
}

impl Mint {
    /// The stream for `turn`, seeding it from `master` if it hasn't been
    /// seeded for this turn (`begin_mint()`).
    fn stream(&mut self, master: [u32; 4], turn: i16) -> &mut Stream {
        if self.seeded.as_ref().is_none_or(|&(t, _)| t != turn) {
            let stream = Stream::new(master)
                .child(i32::from(turn), TAG_TURN)
                .child(0, TAG_MINT);
            self.seeded = Some((turn, stream));
        }
        &mut self.seeded.as_mut().expect("seeded above").1
    }

    /// Picks a free box in `low..=high` (`rnd_alloc_num()`): draws a start
    /// point, then takes the first free box at or above it, wrapping around
    /// to `low`. Returns `None` if the range is full.
    fn alloc_num(
        &mut self,
        db: &Db,
        master: [u32; 4],
        turn: i16,
        low: i32,
        high: i32,
    ) -> Option<i32> {
        let n = self.stream(master, turn).draw(low, high);
        (n..=high).chain(low..n).find(|&i| db.get(i).is_none())
    }
}

impl Game {
    /// Allocates a new entity of `kind` and `sk` at a number drawn from the
    /// `mint` stream, and returns the number (`olympia/code.c:new_ent`).
    ///
    /// Each kind has its own number range, tried first: players 50000-56759,
    /// characters 1000-9999 (noble items 79000-149999), cities 56760-58759,
    /// regions 58760-58999, provinces 20000-49999, storms 79000-149999, and
    /// sublocations and everything else 59000-78999. A full range falls back
    /// to others; each range tried takes one draw.
    ///
    /// # Panics
    ///
    /// Panics for [`Kind::SKILL`], which is never allocated this way, and if
    /// every range is full, as C asserts.
    pub fn new_ent(&mut self, kind: Kind, sk: Subkind) -> i32 {
        let master = self.seed.master_seed();
        let turn = self.system.sysclock.turn;
        let db = &self.db;
        let mint = &mut self.mint;
        let mut alloc = |low, high| mint.alloc_num(db, master, turn, low, high);

        let n = match kind {
            Kind::PLAYER => alloc(50000, 56759),
            Kind::CHAR | Kind::UNFORM => {
                let n = if sk == SUB_NI {
                    alloc(79000, MAX_BOXES - 1)
                } else {
                    alloc(1000, 9999)
                };
                n.or_else(|| alloc(59000, 78999))
                    .or_else(|| alloc(79000, MAX_BOXES - 1))
            }
            Kind::SKILL => panic!("new_ent: skills are not allocated"),
            Kind::LOC => match sk {
                Subkind::CITY => alloc(56760, 58759),
                Subkind::REGION => alloc(58760, 58999),
                Subkind::UNDER
                | Subkind::FOREST
                | Subkind::OCEAN
                | Subkind::CLOUD
                | Subkind::TUNNEL => alloc(20000, 49999),
                _ => alloc(59000, 78999),
            },
            Kind::STORM => alloc(79000, MAX_BOXES - 1),
            _ => alloc(59000, 78999),
        };
        let n = n.or_else(|| alloc(79000, MAX_BOXES - 1)).unwrap_or(-1);

        self.db.alloc_box(n, kind, sk);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // The expected numbers come from olympia-g3's `rnd_alloc_num` and
    // `new_ent`, run over `lib/rng.c` with this master seed and an empty
    // table: code.c's functions compiled against lib/rng.c and lib/rnd.c.
    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];

    fn game(turn: i16) -> Game {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new(MASTER);
        g.system.sysclock.turn = turn;
        g
    }

    #[test]
    fn each_kind_draws_from_its_range_in_stream_order() {
        let mut g = game(3);
        let got = [
            g.new_ent(Kind::PLAYER, Subkind::PL_REGULAR),
            g.new_ent(Kind::CHAR, Subkind::default()),
            g.new_ent(Kind::CHAR, SUB_NI),
            g.new_ent(Kind::LOC, Subkind::CITY),
            g.new_ent(Kind::LOC, Subkind::REGION),
            g.new_ent(Kind::LOC, Subkind::OCEAN),
            g.new_ent(Kind::LOC, Subkind::CASTLE),
            g.new_ent(Kind::STORM, Subkind(59)),
            g.new_ent(Kind::SHIP, Subkind::GALLEY),
        ];
        assert_eq!(
            got,
            [
                52278, 8319, 121_080, 57107, 58810, 31885, 63663, 81805, 65051
            ]
        );
        assert_eq!(g.db.kind(8319), Kind::CHAR);
        assert_eq!(g.db.boxes_of_kind(Kind::LOC), [31885, 57107, 58810, 63663]);
    }

    #[test]
    fn a_full_range_falls_back_to_the_storm_range() {
        let mut g = game(4);
        // Left by the turn 3 run above; the only one in the region range.
        g.db.alloc_box(58810, Kind::LOC, Subkind::REGION);
        let got: Vec<i32> = (0..242)
            .map(|_| g.new_ent(Kind::LOC, Subkind::REGION))
            .collect();
        assert_eq!(got[..4], [58839, 58805, 58982, 58886]);
        assert_eq!(got[238..], [58974, 123_837, 107_558, 101_101]);
    }

    #[test]
    fn the_stream_restarts_each_turn() {
        let mut g = game(1);
        assert_eq!(g.new_ent(Kind::CHAR, Subkind::default()), 8323);
        g.system.sysclock.turn = 2;
        assert_eq!(g.new_ent(Kind::CHAR, Subkind::default()), 9449);
    }

    #[test]
    fn a_full_player_range_takes_a_second_draw() {
        let mut g = game(5);
        for i in 50000..=56759 {
            g.db.alloc_box(i, Kind::ITEM, Subkind::default());
        }
        // The first draw of turn 5 is spent on the player range, the second
        // picks the storm-range box.
        assert_eq!(g.new_ent(Kind::PLAYER, Subkind::PL_REGULAR), 123_115);
    }
}
