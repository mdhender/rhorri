// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `olympia-g3` command, a drop-in replacement for the C engine's binary
//! (`olympia/main.c`).

use std::ffi::OsString;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use rhorri::cstr::strtoull;
use rhorri::game::{Game, NotYetImplemented};
use rhorri::rnd::LegacyRng;
use rhorri::selftest::{self, Layout};
use rhorri::times::times_masthead;

/// `getopt()`'s option string: a letter followed by `:` takes a value.
const OPTSTRING: &[u8] = b"waeEfirl:pR?sStMTAhG:";

/// The usage message for a bad option.
const USAGE: &str = "usage: oly [options]
	-w		Windows mode
	-a		Add new players mode
	-e		Eat orders from libdir/spool
	-E		Eat orders from libdir/spool and terminate
	-f		Don't buffer files for debugging
	-i		Immediate mode
	-l dir		Specify libdir, default ./lib
	-p		Don't make data files pretty
	-r		Run a turn
	-R		Test the random number generator
	-s		Extract safe havens as start city list
	-S		Save the database at completion
	-t		Test ilist code
	-T		Print timing info
	-M		Mail reports and order acks
	-A		Charge player accounts
	-G seed		Set per-game trade-route seed (game creation only)
	--coverage path	Write the command coverage dump to path
	--use-coverage path	Write the use coverage dump to path
	--state-export path	Write the faction state export to path";

/// What the command line asks for.
#[derive(Debug, Default)]
struct Options {
    libdir: Option<OsString>,
    pretty: bool,
    save: bool,
    /// `-r`: run a turn.
    run: bool,
    test_use_const_report_date: bool,
    trade_route_seed: Option<u64>,
    /// Set options rhorri can't act on yet.
    unported: Vec<String>,
    errflag: bool,
}

/// What `main()` does once the options are read.
enum Action {
    /// `-R` was reached: run the self-test and stop.
    TestRandom,
    /// `-t` was reached: run the ilist test and stop.
    TestIlist,
    /// Load the database and do what the options say.
    Run(Options),
}

fn main() -> ExitCode {
    let stdout = io::stdout();
    let is_tty = stdout.is_terminal();
    let mut out = BufWriter::new(stdout.lock());
    let code = run(&mut out, is_tty);
    if let Err(err) = out.flush() {
        eprintln!("olympia-g3: {err}");
        return ExitCode::FAILURE;
    }
    code
}

