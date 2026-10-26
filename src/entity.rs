// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The entity database: boxes and their substructures (`olympia/oly.h`).
//!
//! Field names follow the C structs so a divergence can be traced back.
//! Structs that `save_db()` prints only when `zero_check()` finds a nonzero
//! byte keep their unsaved fields too, and distinguish a null list or string
//! from an empty one, because C's raw-byte check does.

/// A byte string from the data files.
pub type Text = Vec<u8>;

/// `MAX_BOXES`: entity numbers run from 1 to `MAX_BOXES - 1`.
pub const MAX_BOXES: i32 = 150_000;

/// An `ilist`: a list of ints that is either null or allocated.
///
/// C's lists stay allocated when emptied, and `zero_check()` sees the
/// pointer, so an allocated empty list is not the same as a null one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct IList(Option<Vec<i32>>);

impl IList {
    /// The elements, in order.
    #[must_use]
    pub fn as_slice(&self) -> &[i32] {
        self.0.as_deref().unwrap_or(&[])
    }

    /// Appends `n`, allocating the list if it is null (`ilist_append()`).
    pub fn append(&mut self, n: i32) {
        self.0.get_or_insert_with(Vec::new).push(n);
    }

    /// Removes every `n`, keeping the list allocated (`ilist_rem_value()`).
    pub fn rem_value(&mut self, n: i32) {
        if let Some(v) = &mut self.0 {
            v.retain(|&x| x != n);
        }
    }

    /// Whether `n` is in the list (`ilist_lookup() != -1`).
    #[must_use]
    pub fn contains(&self, n: i32) -> bool {
        self.as_slice().contains(&n)
    }

    /// Sorts the list in place, as `loop_known` does before iterating.
    pub fn sort(&mut self) {
        if let Some(v) = &mut self.0 {
            v.sort_unstable();
        }
    }

    /// The number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Whether the list has no elements, null or not.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// An entity kind (`T_xxx`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Kind(pub i8);

impl Kind {
    pub const DELETED: Self = Self(0);
    pub const PLAYER: Self = Self(1);
    pub const CHAR: Self = Self(2);
    pub const LOC: Self = Self(3);
    pub const ITEM: Self = Self(4);
    pub const SKILL: Self = Self(5);
    pub const GATE: Self = Self(6);
    pub const ROAD: Self = Self(7);
    pub const DEADCHAR: Self = Self(8);
    pub const SHIP: Self = Self(9);
    pub const POST: Self = Self(10);
    pub const STORM: Self = Self(11);
    pub const UNFORM: Self = Self(12);
    pub const LORE: Self = Self(13);

    /// The kind's name in the data files (`kind_s[]`).
    ///
    /// # Panics
    ///
    /// Panics for a kind with no name; C would read past the table.
    #[must_use]
    pub fn name(self) -> &'static str {
        usize::try_from(self.0)
            .ok()
            .and_then(|i| KIND_NAMES.get(i))
            .unwrap_or_else(|| panic!("no name for kind {}", self.0))
    }

    /// Looks a kind up by name (`lookup(kind_s, s)`).
    #[must_use]
    pub fn from_name(s: &[u8]) -> Option<Self> {
        KIND_NAMES
            .iter()
            .position(|n| n.as_bytes() == s)
            .map(|i| Self(i as i8))
    }
}

const KIND_NAMES: [&str; 14] = [
    "deleted", "player", "char", "loc", "item", "skill", "gate", "road", "deadchar", "ship",
    "post", "storm", "unform", "lore",
];

/// An entity subkind (`sub_xxx`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Subkind(pub i8);

impl Subkind {
    pub const OCEAN: Self = Self(1);
    pub const PL_REGULAR: Self = Self(49);

