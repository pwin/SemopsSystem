# 0001. One Cargo workspace, imported by subtree

Status: accepted, 21 November 2026. This is phase 2 of the proposal's roadmap ("One
workspace").

## Decision

HOLOS, SHACL_Engine and tarka are imported with `git subtree add`, each into a directory of
its own, with full history:

| Directory | Upstream | Imported at |
| --- | --- | --- |
| `holos/` | github.com/pwin/triplestore, `main` | `1034260`, tag v0.27.0 |
| `shacl/` | github.com/pwin/SHACL_Engine, `develop` | `48f8100`, v0.3.9 plus two CI commits |
| `tarka/` | github.com/pwin/tarka, `main` | `4401336`, v0.3.1 plus three manual commits |

One root `Cargo.toml` makes them a single workspace with one version, one lockfile, one
`[patch]` section and one `vendor/` directory.

### Why subtree directories, not one flat `crates/`

- Paths inside each engine keep working unchanged: HOLOS's conformance harness finds
  `holos/testsuites` and `holos/conformance`, tarka's CLI tests find `tarka/tests/fixtures`,
  and the scripts and examples run where they always have.
- While the old repositories still take commits, `git subtree pull --prefix=holos
  https://github.com/pwin/triplestore main` (and the same for the others) brings them in.
  Moving every file would make each pull a conflict.
- Renaming crates to `semops-*` is phase 3 work. It happens once, in this repository, when
  the old repositories are frozen.

## What changed to make it one workspace

| Change | Why |
| --- | --- |
| `holos/Cargo.toml`, `shacl/Cargo.toml`, `tarka/Cargo.toml` and their lockfiles removed; root `Cargo.toml` added | A crate belongs to the nearest `[workspace]` above it, so the three sub-workspaces had to go |
| One version, `0.29.0`, for every crate | The first number above every component this replaces (holosdb 0.27.0, shacl 0.3.9, tarka-rdf 0.3.1, ontology-quality-suite 0.28.0), so each old package name can continue as a shim without its version going backwards |
| HOLOS crates say `edition = "2021"` explicitly | The workspace default is 2024 (SHACL_Engine's and tarka's). HOLOS's code is 2021 and stays 2021 |
| `rust-version = "1.90"` | The highest of the three (HOLOS asked for 1.87) |
| Resolver 3 | SHACL_Engine's and tarka's. HOLOS used 2. Version 3 adds MSRV-aware resolution and changes nothing else here |
| tarka's git dependencies on `holos-engine` and `shacl` are now path dependencies | They are in the same tree. tarka's `[patch]` re-applying HOLOS's Oxigraph fixes by git tag is gone too, because the root `[patch]` applies to every crate |
| `shacl` keeps its own Oxigraph dependency specifications, without `rdf-12` | An inherited dependency can gain features but not lose them, so inheriting HOLOS's would have made `-p shacl --no-default-features` stop being an RDF 1.1 build, which SHACL_Engine's CI tests |
| One `vendor/spareval` and one `vendor/oxsdatatypes`, both workspace members | HOLOS and SHACL_Engine each vendored the same patched `src/`. SHACL_Engine's `oxsdatatypes` manifest is used, because HOLOS's would break `--all-features` at the linker. See each `PROVENANCE.md` |
| One `.cargo/config.toml` at the root | Cargo reads config from the working directory upward, so the copies in `holos/` and `shacl/` would not apply. They were identical: the wasm32 `getrandom` cfg |
| `[profile.release]` unwinds; new `[profile.release-abort]` | The Python modules need unwinding, and `panic` cannot be set per package. HOLOS's command and server shipped with `panic = "abort"` and keep it through `release-abort`. Release LTO is `thin`, as HOLOS had it; SHACL_Engine and tarka used `fat`, at a large cost in build time for a small gain |
| `default-members` leaves out the three Python modules | tarka-python linked `python3.dll` into the `tarka` command when built in the same run, and `extension-module` leaves test binaries unlinkable. `cargo build` and `cargo test` cover everything else, and `--workspace` brings them back |
| `holos/deploy/Dockerfile` builds from the repository root | It copied `testsuites/` (fetched, absent in a fresh clone, so the build failed) and never copied `vendor/` (needed by `[patch]`). It now copies the workspace, builds `--profile release-abort`, and uses the 1.90 image. `docker-compose.yml` and `OPERATIONS.md` follow. Not yet built: there is no Docker on the machine this was written on |
| `shacl/examples/check.sh` looks for the binary at `../../target` | The target directory is now the workspace root's |

