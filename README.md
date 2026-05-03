# rustune

`rustune` is a Rust port of `fortune-mod` focused on behavior parity while modernizing the internals.

## Intent

Recreate classic fortune-file workflows with safer parsing, deterministic hooks, and compatibility-oriented tests.

## Ambition

The multiple binaries and parity harness point toward a faithful but maintainable Rust replacement for the traditional toolchain around `fortune` and `strfile`.

## Current Status

The repo already includes core library modules, companion binaries, tests, and parity-oriented infrastructure. It looks well past the prototype stage.

## Core Capabilities Or Focus Areas

- Read and write STRFILE-related data structures.
- Provide a `fortune`-style runtime.
- Support deterministic RNG hooks for parity work.
- Ship auxiliary binaries for format/index workflows.
- Use tests and parity tooling to validate behavior.

## Project Layout

- `src/`: Rust source for the main crate or application entrypoint.
- `tests/`: automated tests, fixtures, or parity scenarios.
- `Cargo.toml`: crate or workspace manifest and the first place to check for package structure.

## Setup And Requirements

- Rust toolchain.
- Fortune files or corpora to index and serve.
- Terminal environment for the CLI binaries.

## Build / Run / Test Commands

```bash
cargo build
cargo test
cargo run -- --help
cargo run --bin strfile -- --help
```

## Notes, Limitations, Or Known Gaps

- Behavior parity is important here, especially around file formats and selection semantics.
- The multiple binaries are part of the product surface, not just developer utilities.

## Next Steps Or Roadmap Hints

- Keep compatibility fixtures broad as edge cases in fortune files are discovered.
- Document intentional differences from upstream only after they are stable and tested.