    /// The subkind's name in the data files (`subkind_s[]`).
    ///
    /// # Panics
    ///
    /// Panics for a subkind with no name; C would read past the table.
    #[must_use]
    pub fn name(self) -> &'static str {
        usize::try_from(self.0)
            .ok()
            .and_then(|i| SUBKIND_NAMES.get(i))
            .unwrap_or_else(|| panic!("no name for subkind {}", self.0))
    }

    /// Looks a subkind up by name (`lookup(subkind_s, s)`).
    #[must_use]
    pub fn from_name(s: &[u8]) -> Option<Self> {
        SUBKIND_NAMES
            .iter()
            .position(|n| n.as_bytes() == s)
            .map(|i| Self(i as i8))
    }
}

const SUBKIND_NAMES: [&str; 74] = [
    "<no subkind>",
    "ocean",
    "forest",
    "plain",
    "mountain",
    "desert",
    "swamp",
    "underground",
    "faery hill",
    "island",
    "ring of stones",
    "mallorn grove",
    "bog",
    "cave",
    "city",
    "lair",
    "graveyard",
    "ruins",
    "battlefield",
    "enchanted forest",
    "rocky hill",
    "circle of trees",
    "pits",
    "pasture",
    "oasis",
    "yew grove",
    "sand pit",
    "sacred grove",
    "poppy field",
    "temple",
    "galley",
    "roundship",
    "castle",
    "galley-in-progress",
    "roundship-in-progress",
    "ghost ship",
    "temple-in-progress",
    "inn",
    "inn-in-progress",
    "castle-in-progress",
    "mine",
    "mine-in-progress",
    "scroll",
    "magic",
    "palantir",
    "auraculum",
    "tower",
    "tower-in-progress",
    "pl_system",
    "pl_regular",
    "region",
    "pl_savage",
    "pl_npc",
    "collapsed mine",
    "ni",
    "demon lord",
    "dead body",
    "fog",
    "wind",
    "rain",
    "pit",
    "artifact",
    "pl_silent",
    "npc_token",
    "garrison",
    "cloud",
    "raft",
    "raft-in-progress",
    "suffuse_ring",
    "relic",
    "tunnel",
    "sewer",
    "chamber",
    "tradegood",
];

/// A game date (`olytime`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OlyTime {
    pub day: i16,
    pub turn: i16,
    pub days_since_epoch: i32,
}

/// Where an entity is and what is in it (`struct loc_info`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocInfo {
    pub where_: i32,
    pub here_list: IList,
}

/// One unit's queued orders (`struct order_list`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderList {
    pub unit: i32,
    pub l: Vec<Text>,
}

/// An ADMIT permission (`struct admit`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Admit {
    pub targ: i32,
    pub sense: i32,
    pub l: IList,
}

/// A faction (`struct entity_player`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Player {
    pub full_name: Option<Text>,
    pub email: Option<Text>,
    pub vis_email: Option<Text>,
    pub last_email: Option<Text>,
    pub password: Option<Text>,
    pub first_turn: i32,
    pub last_order_turn: i32,
    pub orders: Vec<OrderList>,
    pub known: IList,
    pub units: IList,
    pub admits: Vec<Admit>,
    pub unformed: IList,
    pub split_lines: i32,
    pub split_bytes: i32,
    pub fast_study: i16,
    pub noble_points: i16,
    pub format: i8,
    pub notab: i8,
    pub first_tower: i8,
    pub sent_orders: i8,
    pub dont_remind: i8,
    pub compuserve: i8,
    pub broken_mailer: i8,
}

/// Attitudes (`struct att_ent`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Disp {
    pub neutral: IList,
    pub hostile: IList,
    pub defend: IList,
}

/// A known skill (`struct skill_ent`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkillEnt {
    pub skill: i32,
    pub days_studied: i32,
    pub experience: i16,
    pub know: i8,
}

/// A held item (`struct item_ent`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemEnt {
    pub item: i32,
    pub qty: i32,
}

