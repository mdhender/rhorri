// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Questions about characters: what they hold, whether they can see each
//! other, and the `oly.h` accessors for their state (`olympia/u.c`,
//! `olympia/oly.h`).

use crate::check::{all_stack, stack_leader, stack_parent};
use crate::entity::{
    BUY, Char, CharMagic, Db, Item, ItemEnt, ItemMagic, Kind, Misc, SELL, Subkind,
};
use crate::game::player;
use crate::game::{Game, NotYetImplemented};
use crate::loc::is_loc_or_ship;
use crate::loc::{LOC_PROVINCE, all_char_here, garrison_here, loc_depth, subloc, weather_here};
use crate::lore::{queue_lore, test_known};
use crate::swear::{LOY_NPC, LOY_SUMMON};

/// `sub_ni`: a noble that stands for an item.
pub const SUB_NI: Subkind = Subkind(54);
/// `SKILL_know`: a skill the character knows.
const SKILL_KNOW: i8 = 2;
/// `item_ghost_warrior`.
const ITEM_GHOST_WARRIOR: i32 = 18;
/// `item_gold`.
pub const ITEM_GOLD: i32 = 1;
/// `sub_npc_token`.
const SUB_NPC_TOKEN: Subkind = Subkind(63);

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
#[must_use]
pub fn loyal_kind(db: &Db, n: i32) -> i32 {
    rp_char(db, n).map_or(0, |c| i32::from(c.loy_kind))
}

