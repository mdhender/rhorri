// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Markets: matching buy and sell trades, the month-end city restock, and
//! the `econ` stream (`olympia/buy.c`).
//!
//! A trade is a unit's or a city's standing offer to buy or sell an item.
//! A unit's trades are matched against the others' in the same market
//! when it enters a trade (the BUY and SELL commands, stage 4), when it is
//! given something that might fill one (`investigate_possible_trade`, then
//! [`check_validated_trades`](Game::check_validated_trades) after the
//! command), and in every market on day 1 of a turn
//! ([`match_all_trades`](Game::match_all_trades)). Cities restock their
//! offers at month end ([`location_trades`](Game::location_trades)).
//!
//! The `econ` stream is one stream per market per turn:
//! `rng_stream_of(rng_stream_of(root, turn, "turn"), where, "econ")`,
//! reseeded for each market (`begin_economy()`). Every draw on it is a
//! keyed leaf on `(item, where, purpose)`, so no draw can shift another.
//! See olympia-g3's `doc/claude/rng-streams.md`.
//!
//! TODO(stage 2): the "Bought ..."/"Sold ..." lines to the traders and the
//! market, the opium `log_write()`, and the GM report's trade income
//! (`gold_pot_basket`, `gold_opium`, `gold_trade`) are report output.
//! FIND BUY/SELL and city trade seeding (the other `econ` draws) come with
//! stage 4 and stage 5.

use crate::entity::{BUY, CONSUME, Db, Kind, NUM_MONTHS, PRODUCE, SELL, Subkind, Trade};
use crate::game::{Game, stage};
use crate::loc::{all_char_here, is_ship, region, subloc};
use crate::rng::{Stream, Tag};
use crate::turn::Phase;
use crate::unit::{ITEM_GOLD, char_moving, has_item, is_prisoner, item_unique};

/// `TAG_TURN`: the per-turn parent of the `econ` stream.
const TAG_TURN: Tag = Tag::new(b"turn");
/// `TAG_ECONOMY`.
const TAG_ECONOMY: Tag = Tag::new(b"econ");
/// `ETAG_PICK`: choose one of several goods.
const ETAG_PICK: Tag = Tag::new(b"pick");
/// `ETAG_STOCK`: does the market carry a good.
const ETAG_STOCK: Tag = Tag::new(b"stok");
/// `ETAG_QTY`: quantity offered.
const ETAG_QTY: Tag = Tag::new(b"qnty");
/// `ETAG_COST`: price.
const ETAG_COST: Tag = Tag::new(b"cost");
/// `ETAG_EXPIRE`: expiry countdown.
const ETAG_EXPIRE: Tag = Tag::new(b"expr");
/// `ETAG_MINE`: a mine's gate-crystal find.
const ETAG_MINE: Tag = Tag::new(b"mine");
/// `ETAG_HARVEST`: a harvest's yield chance.
const ETAG_HARVEST: Tag = Tag::new(b"harv");
/// `ETAG_RING`: a restocked suffuse ring's kind.
const ETAG_RING: Tag = Tag::new(b"ring");

/// `sub_suffuse_ring`.
pub const SUB_SUFFUSE_RING: Subkind = Subkind(68);
/// `sub_tradegood`.
const SUB_TRADEGOOD: Subkind = Subkind(73);
/// `item_opium`.
const ITEM_OPIUM: i32 = 93;

/// `opium_data[]`: what a city buys of opium at each level of addiction,
/// as (quantity, price).
const OPIUM_DATA: [(i32, i32); 8] = [
    (15, 17),
    (28, 17),
    (37, 18),
    (50, 18),
    (55, 19),
    (66, 19),
    (70, 20),
    (80, 20),
];

/// `MAX_OPIUM_ECON`.
const MAX_OPIUM_ECON: i32 = 7;

/// The `econ` stream as last seeded (`economy_rng`). Before the first
/// [`begin_economy`](Game::begin_economy) it is the zeroed static C starts
/// with.
#[derive(Debug, Clone)]
pub struct Economy {
    rng: Stream,
}