fn run(out: &mut impl Write, is_tty: bool) -> ExitCode {
    // Every C run prints these first. 144 is the size of the C engine's
    // struct box on the 64-bit hosts the fixtures run on.
    let preamble = writeln!(out, "The Olympia C code is not 64-bit clean.")
        .and_then(|()| writeln!(out, "at the least: it puts pointers into ilists"))
        .and_then(|()| writeln!(out, "\tsizeof(struct box) = 144"));
    if let Err(err) = preamble {
        eprintln!("olympia-g3: {err}");
        return ExitCode::FAILURE;
    }

    match parse_args(std::env::args_os().skip(1).collect()) {
        Action::TestRandom => {
            // -R runs before any DB is loaded, so no seed has been read.
            let master = LegacyRng::new([0; 4]).master_seed();
            let layout = if is_tty { Layout::Grid } else { Layout::Lines };
            match selftest::test_random(out, master, layout) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("olympia-g3: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Action::TestIlist => not_yet_implemented(out, "-t (ilist_test)"),
        Action::Run(opts) => run_engine(out, opts),
    }
}

/// Flushes standard output, reports `what`, and fails.
fn not_yet_implemented(out: &mut impl Write, what: &str) -> ExitCode {
    let _ = out.flush();
    eprintln!("olympia-g3: {}", NotYetImplemented(what.to_owned()));
    ExitCode::FAILURE
}

/// Loads the database and does what the options say, in `main()`'s order.
fn run_engine(out: &mut impl Write, opts: Options) -> ExitCode {
    if opts.errflag {
        let _ = out.flush();
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    if let Some(flag) = opts.unported.first() {
        return not_yet_implemented(out, flag);
    }

    let libdir = opts.libdir.unwrap_or_else(|| OsString::from("lib"));
    let mut game = Game::new(PathBuf::from(libdir));
    game.pretty_data_files = opts.pretty;
    game.test_use_const_report_date = opts.test_use_const_report_date;
    game.trade_route_seed = opts.trade_route_seed;

    if let Err(err) = game.load_db() {
        let _ = out.flush();
        eprintln!("olympia-g3: {err}");
        return ExitCode::FAILURE;
    }

    if opts.run
        && let Err(err) = game.run_turn(false)
    {
        let _ = out.flush();
        eprintln!("olympia-g3: {err}");
        return ExitCode::FAILURE;
    }

    game.check_db();
    if opts.save
        && let Err(err) = game.save_db()
    {
        let _ = out.flush();
        eprintln!("{err}");
        return ExitCode::FAILURE;
    }
    // TODO(stage 2): with -r and -S, save_logdir() moves lib/log and
    // lib/players into lib/save/<turn>. It prints nothing when both exist.

    times_masthead(&game);
    if writeln!(out, "\tmalloc, realloc = 0, 0").is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Long-form arguments `main()` removes before `getopt()`.
enum LongArg {
    ConstReportDate,
    /// Takes the next argument as a path.
    Path(&'static str),
}

fn long_arg(arg: &[u8]) -> Option<LongArg> {
    match arg {
        b"test-use-const-report-date" => Some(LongArg::ConstReportDate),
        b"--coverage" => Some(LongArg::Path("--coverage")),
        b"--use-coverage" => Some(LongArg::Path("--use-coverage")),
        b"--phase-coverage" => Some(LongArg::Path("--phase-coverage")),
        b"--state-export" => Some(LongArg::Path("--state-export")),
        _ => None,
    }
}

/// Reads the arguments as `main()` does: long-form ones first, then the
/// rest with `getopt()`.
fn parse_args(args: Vec<OsString>) -> Action {
    let mut opts = Options::default();

    let mut rest = Vec::new();
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        match long_arg(arg.as_encoded_bytes()) {
            Some(LongArg::ConstReportDate) => opts.test_use_const_report_date = true,
            Some(LongArg::Path(name)) => {
                // C takes the next argument only if it is non-empty.
                if args.next_if(|v| !v.is_empty()).is_some() {
                    opts.unported.push(name.to_owned());
                } else {
                    opts.errflag = true;
                }
            }
            None => rest.push(arg),
        }
    }

    for (c, value) in getopt(&rest) {
        match c {
            b'l' => opts.libdir = value,
            b'p' => opts.pretty = !opts.pretty,
            b'S' => opts.save = true,
            b'r' => opts.run = true,
            b'R' => return Action::TestRandom,
            b't' => return Action::TestIlist,
            b'G' => {
                let v = value.unwrap_or_default();
                opts.trade_route_seed = Some(strtoull(v.as_encoded_bytes()));
            }
            b'w' | b'a' | b'A' | b'f' | b'e' | b'E' | b'i' | b's' | b'T' | b'M' | b'h' => {
                opts.unported.push(format!("-{}", char::from(c)))
            }
            _ => opts.errflag = true,
        }
    }
    Action::Run(opts)
}

/// POSIX `getopt()` over `args`: yields each option letter, with its value
/// if it takes one. An unknown letter, or a missing value, comes back as
/// `?`. Parsing stops at `--` or the first argument that isn't an option.
struct GetOpt<'a> {
    args: &'a [OsString],
    /// The next argument to read.
    i: usize,
    /// The next letter in the current argument, or 0 between arguments.
    j: usize,
}

impl Iterator for GetOpt<'_> {
    type Item = (u8, Option<OsString>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.j == 0 {
            let arg = self.args.get(self.i)?.as_encoded_bytes();
            if arg == b"--" || arg.len() < 2 || arg[0] != b'-' {
                return None;
            }
            self.j = 1;
        }
        let arg = self.args[self.i].as_encoded_bytes();
        let c = arg[self.j];
        self.j += 1;
        let at_end = self.j >= arg.len();
        if at_end {
            self.i += 1;
            self.j = 0;
        }

        let spec = OPTSTRING.iter().position(|&o| o == c && c != b':');
        let Some(p) = spec else {
            eprintln!("olympia-g3: illegal option -- {}", char::from(c));
            return Some((b'?', None));
        };
        if OPTSTRING.get(p + 1) != Some(&b':') {
            return Some((c, None));
        }
        // The value is the rest of this argument, or the next argument.
        let value = if at_end {
            let v = self.args.get(self.i).cloned();
            if v.is_some() {
                self.i += 1;
            }
            v
        } else {
            let v = bytes_to_os(arg[self.j..].to_vec());
            self.i += 1;
            self.j = 0;
            Some(v)
        };
        match value {
            Some(v) => Some((c, Some(v))),
            None => {
                eprintln!(
                    "olympia-g3: option requires an argument -- {}",
                    char::from(c)
                );
                Some((b'?', None))
            }
        }
    }
}

fn getopt(args: &[OsString]) -> GetOpt<'_> {
    GetOpt { args, i: 0, j: 0 }
}

#[cfg(unix)]
fn bytes_to_os(v: Vec<u8>) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(v)
}
