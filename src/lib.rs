// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Olympia G3 game engine, ported from the C reference implementation in
//! [playbymail/olympia-g3](https://github.com/playbymail/olympia-g3).

pub mod command;
pub mod cstr;
pub mod entity;
pub mod game;
pub mod io;
pub mod lines;
pub mod md5;
pub mod order;
pub mod rnd;
pub mod rng;
pub mod selftest;
pub mod system;
pub mod times;
pub mod trade_route;

mod os;
