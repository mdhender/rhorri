// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `-R` random-stream self-test (`olympia/z.c:test_random`).

use std::io::{self, Write};

use crate::rng::Stream;

/// How the self-test prints its first 100 draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Ten rows of ten, in draw order: what C prints on a terminal.
    Grid,
    /// One draw per line: what C prints otherwise.
    Lines,
}

/// Prints the self-test's draws from a stream rooted at `master`.
///
/// # Errors
///
/// Returns any error from writing to `out`.
pub fn test_random(out: &mut impl Write, master: [u32; 4], layout: Layout) -> io::Result<()> {
    let mut s = Stream::new(master);

    match layout {
        Layout::Grid => {
            for _ in 0..10 {
                let row: Vec<String> = (0..10).map(|_| format!("{:3}", s.draw(1, 10))).collect();
                writeln!(out, "{}", row.join("  "))?;
            }
        }
        Layout::Lines => {
            for _ in 0..100 {
                writeln!(out, "{}", s.draw(1, 10))?;
            }
        }
    }

    for low in (-16..=-10).rev() {
        writeln!(out, "rng_draw({low}, -3) == {}", s.draw(low, -3))?;
    }

    for _ in 0..100 {
        writeln!(out, "{}", s.draw(1000, 9999))?;
    }

    let mut list: Vec<i32> = (1..=10).collect();
    s.shuffle(&mut list);
    writeln!(out, "Scramble:")?;
    for n in list {
        writeln!(out, "{n}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(layout: Layout) -> String {
        let mut out = Vec::new();
        test_random(&mut out, [0; 4], layout).expect("write to a Vec");
        String::from_utf8(out).expect("ASCII output")
    }

    /// The grid holds the same draws as the one-per-line output, in order.
    #[test]
    fn grid_is_the_lines_in_draw_order() {
        let lines = run(Layout::Lines);
        let grid = run(Layout::Grid);
        let (grid_rows, rest) = grid.split_at(grid.match_indices('\n').nth(9).unwrap().0 + 1);
        let flat: String = grid_rows
            .split_whitespace()
            .map(|n| format!("{n}\n"))
            .collect();
        assert_eq!(format!("{flat}{rest}"), lines);
    }

    /// Spot checks against olympia-g3's `tests/olympia/regress/rng-selftest/EXPECT.txt`.
    #[test]
    fn matches_c_expect() {
        let out = run(Layout::Lines);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 218);
        assert_eq!(lines[..2], ["4", "10"]);
        assert_eq!(lines[95..100], ["7", "7", "1", "4", "4"]);
        assert_eq!(lines[100], "rng_draw(-10, -3) == -7");
        assert_eq!(lines[106], "rng_draw(-16, -3) == -14");
        assert_eq!(lines[107], "1311");
        assert_eq!(
            lines[207..],
            [
                "Scramble:",
                "8",
                "7",
                "4",
                "10",
                "2",
                "3",
                "1",
                "9",
                "6",
                "5"
            ]
        );
    }
}
