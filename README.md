# rustune

`rustune` is a Rust port of `fortune-mod` focused on behavior parity while modernizing the internals:

- memory-safe STRFILE parsing/writing
- deterministic RNG hooks for parity testing
- structured diagnostics with `tracing`
- an oracle-driven parity harness

## Binaries

- `fortune`: selects and prints fortunes
- `strfile`: generates `*.dat` indexes
- `fortune-parity`: compares this port against a system `fortune` oracle

## Build

```bash
cargo build
```

## Logging

All binaries use `tracing` and honor `RUST_LOG`.

```bash
RUST_LOG=rustune=trace,fortune=debug cargo run --bin fortune -- tests/corpus/alpha
```

## Usage

### Generate indexes

```bash
cargo run --bin strfile -- tests/corpus/alpha
cargo run --bin strfile -- tests/corpus/beta
```

### Print a fortune

```bash
cargo run --bin fortune -- tests/corpus/alpha tests/corpus/beta
```

### List files and computed probabilities

```bash
cargo run --bin fortune -- -f tests/corpus
```

### Regex mode

```bash
cargo run --bin fortune -- -m Rust tests/corpus/alpha
```

## Deterministic RNG (parity/test)

The port supports the upstream-style deterministic hook:

```bash
FORTUNE_MOD_RAND_HARD_CODED_VALS=0 cargo run --bin fortune -- tests/corpus/alpha
```

`FORTUNE_MOD_USE_SRAND=1` is also supported to force seeded RNG behavior.

## Parity Harness

Run oracle comparisons (requires a system `fortune`, default `/usr/bin/fortune`):

```bash
cargo run --bin fortune-parity -- \
  --subject target/debug/fortune \
  --strfile target/debug/strfile \
  --json-out tmp/parity-report.json
```

The harness reports:

- pass/fail counts
- weighted parity percentage
- per-category scoring
- top regression diffs

## Test Suite

```bash
cargo test
```

Includes:

- unit tests for `.dat` parsing/encoding and source parsing
- integration tests for `strfile` round-trip behavior
- property tests (`proptest`) for random corpus generation and `.dat` stability

## Current Scope

Implemented:

- core `fortune` flags: `-a -o -e -f -l -s -n -m -i -w -c -u -v`
- percent-prefixed sources (`10%foo`, `10% foo`)
- `.dat` parser/writer in network byte order
- directory/file source discovery with default path + LANG-aware candidate probing

Still evolving toward full upstream parity:

- exact output formatting parity for every mode
- full locale recoding parity details
- deeper compatibility for all upstream corner cases