/// A character (`struct entity_char`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Char {
    pub unit_item: i32,
    pub health: i8,
    pub sick: i8,
    pub guard: i8,
    pub loy_kind: i8,
    pub loy_rate: i32,
    pub death_time: OlyTime,
    pub skills: Vec<SkillEnt>,
    pub moving: i32,
    pub unit_lord: i32,
    pub prev_lord: i32,
    pub contact: IList,
    pub magic: Option<Box<CharMagic>>,
    pub prisoner: i8,
    pub behind: i8,
    pub time_flying: i8,
    pub break_point: i8,
    pub rank: i8,
    pub npc_prog: i8,
    pub attack: i16,
    pub defense: i16,
    pub missile: i16,
}

/// A magician's state (`struct char_magic`). Zero-checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharMagic {
    pub max_aura: i32,
    pub cur_aura: i32,
    pub auraculum: i32,
    pub visions: IList,
    pub pledge: i32,
    pub token: i32,
    pub fee: i32,
    pub project_cast: i32,
    pub quick_cast: i16,
    pub ability_shroud: i16,
    pub hide_mage: i8,
    pub hinder_meditation: i8,
    pub magician: i8,
    pub pray: i8,
    pub aura_reflect: i8,
    pub hide_self: i8,
    pub swear_on_release: i8,
    pub knows_weather: i8,
    pub vis_protect: i8,
    pub default_garr: i8,
    /// Not saved.
    pub mage_worked: i8,
    /// Not saved.
    pub ferry_flag: i8,
    /// Not saved.
    pub pledged_to_us: IList,
}

/// A location (`struct entity_loc`). Zero-checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loc {
    pub prov_dest: IList,
    pub shroud: i16,
    pub barrier: i16,
    pub civ: i8,
    pub hidden: i8,
    pub dist_from_gate: i8,
    pub sea_lane: i8,
    /// Not saved: temporary list link.
    pub next: i32,
}

/// A sublocation, structure or ship (`struct entity_subloc`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subloc {
    pub teaches: IList,
    pub opium_econ: i32,
    pub defense: i32,
    pub loot: i8,
    pub damage: u8,
    pub galley_ram: i8,
    pub shaft_depth: i16,
    pub castle_lev: i8,
    pub build_materials: i8,
    pub effort_required: i32,
    pub effort_given: i32,
    pub moving: i32,
    pub capacity: i32,
    pub near_cities: IList,
    pub safe: i8,
    pub major: i8,
    pub prominence: i8,
    pub uldim_flag: i8,
    pub summer_flag: i8,
    pub quest_late: i8,
    pub tunnel_level: i8,
    pub link_when: i8,
    pub link_open: i8,
    pub link_to: IList,
    pub link_from: IList,
    pub bound_storms: IList,
}

/// An item kind (`struct entity_item`). Zero-checked, including whether it
/// has an [`ItemMagic`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub weight: i16,
    pub land_cap: i16,
    pub ride_cap: i16,
    pub fly_cap: i16,
    pub attack: i16,
    pub defense: i16,
    pub missile: i16,
    pub is_man_item: i8,
    pub animal: i8,
    pub prominent: i8,
    pub capturable: i8,
    pub plural_name: Option<Text>,
    pub base_price: i32,
    pub who_has: i32,
    pub magic: Option<Box<ItemMagic>>,
}

/// An artifact's magic (`struct item_magic`). Zero-checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemMagic {
    pub creator: i32,
    pub region_created: i32,
    pub lore: i32,
    pub curse_loyalty: i8,
    pub cloak_region: i8,
    pub cloak_creator: i8,
    pub use_key: i8,
    pub may_use: IList,
    pub may_study: IList,
    pub project_cast: i32,
    pub token_ni: i32,
    pub quick_cast: i16,
    pub aura_bonus: i16,
    pub aura: i16,
    pub relic_decay: i16,
    pub attack_bonus: i8,
    pub defense_bonus: i8,
    pub missile_bonus: i8,
    pub token_num: i8,
    pub orb_use_count: i8,
    /// Not saved.
    pub one_turn_use: i8,
}

