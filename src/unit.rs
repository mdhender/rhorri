// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Questions about characters: what they hold, whether they can see each
//! other, and the `oly.h` accessors for their state (`olympia/u.c`,
//! `olympia/oly.h`).

use crate::check::{all_stack, stack_leader, stack_parent};
use crate::entity::{Char, CharMagic, Db, Misc, Subkind};
use crate::game::player;
use crate::loc::{LOC_PROVINCE, garrison_here, loc_depth, subloc, weather_here};

/// `item_ghost_warrior`.
const ITEM_GHOST_WARRIOR: i32 = 18;
/// `item_gold`.
pub const ITEM_GOLD: i32 = 1;
/// `LOY_npc`.
const LOY_NPC: i8 = 4;
/// `LOY_summon`.
const LOY_SUMMON: i8 = 5;

/// `rp_char(n)`.
fn rp_char(db: &Db, n: i32) -> Option<&Char> {
    db.get(n).and_then(|b| b.char_.as_deref())
}

/// `rp_magic(n)`.
fn rp_magic(db: &Db, n: i32) -> Option<&CharMagic> {
    rp_char(db, n).and_then(|c| c.magic.as_deref())
}

/// `rp_misc(n)`.
fn rp_misc(db: &Db, n: i32) -> Option<&Misc> {
    db.get(n).and_then(|b| b.misc.as_deref())
}

/// `char_moving()`: the day the character started moving, or 0.
#[must_use]
pub fn char_moving(db: &Db, n: i32) -> i32 {
    rp_char(db, n).map_or(0, |c| c.moving)
}

/// `char_gone()`: whether the character is away, moving.
#[must_use]
pub fn char_gone(db: &Db, n: i32) -> bool {
    char_moving(db, n) != 0
}

/// `is_prisoner()`.
#[must_use]
pub fn is_prisoner(db: &Db, n: i32) -> bool {
    rp_char(db, n).is_some_and(|c| c.prisoner != 0)
}

/// `char_health()`.
#[must_use]
pub fn char_health(db: &Db, n: i32) -> i32 {
    rp_char(db, n).map_or(0, |c| i32::from(c.health))
}

/// `npc_program()`.
#[must_use]
pub fn npc_program(db: &Db, n: i32) -> i32 {
    rp_char(db, n).map_or(0, |c| i32::from(c.npc_prog))
}

/// `loyal_kind()`.
fn loyal_kind(db: &Db, n: i32) -> i8 {
    rp_char(db, n).map_or(0, |c| c.loy_kind)
}

/// `is_npc()`: has a subkind, or is loyal as an NPC or a summoned unit.
#[must_use]
pub fn is_npc(db: &Db, n: i32) -> bool {
    db.subkind(n) != Subkind::default()
        || loyal_kind(db, n) == LOY_NPC
        || loyal_kind(db, n) == LOY_SUMMON
}

/// `only_defeatable()`: the item needed to defeat `n`, or 0.
#[must_use]
pub fn only_defeatable(db: &Db, n: i32) -> i32 {
    rp_misc(db, n).map_or(0, |m| m.only_vulnerable)
}

/// `restricted_control()`: the `allow` letter `n` issues orders as, or 0.
#[must_use]
pub fn restricted_control(db: &Db, n: i32) -> u8 {
    rp_misc(db, n).map_or(0, |m| m.cmd_allow)
}

/// `char_hidden()`.
fn char_hidden(db: &Db, n: i32) -> bool {
    rp_magic(db, n).is_some_and(|m| m.hide_self != 0)
}

/// `ferry_horn()`.
#[must_use]
pub fn ferry_horn(db: &Db, n: i32) -> bool {
    rp_magic(db, n).is_some_and(|m| m.ferry_flag != 0)
}

/// `has_item()`: how many of `item` `who` holds.
///
/// # Panics
///
/// Panics if `who` or `item` isn't a valid box, as C asserts.
#[must_use]
pub fn has_item(db: &Db, who: i32, item: i32) -> i32 {
    assert!(db.valid_box(who), "has_item: invalid box {who}");
    assert!(db.valid_box(item), "has_item: invalid item {item}");
    db.entity(who)
        .items
        .iter()
        .find(|e| e.item == item)
        .map_or(0, |e| e.qty)
}

/// `man_item()`.
fn man_item(db: &Db, n: i32) -> bool {
    db.get(n)
        .and_then(|b| b.item.as_ref())
        .is_some_and(|i| i.is_man_item != 0)
}

/// `is_fighter()`.
fn is_fighter(db: &Db, n: i32) -> bool {
    db.get(n)
        .and_then(|b| b.item.as_ref())
        .is_some_and(|i| i.attack != 0 || i.defense != 0 || i.missile != 0)
        || n == ITEM_GHOST_WARRIOR
}

/// `count_any()`: the figures in a unit: the noble (unless a garrison),
/// and its men and fighters.
#[must_use]
pub fn count_any(db: &Db, who: i32) -> i32 {
    let mut sum = i32::from(db.subkind(who) != Subkind::GARRISON);
    // loop_inv
    assert!(db.valid_box(who), "loop_inv: invalid box {who}");
    for e in &db.entity(who).items {
        if db.valid_box(e.item) && e.qty > 0 && (man_item(db, e.item) || is_fighter(db, e.item)) {
            sum += e.qty;
        }
    }
    sum
}

/// `count_stack_any()`: the figures in `who`'s stack.
#[must_use]
pub fn count_stack_any(db: &Db, who: i32) -> i32 {
    all_stack(db, who).iter().map(|&i| count_any(db, i)).sum()
}

/// `char_alone()`: unstacked, and with no one else in its unit.
fn char_alone(db: &Db, n: i32) -> bool {
    stack_parent(db, n) == 0 && count_stack_any(db, n) == 1
}

/// `char_really_hidden()`: hidden and alone.
#[must_use]
pub fn char_really_hidden(db: &Db, n: i32) -> bool {
    char_hidden(db, n) && char_alone(db, n)
}

/// `contacted()`: whether `a` has contacted `b` or `b`'s faction.
///
/// C's `p_char(a)` allocates `a`'s character struct; every caller passes a
/// character, which has one.
#[must_use]
pub fn contacted(db: &Db, a: i32, b: i32) -> bool {
    let Some(c) = rp_char(db, a) else {
        return false;
    };
    c.contact.contains(b) || c.contact.contains(player(db, b))
}

/// `char_where()`: whether `who` can see `target` at `where_`. A hidden
/// target, or one in fog in a province, is seen only by its own faction,
/// as the garrison, by those it contacted, and by its own stack.
#[must_use]
pub fn char_where(db: &Db, where_: i32, who: i32, target: i32) -> bool {
    if where_ != subloc(db, target) {
        return false;
    }
    if char_really_hidden(db, target)
        || (loc_depth(db, where_) == LOC_PROVINCE && weather_here(db, where_, Subkind::FOG) != 0)
    {
        let pl = player(db, who);
        return pl == player(db, target)
            || target == garrison_here(db, where_)
            || contacted(db, target, who)
            || stack_leader(db, target) == stack_leader(db, who);
    }
    true
}

/// `char_here()`: whether `who` can see `target` where `who` is.
#[must_use]
pub fn char_here(db: &Db, who: i32, target: i32) -> bool {
    char_where(db, subloc(db, who), who, target)
}

/// `olympia/move.c:restore_stack_actions`: `who`'s stack stops moving.
pub fn restore_stack_actions(db: &mut Db, who: i32) {
    for i in all_stack(db, who) {
        db.p_char(i).moving = 0;
    }
}
