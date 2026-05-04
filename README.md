# rustune

`rustune` is a Rust implementation of the classic `fortune`/`strfile` toolchain with a strong bias toward `fortune-mod` behavior parity. The repository contains:

- A `fortune`-style CLI for selecting and printing fortunes.
- A `strfile`-compatible index builder for generating `.dat` files from text corpora.
- A parity harness that compares `rustune` against a system `fortune` binary.
- A reusable library crate that separates parsing, discovery, selection, and RNG concerns.

The project is aimed at preserving the traditional fortune-file workflow while replacing the C-era implementation details with a safer and easier-to-test Rust codebase.

## Goals

- Preserve the everyday `fortune`/`strfile` workflow.
- Model `fortune-mod` selection behavior closely enough to support parity testing.
- Keep the implementation modular so file format handling, source discovery, and selection logic can evolve independently.
- Support deterministic execution paths for tests and oracle-based comparisons.

## What The Repository Implements

At a high level, the main `rustune` binary does four things:

1. Parses CLI flags and source specifications.
2. Discovers fortune text files and their `.dat` index files.
3. Loads indexed corpora, filters candidate records, and computes source probabilities.
4. Selects or searches fortunes and prints results in a `fortune`-style format.

The companion `strfile` binary builds the `.dat` sidecar file required to read a fortune corpus efficiently. The `fortune-parity` binary exists for development and verification work: it runs the system oracle and the Rust implementation against the same inputs and reports whether they match.

## Repository Structure

### Top Level

- `Cargo.toml`: package manifest, binary targets, and dependency list.
- `Cargo.lock`: locked dependency graph.
- `README.md`: project documentation.
- `LICENSE`: repository license.
- `src/`: library modules and binary entrypoints.
- `tests/`: integration tests and sample corpora.

### Source Tree

- `src/lib.rs`: library crate root that re-exports the internal modules used by the binaries.
- `src/main.rs`: entrypoint for the `rustune` binary.
- `src/bin/strfile.rs`: entrypoint for the `strfile` binary.
- `src/bin/fortune-parity.rs`: entrypoint for the parity harness.

### Core Library Modules

- `src/datfile.rs`: STRFILE parsing and encoding.
  This module defines the `DatHeader`, `DatFile`, and `FortuneFile` types. It is responsible for reading and writing `.dat` files, validating offset tables, deriving `.dat` paths from text file paths, and slicing individual fortune records from the raw corpus bytes.

- `src/strfile_builder.rs`: `.dat` generation.
  This module parses a text corpus into records using a delimiter line, computes record statistics, optionally randomizes or lexicographically orders offsets, and produces a serialized `DatFile`.

- `src/discovery.rs`: source discovery.
  This module finds fortune corpora from explicit CLI inputs or default search paths. It understands directories, single files, `FORTUNE_PATH`, locale-based directory lookup via `LANG`, and offensive corpus naming conventions such as `*-o`.

- `src/sources.rs`: source specification parsing.
  This module parses CLI source arguments, including percentage-prefixed inputs like `25%file` or `25% file`, and converts them into typed source specifications for later discovery and weighting.

- `src/fortune_engine.rs`: loading, weighting, searching, and selection.
  This module opens discovered corpora, applies short/long filters, computes effective source probabilities, performs regex-based record searches, and selects a random fortune using behavior intended to mirror upstream semantics.

- `src/rng.rs`: RNG abstraction and determinism hooks.
  This module centralizes random-number behavior. It supports thread RNG, a `srand`-style seeded mode, and a deterministic hard-coded mode driven by environment variables for parity testing.

- `src/logging.rs`: tracing initialization.
  This module provides opt-in tracing subscriber setup so binaries can expose debug information when `--verbose` is passed.

## Binaries

### `rustune`

The main CLI behaves like a `fortune` command. Important options currently implemented include:

- `-a`, `--all`: allow any corpus, including offensive files.
- `-o`, `--offensive`: select only offensive corpora.
- `-e`, `--equal`: weight files equally instead of by candidate count.
- `-f`, `--files`: print file probabilities instead of a fortune.
- `-l`, `--long`: restrict selection to records longer than the threshold.
- `-s`, `--short`: restrict selection to records at or below the threshold.
- `-n`, `--length <N>`: threshold used by `--short` and `--long`.
- `-m`, `--match <REGEX>`: print all matching fortunes.
- `-i`, `--ignore-case`: case-insensitive regex matching; requires `-m`.
- `-w`, `--wait`: sleep after printing using a simple output-length heuristic.
- `-c`, `--show-source`: print the selected source path before the fortune.
- `-u`, `--no-recode`: accepted for compatibility; locale recoding is not yet implemented.
- `-v`, `--version`: print the package version.
- `--verbose`: enable tracing output.

Source arguments may be:

- One or more indexed fortune text files.
- A directory containing indexed fortune files.
- A percentage-qualified source such as `70%/path/to/file`.
- The special token `all`, which expands across default fortune directories.

When no source arguments are provided, `rustune` falls back to the default fortune search path and locale-aware subdirectories.

### `strfile`

`strfile` builds a `.dat` index file from a plain-text fortune corpus. Supported behaviors include:

- Custom delimiter selection via `-c`, `--delimiter`.
- Randomized offset order via `-r`, `--random`.
- Lexicographically ordered offsets via `-o`, `--order`.
- Silent mode via `-s`, `--silent`.
- Optional preservation of empty records via `--allow-empty`.
- Verbose tracing via `--verbose`.