/// An item a skill requires (`struct req_ent`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReqEnt {
    pub item: i32,
    pub qty: i32,
    pub consume: i8,
}

/// A skill (`struct entity_skill`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Skill {
    pub time_to_learn: i32,
    pub required_skill: i32,
    pub np_req: i32,
    pub offered: IList,
    pub research: IList,
    pub req: Vec<ReqEnt>,
    pub produced: i32,
    pub no_exp: i32,
}

/// A gate or road (`struct entity_gate`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gate {
    pub to_loc: i32,
    pub notify_jumps: i32,
    pub notify_unseal: i32,
    pub seal_key: i16,
    pub road_hidden: i8,
}

/// Everything else (`struct entity_misc`). Zero-checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Misc {
    pub display: Option<Text>,
    pub npc_created: i32,
    pub npc_home: i32,
    pub npc_cookie: i32,
    pub summoned_by: i32,
    pub save_name: Option<Text>,
    pub old_lord: i32,
    pub npc_memory: IList,
    pub only_vulnerable: i32,
    pub garr_castle: i32,
    pub bind_storm: i32,
    pub storm_str: i16,
    pub npc_dir: i8,
    pub mine_delay: i8,
    pub cmd_allow: u8,
    /// Not saved.
    pub opium_double: i8,
    /// Not saved.
    pub post_txt: Option<Vec<Text>>,
    /// Not saved.
    pub storm_move: i32,
    /// Not saved.
    pub garr_watch: IList,
    /// Not saved.
    pub garr_host: IList,
    /// Not saved.
    pub garr_tax: i32,
    /// Not saved.
    pub garr_forward: i32,
}

/// A unit's current command (`struct command`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Command {
    pub who: i32,
    pub wait: i32,
    pub cmd: i32,
    pub use_skill: i32,
    pub use_exp: i32,
    pub days_executing: i32,
    /// The arguments `a` through `h`.
    pub args: [i32; 8],
    pub line: Option<Text>,
    pub parsed_line: Option<Text>,
    pub parse: Vec<Text>,
    pub state: i8,
    pub status: i8,
    pub poll: i8,
    pub pri: i8,
    pub conditional: i8,
    pub inhibit_finish: i8,
    /// Not saved.
    pub fuzzy: bool,
}

/// A pending trade (`struct trade`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Trade {
    pub kind: i32,
    pub item: i32,
    pub qty: i32,
    pub cost: i32,
    pub cloak: i32,
    pub have_left: i32,
    pub month_prod: i32,
    pub expire: i32,
    /// Not saved: the trader.
    pub who: i32,
}

/// `BUY` and `SELL` trade kinds.
pub const BUY: i32 = 1;
pub const SELL: i32 = 2;

/// An entity (`struct box`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entity {
    pub kind: Kind,
    pub skind: Subkind,
    pub name: Option<Text>,
    pub loc_info: LocInfo,
    pub player: Option<Box<Player>>,
    pub char_: Option<Box<Char>>,
    pub loc: Option<Box<Loc>>,
    pub subloc: Option<Box<Subloc>>,
    pub item: Option<Box<Item>>,
    pub skill: Option<Box<Skill>>,
    pub gate: Option<Box<Gate>>,
    pub misc: Option<Box<Misc>>,
    pub disp: Option<Box<Disp>>,
    pub cmd: Option<Box<Command>>,
    pub items: Vec<ItemEnt>,
    pub trades: Vec<Trade>,
    /// Scratch space.
    pub temp: i32,
}

/// The entity table (`bx[]`).
#[derive(Debug, Clone)]
pub struct Db {
    boxes: Vec<Option<Box<Entity>>>,
}

impl Default for Db {
    fn default() -> Self {
        Self::new()
    }
}

