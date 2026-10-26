// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Players' queued orders (`olympia/order.c`).

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::cstr::{atoi, fuzzy_eq, i_str_eq, is_white};
use crate::entity::{Db, Kind, OrderList, Text};
use crate::lines::LineReader;
use crate::os::{make_dir, strerror};

/// The most orders a unit can have queued.
const MAX_QUEUED: usize = 250;

/// `p_order_head()`: unit `who`'s queue in player `pl`'s orders, made
/// if there is none.
fn p_order_head(db: &mut Db, pl: i32, who: i32) -> &mut Vec<Text> {
    let orders = &mut db.p_player(pl).orders;
    let i = orders
        .iter()
        .position(|o| o.unit == who)
        .unwrap_or_else(|| {
            orders.push(OrderList {
                unit: who,
                l: Vec::new(),
            });
            orders.len() - 1
        });
    &mut orders[i].l
}

/// Queues `s` for unit `who` in player `pl`'s orders (`queue_order()`).
pub fn queue_order(db: &mut Db, pl: i32, who: i32, s: &[u8]) {
    let list = p_order_head(db, pl, who);
    if list.len() < MAX_QUEUED {
        list.push(s.to_vec());
    }
}

/// Puts `s` at the head of unit `who`'s orders in player `pl`'s orders
/// (`prepend_order()`). Unlike [`queue_order`], there is no limit.
pub fn prepend_order(db: &mut Db, pl: i32, who: i32, s: &[u8]) {
    p_order_head(db, pl, who).insert(0, s.to_vec());
}

/// `queue()`: queues `s` for unit `who` in its faction's orders.
pub fn queue(db: &mut Db, who: i32, s: &[u8]) {
    let pl = crate::game::player(db, who);
    queue_order(db, pl, who, s);
}

/// `rp_order_head()`: unit `who`'s queue in player `pl`'s orders.
fn rp_order_head(db: &Db, pl: i32, who: i32) -> Option<&OrderList> {
    db.get(pl)?
        .player
        .as_deref()?
        .orders
        .iter()
        .find(|o| o.unit == who)
}

/// `top_order()`: the next order queued for `who` in player `pl`'s orders.
#[must_use]
pub fn top_order(db: &Db, pl: i32, who: i32) -> Option<&[u8]> {
    rp_order_head(db, pl, who)?.l.first().map(Vec::as_slice)
}

/// `is_stop_order()`: whether the order's first word is `stop`.
///
/// After the exact test, C checks the rest of the line, not the word, for
/// a fuzzy match. The rest is empty or starts with white space, so it never
/// matches, and `stpo` is not a stop order; kept as C has it.
fn is_stop_order(s: &[u8]) -> bool {
    let start = s.iter().position(|&c| !is_white(c)).unwrap_or(s.len());
    let s = &s[start..];
    let end = s.iter().position(|&c| is_white(c)).unwrap_or(s.len());
    let (word, rest) = s.split_at(end);
    i_str_eq(word, b"stop") || fuzzy_eq(rest, b"stop")
}

/// `stop_order()`: whether `who`'s next order is `stop`. A stop order must
/// be first in the queue.
#[must_use]
pub fn stop_order(db: &Db, pl: i32, who: i32) -> bool {
    top_order(db, pl, who).is_some_and(is_stop_order)
}

/// `pop_order()`: drops `who`'s next order.
///
/// # Panics
///
/// Panics if `who` has no order queued, as C asserts.
pub fn pop_order(db: &mut Db, pl: i32, who: i32) {
    let list = db
        .get_mut(pl)
        .and_then(|b| b.player.as_deref_mut())
        .and_then(|p| p.orders.iter_mut().find(|o| o.unit == who))
        .expect("pop_order: no order queue");
    assert!(!list.l.is_empty(), "pop_order: no orders queued");
    list.l.remove(0);
}

