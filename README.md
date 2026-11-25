# SemOps System

One workspace for the pwin RDF toolchain: the HOLOS triplestore and SPARQL engine, the SHACL
engine and the tarka mapper, built, versioned and released together. It implements
*Proposal: one SemOps toolchain from the pwin RDF repositories* (21 November 2026).

| Directory | What it is | Came from |
| --- | --- | --- |
| [`holos/`](holos/) | RDF 1.2 store, SPARQL 1.2 engine and server, access policy, GeoSPARQL, holons, two SHACL validators | [pwin/triplestore](https://github.com/pwin/triplestore) 0.27.0 |
| [`shacl/`](shacl/) | SHACL 1.0/1.2 and SHACL-AF validator | [pwin/SHACL_Engine](https://github.com/pwin/SHACL_Engine) 0.3.9 |
| [`tarka/`](tarka/) | CSV, Parquet and Polars to RDF 1.1 through TARQL or OTTR | [pwin/tarka](https://github.com/pwin/tarka) 0.3.1 |
| [`crates/semops/`](crates/semops/) | The `semops` command. So far `semops check`, the ontology quality checks run natively | — |
| [`crates/semops-checks/`](crates/semops-checks/) | The ontology quality check registry (61 checks, their SHACL shapes, SPARQL queries and repair templates) compiled in, and a runner for it on HOLOS | [pwin/consolidated-ontology-quality-suite-python](https://github.com/pwin/consolidated-ontology-quality-suite-python) 0.28.0's `resources/`, and its merge rules ported |
| [`crates/semops-functions/`](crates/semops-functions/) | The SPARQL extension functions every evaluator registers: XPath `fn:` and `math:`, Jena `afn:` and property functions, SPIN `spif:` | HOLOS's and tarka's function code, merged and extended |
| [`crates/semops-wasm/`](crates/semops-wasm/) | The `semops-wasm` and `semops-wasm-node` npm packages: the store, the SHACL validator and the quality checks in one WebAssembly module | holos-wasm and shacl-wasm, linked together |
| [`crates/semops-placetime/`](crates/semops-placetime/) | URIs for calendar intervals, instants and WGS84 points, each described in RDF from the URI alone, in the reference.data.gov.uk patterns; in reference.data.gov.uk's vocabulary or in gist 14.1.0, through the placetime ontology; served by `holos-server --placetime` | A Rust rewrite of [epimorphics/IntervalServer](https://github.com/epimorphics/IntervalServer), with points added |
| [`deploy/azure/`](deploy/azure/) | Helm chart for AKS, Bicep for Container Apps, the API Management identity policy, and the guide to Azure hosting and NFS storage | — |
| [`vendor/`](vendor/) | The two patched Oxigraph crates every engine builds against | One copy of what HOLOS and SHACL_Engine each vendored |
| [`docs/manual/`](docs/manual/) | The manual: installing, a first run, the server, checking, validating, mapping, storage, security, Azure, and a reference. `python docs/manual/build.py` builds it as a website, an EPUB and an A4 PDF | — |
| [`docs/decisions/`](docs/decisions/) | Why the workspace is laid out as it is, what it is called, and what it must be able to do | — |
| [`CHANGELOG.md`](CHANGELOG.md) | What each SemOps System release changed | — |
| [`versions.toml`](versions.toml) | Which engine release the editor, extension and suite must use: the workspace's own, from 0.31.0 | — |
| [`python/semops-system/`](python/semops-system/) | The `semops-system` package on PyPI: one native wheel, the three engines and the quality checks in one extension module built from `crates/semops-python`, with tests of everything the product must do | — |

Each engine directory was imported with `git subtree`, with its history, and keeps its own
README, changelog and documentation. The engines' original repositories, pwin/triplestore,
pwin/SHACL_Engine and pwin/tarka, are archived: work on them happens here.

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

`holos-server --placetime` also answers the interval URIs reference.data.gov.uk used to
serve, under `https://<host>/` or a configured base: `/id/quarter/2006-Q1` redirects to a
description of that quarter, and likewise for years to seconds in the British (Julian until
1752), Gregorian and Julian calendars, for instants and intervals of any length, and for
WGS84 points (`/id/geopoint/wgs84/X-0.1276Y51.5072`), which GeoSPARQL queries can use once
loaded. [docs/decisions/0004](docs/decisions/0004-placetime.md) has the patterns, what
changed from IntervalServer and the licence position. With `--placetime-vocabulary gist` the
same URIs are described in gist 14.1.0, through a placetime ontology built on it;
[docs/decisions/0007](docs/decisions/0007-placetime-in-gist.md) has the model.

`semops check ontology.ttl data.ttl` runs the ontology quality suite's 61-check registry
natively, compiled into the command, with the same findings as
`ontology-suite check` (any `--engine`) and no Python, and exits 1 on a violation for a CI
gate. [docs/decisions/0005](docs/decisions/0005-check-registry.md) says how it was held to
the suite's output.

On Azure the same image runs on AKS (a writer on a managed disk or Azure NetApp Files, read
replicas sharing checkpoints on an NFS volume) or on Container Apps over an Azure Files NFS
share, with Entra ID at the edge through built-in authentication or API Management.
[deploy/azure/README.md](deploy/azure/README.md) helps choose, and
[docs/decisions/0006](docs/decisions/0006-azure-nfs-and-hardening.md) records the storage
rules and the security fixes made along the way.

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

The product is **SemOps System**. It ships as the `semops` command, the `semops-system`
wheel on PyPI (`import semops_system`), the `semops-wasm` npm packages and the
`ghcr.io/pwin/semops` image. Each engine is also published from here on its own, under its
existing name, for anyone who wants one: `holosdb`, `shacl` and `tarka-rdf` on PyPI,
`holos-wasm` and `shacl-wasm` on npm, and its commands in each release.
[docs/decisions/0002](docs/decisions/0002-names-and-registries.md) explains the choices.

## Status

| Phase | State |
| --- | --- |
| 1. Align | `versions.toml` and `scripts/check_pins.py` are in place. The editor, extension and suite are still one engine release behind |
| 2. One workspace | The three engines are in one workspace with one vendor copy and one CI, and the image builds and passes its smoke test. The check registry is a crate, `semops-checks` |
| 3. One artifact: `semops` command, `semops-system` wheel, one WASM package, OCI image | The `semops` command is one binary: `serve`, `load`, `query`, `update`, `validate`, `map`, `check` and the store commands, with the old command names as aliases of it ([docs/decisions/0008](docs/decisions/0008-one-binary.md)); the image ships it alone. A `v*` tag runs `release.yml`, which builds the binary for five platforms and the multi-arch image (`ghcr.io/pwin/semops`); [0.30.0](https://github.com/pwin/SemopsSystem/releases/tag/v0.30.0) was the first release and [0.31.0](https://github.com/pwin/SemopsSystem/releases/tag/v0.31.0) is the latest. `semops-system` is one native wheel built from the workspace, the three engines and the quality checks in one extension module, and it installs the `semops` command ([docs/decisions/0010](docs/decisions/0010-native-wheel.md)); the one WASM package is `semops-wasm`, the store, the validator and the quality checks in one module for bundlers and for Node, on npm ([docs/decisions/0009](docs/decisions/0009-one-wasm-package.md)); `semops diff` and `semops repair` port the suite's version comparison and repairs |
| 4. Azure, one SHACL validator, one function library | The function library is done, and the native validator checks SHACL-SPARQL through the `shacl` engine. Azure has deployment templates (AKS, Container Apps, API Management) checked in CI but not yet deployed; `azure.yaml` makes the repository an `azd` template for the Container Apps option, and a GitHub Action (`actions/check`) and an Azure Pipelines template run `semops check` as a CI gate. One SHACL engine: HOLOS validates with `shacl`, the crate published on its own, and its adapted copy is gone ([docs/decisions/0011](docs/decisions/0011-one-shacl-engine.md)); the native evaluator stays for Core and incremental revalidation over the store. Every `semops` subcommand reads `abfss://` and Blob Storage URLs as inputs ([docs/decisions/0012](docs/decisions/0012-blob-urls-as-inputs.md)). `semops backup --to`, `semops upload` and the chart's backup job copy checkpoints to Blob Storage ([0013](docs/decisions/0013-checkpoints-to-blob-storage.md)). The server sends OpenTelemetry traces and metrics over OTLP/HTTP, to a collector that forwards to Application Insights ([0014](docs/decisions/0014-telemetry.md)) |

## Licence

MIT OR Apache-2.0, as each engine was, except `crates/semops-placetime`, which is Apache-2.0
because it is a rewrite of Apache-2.0 code; its NOTICE file says what it derives from.