impl Db {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            boxes: vec![None; MAX_BOXES as usize],
        }
    }

    /// The box `n`, if it is allocated (`bx[n] != NULL`), deleted or not.
    #[must_use]
    pub fn get(&self, n: i32) -> Option<&Entity> {
        usize::try_from(n)
            .ok()
            .and_then(|i| self.boxes.get(i))
            .and_then(|b| b.as_deref())
    }

    /// The box `n` for changing, if it is allocated.
    pub fn get_mut(&mut self, n: i32) -> Option<&mut Entity> {
        usize::try_from(n)
            .ok()
            .and_then(|i| self.boxes.get_mut(i))
            .and_then(|b| b.as_deref_mut())
    }

    /// The box `n`, which must be allocated.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not allocated.
    #[must_use]
    pub fn entity(&self, n: i32) -> &Entity {
        self.get(n)
            .unwrap_or_else(|| panic!("box {n} is not allocated"))
    }

    /// The box `n` for changing, which must be allocated.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not allocated.
    pub fn entity_mut(&mut self, n: i32) -> &mut Entity {
        self.get_mut(n)
            .unwrap_or_else(|| panic!("box {n} is not allocated"))
    }

    /// `kind(n)`: the box's kind, or [`Kind::DELETED`] if it is out of range
    /// or unallocated.
    #[must_use]
    pub fn kind(&self, n: i32) -> Kind {
        if n > 0 && n < MAX_BOXES {
            self.get(n).map_or(Kind::DELETED, |b| b.kind)
        } else {
            Kind::DELETED
        }
    }

    /// `subkind(n)`.
    #[must_use]
    pub fn subkind(&self, n: i32) -> Subkind {
        self.get(n).map_or(Subkind::default(), |b| b.skind)
    }

    /// `valid_box(n)`.
    #[must_use]
    pub fn valid_box(&self, n: i32) -> bool {
        self.kind(n) != Kind::DELETED
    }

    /// Allocates box `n` (`alloc_box()`).
    ///
    /// # Panics
    ///
    /// Panics if `n` is out of range or already allocated, as C asserts.
    pub fn alloc_box(&mut self, n: i32, kind: Kind, sk: Subkind) {
        assert!(n > 0 && n < MAX_BOXES, "alloc_box: box {n} out of range");
        let slot = &mut self.boxes[n as usize];
        assert!(slot.is_none(), "alloc_box: DUP box {n}");
        *slot = Some(Box::new(Entity {
            kind,
            skind: sk,
            ..Entity::default()
        }));
    }

    /// Marks box `n` deleted (`delete_box()`); it stays allocated.
    pub fn delete_box(&mut self, n: i32) {
        self.entity_mut(n).kind = Kind::DELETED;
    }

    /// The boxes of `kind` in ascending order (`loop_kind`). C keeps each
    /// kind chain sorted by box number.
    #[must_use]
    pub fn boxes_of_kind(&self, kind: Kind) -> Vec<i32> {
        if kind == Kind::DELETED {
            return Vec::new();
        }
        self.allocated()
            .filter(|&n| self.entity(n).kind == kind)
            .collect()
    }

    /// Every allocated box number in ascending order, deleted ones included.
    pub fn allocated(&self) -> impl Iterator<Item = i32> + '_ {
        self.boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_some())
            .map(|(i, _)| i as i32)
    }

    /// `name(n)`: the box's name, or empty.
    #[must_use]
    pub fn name(&self, n: i32) -> &[u8] {
        self.entity(n).name.as_deref().unwrap_or(b"")
    }

    /// `set_name()`: stores `s` with `[` and `]` turned into `{` and `}`.
    pub fn set_name(&mut self, n: i32, s: &[u8]) {
        let name = s
            .iter()
            .map(|&c| match c {
                b'[' => b'{',
                b']' => b'}',
                c => c,
            })
            .collect();
        self.entity_mut(n).name = Some(name);
    }

    /// `loc(n)`: where the box is.
    #[must_use]
    pub fn loc(&self, n: i32) -> i32 {
        self.entity(n).loc_info.where_
    }

    /// Moves `who` to `new_loc`, keeping here lists in step (`set_where()`).
    ///
    /// # Panics
    ///
    /// Panics if `who` is `new_loc`, or the here lists disagree with
    /// `where`, as C asserts.
    pub fn set_where(&mut self, who: i32, new_loc: i32) {
        assert_ne!(who, new_loc, "set_where: {who} into itself");
        let old_loc = self.loc(who);
        if old_loc > 0 {
            let here = &mut self.entity_mut(old_loc).loc_info.here_list;
            assert!(here.contains(who), "{who} not in here list of {old_loc}");
            here.rem_value(who);
        }
        if new_loc > 0 {
            let here = &mut self.entity_mut(new_loc).loc_info.here_list;
            assert!(
                !here.contains(who),
                "{who} already in here list of {new_loc}"
            );
            here.append(who);
        }
        self.entity_mut(who).loc_info.where_ = new_loc;
    }

    /// `p_char(n)`: the character struct, allocated on demand.
    pub fn p_char(&mut self, n: i32) -> &mut Char {
        self.entity_mut(n).char_.get_or_insert_with(Box::default)
    }

    /// `p_magic(n)`: the magic struct, allocating it and the character
    /// struct on demand.
    pub fn p_magic(&mut self, n: i32) -> &mut CharMagic {
        self.p_char(n).magic.get_or_insert_with(Box::default)
    }

    /// `p_item(n)`: the item struct, allocated on demand.
    pub fn p_item(&mut self, n: i32) -> &mut Item {
        self.entity_mut(n).item.get_or_insert_with(Box::default)
    }

    /// `p_item_magic(n)`: the item magic struct, allocating it and the item
    /// struct on demand.
    pub fn p_item_magic(&mut self, n: i32) -> &mut ItemMagic {
        self.p_item(n).magic.get_or_insert_with(Box::default)
    }

    /// `p_player(n)`: the player struct, allocated on demand.
    pub fn p_player(&mut self, n: i32) -> &mut Player {
        self.entity_mut(n).player.get_or_insert_with(Box::default)
    }

    /// `p_disp(n)`: the attitude struct, allocated on demand.
    pub fn p_disp(&mut self, n: i32) -> &mut Disp {
        self.entity_mut(n).disp.get_or_insert_with(Box::default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emptied_list_is_not_null() {
        let mut l = IList::default();
        assert_eq!(l, IList::default());
        l.append(5);
        l.rem_value(5);
        assert!(l.is_empty());
        assert_ne!(l, IList::default(), "still allocated, as in C");
    }

    #[test]
    fn kind_of_unallocated_or_out_of_range_boxes_is_deleted() {
        let mut db = Db::new();
        db.alloc_box(10, Kind::ITEM, Subkind::default());
        assert_eq!(db.kind(10), Kind::ITEM);
        assert_eq!(db.kind(11), Kind::DELETED);
        assert_eq!(db.kind(0), Kind::DELETED);
        assert_eq!(db.kind(-1), Kind::DELETED);
        assert_eq!(db.kind(MAX_BOXES), Kind::DELETED);
    }

    #[test]
    fn set_name_replaces_brackets() {
        let mut db = Db::new();
        db.alloc_box(10, Kind::ITEM, Subkind::default());
        db.set_name(10, b"a [b] c");
        assert_eq!(db.name(10), b"a {b} c");
    }

    #[test]
    fn kind_and_subkind_names_round_trip() {
        assert_eq!(Kind::from_name(b"unform"), Some(Kind::UNFORM));
        assert_eq!(Kind::UNFORM.name(), "unform");
        assert_eq!(Subkind::PL_REGULAR.name(), "pl_regular");
        assert_eq!(Subkind::from_name(b"tradegood"), Some(Subkind(73)));
    }
}
