// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `olympia-g3` command, a drop-in replacement for the C engine's binary.

use std::io::{self, BufWriter, IsTerminal, Write};
use std::process::ExitCode;

use rhorri::rnd::LegacyRng;
use rhorri::selftest::{self, Layout};

/// Long-form arguments `main()` removes before `getopt()`, with whether each
/// takes a value.
const LONG_ARGS: [(&str, bool); 5] = [
    ("test-use-const-report-date", false),
    ("--coverage", true),
    ("--use-coverage", true),
    ("--phase-coverage", true),
    ("--state-export", true),
];

fn main() -> ExitCode {
    let stdout = io::stdout();
    let is_tty = stdout.is_terminal();
    let mut out = BufWriter::new(stdout.lock());
    if let Err(err) = run(&mut out, is_tty).and_then(|()| out.flush()) {
        eprintln!("olympia-g3: {err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run(out: &mut impl Write, is_tty: bool) -> io::Result<()> {
    // Every C run prints these first (olympia/main.c:main). 144 is the size
    // of the C engine's struct box on the 64-bit hosts the fixtures run on.
    writeln!(out, "The Olympia C code is not 64-bit clean.")?;
    writeln!(out, "at the least: it puts pointers into ilists")?;
    writeln!(out, "\tsizeof(struct box) = 144")?;

    let args = getopt_args(std::env::args().skip(1));

    // TODO: port the rest of the command line. Only a leading -R is handled:
    // getopt() returns it first, and test_random() exits before any later
    // option is read.
    if args.first().is_some_and(|a| a.starts_with("-R")) {
        // -R runs before any DB is loaded, so no seed has been read.
        let master = LegacyRng::new([0; 4]).master_seed();
        let layout = if is_tty { Layout::Grid } else { Layout::Lines };
        return selftest::test_random(out, master, layout);
    }

    out.flush()?;
    Err(io::Error::other("not yet implemented"))
}

/// The arguments left for `getopt()` once the long-form ones are removed.
fn getopt_args(args: impl Iterator<Item = String>) -> Vec<String> {
    let mut rest = Vec::new();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match LONG_ARGS.iter().find(|(name, _)| *name == arg) {
            Some((_, true)) => {
                // C takes the next argument only if it is non-empty.
                args.next_if(|v| !v.is_empty());
            }
            Some((_, false)) => {}
            None => rest.push(arg),
        }
    }
    rest
}
