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

    /// Empties the list, keeping it allocated if it is (`ilist_clear()`).
    pub fn clear(&mut self) {
        if let Some(v) = &mut self.0 {
            v.clear();
        }
    }

    /// Removes the element at index `i` (`ilist_delete()`).
    ///
    /// # Panics
    ///
    /// Panics if `i` is out of range, as C asserts.
    pub fn delete(&mut self, i: i32) {
        let len = self.len();
        let ix = usize::try_from(i).ok().filter(|&ix| ix < len);
        let ix = ix.unwrap_or_else(|| panic!("ilist_delete: index {i} out of range 0..{len}"));
        self.0.as_mut().expect("non-empty").remove(ix);
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

    /// Sorts the list in place by `key`, which must order the elements
    /// totally (as a `qsort()` comparator must, for any sort to agree).
    pub fn sort_by_key<K: Ord>(&mut self, key: impl FnMut(&i32) -> K) {
        if let Some(v) = &mut self.0 {
            v.sort_unstable_by_key(key);
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
    pub const FOREST: Self = Self(2);
    pub const PLAIN: Self = Self(3);
    pub const MOUNTAIN: Self = Self(4);
    pub const DESERT: Self = Self(5);
    pub const SWAMP: Self = Self(6);
    pub const UNDER: Self = Self(7);
    pub const FAERY_HILL: Self = Self(8);
    pub const ISLAND: Self = Self(9);
    pub const STONE_CIR: Self = Self(10);
    pub const MALLORN_GROVE: Self = Self(11);
    pub const BOG: Self = Self(12);
    pub const CAVE: Self = Self(13);
    pub const CITY: Self = Self(14);
    pub const LAIR: Self = Self(15);
    pub const GRAVEYARD: Self = Self(16);
    pub const RUINS: Self = Self(17);
    pub const BATTLEFIELD: Self = Self(18);
    pub const ENCH_FOREST: Self = Self(19);
    pub const ROCKY_HILL: Self = Self(20);
    pub const TREE_CIRCLE: Self = Self(21);
    pub const PITS: Self = Self(22);
    pub const PASTURE: Self = Self(23);
    pub const OASIS: Self = Self(24);
    pub const YEW_GROVE: Self = Self(25);
    pub const SAND_PIT: Self = Self(26);
    pub const SACRED_GROVE: Self = Self(27);
    pub const POPPY_FIELD: Self = Self(28);
    pub const TEMPLE: Self = Self(29);
    pub const GALLEY: Self = Self(30);
    pub const ROUNDSHIP: Self = Self(31);
    pub const CASTLE: Self = Self(32);
    pub const GALLEY_NOTDONE: Self = Self(33);
    pub const ROUNDSHIP_NOTDONE: Self = Self(34);
    pub const GHOST_SHIP: Self = Self(35);
    pub const TEMPLE_NOTDONE: Self = Self(36);
    pub const INN: Self = Self(37);
    pub const INN_NOTDONE: Self = Self(38);
    pub const CASTLE_NOTDONE: Self = Self(39);
    pub const MINE: Self = Self(40);
    pub const MINE_NOTDONE: Self = Self(41);
    pub const TOWER: Self = Self(46);
    pub const TOWER_NOTDONE: Self = Self(47);
    pub const PL_REGULAR: Self = Self(49);
    pub const REGION: Self = Self(50);
    pub const MINE_COLLAPSED: Self = Self(53);
    pub const FOG: Self = Self(57);
    pub const WIND: Self = Self(58);
    pub const RAIN: Self = Self(59);
    pub const HADES_PIT: Self = Self(60);
    pub const GARRISON: Self = Self(64);
    pub const CLOUD: Self = Self(65);
    pub const RAFT: Self = Self(66);
    pub const RAFT_NOTDONE: Self = Self(67);
    pub const TUNNEL: Self = Self(70);
    pub const SEWER: Self = Self(71);
    pub const CHAMBER: Self = Self(72);

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

/// `MONTH_DAYS`: days in a game month (a turn).
pub const MONTH_DAYS: i16 = 30;

/// `NUM_MONTHS`: game months in a year.
pub const NUM_MONTHS: i32 = 8;

impl OlyTime {
    /// Advances to the next day (`olympia/u.c:olytime_increment`).
    pub fn increment(&mut self) {
        self.days_since_epoch += 1;
        // C's `short` increment truncates on the store.
        self.day = self.day.wrapping_add(1);
    }

    /// Readies the clock for the next turn: day 0 of the next turn
    /// (`olympia/u.c:olytime_turn_change`). Day 1 follows from
    /// [`increment`](Self::increment); the days since the epoch don't move
    /// until then.
    pub fn turn_change(&mut self) {
        self.day = 0;
        self.turn = self.turn.wrapping_add(1);
    }

    /// `oly_month()`: the month of the year, 0-based. It is -1 at turn 0.
    #[must_use]
    pub fn month(&self) -> i32 {
        (i32::from(self.turn) - 1) % NUM_MONTHS
    }
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
    /// The locations the faction's units touched this turn (`locs`). Not
    /// saved; the reports read it.
    pub locs: IList,
    /// The lore sheets owed this turn (`deliver_lore`). Not saved.
    pub deliver_lore: IList,
    /// The provinces the faction's weather mages saw this turn
    /// (`weather_seen`). Not saved.
    pub weather_seen: IList,
    /// The commands the faction's units started this turn (`cmd_count`).
    /// Not saved.
    pub cmd_count: i16,
    /// The noble points gained this turn (`np_gained`). Not saved.
    pub np_gained: i16,
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
    /// Hired this turn, so its loyalty doesn't erode (`fresh_hire`). Not
    /// saved.
    pub fresh_hire: i8,
    /// Got a new lord this turn (`new_lord`). Not saved.
    pub new_lord: i8,
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
    /// Pillaged this month (`recent_loot`). Not saved.
    pub recent_loot: i8,
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
    /// Held back a day by an auto-attack (`second_wait`). Not saved.
    pub second_wait: bool,
    /// The `wait` command's parsed conditions (`wait_parse`). Not saved.
    pub wait_parse: Vec<WaitArg>,
    /// The day `finish_command()` last ran (`debug`). Not saved.
    pub debug: i8,
}

/// One parsed condition of a `wait` command (`struct wait_arg`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaitArg {
    /// The condition's index in `wait_tags`, aliases folded.
    pub tag: i32,
    pub a1: i32,
    pub a2: i32,
    /// The flag a `flag` condition waits for.
    pub flag: Option<Text>,
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

/// The trade kinds: a unit's or a city's offers to buy or sell
/// (`BUY`, `SELL`), and the standing production and consumption a city's
/// offers are restocked from (`PRODUCE`, `CONSUME`).
pub const BUY: i32 = 1;
pub const SELL: i32 = 2;
pub const PRODUCE: i32 = 3;
pub const CONSUME: i32 = 4;

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
    /// Where the box comes in the reports (`output_order`). Not saved.
    pub output_order: i32,
    /// The next box of the same kind (`x_next_kind`); see [`Db::kind_next`].
    x_next_kind: i32,
    /// The next box of the same subkind (`x_next_sub`); see
    /// [`Db::sub_next`].
    x_next_sub: i32,
}

/// `T_MAX`: one past the highest kind.
pub const T_MAX: usize = KIND_NAMES.len();

/// `SUB_MAX`: one past the highest subkind.
pub const SUB_MAX: usize = SUBKIND_NAMES.len();

/// Which of the two box chains a link belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chain {
    /// Boxes of one kind (`box_head`, `x_next_kind`).
    Kind,
    /// Boxes of one subkind (`sub_head`, `x_next_sub`).
    Sub,
}

/// The shortcut `add_next_chain()` and `add_sub_chain()` keep: the box added
/// last and its chain. A box numbered above it is appended to it directly if
/// it still ends a chain. Removing that box from its chain drops the
/// shortcut.
#[derive(Debug, Clone, Copy)]
struct ChainCache {
    last: i32,
    key: i32,
}

impl ChainCache {
    /// No shortcut: C's initial and cleared value, whose key matches no box
    /// added to `chain` (kind 0 boxes go on no kind chain).
    const fn empty(chain: Chain) -> Self {
        match chain {
            Chain::Kind => Self { last: 0, key: 0 },
            Chain::Sub => Self { last: 0, key: -1 },
        }
    }
}

/// The entity table (`bx[]`), with every box threaded on a chain of its
/// kind and a chain of its subkind (`olympia/code.c`).
///
/// Each chain is kept in ascending box order. Like C, adding a box first
/// tries a shortcut: if it is numbered above the last box added to the same
/// chain, and that box has no successor, it is appended there. Taking that
/// box off its chain (`delete_box`, a kind or subkind change) resets the
/// shortcut (`olympia/code.c:remove_next_chain`, `remove_sub_chain`).
#[derive(Debug, Clone)]
pub struct Db {
    boxes: Vec<Option<Box<Entity>>>,
    box_head: [i32; T_MAX],
    sub_head: [i32; SUB_MAX],
    kind_cache: ChainCache,
    sub_cache: ChainCache,
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
            box_head: [0; T_MAX],
            sub_head: [0; SUB_MAX],
            kind_cache: ChainCache::empty(Chain::Kind),
            sub_cache: ChainCache::empty(Chain::Sub),
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

    /// Allocates box `n` and threads it on its kind and subkind chains
    /// (`olympia/code.c:alloc_box`). To allocate a box at a number the
    /// `mint` stream picks, use [`Game::new_ent`](crate::game::Game::new_ent).
    ///
    /// # Panics
    ///
    /// Panics if `n` is out of range or already allocated, as C asserts.
    pub fn alloc_box(&mut self, n: i32, kind: Kind, sk: Subkind) {
        assert!(n > 0 && n < MAX_BOXES, "alloc_box: box {n} out of range");
        let slot = &mut self.boxes[n as usize];
        if slot.is_some() {
            eprintln!("alloc_box: DUP box {n}");
            panic!("alloc_box: DUP box {n}");
        }
        *slot = Some(Box::new(Entity {
            kind,
            skind: sk,
            ..Entity::default()
        }));
        self.add_chain(n, Chain::Kind);
        self.add_chain(n, Chain::Sub);
    }

    /// Takes box `n` off its chains and marks it deleted
    /// (`olympia/code.c:delete_box`). It stays allocated, so its number is
    /// not reused until the next load.
    ///
    /// # Panics
    ///
    /// Panics if `n` is not allocated or is already deleted, as C asserts.
    pub fn delete_box(&mut self, n: i32) {
        self.remove_chain(n, Chain::Kind);
        self.remove_chain(n, Chain::Sub);
        self.entity_mut(n).kind = Kind::DELETED;
    }

    /// Changes box `n`'s kind, moving it to the new kind's chain
    /// (`olympia/code.c:change_box_kind`).
    ///
    /// # Panics
    ///
    /// Panics if `n` is not allocated or is deleted, as C asserts.
    pub fn change_box_kind(&mut self, n: i32, kind: Kind) {
        self.remove_chain(n, Chain::Kind);
        self.entity_mut(n).kind = kind;
        self.add_chain(n, Chain::Kind);
    }

    /// Changes box `n`'s subkind, moving it to the new subkind's chain
    /// (`olympia/code.c:change_box_subkind`).
    ///
    /// # Panics
    ///
    /// Panics if `n` is not allocated.
    pub fn change_box_subkind(&mut self, n: i32, sk: Subkind) {
        if self.subkind(n) == sk {
            return;
        }
        self.remove_chain(n, Chain::Sub);
        self.entity_mut(n).skind = sk;
        self.add_chain(n, Chain::Sub);
    }

    /// The first box of `kind` (`kind_first()`), or 0.
    #[must_use]
    pub fn kind_first(&self, kind: Kind) -> i32 {
        self.box_head[chain_index(i32::from(kind.0))]
    }

    /// The box after `n` on its kind chain (`kind_next()`), or 0.
    ///
    /// To loop as C's `loop_kind` does, read the next box before acting on
    /// the current one: boxes added to the chain ahead of the loop are
    /// visited, and the current box may be deleted.
    #[must_use]
    pub fn kind_next(&self, n: i32) -> i32 {
        self.entity(n).x_next_kind
    }

    /// The first box of subkind `sk` (`sub_first()`), or 0.
    #[must_use]
    pub fn sub_first(&self, sk: Subkind) -> i32 {
        self.sub_head[chain_index(i32::from(sk.0))]
    }

    /// The box after `n` on its subkind chain (`sub_next()`), or 0.
    #[must_use]
    pub fn sub_next(&self, n: i32) -> i32 {
        self.entity(n).x_next_sub
    }

    /// The boxes on the chain of `kind` (`loop_kind`), in chain order, which
    /// is ascending.
    ///
    /// This is a snapshot. It visits what `loop_kind` would as long as the
    /// loop body doesn't add boxes of `kind`; one that does must walk the
    /// chain with [`kind_first`](Self::kind_first) and
    /// [`kind_next`](Self::kind_next) instead.
    #[must_use]
    pub fn boxes_of_kind(&self, kind: Kind) -> Vec<i32> {
        let mut v = Vec::new();
        let mut i = self.kind_first(kind);
        while i > 0 {
            v.push(i);
            i = self.kind_next(i);
        }
        v
    }

    /// The boxes on the chain of subkind `sk` (`loop_subkind`), in chain
    /// order. A snapshot, like [`boxes_of_kind`](Self::boxes_of_kind).
    #[must_use]
    pub fn boxes_of_subkind(&self, sk: Subkind) -> Vec<i32> {
        let mut v = Vec::new();
        let mut i = self.sub_first(sk);
        while i > 0 {
            v.push(i);
            i = self.sub_next(i);
        }
        v
    }

    /// The chain key of box `n`: its kind or its subkind.
    fn chain_key(&self, n: i32, chain: Chain) -> i32 {
        let b = self.entity(n);
        match chain {
            Chain::Kind => i32::from(b.kind.0),
            Chain::Sub => i32::from(b.skind.0),
        }
    }

    fn next_link(&self, n: i32, chain: Chain) -> i32 {
        let b = self.entity(n);
        match chain {
            Chain::Kind => b.x_next_kind,
            Chain::Sub => b.x_next_sub,
        }
    }

    fn set_next_link(&mut self, n: i32, chain: Chain, next: i32) {
        let b = self.entity_mut(n);
        match chain {
            Chain::Kind => b.x_next_kind = next,
            Chain::Sub => b.x_next_sub = next,
        }
    }

    fn head_mut(&mut self, chain: Chain, key: i32) -> &mut i32 {
        let i = chain_index(key);
        match chain {
            Chain::Kind => &mut self.box_head[i],
            Chain::Sub => &mut self.sub_head[i],
        }
    }

    fn cache_mut(&mut self, chain: Chain) -> &mut ChainCache {
        match chain {
            Chain::Kind => &mut self.kind_cache,
            Chain::Sub => &mut self.sub_cache,
        }
    }

    /// Threads box `n` onto a chain in ascending order
    /// (`olympia/code.c:add_next_chain`, `add_sub_chain`). Deleted boxes
    /// (kind 0) go on no kind chain; subkind 0 has a chain like any other.
    fn add_chain(&mut self, n: i32, chain: Chain) {
        let key = self.chain_key(n, chain);
        if chain == Chain::Kind && key == 0 {
            return;
        }

        // The shortcut ("optim!" in C).
        let cache = *self.cache_mut(chain);
        if cache.key == key && n > cache.last && self.next_link(cache.last, chain) == 0 {
            self.set_next_link(cache.last, chain, n);
            self.set_next_link(n, chain, 0);
            self.cache_mut(chain).last = n;
            return;
        }
        *self.cache_mut(chain) = ChainCache { last: n, key };

        let head = *self.head_mut(chain, key);
        if head == 0 {
            *self.head_mut(chain, key) = n;
            self.set_next_link(n, chain, 0);
            return;
        }
        if n < head {
            self.set_next_link(n, chain, head);
            *self.head_mut(chain, key) = n;
            return;
        }
        let mut i = head;
        loop {
            let next = self.next_link(i, chain);
            if !(next > 0 && next < n) {
                break;
            }
            i = next;
        }
        let next = self.next_link(i, chain);
        self.set_next_link(n, chain, next);
        self.set_next_link(i, chain, n);
    }

    /// Takes box `n` off a chain (`olympia/code.c:remove_next_chain`,
    /// `remove_sub_chain`).
    ///
    /// # Panics
    ///
    /// Panics if `n` isn't on the chain, as C asserts.
    fn remove_chain(&mut self, n: i32, chain: Chain) {
        let key = self.chain_key(n, chain);
        let next = self.next_link(n, chain);
        let head = *self.head_mut(chain, key);
        if head == n {
            *self.head_mut(chain, key) = next;
        } else {
            let mut i = head;
            while i > 0 && self.next_link(i, chain) != n {
                i = self.next_link(i, chain);
            }
            assert!(i > 0, "box {n} is not on its {chain:?} chain");
            self.set_next_link(i, chain, next);
        }
        self.set_next_link(n, chain, 0);

        if n == self.cache_mut(chain).last {
            *self.cache_mut(chain) = ChainCache::empty(chain);
        }
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

    /// `p_misc(n)`: the misc struct, allocated on demand.
    pub fn p_misc(&mut self, n: i32) -> &mut Misc {
        self.entity_mut(n).misc.get_or_insert_with(Box::default)
    }

    /// `p_loc(n)`: the location struct, allocated on demand.
    pub fn p_loc(&mut self, n: i32) -> &mut Loc {
        self.entity_mut(n).loc.get_or_insert_with(Box::default)
    }

    /// `p_subloc(n)`: the sublocation struct, allocated on demand.
    pub fn p_subloc(&mut self, n: i32) -> &mut Subloc {
        self.entity_mut(n).subloc.get_or_insert_with(Box::default)
    }

    /// `p_disp(n)`: the attitude struct, allocated on demand.
    pub fn p_disp(&mut self, n: i32) -> &mut Disp {
        self.entity_mut(n).disp.get_or_insert_with(Box::default)
    }
}

/// The index of a chain head for a kind or subkind.
///
/// # Panics
///
/// Panics if it has no chain; C would index past `box_head` or `sub_head`.
fn chain_index(key: i32) -> usize {
    usize::try_from(key).unwrap_or_else(|_| panic!("no chain for kind or subkind {key}"))
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

    #[test]
    fn chains_stay_in_box_order() {
        let mut db = Db::new();
        for n in [50, 10, 30, 60, 20] {
            db.alloc_box(n, Kind::CHAR, Subkind::default());
        }
        db.alloc_box(40, Kind::LOC, Subkind::CITY);
        db.alloc_box(35, Kind::LOC, Subkind::CITY);
        assert_eq!(db.boxes_of_kind(Kind::CHAR), [10, 20, 30, 50, 60]);
        assert_eq!(
            db.boxes_of_subkind(Subkind::default()),
            [10, 20, 30, 50, 60]
        );
        assert_eq!(db.boxes_of_subkind(Subkind::CITY), [35, 40]);

        db.delete_box(30);
        db.change_box_kind(50, Kind::DEADCHAR);
        db.change_box_subkind(20, Subkind::CITY);
        assert_eq!(db.boxes_of_kind(Kind::CHAR), [10, 20, 60]);
        assert_eq!(db.boxes_of_kind(Kind::DEADCHAR), [50]);
        assert_eq!(db.boxes_of_subkind(Subkind::CITY), [20, 35, 40]);
        assert_eq!(db.boxes_of_subkind(Subkind::default()), [10, 50, 60]);
        assert_eq!(db.kind(30), Kind::DELETED);
        assert!(db.get(30).is_some(), "a deleted box stays allocated");
    }

    #[test]
    fn a_box_added_after_the_shortcut_box_is_deleted_is_on_its_chains() {
        let mut db = Db::new();
        db.alloc_box(10, Kind::STORM, Subkind(59));
        db.alloc_box(20, Kind::STORM, Subkind(59));
        db.delete_box(20);
        // Deleting 20 dropped the shortcut, so 30 is not appended to it.
        db.alloc_box(30, Kind::STORM, Subkind(59));
        assert_eq!(db.boxes_of_kind(Kind::STORM), [10, 30]);
        assert_eq!(db.boxes_of_subkind(Subkind(59)), [10, 30]);
    }
}