/// Loads every `orders/<player>` file (`load_orders()`).
pub fn load_orders(db: &mut Db, dir: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("can't open {}: {}", dir.display(), strerror(&err));
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.as_encoded_bytes();
        if !name.first().is_some_and(u8::is_ascii_digit) {
            continue;
        }
        let fact = atoi(name);
        if !db.valid_box(fact) {
            eprintln!("ERROR: orders/{fact} but no box [{fact}]");
            continue;
        }
        load_player_orders(db, fact, &dir.join(OsStr::new(&fact.to_string())));
    }
}

fn load_player_orders(db: &mut Db, pl: i32, path: &Path) {
    let Some(mut r) = LineReader::open(path) else {
        return;
    };
    while let Some(line) = r.readlin() {
        let unit = atoi(&line);
        let text = match line.iter().position(|&c| c == b':') {
            Some(colon) => &line[colon + 1..],
            None => &line[line.len()..],
        };
        queue_order(db, pl, unit, text);
    }
}

/// Rewrites the `orders` directory from the players' queues
/// (`save_orders()`).
pub fn save_orders(db: &Db, dir: &Path) {
    let _ = fs::remove_dir_all(dir);
    make_dir(dir);
    for pl in db.boxes_of_kind(Kind::PLAYER) {
        save_player_orders(db, pl, &dir.join(pl.to_string()));
    }
}

/// Writes player `pl`'s queued orders, creating the file only if there is
/// at least one (`save_player_orders()`).
fn save_player_orders(db: &Db, pl: i32, path: &Path) {
    let Some(player) = db.entity(pl).player.as_deref() else {
        return;
    };
    let mut out = Vec::new();
    for orders in &player.orders {
        if !db.valid_box(orders.unit) || db.kind(orders.unit) == Kind::DEADCHAR {
            continue;
        }
        for line in &orders.l {
            out.extend_from_slice(format!("{}:", orders.unit).as_bytes());
            out.extend_from_slice(line);
            out.push(b'\n');
        }
    }
    if out.is_empty() {
        return;
    }
    match File::create(path) {
        // fprintf() is unchecked in C.
        Ok(mut file) => {
            let _ = file.write_all(&out);
        }
        Err(err) => eprintln!("can't write {}: {}", path.display(), strerror(&err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Subkind;

    #[test]
    fn stop_orders_are_read_like_c() {
        assert!(is_stop_order(b"stop"));
        assert!(is_stop_order(b"  STOP now"));
        // The fuzzy test looks at the rest of the line, not the word.
        assert!(!is_stop_order(b"stpo"));
        assert!(!is_stop_order(b"x tsop"));
        assert!(!is_stop_order(b"stopping"));
    }

    #[test]
    fn top_and_pop_walk_a_units_queue() {
        let mut db = Db::new();
        db.alloc_box(500, Kind::PLAYER, Subkind::default());
        assert_eq!(top_order(&db, 500, 1001), None);
        assert_eq!(top_order(&db, 0, 1001), None);
        queue_order(&mut db, 500, 1001, b"stop");
        queue_order(&mut db, 500, 1001, b"move n");
        assert!(stop_order(&db, 500, 1001));
        pop_order(&mut db, 500, 1001);
        assert_eq!(top_order(&db, 500, 1001), Some(&b"move n"[..]));
        assert!(!stop_order(&db, 500, 1001));
    }

    #[test]
    fn queue_order_groups_by_unit_and_caps_the_queue() {
        let mut db = Db::new();
        db.alloc_box(500, Kind::PLAYER, Subkind::default());
        queue_order(&mut db, 500, 1001, b"move n");
        queue_order(&mut db, 500, 1002, b"stop");
        queue_order(&mut db, 500, 1001, b"move s");
        for _ in 0..300 {
            queue_order(&mut db, 500, 1003, b"wait");
        }
        let orders = &db.entity(500).player.as_ref().unwrap().orders;
        assert_eq!(orders[0].unit, 1001);
        assert_eq!(orders[0].l, [b"move n".to_vec(), b"move s".to_vec()]);
        assert_eq!(orders[1].unit, 1002);
        assert_eq!(orders[2].l.len(), MAX_QUEUED);
    }
}
