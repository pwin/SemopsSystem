# 0009. One WebAssembly package: `semops-wasm`

Status: accepted and implemented, 22 November 2026, in `crates/semops-wasm`, with a
`start` feature added to `holos-wasm` and `shacl-wasm`.

## Context

Phase 3 of the proposal ships "one WASM package, used by both the browser editor and the
VS Code extension, so they can never again be a release behind". Until now there were two:
`holos-wasm` for the store and `shacl-wasm` for the validator, each published to npm twice
(`-node` for Node). The quality checks were in neither, so the VS Code extension carried
its own copy of the registry, which the proposal says should go away.
[0002](0002-names-and-registries.md) fixed the names: `semops-wasm` for browsers and
bundlers, `semops-wasm-node` for Node.

## Decision

**A new crate links the two engines' bindings and adds the checks.**
`crates/semops-wasm` depends on `holos-wasm` and `shacl-wasm` and re-exports their classes
and functions under the names they already have:

- `Store`, from holos-wasm;
- `Validator`, `Report`, `validateTurtle` and `rdfsClosure`, from shacl-wasm.

Code written against either package therefore moves by changing its import. It adds three
functions:

- `check(text, format, engine, base)`: the ontology quality checks, as `semops check`
  runs them. It returns `{ findings, failed, complete }`, with each finding's fields in
  camelCase. `complete` is false when a check's query did not run, since a check that did
  not run finds nothing and a caller gating on findings must be able to tell.
- `listChecks()`: the registry.
- `version()`.

The checks run on the same two engines the module already holds, so they add the
registry's embedded data and the runner, not a third engine.

**One start function.** A WebAssembly module has one, and both engine crates declared one
to install the panic hook. Each now does so only with its new `start` feature, which is on
by default, so `holos-wasm` and `shacl-wasm` build as before. `semops-wasm` turns both off
and installs the hook itself.

**Two builds, as before.** `node crates/semops-wasm/build.mjs` runs wasm-pack for the
`bundler` target (`semops-wasm`) and the `nodejs` target (`semops-wasm-node`), and sets
the package name, the licence files and the repository, as holos-wasm's script does. The
CI `wasm` job builds both, runs `tests/smoke.cjs` against the Node build, and keeps the
two packages as a workflow artifact.

## Not done

- **Publishing to npm.** `.github/workflows/npm.yml` publishes both packages, with
  provenance, when run by hand with the repository secret `NPM_TOKEN` set. It has not been
  run for real yet.
- **Old names as shims.** 0002 keeps `holos-wasm`, `shacl-wasm` and their `-node` builds as
  shims over `semops-wasm` for two minor releases. That follows the first npm publish.
- **Mapping in the browser.** tarka's TARQL and OTTR mapping is not in the module yet.
  Polars cannot build for wasm32, and the CSV path has not been separated from it.
- **A `web` target**, loadable by a `<script type="module">` with no bundler. The
  engines' packages never had one, and the editor uses a bundler.
