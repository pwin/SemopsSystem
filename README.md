# SemOps System

One workspace for the pwin RDF toolchain: the HOLOS triplestore and SPARQL engine, the SHACL
engine and the tarka mapper, built, versioned and released together. It implements
*Proposal: one SemOps toolchain from the pwin RDF repositories* (21 November 2026).

| Directory | What it is | Came from |
| --- | --- | --- |
| [`holos/`](holos/) | RDF 1.2 store, SPARQL 1.2 engine and server, access policy, GeoSPARQL, holons, two SHACL validators | [pwin/triplestore](https://github.com/pwin/triplestore) 0.27.0 |
| [`shacl/`](shacl/) | SHACL 1.0/1.2 and SHACL-AF validator | [pwin/SHACL_Engine](https://github.com/pwin/SHACL_Engine) 0.3.9 |
| [`tarka/`](tarka/) | CSV, Parquet and Polars to RDF 1.1 through TARQL or OTTR | [pwin/tarka](https://github.com/pwin/tarka) 0.3.1 |
| [`crates/semops-functions/`](crates/semops-functions/) | The SPARQL extension functions every evaluator registers: XPath `fn:` and `math:`, Jena `afn:` and property functions, SPIN `spif:` | HOLOS's and tarka's function code, merged and extended |
| [`vendor/`](vendor/) | The two patched Oxigraph crates every engine builds against | One copy of what HOLOS and SHACL_Engine each vendored |
| [`docs/decisions/`](docs/decisions/) | Why the workspace is laid out as it is, what it is called, and what it must be able to do | — |
| [`versions.toml`](versions.toml) | Which engine releases the editor, extension and suite must use until the workspace ships its own packages | — |
| [`python/semops-system/`](python/semops-system/) | The `semops-system` package on PyPI: the three engines as one install, with tests of everything the product must do | — |

Each engine directory was imported with `git subtree`, with its history, and keeps its own
README, changelog and documentation.

## What it does

Everything HOLOS does, on every surface: RDF 1.2, SPARQL 1.2 query and update, SHACL 1.0
and 1.2 (Core, SHACL-SPARQL, node expressions and rules), GeoSPARQL, and a persistent store.
tarka maps tabular data to RDF 1.1; SPARQL 1.2 run by the engine builds RDF 1.2 from its
output. The extension functions (XPath `fn:` and `math:`, Jena `afn:`, SPIN `spif:`, and
the property functions `apf:strSplit`, `list:member`, `rdfs:member` and the rest) are one
library, `semops-functions`, that HOLOS queries and updates, both SHACL validators and tarka
all register, tested against one table of cases in each. HOLOS's native validator checks
SHACL-SPARQL constraints too, through the adapted engine.
[docs/decisions/0003](docs/decisions/0003-capability-floor-and-functions.md) has the
requirement, what is supported and what is deliberately not.

## Build and test

Rust 1.90 or later, a C++ toolchain and libclang (for RocksDB).

```sh
cargo build                      # every crate except the three Python modules
sh scripts/fetch-testsuites.sh   # the W3C RDF, SPARQL and SHACL suites (about 70 MB)
cargo test                       # the engines' tests and the W3C conformance ratchet
```

Without the fetched suites the conformance tests skip rather than fail. The Python modules
build with maturin, one at a time, for example
`maturin build -m holos/crates/holos-python/Cargo.toml`.

## Names

The product is **SemOps System**. It will ship as the `semops` command, the `semops-system`
wheel on PyPI (`import semops_system`), the `semops-wasm` npm packages and the
`ghcr.io/pwin/semops` image. Until then the engines ship under their existing names.
[docs/decisions/0002](docs/decisions/0002-names-and-registries.md) explains the choices.

## Status

| Phase | State |
| --- | --- |
| 1. Align | `versions.toml` and `scripts/check_pins.py` are in place. The editor, extension and suite are still one engine release behind |
| 2. One workspace | The three engines are in one workspace with one vendor copy and one CI, and the image builds and passes its smoke test. The check registry crate is still to do |
| 3. One artifact: `semops` command, `semops-system` wheel, one WASM package, OCI image | `semops-system` 0.29.0.dev1 is on PyPI as a bridge that pins the three engine wheels; the native wheel, command, WASM package and image are not started |
| 4. Azure, one SHACL validator, one function library | The function library is done, and the native validator checks SHACL-SPARQL through the adapted engine. One validator and Azure are not started |

## Licence

MIT OR Apache-2.0, as each engine was.