impl Default for Economy {
    fn default() -> Self {
        Self {
            rng: Stream::new([0; 4]),
        }
    }
}

impl Economy {
    /// The market stream for `where_` in `turn` (`begin_economy()`).
    #[must_use]
    pub fn begin(master: [u32; 4], turn: i16, where_: i32) -> Self {
        Self {
            rng: Stream::new(master)
                .child(i32::from(turn), TAG_TURN)
                .child(where_, TAG_ECONOMY),
        }
    }

    /// `econ_pick()`: choose one of several goods.
    #[must_use]
    pub fn pick(&self, item: i32, where_: i32, low: i32, high: i32) -> i32 {
        self.rng.keyed(item, where_, ETAG_PICK, low, high)
    }

    /// `econ_stock()`: does the market carry a good.
    #[must_use]
    pub fn stock(&self, item: i32, where_: i32, low: i32, high: i32) -> i32 {
        self.rng.keyed(item, where_, ETAG_STOCK, low, high)
    }

    /// `econ_qty()`: quantity offered.
    #[must_use]
    pub fn qty(&self, item: i32, where_: i32, low: i32, high: i32) -> i32 {
        self.rng.keyed(item, where_, ETAG_QTY, low, high)
    }

    /// `econ_cost()`: price.
    #[must_use]
    pub fn cost(&self, item: i32, where_: i32, low: i32, high: i32) -> i32 {
        self.rng.keyed(item, where_, ETAG_COST, low, high)
    }

    /// `econ_expire()`: expiry countdown.
    #[must_use]
    pub fn expire(&self, item: i32, where_: i32, low: i32, high: i32) -> i32 {
        self.rng.keyed(item, where_, ETAG_EXPIRE, low, high)
    }

    /// `econ_unit_day()`: a draw for unit `who` working `item` on `day`, on
    /// the unit's own child of the market stream.
    fn unit_day(&self, item: i32, who: i32, day: i16, tag: Tag, low: i32, high: i32) -> i32 {
        self.rng
            .child(who, tag)
            .keyed(item, i32::from(day), tag, low, high)
    }

    /// `econ_mine()`: a mine's gate-crystal find for `who` on `day`.
    #[must_use]
    pub fn mine(&self, item: i32, who: i32, day: i16, low: i32, high: i32) -> i32 {
        self.unit_day(item, who, day, ETAG_MINE, low, high)
    }

    /// `econ_harvest()`: a harvest's yield chance for `who` on `day`.
    #[must_use]
    pub fn harvest(&self, item: i32, who: i32, day: i16, low: i32, high: i32) -> i32 {
        self.unit_day(item, who, day, ETAG_HARVEST, low, high)
    }

    /// `econ_ring()`: a restocked suffuse ring's kind.
    #[must_use]
    pub fn ring(&self, item: i32, where_: i32, low: i32, high: i32) -> i32 {
        self.rng.keyed(item, where_, ETAG_RING, low, high)
    }
}

/// A trade, by its trader and its place in the trader's list. C holds
/// trades by pointer; nothing that runs while a list of these is held adds
/// or removes a trade, so the places stay put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TradeRef {
    who: i32,
    ix: usize,
}

/// The trade at `r`.
fn trade(db: &Db, r: TradeRef) -> &Trade {
    &db.entity(r.who).trades[r.ix]
}

/// The places of `who`'s trades that `loop_trade` visits, as of now: the
/// ones for valid items with something left. The list is copied when the
/// loop starts, but each trade is checked when it is reached; callers
/// that change trades while looping recheck with [`live`].
fn loop_trade(db: &Db, who: i32) -> Vec<TradeRef> {
    assert!(db.valid_box(who), "loop_trade: invalid box {who}");
    (0..db.entity(who).trades.len())
        .map(|ix| TradeRef { who, ix })
        .filter(|&r| live(db, r))
        .collect()
}

