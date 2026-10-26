// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Olympia G3 game engine, ported from the C reference implementation in
//! [playbymail/olympia-g3](https://github.com/playbymail/olympia-g3).

pub mod art;
pub mod buy;
pub mod check;
pub mod code;
pub mod combat;
pub mod command;
pub mod cstr;
pub mod damage;
pub mod day;
pub mod dir;
pub mod entity;
pub mod enty;
pub mod faery;
pub mod game;
pub mod garr;
pub mod hades;
pub mod input;
pub mod io;
pub mod lines;
pub mod loc;
pub mod lore;
pub mod md5;
pub mod mint;
pub mod month;
pub mod movement;
pub mod npc;
pub mod order;
pub mod produce;
pub mod rnd;
pub mod rng;
pub mod savage;
pub mod seed;
pub mod selftest;
pub mod storm;
pub mod swear;
pub mod system;
pub mod times;
pub mod trade_route;
pub mod turn;
pub mod unit;
pub mod wait;

mod os;
