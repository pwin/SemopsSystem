# 0008. One binary: `semops`

Status: accepted and implemented, 22 November 2026, in `crates/semops` and the four engine
command crates.

## Context

Phase 3 of the proposal ships "one binary, `semops`, with subcommands: `serve`, `load`,
`query`, `update`, `validate`, `map`, `check`, `diff`, `repair`", with "the old `holos`,
`shacl` and `tarka` names" staying "as aliases for a deprecation period". Until now `semops`
had one subcommand, `check`, and the rest were four separate binaries, `holos-server`,
`holos`, `shacl` and `tarka`, each a `main.rs` of 700 to 2,000 lines.

## Decision

**Each engine command became a library with a binary in front of it.** In `holos-cli`,
`holos-server`, `shacl-cli` and `tarka-cli`, `src/main.rs` moved to `src/lib.rs` unchanged
but for its entry point, now `pub fn main_with(args)`, which takes the command line as the
operating system gives it, program name first. The new `src/main.rs` of each holds what
only a binary may hold, its global allocator, and calls `main_with(std::env::args_os())`.
Because a file named `main.rs` still exists, git records `lib.rs` as new and `main.rs` as
rewritten, not as a rename. A `git subtree pull` that brings an upstream change to an
engine's `main.rs` will therefore conflict in that file, and the change has to be carried
into `lib.rs` by hand. That cost falls only on the engines' remaining upstream commits, as
the work moves into this repository. (There are none now: the engines' repositories were
archived on 25 November 2026.)

**`semops` links all four and forwards to them**, argument for argument:

| `semops` | Runs |
| --- | --- |
| `serve` | `holos-server` |
| `load`, `query`, `update`, `stats`, `dump`, `backup`, `compact`, `entail`, `rules` | `holos` with that command |
| `validate` | `shacl` |
| `map` | `tarka` |
| `check` | The quality checks, as before |

`--help` and `--version` after a subcommand go to the engine, so `semops query --help` is
`holos`'s help. `semops validate` is the SHACL engine, not `holos validate`: SHACL_Engine
covers every feature, and it is the validator the proposal's "one validator" builds on.

`holos load` is new: `--data` into a persistent `--store`, and nothing after. Every `holos`
command already loaded `--data` before it ran, so `load` is that step alone. It refuses to
run without `--store`, where it would load into memory and throw the result away.

**Aliases from one file.** Run as a copy or link named `holos-server`, `holos`, `shacl` or
`tarka`, `semops` is that command, chosen from the program name. The container image now
builds `semops` alone and links the four names to it, so the image's entry point,
`holos-server`, and every script that calls the old names keep working. The four separate
binaries are still built, for anyone who wants one engine's command and not the rest.

**One allocator.** A binary has one global allocator, and two of the four commands chose
one: `holos-server` a counting allocator, which `--max-query-memory` needs to see an
allocation before it happens; `shacl` mimalloc, worth about a quarter of the time a large
parse takes. `semops` uses the counting allocator around mimalloc. The counting allocator
is the workspace's one file of `unsafe` (`holos-server/src/alloc.rs`). `semops` compiles
that same file in by path rather than copying it or moving it into a library, which would
mean lifting `forbid(unsafe_code)` from one.

## Consequences

- `semops` needs Rust 1.95, because `tarka-cli` does (Polars). The MSRV job's 1.90 floor
  leaves `semops` out, as it leaves out `tarka-cli`, and the image builds on `rust:1.95`.
- `semops` is larger, about 115 MB as an unoptimised debug build on Windows. It holds every
  engine, the Polars reader included.
- The engines' standalone test suites still run their own binaries, and
  `crates/semops/tests/commands.rs` runs each subcommand and each alias name.

## Not done

- `semops diff` and `semops repair`, which need the Ontology Quality Suite's version
  comparison and repair templates ported.
- Retiring the four separate binaries. The proposal keeps the old names for a deprecation
  period, and the links serve that.
- Subcommand names for `holos`'s `validate`, which is reachable as `holos validate` and as
  `semops` run under the name `holos`.
