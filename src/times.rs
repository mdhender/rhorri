// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Olympia Times masthead (`olympia/c2.c:times_masthead`).

use std::fs::File;
use std::io::Write;
use std::process;

use chrono::{Datelike, Local};

use crate::entity::{Kind, NUM_MONTHS, Subkind};
use crate::game::Game;

/// The game months (`month_names[]`).
const MONTH_NAMES: [&str; 8] = [
    "Fierce winds",
    "Snowmelt",
    "Blossom bloom",
    "Sunsear",
    "Thunder and rain",
    "Harvest",
    "Waning days",
    "Dark night",
];

/// The calendar months (`months[]`), spelled as C spells them.
const MONTHS: [&str; 12] = [
    "January",
    "Februrary",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Pads `s` to `width` bytes like `printf("%*s")`: on the left for a
/// positive width, on the right for a negative one.
fn pad(s: &[u8], width: i32) -> Vec<u8> {
    let fill = (width.unsigned_abs() as usize).saturating_sub(s.len());
    let mut out = Vec::with_capacity(s.len() + fill);
    if width < 0 {
        out.extend_from_slice(s);
        out.resize(s.len() + fill, b' ');
    } else {
        out.resize(fill, b' ');
        out.extend_from_slice(s);
    }
    out
}

/// Writes `times_0`, the Times masthead.
///
/// Like C, exits 1 if the file can't be created.
pub fn times_masthead(game: &Game) {
    let path = game.lib_path("times_0");
    let Ok(mut file) = File::create(&path) else {
        eprint!("can't write {}", path.display());
        process::exit(1);
    };

    let date = if game.test_use_const_report_date {
        "January 1, 2000".to_owned()
    } else {
        let now = Local::now();
        format!(
            "{} {}, {}",
            MONTHS[now.month0() as usize],
            now.day(),
            now.year()
        )
    };

    let nplayers = game
        .db
        .boxes_of_kind(Kind::PLAYER)
        .into_iter()
        .filter(|&i| game.db.subkind(i) == Subkind::PL_REGULAR)
        .count();
    let clock = game.system.sysclock;
    let turn = i32::from(clock.turn);
    let turn_s = format!(
        "turn {turn}, {nplayers} player{}",
        if nplayers == 1 { "" } else { "s" }
    );
    // oly_month() is -1 at turn 0; C clamps it (playbymail/olympia-g3#3).
    let month = ((turn - 1) % NUM_MONTHS).max(0);
    let year = (turn - 1) / NUM_MONTHS;
    let issue_s = format!("{} year {}", MONTH_NAMES[month as usize], year + 1);

    let null = b"(null)".as_slice();
    let from_host = game.system.from_host.as_deref().unwrap_or(null);
    let game_title = game.system.game_title.as_deref().unwrap_or(null);
    let game_url = game.system.game_url.as_deref().unwrap_or(null);

    let mut out = Vec::new();
    out.extend_from_slice(b"From: ");
    out.extend_from_slice(from_host);
    out.extend_from_slice(b"\nSubject: ");
    out.extend_from_slice(game_title);
    out.extend_from_slice(format!(" - The Olympia Times - Issue {turn}\n\n").as_bytes());
    out.extend_from_slice(
        b"   +----------------------------------------------------------------------+\n",
    );
    out.extend_from_slice(format!("   | The Olympia Times {issue_s:>50} |\n").as_bytes());
    out.extend_from_slice(format!("   | {date:<68} |\n").as_bytes());
    out.extend_from_slice(
        b"   |                                                                      |\n",
    );
    out.extend_from_slice(format!("   | {turn_s} ").as_bytes());
    out.extend_from_slice(&pad(from_host, 67 - turn_s.len() as i32));
    out.extend_from_slice(b" |\n");
    out.extend_from_slice(
        b"   +----------------------------------------------------------------------+\n\n",
    );
    out.extend_from_slice(b"     Questions, comments, to play: ");
    out.extend_from_slice(game_url);
    out.extend_from_slice(b"\n\n");
    out.extend_from_slice(b"                             Olympia PBEM\n\n");
    out.extend_from_slice(b"                                *  *  *\n\n");
    out.extend_from_slice(b"                                *  *  *\n\n");
    // fprintf() is unchecked in C.
    let _ = file.write_all(&out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_like_printf_star() {
        assert_eq!(pad(b"ab", 4), b"  ab");
        assert_eq!(pad(b"ab", -4), b"ab  ");
        assert_eq!(pad(b"abcdef", 4), b"abcdef");
    }
}