### The lockfile

The root `Cargo.lock` started from HOLOS's and was completed by cargo. All 186 crates HOLOS
locked are at the versions HOLOS tested. SHACL_Engine's and tarka's own dependencies moved
by patch releases within the same minor versions (29 crates for SHACL_Engine, 67 for tarka,
which had locked HOLOS's dependency tree independently).

## Test suites, and the phase 2 gate

- `scripts/fetch-testsuites.sh` fetches the W3C RDF and SPARQL suites (`w3c/rdf-tests`) and
  the W3C SHACL suites (`w3c/data-shapes`), each **pinned** to the commit HOLOS's baselines
  were recorded against: rdf-tests `8af71fe` (21 August 2026) and data-shapes `7a293f5`
  (24 August 2026), read from the maintainer's own HOLOS checkout. No script fetched
  data-shapes before, and HOLOS's own script fetched rdf-tests at HEAD.
- Why pinned: at HEAD on 21 November 2026, HOLOS 0.27.0 failed 33 tests across 8 suites
  that it had never been run against. The RDF 1.2 suite had been reorganised (1405 tests to
  380) and the SHACL 1.2 suite had gained and changed tests. Upstream HOLOS, built in its
  own repository, failed exactly the same 33, so this was suite drift, not the merge.
  Moving a pin is now a deliberate change with a reviewed re-baseline.
- Two of those 33 were not drift at all. RDF 1.1's RDF/XML manifest has no base, so its
  tests are named after the file they were loaded from, and the baseline held
  `file:///C:/repos/new_triplestore_sparql_engine/testsuites/...`. That only matches in that
  checkout. The ratchet in `holos-conformance/tests/w3c.rs` now compares everything up to
  `testsuites/` as the same, so baselines are portable and the existing ones still match.
- Result, on Windows, 21 November 2026: every test in the workspace passes, including all 12
  W3C conformance ratchets, the SPARQL and Graph Store protocol tests, HOLOS's 866 tests,
  SHACL_Engine's (with its own bundled W3C suites) and tarka's.
- SHACL_Engine carries its own copy of the SHACL suites in `shacl/testsuite/`. Its SHACL 1.0
  copy is identical to the fetched one. Its SHACL 1.2 copy differs in 336 files, because the
  1.2 suite is still changing upstream. Pointing HOLOS at the bundled copy, so there is one
  SHACL suite and no fetch, is a follow-up, done once the two 1.2 copies are reconciled.

## Not yet done in this phase

- The old names re-exporting new ones. (The registry became a crate, `semops-checks`, on
  22 November 2026: see [0005](0005-check-registry.md).)
- One SHACL validator. Done on 25 November 2026 ([0011](0011-one-shacl-engine.md)): HOLOS
  validates with `shacl`, and its copy, `holos-shacl-engine`, is gone.
- rustfmt and clippy over `holos/`. HOLOS was never gated on either: it has 74 rustfmt
  differences and about 270 clippy warnings, inherited from upstream. Both wait until
  pwin/triplestore stops taking commits, so subtree pulls stay clean. Until then CI gates
  `shacl/`, `tarka/` and `vendor/` (no rustfmt difference, no clippy warning) and reports
  HOLOS's count. The gate is by directory because clippy lints every workspace member a
  build compiles, so `-p shacl -- -D warnings` failed on `vendor/spareval` as a dependency.
  That crate now allows the two default lints Oxigraph's design trips.
- Release workflows. Each engine's old `release.yml` came in with its subtree but does not
  run (GitHub reads only the root `.github/`). They are the reference for phase 3's single
  release.
