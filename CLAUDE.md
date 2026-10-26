# CLAUDE.md

Guidance for working in **rhorri**, the Rust + Rhai port of the Olympia G3
play-by-mail game engine.

## What this is

rhorri ports the G3 engine (`olympia-g3`, ~54K lines of C) to Rust. Rhai takes
the place of the C engine's embedded Lua for scenario scripting. The port is
driven from this repo. License: AGPL-3.0, the same as G3.

The C engine is the **reference implementation**. It lives in
[playbymail/olympia-g3](https://github.com/playbymail/olympia-g3), cloned
locally at `~/Software/playbymail/olympia-g3`. Its golden trees and regress
fixtures are the spec. rhorri is correct when it produces the same bytes from
the same inputs.

## The spec lives in olympia-g3

- **Don't copy the spec into this repo.** Run the fixtures in place in the
  olympia-g3 checkout, so there is only one copy of the spec.
- **Don't edit olympia-g3 from here.** Spec changes go through that repo's own
  branch + PR workflow and its
  [C engine policy](https://github.com/playbymail/olympia-g3/blob/main/CLAUDE.md#c-engine-policy-reference-implementation).
  In short, C changes only for bug fixes (each with a pinning fixture), new
  fixtures, and harness changes that help the port.
- **The port reproduces C behavior, bugs included.** If you find a bug in the C
  engine, file it in olympia-g3. Fix it there first, with a fixture, then
  follow the fix here. Don't "fix" it only in the port.
- **When the port and C disagree,** first decide which one is wrong. Usually
  it's the port. If it's C, follow the step above.
- **Every behavior change in olympia-g3 files an issue here.** Work those
  issues to keep the port in step.

Useful references in the olympia-g3 checkout:

| Read | When you |
|---|---|
| `CLAUDE.md` | need the build, the two golden gates, or the C engine policy |
| `doc/turn-execution-order.md` | port a turn phase, or need the order phases run in |
| `doc/claude/rng-streams.md` | port any random draw (every stream, its tag and its keying) |
| `doc/claude/testing.md` | need the list of gates and what each one pins |
| `tests/olympia/fixtures/README.md` | need to know how a fixture's saved game is built (`scenario.patch` over `world.tgz`) |
| `tests/ultron/README.md` | need the command, `use` and phase backlogs: which fixture covers which behavior |
| `doc/ultron-harness-architecture.md` | touch the coverage dumps or `--state-export` |

## The contracts

These must stay identical to the C engine. They are not open to redesign here.

1. **The DB format.** rhorri reads the same `lib/` directory and writes every
   file `-S` writes, byte for byte. That includes `loc`, `item`, `master`,
   `fact/*`, `orders/*`, `randseed`, the reports under `save/` and `fact/`, and
   the `-p` "pretty" variants. Fixtures hash these files.
2. **The RNG streams.** Port the MD5 primitive (`lib/rnd.c`) and the
   addressable layer (`lib/rng.{c,h}`: `rng_seed`, `rng_stream_of`,
   `rng_draw`, `rng_keyed`) bit-exactly. Keep every stream's tag, keys, day
   keying and draw order as they are, rooted at the master seed from
   `lib/randseed`. A draw that moves, is added or is dropped changes the
   output, even where the game would play "the same".
3. **The CLI and dump formats.** The regress `check.sh` scripts call the
   engine directly. rhorri accepts the same flags (`-l`, `-r`, `-S`, `-i`,
   `-s`, `-a`, `-p`, the `test-use-const-report-date` test flag, …). It also
   writes the same `--coverage`, `--use-coverage`, `--phase-coverage` and
   `--state-export` output, because fixtures assert those too.

Things the C engine relies on that a port gets wrong easily:

- **Integer semantics.** C `int` is 32-bit here. Use `i32`/`u32` where the C
  code uses `int`/`unsigned`, so overflow, truncation and sign behave the same.
  Rust `/` and `%` truncate like C99. Signed overflow is UB in C, and the
  ASan/UBSan gate keeps the fixtures clear of it. If the port hits a case that
  overflows, report it rather than picking a behavior.
- **Iteration order.** Entity lists (`ilist`), location occupant order and
  `qsort` results all show up in the output. Sort comparators were made total
  (playbymail/olympia-g3#38), so any sort algorithm gives the same order. If
  you find a comparator that isn't total, that's a C bug: file it there.
- **Evaluation order.** C's unspecified argument evaluation order was removed
  where it moved RNG draws (playbymail/olympia-g3#38). Port each call in the
  order the C code makes it now.
- **Text output.** Reports and data files are formatted with `printf` and
  word-wrapped by the engine. Match the formatting exactly, including spacing,
  line breaks and padding.

## Scope

- **The engine first** (`olympia-g3`). Fixtures supply pre-built worlds
  (`lib.tgz`, `world.tgz`, frozen `scenario.patch`/`scenario.tgz`), so the map
  generator (`mapgen-g3`), the island generator (`island-g3`) and `genesis-g3`
  are out of scope for now.
- **Rhai scripting comes later.** The fixtures the port runs are frozen saved
  games and don't need the scripting bindings. When Rhai bindings land, they
  mirror the `oly.*` Lua module in `olympia/lua_bindings.c`. Like the Lua
  bindings, they are for authoring scenarios. Orders under test always go
  through order files and a normal turn, never through immediate mode.
- **New fixtures are built in olympia-g3,** against the C engine.

## The gate

The port is checked by running olympia-g3's golden flow and regress
`check.sh` scripts against the rhorri binary. Those scripts expect the binary
at `build/<preset>/olympia-g3` in the olympia-g3 checkout. Running them
against rhorri needs a binary override in that harness. That change is
allowed under the C engine policy but doesn't exist yet: add it there before
relying on it here. Until then, check against a fixture by hand: unpack its
saved game, run the turn with both engines, and diff the `lib/` trees.

Fixtures for the parts not yet ported will fail. Track progress by which
fixtures pass, not by lines ported.

## Build

There is no crate yet. Once there is:

- Stable Rust and Cargo.
- `cargo fmt` (default rustfmt settings) and `cargo clippy` clean.
- `cargo test` for unit tests. The olympia-g3 fixtures remain the spec, and
  unit tests don't replace them.

## Conventions

- Rust that reads like Rust: port the behavior, not the C structure, as long
  as the output stays byte-identical. Keep a comment pointing to the C source
  (`olympia/<file>.c:<function>`) on ported functions where the mapping isn't
  obvious, so a divergence can be traced back.
- No `unsafe` unless there is no reasonable alternative, with a comment saying
  why.

### Git workflow

Changes land via **feature branch + pull request**:

- **Branch** from `main` named `type/slug` (`feat/…`, `fix/…`, `docs/…`,
  `refactor/…`, `build/…`), matching the Conventional-Commits type.
- **Reference the issue** in the PR body: `Closes #NN` or `Refs #NN`. A bare
  `#NN` is a rhorri issue. Write olympia-g3 issues and PRs as
  `playbymail/olympia-g3#NN`.
- **Squash-merge** to keep `main` linear. The squashed commit keeps the
  `type(scope): subject (#NN)` subject.