/// Whether `loop_trade` would visit the trade at `r` now.
fn live(db: &Db, r: TradeRef) -> bool {
    let t = trade(db, r);
    db.valid_box(t.item) && t.qty > 0
}

/// `market_here()`: the city `who` is trading in, or 0 if it isn't in one
/// (or is aboard a ship).
fn market_here(db: &Db, mut who: i32) -> i32 {
    while who > 0 && db.subkind(who) != Subkind::CITY {
        if is_ship(db, who) {
            return 0;
        }
        who = db.loc(who);
    }
    who
}

/// The characters in `where_` whose trades count: all but `except`,
/// prisoners and characters on the move.
fn traders_here(db: &Db, where_: i32, except: i32) -> Vec<i32> {
    all_char_here(db, where_)
        .into_iter()
        .filter(|&i| i != except && !is_prisoner(db, i) && char_moving(db, i) == 0)
        .collect()
}

/// `seller_list()`: the sell trades in `where_`, except `except`'s: the
/// characters' cheapest first (ties in the order found), then the
/// location's own.
fn seller_list(db: &Db, where_: i32, except: i32) -> Vec<TradeRef> {
    let mut l: Vec<(TradeRef, i32)> = Vec::new();
    for i in traders_here(db, where_, except) {
        for r in loop_trade(db, i) {
            if trade(db, r).kind == SELL {
                let sort = l.len() as i32;
                l.push((r, sort));
            }
        }
    }
    // seller_comp: by cost, then by sort. C subtracts the costs, which
    // overflows only for costs no game reaches; this compares them.
    l.sort_by_key(|&(r, sort)| (trade(db, r).cost, sort));
    let mut l: Vec<TradeRef> = l.into_iter().map(|(r, _)| r).collect();
    l.extend(
        loop_trade(db, where_)
            .into_iter()
            .filter(|&r| trade(db, r).kind == SELL),
    );
    l
}

/// `buyer_list()`: the buy trades in `where_`, except `except`'s: the
/// characters', then the location's own.
fn buyer_list(db: &Db, where_: i32, except: i32) -> Vec<TradeRef> {
    let mut l = Vec::new();
    for i in traders_here(db, where_, except) {
        l.extend(
            loop_trade(db, i)
                .into_iter()
                .filter(|&r| trade(db, r).kind == BUY),
        );
    }
    l.extend(
        loop_trade(db, where_)
            .into_iter()
            .filter(|&r| trade(db, r).kind == BUY),
    );
    l
}

/// `reduce_qty()`: how much of trade `t` can go through at `cost` each:
/// what a buyer can afford or a seller holds, keeping back its
/// `have_left`. A city makes the gold or goods, so its trades go through
/// whole.
///
/// # Panics
///
/// Panics for a trade that is neither a buy nor a sell, as C asserts.
fn reduce_qty(db: &Db, t: &Trade, cost: i32) -> i32 {
    if db.kind(t.who) == Kind::LOC {
        return t.qty;
    }
    let item = match t.kind {
        BUY => ITEM_GOLD,
        SELL => t.item,
        k => panic!("reduce_qty: trade kind {k}"),
    };
    let has = (has_item(db, t.who, item) - t.have_left).max(0);
    if t.kind == BUY {
        t.qty.min(has / cost)
    } else {
        t.qty.min(has)
    }
}

/// `is_tradegood()`.
fn is_tradegood(db: &Db, item: i32) -> bool {
    db.subkind(item) == SUB_TRADEGOOD
}

/// `find_trade()`: the place of `who`'s live trade of `kind` for `item`.
fn find_trade(db: &Db, who: i32, kind: i32, item: i32) -> Option<usize> {
    loop_trade(db, who)
        .into_iter()
        .find(|&r| {
            let t = trade(db, r);
            t.kind == kind && t.item == item
        })
        .map(|r| r.ix)
}