By default, the output file is written next to the input file as `<input>.dat`.

### `fortune-parity`

`fortune-parity` is a development utility that compares this implementation against an oracle `fortune` binary, defaulting to `/usr/bin/fortune`. It:

- Ensures a test corpus exists.
- Generates missing `.dat` files using the local `strfile` binary.
- Runs predefined parity cases against both binaries.
- Emits a markdown report with weighted category scores.
- Can optionally write a JSON report.

This tool is useful when changing selection logic, discovery rules, or CLI behavior and wanting a quick signal on compatibility regressions.

## Fortune File Model

The repository uses the traditional split representation:

- A text corpus file containing records separated by delimiter lines.
- A sibling `.dat` file containing record metadata and offsets.

In this implementation:

- Record separators are detected as lines containing exactly the delimiter byte.
- Offsets are stored as big-endian `u32` values.
- The `.dat` header stores version, record count, longest record length, shortest record length, flags, and delimiter.
- Record bodies are read directly from the text file using the indexed offsets.

The builder and reader are intentionally kept close to one another so round-trip tests can verify that a generated `.dat` file is accepted by the runtime.

## Selection And Weighting Semantics

Selection happens in two stages:

1. A source file is chosen.
2. A record within that source is chosen.

The repository supports two source-selection modes:

- Probability-percent mode: used when explicit percentages are provided or when `--equal` is active.
- Candidate-count mode: used when source weight should reflect the number of records surviving the active length filter.

After a source is selected, record selection uses the internal RNG and then walks forward if necessary until it lands on a record that satisfies the active length filter. This is one of the parity-oriented details implemented in `src/fortune_engine.rs`.

## Deterministic And Compatibility Hooks

For testing and parity work, random behavior can be made reproducible:

- `FORTUNE_MOD_RAND_HARD_CODED_VALS=<number>` forces a deterministic value source.
- `FORTUNE_MOD_USE_SRAND=1` switches to a seeded RNG mode intended to approximate `srand`-style behavior.
- `FORTUNE_PATH` overrides the default search directories used during source discovery.
- `LANG` influences locale directory discovery inside the default fortune paths.

These hooks are especially important for tests that assert exact file weighting or exact record selection.

## Building

```bash
cargo build
```

Build a specific binary:

```bash
cargo build --bin rustune
cargo build --bin strfile
cargo build --bin fortune-parity
```

## Running

Show the `rustune` help:

```bash
cargo run --bin rustune -- --help
```

Build an index:

```bash
cargo run --bin strfile -- fortunes/my-corpus
```

Print a fortune from a specific corpus:

```bash
cargo run --bin rustune -- fortunes/my-corpus
```

List source probabilities instead of printing a fortune:

```bash
cargo run --bin rustune -- -f fortunes/
```

Search for matching fortunes:

```bash
cargo run --bin rustune -- -m Rust -i fortunes/
```

Run the parity harness:

```bash
cargo run --bin fortune-parity --
```

## Testing

Run the full test suite:

```bash
cargo test
```

The test suite includes:

- Unit tests inside core modules such as `datfile`, `sources`, and `strfile_builder`.
- Integration tests for CLI behavior in `tests/fortune_cli.rs`.
- Round-trip tests covering build/read compatibility in `tests/strfile_roundtrip.rs`.
- Property-based tests in `tests/strfile_proptest.rs` to exercise `.dat` generation across generated corpora.

## Test Fixtures And Corpus Layout

The `tests/` directory contains both executable tests and sample fortune files:

- `tests/fortune_cli.rs`: integration coverage for probability listing, deterministic selection, and source-banner output.
- `tests/strfile_roundtrip.rs`: verifies that a built `.dat` file can be reopened and read correctly.
- `tests/strfile_proptest.rs`: property-based validation of offset ordering and header correctness.
- `tests/corpus/alpha`
- `tests/corpus/alpha.dat`
- `tests/corpus/beta`
- `tests/corpus/beta.dat`

The `tests/corpus/` files serve as small indexed corpora for parity and behavioral checks.

## Design Notes

- The crate is intentionally split so binaries stay thin and orchestration-heavy logic remains testable in library modules.
- `anyhow` is used for ergonomic error propagation in binaries and library entrypoints.
- `clap` drives the CLI definitions.
- `tracing` is used for optional debug instrumentation.
- `serde` and `serde_json` are used by the parity harness report output.
- `proptest` is used to cover record-layout edge cases beyond fixed fixtures.

## Current Gaps And Non-Goals

The implementation already covers a substantial portion of the classic workflow, but some behavior is explicitly incomplete or compatibility-oriented:

- `--no-recode` is recognized, but locale-based recoding is not implemented yet.
- Parity is measured against selected cases, not a claim of complete `fortune-mod` equivalence.
- The repository currently focuses on the classic text-plus-`.dat` workflow rather than broader ecosystem tooling.

## Summary

If you are working in this repository, the normal flow is:

1. Create or edit a text corpus.
2. Generate its `.dat` index with `strfile`.
3. Read from it with `rustune`.
4. Use `cargo test` and `fortune-parity` to validate behavior when changing the implementation.

That workflow is the center of the project, and the source layout mirrors it directly.
