# Project Workflow

Guidance for agents when working in this repository.

## Priorities

Fix bugs before adding features.
Before offering to implement a change or work on documentation, check for open bugs (`gh issue list --label bug --state open`).
If any are open, do not make the offer; propose working on the bugs instead.

A bug here is the port producing different output from the C engine on a fixture that is meant to pass.
A bug in the C engine is filed and fixed in olympia-g3 first (see [CLAUDE.md](CLAUDE.md)).

## Build and test

Prefer `cargo run` over building and invoking the binary yourself.
Cargo writes build output to `target/`; keep it there and keep `target/` ignored.
Put any other scratch output, such as unpacked fixture saved games and `lib/` trees to diff, in `tmp/`, never in the root of the repository.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
harness/run.sh
```

All four must pass before committing.
`harness/run.sh` passes when every fixture in `harness/passing.txt` still passes; add each new pass it reports to that file in the same commit.
The unit tests do not replace the olympia-g3 fixtures, which remain the spec.

## Versioning

- Semantic versioning.
  The current version lives in `Cargo.toml` as `[package] version`, for example `version = "0.1.0-beta"`.
  It is the single source: code that needs it reads `env!("CARGO_PKG_VERSION")` rather than repeating the number.
- **Every commit that changes code must bump the version in `Cargo.toml`.**
  Documentation-only commits are encouraged to bump it but are not required to.
- Choose the bump by the change: patch for fixes, minor for new features or any breaking change while below v1.0.0.
  Keep the `-beta` pre-release suffix until told otherwise.
- Commit the `Cargo.lock` change with the bump.
  Bumping the version rewrites the crate's own entry in `Cargo.lock`; run a `cargo` command after editing `Cargo.toml` so the two agree.
- The version is rhorri's own, not the C engine's.
  The C engine prints no version, so rhorri's version must not appear in any file or output that fixtures compare.
- Keep `publish = false` in `Cargo.toml`.
  rhorri is not published to crates.io.
- Do not create git tags.
  Versions are tagged on GitHub by the user.

## Git workflow

- Commit directly to `main`.
  No feature branches or pull requests; history is the recovery mechanism.
  This may change after v1.0.0.
- Write commit subjects in Conventional-Commits form, `type(scope): subject` (`feat`, `fix`, `docs`, `refactor`, `build`, `test`).
- Reference the issue in the commit body: `Closes #NN` or `Refs #NN`.
  A bare `#NN` is a rhorri issue.
  Write olympia-g3 issues and PRs as `playbymail/olympia-g3#NN`.
- Agents are authorized to commit and push each task as it is completed, provided all checks pass and code changes include a version bump.
- Agents are authorized to create GitHub labels as needed.
- Assign every issue and pull request to `@me` when creating it (`gh issue create --assignee @me`, `gh pr create --assignee @me`).

## CI

Do not create GitHub Actions workflows or other GitHub CI actions.
Run the checks above locally instead.