/// `new_trade()`: the place of `who`'s live trade of `kind` for `item`,
/// or of a new, empty one added to the end of its list.
fn new_trade(db: &mut Db, who: i32, kind: i32, item: i32) -> usize {
    if let Some(ix) = find_trade(db, who, kind, item) {
        return ix;
    }
    let trades = &mut db.entity_mut(who).trades;
    trades.push(Trade {
        who,
        kind,
        item,
        ..Trade::default()
    });
    trades.len() - 1
}

impl Game {
    /// `begin_economy()`: seeds the `econ` stream for the market at
    /// `where_` this turn.
    pub fn begin_economy(&mut self, where_: i32) {
        let master = self.seed.master_seed();
        self.economy = Economy::begin(master, self.system.sysclock.turn, where_);
    }

    /// `attempt_trade()`: as much of `buyer` and `seller` as both can go
    /// through, at the seller's price.
    fn attempt_trade(&mut self, buyer: TradeRef, seller: TradeRef) -> Phase {
        let db = &self.db;
        let (b, s) = (*trade(db, buyer), *trade(db, seller));
        assert_eq!(b.item, s.item, "attempt_trade: items differ");
        let item = b.item;

        if b.cost < s.cost {
            return Ok(());
        }
        let qty = reduce_qty(db, &b, s.cost).min(reduce_qty(db, &s, s.cost));
        if qty <= 0 {
            return Ok(());
        }
        let cost = s.cost * qty;

        let bt = &mut self.db.entity_mut(buyer.who).trades[buyer.ix];
        bt.qty -= qty;
        assert!(bt.qty >= 0, "attempt_trade: buyer qty {}", bt.qty);
        let st = &mut self.db.entity_mut(seller.who).trades[seller.ix];
        st.qty -= qty;
        assert!(st.qty >= 0, "attempt_trade: seller qty {}", st.qty);

        if self.db.kind(b.who) == Kind::LOC {
            self.gen_item(s.who, ITEM_GOLD, cost);
            self.consume_item(s.who, item, qty);
            // TODO(stage 2): gold_pot_basket, gold_opium (and its
            // log_write) or gold_trade += cost, for the GM report.
        } else if self.db.kind(s.who) == Kind::LOC {
            self.consume_item(b.who, ITEM_GOLD, cost);
            if item_unique(&self.db, item) != 0 {
                self.move_item(s.who, b.who, item, qty)?;
            } else {
                self.gen_item(b.who, item, qty);
            }
        } else {
            self.move_item(b.who, s.who, ITEM_GOLD, cost)?;
            self.move_item(s.who, b.who, item, qty)?;
        }
        // TODO(stage 2): "Bought ..." to the buyer, "Sold ..." to the
        // seller, and the sale to the market, each naming the other side
        // unless it trades cloaked.
        Ok(())
    }

