# Contributing to leafread

Thanks for your interest in improving leafread. This document explains how to
set up the project, what we expect from changes, and how to get them merged.

## Code of conduct

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to contribute

- Report bugs with a minimal reproduction (a small Markdown file plus the
  command or key sequence that misbehaves).
- Suggest features with the problem they solve, not only the solution.
- Improve documentation — README, `--help` text, `docs/`.
- Send pull requests for issues labelled
  [`good first issue`](https://github.com/cigan1/leafread/labels/good%20first%20issue).
- Add test fixtures for Markdown constructs that are not covered yet.

## Development setup

```sh
git clone https://github.com/cigan1/leafread
cd leafread
cargo build            # debug build
cargo test             # unit + integration tests
cargo run -- tests/fixtures/kitchen-sink.md
```

Requirements:

- Rust 1.86 or newer (stable)
- No system libraries. Image support uses pure-Rust encoders by default; the
  `libchafa` half-block renderer is intentionally **not** linked so binaries
  stay portable.

## Before you open a pull request

Run the same checks CI runs:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```

All four must pass. CI also runs an open-source compliance job (required files,
license metadata, workflow safety) and `cargo-deny` (licenses, advisories).

## Pull request expectations

- Keep changes focused; one concern per PR.
- Add or update tests for behavior changes. New Markdown support should come
  with fixtures and assertions.
- Update the README or `--help` text when user-visible behavior changes.
- Add a `CHANGELOG.md` entry under `## [Unreleased]` for user-visible changes.
- Do not commit generated files, editor backups, or secrets.
- Commit messages: short imperative subject; add context in the body when it
  helps. There is no strict format.

## Architecture in one paragraph

`src/markdown/parser.rs` converts the comrak AST into a small document model
(`src/markdown/model.rs`). `src/markdown/layout.rs` turns that model into
styled, wrapped terminal lines while recording metadata (TOC entries, link
spans, image placements). Consumers are `src/ansi.rs` (piped output) and
`src/tui/` (interactive viewer). Mathematics and Mermaid have their own modules
(`src/math.rs`, `src/mermaid.rs`) and are called by the layout engine. See
[docs/architecture.md](docs/architecture.md) for details.

## Testing guidelines

- Unit tests live next to the code (`#[cfg(test)] mod tests`).
- End-to-end CLI tests live in `tests/cli.rs` and run the real binary.
- Prefer assertions on the rendered text (`Rendered::lines`) over pixel/screen
  snapshots; it keeps tests readable and terminal-independent.
- Fixtures live in `tests/fixtures/`.

## Adding a new Markdown feature

1. Extend the parser (`src/markdown/parser.rs`) to map the comrak node.
2. Extend the model (`src/markdown/model.rs`).
3. Implement layout (`src/markdown/layout.rs`) including wrapping behavior.
4. Add fixtures and tests at each level you touched.
5. Note the feature in the README support table.

## License of contributions

leafread is dual-licensed under MIT OR Apache-2.0. Unless you explicitly state
otherwise, any contribution intentionally submitted for inclusion in this
project shall be dual-licensed as above, without any additional terms or
conditions. See the [License section of the README](README.md#license).