/// `noble_item()`: the item a noble stands for, or 0.
#[must_use]
pub fn noble_item(db: &Db, n: i32) -> i32 {
    rp_char(db, n).map_or(0, |c| c.unit_item)
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

/// `char_sick()`.
#[must_use]
pub fn char_sick(db: &Db, n: i32) -> bool {
    rp_char(db, n).is_some_and(|c| c.sick != 0)
}

/// `is_magician()`.
#[must_use]
pub fn is_magician(db: &Db, n: i32) -> bool {
    rp_magic(db, n).is_some_and(|m| m.magician != 0)
}

/// `default_garrison()`: a unit that stands in for a garrison's men.
#[must_use]
pub fn default_garrison(db: &Db, n: i32) -> bool {
    rp_magic(db, n).is_some_and(|m| m.default_garr != 0)
}

/// `release_swear()`: a prisoner who swears to whoever frees it.
#[must_use]
pub fn release_swear(db: &Db, n: i32) -> bool {
    rp_magic(db, n).is_some_and(|m| m.swear_on_release != 0)
}

/// `char_max_aura()`.
#[must_use]
pub fn char_max_aura(db: &Db, n: i32) -> i32 {
    rp_magic(db, n).map_or(0, |m| m.max_aura)
}

/// `char_auraculum()`.
#[must_use]
pub fn char_auraculum(db: &Db, n: i32) -> i32 {
    rp_magic(db, n).map_or(0, |m| m.auraculum)
}

/// `item_aura()`.
#[must_use]
pub fn item_aura(db: &Db, n: i32) -> i32 {
    rp_item_magic(db, n).map_or(0, |m| i32::from(m.aura))
}

/// `item_aura_bonus()`.
#[must_use]
pub fn item_aura_bonus(db: &Db, n: i32) -> i32 {
    rp_item_magic(db, n).map_or(0, |m| i32::from(m.aura_bonus))
}

/// `item_curse()`: a cursed item's loyalty curse and its creator, as
/// `(curse_loyalty, creator)`, if the item has magic.
#[must_use]
pub fn item_curse(db: &Db, n: i32) -> Option<(i32, i32)> {
    rp_item_magic(db, n).map(|m| (i32::from(m.curse_loyalty), m.creator))
}

/// `loop_inv()`: a copy of `who`'s inventory, skipping invalid items and
/// those it holds none of.
///
/// # Panics
///
/// Panics if `who` isn't a valid box, as C asserts.
#[must_use]
pub fn inventory(db: &Db, who: i32) -> Vec<ItemEnt> {
    assert!(db.valid_box(who), "loop_inv: invalid box {who}");
    db.entity(who)
        .items
        .iter()
        .filter(|e| db.valid_box(e.item) && e.qty > 0)
        .copied()
        .collect()
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
#[must_use]
pub fn man_item(db: &Db, n: i32) -> bool {
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
#[must_use]
pub fn char_alone(db: &Db, n: i32) -> bool {
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

/// `rp_item(n)`.
fn rp_item(db: &Db, n: i32) -> Option<&Item> {
    db.get(n).and_then(|b| b.item.as_deref())
}

/// `rp_item_magic(n)`.
fn rp_item_magic(db: &Db, n: i32) -> Option<&ItemMagic> {
    rp_item(db, n).and_then(|i| i.magic.as_deref())
}

/// `item_lore()`: the lore sheet that comes with an item, or 0.
#[must_use]
pub fn item_lore(db: &Db, n: i32) -> i32 {
    rp_item_magic(db, n).map_or(0, |m| m.lore)
}

/// `item_unique()`: who holds a unique item, or 0 for an ordinary one.
#[must_use]
pub fn item_unique(db: &Db, n: i32) -> i32 {
    rp_item(db, n).map_or(0, |i| i.who_has)
}

/// `item_animal()`: whether an item is an animal.
#[must_use]
pub fn item_animal(db: &Db, n: i32) -> bool {
    rp_item(db, n).is_some_and(|i| i.animal != 0)
}

/// `item_relic_decay()`: how many months a relic has left.
#[must_use]
pub fn item_relic_decay(db: &Db, n: i32) -> i32 {
    rp_item_magic(db, n).map_or(0, |m| i32::from(m.relic_decay))
}

/// `loyal_rate()`.
#[must_use]
pub fn loyal_rate(db: &Db, n: i32) -> i32 {
    rp_char(db, n).map_or(0, |c| c.loy_rate)
}

/// `char_rank()`: a noble's rank (`RANK_lord` and up), or 0.
#[must_use]
pub fn char_rank(db: &Db, n: i32) -> i32 {
    rp_char(db, n).map_or(0, |c| i32::from(c.rank))
}

/// `char_pledge()`: whom `n` is pledged to, or 0.
#[must_use]
pub fn char_pledge(db: &Db, n: i32) -> i32 {
    rp_magic(db, n).map_or(0, |m| m.pledge)
}

/// `body_old_lord()`: the faction a dead noble's body belonged to.
#[must_use]
pub fn body_old_lord(db: &Db, n: i32) -> i32 {
    rp_misc(db, n).map_or(0, |m| m.old_lord)
}

/// `skill_np_req()`: the noble points a skill costs.
#[must_use]
pub fn skill_np_req(db: &Db, n: i32) -> i32 {
    db.get(n)
        .and_then(|b| b.skill.as_deref())
        .map_or(0, |s| s.np_req)
}

/// `olympia/use.c:char_np_total`: the noble points a character is worth:
/// one, plus its oath, plus the cost of every skill on its list. An NPC
/// (but a dead noble's body) is worth none.
#[must_use]
pub fn char_np_total(db: &Db, who: i32) -> i32 {
    /// `sub_dead_body`.
    const SUB_DEAD_BODY: Subkind = Subkind(56);
    /// `LOY_oath`.
    const LOY_OATH: i32 = 2;

    let mut sum = 1; // chars cost 1 NP to start
    if is_npc(db, who) && db.subkind(who) != SUB_DEAD_BODY {
        return 0;
    }
    if loyal_kind(db, who) == LOY_OATH {
        sum += loyal_rate(db, who);
    }
    assert!(db.valid_box(who), "loop_char_skill: invalid box {who}");
    if let Some(c) = rp_char(db, who) {
        for e in &c.skills {
            sum += skill_np_req(db, e.skill);
        }
    }
    sum
}

/// `item_attack()`.
#[must_use]
pub fn item_attack(db: &Db, n: i32) -> i32 {
    rp_item(db, n).map_or(0, |i| i32::from(i.attack))
}

/// `item_defense()`.
#[must_use]
pub fn item_defense(db: &Db, n: i32) -> i32 {
    rp_item(db, n).map_or(0, |i| i32::from(i.defense))
}

/// `item_token_num()`: how many units an NPC token controls at most.
#[must_use]
pub fn item_token_num(db: &Db, n: i32) -> i32 {
    rp_item_magic(db, n).map_or(0, |m| i32::from(m.token_num))
}

/// `item_token_ni()`: the item an NPC token's units stand for.
#[must_use]
pub fn item_token_ni(db: &Db, n: i32) -> i32 {
    rp_item_magic(db, n).map_or(0, |m| m.token_ni)
}

/// `npc_last_dir()`: the direction an NPC last moved in, or 0.
#[must_use]
pub fn npc_last_dir(db: &Db, n: i32) -> i32 {
    rp_misc(db, n).map_or(0, |m| i32::from(m.npc_dir))
}

/// `olympia/use.c:exp_level`: the experience level for `exp` points.
#[must_use]
pub fn exp_level(exp: i32) -> i32 {
    match exp {
        ..=4 => 1,    // exp_novice
        5..=11 => 2,  // exp_journeyman
        12..=20 => 3, // exp_teacher
        21..=34 => 4, // exp_master
        _ => 5,       // exp_grand
    }
}

/// `olympia/use.c:has_skill`: `who`'s experience level in a skill it
/// knows, or 0.
#[must_use]
pub fn has_skill(db: &Db, who: i32, skill: i32) -> i32 {
    rp_char(db, who)
        .and_then(|c| c.skills.iter().find(|e| e.skill == skill))
        .filter(|e| e.know == SKILL_KNOW)
        .map_or(0, |e| exp_level(i32::from(e.experience)))
}

/// `has_use_key()`: the first item `who` holds with `use_key` `key`, or 0.
#[must_use]
pub fn has_use_key(db: &Db, who: i32, key: i32) -> i32 {
    assert!(db.valid_box(who), "loop_inv: invalid box {who}");
    db.entity(who)
        .items
        .iter()
        .filter(|e| db.valid_box(e.item) && e.qty > 0)
        .find(|e| rp_item_magic(db, e.item).is_some_and(|m| i32::from(m.use_key) == key))
        .map_or(0, |e| e.item)
}

/// `stack_has_use_key()`: an item with `use_key` `key` held by someone in
/// `who`'s stack of `who`'s faction, or 0.
#[must_use]
pub fn stack_has_use_key(db: &Db, who: i32, key: i32) -> i32 {
    let pl = player(db, who);
    all_stack(db, stack_leader(db, who))
        .into_iter()
        .filter(|&i| player(db, i) == pl)
        .map(|i| has_use_key(db, i, key))
        .find(|&ret| ret != 0)
        .unwrap_or(0)
}

/// `beast_capturable()`: a noble standing for a capturable beast.
#[must_use]
pub fn beast_capturable(db: &Db, who: i32) -> bool {
    db.subkind(who) == SUB_NI && rp_item(db, noble_item(db, who)).is_some_and(|i| i.capturable != 0)
}

impl Game {
    /// `olympia/u.c:new_char`: a new character of subkind `sk`, standing
    /// for item `ni`, at `where_` (or the location holding it), sworn to
    /// `pl` with loyalty `loy_kind` at `loy_lev`. Returns its number.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] if `where_` is a character, to stack
    /// the new one under (`join_stack`), or if swearing it reaches
    /// something not ported.
    #[allow(clippy::too_many_arguments)]
    pub fn new_char(
        &mut self,
        sk: Subkind,
        ni: i32,
        where_: i32,
        health: i32,
        pl: i32,
        loy_kind: i32,
        loy_lev: i32,
        name: Option<&[u8]>,
    ) -> Result<i32, NotYetImplemented> {
        let new = self.new_ent(Kind::CHAR, sk);
        if new < 0 {
            return Ok(-1);
        }

        if let Some(name) = name.filter(|n| !n.is_empty()) {
            self.db.set_name(new, name);
        }
        let p = self.db.p_char(new);
        p.health = health as i8;
        p.unit_item = ni;
        p.break_point = 50;
        p.attack = 60;
        p.defense = 60;

        let dest = if is_loc_or_ship(&self.db, where_) {
            where_
        } else {
            subloc(&self.db, where_)
        };
        self.db.set_where(new, dest);

        self.set_lord(new, pl, loy_kind, loy_lev)?;

        if self.db.kind(where_) == Kind::CHAR {
            return Err(NotYetImplemented("join_stack".to_owned()));
        }

        if beast_capturable(&self.db, new) || is_npc(&self.db, new) {
            self.db.p_char(new).break_point = 0;
        }
        Ok(new)
    }

    /// `olympia/u.c:gen_item`: gives `who` `qty` more of an ordinary item.
    ///
    /// # Panics
    ///
    /// Panics for a unique item, as C asserts.
    pub fn gen_item(&mut self, who: i32, item: i32, qty: i32) {
        assert_eq!(item_unique(&self.db, item), 0, "gen_item: unique {item}");
        self.add_item(who, item, qty);
    }

    /// `olympia/u.c:add_item`: adds `qty` of `item` to `who`'s inventory.
    /// A character's faction learns the item's lore, and a trade the new
    /// stock might fill is queued for matching.
    ///
    /// # Panics
    ///
    /// Panics if `who` or `item` isn't a valid box, or `qty` is negative,
    /// as C asserts.
    fn add_item(&mut self, who: i32, item: i32, qty: i32) {
        assert!(self.db.valid_box(who), "add_item: invalid box {who}");
        assert!(self.db.valid_box(item), "add_item: invalid item {item}");
        assert!(qty >= 0, "add_item: qty {qty}");

        let lore = item_lore(&self.db, item);
        if lore != 0 && self.db.kind(who) == Kind::CHAR && !test_known(&self.db, who, item) {
            queue_lore(&mut self.db, who, item, false);
        }

        let items = &mut self.db.entity_mut(who).items;
        let old = if let Some(e) = items.iter_mut().find(|e| e.item == item) {
            let old = e.qty;
            e.qty += qty;
            old
        } else {
            items.push(ItemEnt { item, qty });
            0
        };
        self.investigate_possible_trade(who, item, old);
    }

    /// `olympia/u.c:sub_item`: takes `qty` of `item` from `who`, if it
    /// holds that many. Returns whether it did.
    ///
    /// # Panics
    ///
    /// Panics if `who` or `item` isn't a valid box, or `qty` is negative,
    /// as C asserts.
    pub(crate) fn sub_item(&mut self, who: i32, item: i32, qty: i32) -> bool {
        assert!(self.db.valid_box(who), "sub_item: invalid box {who}");
        assert!(self.db.valid_box(item), "sub_item: invalid item {item}");
        assert!(qty >= 0, "sub_item: qty {qty}");
        match self
            .db
            .entity_mut(who)
            .items
            .iter_mut()
            .find(|e| e.item == item)
        {
            Some(e) if e.qty >= qty => {
                e.qty -= qty;
                true
            }
            _ => false,
        }
    }

    /// `olympia/u.c:consume_item`: uses up `qty` of an ordinary item.
    /// Returns whether `who` had that many.
    ///
    /// # Panics
    ///
    /// Panics for a unique item, as C asserts.
    pub fn consume_item(&mut self, who: i32, item: i32, qty: i32) -> bool {
        assert_eq!(
            item_unique(&self.db, item),
            0,
            "consume_item: unique {item}"
        );
        self.sub_item(who, item, qty)
    }

    /// `olympia/u.c:move_item`: moves `qty` of `item` from `from` to `to`;
    /// to 0 discards them (`drop_item`). Returns whether `from` had that
    /// many.
    ///
    /// # Errors
    ///
    /// Returns [`NotYetImplemented`] for dropping a unique item (which goes
    /// to the province, for EXPLORE to find) and for moving an NPC token
    /// (`move_token`), both of which come with the item commands.
    ///
    /// # Panics
    ///
    /// Panics if more than one of a unique item is moved, as C asserts.
    pub fn move_item(
        &mut self,
        from: i32,
        to: i32,
        item: i32,
        qty: i32,
    ) -> Result<bool, NotYetImplemented> {
        if qty <= 0 {
            return Ok(true);
        }
        if to == 0 {
            // drop_item()
            if item_unique(&self.db, item) == 0 {
                return Ok(self.consume_item(from, item, qty));
            }
            return Err(NotYetImplemented("drop_item (unique item)".to_owned()));
        }
        if !self.sub_item(from, item, qty) {
            return Ok(false);
        }
        self.add_item(to, item, qty);
        if item_unique(&self.db, item) != 0 {
            assert_eq!(qty, 1, "move_item: {qty} of unique {item}");
            self.db.p_item(item).who_has = to;
            if self.db.subkind(item) == SUB_NPC_TOKEN {
                return Err(NotYetImplemented("move_token".to_owned()));
            }
        }
        Ok(true)
    }

    /// `olympia/u.c:create_unique_item`: a new unique item of subkind `sk`
    /// (numbered on the `mint` stream), given to `who`. Returns its number,
    /// or -1 if no number is free.
    pub fn create_unique_item(&mut self, who: i32, sk: Subkind) -> i32 {
        let new = self.new_ent(Kind::ITEM, sk);
        if new < 0 {
            return -1;
        }
        self.db.p_item(new).who_has = who;
        self.add_item(who, new, 1);
        new
    }

    /// `olympia/buy.c:investigate_possible_trade`: queues `who` for trade
    /// matching if, now that it holds more of `item` than its `old_has`,
    /// a buy (for gold) or a sell of the item might go through.
    fn investigate_possible_trade(&mut self, who: i32, item: i32, old_has: i32) {
        let db = &self.db;
        // loop_trade skips trades for invalid items and spent trades.
        let trades = db
            .entity(who)
            .trades
            .iter()
            .filter(|t| db.valid_box(t.item) && t.qty > 0);
        let check = if item == ITEM_GOLD {
            trades
                .filter(|t| t.kind == BUY)
                .any(|t| (old_has - t.have_left) / t.cost < t.qty)
        } else {
            trades
                .filter(|t| t.kind == SELL && t.item == item)
                .any(|t| old_has - t.have_left < t.qty)
        };
        if check {
            self.driver.trades_to_check.push(who);
        }
    }
}

/// `item_peasant`: what a unit with no noble item weighs as.
const ITEM_PEASANT: i32 = 10;

/// What a unit or stack weighs and can carry (`struct weights`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Weights {
    pub animals: i32,
    /// The total weight of the unit or stack.
    pub total_weight: i32,
    /// The weight it can carry walking.
    pub land_cap: i32,
    pub land_weight: i32,
    /// The weight it can carry on horseback.
    pub ride_cap: i32,
    pub ride_weight: i32,
    pub fly_cap: i32,
    pub fly_weight: i32,
}

/// `olympia/u.c:add_item_weight`: adds `qty` of `item` to `w`. An item
/// with a capacity carries (its capacity, or nothing if it is -1) rather
/// than being carried.
fn add_item_weight(db: &Db, item: i32, qty: i32, w: &mut Weights) {
    let it = rp_item(db, item);
    let field = |f: fn(&Item) -> i16| it.map_or(0, |i| i32::from(f(i)));
    let wt = field(|i| i.weight) * qty;
    let lc = field(|i| i.land_cap);
    let rc = field(|i| i.ride_cap);
    let fc = field(|i| i.fly_cap);

    if lc != 0 {
        w.land_cap += lc.max(0) * qty;
    } else {
        w.land_weight += wt;
    }
    if rc != 0 {
        w.ride_cap += rc.max(0) * qty;
    } else {
        w.ride_weight += wt;
    }
    if fc != 0 {
        w.fly_cap += fc.max(0) * qty;
    } else {
        w.fly_weight += wt;
    }
    w.total_weight += wt;

    if it.is_some_and(|i| i.animal != 0) {
        w.animals += qty;
    }
}

/// `olympia/u.c:determine_unit_weights`: what the character `who` and
/// its inventory weigh and can carry.
///
/// # Panics
///
/// Panics if `who` isn't a character, as C asserts.
#[must_use]
pub fn determine_unit_weights(db: &Db, who: i32) -> Weights {
    assert_eq!(db.kind(who), Kind::CHAR, "determine_unit_weights: {who}");
    let mut w = Weights::default();
    let unit_base = match noble_item(db, who) {
        0 => ITEM_PEASANT,
        n => n,
    };
    add_item_weight(db, unit_base, 1, &mut w);
    // loop_inv
    for e in &db.entity(who).items {
        if db.valid_box(e.item) && e.qty > 0 {
            add_item_weight(db, e.item, e.qty, &mut w);
        }
    }
    w
}

/// `olympia/u.c:determine_stack_weights`: what `who` and everyone in
/// its stack weigh and can carry.
#[must_use]
pub fn determine_stack_weights(db: &Db, who: i32) -> Weights {
    let mut w = determine_unit_weights(db, who);
    for i in crate::loc::all_here(db, who) {
        let v = determine_unit_weights(db, i);
        w.total_weight += v.total_weight;
        w.land_cap += v.land_cap;
        w.ride_cap += v.ride_cap;
        w.fly_cap += v.fly_cap;
        w.land_weight += v.land_weight;
        w.ride_weight += v.ride_weight;
        w.fly_weight += v.fly_weight;
        w.animals += v.animals;
    }
    w
}

/// `olympia/u.c:count_stack_units`: the characters in `who`'s stack,
/// counting `who`.
#[must_use]
pub fn count_stack_units(db: &Db, who: i32) -> i32 {
    1 + all_char_here(db, who).len() as i32
}

/// `olympia/u.c:count_stack_move_nobles`: the nobles in `who`'s stack
/// that slow its army: `who`, its own faction's units, and any other unit
/// that is neither an NPC nor a prisoner.
#[must_use]
pub fn count_stack_move_nobles(db: &Db, who: i32) -> i32 {
    let pl = player(db, who);
    1 + all_char_here(db, who)
        .into_iter()
        .filter(|&i| player(db, i) == pl || (!is_npc(db, i) && !is_prisoner(db, i)))
        .count() as i32
}

/// `olympia/u.c:count_stack_figures`: the figures in `who`'s stack.
#[must_use]
pub fn count_stack_figures(db: &Db, who: i32) -> i32 {
    count_stack_any(db, who)
}

/// `olympia/u.c:stack_has_item`: how much of `item` the members of
/// `who`'s stack in its faction hold.
#[must_use]
pub fn stack_has_item(db: &Db, who: i32, item: i32) -> i32 {
    let pl = player(db, who);
    all_stack(db, stack_leader(db, who))
        .into_iter()
        .filter(|&i| player(db, i) == pl)
        .map(|i| has_item(db, i, item))
        .sum()
}

impl Game {
    /// `olympia/u.c:stack_sub_item`: takes `qty` of `item` from `who`
    /// first, then from the friendly members of its stack from the leader
    /// down. Returns false, taking nothing, if the stack hasn't that many.
    ///
    /// # Panics
    ///
    /// Panics if the stack's holdings change under it, as C asserts.
    pub fn stack_sub_item(&mut self, who: i32, item: i32, mut qty: i32) -> bool {
        if stack_has_item(&self.db, who, item) < qty {
            return false;
        }

        let n = has_item(&self.db, who, item).min(qty);
        if n > 0 {
            qty -= n;
            self.sub_item(who, item, n);
        }
        assert!(qty >= 0, "stack_sub_item: {qty}");
        if qty == 0 {
            return true;
        }

        // Borrow what is needed from friendly stackmates.
        let pl = player(&self.db, who);
        for i in all_stack(&self.db, stack_leader(&self.db, who)) {
            if qty <= 0 {
                break;
            }
            if player(&self.db, i) != pl {
                continue;
            }
            let n = has_item(&self.db, i, item).min(qty);
            if n > 0 {
                qty -= n;
                self.sub_item(i, item, n);
            }
        }
        assert_eq!(qty, 0, "stack_sub_item: stack_has_item lied");
        true
    }

    /// `olympia/u.c:autocharge`: `who`'s stack pays `amount` gold.
    pub fn autocharge(&mut self, who: i32, amount: i32) -> bool {
        self.stack_sub_item(who, ITEM_GOLD, amount)
    }
}

/// `olympia/move.c:restore_stack_actions`: `who`'s stack stops moving.
pub fn restore_stack_actions(db: &mut Db, who: i32) {
    for i in all_stack(db, who) {
        db.p_char(i).moving = 0;
    }
}
