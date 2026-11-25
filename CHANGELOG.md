# Changes

SemOps System's own releases. The engines keep their own changelogs, in `holos/`, `shacl/`
and `tarka/`, for their history before the workspace.

## Unreleased

- Each engine is published from the workspace on its own, at the workspace version:
  `holosdb`, `shacl` and `tarka-rdf` on PyPI, and `holos-wasm`, `shacl-wasm` and their
  `-node` builds on npm, by `engines.yml`. The engines' old repositories stop releasing
  (#23).
- Each release carries the engines' commands on their own, beside `semops` (#23).
- `semops-system` no longer requires Polars. `pip install "semops-system[polars]"` adds it,
  for mapping data frames (#23).
- A SPARQL constraint's result carries `sh:value` when `?value` is a term the query computed
  (`BIND`, an aggregate, `STR()`) and neither graph holds, as SHACL 5.3.2 requires; it used to
  be left out while the message still showed it. `?path` counts only when it is an IRI. In
  `shacl` and in HOLOS (#24, reported as SHACL_Engine#3).
- One SHACL engine. HOLOS validates SHACL-SPARQL, node expressions and rules with `shacl`,
  the crate published on its own, and its adapted copy, `holos-shacl-engine`, is gone.
  `shacl` gains the incremental-revalidation API HOLOS needs: `Graph::apply` and the shapes'
  dependency index. W3C results are unchanged
  ([ADR 0011](docs/decisions/0011-one-shacl-engine.md)).

## 0.30.0, 23 November 2026

- `semops diff OLD NEW` compares two versions of an ontology and suggests a semantic-version
  bump, as the Ontology Quality Suite's `version-diff` does (#14).
- `semops repair FILE` applies the quality checks' repairs to a file, as the suite and the
  VS Code extension do (#15).
- `pip install semops-system` installs the `semops` command, with every subcommand (#17).
- The repository is an `azd` template for the Container Apps option (#16), and a GitHub
  Action and an Azure Pipelines template run `semops check` as a CI gate (#19).
- The first published artifacts: the `semops` binaries and the image `ghcr.io/pwin/semops`
  (#13), `semops-system` on PyPI, and `semops-wasm` and `semops-wasm-node` on npm (#20).

## 0.29.0, 23 November 2026

The first release under the workspace's version. On PyPI, `semops-system` 0.29.0 is the
first native wheel, replacing the 0.29.0.dev1 bridge to the engines' separate wheels.

- One Cargo workspace for HOLOS, SHACL_Engine and tarka, with one version and one CI.
- `semops check`: the 61 ontology quality checks, run natively (#4), with JUnit and SARIF
  reports for CI (#12).
- `semops`, one binary: `serve`, `load`, `query`, `update`, `validate`, `map`, `check` and
  the store commands, with the old command names as aliases of it (#9).
- `semops-wasm` and `semops-wasm-node`: the store, the validator and the quality checks in
  one WebAssembly module (#10).
- `semops-system`, one native Python wheel carrying the three engines and the checks (#11).
- Placetime: the reference.data.gov.uk interval URIs, WGS84 points, and a gist 14.1.0
  vocabulary for them (#3, #7, #8).
- Azure: a Helm chart for AKS, Bicep for Container Apps with NFS storage, an API
  Management identity policy, and NFS-safe storage in the server (#5).
- A release workflow for the binaries and the multi-arch container image (#13), and the
  manual as a website, an EPUB and a PDF (#6).