    /// `scan_trades()`: tries `t` against each trade in `l` for the same
    /// item from someone else, until `t` is filled.
    fn scan_trades(&mut self, t: TradeRef, l: &[TradeRef]) -> Phase {
        for &other in l {
            let (tt, ot) = (*trade(&self.db, t), *trade(&self.db, other));
            if tt.qty <= 0 {
                break;
            }
            if ot.item != tt.item || ot.who == tt.who {
                continue;
            }
            match tt.kind {
                BUY => self.attempt_trade(t, other)?,
                SELL => self.attempt_trade(other, t)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// `match_trades()`: tries each of `who`'s trades against the others
    /// in its market.
    ///
    /// # Panics
    ///
    /// Panics if one of `who`'s trades names another trader, as C asserts.
    pub fn match_trades(&mut self, who: i32) -> Phase {
        let where_ = subloc(&self.db, who);
        if market_here(&self.db, who) == 0 {
            return Ok(());
        }
        let mut sellers = None;
        let mut buyers = None;
        let n = self.db.entity(who).trades.len();
        for ix in 0..n {
            let r = TradeRef { who, ix };
            if !live(&self.db, r) {
                continue;
            }
            let t = *trade(&self.db, r);
            assert_eq!(t.who, who, "match_trades: trade of {} on {who}", t.who);
            let l = match t.kind {
                BUY => sellers.get_or_insert_with(|| seller_list(&self.db, where_, who)),
                SELL => buyers.get_or_insert_with(|| buyer_list(&self.db, where_, who)),
                _ => continue,
            };
            let l = l.clone();
            self.scan_trades(r, &l)?;
        }
        Ok(())
    }

    /// `olympia/buy.c:match_all_trades`: on day 1, every buyer in each
    /// market tries the sellers there.
    pub fn match_all_trades(&mut self) -> Phase {
        for where_ in self.db.boxes_of_kind(Kind::LOC) {
            if market_here(&self.db, where_) == 0 {
                continue;
            }
            let sellers = seller_list(&self.db, where_, 0);
            let buyers = buyer_list(&self.db, where_, 0);
            if buyers.is_empty() || sellers.is_empty() {
                continue;
            }
            for &b in &buyers {
                self.scan_trades(b, &sellers)?;
            }
        }
        Ok(())
    }

    /// `olympia/buy.c:check_validated_trades`: matches the trades of each
    /// unit queued by `investigate_possible_trade`, including those queued
    /// along the way, then empties the queue.
    pub fn check_validated_trades(&mut self) -> Phase {
        let mut i = 0;
        while i < self.driver.trades_to_check.len() {
            let who = self.driver.trades_to_check[i];
            self.match_trades(who)?;
            i += 1;
        }
        self.driver.trades_to_check.clear();
        Ok(())
    }

    /// `olympia/buy.c:opium_market_delta`: a city's opium addiction rises
    /// when it bought all it would last month and falls when it bought
    /// none, and sets what it will consume this month.
    ///
    /// The city's first opium buy is looked up on its own trade list, not
    /// with `find_trade()`, which passes over a sold-out trade.
    ///
    /// # Panics
    ///
    /// Panics if `where_` isn't a city, as C asserts, or if its addiction
    /// level is out of range when read, which C would read past its table.
    fn opium_market_delta(&mut self, where_: i32) {
        assert_eq!(self.db.subkind(where_), Subkind::CITY, "opium_market_delta");
        let found = self
            .db
            .entity(where_)
            .trades
            .iter()
            .find(|t| t.kind == BUY && t.item == ITEM_OPIUM)
            .map(|t| t.qty);
        let p = self.db.p_subloc(where_);
        if let Some(qty) = found {
            if qty < 1 {
                p.opium_econ += 1;
            } else if qty == opium_data(p.opium_econ).0 {
                p.opium_econ -= 1;
            }
        }
        p.opium_econ = p.opium_econ.clamp(0, MAX_OPIUM_ECON);
        let econ = p.opium_econ;

        let ix = new_trade(&mut self.db, where_, CONSUME, ITEM_OPIUM);
        let t = &mut self.db.entity_mut(where_).trades[ix];
        (t.qty, t.cost) = opium_data(econ);
        t.cloak = if econ > 0 { 1 } else { 2 };
    }

    /// `olympia/buy.c:expire_trades`: counts down `where_`'s trades, and
    /// drops the trade-good offers that have run out, except a buy of a
    /// good some character still carries.
    pub fn expire_trades(&mut self, where_: i32) {
        let mut i = 0;
        while i < self.db.entity(where_).trades.len() {
            let t = &mut self.db.entity_mut(where_).trades[i];
            if t.expire > 0 {
                t.expire -= 1;
            }
            let t = *t;
            if is_tradegood(&self.db, t.item) && t.expire <= 0 {
                let carried = t.kind == BUY && self.someone_carries(t.item);
                if !carried {
                    self.db.entity_mut(where_).trades.remove(i);
                    continue;
                }
            }
            i += 1;
        }
    }

    /// Whether any character holds some of `item` (`loop_char`,
    /// `loop_inv`).
    fn someone_carries(&self, item: i32) -> bool {
        let db = &self.db;
        db.boxes_of_kind(Kind::CHAR).into_iter().any(|j| {
            db.entity(j)
                .items
                .iter()
                .any(|e| db.valid_box(e.item) && e.qty > 0 && e.item == item)
        })
    }

    /// `olympia/buy.c:loc_trade_sup`: restocks `where_`'s offers from what
    /// it produces and consumes. A good produced once a year is offered
    /// only in the month before it is produced, unless `override_` is set.
    pub fn loc_trade_sup(&mut self, where_: i32, override_: bool) {
        self.expire_trades(where_);

        let n = self.db.entity(where_).trades.len();
        for ix in 0..n {
            let r = TradeRef { who: where_, ix };
            if !live(&self.db, r) {
                continue;
            }
            let t = *trade(&self.db, r);

            let mut okay = true;
            if t.month_prod != 0 && !override_ {
                let this_month = self.system.sysclock.month() - 1;
                let next_month = (this_month + 1) % NUM_MONTHS;
                let prod_month = t.month_prod - 1;
                if next_month != prod_month {
                    okay = false;
                }
            }

            let kind = match t.kind {
                PRODUCE if okay => SELL,
                CONSUME => BUY,
                _ => continue,
            };
            let ix = new_trade(&mut self.db, where_, kind, t.item);
            let new = &mut self.db.entity_mut(where_).trades[ix];
            if new.qty < t.qty {
                new.qty = t.qty;
            }
            new.cost = t.cost;
            new.cloak = t.cloak;
            new.expire = t.expire;
        }
    }

    /// `olympia/buy.c:trade_suffuse_ring`: a city of Faery or the
    /// Cloudlands with no golden ring for sale may get one, on its `econ`
    /// stream.
    ///
    /// # Panics
    ///
    /// Panics if no entity number is free for the ring, which C would
    /// crash on.
    fn trade_suffuse_ring(&mut self, where_: i32) {
        let db = &self.db;
        let found = loop_trade(db, where_).into_iter().any(|r| {
            let t = trade(db, r);
            db.subkind(t.item) == SUB_SUFFUSE_RING && t.kind == SELL && t.qty > 0
        });

        self.begin_economy(where_);

        if found
            || self
                .economy
                .stock(i32::from(SUB_SUFFUSE_RING.0), where_, 1, 3)
                < 3
        {
            return;
        }

        let item = self.new_suffuse_ring(where_);
        assert!(item > 0, "trade_suffuse_ring: no entity for the ring");

        let cost = 450
            + self
                .economy
                .cost(i32::from(SUB_SUFFUSE_RING.0), where_, 0, 12)
                * 50;
        let ix = new_trade(&mut self.db, where_, SELL, item);
        let new = &mut self.db.entity_mut(where_).trades[ix];
        new.qty = 1;
        new.cost = cost;
        new.cloak = 0;
    }

    /// `olympia/buy.c:location_trades`: each city's month-end restock: its
    /// opium market, its offers, and a golden ring in Faery and the
    /// Cloudlands.
    pub fn location_trades(&mut self) {
        stage("location_trades()");
        for where_ in self.db.boxes_of_subkind(Subkind::CITY) {
            self.opium_market_delta(where_);
            self.loc_trade_sup(where_, false);

            let reg = region(&self.db, where_);
            if reg == self.system.faery_region || reg == self.system.cloud_region {
                self.trade_suffuse_ring(where_);
            }
        }
    }
}

/// `opium_data[level]`.
///
/// # Panics
///
/// Panics if `level` is out of range.
fn opium_data(level: i32) -> (i32, i32) {
    usize::try_from(level)
        .ok()
        .and_then(|i| OPIUM_DATA.get(i))
        .copied()
        .unwrap_or_else(|| panic!("opium_data: level {level} out of range"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // The expected draws come from olympia-g3's `begin_economy` and
    // `econ_*` leaves, run over `lib/rng.c` and `lib/rnd.c` with this
    // master seed.
    const MASTER: [u32; 4] = [0x0123_4567, 0x89ab_cdef, 0xfedc_ba98, 0x7654_3210];
    const RING: i32 = 68;

    #[test]
    fn econ_leaves_match_c() {
        let e = Economy::begin(MASTER, 3, 56800);
        assert_eq!(e.stock(RING, 56800, 1, 3), 3);
        assert_eq!(e.stock(RING, 56801, 1, 3), 3);
        assert_eq!(e.cost(RING, 56800, 0, 12), 5);
        assert_eq!(e.ring(RING, 56800, 1, 5), 3);
        assert_eq!(e.pick(77, 56800, 1, 100), 30);
        assert_eq!(e.qty(77, 56800, 1, 100), 21);
        assert_eq!(e.expire(77, 56800, 1, 100), 63);
        assert_eq!(e.mine(79, 1234, 7, 1, 100), 49);
        assert_eq!(e.harvest(77, 1234, 7, 1, 100), 88);
    }

    #[test]
    fn each_market_has_its_own_stream() {
        // (stock, cost, ring) for the suffuse-ring restock of markets
        // 56800..56809 in turn 3.
        let want = [
            (3, 5, 3),
            (1, 1, 2),
            (1, 4, 1),
            (1, 0, 4),
            (3, 6, 5),
            (3, 5, 2),
            (2, 2, 2),
            (1, 7, 2),
            (1, 12, 3),
            (3, 3, 2),
        ];
        for (w, &(stock, cost, ring)) in (56800..).zip(&want) {
            let e = Economy::begin(MASTER, 3, w);
            assert_eq!(
                (
                    e.stock(RING, w, 1, 3),
                    e.cost(RING, w, 0, 12),
                    e.ring(RING, w, 1, 5)
                ),
                (stock, cost, ring),
                "market {w}"
            );
        }
    }

    const CITY: i32 = 56800;
    const PROV: i32 = 20000;
    const GOODS: i32 = 77;

    /// A province holding a city with characters 1001..=1003 in it, and
    /// the items gold and lumber.
    fn market() -> Game {
        let mut g = Game::new(PathBuf::from("lib"));
        g.seed = crate::rnd::LegacyRng::new(MASTER);
        let db = &mut g.db;
        db.alloc_box(PROV, Kind::LOC, Subkind::PLAIN);
        db.alloc_box(CITY, Kind::LOC, Subkind::CITY);
        db.set_where(CITY, PROV);
        for who in 1001..=1003 {
            db.alloc_box(who, Kind::CHAR, Subkind::default());
            db.set_where(who, CITY);
        }
        db.alloc_box(ITEM_GOLD, Kind::ITEM, Subkind::default());
        db.alloc_box(GOODS, Kind::ITEM, Subkind::default());
        g
    }

    fn offer(g: &mut Game, who: i32, kind: i32, qty: i32, cost: i32) {
        g.db.entity_mut(who).trades.push(Trade {
            who,
            kind,
            item: GOODS,
            qty,
            cost,
            ..Trade::default()
        });
    }

    #[test]
    fn day_one_matches_buyers_with_the_cheapest_sellers() {
        let mut g = market();
        g.gen_item(1001, ITEM_GOLD, 100);
        g.gen_item(1002, GOODS, 5);
        g.gen_item(1003, GOODS, 5);
        offer(&mut g, 1001, BUY, 8, 10);
        offer(&mut g, 1002, SELL, 5, 9);
        offer(&mut g, 1003, SELL, 5, 7);
        offer(&mut g, CITY, SELL, 10, 1);

        g.match_all_trades().unwrap();

        // 1003 sells 5 at 7, then 1002 sells 3 at 9; the city, listed
        // after the characters, sells nothing.
        let db = &g.db;
        assert_eq!(has_item(db, 1001, GOODS), 8);
        assert_eq!(has_item(db, 1001, ITEM_GOLD), 100 - 35 - 27);
        assert_eq!(has_item(db, 1003, ITEM_GOLD), 35);
        assert_eq!(has_item(db, 1002, ITEM_GOLD), 27);
        assert_eq!(has_item(db, 1002, GOODS), 2);
        assert_eq!(db.entity(1001).trades[0].qty, 0);
        assert_eq!(db.entity(1002).trades[0].qty, 2);
        assert_eq!(db.entity(CITY).trades[0].qty, 10);
    }

    #[test]
    fn a_buyer_buys_only_what_it_can_afford() {
        let mut g = market();
        g.gen_item(1001, ITEM_GOLD, 25);
        offer(&mut g, 1001, BUY, 10, 5);
        offer(&mut g, CITY, SELL, 10, 4);
        g.match_all_trades().unwrap();
        // 25 gold buys 6 at 4; the city makes the goods.
        assert_eq!(has_item(&g.db, 1001, GOODS), 6);
        assert_eq!(has_item(&g.db, 1001, ITEM_GOLD), 1);
        assert_eq!(g.db.entity(1001).trades[0].qty, 4);
    }

    #[test]
    fn opium_addiction_falls_when_none_is_sold() {
        let mut g = market();
        g.db.alloc_box(ITEM_OPIUM, Kind::ITEM, Subkind::default());
        g.db.p_subloc(CITY).opium_econ = 3;
        g.db.entity_mut(CITY).trades.push(Trade {
            who: CITY,
            kind: BUY,
            item: ITEM_OPIUM,
            qty: 50,
            cost: 18,
            ..Trade::default()
        });
        g.opium_market_delta(CITY);
        assert_eq!(g.db.entity(CITY).subloc.as_ref().unwrap().opium_econ, 2);
        let t = g.db.entity(CITY).trades[1];
        assert_eq!((t.kind, t.qty, t.cost, t.cloak), (CONSUME, 37, 18, 1));
    }

    #[test]
    fn opium_addiction_rises_when_the_city_sold_out() {
        let mut g = market();
        g.db.alloc_box(ITEM_OPIUM, Kind::ITEM, Subkind::default());
        g.db.entity_mut(CITY).trades.push(Trade {
            who: CITY,
            kind: BUY,
            item: ITEM_OPIUM,
            qty: 0,
            cost: 17,
            ..Trade::default()
        });
        g.opium_market_delta(CITY);
        assert_eq!(g.db.entity(CITY).subloc.as_ref().unwrap().opium_econ, 1);
        let t = g.db.entity(CITY).trades[1];
        assert_eq!((t.kind, t.qty, t.cost, t.cloak), (CONSUME, 28, 17, 1));
    }

    #[test]
    fn restock_offers_what_a_city_produces_and_consumes() {
        let mut g = market();
        g.system.sysclock.turn = 2; // month 1
        offer(&mut g, CITY, PRODUCE, 12, 6);
        offer(&mut g, CITY, CONSUME, 4, 9);
        g.db.entity_mut(CITY).trades[0].month_prod = 2; // produced next month
        offer(&mut g, CITY, SELL, 20, 5);
        g.loc_trade_sup(CITY, false);
        let t = &g.db.entity(CITY).trades;
        assert_eq!(t.len(), 4);
        // The standing sell keeps its larger quantity and takes the price.
        assert_eq!((t[2].kind, t[2].qty, t[2].cost), (SELL, 20, 6));
        assert_eq!((t[3].kind, t[3].qty, t[3].cost), (BUY, 4, 9));

        // In any other month the yearly good isn't offered.
        let mut g = market();
        g.system.sysclock.turn = 3;
        offer(&mut g, CITY, PRODUCE, 12, 6);
        g.db.entity_mut(CITY).trades[0].month_prod = 2;
        g.loc_trade_sup(CITY, false);
        assert_eq!(g.db.entity(CITY).trades.len(), 1);
    }
}
